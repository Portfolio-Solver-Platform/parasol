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

        // TODO: Start main compilation here myself to make sure
        //       it has started before exiting this function call.
        //       Also call wait_for_compile to wait for it (which will call
        //       compilation_stopped and compilation_finished).
        //       Also, modify register_main_compilation to assume it has already
        //       been started. This means that unstarted_main_compilations
        //       probably should be removed.

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
        if self.state.read().await.is_main_compilation(solver_name) {
            self.manager.wait_for(solver_name).await
        } else {
            Err(super::compilation_manager::WaitForError::NotStarted(
                solver_name.to_string(),
            ))
        }
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
#[derive(PartialOrd, PartialEq, Eq, Ord, Clone)]
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
    pub fn empty() -> Self {
        Self(BTreeMap::new())
    }

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
        let priority = self
            .0
            .iter()
            .find_map(|(priority, solver_id)| (solver_id == id).then_some(priority))
            .cloned();

        if let Some(p) = &priority {
            self.0.remove(p);
        }
        priority
    }

    fn insert(&mut self, solver_id: SolverId, priority: Priority) {
        self.0.insert(priority, solver_id);
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

    pub fn is_main_compilation(&self, solver: &str) -> bool {
        if let Some(RunningCompilation::Main(_)) = self.running_compilations.get(solver) {
            return true;
        }

        if self
            .unstarted_main_compilations
            .iter()
            .any(|(id, _)| id == solver)
        {
            return true;
        }

        false
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
    pub fn take_compilation_work(&mut self) -> Vec<CompilationWork> {
        let mut work = Vec::new();
        // Prioritise main compilations.
        self.unstarted_main_compilations
            .drain(..)
            .for_each(|(solver, compilation)| {
                work.push(CompilationWork::Start(solver.clone()));
                self.running_compilations
                    .insert(solver, RunningCompilation::Main(compilation));

                self.used_cores += 1;
            });

        while self.used_cores > self.available_cores {
            let candidate_to_stop = self
                .running_compilations
                .iter()
                .filter_map(|(id, run)| match run {
                    RunningCompilation::Extra(priority) => Some((id.clone(), priority.clone())),
                    _ => None,
                })
                .max_by_key(|(_, priority)| priority.clone());

            if let Some((solver_id, priority)) = candidate_to_stop {
                logging::info!("stopping extra compilation for solver '{solver_id}'");
                self.running_compilations.remove(&solver_id);
                self.used_cores -= 1;

                // Re-queue it so it can run later when cores are free
                self.extra_compilations_queue
                    .insert(solver_id.clone(), priority);

                work.push(CompilationWork::Stop(solver_id));
            } else {
                // We cannot stop Main compilations, so we break.
                logging::error_msg!(
                    "no extra compilations left to stop, but still over core budget"
                );
                break;
            }
        }

        while self.used_cores < self.available_cores {
            if let Some((solver_id, priority)) = self.extra_compilations_queue.take_next() {
                logging::info!("starting extra compilation for solver '{solver_id}'");
                self.running_compilations.insert(
                    solver_id.clone(),
                    RunningCompilation::Extra(Priority(priority.0)),
                );
                self.used_cores += 1;
                work.push(CompilationWork::Start(solver_id));
            } else {
                break;
            }
        }

        work
    }

    pub fn compilation_finished(&mut self, solver: &SolverId) {
        if let Some(compilation) = self.running_compilations.remove(solver) {
            self.used_cores -= 1;

            if let RunningCompilation::Main(compilation) = compilation {
                self.available_cores -= compilation.cores;
            }
        }
    }

    pub fn compilation_stopped(&mut self, solver: &SolverId) {
        if let Some(compilation) = self.running_compilations.remove(solver) {
            self.used_cores -= 1;

            let priority = match compilation {
                RunningCompilation::Main(compilation) => {
                    self.available_cores -= compilation.cores;
                    compilation.priority
                }
                RunningCompilation::Extra(priority) => Some(priority),
            };

            if let Some(priority) = priority {
                self.extra_compilations_queue
                    .insert(solver.clone(), priority);
            }
        }
    }

    pub fn get_main_compilations_except(
        &self,
        exception_solvers: &HashSet<SolverId>,
    ) -> HashSet<SolverId> {
        let mut result = HashSet::new();

        for (id, run) in &self.running_compilations {
            if matches!(run, RunningCompilation::Main(_)) && !exception_solvers.contains(id) {
                result.insert(id.clone());
            }
        }

        for (id, _) in &self.unstarted_main_compilations {
            if !exception_solvers.contains(id) {
                result.insert(id.clone());
            }
        }

        result
    }
}

impl MainCompilation {
    pub const fn new(cores: Cores, priority: Option<Priority>) -> Self {
        Self { cores, priority }
    }
}
