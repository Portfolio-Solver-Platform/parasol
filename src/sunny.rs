use std::sync::Arc;

use crate::config::Config;
use crate::fzn_to_features::fzn_to_features;
use crate::mzn_to_fzn::convert_mzn;
use crate::scheduler::Scheduler;
use crate::static_schedule::static_schedule;
use crate::{ai::Ai, args::Args};
use crate::{logging, solver_discovery, solver_manager};
use tokio::time::{Duration, sleep};
use tokio_util::sync::CancellationToken;
const FEATURES_SOLVER: &str = "gecode";

pub async fn sunny(
    args: &Args,
    ai: impl Ai,
    config: Config,
    solvers: Arc<solver_discovery::Solvers>,
    token: CancellationToken,
) -> Result<(), ()> {
    let mut scheduler = Scheduler::new(args, &config, solvers, token)
        .await
        .map_err(|e| logging::error!(e.into()))?;

    let result = sunny_inner(args, ai, &config, &mut scheduler).await;

    if let Err(e) = scheduler.solver_manager.stop_all_solvers().await {
        handle_schedule_errors(e);
    }
    result
}

async fn sunny_inner(
    args: &Args,
    mut ai: impl Ai,
    config: &Config,
    scheduler: &mut Scheduler,
) -> Result<(), ()> {
    let timer_duration = Duration::from_secs(config.dynamic_schedule_interval);
    let cores = args.cores.unwrap_or(2);

    let schedule = static_schedule(args, cores)
        .await
        .map_err(|e| logging::error!(e.into()))?;

    let schedule_len = schedule.len();
    if let Err(errors) = scheduler.apply(schedule).await {
        let errorlen = errors.len();
        handle_schedule_errors(errors);
        if errorlen == schedule_len {
            return Err(());
        }
    }

    let mut timer = sleep(timer_duration);

    let conversion = convert_mzn(args, FEATURES_SOLVER)
        .await
        .map_err(|e| logging::error!(e.into()))?;

    let features = fzn_to_features(conversion.fzn())
        .await
        .map_err(|e| logging::error!(e.into()))?;

    loop {
        timer.await;
        let schedule = ai
            .schedule(&features, cores)
            .map_err(|e| logging::error!(e.into()))?;

        let schedule_len = schedule.len();
        if let Err(errors) = scheduler.apply(schedule).await {
            let errorlen = errors.len();
            handle_schedule_errors(errors);
            if errorlen == schedule_len {
                return Err(());
            }
        }

        timer = sleep(timer_duration);
    }
}

fn handle_schedule_errors(errors: Vec<solver_manager::Error>) {
    logging::error_msg!("got the following errors when applying the schedule:");
    errors.into_iter().for_each(|e| logging::error!(e.into()));
}
