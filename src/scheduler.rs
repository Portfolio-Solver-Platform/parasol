use crate::{
    args::{Args, DebugVerbosityLevel},
    config::Config,
    solver_manager::{Error, SolverManager},
};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;
use sysinfo::System;
use tokio::sync::Mutex;

#[derive(Clone, Debug)]
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

struct SolverInfo {
    name: String,
    cores: usize,
}

struct ScheduleChanges {
    to_start: Schedule,
    to_suspend: Vec<usize>,
    to_resume: Vec<usize>,
}

struct MemoryEnforcerState {
    running_solvers: HashMap<usize, SolverInfo>,
    suspended_solvers: HashMap<usize, SolverInfo>,
    system: System,
    memory_threshold: f64,
    memory_limit: u64, // In bytes (0 = use system total)
    id: usize,
}

pub struct Scheduler {
    state: Arc<Mutex<MemoryEnforcerState>>,
    pub solver_manager: Arc<SolverManager>,
}

fn is_over_threshold(used: f64, total: f64, threshold: f64) -> bool {
    used / total > threshold
}

impl Scheduler {
    pub async fn new(args: &Args, config: &Config) -> std::result::Result<Self, Error> {
        let solver_manager = Arc::new(SolverManager::new(args.clone()).await?);

        let memory_limit = std::env::var("MEMORY_LIMIT")
            .ok()
            .and_then(|val| val.parse::<u64>().ok())
            .map(|mib| mib * 1024 * 1024)
            .unwrap_or(0);

        let state = Arc::new(Mutex::new(MemoryEnforcerState {
            running_solvers: HashMap::new(),
            suspended_solvers: HashMap::new(),
            system: System::new_all(),
            memory_threshold: config.memory_threshold,
            memory_limit,
            id: 0,
        }));

        let state_clone = state.clone();
        let solver_manager_clone = solver_manager.clone();
        let debug_verbosity = args.debug_verbosity;
        let memory_enforcer_interval = config.memory_enforcer_interval;
        tokio::spawn(async move {
            Self::memory_enforcer_loop(
                state_clone,
                solver_manager_clone,
                debug_verbosity,
                memory_enforcer_interval,
            )
            .await;
        });

        Ok(Self {
            state,
            solver_manager,
        })
    }

    fn get_memory_usage(
        state: &mut MemoryEnforcerState,
        debug_verbosity: DebugVerbosityLevel,
    ) -> (f64, f64) {
        state
            .system
            .refresh_processes(sysinfo::ProcessesToUpdate::All, false);
        state.system.refresh_memory();

        let used = state.system.used_memory() as f64;
        let total = if state.memory_limit > 0 {
            state.memory_limit as f64
        } else {
            state.system.total_memory() as f64
        };
        if debug_verbosity >= DebugVerbosityLevel::Info {
            let div = (1024 * 1024) as f64;

            println!(
                "Info: Memory used by system: {} MiB, Memory Available: {} MiB, Memory threshold: {}",
                used / div,
                total / div,
                total * state.memory_threshold / div,
            );
        }
        (used, total)
    }

    async fn kill_suspended_until_under_threshold(
        state: &mut MemoryEnforcerState,
        solver_manager: &Arc<SolverManager>,
        mut used_memory: f64,
        total_memory: f64,
        debug_verbosity: DebugVerbosityLevel,
    ) -> f64 {
        let ids: Vec<usize> = state.suspended_solvers.keys().copied().collect();
        let mut sorted = solver_manager
            .solvers_sorted_by_mem(&ids, &state.system)
            .await;

        while !sorted.is_empty()
            && is_over_threshold(used_memory, total_memory, state.memory_threshold)
        {
            let (mem, id) = sorted.remove(0);
            state.suspended_solvers.remove(&id);
            if let Err(e) = solver_manager.stop_solver(id).await {
                if debug_verbosity >= DebugVerbosityLevel::Error {
                    eprintln!("failed to stop suspended solver: {e}");
                }
            } else {
                used_memory -= mem as f64;
            }
        }
        used_memory
    }

    async fn kill_running_until_under_threshold(
        state: &mut MemoryEnforcerState,
        solver_manager: &Arc<SolverManager>,
        mut used_memory: f64,
        total_memory: f64,
        debug_verbosity: DebugVerbosityLevel,
    ) -> f64 {
        let ids: Vec<usize> = state.running_solvers.keys().copied().collect();
        let total_cores: usize = state
            .running_solvers
            .iter()
            .map(|(_, info)| info.cores)
            .sum();
        if total_cores == 0 {
            return used_memory;
        }

        let sorted = solver_manager
            .solvers_sorted_by_mem(&ids, &state.system)
            .await;
        let per_core_threshold =
            (total_memory / total_cores as f64 * state.memory_threshold) as u64;

        let mut remaining = Vec::new();

        for (solver_mem, id) in sorted {
            let cores = match state.running_solvers.get(&id) {
                Some(info) => info.cores as u64,
                None => {
                    // should never fail since the state is locked however error logging just for safety
                    if debug_verbosity >= DebugVerbosityLevel::Error {
                        eprintln!(
                            "Failed to get solver info. Cause of this error is probably from a logic error in the code"
                        );
                    }
                    continue;
                }
            };
            if solver_mem / cores > per_core_threshold {
                // use number of cores a process has to decide if it uses more that its fair share
                state.running_solvers.remove(&id);
                if let Err(e) = solver_manager.stop_solver(id).await {
                    if debug_verbosity >= DebugVerbosityLevel::Error {
                        eprintln!("failed to stop running solver: {e}");
                    }
                } else {
                    used_memory -= solver_mem as f64;
                }
            } else {
                remaining.push((solver_mem, id));
            }
        }
        while !remaining.is_empty()
            && is_over_threshold(used_memory, total_memory, state.memory_threshold)
        {
            let (mem, id) = remaining.remove(0);
            state.running_solvers.remove(&id);
            if let Err(e) = solver_manager.stop_solver(id).await {
                if debug_verbosity >= DebugVerbosityLevel::Error {
                    eprintln!("failed to stop running solver: {e}");
                }
            } else {
                used_memory -= mem as f64;
            }
        }
        used_memory
    }

