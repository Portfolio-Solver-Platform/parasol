use crate::args::{Args, DebugVerbosityLevel};
use crate::model_parser::{
    ModelParseError, ObjectiveType, ObjectiveValue, get_objective_type, insert_objective,
};
use crate::scheduler::ScheduleElement;
use crate::solver_output::{Output, Solution, Status};
use crate::{mzn_to_fzn, solver_output};
use futures::future::join_all;
use std::collections::{HashMap, HashSet};
use std::io::ErrorKind;
use std::path::Path;
use std::process::Stdio;
use std::sync::Arc;
use sysinfo::{Pid, System};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::{Mutex, RwLock, mpsc};
use tokio::task::JoinHandle;

const SUSPEND_SIGNAL: &str = "SIGSTOP";
const RESUME_SIGNAL: &str = "SIGCONT";
const KILL_SIGNAL: &str = "SIGTERM";

#[derive(Debug)]
pub enum Error {
    KillTree(kill_tree::Error),
    InvalidSolver(String),
    Io(std::io::Error),
    OutputParseError(solver_output::Error),
    ModelParse(ModelParseError),
    FznConversion(mzn_to_fzn::ConversionError),
    UseOfOznBeforeCompilation,
}
pub type Result<T> = std::result::Result<T, Error>;

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::Io(e)
    }
}

impl From<mzn_to_fzn::ConversionError> for Error {
    fn from(value: mzn_to_fzn::ConversionError) -> Self {
        Self::FznConversion(value)
    }
}

impl From<kill_tree::Error> for Error {
    fn from(value: kill_tree::Error) -> Self {
        Error::KillTree(value)
    }
}

impl From<solver_output::Error> for Error {
    fn from(value: solver_output::Error) -> Self {
        Error::OutputParseError(value)
    }
}

impl From<ModelParseError> for Error {
    fn from(value: ModelParseError) -> Self {
        Error::ModelParse(value)
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::KillTree(e) => write!(f, "failed to kill process tree: {}", e),
            Error::InvalidSolver(msg) => write!(f, "invalid solver: {}", msg),
            Error::Io(e) => write!(f, "IO error: {}", e),
            Error::OutputParseError(e) => write!(f, "output parse error: {:?}", e),
            Error::ModelParse(e) => write!(f, "model parse error: {:?}", e),
            Error::FznConversion(e) => {
                write!(f, "failed to convert mzn to fzn: {e:?}")
            }
            Error::UseOfOznBeforeCompilation => {
                write!(f, "ozn file was used before it was compiled")
            }
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::Io(e) => Some(e),
            _ => None,
        }
    }
}

#[derive(Debug)]
enum Msg {
    Solution(Solution),
    Status(Status),
}

pub struct SolverManager {
    tx: mpsc::UnboundedSender<Msg>,
    solver_to_pid: Arc<Mutex<HashMap<usize, u32>>>,
    args: Args,
    mzn_to_fzn: mzn_to_fzn::CachedConverter,
    best_objective: Arc<RwLock<Option<ObjectiveValue>>>,
    objective_type: ObjectiveType,
}

struct PipeCommand {
    pub left: Child,
    pub right: Child,
    pub pipe: JoinHandle<std::io::Result<u64>>,
}

impl SolverManager {
    pub async fn new(args: Args) -> std::result::Result<Self, Error> {
        let objective_type = get_objective_type(&args.model).await?;
        let (tx, rx) = mpsc::unbounded_channel::<Msg>();
        let solver_to_pid = Arc::new(Mutex::new(HashMap::new()));

        cleanup_handler(solver_to_pid.clone());
        let solver_to_pid_clone = solver_to_pid.clone();
        let best_objective = Arc::new(RwLock::new(None));

        let shared_objective = best_objective.clone();
        tokio::spawn(async move {
            Self::receiver(rx, solver_to_pid_clone, objective_type, shared_objective).await
        });

        Ok(Self {
            tx,
            solver_to_pid,
            mzn_to_fzn: mzn_to_fzn::CachedConverter::new(args.debug_verbosity),
            args,
            best_objective,
            objective_type,
        })
    }

