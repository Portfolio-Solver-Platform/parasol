use crate::args::RunArgs;
use crate::insert_objective::ObjectiveInserter;
use crate::model_parser::{ModelParseError, ObjectiveType, ObjectiveValue, get_objective_type};
use crate::mzn_to_fzn::compilation_manager::{self, CompilationManager};
use crate::process_tree::{
    get_process_tree_memory, recursive_force_kill, send_signals_to_process_tree,
};
use crate::scheduler::ScheduleElement;
use crate::solver_config::SolverInputType;
use crate::solver_output::{Output, Solution, Status};
use crate::{logging, mzn_to_fzn, solver_config, solver_output};
use async_tempfile::TempFile;
use futures::future::join_all;
use nix::errno::Errno;
#[cfg(target_os = "linux")]
use nix::sched::{CpuSet, sched_setaffinity};
use nix::sys::signal::Signal;
#[cfg(target_os = "linux")]
use nix::unistd;
use std::collections::{BTreeSet, HashMap, HashSet};
use std::io::Write;
use std::path::Path;
use std::process::Stdio;
use std::sync::Arc;
use sysinfo::System;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::{Mutex, RwLock, mpsc};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("waited for a failed compilation")]
    WaitForCompilation(#[from] compilation_manager::WaitForError),
    #[error("invalid solver: {0}")]
    InvalidSolver(String),
    #[error("IO error")]
    Io(#[from] std::io::Error),
    #[error("failed to parse solver output")]
    OutputParse(#[from] solver_output::Error),
    #[error("failed to parse model")]
    ModelParse(#[from] ModelParseError),
    #[error("failed to convert MiniZinc (mzn) to FlatZinc (fzn) format")]
    FznConversion(#[from] mzn_to_fzn::ConversionError),
    #[error("failed to retrieve system cores")]
    CPUCoresRetrieval(String),
    #[error("could not set solver to a specific core")]
    SolverSetCoreAffinity(#[from] Errno),
    #[error("solver with ID '{0}' has input type of JSON but has no executable")]
    ExecutableMissingForJsonSolver(String),
    #[error("piping failed for process: {0}")]
    Pipe(String),
    #[error("task join failed")]
    JoinError(#[from] tokio::task::JoinError),
    #[error("conversion was cancelled")]
    MznToFzn(#[from] mzn_to_fzn::Error),
}
pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug)]
enum Msg {
    Solution(Solution),
    Status(Status),
}

#[derive(Clone)]
struct SolverProcess {
    pid: u32,
    best_objective: Option<ObjectiveValue>,
}

impl Drop for SolverProcess {
    fn drop(&mut self) {
        let _ = send_signals_to_process_tree(self.pid, vec![Signal::SIGTERM, Signal::SIGCONT]);
        let pid_clone = self.pid;

        std::thread::spawn(move || {
            let _ = recursive_force_kill(pid_clone);
        });
    }
}

pub struct SolverManager {
    tx: mpsc::UnboundedSender<Msg>,
    solver_processes: Arc<Mutex<HashMap<u64, SolverProcess>>>,
    current_solvers: Arc<Mutex<HashSet<u64>>>,
    args: RunArgs,
    mzn_to_fzn: Arc<CompilationManager>,
    best_objective: Arc<RwLock<Option<ObjectiveValue>>>,
    solver_info: Arc<solver_config::Solvers>,
    objective_type: ObjectiveType,
    solver_args: HashMap<String, Vec<String>>,
    available_cores: Arc<Mutex<BTreeSet<usize>>>, // assume that smallest ids is fastest cores, hence we use btreeset to sort the core id's
}

struct PipeCommand {
    pub left: Child,
    pub right: Child,
    pub pipe: JoinHandle<std::io::Result<u64>>,
}

struct PreparedSolver {
    fzn: Child,
    ozn: Child,
    pipe: JoinHandle<std::io::Result<u64>>,
    fzn_guard: Option<TempFile>,
    allocated_cores: Vec<usize>,
}

impl SolverManager {
    pub async fn new(
        args: RunArgs,
        solver_args: HashMap<String, Vec<String>>,
        solver_info: Arc<solver_config::Solvers>,
        compilation_manager: Arc<CompilationManager>,
        program_cancellation_token: CancellationToken,
    ) -> std::result::Result<Self, Error> {
        let objective_type = get_objective_type(&args.minizinc.minizinc_exe, &args.model).await?;
        let (tx, rx) = mpsc::unbounded_channel::<Msg>();
        let solvers = Arc::new(Mutex::new(HashMap::new()));

        let best_objective: Arc<RwLock<Option<i64>>> = Arc::new(RwLock::new(None));

        let shared_objective = best_objective.clone();
        tokio::spawn(async move {
            Self::receiver(
                rx,
                objective_type,
                shared_objective,
                program_cancellation_token,
            )
            .await
        });
        let mut cores = BTreeSet::new();
        if let Some(core_ids) = core_affinity::get_core_ids() {
            for core in core_ids {
                cores.insert(core.id);
            }
        } else {
            return Err(Error::CPUCoresRetrieval(
                "Could not retrieve system cores".to_string(),
            ));
        }

        Ok(Self {
            tx,
            solver_processes: solvers,
            solver_info: solver_info.clone(),
            mzn_to_fzn: compilation_manager,
            current_solvers: Default::default(),
            args,
            best_objective,
            objective_type,
            solver_args,
            available_cores: Arc::new(Mutex::new(cores)),
        })
    }

    async fn receiver(
        mut rx: mpsc::UnboundedReceiver<Msg>,
        objective_type: ObjectiveType,
        shared_objective: Arc<RwLock<Option<ObjectiveValue>>>,
        program_cancellation_token: CancellationToken,
    ) {
        let mut objective: Option<ObjectiveValue> = None;

        while let Some(output) = rx.recv().await {
            match output {
                Msg::Solution(Solution {
                    solution: s,
                    objective: Some(o),
                }) => {
                    if objective_type.is_better(objective, o) {
                        objective = Some(o);
                        {
                            let mut guard = shared_objective.write().await;
                            *guard = Some(o);
                        }
                        println!("{}", s.trim_end());
                        let _ = std::io::stdout().flush();
                    }
                }
                Msg::Solution(Solution {
                    solution: s,
                    objective: None, // is satisfaction problem
                }) => {
                    println!("{}", s.trim_end());
                    let _ = std::io::stdout().flush();
                    // In satisfaction problems, we are only interested in a single solution
                    program_cancellation_token.cancel();
                    break;
                }
                Msg::Status(status) => {
                    if status != Status::Unknown {
                        println!("{}", status.to_dzn_string());
                        let _ = std::io::stdout().flush();
                        program_cancellation_token.cancel();
                        break;
                    }
                }
            }
        }
    }

    fn get_solver_command(
        fzn_path: &Path,
        solver_name: &str,
        cores: usize,
        solver_info: &solver_config::Solvers,
        minizinc_exe: &Path,
        solver_args: &HashMap<String, Vec<String>>,
    ) -> Result<Command> {
        let solver = solver_info.get_by_id(solver_name);

        let make_fzn_cmd = || {
            let mut cmd = Command::new(minizinc_exe);
            cmd.arg("--solver").arg(solver_name);
            cmd
        };
        let mut cmd = match solver {
            Some(solver) => match solver.input_type() {
                SolverInputType::Fzn => make_fzn_cmd(),
                SolverInputType::Json => solver
                    .executable()
                    .ok_or_else(|| Error::ExecutableMissingForJsonSolver(solver_name.to_owned()))?
                    .clone()
                    .into_command(),
            },
            None => make_fzn_cmd(),
        };

        cmd.arg(fzn_path);

        // Apply solver-specific arguments from config
        if let Some(args) = solver_args.get(solver_name) {
            for arg in args {
                cmd.arg(arg);
            }
        } else {
            logging::error_msg!("Solver '{solver_name}' does not have an arguments configuration");
        }

        let supports_p_flag = solver
            .map(|solver| solver.supported_std_flags().p)
            .unwrap_or(true);
        if supports_p_flag {
            cmd.arg("-p").arg(cores.to_string());
        }

        Ok(cmd)
    }

    fn get_ozn_command(minizinc_exe: &Path, ozn_path: &Path) -> Command {
        let mut cmd = Command::new(minizinc_exe);
        cmd.arg("--ozn-file");
        cmd.arg(ozn_path);
        cmd
    }

    #[allow(clippy::too_many_arguments)]
    async fn prepare_solver_process(
        solver_name: &str,
        cores: usize,
        elem_id: u64,
        cancellation_token: &CancellationToken,
        mzn_to_fzn: &CompilationManager,
        solver_info: &solver_config::Solvers,
        best_objective: &RwLock<Option<ObjectiveValue>>,
        objective_type: ObjectiveType,
        minizinc_exe: &Path,
        solver_args: &HashMap<String, Vec<String>>,
        solver_processes: &Mutex<HashMap<u64, SolverProcess>>,
        #[cfg(target_os = "linux")] available_cores: &Arc<Mutex<BTreeSet<usize>>>,
        #[cfg(target_os = "linux")] pin_yuck: bool,
    ) -> std::result::Result<PreparedSolver, ()> {
        mzn_to_fzn.start(solver_name.to_string()).await;

        let Some(conversion_paths) = cancellation_token
            .run_until_cancelled(mzn_to_fzn.wait_for(solver_name))
            .await
        else {
            logging::info!("solver '{solver_name}' was cancelled while waiting for compilation");
            return Err(());
        };

        let Ok(conversion_paths) = conversion_paths.map_err(|e| logging::error!(e.into())) else {
            return Err(());
        };

        // Create ObjectiveInserter inside the spawn
        let objective_inserter = ObjectiveInserter::new(Arc::new(solver_info.clone()));

        let objective = *best_objective.read().await;
        let (fzn_final_path, fzn_guard) = if let Some(obj) = objective {
            if let Ok(new_temp_file) = objective_inserter
                .insert_objective(solver_name, conversion_paths.fzn(), &objective_type, obj)
                .await
            {
                (new_temp_file.file_path().to_path_buf(), Some(new_temp_file))
            } else {
                (conversion_paths.fzn().to_path_buf(), None)
            }
        } else {
            (conversion_paths.fzn().to_path_buf(), None)
        };

        let Ok(mut fzn_cmd) = Self::get_solver_command(
            &fzn_final_path,
            solver_name,
            cores,
            solver_info,
            minizinc_exe,
            solver_args,
        )
        .map_err(|e| logging::error!(e.into())) else {
            return Err(());
        };

        #[cfg(unix)]
        fzn_cmd.process_group(0); // let OS give it a group process id
        fzn_cmd.stderr(Stdio::piped());

        let mut ozn_cmd = Self::get_ozn_command(minizinc_exe, conversion_paths.ozn());
        ozn_cmd.stdout(Stdio::piped());
        ozn_cmd.stderr(Stdio::piped());

        // we lock on solvers to guarantee we dont in another thread try to stop them at the same time
        let mut map = solver_processes.lock().await;
        let Ok(PipeCommand {
            left: fzn,
            right: ozn,
            pipe,
        }) = pipe(fzn_cmd, ozn_cmd).map_err(|e| logging::error!(e.into()))
        else {
            return Err(());
        };

        let pid = fzn.id().expect("Child has no PID");
        let solver_proccess = SolverProcess {
            pid,
            best_objective: objective,
        };

        map.insert(elem_id, solver_proccess);
        drop(map);

        logging::info!("Solver {solver_name} now is running");

        #[allow(unused_mut)]
        let mut allocated_cores: Vec<usize> = Vec::new();
        #[cfg(target_os = "linux")]
        if pin_yuck {
            match pin_yuck_solver_to_cores(pid, cores, available_cores).await {
                Ok(cores) => allocated_cores = cores,
                Err(e) => {
                    logging::error!(e.into());
                    return Err(());
                }
            }
        }

        Ok(PreparedSolver {
            fzn,
            ozn,
            pipe,
            fzn_guard,
            allocated_cores,
        })
    }

    async fn start_solver(&self, elem: &ScheduleElement, cancellation_token: CancellationToken) {
        {
            self.current_solvers.lock().await.insert(elem.id); // keep track of current running/suspended solvers
        }

        // Clone all necessary fields before spawning
        let mzn_to_fzn = self.mzn_to_fzn.clone();
        let solver_info = self.solver_info.clone();
        let minizinc_exe = self.args.minizinc.minizinc_exe.clone();
        let solver_args = self.solver_args.clone();
        let solver_processes = self.solver_processes.clone();
        let tx = self.tx.clone();
        let available_cores = self.available_cores.clone();
        let objective_type = self.objective_type;
        let elem = elem.clone();
        let current_solvers = self.current_solvers.clone();
        #[cfg(target_os = "linux")]
        let pin_yuck = self.args.pin_yuck;
        let best_objective = self.best_objective.clone();

        tokio::spawn(async move {
            let solver_name = &elem.info.name;
            let cores = elem.info.cores;
            let elem_id = elem.id;

            let result = Self::prepare_solver_process(
                solver_name,
                cores,
                elem_id,
                &cancellation_token,
                &mzn_to_fzn,
                &solver_info,
                &best_objective,
                objective_type,
                &minizinc_exe,
                &solver_args,
                &solver_processes,
                #[cfg(target_os = "linux")]
                &available_cores,
                #[cfg(target_os = "linux")]
                pin_yuck,
            )
            .await;

            let Ok(PreparedSolver {
                mut fzn,
                mut ozn,
                pipe,
                fzn_guard,
                allocated_cores,
            }) = result
            else {
                current_solvers.lock().await.remove(&elem_id);
                return;
            };

            let ozn_stdout = ozn.stdout.take().expect("Failed to take ozn stdout");
            let ozn_stderr = ozn.stderr.take().expect("Failed to take ozn stderr");
            let fzn_stderr = fzn.stderr.take().expect("Failed to take fzt stderr");

            let solver_id = elem.id;
            let solver_name_for_wait = elem.info.name.clone();
            let solvers_for_stdout = solver_processes.clone();
            let solvers_for_wait = solver_processes.clone();
            let available_cores_for_wait = available_cores.clone();

            let cancellation_token_stdout = cancellation_token.clone();
            tokio::spawn(async move {
                Self::handle_solver_stdout(
                    ozn_stdout,
                    pipe,
                    tx,
                    solver_id,
                    solvers_for_stdout,
                    objective_type,
                    cancellation_token_stdout,
                )
                .await;
            });

            tokio::spawn(async move { Self::handle_solver_stderr(fzn_stderr).await });
            tokio::spawn(async move { Self::handle_solver_stderr(ozn_stderr).await });

            tokio::spawn(async move {
                let _keep_alive = fzn_guard;

                tokio::select! {
                    result = fzn.wait() => {
                        match result {
                            Ok(status) if !status.success() => {
                                logging::info!("Solver '{}' exited with status: {}", solver_name_for_wait, status);
                            }
                            Err(e) => {
                                logging::error_msg!("Error waiting for solver '{}': {}", solver_name_for_wait, e);
                            }
                            _ => {}
                        }
                    }
                    _ = cancellation_token.cancelled() => {
                        logging::info!("Solver '{}' cancelled", solver_name_for_wait);
                    }
                }

                {
                    let mut cores_guard = available_cores_for_wait.lock().await;
                    for core_id in allocated_cores {
                        cores_guard.insert(core_id);
                    }
                }
                logging::info!("solver exitted {solver_id}");
                current_solvers.lock().await.remove(&solver_id);
                let mut map = solvers_for_wait.lock().await;
                map.remove(&solver_id);
            });
        });
    }

    async fn handle_solver_stdout(
        stdout: tokio::process::ChildStdout,
        pipe: JoinHandle<std::io::Result<u64>>,
        tx: tokio::sync::mpsc::UnboundedSender<Msg>,
        solver_id: u64,
        solver_processes: Arc<Mutex<HashMap<u64, SolverProcess>>>,
        objective_type: ObjectiveType,
        cancellation_token: CancellationToken,
    ) {
        let reader = BufReader::new(stdout);
        let mut lines = reader.lines();
        let mut parser = solver_output::Parser::new(objective_type);

        let mut local_best: Option<ObjectiveValue> = {
            let map = solver_processes.lock().await;
            map.get(&solver_id).and_then(|s| s.best_objective)
        };

        loop {
            let line = tokio::select! {
                result = lines.next_line() => {
                    match result {
                        Ok(Some(line)) => line,
                        Ok(None) => break,
                        Err(err) => {
                            logging::error!(HandleStdoutError::Read(err).into());
                            break;
                        }
                    }
                }
                _ = cancellation_token.cancelled() => break,
            };
            let output = match parser.next_line(&line) {
                Ok(o) => o,
                Err(e) => {
                    logging::error!(HandleStdoutError::Parse(e).into());
                    continue;
                }
            };

            let Some(output) = output else {
                continue;
            };

            let msg = match output {
                Output::Solution(Solution {
                    solution: s,
                    objective: None,
                }) => Msg::Solution(Solution {
                    solution: s,
                    objective: None,
                }),
                Output::Solution(Solution {
                    solution: s,
                    objective: Some(o),
                }) => {
                    if objective_type.is_better(local_best, o) {
                        local_best = Some(o);
                        let mut map = solver_processes.lock().await;
                        if let Some(state) = map.get_mut(&solver_id) {
                            state.best_objective = local_best;
                        }
                    }
                    Msg::Solution(Solution {
                        solution: s,
                        objective: Some(o),
                    })
                }
                Output::Status(status) => Msg::Status(status),
            };

            if let Err(e) = tx.send(msg) {
                logging::error!(HandleStdoutError::from(e).into());
                break;
            }
        }

        match pipe.await {
            Ok(_) => {}
            Err(e) => {
                logging::error!(HandleStdoutError::from(e).into());
            }
        }
    }

    async fn handle_solver_stderr(stderr: tokio::process::ChildStderr) {
        let reader = BufReader::new(stderr);
        let mut lines = reader.lines();

        while let Some(line) = lines.next_line().await.unwrap_or_else(|e| {
            logging::error_msg!("Error reading solver stderr: {}", e);
            None
        }) {
            logging::error_msg!("Solver stderr: {}", line);
        }
    }

    pub async fn start_solvers(
        &self,
        schedule: &[ScheduleElement],
        cancellation_token: CancellationToken,
    ) {
        let futures = schedule
            .iter()
            .map(|elem| self.start_solver(elem, cancellation_token.clone()));
        join_all(futures).await;
    }

    async fn send_signals_to_solver(
        signals: Vec<Signal>,
        id: u64,
        solvers_guard: tokio::sync::MutexGuard<'_, HashMap<u64, SolverProcess>>,
    ) -> Result<()> {
        let pid = match solvers_guard.get(&id) {
            Some(state) => state.pid,
            None => return Err(Error::InvalidSolver(format!("Solver {id} not running"))),
        };
        send_signals_to_process_tree(pid, signals)
            .map_err(|e| Error::InvalidSolver(format!("Failed to send signals: {}", e)))
    }

    async fn send_signals_to_solvers(
        signals: Vec<Signal>,
        ids: &[u64],
        solver_processes: tokio::sync::MutexGuard<'_, HashMap<u64, SolverProcess>>,
    ) -> std::result::Result<(), Vec<Error>> {
        let futures = ids.iter().map(async |id| {
            let pid = match solver_processes.get(id) {
                Some(state) => state.pid,
                None => return Err(Error::InvalidSolver(format!("Solver {id} not running"))),
            };
            send_signals_to_process_tree(pid, signals.clone())
                .map_err(|e| Error::InvalidSolver(format!("Failed to send signals: {}", e)))
        });
        let results = join_all(futures).await;
        let errors: Vec<Error> = results.into_iter().filter_map(|res| res.err()).collect();

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    #[allow(dead_code)]
    async fn send_signals_to_all_solvers(
        solver_processes: Arc<Mutex<HashMap<u64, SolverProcess>>>,
        signals: Vec<Signal>,
    ) -> std::result::Result<(), Vec<Error>> {
        let solvers_guard = solver_processes.lock().await;
        let ids: Vec<u64> = { solvers_guard.keys().cloned().collect() };
        Self::send_signals_to_solvers(signals, &ids, solvers_guard).await
    }

    #[allow(dead_code)]
    pub async fn suspend_solver(&self, id: u64) -> std::result::Result<(), Error> {
        let solvers_guard = self.solver_processes.lock().await;
        Self::send_signals_to_solver(vec![Signal::SIGSTOP], id, solvers_guard).await
    }

    pub async fn suspend_solvers(&self, ids: &[u64]) -> std::result::Result<(), Vec<Error>> {
        let solvers_guard = self.solver_processes.lock().await;
        Self::send_signals_to_solvers(vec![Signal::SIGSTOP], ids, solvers_guard).await
    }

    #[allow(dead_code)]
    pub async fn suspend_all_solvers(&self) -> std::result::Result<(), Vec<Error>> {
        Self::send_signals_to_all_solvers(self.solver_processes.clone(), vec![Signal::SIGSTOP])
            .await
    }

    #[allow(dead_code)]
    pub async fn resume_solver(&self, id: u64) -> std::result::Result<(), Error> {
        let solvers_guard = self.solver_processes.lock().await;
        Self::send_signals_to_solver(vec![Signal::SIGCONT], id, solvers_guard).await
    }

    pub async fn resume_solvers(&self, ids: &[u64]) -> std::result::Result<(), Vec<Error>> {
        let solvers_guard = self.solver_processes.lock().await;
        Self::send_signals_to_solvers(vec![Signal::SIGCONT], ids, solvers_guard).await
    }

    #[allow(dead_code)]
    pub async fn resume_all_solvers(&self) -> std::result::Result<(), Vec<Error>> {
        Self::send_signals_to_all_solvers(self.solver_processes.clone(), vec![Signal::SIGCONT])
            .await
    }

    async fn _stop_solver(
        solver_processes: Arc<Mutex<HashMap<u64, SolverProcess>>>,
        id: u64,
    ) -> std::result::Result<(), Error> {
        let mut map = solver_processes.lock().await;
        Self::kill_solver(id, &mut map).await
    }

    async fn _stop_solvers(
        solver_processes: Arc<Mutex<HashMap<u64, SolverProcess>>>,
        ids: &[u64],
    ) -> std::result::Result<(), Vec<Error>> {
        let mut results = Vec::new();
        {
            let mut map = solver_processes.lock().await;
            for id in ids {
                results.push(Self::kill_solver(*id, &mut map).await);
            }
        };

        let errors: Vec<Error> = results.into_iter().filter_map(|res| res.err()).collect();

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    async fn _stop_all_solvers(
        solver_processes: Arc<Mutex<HashMap<u64, SolverProcess>>>,
    ) -> std::result::Result<(), Vec<Error>> {
        let mut results = Vec::new();

        {
            let mut map = solver_processes.lock().await;
            let ids: Vec<u64> = map.keys().copied().collect();
            for id in ids {
                results.push(Self::kill_solver(id, &mut map).await);
            }
        };

        let errors: Vec<Error> = results.into_iter().filter_map(|res| res.err()).collect();

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    pub async fn stop_solver(&self, id: u64) -> std::result::Result<(), Error> {
        Self::_stop_solver(self.solver_processes.clone(), id).await
    }

    pub async fn stop_solvers(&self, ids: &[u64]) -> std::result::Result<(), Vec<Error>> {
        Self::_stop_solvers(self.solver_processes.clone(), ids).await
    }

    #[allow(dead_code)]
    pub async fn stop_all_solvers(&self) -> std::result::Result<(), Vec<Error>> {
        Self::_stop_all_solvers(self.solver_processes.clone()).await
    }

    pub async fn active_solver_ids(&self) -> HashSet<u64> {
        self.current_solvers.lock().await.clone()
    }

    pub async fn solvers_sorted_by_mem(&self, ids: &[u64], system: &System) -> Vec<(u64, u64)> {
        let solvers: Vec<(u32, u64)> = {
            let map = self.solver_processes.lock().await;
            let mut solvers: Vec<(u32, u64)> = Vec::new();
            for id in ids {
                match map.get(id) {
                    Some(state) => solvers.push((state.pid, *id)),
                    None => {
                        logging::warning!(
                            "solvers_sorted_by_mem failed to extract solver pid for id {}",
                            id
                        );
                    }
                }
            }
            solvers
        };

        let mut solver_mem = solvers
            .into_iter()
            .map(|(pid, id)| (get_process_tree_memory(system, pid), id))
            .collect::<Vec<(u64, u64)>>();
        solver_mem.sort_by_key(|(mem, _)| std::cmp::Reverse(*mem));
        solver_mem
    }

    pub async fn get_best_objective(&self) -> Option<ObjectiveValue> {
        *self.best_objective.read().await
    }

    pub async fn get_solver_objectives(&self) -> HashMap<u64, Option<ObjectiveValue>> {
        self.solver_processes
            .lock()
            .await
            .iter()
            .map(|(id, state)| (*id, state.best_objective))
            .collect()
    }

    pub fn objective_type(&self) -> ObjectiveType {
        self.objective_type
    }

    async fn kill_solver(
        id: u64,
        solvers_map: &mut tokio::sync::MutexGuard<'_, HashMap<u64, SolverProcess>>,
    ) -> Result<()> {
        // let RAII clean up the solver. Look in drop function for SolverProcess.
        if solvers_map.remove(&id).is_none() {
            return Err(Error::InvalidSolver(format!("Solver {id} not running")));
        }

        Ok(())
    }
}

fn pipe(mut left: Command, mut right: Command) -> Result<PipeCommand> {
    let mut left_child = left.stdout(Stdio::piped()).spawn()?;

    #[cfg(unix)]
    {
        let left_pid = left_child
            .id()
            .ok_or_else(|| Error::Pipe("Could not get PID for process".to_string()))?;
        right.process_group(left_pid as i32);
    }

    let mut right_child = right.stdin(Stdio::piped()).spawn()?;

    let mut left_stdout = left_child
        .stdout
        .take()
        .ok_or_else(|| Error::Pipe("Could not capture the left process' stdout".to_string()))?;
    let mut right_stdin = right_child
        .stdin
        .take()
        .ok_or_else(|| Error::Pipe("Could not capture the right process' stdin".to_string()))?;

    let pipe_task =
        tokio::spawn(async move { tokio::io::copy(&mut left_stdout, &mut right_stdin).await });

    Ok(PipeCommand {
        left: left_child,
        right: right_child,
        pipe: pipe_task,
    })
}

#[cfg(target_os = "linux")]
async fn pin_yuck_solver_to_cores(
    pid: u32,
    cores: usize,
    available_cores: &Arc<Mutex<BTreeSet<usize>>>,
) -> Result<Vec<usize>> {
    let mut cpu_set = CpuSet::new();
    let mut allocated_cores: Vec<usize> = Vec::new();

    {
        let mut available_cores_guard = available_cores.lock().await;
        for _ in 0..cores {
            if let Some(val) = available_cores_guard.pop_first() {
                if let Err(e) = cpu_set.set(val) {
                    for c in &allocated_cores {
                        available_cores_guard.insert(*c);
                    }
                    return Err(e.into());
                }
                allocated_cores.push(val);
            } else {
                for c in &allocated_cores {
                    available_cores_guard.insert(*c);
                }

                return Err(Error::CPUCoresRetrieval(
                    "Schedule contained more cores than there was available".to_string(),
                ));
            }
        }
    }

    if let Err(e) = sched_setaffinity(unistd::Pid::from_raw(pid as i32), &cpu_set) {
        logging::warning!("Failed to set affinity (process might have exited): {e}");
    }

    Ok(allocated_cores)
}

#[derive(Debug, thiserror::Error)]
enum HandleStdoutError {
    #[error("failed to read solver stdout")]
    Read(tokio::io::Error),
    #[error("failed to parse solver stdout")]
    Parse(solver_output::Error),
    #[error("failed to send message")]
    SendMessage(#[from] tokio::sync::mpsc::error::SendError<Msg>),
    #[error("failed to pipe from fzn to ozn")]
    Pipe(#[from] tokio::task::JoinError),
}
