use crate::config::Config;
use crate::fzn_to_features::fzn_to_features;
use crate::mzn_to_fzn::convert_mzn;
use crate::scheduler::Scheduler;
use crate::static_schedule::static_schedule;
use crate::{ai::Ai, args::Args};
use crate::{logging, solver_manager};
use tokio::time::{Duration, sleep};
use tokio_util::sync::CancellationToken;
const FEATURES_SOLVER: &str = "coinbc";

pub async fn sunny(args: Args, mut ai: impl Ai, config: Config, token: CancellationToken) {
    let timer_duration = Duration::from_secs(config.dynamic_schedule_interval);
    let cores = args.cores.unwrap_or(2);
    let mut scheduler = Scheduler::new(&args, &config, token)
        .await
        .map_err(|e| logging::error!(e.into()))
        .expect("Failed to create scheduler");

    let schedule = static_schedule(&args, cores)
        .await
        .map_err(|e| logging::error!(e.into()))
        .unwrap();
    let schedule_len = schedule.len();
    if let Err(errors) = scheduler.apply(schedule).await {
        handle_schedule_errors(errors, schedule_len);
    }

    let mut timer = sleep(timer_duration);
    let conversion = convert_mzn(
        &args.minizinc_exe,
        &args.model,
        args.data.as_deref(),
        FEATURES_SOLVER,
        args.debug_verbosity,
    )
    .await
    .map_err(|e| logging::error!(e.into()))
    .expect("failed to initially convert .mzn to .fzn");

    let features = fzn_to_features(conversion.fzn())
        .await
        .map_err(|e| logging::error!(e.into()))
        .expect("if we fail to get features, we can't run the AI and thus can't recover");

    loop {
        timer.await;

        let schedule = ai
            .schedule(&features, cores)
            .map_err(|e| logging::error!(e.into()))
            .unwrap();
        let schedule_len = schedule.len();
        if let Err(errors) = scheduler.apply(schedule).await {
            handle_schedule_errors(errors, schedule_len);
        }

        timer = sleep(timer_duration);
    }
}

fn handle_schedule_errors(errors: Vec<solver_manager::Error>, schedule_len: usize) {
    let errors_len = errors.len();
    logging::error_msg!("got the following errors when applying the schedule:");
    errors.into_iter().for_each(|e| logging::error!(e.into()));

    if errors_len == schedule_len {
        panic!("all solvers failed");
    }
}