    async fn receiver(
        mut rx: mpsc::UnboundedReceiver<Msg>,
        solver_to_pid: Arc<Mutex<HashMap<usize, u32>>>,
        objective_type: ObjectiveType,
        shared_objective: Arc<RwLock<Option<ObjectiveValue>>>,
    ) {
        let mut objective: Option<ObjectiveValue> = None;

        while let Some(output) = rx.recv().await {
            match output {
                Msg::Solution(s) => {
                    if objective_type.is_better(objective, s.objective) {
                        objective = Some(s.objective);
                        {
                            let mut guard = shared_objective.write().await;
                            *guard = Some(s.objective);
                        }
                        println!("{}", s.solution.trim_end());
                    }
                }
                Msg::Status(status) => {
                    if status != Status::Unknown {
                        println!("{}", status.to_dzn_string());
                        break;
                    }
                }
            }
        }

        Self::_stop_all_solvers(solver_to_pid.clone())
            .await
            .expect("could not kill all solvers");
        std::process::exit(0);
    }

    async fn get_fzn_command(
        &self,
        fzn_path: &Path,
        solver_name: &str,
        cores: usize,
    ) -> Result<Command> {
        let mut cmd = Command::new("minizinc");
        cmd.arg("--solver").arg(solver_name);
        cmd.arg(fzn_path);

        cmd.arg("-i");

        // if self.args.output_objective { // TODO make this an option to the output we print since it is in the rules i think
        //     cmd.arg("--output-objective");
        // }

        // if self.args.ignore_search {  // TODO maybe also this? This option however gives some errors for some solvers
        //     cmd.arg("-f");
        // }
        // cmd.arg("-f");

        cmd.arg("-p").arg(cores.to_string());

        Ok(cmd)
    }

    async fn get_ozn_command(&self) -> Result<Command> {
        let mut cmd = Command::new("minizinc");
        cmd.arg("--ozn-file");

        let mut error = None;
        self.mzn_to_fzn
            .use_ozn_file(|ozn| match ozn {
                Some(ozn) => {
                    cmd.arg(ozn);
                }
                None => {
                    error = Some(Error::UseOfOznBeforeCompilation);
                }
            })
            .await;

        match error {
            None => Ok(cmd),
            Some(error) => Err(error),
        }
    }

    async fn start_solver(
        &self,
        elem: &ScheduleElement,
        objective: Option<ObjectiveValue>,
    ) -> Result<()> {
        let solver_name = &elem.info.name;
        let cores = elem.info.cores;

        let fzn_original_path = self
            .mzn_to_fzn
            .convert(&self.args.model, self.args.data.as_deref(), solver_name)
            .await?;

        let (fzn_final_path, fzn_guard) = if let Some(obj) = objective {
            if let Ok(new_temp_file) =
                insert_objective(&fzn_original_path, &self.objective_type, obj)
            {
                (new_temp_file.path().to_path_buf(), Some(new_temp_file))
            } else {
                (fzn_original_path, None)
            }
        } else {
            (fzn_original_path, None)
        };

        let mut fzn_cmd = self
            .get_fzn_command(&fzn_final_path, solver_name, cores)
            .await?;
        #[cfg(unix)]
        fzn_cmd.process_group(0); // let OS give it a group process id
        fzn_cmd.stderr(Stdio::piped());

        let mut ozn_cmd = self.get_ozn_command().await?;
        ozn_cmd.stdout(Stdio::piped());
        ozn_cmd.stderr(Stdio::piped());

        let PipeCommand {
            left: mut fzn,
            right: mut ozn,
            pipe,
        } = pipe(fzn_cmd, ozn_cmd).await?;

        let pid = fzn.id().expect("Child has no PID");
        {
            let mut map = self.solver_to_pid.lock().await;
            map.insert(elem.id, pid);
        }

        let ozn_stdout = ozn.stdout.take().expect("Failed to take ozn stdout");
        let ozn_stderr = ozn.stderr.take().expect("Failed to take ozn stderr");
        let fzn_stderr = fzn.stderr.take().expect("Failed to take fzt stderr");

        let tx_clone = self.tx.clone();
        let verbosity = self.args.debug_verbosity;
        tokio::spawn(async move {
            Self::handle_solver_stdout(ozn_stdout, pipe, tx_clone, verbosity).await;
        });

        let verbosity_stderr = self.args.debug_verbosity;
        tokio::spawn(async move { Self::handle_solver_stderr(fzn_stderr, verbosity_stderr).await });
        tokio::spawn(async move { Self::handle_solver_stderr(ozn_stderr, verbosity_stderr).await });

        let solver_to_pid_clone = self.solver_to_pid.clone();
        let solver_name = elem.info.name.clone();
        let solver_id = elem.id;
        let verbosity_wait = self.args.debug_verbosity;

        tokio::spawn(async move {
            let _keep_alive = fzn_guard;
            match fzn.wait().await {
                Ok(status) if !status.success() => {
                    if verbosity_wait >= DebugVerbosityLevel::Info {
                        eprintln!("Solver '{}' exited with status: {}", solver_name, status);
                    }
                }
                Err(e) => {
                    if verbosity_wait >= DebugVerbosityLevel::Error {
                        eprintln!("Error waiting for solver '{}': {}", solver_name, e);
                    }
                }
                _ => {}
            }
            let mut map = solver_to_pid_clone.lock().await;
            map.remove(&solver_id);
        });

        Ok(())
    }

