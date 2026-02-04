#![allow(dead_code)]
use tokio::sync::RwLock;

use super::compilation_manager::CompilationManager;
use crate::{
    args::RunArgs,
    is_cancelled::IsCancelled,
    logging,
    mzn_to_fzn::compilation_manager::{CompilationStatus, WaitForResult},
};
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    sync::Arc,
};

// TODO: Find better name (remember to replace file name as well)
pub struct CompilationCoreManager {
    manager: Arc<CompilationManager>,
    state: Arc<RwLock<State>>,
}

impl CompilationCoreManager {
    pub fn new(args: Arc<RunArgs>, compilation_priorities: SolverPriority) -> Self {
        let queue = State::from_vec(compilation_priorities);
        Self {
            manager: Arc::new(CompilationManager::new(args)),
            state: Arc::new(RwLock::new(queue)),
        }
    }

    pub async fn start(&self, solver_id: String, cores: Cores) {
        let status = self.manager.status(&solver_id).await;
        if matches!(status, CompilationStatus::Done) {
            logging::info!(
                "did not start the compilation of solver '{solver_id}' because it was already done",
            );
            return;
        }

        self.state
            .write()
            .await
            .register_main_compilation(solver_id, cores);

        let manager = Arc::clone(&self.manager);
        let state = Arc::clone(&self.state);
        Self::spawn_compilation_worker_thread(manager, state);
    }

    pub async fn stop_all_except(&self, exception_solver_ids: HashSet<SolverId>) {
        let state = self.state.read().await;
        let exception_solvers = state.get_main_compilations_except(&exception_solver_ids);
        self.manager.stop_many(exception_solvers).await;
    }

    pub async fn wait_for(&self, solver_name: &str) -> WaitForResult {
        // TODO: It should only be able to wait for main compilations.
        self.manager.wait_for(solver_name).await
    }

    async fn wait_for_compile(
        manager: Arc<CompilationManager>,
        state: Arc<RwLock<State>>,
        solver_id: SolverId,
    ) {
        let result = manager.wait_for(&solver_id).await;
        if let Err(error) = result
            && error.is_cancelled()
        {
            state.write().await.compilation_stopped(&solver_id);
        } else {
            // NOTE: we don't handle the error if one occurred because the user can handle it themselves when they wait for the compilation.

            // It might have failed, but we don't want to repeat it so we still mark it as done
            state.write().await.compilation_finished(&solver_id);
        }

        Self::spawn_compilation_worker_thread(manager, state);
    }

    fn spawn_compilation_worker_thread(
        manager: Arc<CompilationManager>,
        state: Arc<RwLock<State>>,
    ) {
        tokio::spawn(async move {
            let mut state_lock = state.write().await;
            let work_list = state_lock.take_compilation_work();

            // TODO: Collect all Stop variants and use stop_many instead
            for work in work_list {
                match work {
                    CompilationWork::Start(solver_id) => {
                        manager.start(solver_id.clone()).await;
                        let manager = Arc::clone(&manager);
                        let state = Arc::clone(&state);
                        tokio::spawn(async move {
                            Self::wait_for_compile(manager, state, solver_id).await;
                        });
                    }
                    CompilationWork::Stop(solver_id) => {
                        manager.stop(solver_id).await;
                    }
                }
            }
        });
    }
}

type SolverId = String;
#[derive(PartialOrd, PartialEq, Eq, Ord)]
struct Priority(u64);

type Cores = usize;

enum RunningCompilation {
    Main(MainCompilation),
    Extra(Priority),
}

enum CompilationWork {
    Start(SolverId),
    Stop(SolverId),
}

pub struct SolverPriority(BTreeMap<Priority, SolverId>);

impl SolverPriority {
    pub fn from_descending_priority(solvers: impl IntoIterator<Item = String>) -> Self {
        let priorities = solvers
            .into_iter()
            .enumerate()
            .map(|(index, solver_id)| (Priority(index as u64), solver_id));
        Self(BTreeMap::from_iter(priorities))
    }

    fn take_next(&mut self) -> Option<(SolverId, Priority)> {
        self.0
            .pop_first()
            .map(|(priority, solver)| (solver, priority))
    }

    fn remove_by_solver_id(&mut self, id: &SolverId) -> Option<Priority> {
        todo!()
    }
}

struct MainCompilation {
    cores: Cores,
    priority: Option<Priority>,
}

/// Not thread-safe
struct State {
    unstarted_main_compilations: Vec<(SolverId, MainCompilation)>,
    extra_compilations_queue: SolverPriority,
    running_compilations: HashMap<SolverId, RunningCompilation>,
    available_cores: Cores,
    used_cores: Cores,
}

impl State {
    pub fn from_vec(priority: SolverPriority) -> Self {
        Self {
            extra_compilations_queue: priority,
            unstarted_main_compilations: Vec::new(),
            running_compilations: Default::default(),
            available_cores: 0,
            used_cores: 0,
        }
    }

    pub fn register_main_compilation(&mut self, solver: SolverId, cores: Cores) {
        let running_compilation = self.running_compilations.remove(&solver);

        match running_compilation {
            None => {
                let optional_priority = self.extra_compilations_queue.remove_by_solver_id(&solver);
                self.available_cores += cores;
                self.unstarted_main_compilations
                    .push((solver, MainCompilation::new(cores, optional_priority)));
            }
            Some(RunningCompilation::Main(old_compilation)) => {
                let core_change = (cores as isize) - (old_compilation.cores as isize);
                self.available_cores = self.available_cores.saturating_add_signed(core_change);
                self.running_compilations.insert(
                    solver,
                    RunningCompilation::Main(MainCompilation::new(cores, old_compilation.priority)),
                );
            }
            Some(RunningCompilation::Extra(priority)) => {
                self.available_cores += cores;
                self.running_compilations.insert(
                    solver,
                    RunningCompilation::Main(MainCompilation::new(cores, Some(priority))),
                );
            }
        }
    }

    /// Assumes the work is performed after this call
    #[must_use = "the returned work has to be performed after calling this function"]
    pub fn take_compilation_work(&mut self) -> impl Iterator<Item = CompilationWork> + use<> {
        // Prioritise main compilations.
        // If there is not enough cores for all main compilations, stop extra compilations.
        todo!();
        Vec::new().into_iter()
    }

    pub fn compilation_finished(&mut self, solver: &SolverId) {
        // Remove from running solvers.
        // If a main compilation, remove cores from available cores.
        // Remove 1 core from used_cores.
        todo!()
    }

    pub fn compilation_stopped(&mut self, solver: &SolverId) {
        // Remove from running solvers.
        // If a main compilation, remove cores from available cores.
        // If an extra compilation, add to queue again.
        // Remove 1 core from used_cores.
        todo!()
    }

    pub fn get_main_compilations_except(
        &self,
        exception_solvers: &HashSet<SolverId>,
    ) -> HashSet<SolverId> {
        todo!()
    }
}

impl MainCompilation {
    pub const fn new(cores: Cores, priority: Option<Priority>) -> Self {
        Self { cores, priority }
    }
}
