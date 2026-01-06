use crate::args::Args;
use crate::insert_objective::insert_objective;
use crate::model_parser::{ModelParseError, ObjectiveType, ObjectiveValue, get_objective_type};
use crate::process_tree::get_process_tree_memory;
use crate::scheduler::ScheduleElement;
use crate::solver_output::{Output, Solution, Status};
use crate::{logging, mzn_to_fzn, solver_output};
use futures::future::join_all;

use nix::errno::Errno;
#[cfg(target_os = "linux")]
use nix::sched::{CpuSet, sched_setaffinity};
use nix::sys::signal::{self, Signal};

use nix::unistd;
use std::collections::{BTreeSet, HashMap, HashSet};
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
}
pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug)]
enum Msg {
    Solution(Solution),
    Status(Status),
}

struct SolverProcess {
    pid: u32,
    best_objective: Option<ObjectiveValue>,
}

impl Drop for SolverProcess {
    fn drop(&mut self) {
        let gpid = unistd::Pid::from_raw(-(self.pid as i32));
        let _ = signal::kill(gpid, Signal::SIGTERM);
        let _ = signal::kill(gpid, Signal::SIGCONT);
    }
}

pub struct SolverManager {
    tx: mpsc::UnboundedSender<Msg>,
    solvers: Arc<Mutex<HashMap<u64, SolverProcess>>>,
    args: Args,
    mzn_to_fzn: mzn_to_fzn::CachedConverter,
    best_objective: Arc<RwLock<Option<ObjectiveValue>>>,
    objective_type: ObjectiveType,
    solver_args: HashMap<String, Vec<String>>,
    available_cores: Arc<Mutex<BTreeSet<usize>>>, // assume that smallest ids is fastest cores, hence we use btreeset to sort the core id's
}

struct PipeCommand {
    pub left: Child,
    pub right: Child,
    pub pipe: JoinHandle<std::io::Result<u64>>,
}

