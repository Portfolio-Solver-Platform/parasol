use crate::input::{Args, DebugVerbosityLevel};
use crate::model_parser::{ModelParseError, ObjectiveType, parse_objective_type};
use crate::solver_output::{Output, OutputParseError, Solution, Status};
use futures::future::join_all;
use std::collections::HashMap;
use std::io::ErrorKind;
use std::process::Stdio;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::Mutex;
use tokio::sync::mpsc;

const SUSPEND_SIGNAL: &str = "SIGSTOP";
const RESUME_SIGNAL: &str = "SIGCONT";
const KILL_SIGNAL: &str = "SIGTERM";
pub struct ScheduleElement {
    pub id: usize,
    pub solver: String,
    pub cores: usize,
}

impl ScheduleElement {
    pub fn new(id: usize, solver: String, cores: usize) -> Self {
        Self { id, solver, cores }
    }
}

pub type Schedule = Vec<ScheduleElement>;

#[derive(Debug)]
pub enum Error {
    KillTree(kill_tree::Error),
    InvalidSolver(String),
    Io(std::io::Error),
    OutputParseError(OutputParseError),
    ModelParse(ModelParseError),
}
pub type Result<T> = std::result::Result<T, Error>;

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::Io(e)
    }
}

impl From<kill_tree::Error> for Error {
    fn from(value: kill_tree::Error) -> Self {
        Error::KillTree(value)
    }
}

