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
        match self.manager.status(&solver_id).await {
            // Ignore compilations that are done or running
            // TODO: What should happen if the cores are different but the solver is the same?
            CompilationStatus::Done => {
                logging::info!(
                    "did not start the compilation of solver '{solver_id}' because it was already done",
                );
                return;
            }
            CompilationStatus::NotStarted | CompilationStatus::Running => {}
        }

        let solver_id = SolverId(solver_id);
        self.queue
            .write()
            .await
            .register_main_compilation(solver_id, cores);

        let manager = Arc::clone(&self.manager);
        let queue = Arc::clone(&self.queue);
        Self::do_extra_compilations_work(manager, queue).await;

        // self.queue.write().await.take_to_start(&solver_id);
        // self.manager.start(solver_id.to_string()).await;
        //
        // let extra_compilations_count = cores - 1;
        // let extra_compilations = self
        //     .queue
        //     .write()
        //     .await
        //     .take_next_to_start(extra_compilations_count);
        // for solver_id in extra_compilations {
        //     let manager = Arc::clone(&self.manager);
        //     let queue = Arc::clone(&self.queue);
        //     tokio::spawn(async move {
        //         Self::extra_compilation(manager, queue, solver_id).await;
        //     });
        // }
        //
        // let manager = Arc::clone(&self.manager);
        // let queue = Arc::clone(&self.queue);
        // tokio::spawn(async move {
        //     Self::main_compilation(manager, queue, solver_id, extra_compilations_count).await;
        // });
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

    async fn extra_compilation(
        manager: Arc<CompilationManager>,
        queue: Arc<RwLock<CompilationPriority>>,
        solver_id: SolverId,
    ) {
        manager.start(solver_id.0.clone()).await;
        let result = manager.wait_for(&solver_id.0).await;

        if let Err(error) = result
            && error.is_cancelled()
        {
            queue.write().await.compilation_stopped(&solver_id);
            return;
        }

        // It might have failed, but we don't want to repeat it so we still mark it as done
        queue.write().await.compilation_finished(&solver_id);

        Box::pin(Self::do_extra_compilations_work(manager, queue)).await;
    }

    /// Precondition: the solver has been started in the self.queue.
    async fn main_compilation(
        manager: Arc<CompilationManager>,
        queue: Arc<RwLock<CompilationPriority>>,
        solver_id: SolverId,
    ) {
        // We are not interested in the result because we in either case
        // want to stop the extra compilations.
        let _ = manager.wait_for(&solver_id.to_string()).await;

        // and stops the extra compilations when done.
        Self::do_extra_compilations_work(manager, queue).await;
    }

    async fn do_extra_compilations_work(
        manager: Arc<CompilationManager>,
        queue: Arc<RwLock<CompilationPriority>>,
    ) {
        let work = queue.write().await.take_compilation_work();

        // Compilations should happen in a different thread
        let manager = Arc::clone(&manager);
        let queue = Arc::clone(&queue);
        tokio::spawn(async move {
            Self::extra_compilation(manager, queue, SolverId("hello".to_string())).await;
        });

        // TODO: Perform the work

        todo!()
    }
}

struct SolverId(String);
#[derive(PartialOrd, PartialEq, Eq, Ord)]
struct Priority(u64);

type Cores = u64;

enum RunningCompilation {
    Main(Cores, Option<Priority>),
    Extra(Priority),
}

/// Not thread-safe
struct CompilationPriority {
    extra_compilations_queue: BTreeMap<Priority, SolverId>,
    main_compilations_to_run: Vec<(SolverId, Cores, Option<Priority>)>,
    running_compilations: HashMap<SolverId, RunningCompilation>,
    available_cores: Cores,
}

enum ExtraCompilationWork {
    Start(SolverId),
    Stop(SolverId),
}

impl CompilationPriority {
    pub fn from_vec(solvers: Vec<String>) -> Self {
        let priorities = solvers
            .into_iter()
            .rev()
            .enumerate()
            .map(|(index, solver_id)| (Priority(index as u64), SolverId(solver_id)));

        Self {
            extra_compilations_queue: BTreeMap::from_iter(priorities),
            main_compilations_to_run: Vec::new(),
            running_compilations: Default::default(),
            available_cores: 0,
        }
    }

    pub fn register_main_compilation(&mut self, solver: SolverId, cores: Cores) {
        // Remember to check whether it is already running

        // self.available_cores += cores (_not_ cores - 1)
        todo!()
    }

    /// Assumes the work is performed after this call
    #[must_use = "the returned work has to be performed after calling this function"]
    pub fn take_compilation_work(&mut self) -> Vec<ExtraCompilationWork> {
        // Prioritise main compilations
        // If there is not enough cores for all main compilations
        todo!()
    }

    pub fn compilation_finished(&mut self, solver: &SolverId) {
        todo!()
    }

    pub fn compilation_stopped(&mut self, solver: &SolverId) {
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
