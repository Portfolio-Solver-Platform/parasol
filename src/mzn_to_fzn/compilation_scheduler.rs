use itertools::{Either, Itertools};
use tokio::sync::RwLock;
use tokio_util::sync::CancellationToken;

use super::compilation_executor::CompilationExecutor;
use crate::{
    args::RunArgs,
    is_cancelled::IsErrorCancelled,
    logging,
    mzn_to_fzn::compilation_executor::{CompilationStatus, WaitForResult},
};
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    sync::Arc,
};

pub struct CompilationScheduler {
    executor: Arc<CompilationExecutor>,
    state: Arc<RwLock<State>>,
    cancellation_token: CancellationToken,
}

impl CompilationScheduler {
    pub fn new(args: Arc<RunArgs>, compilation_priorities: SolverPriority) -> Self {
        let queue = State::from_vec(compilation_priorities);
        let cancellation_token = CancellationToken::new();
        Self {
            executor: Arc::new(CompilationExecutor::new(args, cancellation_token.child_token())),
            state: Arc::new(RwLock::new(queue)),
            cancellation_token
        }
    }

    pub async fn start(&self, solver_id: String, cores: Cores) {
        let status = self.executor.status(&solver_id).await;
        if matches!(status, CompilationStatus::Done) {
            logging::info!(
                "did not start the compilation of solver '{solver_id}' because it was already done",
            );
            return;
        }

        let mut state = self.state.write().await;

        self.executor.start(solver_id.clone()).await;

        state.register_main_compilation(solver_id.clone(), cores);

        let executor_clone = Arc::clone(&self.executor);
        let state_clone = Arc::clone(&self.state);
        tokio::spawn(async move {
            Self::wait_for_compilation(executor_clone, state_clone, &solver_id).await;
        });

        let executor_clone = Arc::clone(&self.executor);
        let state_clone = Arc::clone(&self.state);
        Self::spawn_compilation_worker_thread(executor_clone, state_clone);
    }

    pub async fn stop_all_except(&self, exception_solver_ids: HashSet<SolverId>) {
        let state = self.state.read().await;
        let exception_solvers = state.get_main_compilations_except(&exception_solver_ids);
        self.executor.stop_many(exception_solvers).await;
    }

    /// Cancellation safe
    pub async fn wait_for(&self, solver_name: &str) -> WaitForResult {
        self.executor.wait_for(solver_name).await
    }

    pub async fn disable_extra_compilations(&self) {
        self.state.write().await.disable();

        let executor = Arc::clone(&self.executor);
        let state = Arc::clone(&self.state);
        Self::spawn_compilation_worker_thread(executor, state);
    }

    async fn wait_for_compilation(
        executor: Arc<CompilationExecutor>,
        state: Arc<RwLock<State>>,
        solver_id: &SolverId,
    ) {
        let result = executor.wait_for(solver_id).await;
        if result.is_error_cancelled() {
            state.write().await.compilation_stopped(solver_id);
        } else {
            // NOTE: we don't handle the error if one occurred because the user can handle it themselves when they wait for the compilation.

            // It might have failed, but we don't want to repeat it so we still mark it as done
            state.write().await.compilation_finished(solver_id);
            Self::spawn_compilation_worker_thread(executor, state);
        }
    }

    fn spawn_compilation_worker_thread(
        executor: Arc<CompilationExecutor>,
        state: Arc<RwLock<State>>,
    ) {
        tokio::spawn(async move {
            let mut state_lock = state.write().await;
            let work_list = state_lock.take_compilation_work();

            let (to_start, to_stop): (Vec<_>, Vec<_>) =
                work_list.into_iter().partition_map(|work| match work {
                    CompilationWork::Start(solver_id) => Either::Left(solver_id),
                    CompilationWork::Stop(solver_id) => Either::Right(solver_id),
                });

            executor.stop_many(to_stop).await;

            executor.start_many(to_start.clone()).await;
            for solver_id in to_start {
                let executor = Arc::clone(&executor);
                let state = Arc::clone(&state);
                tokio::spawn(async move {
                    Self::wait_for_compilation(executor, state, &solver_id).await;
                });
            }
        });
    }
}

