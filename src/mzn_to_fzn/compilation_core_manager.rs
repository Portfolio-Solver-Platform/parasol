use tokio::sync::RwLock;

use super::compilation_manager::CompilationManager;
use crate::{
    args::RunArgs,
    is_cancelled::IsCancelled,
    logging,
    mzn_to_fzn::compilation_manager::{self, CompilationStatus, WaitForResult},
};
use std::{
    collections::{BTreeMap, HashSet},
    sync::Arc,
};

// TODO: Find better name (remember to replace file name as well)
pub struct CompilationCoreManager {
    manager: Arc<CompilationManager>,
    queue: Arc<RwLock<CompilationPriorityQueue>>,
}

impl CompilationCoreManager {
    pub fn new(args: Arc<RunArgs>, compilation_priorities: Vec<String>) -> Self {
        let queue = CompilationPriorityQueue::from_vec(compilation_priorities);
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
        // Only stop if the solver is a main compilation.
        // If it is an extra compilation, ignore the call.
        //
        // The main compilation thread should handle being cancelled, such that
        // it also stops the relevant extra compilations.
        todo!()
    }

    pub async fn stop_all_except(&self, exception_solver_ids: HashSet<String>) {
        // Only stop main compilations, ignore extra compilations when deciding
        // which compilations to stop.
        todo!()
    }

    pub async fn wait_for(&self, solver_name: &str) -> WaitForResult {
        // Should this be able to wait for extra compilations?
        // or only main compilations?
        todo!()
    }
}

#[derive(PartialEq, Eq)]
struct SolverId(String);
#[derive(PartialOrd, PartialEq, Eq, Ord, Clone)]
struct Priority(u64);

type Cores = u64;

/// Not thread-safe
struct CompilationPriorityQueue {
    compilations_queue: BTreeMap<Priority, SolverId>,
    running_compilations: BTreeMap<Priority, SolverId>,
}

impl CompilationPriorityQueue {
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

    pub fn insert(&mut self, solver: SolverId, priority: Priority) {
        self.compilations_queue.insert(priority, solver);
    }

    pub fn remove(&mut self, solver: &SolverId) -> Option<(SolverId, Priority)> {
        let priority = self
            .compilations_queue
            .iter()
            .find_map(|(priority, solver_id)| (solver == solver_id).then_some(priority.clone()));

        priority
            .map(|priority| {
                self.compilations_queue
                    .remove(&priority)
                    .map(|solver_id| (solver_id, priority))
            })
            .flatten()
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