    async fn remove_exited_solvers(
        state: &mut MemoryEnforcerState,
        solver_manager: &Arc<SolverManager>,
    ) {
        let active = solver_manager.active_solver_ids().await;
        state.running_solvers.retain(|id, _| active.contains(id));
        state.suspended_solvers.retain(|id, _| active.contains(id));
    }

    async fn memory_enforcer_loop(
        state: Arc<Mutex<MemoryEnforcerState>>,
        solver_manager: Arc<SolverManager>,
        debug_verbosity: DebugVerbosityLevel,
        memory_enforcer_interval: u64,
    ) {
        let mut interval = tokio::time::interval(Duration::from_secs(memory_enforcer_interval));

        loop {
            interval.tick().await;
            let mut state = state.lock().await;

            Self::remove_exited_solvers(&mut state, &solver_manager).await;

            let (used, total) = Self::get_memory_usage(&mut state, debug_verbosity);
            if !is_over_threshold(used, total, state.memory_threshold) {
                continue;
            }
            let used = Self::kill_suspended_until_under_threshold(
                &mut state,
                &solver_manager,
                used,
                total,
                debug_verbosity,
            )
            .await;

            if is_over_threshold(used, total, state.memory_threshold) {
                Self::kill_running_until_under_threshold(
                    &mut state,
                    &solver_manager,
                    used,
                    total,
                    debug_verbosity,
                )
                .await;
            }
        }
    }

    async fn categorize_schedule(
        schedule: Schedule,
        state: &mut MemoryEnforcerState,
        solver_manager: Arc<SolverManager>,
    ) -> ScheduleChanges {
        Self::remove_exited_solvers(state, &solver_manager).await;

        let mut to_start = Vec::new();
        let mut to_resume = Vec::new();
        let mut keep_running = Vec::new();
        let mut running: HashSet<_> = state.running_solvers.keys().copied().collect();
        let mut suspended: HashSet<_> = state.suspended_solvers.keys().copied().collect();

        for elem in schedule {
            if running.remove(&elem.id) {
                // already running, keep it running
                keep_running.push(elem.id);
            } else if suspended.remove(&elem.id) {
                to_resume.push(elem.id);
            } else {
                to_start.push(elem);
            }
        }

        let to_suspend = running.into_iter().collect();

        ScheduleChanges {
            to_start,
            to_suspend,
            to_resume,
        }
    }

    fn apply_changes_to_state(state: &mut MemoryEnforcerState, changes: &ScheduleChanges) {
        for elem in &changes.to_start {
            state.running_solvers.insert(
                elem.id,
                SolverInfo {
                    name: elem.solver.clone(),
                    cores: elem.cores,
                },
            );
        }

        for &id in &changes.to_resume {
            if let Some(info) = state.suspended_solvers.remove(&id) {
                state.running_solvers.insert(id, info);
            }
        }

        for &id in &changes.to_suspend {
            if let Some(info) = state.running_solvers.remove(&id) {
                state.suspended_solvers.insert(id, info);
            }
        }
    }

    pub async fn apply(&mut self, mut schedule: Schedule) -> std::result::Result<(), Vec<Error>> {
        let mut state = self.state.lock().await;
        for elem in &mut schedule {
            elem.id = state.id;
            state.id += 1;
        }
        if let Some(objective) = self.solver_manager.get_best_objective().await {
            self.solver_manager.stop_all_solvers().await.unwrap();
            // insert new objective bound

            self.solver_manager.start_solvers(&schedule).await?;
            state.suspended_solvers = HashMap::new();
            state.running_solvers = schedule
                .into_iter()
                .map(|elem| {
                    (
                        elem.id,
                        SolverInfo {
                            name: elem.solver,
                            cores: elem.cores,
                        },
                    )
                })
                .collect();
            Ok(())
        } else {
            let changes =
                Self::categorize_schedule(schedule, &mut state, self.solver_manager.clone()).await;
            Self::apply_changes_to_state(&mut state, &changes);

            self.solver_manager
                .suspend_solvers(&changes.to_suspend)
                .await?;
            self.solver_manager
                .resume_solvers(&changes.to_resume)
                .await?;
            self.solver_manager.start_solvers(&changes.to_start).await
        }
    }
}