    async fn handle_solver_stdout(
        stdout: tokio::process::ChildStdout,
        pipe: JoinHandle<std::io::Result<u64>>,
        tx: tokio::sync::mpsc::UnboundedSender<Msg>,
        verbosity: DebugVerbosityLevel,
    ) {
        let reader = BufReader::new(stdout);
        let mut lines = reader.lines();
        let mut parser = solver_output::Parser::new();

        while let Ok(Some(line)) = lines.next_line().await.map_err(|err| {
            if verbosity >= DebugVerbosityLevel::Error {
                eprintln!("Error reading solver stdout: {err}");
            }
        }) {
            let output = match parser.next_line(&line) {
                Ok(o) => o,
                Err(e) => {
                    if verbosity >= DebugVerbosityLevel::Error {
                        eprintln!("Error parsing solver output: {e}");
                    }
                    continue;
                }
            };
            let Some(output) = output else {
                continue;
            };

            let msg = match output {
                Output::Solution(solution) => Msg::Solution(solution),
                Output::Status(status) => Msg::Status(status),
            };

            if let Err(e) = tx.send(msg) {
                if verbosity >= DebugVerbosityLevel::Error {
                    eprintln!("Could not send message, receiver dropped: {}", e);
                }
                break;
            }
        }

        match pipe.await {
            Ok(_) => {}
            Err(e) => {
                if verbosity >= DebugVerbosityLevel::Error {
                    eprintln!("Error piping from fzn to ozn: {e}");
                }
            }
        }
    }

    async fn handle_solver_stderr(
        stderr: tokio::process::ChildStderr,
        verbosity: DebugVerbosityLevel,
    ) {
        let reader = BufReader::new(stderr);
        let mut lines = reader.lines();

        while let Some(line) = lines.next_line().await.unwrap_or_else(|e| {
            if verbosity >= DebugVerbosityLevel::Error {
                eprintln!("Error reading solver stderr: {}", e);
            }
            None
        }) {
            if verbosity >= DebugVerbosityLevel::Error {
                eprintln!("Solver stderr: {}", line);
            }
        }
    }

