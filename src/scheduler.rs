#![allow(warnings)]

use crate::input::{Args, OutputMode};
use crate::solver_output::{Output, Solution};
use command_group::{CommandGroup, GroupChild};
use futures::future::join_all;
use kill_tree;
use std::collections::HashMap;
use std::fmt;
use std::io;
use std::io::{BufRead, BufReader};
use std::os::unix::process;
use std::process::{ChildStderr, ChildStdout};
use std::process::{Command, Stdio};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;

pub struct ScheduleElement {
    pub solver: String,
    pub cores: usize,
}

impl ScheduleElement {
    pub fn new(solver: String, cores: usize) -> Self {
        Self { solver, cores }
    }
}

pub type Schedule = Vec<ScheduleElement>;

#[derive(Debug)]
pub enum Error {
    KillTree(kill_tree::Error),
    InvalidSolver(String),
}
pub type Result<T> = std::result::Result<T, Error>;

impl From<kill_tree::Error> for Error {
    fn from(value: kill_tree::Error) -> Self {
        Error::KillTree(value)
    }
}

#[derive(Debug)]
struct Msg {}

impl Msg {
    fn new() -> Self {
        Msg {}
    }
}

pub struct Scheduler {
    tx: Sender<Msg>,
    rx: Receiver<Msg>,
    running_processes: Arc<Mutex<Vec<GroupChild>>>,
    solver_to_pid: HashMap<String, u32>,
    args: Args,
}

impl Scheduler {
    pub fn new(args: Args) -> Self {
        let (tx, rx) = mpsc::channel::<Msg>();
        let running_processes: Arc<Mutex<Vec<GroupChild>>> = cleanup_handler();

        Self {
            tx,
            rx,
            running_processes,
            solver_to_pid: HashMap::new(),
            args,
        }
    }

    pub fn listen_for_results(&self) {
        for msg in &self.rx {
            println!("{:?}", msg);
        }
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let _ = self.stop_all_solvers().await;
        });
    }

    fn start_solver(&self, elem: ScheduleElement) -> io::Result<()> {
        let mut cmd = Command::new("minizinc");
        cmd.arg("--solver").arg(elem.solver);
        cmd.arg(&self.args.model);

        if let Some(data_path) = &self.args.data {
            cmd.arg(data_path);
        }

        cmd.arg("-i");
        cmd.arg("--json-stream");
        cmd.arg("--output-mode").arg("json");
        cmd.arg("-f");
        cmd.arg("--output-objective");

        if self.args.output_objective {
            cmd.arg("--output-objective");
        }

        if self.args.ignore_search {
            cmd.arg("-f");
        }
        cmd.arg("-p").arg(elem.cores.to_string());

        let mut group_child = cmd
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .group_spawn()?;

        let stdout = group_child
            .inner()
            .stdout
            .take()
            .expect("Failed to capture stdout");

        let stderr = group_child
            .inner()
            .stderr
            .take()
            .expect("failed to capture stderr");

        let pid = group_child.id();

        {
            let mut group_childs = self.running_processes.lock().unwrap();
            group_childs.push(group_child);
        }

        let tx_clone = self.tx.clone();
        let running_processes_clone = self.running_processes.clone(); // clones the reference to the Vec
        thread::spawn(move || {
            Self::handle_solver_stdout(stdout, tx_clone, pid, running_processes_clone);
        });

        thread::spawn(move || Self::handle_solver_stderr(stderr));

        Ok(())
    }

    fn handle_solver_stdout(
        stdout: ChildStdout,
        tx: Sender<Msg>,
        pid: u32,
        running_processes: Arc<Mutex<Vec<GroupChild>>>,
    ) {
        let reader = BufReader::new(stdout);

        for line in reader.lines() {
            match line {
                Ok(l) => {
                    let _output = l;
                    let msg = Msg::new();

                    if let Err(e) = tx.send(msg) {
                        eprintln!("Could not send message, receiver dropped: {}", e);
                        break;
                    }
                }
                Err(e) => eprintln!("Error reading line: {e}"),
            }
        }

        let mut processes = running_processes.lock().unwrap();
        if let Some(index) = processes.iter().position(|child| child.id() == pid) {
            let mut child = processes.remove(index);

            if let Err(e) = child.wait() {
                eprintln!("Error waiting on child process {}: {}", pid, e);
            }
        }
    }

    fn handle_solver_stderr(stderr: ChildStderr) {
        let reader = BufReader::new(stderr);
        for line in reader.lines() {
            match line {
                Ok(l) => eprintln!("Minizinc Solver: {}", l),
                Err(e) => eprintln!("Minizinc Stderr Error: {}", e),
            }
        }
    }

    pub fn apply_schedule(&self, schedule: Schedule) -> io::Result<()> {
        for elem in schedule {
            self.start_solver(elem);
        }
        Ok(())
    }

    pub async fn stop_solver(&self, solver_name: &str) -> std::result::Result<(), Error> {
        let id = match self.solver_to_pid.get(solver_name) {
            Some(id) => id,
            None => {
                return Err(Error::InvalidSolver(format!(
                    "Solver {solver_name} was not running"
                )));
            }
        };
        let option_pid = {
            let processes = self.running_processes.lock().unwrap();
            processes.iter().find_map(|child| {
                if &child.id() == id {
                    Some(child.id())
                } else {
                    None
                }
            })
        };
        match option_pid {
            Some(pid) => {
                kill_tree::tokio::kill_tree(pid);
                // remove process and hashmap entry
                Ok(())
            }
            Nothing => Err(Error::InvalidSolver(format!(
                "Solver {solver_name} was not running"
            ))),
        }
    }

    pub async fn stop_all_solvers(&self) -> std::result::Result<(), Vec<Error>> {
        let target_pids: Vec<u32> = {
            let processes = self.running_processes.lock().unwrap();
            processes.iter().map(|child| child.id()).collect()
        };

        let kill_futures = target_pids
            .into_iter()
            .map(|id| kill_tree::tokio::kill_tree(id));

        let results = join_all(kill_futures).await;

        let errors: Vec<_> = results
            .into_iter()
            .filter_map(|res| res.err())
            .map(Error::from)
            .collect();

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }
}

pub fn cleanup_handler() -> Arc<Mutex<Vec<GroupChild>>> {
    let running_processes: Arc<Mutex<Vec<GroupChild>>> = Arc::new(Mutex::new(Vec::new()));
    let processes_for_signal = running_processes.clone();

    ctrlc::set_handler(move || {
        let pids: std::sync::MutexGuard<'_, Vec<GroupChild>> = processes_for_signal.lock().unwrap();

        for child in pids.iter() {
            // kill the minizinc solver plus all the processes it spawned (including grandchildren)
            let process_id = child.id();
            let _ = kill_tree::blocking::kill_tree(process_id);
        }

        std::process::exit(0);
    })
    .expect("Error setting Ctrl-C handler");
    return running_processes;
}
