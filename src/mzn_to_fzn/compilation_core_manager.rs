use std::sync::Arc;

use crate::args::RunArgs;

use super::compilation_manager::CompilationManager;

// TODO: Find better name
pub struct CompilationCoreManager {
    manager: CompilationManager,
}

impl CompilationCoreManager {
    pub fn new(args: Arc<RunArgs>) -> Self {
        Self {
            manager: CompilationManager::new(args),
        }
    }

    pub fn start(&self, solver_name: String, cores: u64) {
        // Start the solver through the self.manager.
        // If cores > 1, then register that cores - 1 are available.
        // Then, wait_for the compilation for the original solver.
        // When done, register that cores - 1 are not available anymore.
        // This should stop extra compilations.
        //
        // The extra compilations should be done in a prioritised manner,
        // i.e., when starting new compilations, take the ones with highest
        // priority (that have not yet been started or is done),
        // and when stopping compilations, stop the once with lowest priority.
        todo!()
    }

    pub fn stop(&self, solver_name: String) {
        todo!()
    }
}
