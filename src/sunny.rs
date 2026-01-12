use crate::config::Config;
use crate::fzn_to_features::fzn_to_features;
use crate::mzn_to_fzn::convert_mzn;
use crate::scheduler::Scheduler;
use crate::static_schedule::{static_schedule, timeout_schedule};
use crate::{ai::Ai, args::Args};
use crate::{logging, solver_manager};
use tokio::time::{Duration, sleep, timeout};
use tokio_util::sync::CancellationToken;
const FEATURES_SOLVER: &str = "gecode";

pub async fn sunny(
    args: &Args,
    ai: impl Ai,
    config: Config,
    token: CancellationToken,
) -> Result<(), ()> {
    let mut scheduler = Scheduler::new(args, &config, token.clone())
        .await
        .map_err(|e| logging::error!(e.into()))?;

    let result = sunny_inner(args, ai, &config, &mut scheduler, token).await;

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
    token: CancellationToken,
) -> Result<(), ()> {
    let timer_duration = Duration::from_secs(config.dynamic_schedule_interval);
    let mut cores = args.cores.unwrap_or(2);

    let solver_cores = if args.pin_cores {
        if cores <= 1 {
            logging::warning!("Too few cores are set. Using 2 cores");
            cores = 2;
        }
        cores - 1 // We subtract one because we are gonna be extraction features in the bagground for the feature extractor
    } else {
        cores
    };

    let schedule = static_schedule(args, solver_cores)
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

    let timer = sleep(timer_duration);
    let feature_extraction_max_duration = Duration::from_secs(10);
    let timeout_result = timeout(feature_extraction_max_duration, get_features(args)).await;
    let schedule = match timeout_result {
        Ok(features_result) => {
            let features = features_result?;
            ai.schedule(&features, cores)
                .map_err(|e| logging::error!(e.into()))?
        }
        Err(_) => {
            logging::warning!("Feature extraction timed out. Running timeout schedule");
            timeout_schedule(args, cores)
                .await
                .map_err(|e| logging::error!(e.into()))?
        }
    };

    timer.await;

    let schedule_len = schedule.len();
    if let Err(errors) = scheduler.apply(schedule).await {
        let errorlen = errors.len();
        handle_schedule_errors(errors);
        if errorlen == schedule_len {
            return Err(());
        }
    }
    token.cancelled().await; // Wait until the solution has been found
    Ok(())
}

async fn get_features(args: &Args) -> Result<Vec<f32>, ()> {
    let conversion = convert_mzn(args, FEATURES_SOLVER)
        .await
        .map_err(|e| logging::error!(e.into()))?;

    let features = fzn_to_features(conversion.fzn())
        .await
        .map_err(|e| logging::error!(e.into()))?;
    Ok(features)
}

fn handle_schedule_errors(errors: Vec<solver_manager::Error>) {
    logging::error_msg!("got the following errors when applying the schedule:");
    errors.into_iter().for_each(|e| logging::error!(e.into()));
}
