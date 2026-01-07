use crate::{
    args::{Args, DebugVerbosityLevel},
    config::Config,
    logging,
    model_parser::ObjectiveValue,
    solver_manager::{Error, SolverManager},
};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;
use sysinfo::System;
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;

#[derive(Clone, Debug)]
pub struct ScheduleElement {
    pub id: u64,
    pub info: SolverInfo,
}

impl ScheduleElement {
    pub fn new(id: u64, info: SolverInfo) -> Self {
        Self { id, info }
    }
}

pub type Schedule = Vec<ScheduleElement>;
pub type Portfolio = Vec<SolverInfo>;

#[derive(Debug, Eq, PartialEq, Clone)]
pub struct SolverInfo {
    pub name: String,
    pub cores: usize,
    pub objective: Option<ObjectiveValue>,
}

impl std::fmt::Display for SolverInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "name({}),cores({}),objective({})",
            self.name,
            self.cores,
            self.objective
                .map(|x| x.to_string())
                .unwrap_or("None".to_owned())
        )
    }
}

impl SolverInfo {
    pub fn new(name: String, cores: usize) -> Self {
        Self {
            name,
            cores,
            objective: None,
        }
    }
}

#[derive(Debug)]
struct ScheduleChanges {
    to_start: Schedule,
    to_suspend: Vec<u64>,
    to_resume: Vec<u64>,
}

struct MemoryEnforcerState {
    running_solvers: HashMap<u64, SolverInfo>,
    suspended_solvers: HashMap<u64, SolverInfo>,
    system: System,
    memory_limit: u64, // In bytes (0 = use system total)
    next_solver_id: u64,
    prev_objective: Option<ObjectiveValue>,
    config: Config,
    debug_verbosity: DebugVerbosityLevel,
}

pub struct Scheduler {
    state: Arc<Mutex<MemoryEnforcerState>>,
    pub solver_manager: Arc<SolverManager>,
}

fn is_over_threshold(used: f64, total: f64, threshold: f64) -> bool {
    used / total > threshold
}

impl Scheduler {
    pub async fn new(
        args: &Args,
        config: &Config,
        token: CancellationToken,
    ) -> std::result::Result<Self, Error> {
        let solver_manager =
            Arc::new(SolverManager::new(args.clone(), config.solver_args.clone(), token).await?);

        let memory_limit = std::env::var("MEMORY_LIMIT")
            .ok()
            .and_then(|val| val.parse::<u64>().ok())
            .map(|mib| mib * 1024 * 1024)
            .unwrap_or(0);

        let debug_verbosity = args.debug_verbosity;

        let state = Arc::new(Mutex::new(MemoryEnforcerState {
            running_solvers: HashMap::new(),
            suspended_solvers: HashMap::new(),
            system: System::new_all(),
            memory_limit,
            next_solver_id: 0,
            prev_objective: None,
            config: config.clone(),
            debug_verbosity,
        }));

        let state_clone = state.clone();
        let solver_manager_clone = solver_manager.clone();
        let config_clone = config.clone();
        tokio::spawn(async move {
            Self::memory_enforcer_loop(state_clone, solver_manager_clone, config_clone).await;
        });

        Ok(Self {
            state,
            solver_manager,
        })
    }

    fn get_memory_usage(state: &mut MemoryEnforcerState) -> (f64, f64) {
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
        (used, total)
    }