    pub async fn start_solvers(
        &self,
        schedule: &[ScheduleElement],
        objective: Option<ObjectiveValue>,
    ) -> std::result::Result<(), Vec<Error>> {
        let futures = schedule
            .iter()
            .map(|elem| self.start_solver(elem, objective));
        let results = join_all(futures).await;
        let errors: Vec<Error> = results.into_iter().filter_map(Result::err).collect();

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    // could probably be optimized to be able to send multiple signals to a process at a time, instead of traversing it twice
    async fn send_signal_to_solver(
        solver_to_pid: Arc<Mutex<HashMap<usize, u32>>>,
        id: usize,
        signal: String,
    ) -> std::result::Result<(), Error> {
        let pid = {
            let map = solver_to_pid.lock().await;
            match map.get(&id) {
                Some(&p) => p,
                None => return Err(Error::InvalidSolver(format!("Solver {id} not running"))),
            }
        };

        let config = kill_tree::Config {
            signal,
            ..Default::default()
        };
        if let Err(e) = kill_tree::tokio::kill_tree_with_config(pid, &config).await {
            let is_zombie = match &e {
                kill_tree::Error::Io(io_err) => io_err.kind() == ErrorKind::NotFound,
                kill_tree::Error::InvalidProcessId { .. } => true,
                _ => false,
            };
            if !is_zombie {
                return Err(Error::KillTree(e));
            }
        }

        Ok(())
    }

    async fn send_signal_to_solvers(
        solver_to_pid: Arc<Mutex<HashMap<usize, u32>>>,
        ids: &[usize],
        signal: &str,
    ) -> std::result::Result<(), Vec<Error>> {
        let futures = ids
            .iter()
            .map(|id| Self::send_signal_to_solver(solver_to_pid.clone(), *id, signal.to_string()));
        let results = join_all(futures).await;
        let errors: Vec<Error> = results.into_iter().filter_map(|res| res.err()).collect();

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    async fn send_signal_to_all_solvers(
        solver_to_pid: Arc<Mutex<HashMap<usize, u32>>>,
        signal: &str,
    ) -> std::result::Result<(), Vec<Error>> {
        let ids: Vec<usize> = { solver_to_pid.lock().await.keys().cloned().collect() };
        Self::send_signal_to_solvers(solver_to_pid.clone(), &ids, signal).await
    }

    pub async fn suspend_solver(&self, id: usize) -> std::result::Result<(), Error> {
        Self::send_signal_to_solver(self.solver_to_pid.clone(), id, String::from(SUSPEND_SIGNAL))
            .await
    }

    pub async fn suspend_solvers(&self, ids: &[usize]) -> std::result::Result<(), Vec<Error>> {
        Self::send_signal_to_solvers(self.solver_to_pid.clone(), ids, SUSPEND_SIGNAL).await
    }

    pub async fn suspend_all_solvers(&self) -> std::result::Result<(), Vec<Error>> {
        Self::send_signal_to_all_solvers(self.solver_to_pid.clone(), SUSPEND_SIGNAL).await
    }

    pub async fn resume_solver(&self, id: usize) -> std::result::Result<(), Error> {
        Self::send_signal_to_solver(self.solver_to_pid.clone(), id, String::from(RESUME_SIGNAL))
            .await
    }

    pub async fn resume_solvers(&self, ids: &[usize]) -> std::result::Result<(), Vec<Error>> {
        Self::send_signal_to_solvers(self.solver_to_pid.clone(), ids, RESUME_SIGNAL).await
    }

    pub async fn resume_all_solvers(&self) -> std::result::Result<(), Vec<Error>> {
        Self::send_signal_to_all_solvers(self.solver_to_pid.clone(), RESUME_SIGNAL).await
    }

    async fn _stop_solver(
        solver_to_pid: Arc<Mutex<HashMap<usize, u32>>>,
        id: usize,
    ) -> std::result::Result<(), Error> {
        Self::send_signal_to_solver(solver_to_pid.clone(), id, String::from(KILL_SIGNAL)).await?;
        let _ = Self::send_signal_to_solver(solver_to_pid.clone(), id, String::from(RESUME_SIGNAL))
            .await; // we ignore since the process might already be dead
        Ok(())
    }

    async fn _stop_solvers(
        solver_to_pid: Arc<Mutex<HashMap<usize, u32>>>,
        ids: &[usize],
    ) -> std::result::Result<(), Vec<Error>> {
        Self::send_signal_to_solvers(solver_to_pid.clone(), ids, KILL_SIGNAL).await?;
        let _ = Self::send_signal_to_solvers(solver_to_pid.clone(), ids, RESUME_SIGNAL).await; // we ignore since the process might already be dead
        Ok(())
    }

    async fn _stop_all_solvers(
        solver_to_pid: Arc<Mutex<HashMap<usize, u32>>>,
    ) -> std::result::Result<(), Vec<Error>> {
        let ids: Vec<usize> = {
            let map = solver_to_pid.lock().await;
            map.keys().copied().collect()
        };

        Self::_stop_solvers(solver_to_pid.clone(), &ids).await
    }

    pub async fn stop_solver(&self, id: usize) -> std::result::Result<(), Error> {
        Self::_stop_solver(self.solver_to_pid.clone(), id).await
    }

    pub async fn stop_solvers(&self, ids: &[usize]) -> std::result::Result<(), Vec<Error>> {
        Self::_stop_solvers(self.solver_to_pid.clone(), ids).await
    }

    pub async fn stop_all_solvers(&self) -> std::result::Result<(), Vec<Error>> {
        Self::_stop_all_solvers(self.solver_to_pid.clone()).await
    }

    fn get_process_tree_memory(system: &System, root_pid: u32) -> u64 {
        let root_pid = Pid::from_u32(root_pid);
        let mut total_memory = 0u64;
        let mut pids_to_check = vec![root_pid];

        while let Some(pid) = pids_to_check.pop() {
            if let Some(process) = system.process(pid) {
                total_memory += process.memory();
                for (child_pid, child_process) in system.processes() {
                    if child_process.parent() == Some(pid) {
                        pids_to_check.push(*child_pid);
                    }
                }
            }
        }

        total_memory
    }

    pub async fn active_solver_ids(&self) -> HashSet<usize> {
        self.solver_to_pid.lock().await.keys().copied().collect()
    }

    pub async fn solvers_sorted_by_mem(&self, ids: &[usize], system: &System) -> Vec<(u64, usize)> {
        let solvers: Vec<(u32, usize)> = {
            let map = self.solver_to_pid.lock().await;
            let mut solvers: Vec<(u32, usize)> = Vec::new();
            for id in ids {
                match map.get(id) {
                    Some(pid) => solvers.push((*pid, *id)),
                    None => {
                        if self.args.debug_verbosity >= DebugVerbosityLevel::Warning {
                            eprintln!(
                                "solvers_sorted_by_mem failed to extract solver pid for id {}",
                                id
                            );
                        }
                    }
                }
            }
            solvers
        };

        let mut solver_mem = solvers
            .into_iter()
            .map(|(pid, id)| (Self::get_process_tree_memory(&system, pid), id))
            .collect::<Vec<(u64, usize)>>();
        solver_mem.sort_by_key(|(mem, _)| std::cmp::Reverse(*mem));
        solver_mem
    }

    pub async fn get_best_objective(&self) -> Option<ObjectiveValue> {
        *self.best_objective.read().await
    }
}

fn cleanup_handler(solver_to_pid: Arc<Mutex<HashMap<usize, u32>>>) {
    let solver_to_pid_clone = solver_to_pid.clone();

    ctrlc::set_handler(move || {
        let solver_to_pid_guard = solver_to_pid_clone.blocking_lock();

        for pid in solver_to_pid_guard.values() {
            let _ = kill_tree::blocking::kill_tree(*pid);

            // Resume the stopped processes can receive kill signal
            let resume_config = kill_tree::Config {
                signal: String::from(RESUME_SIGNAL),
                ..Default::default()
            };
            let _ = kill_tree::blocking::kill_tree_with_config(*pid, &resume_config);
        }

        std::process::exit(0);
    })
    .expect("Error setting Ctrl-C handler");
}

async fn pipe(mut left: Command, mut right: Command) -> Result<PipeCommand> {
    let mut left_child = left.stdout(Stdio::piped()).spawn()?;
    let mut right_child = right.stdin(Stdio::piped()).spawn()?;

    let mut left_stdout = left_child.stdout.take().expect("left stdout not captured");
    let mut right_stdin = right_child.stdin.take().expect("right stdin not captured");

    let pipe_task =
        tokio::spawn(async move { tokio::io::copy(&mut left_stdout, &mut right_stdin).await });

    Ok(PipeCommand {
        left: left_child,
        right: right_child,
        pipe: pipe_task,
    })
}