impl From<OutputParseError> for Error {
    fn from(value: OutputParseError) -> Self {
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

pub struct Scheduler {
    tx: mpsc::UnboundedSender<Msg>,
    solver_to_pid: Arc<Mutex<HashMap<usize, u32>>>,
    args: Args,
}

impl Scheduler {
    pub fn new(args: Args) -> std::result::Result<Self, Error> {
        let objective_type = parse_objective_type(&args.model)?;
        let (tx, rx) = mpsc::unbounded_channel::<Msg>();
        let solver_to_pid = Arc::new(Mutex::new(HashMap::new()));

        cleanup_handler(solver_to_pid.clone());
        let solver_to_pid_clone = solver_to_pid.clone();

        tokio::spawn(async move { Self::receiver(rx, solver_to_pid_clone, objective_type).await });

        Ok(Self {
            tx,
            solver_to_pid,
            args,
        })
    }

    async fn receiver(
        mut rx: mpsc::UnboundedReceiver<Msg>,
        solver_to_pid: Arc<Mutex<HashMap<usize, u32>>>,
        objective_type: ObjectiveType,
    ) {
        let mut objective: Option<i64> = None;

        while let Some(output) = rx.recv().await {
            match output {
                Msg::Solution(s) => {
                    if objective_type.is_better(objective, s.objective) {
                        objective = Some(s.objective);
                        println!("{}", s.solution);
                    }
                }
                Msg::Status(s) => {
                    println!("{:?}", s);
                    if matches!(s, Status::OptimalSolution) {
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

    async fn start_solver(&mut self, elem: ScheduleElement) -> std::io::Result<()> {
        let mut cmd = Command::new("minizinc");
        cmd.arg("--solver").arg(&elem.solver);
        cmd.arg(&self.args.model);

        if let Some(data_path) = &self.args.data {
            cmd.arg(data_path);
        }

        cmd.arg("-i");
        cmd.arg("--json-stream");
        cmd.arg("--output-mode").arg("json");
        cmd.arg("--output-objective");

        // if self.args.output_objective { // TODO make this an option to the output we print since it is in the rules i think
        //     cmd.arg("--output-objective");
        // }

        // if self.args.ignore_search {  // TODO maybe also this? This option however gives some errors for some solvers
        //     cmd.arg("-f");
        // }
        // cmd.arg("-f");

        cmd.arg("-p").arg(elem.cores.to_string());

        #[cfg(unix)]
        cmd.process_group(0); // let OS give it a group process id

        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        let mut child = cmd.spawn()?;
        let pid = child.id().expect("Child has no PID");
        {
            let mut map = self.solver_to_pid.lock().await;
            map.insert(elem.id, pid);
        }

        let stdout = child.stdout.take().expect("Failed stdout");
        let stderr = child.stderr.take().expect("Failed stderr");

        let tx_clone = self.tx.clone();
        let verbosity = self.args.debug_verbosity;
        tokio::spawn(async move {
            Self::handle_solver_stdout(stdout, tx_clone, verbosity).await;
        });

        let verbosity_stderr = self.args.debug_verbosity;
        tokio::spawn(async move { Self::handle_solver_stderr(stderr, verbosity_stderr).await });

        let solver_to_pid_clone = self.solver_to_pid.clone();
        let solver_name = elem.solver.clone();
        let verbosity_wait = self.args.debug_verbosity;

        tokio::spawn(async move {
            match child.wait().await {
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
            map.remove(&elem.id);
        });

        Ok(())
    }

    async fn handle_solver_stdout(
        stdout: tokio::process::ChildStdout,
        tx: tokio::sync::mpsc::UnboundedSender<Msg>,
        verbosity: DebugVerbosityLevel,
    ) {
        let reader = BufReader::new(stdout);
        let mut lines = reader.lines();

        loop {
            match lines.next_line().await {
                Ok(Some(line)) => {
                    let output = match Output::parse(&line, verbosity) {
                        Ok(o) => o,
                        Err(e) => {
                            if verbosity >= DebugVerbosityLevel::Error {
                                eprintln!("Error parsing solver output: {:?}", e);
                            }
                            continue;
                        }
                    };
                    let msg = match output {
                        Output::Ignore => continue,
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
                Ok(None) => break, // EOF
                Err(e) => {
                    if verbosity >= DebugVerbosityLevel::Error {
                        eprintln!("Error reading solver stdout: {}", e);
                    }
                    break;
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
        &mut self,
        schedule: Schedule,
    ) -> std::result::Result<(), Vec<std::io::Error>> {
        let mut errors = Vec::new();

        for elem in schedule {
            if let Err(e) = self.start_solver(elem).await {
                errors.push(e);
            }
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

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
        ids: Vec<usize>,
        signal: String,
    ) -> std::result::Result<(), Vec<Error>> {
        let futures = ids
            .iter()
            .map(|id| Self::send_signal_to_solver(solver_to_pid.clone(), *id, signal.clone()));
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
        signal: String,
    ) -> std::result::Result<(), Vec<Error>> {
        let ids: Vec<usize> = { solver_to_pid.lock().await.keys().cloned().collect() };
        Self::send_signal_to_solvers(solver_to_pid.clone(), ids, signal).await
    }

    pub async fn suspend_solver(&self, id: usize) -> std::result::Result<(), Error> {
        Self::send_signal_to_solver(self.solver_to_pid.clone(), id, String::from(SUSPEND_SIGNAL))
            .await
    }

    pub async fn suspend_solvers(&self, ids: Vec<usize>) -> std::result::Result<(), Vec<Error>> {
        Self::send_signal_to_solvers(
            self.solver_to_pid.clone(),
            ids,
            String::from(SUSPEND_SIGNAL),
        )
        .await
    }

    pub async fn suspend_all_solvers(&self) -> std::result::Result<(), Vec<Error>> {
        Self::send_signal_to_all_solvers(self.solver_to_pid.clone(), String::from(SUSPEND_SIGNAL))
            .await
    }

    pub async fn resume_solver(&self, id: usize) -> std::result::Result<(), Error> {
        Self::send_signal_to_solver(self.solver_to_pid.clone(), id, String::from(RESUME_SIGNAL))
            .await
    }

    pub async fn resume_solvers(&self, ids: Vec<usize>) -> std::result::Result<(), Vec<Error>> {
        Self::send_signal_to_solvers(self.solver_to_pid.clone(), ids, String::from(RESUME_SIGNAL))
            .await
    }

    pub async fn resume_all_solvers(&self) -> std::result::Result<(), Vec<Error>> {
        Self::send_signal_to_all_solvers(self.solver_to_pid.clone(), String::from(RESUME_SIGNAL))
            .await
    }

    async fn _stop_solver(
        solver_to_pid: Arc<Mutex<HashMap<usize, u32>>>,
        id: usize,
    ) -> std::result::Result<(), Error> {
        Self::send_signal_to_solver(solver_to_pid.clone(), id, String::from(KILL_SIGNAL)).await
    }

    async fn _stop_solvers(
        solver_to_pid: Arc<Mutex<HashMap<usize, u32>>>,
        ids: Vec<usize>,
    ) -> std::result::Result<(), Vec<Error>> {
        Self::send_signal_to_solvers(solver_to_pid.clone(), ids, String::from(KILL_SIGNAL)).await
    }

    async fn _stop_all_solvers(
        solver_to_pid: Arc<Mutex<HashMap<usize, u32>>>,
    ) -> std::result::Result<(), Vec<Error>> {
        Self::send_signal_to_all_solvers(solver_to_pid.clone(), String::from(KILL_SIGNAL)).await
    }

    pub async fn stop_solver(&self, id: usize) -> std::result::Result<(), Error> {
        Self::_stop_solver(self.solver_to_pid.clone(), id).await
    }

    pub async fn stop_solvers(&self, ids: Vec<usize>) -> std::result::Result<(), Vec<Error>> {
        Self::_stop_solvers(self.solver_to_pid.clone(), ids).await
    }

    pub async fn stop_all_solvers(&self) -> std::result::Result<(), Vec<Error>> {
        Self::_stop_all_solvers(self.solver_to_pid.clone()).await
    }
}

fn cleanup_handler(solver_to_pid: Arc<Mutex<HashMap<usize, u32>>>) {
    let solver_to_pid_clone = solver_to_pid.clone();

    ctrlc::set_handler(move || {
        let solver_to_pid_guard = solver_to_pid_clone.blocking_lock();

        for pid in solver_to_pid_guard.values() {
            let _ = kill_tree::blocking::kill_tree(*pid);
        }

        std::process::exit(0);
    })
    .expect("Error setting Ctrl-C handler");
}
