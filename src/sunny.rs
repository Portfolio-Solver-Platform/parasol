use crate::config::Config;
use crate::fzn_to_features::fzn_to_features;
use crate::mzn_to_fzn::convert_mzn;
use crate::scheduler::Scheduler;
use crate::static_schedule::static_schedule;
use crate::{ai::Ai, args::Args};
use tokio::time::{Duration, sleep};
use tokio_util::sync::CancellationToken;
const FEATURES_SOLVER: &str = "gecode";

pub async fn sunny(args: Args, mut ai: impl Ai, config: Config, token: CancellationToken) {
    let timer_duration = Duration::from_secs(config.dynamic_schedule_interval);
    let cores = args.cores.unwrap_or(2);
    let mut scheduler = Scheduler::new(&args, &config, token)
        .await
        .expect("Failed to create scheduler");

    let schedule = static_schedule(&args, cores).await.unwrap();
    let schedule_len = schedule.len();
    if let Err(errors) = scheduler.apply(schedule).await {
        if errors.len() == schedule_len {
            panic!("all solvers failed");
        } else {
            eprintln!("Got the following errors: {:?}", errors);
        }
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
    .expect("failed to initially convert .mzn to .fzn");

    let features = fzn_to_features(conversion.fzn())
        .await
        .expect("if we fail to get features, we can't run the AI and thus can't recover");

    loop {
        timer.await;

        let schedule = ai.schedule(&features, cores).unwrap();
        let schedule_len = schedule.len();
        if let Err(errors) = scheduler.apply(schedule).await {
            if errors.len() == schedule_len {
                panic!("all solvers failed");
            } else {
                eprintln!("Got the following errors: {:?}", errors);
            }
        }

        timer = sleep(timer_duration);
    }
}