impl SolverManager {
    pub async fn new(
        args: Args,
        solver_args: HashMap<String, Vec<String>>,
        token: CancellationToken,
    ) -> std::result::Result<Self, Error> {
        let objective_type = get_objective_type(&args.minizinc_exe, &args.model).await?;
        let (tx, rx) = mpsc::unbounded_channel::<Msg>();
        let solvers = Arc::new(Mutex::new(HashMap::new()));

        let best_objective: Arc<RwLock<Option<i64>>> = Arc::new(RwLock::new(None));

        let shared_objective = best_objective.clone();
        let token_clone = token.clone();
        tokio::spawn(async move {
            Self::receiver(rx, objective_type, shared_objective, token_clone).await
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
            solvers,
            mzn_to_fzn: mzn_to_fzn::CachedConverter::new(args.clone()),
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
        token: CancellationToken,
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
                    }
                }
                Msg::Solution(Solution {
                    solution: s,
                    objective: None, // is satisfaction problem
                }) => println!("{}", s.trim_end()),
                Msg::Status(status) => {
                    if status != Status::Unknown {
                        println!("{}", status.to_dzn_string());
                        token.cancel();
                        break;
                    }
                }
            }
        }
    }

    fn get_fzn_command(&self, fzn_path: &Path, solver_name: &str, cores: usize) -> Command {
        // Taskset approach (commented out, using sched_setaffinity instead)
        // let mut cmd = if !allocated_cores.is_empty() {
        //     let core_list = allocated_cores
        //         .iter()
        //         .map(|c| c.to_string())
        //         .collect::<Vec<_>>()
        //         .join(",");
        //     let mut taskset_cmd = Command::new("taskset");
        //     taskset_cmd.arg("-c").arg(core_list);
        //     taskset_cmd.arg(&self.args.minizinc_exe);
        //     taskset_cmd
        // } else {
        //     Command::new(&self.args.minizinc_exe)
        // };

        let mut cmd = Command::new(&self.args.minizinc_exe);
        cmd.arg("--solver").arg(solver_name);
        cmd.arg(fzn_path);

        // Apply solver-specific arguments from config
        if let Some(args) = self.solver_args.get(solver_name) {
            for arg in args {
                cmd.arg(arg);
            }
        } else {
            logging::error_msg!("Solver '{solver_name}' does not have an arguments configuration");
        }

        cmd.arg("-p").arg(cores.to_string());

        cmd
    }

    fn get_ozn_command(&self, ozn_path: &Path) -> Command {
        let mut cmd = Command::new(&self.args.minizinc_exe);
        cmd.arg("--ozn-file");
        cmd.arg(ozn_path);
        cmd
    }

    async fn start_solver(
        &self,
        elem: &ScheduleElement,
        objective: Option<ObjectiveValue>,
    ) -> Result<()> {
        let solver_name = &elem.info.name;
        let cores = elem.info.cores;

        let conversion_paths = self
            .mzn_to_fzn
            .convert(solver_name)
            .await?;

        let (fzn_final_path, fzn_guard) = if let Some(obj) = objective {
            if let Ok(new_temp_file) =
                insert_objective(conversion_paths.fzn(), &self.objective_type, obj).await
            {
                (new_temp_file.file_path().to_path_buf(), Some(new_temp_file))
            } else {
                (conversion_paths.fzn().to_path_buf(), None)
            }
        } else {
            (conversion_paths.fzn().to_path_buf(), None)
        };

        // Taskset approach: allocate cores before building the command
        // let mut allocated_cores: Vec<usize> = Vec::new();
        // #[cfg(target_os = "linux")]
        // {
        //     let mut available_cores_guard = self.available_cores.lock().await;
        //     for _ in 0..cores {
        //         if let Some(val) = available_cores_guard.pop_first() {
        //             allocated_cores.push(val);
        //         } else {
        //             // Return already-allocated cores before erroring
        //             for c in &allocated_cores {
        //                 available_cores_guard.insert(*c);
        //             }
        //             return Err(Error::CPUCoresRetrieval(
        //                 "Schedule contained more cores than there was available".to_string(),
        //             ));
        //         }
        //     }
        // }

        let mut fzn_cmd = self.get_fzn_command(&fzn_final_path, solver_name, cores);
        #[cfg(unix)]
        fzn_cmd.process_group(0); // let OS give it a group process id
        fzn_cmd.stderr(Stdio::piped());

        let mut ozn_cmd = self.get_ozn_command(conversion_paths.ozn());
        ozn_cmd.stdout(Stdio::piped());
        ozn_cmd.stderr(Stdio::piped());

        let PipeCommand {
            left: mut fzn,
            right: mut ozn,
            pipe,
        } = pipe(fzn_cmd, ozn_cmd).await?;

        let pid = fzn.id().expect("Child has no PID");
        let mut allocated_cores: Vec<usize> = Vec::new();
        #[cfg(target_os = "linux")]
        if self.args.pin_cores {
            let mut cpu_set = CpuSet::new();
            {
                let mut available_cores_guard = self.available_cores.lock().await;
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
                eprintln!(
                    "Warning: Failed to set affinity (process might have exited): {}",
                    e
                );
            }
        }

        {
            let mut map = self.solvers.lock().await;
            map.insert(
                elem.id,
                SolverProcess {
                    pid,
                    best_objective: objective,
                },
            );
        }

        let ozn_stdout = ozn.stdout.take().expect("Failed to take ozn stdout");
        let ozn_stderr = ozn.stderr.take().expect("Failed to take ozn stderr");
        let fzn_stderr = fzn.stderr.take().expect("Failed to take fzt stderr");

        let tx_clone = self.tx.clone();
        let solvers_clone_stdout = self.solvers.clone();
        let solver_id = elem.id;
        let objective_type = self.objective_type;
        tokio::spawn(async move {
            Self::handle_solver_stdout(
                ozn_stdout,
                pipe,
                tx_clone,
                solver_id,
                solvers_clone_stdout,
                objective_type,
            )
            .await;
        });

        tokio::spawn(async move { Self::handle_solver_stderr(fzn_stderr).await });
        tokio::spawn(async move { Self::handle_solver_stderr(ozn_stderr).await });

        let solvers_clone = self.solvers.clone();
        let solver_name = elem.info.name.clone();
        let available_cores_clone = self.available_cores.clone();

        tokio::spawn(async move {
            let _keep_alive = fzn_guard;
            match fzn.wait().await {
                Ok(status) if !status.success() => {
                    logging::info!("Solver '{}' exited with status: {}", solver_name, status);
                }
                Err(e) => {
                    logging::error_msg!("Error waiting for solver '{}': {}", solver_name, e);
                }
                _ => {}
            }

            {
                let mut cores_guard = available_cores_clone.lock().await;
                for core_id in allocated_cores {
                    cores_guard.insert(core_id);
                }
            }

            let mut map = solvers_clone.lock().await;
            map.remove(&solver_id);
        });

        Ok(())
    }

    async fn handle_solver_stdout(
        stdout: tokio::process::ChildStdout,
        pipe: JoinHandle<std::io::Result<u64>>,
        tx: tokio::sync::mpsc::UnboundedSender<Msg>,
        solver_id: u64,
        solvers: Arc<Mutex<HashMap<u64, SolverProcess>>>,
        objective_type: ObjectiveType,
    ) {
        let reader = BufReader::new(stdout);
        let mut lines = reader.lines();
        let mut parser = solver_output::Parser::new(objective_type);

        let mut local_best: Option<ObjectiveValue> = {
            let map = solvers.lock().await;
            map.get(&solver_id).and_then(|s| s.best_objective)
        };

        while let Ok(Some(line)) = lines.next_line().await.map_err(|err| {
            logging::error!(HandleStdoutError::Read(err).into());
        }) {
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
                        let mut map = solvers.lock().await;
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
        solvers: Arc<Mutex<HashMap<u64, SolverProcess>>>,
        id: u64,
        signal: Signal,
    ) -> std::result::Result<(), Error> {
        let map = solvers.lock().await;
        let pid = match map.get(&id) {
            Some(state) => state.pid,
            None => return Err(Error::InvalidSolver(format!("Solver {id} not running"))),
        };
        let gpid = unistd::Pid::from_raw(-(pid as i32));
        let _ = signal::kill(gpid, signal);

        Ok(())
    }

    async fn send_signal_to_solvers(
        solvers: Arc<Mutex<HashMap<u64, SolverProcess>>>,
        ids: &[u64],
        signal: Signal,
    ) -> std::result::Result<(), Vec<Error>> {
        let futures = ids
            .iter()
            .map(|id| Self::send_signal_to_solver(solvers.clone(), *id, signal));
        let results = join_all(futures).await;
        let errors: Vec<Error> = results.into_iter().filter_map(|res| res.err()).collect();

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    async fn send_signal_to_all_solvers(
        solvers: Arc<Mutex<HashMap<u64, SolverProcess>>>,
        signal: Signal,
    ) -> std::result::Result<(), Vec<Error>> {
        let ids: Vec<u64> = { solvers.lock().await.keys().cloned().collect() };
        Self::send_signal_to_solvers(solvers.clone(), &ids, signal).await
    }

    pub async fn suspend_solver(&self, id: u64) -> std::result::Result<(), Error> {
        Self::send_signal_to_solver(self.solvers.clone(), id, Signal::SIGSTOP).await
    }

    pub async fn suspend_solvers(&self, ids: &[u64]) -> std::result::Result<(), Vec<Error>> {
        Self::send_signal_to_solvers(self.solvers.clone(), ids, Signal::SIGSTOP).await
    }

    pub async fn suspend_all_solvers(&self) -> std::result::Result<(), Vec<Error>> {
        Self::send_signal_to_all_solvers(self.solvers.clone(), Signal::SIGSTOP).await
    }

    pub async fn resume_solver(&self, id: u64) -> std::result::Result<(), Error> {
        Self::send_signal_to_solver(self.solvers.clone(), id, Signal::SIGCONT).await
    }

    pub async fn resume_solvers(&self, ids: &[u64]) -> std::result::Result<(), Vec<Error>> {
        Self::send_signal_to_solvers(self.solvers.clone(), ids, Signal::SIGCONT).await
    }

    pub async fn resume_all_solvers(&self) -> std::result::Result<(), Vec<Error>> {
        Self::send_signal_to_all_solvers(self.solvers.clone(), Signal::SIGCONT).await
    }

    async fn _stop_solver(
        solvers: Arc<Mutex<HashMap<u64, SolverProcess>>>,
        id: u64,
    ) -> std::result::Result<(), Error> {
        Self::kill_solver(solvers, id).await
    }

    async fn _stop_solvers(
        solvers: Arc<Mutex<HashMap<u64, SolverProcess>>>,
        ids: &[u64],
    ) -> std::result::Result<(), Vec<Error>> {
        let futures = ids.iter().map(|id| Self::kill_solver(solvers.clone(), *id));
        let results = join_all(futures).await;
        let errors: Vec<Error> = results.into_iter().filter_map(|res| res.err()).collect();

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    async fn _stop_all_solvers(
        solvers: Arc<Mutex<HashMap<u64, SolverProcess>>>,
    ) -> std::result::Result<(), Vec<Error>> {
        let ids: Vec<u64> = {
            let map = solvers.lock().await;
            map.keys().copied().collect()
        };

        Self::_stop_solvers(solvers.clone(), &ids).await
    }

    pub async fn stop_solver(&self, id: u64) -> std::result::Result<(), Error> {
        Self::_stop_solver(self.solvers.clone(), id).await
    }

    pub async fn stop_solvers(&self, ids: &[u64]) -> std::result::Result<(), Vec<Error>> {
        Self::_stop_solvers(self.solvers.clone(), ids).await
    }

    pub async fn stop_all_solvers(&self) -> std::result::Result<(), Vec<Error>> {
        Self::_stop_all_solvers(self.solvers.clone()).await
    }

    pub async fn active_solver_ids(&self) -> HashSet<u64> {
        self.solvers.lock().await.keys().copied().collect()
    }

    pub async fn solvers_sorted_by_mem(&self, ids: &[u64], system: &System) -> Vec<(u64, u64)> {
        let solvers: Vec<(u32, u64)> = {
            let map = self.solvers.lock().await;
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
        self.solvers
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
        solvers: Arc<Mutex<HashMap<u64, SolverProcess>>>,
        id: u64,
    ) -> std::result::Result<(), Error> {
        let mut map = solvers.lock().await;
        if map.remove(&id).is_none() {
            return Err(Error::InvalidSolver(format!("Solver {id} not running")));
        }

        Ok(())
    }
}

async fn pipe(mut left: Command, mut right: Command) -> Result<PipeCommand> {
    let mut left_child = left.stdout(Stdio::piped()).spawn()?;

    #[cfg(unix)]
    {
        let left_pid = left_child.id().expect("left child has no PID");
        right.process_group(left_pid as i32);
    }

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