impl Drop for CompilationScheduler {
    fn drop(&mut self) {
        self.cancellation_token.cancel()
    }
}

type SolverId = String;
#[derive(Debug, PartialOrd, PartialEq, Eq, Ord, Clone)]
struct Priority(u64);

type Cores = usize;

#[derive(Debug)]
enum RunningCompilation {
    Main(MainCompilation),
    Extra(Priority),
}

#[derive(Debug)]
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

#[derive(Debug)]
struct MainCompilation {
    cores: Cores,
    priority: Option<Priority>,
}

/// Not thread-safe
struct State {
    extra_compilations_queue: SolverPriority,
    running_compilations: HashMap<SolverId, RunningCompilation>,
    available_cores: Cores,
    used_cores: Cores,
    is_enabled: bool,
}

impl State {
    pub fn from_vec(priority: SolverPriority) -> Self {
        Self {
            extra_compilations_queue: priority,
            running_compilations: Default::default(),
            available_cores: 0,
            used_cores: 0,
            is_enabled: true,
        }
    }

    /// Precondition: The solver should be started.
    pub fn register_main_compilation(&mut self, solver: SolverId, cores: Cores) {
        logging::info!("registering main compilation for solver '{solver}' with cores '{cores}'");
        self.used_cores += 1;
        self.available_cores += cores;

        let running_compilation = self.running_compilations.remove(&solver);
        let priority = match running_compilation {
            None => self.extra_compilations_queue.remove_by_solver_id(&solver),
            Some(RunningCompilation::Extra(priority)) => Some(priority),
            Some(RunningCompilation::Main(old_compilation)) => {
                self.available_cores = self.available_cores.saturating_sub(old_compilation.cores);
                old_compilation.priority
            }
        };

        self.running_compilations.insert(
            solver,
            RunningCompilation::Main(MainCompilation::new(cores, priority)),
        );
    }

    /// Assumes the work is performed after this call
    #[must_use = "the returned work has to be performed after calling this function"]
    pub fn take_compilation_work(&mut self) -> Vec<CompilationWork> {
        logging::info!(
            "deciding on extra compilations based on used cores ({}) and available cores ({}){}",
            self.used_cores,
            self.available_cores,
            if self.is_enabled {
                ""
            } else {
                " (extra compilations are disabled, so if there are any, it will stop them all)"
            }
        );

        let mut work = Vec::new();

        while !self.is_enabled || self.used_cores > self.available_cores {
            let candidate_to_stop = self
                .running_compilations
                .iter()
                .filter_map(|(id, run)| match run {
                    RunningCompilation::Extra(priority) => Some((id.clone(), priority.clone())),
                    _ => None,
                })
                .max_by_key(|(_, priority)| priority.clone());

            if let Some((solver_id, priority)) = candidate_to_stop {
                logging::info!("decided to stop extra compilation for solver '{solver_id}'");
                self.running_compilations.remove(&solver_id);
                self.used_cores -= 1;

                // Re-queue it so it can run later when cores are free
                self.extra_compilations_queue
                    .insert(solver_id.clone(), priority);

                work.push(CompilationWork::Stop(solver_id));
            } else {
                // We cannot stop Main compilations, so we break.
                if self.is_enabled {
                    logging::error_msg!(
                        "no extra compilations left to stop, but still over core budget"
                    );
                }
                break;
            }
        }

        while self.is_enabled && self.used_cores < self.available_cores {
            if let Some((solver_id, priority)) = self.extra_compilations_queue.take_next() {
                logging::info!("decided to start extra compilation for solver '{solver_id}'");
                self.running_compilations
                    .insert(solver_id.clone(), RunningCompilation::Extra(priority));
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

        result
    }

    pub fn disable(&mut self) {
        self.is_enabled = false;
        logging::info!("disabled extra compilations");
    }
}

impl MainCompilation {
    pub const fn new(cores: Cores, priority: Option<Priority>) -> Self {
        Self { cores, priority }
    }
}