    async fn kill_suspended_until_under_threshold(
        state: &mut MemoryEnforcerState,
        solver_manager: &Arc<SolverManager>,
        mut used_memory: f64,
        total_memory: f64,
    ) -> f64 {
        let ids: Vec<u64> = state.suspended_solvers.keys().copied().collect();
        let mut sorted = solver_manager
            .solvers_sorted_by_mem(&ids, &state.system)
            .await;

        while !sorted.is_empty()
            && is_over_threshold(used_memory, total_memory, state.config.memory_threshold)
        {
            let (mem, id) = sorted.remove(0);
            state.suspended_solvers.remove(&id);
            if let Err(e) = solver_manager.stop_solver(id).await {
                logging::error!(e.into());
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
    ) -> f64 {
        let ids: Vec<u64> = state.running_solvers.keys().copied().collect();
        let total_cores: usize = state.running_solvers.values().map(|info| info.cores).sum();
        if total_cores == 0 {
            return used_memory;
        }

        let sorted = solver_manager
            .solvers_sorted_by_mem(&ids, &state.system)
            .await;
        let per_core_threshold =
            (total_memory / total_cores as f64 * state.config.memory_threshold) as u64;

        let mut remaining = Vec::new();

        for (solver_mem, id) in sorted {
            let cores = match state.running_solvers.get(&id) {
                Some(info) => info.cores as u64,
                None => {
                    // should never fail since the state is locked however error logging just for safety
                    logging::error_msg!(
                        "Failed to get solver info. Cause of this error is probably from a logic error in the code"
                    );
                    continue;
                }
            };
            if solver_mem / cores > per_core_threshold {
                // use number of cores a process has to decide if it uses more that its fair share
                state.running_solvers.remove(&id);
                if let Err(e) = solver_manager.stop_solver(id).await {
                    logging::error_msg!("failed to stop running solver: {e}");
                } else {
                    used_memory -= solver_mem as f64;
                }
            } else {
                remaining.push((solver_mem, id));
            }
        }
        while !remaining.is_empty()
            && is_over_threshold(used_memory, total_memory, state.config.memory_threshold)
        {
            let (mem, id) = remaining.remove(0);
            state.running_solvers.remove(&id);
            if let Err(e) = solver_manager.stop_solver(id).await {
                logging::error_msg!("failed to stop running solver: {e}");
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
        config: Config,
    ) {
        let mut interval =
            tokio::time::interval(Duration::from_secs(config.memory_enforcer_interval));

        loop {
            interval.tick().await;
            let mut state: tokio::sync::MutexGuard<'_, MemoryEnforcerState> = state.lock().await;
            Self::remove_exited_solvers(&mut state, &solver_manager).await;
            let (used, total) = Self::get_memory_usage(&mut state);
            if !is_over_threshold(used, total, config.memory_threshold) {
                continue;
            }

            let div = (1024 * 1024) as f64;
            logging::info!(
                "Memory used by system: {} MiB, Memory Available: {} MiB, Memory threshold: {}",
                used / div,
                total / div,
                total * state.config.memory_threshold / div,
            );

            let used = Self::kill_suspended_until_under_threshold(
                &mut state,
                &solver_manager,
                used,
                total,
            )
            .await;

            if is_over_threshold(used, total, config.memory_threshold) {
                Self::kill_running_until_under_threshold(&mut state, &solver_manager, used, total)
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
            state.running_solvers.insert(elem.id, elem.info.clone());
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

    pub async fn apply(&mut self, portfolio: Portfolio) -> std::result::Result<(), Vec<Error>> {
        let mut state = self.state.lock().await;
        let new_objective = self.solver_manager.get_best_objective().await;

        if new_objective != state.prev_objective {
            logging::info!(
                "apply function objectives: old objective: {:?}, new: {:?}",
                state.prev_objective,
                new_objective
            );
            state.prev_objective = new_objective;

            if let Some(obj) = new_objective {
                let solver_objectives = self.solver_manager.get_solver_objectives().await;
                let objective_type = self.solver_manager.objective_type();
                let to_restart: Vec<u64> = solver_objectives
                    .iter()
                    .filter(|(_, best)| objective_type.is_better(**best, obj))
                    .map(|(id, _)| *id)
                    .collect();

                if !to_restart.is_empty() {
                    logging::info!("solver objectives: {:?}", solver_objectives);
                    logging::info!("solver to restart {:?}", to_restart);
                }

                self.solver_manager.stop_solvers(&to_restart).await?;

                for id in &to_restart {
                    state.running_solvers.remove(id);
                    state.suspended_solvers.remove(id);
                }
            }
        }

        let schedule = Self::assign_ids(portfolio, &mut state);
        let changes =
            Self::categorize_schedule(schedule.clone(), &mut state, self.solver_manager.clone())
                .await;
        Self::apply_changes_to_state(&mut state, &changes);

        if state.debug_verbosity >= DebugVerbosityLevel::Info
            && (!changes.to_start.is_empty()
                || !changes.to_suspend.is_empty()
                || !changes.to_resume.is_empty())
        {
            logging::info!("changes: {:?}", changes);
        }

        if let Err(e) = self
            .solver_manager
            .suspend_solvers(&changes.to_suspend)
            .await
        {
            logging::error_msg!("failed to suspend solvers: {:?}", e);
            self.solver_manager
                .stop_solvers(&changes.to_suspend)
                .await?;
        }

        if let Err(e) = self.solver_manager.resume_solvers(&changes.to_resume).await {
            logging::error_msg!("Failed to resume solvers: {e:?}");
            let mut resume_elements = Vec::new();
            for schedule_elem in &schedule {
                for resume_id in &changes.to_resume {
                    if &schedule_elem.id == resume_id {
                        resume_elements.push(schedule_elem.clone());
                    }
                }
            }
            self.solver_manager
                .start_solvers(&resume_elements, state.prev_objective)
                .await?;
        }

        self.solver_manager
            .start_solvers(&changes.to_start, state.prev_objective)
            .await
    }

    fn assign_ids(
        portfolio: Portfolio,
        state: &mut tokio::sync::MutexGuard<'_, MemoryEnforcerState>,
    ) -> Schedule {
        let running_solvers: Vec<(u64, SolverInfo)> = state
            .running_solvers
            .iter()
            .map(|(k, v)| (*k, v.clone()))
            .collect();

        let suspended_solvers: Vec<(u64, SolverInfo)> = state
            .suspended_solvers
            .iter()
            .map(|(k, v)| (*k, v.clone()))
            .collect();

        let mut solvers = running_solvers;
        solvers.extend(suspended_solvers);

        let mut schedule = Vec::new();
        for new_info in portfolio.into_iter() {
            let mut i = 0;

            while i < solvers.len() && new_info != solvers[i].1 {
                i += 1
            }

            let id = if i < solvers.len() {
                solvers.remove(i).0
            } else {
                let id = state.next_solver_id;
                state.next_solver_id += 1;
                id
            };

            let elem = ScheduleElement::new(id, new_info);
            schedule.push(elem);
        }

        schedule
    }
}
