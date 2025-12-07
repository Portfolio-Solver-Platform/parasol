use crate::input::Args;
use crate::model_parser::{ObjectiveType, parse_objective_type};
use crate::solver_output::{Output, OutputParseError, Solution, Status};
use futures::future::join_all;
use nix::sys::signal::{self, Signal};
use nix::unistd::Pid;
use std::collections::HashMap;
use std::io::ErrorKind;
#[cfg(unix)]
use std::os::unix::process::CommandExt;
use std::process::Stdio;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::Mutex;
use tokio::sync::mpsc;

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

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::KillTree(e) => write!(f, "failed to kill process tree: {}", e),
            Error::InvalidSolver(msg) => write!(f, "invalid solver: {}", msg),
            Error::Io(e) => write!(f, "IO error: {}", e),
            Error::OutputParseError(e) => write!(f, "output parse error: {:?}", e),
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
    pub fn new(args: Args) -> Self {
        let objective_type =
            parse_objective_type(&args.model).expect("Failed to parse objective type from model");
        let (tx, rx) = mpsc::unbounded_channel::<Msg>();
        let solver_to_pid = Arc::new(Mutex::new(HashMap::new()));

        cleanup_handler(solver_to_pid.clone());
        let solver_to_pid_clone = solver_to_pid.clone();

        tokio::spawn(async move { Self::receiver(rx, solver_to_pid_clone, objective_type).await });

        Self {
            tx,
            solver_to_pid,
            args,
        }
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
        cmd.process_group(0); // let os give it a process id

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
        tokio::spawn(async move {
            if let Err(e) = Self::handle_solver_stdout(stdout, tx_clone).await {
                eprintln!("Error handling solver stdout: {:?}", e);
            }
        });

        tokio::spawn(async move { Self::handle_solver_stderr(stderr).await });

        let solver_to_pid_clone = self.solver_to_pid.clone();

        tokio::spawn(async move {
            let _ = child.wait().await;
            let mut map = solver_to_pid_clone.lock().await;
            map.remove(&elem.id);
        });

        Ok(())
    }

    async fn handle_solver_stdout(
        stdout: tokio::process::ChildStdout,
        tx: tokio::sync::mpsc::UnboundedSender<Msg>,
    ) -> std::result::Result<(), Error> {
        let reader = BufReader::new(stdout);
        let mut lines = reader.lines();

        while let Ok(Some(line)) = lines.next_line().await {
            let output = Output::parse(&line)?;
            let msg = match output {
                Output::Ignore => continue,
                Output::Solution(solution) => Msg::Solution(solution),
                Output::Status(status) => Msg::Status(status),
            };

            if let Err(e) = tx.send(msg) {
                eprintln!("Could not send message, receiver dropped: {}", e);
                break;
            }
        }
        Ok(())
    }

    async fn handle_solver_stderr(stderr: tokio::process::ChildStderr) {
        let reader = BufReader::new(stderr);
        let mut lines = reader.lines();

        while let Ok(Some(line)) = lines.next_line().await {
            // maybe problem we only handle if the error is okay
            eprintln!("Minizinc Solver: {}", line);
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

    async fn _stop_solver(
        solver_to_pid: Arc<Mutex<HashMap<usize, u32>>>,
        id: usize,
    ) -> std::result::Result<(), Error> {
        let pid = {
            let map = solver_to_pid.lock().await;
            map.get(&id).copied()
        };

        if let Some(pid) = pid {
            if let Err(e) = kill_tree::tokio::kill_tree(pid).await {
                let is_zombie = match &e {
                    kill_tree::Error::Io(io_err) => io_err.kind() == ErrorKind::NotFound,
                    kill_tree::Error::InvalidProcessId { .. } => true,
                    _ => false,
                };
                if !is_zombie {
                    return Err(Error::KillTree(e));
                }
            }

            let mut map = solver_to_pid.lock().await;
            map.remove(&id);
        }

        Ok(())
    }

    pub async fn stop_solver(&self, id: usize) -> std::result::Result<(), Error> {
        Self::_stop_solver(self.solver_to_pid.clone(), id).await
    }

    async fn _stop_solvers(
        solver_to_pid: Arc<Mutex<HashMap<usize, u32>>>,
        ids: Vec<usize>,
    ) -> std::result::Result<(), Vec<Error>> {
        let kill_futures = ids
            .iter()
            .map(|id| Self::_stop_solver(solver_to_pid.clone(), *id));

        let results = join_all(kill_futures).await;

        let errors: Vec<Error> = results
            .into_iter()
            .filter_map(|result| result.err())
            .collect();

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    pub async fn stop_solvers(&self, ids: Vec<usize>) -> std::result::Result<(), Vec<Error>> {
        Self::_stop_solvers(self.solver_to_pid.clone(), ids).await
    }

    async fn _stop_all_solvers(
        solver_to_pid: Arc<Mutex<HashMap<usize, u32>>>,
    ) -> std::result::Result<(), Vec<Error>> {
        let ids = {
            let map = solver_to_pid.lock().await;
            map.keys().copied().collect()
        };
        Self::_stop_solvers(solver_to_pid.clone(), ids).await
    }

    pub async fn stop_all_solvers(&self) -> std::result::Result<(), Vec<Error>> {
        Self::_stop_all_solvers(self.solver_to_pid.clone()).await
    }

    pub async fn pause_solver(&self, id: usize) -> std::result::Result<(), Error> {
        self.send_signal(id, Signal::SIGSTOP).await
    }

    pub async fn resume_solver(&self, id: usize) -> std::result::Result<(), Error> {
        self.send_signal(id, Signal::SIGCONT).await
    }

    async fn send_signal(&self, id: usize, signal: Signal) -> std::result::Result<(), Error> {
        let pid = {
            let map = self.solver_to_pid.lock().await;
            match map.get(&id) {
                Some(&p) => p,
                None => return Err(Error::InvalidSolver(format!("Solver {id} not running"))),
            }
        };

        let pgid = Pid::from_raw(-(pid as i32));
        match signal::kill(pgid, signal) {
            Ok(_) => Ok(()),
            Err(errno) => {
                // Nix wraps errno into a proper Rust error
                Err(Error::Io(std::io::Error::from_raw_os_error(errno as i32)))
            }
        }
    }

    pub async fn suspend_all_solvers(&self) -> std::result::Result<(), Vec<Error>> {
        let ids: Vec<usize> = { self.solver_to_pid.lock().await.keys().cloned().collect() };

        let futures = ids.iter().map(|id| self.pause_solver(*id));
        let results = join_all(futures).await;

        let errors: Vec<Error> = results.into_iter().filter_map(|res| res.err()).collect();

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
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
