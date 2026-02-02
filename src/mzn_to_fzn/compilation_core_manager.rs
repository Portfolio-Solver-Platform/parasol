use tokio::sync::RwLock;

use super::compilation_manager::CompilationManager;
use crate::{
    args::RunArgs,
    is_cancelled::IsCancelled,
    logging,
    mzn_to_fzn::{
        compilation,
        compilation_manager::{self, CompilationStatus, WaitForResult},
    },
};
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    sync::Arc,
};

// TODO: Find better name (remember to replace file name as well)
pub struct CompilationCoreManager {
    manager: Arc<CompilationManager>,
    queue: Arc<RwLock<CompilationPriority>>,
}

// New way of keeping track:
//
// General idea is to keep track of available extra compilations in CompilationPriority.
//
// register_main_compilation(solver, cores)
// take_next_extra_compilation() -> solver
// Whenever an extra compilation or main compilation is done, it should
// run a common function that keeps calling take_next_extra_compilation() and starting it.
// This common function should perhaps also be called right after registering a main compilation.
// This is because register_main_compilation will let CompilationPriority register
// that new extra compilations can be performed.
//
// take_next_extra_compilation() should register the extra compilation as being run,
// such that subsequent executions of the function doesn't return the same compilation.
// There should also be a "compilation_stopped(solver)" that allows you to register
// that a compilation stopped, and there should be "compilation_finished(solver)" to
// register that a compilation finished. Note that "stopped" here means it stopped
// before being finished.
//
// Note that when registering main compilation, it should check whether it is
// in the queue of upcoming compilations, as well as checking if it is already
// being compiled as an extra compilation.
// If the compilation is already registered as a main compilation, then also
// make sure that the cores are the same. If they are not, and the new cores are smaller,
// then stop as many extra compilations as the difference, or if there now are more cores,
// then instead start more extra compilations.
//
// Important things to handle:
// - The same main compilation being started multiple times, and with different cores.
// - Main compilation being started, but it is already running as an extra compilation.

//
//
// General procedure:
//
// Create a CompilationPriority struct that manages which compilation
// should be done next. The next compilations should be stored in a btree.
//
// Start the solver through the self.manager.
// If cores > 1, then start cores - 1 compilations and register these
// in the CompilationPriority struct.
// Then, start threads that wait for the extra compilations.
//  - In these threads, when it is done, it should start a new one
//    by registering the compilation as done in the CompilationPriority
//    and then starting a new compilation.
// Then, wait for the compilation.
// Then, stop cores - 1 compilations.
//
// TODO: Handle main compilations should not be able to be stopped.
//       Currently, they can be stopped when another main compilation is finished, and it has low
//       priority.
//       - Should be handled by registering it as a main compilation.
impl CompilationCoreManager {
    pub fn new(args: Arc<RunArgs>, compilation_priorities: Vec<String>) -> Self {
        let queue = CompilationPriority::from_vec(compilation_priorities);
        Self {
            manager: Arc::new(CompilationManager::new(args)),
            queue: Arc::new(RwLock::new(queue)),
        }
    }

    pub async fn start(&self, solver_id: String, cores: Cores) {
        // TODO: Lock on a `state` (maybe just the queue) for the entire function to avoid race conditions
        match self.manager.status(&solver_id).await {
            CompilationStatus::Done => {
                logging::info!(
                    "did not start the compilation of solver '{solver_id}' because it was already done",
                );
                return;
            }
            CompilationStatus::Running => {
                // TODO: Handle a possible difference in the cores if it was a main compilation.
                //       Also, handle if it was an extra compilation.
            }
            CompilationStatus::NotStarted => {}
        }

        // TODO: Remove the solver_id from the queue, so if it was already in the queue,
        //       it will not be attempted to be started later as an extra compilation.

        self.manager.start(solver_id.clone()).await;

        let extra_compilations = self.queue.write().await.take_next_to_start(cores - 1);
        let extra_compilations_count = extra_compilations.len();
        self.manager
            .start_many(extra_compilations.iter().map(|id| id.to_string()))
            .await;

        let manager = Arc::clone(&self.manager);
        let queue = Arc::clone(&self.queue);
        tokio::spawn(async move {
            // Wait for main compilation and stop as many solvers as before
            let _ = manager.wait_for(&solver_id).await;
            let to_stop = queue
                .write()
                .await
                .take_next_to_stop(extra_compilations_count as u64);

            manager
                .stop_many(to_stop.iter().map(|id| id.to_string()))
                .await;
        });

        for solver_id in extra_compilations {
            let manager = Arc::clone(&self.manager);
            let queue = Arc::clone(&self.queue);
            tokio::spawn(async move {
                let mut solver_id_string = solver_id.to_string();
                loop {
                    let result = manager.wait_for(&solver_id_string).await;

                    let mut queue = queue.write().await;

                    if let Err(error) = result
                        && (error.is_cancelled()
                            || !matches!(error, compilation_manager::WaitForError::Conversion))
                    {
                        queue.compilation_stopped(&solver_id);
                        break;
                    }

                    // It might have failed, but we don't want to repeat it so we still mark it as done
                    queue.compilation_finished(&solver_id);

                    match queue.take_next_to_start(1).as_slice() {
                        [id] => solver_id_string = id.to_string(),
                        [] => break,
                        _ => {
                            logging::error_msg!("queue returned more than 1");
                            break;
                        }
                    }
                }
            });
        }
    }

    pub fn stop(&self, solver_id: &str) {
        todo!()
    }

    pub async fn stop_all_except(&self, exception_solver_ids: HashSet<String>) {
        todo!()
    }

    pub async fn wait_for(&self, solver_name: &str) -> WaitForResult {
        todo!()
    }
}

struct SolverId(String);
#[derive(PartialOrd, PartialEq, Eq, Ord)]
struct Priority(u64);

type Cores = u64;

/// Not thread-safe
struct CompilationPriority {
    compilations_queue: BTreeMap<Priority, SolverId>,
    running_compilations: BTreeMap<Priority, SolverId>,
}

impl CompilationPriority {
    pub fn from_vec(solvers: Vec<String>) -> Self {
        let priorities = solvers
            .into_iter()
            .rev()
            .enumerate()
            .map(|(index, solver_id)| (Priority(index as u64), SolverId(solver_id)));

        Self {
            compilations_queue: BTreeMap::from_iter(priorities),
            running_compilations: BTreeMap::new(),
        }
    }

    #[must_use = "the returned solvers needs to be started"]
    pub fn take_next_to_start(&mut self, amount: u64) -> Vec<SolverId> {
        // Pop from compilations_queue and push to running_compilations
        todo!()
    }

    #[must_use = "the returned solvers needs to be stopped"]
    pub fn take_next_to_stop(&mut self, amount: u64) -> Vec<SolverId> {
        // Pop from running_compilations and push to compilations_queue
        // (maybe this is the same as calling self.compilation_stopped though)
        todo!()
    }

    pub fn compilation_finished(&mut self, solver: &SolverId) {
        // Find in running_compilations and remove
        todo!()
    }

    pub fn compilation_stopped(&mut self, solver: &SolverId) {
        // Pop from running_compilations and push to compilations_queue
        todo!()
    }
}

impl ToString for SolverId {
    fn to_string(&self) -> String {
        self.0.clone()
    }
}

impl SolverId {
    pub fn into_string(self) -> String {
        self.0
    }
}
