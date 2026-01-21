use std::sync::Arc;

use crate::config::Config;
use crate::fzn_to_features::fzn_to_features;
use crate::mzn_to_fzn::convert_mzn;
use crate::scheduler::{Portfolio, Scheduler};
use crate::static_schedule::{static_schedule, timeout_schedule};
use crate::{ai::Ai, args::Args};
use crate::{logging, solver_discovery, solver_manager};
use tokio::time::{Duration, sleep, timeout};
use tokio_util::sync::CancellationToken;
const FEATURES_SOLVER: &str = "gecode";

pub async fn sunny(
    args: &Args,
    ai: Option<impl Ai>,
    config: Config,
    solvers: Arc<solver_discovery::Solvers>,
    program_cancellation_token: CancellationToken,
) -> Result<(), ()> {
    let mut scheduler = Scheduler::new(args, &config, solvers, program_cancellation_token)
        .await
        .map_err(|e| logging::error!(e.into()))?;

    let (cores, initial_solver_cores) = get_cores(args, &ai);

    let initial_schedule = static_schedule(args, initial_solver_cores)
        .await
        .map_err(|e| logging::error!(e.into()))?;

    let static_runtime = Duration::from_secs(args.static_runtime);
    let mut timer = sleep(static_runtime);

    let schedule = if let Some(mut ai) = ai {
        start_with_ai(args, &mut ai, &mut scheduler, initial_schedule, cores).await?
    } else {
        start_without_ai(args, &mut scheduler, initial_schedule).await?
    };

    let restart_interval = Duration::from_secs(args.restart_interval);
    // Restart loop, where it share bounds. It runs forever until it finds a solution, where it will then be cancelled by the cancellation token.
    loop {
        timer.await;
        let schedule_len = schedule.len();
        if let Err(errors) = scheduler.apply(schedule.clone(), None).await {
            let errorlen = errors.len();
            handle_schedule_errors(errors);
            if errorlen == schedule_len {
                return Err(());
            }
        }
        timer = sleep(restart_interval);
    }
}

async fn start_with_ai(
    args: &Args,
    ai: &mut impl Ai,
    scheduler: &mut Scheduler,
    initial_schedule: Portfolio,
    cores: usize,
) -> Result<Portfolio, ()> {
    // Static schedule, compilation only
    // Feature extraction
    // Timeout
    let cancellation_token = CancellationToken::new();

    let static_runtime_duration = Duration::from_secs(args.static_runtime);
    let static_runtime_timeout_future = sleep(static_runtime_duration);
    let feature_timeout_duration =
        Duration::from_secs(args.feature_timeout.max(args.static_runtime));
    let feature_timeout_timeout_future = sleep(feature_timeout_duration);

    let get_features_future = get_features(args, cancellation_token.clone());

    let scheduler_task =
        scheduler.apply(initial_schedule.clone(), Some(cancellation_token.clone()));

    tokio::pin!(static_runtime_timeout_future);
    tokio::pin!(feature_timeout_timeout_future);
    tokio::pin!(get_features_future);
    tokio::pin!(scheduler_task);

    let mut static_timeout_expired = false;
    let mut feature_timeout_expired = false;
    let mut got_features = false;
    let mut app_features = false;
    loop {
        let r = tokio::select! {
            (feat_res, _sleep_res) = &mut static_runtime_timeout_future => {
                static_timeout_expired = true;
                (feat_res, None)
            }

            sched_res = &mut scheduler_task => {
                let (feat_res, _sleep_res) = barrier.await;
                (feat_res, Some(sched_res))
            }
        };
    }

    // let feature_timeout_duration =
    //     Duration::from_secs(args.feature_timeout.max(args.static_runtime)); // if static runtime is higher thatn feature_runtime, we anyways have to wait, so we have more time to extract features
    // let barrier = async {
    //     tokio::join!(
    //         timeout(feature_timeout_duration, get_features(args, cancellation_token.clone())),
    //         sleep(static_runtime_duration)
    //     )
    // };
    // tokio::pin!(barrier);

    // let scheduler_task = scheduler.apply(initial_schedule.clone(), Some(cancellation_token.clone()));
    // tokio::pin!(scheduler_task);

    // let (features_result, static_schedule_finished) = tokio::select! {
    //     (feat_res, _sleep_res) = &mut barrier => {
    //         (feat_res, None)
    //     }

    //     sched_res = &mut scheduler_task => {
    //         let (feat_res, _sleep_res) = barrier.await;
    //         (feat_res, Some(sched_res))
    //     }
    // };
    todo!();
    // let schedule = match features_result {
    //     Ok(features_result) => {
    //         let features = features_result?;
    //         ai.schedule(&features, cores)
    //             .map_err(|e| logging::error!(e.into()))?
    //     }
    //     Err(_) => {
    //         logging::info!("Feature extraction timed out. Running timeout schedule");
    //         timeout_schedule(args, cores)
    //             .await
    //             .map_err(|e| logging::error!(e.into()))?
    //     }
    // };

    // match static_schedule_finished {
    //     Some(Ok(())) => {}
    //     Some(Err(errors)) => {
    //         let error_len = errors.len();
    //         handle_schedule_errors(errors);
    //         if error_len == initial_schedule.len() {
    //             return Err(());
    //         }
    //     }
    //     None => {
    //         logging::info!("applying static schedule timed out");
    //     }
    // }
    // Ok(schedule)
}

// async fn start_with_ai(
//     args: &Args,
//     ai: &mut impl Ai,
//     scheduler: &mut Scheduler,
//     initial_schedule: Portfolio,
//     cores: usize,
// ) -> Result<Portfolio, ()> {
//     let feature_timeout_duration =
//         Duration::from_secs(args.feature_timeout.max(args.static_runtime)); // if static runtime is higher thatn feature_runtime, we anyways have to wait, so we have more time to extract features
//     let static_runtime_duration = Duration::from_secs(args.static_runtime);
//     let barrier = async {
//         tokio::join!(
//             timeout(feature_timeout_duration, get_features(args)),
//             sleep(static_runtime_duration)
//         )
//     };
//     tokio::pin!(barrier);

//     let scheduler_task = scheduler.apply(initial_schedule.clone());
//     tokio::pin!(scheduler_task);

//     let (features_result, static_schedule_finished) = tokio::select! {
//         (feat_res, _sleep_res) = &mut barrier => {
//             (feat_res, None)
//         }

//         sched_res = &mut scheduler_task => {
//             let (feat_res, _sleep_res) = barrier.await;
//             (feat_res, Some(sched_res))
//         }
//     };

//     let schedule = match features_result {
//         Ok(features_result) => {
//             let features = features_result?;
//             ai.schedule(&features, cores)
//                 .map_err(|e| logging::error!(e.into()))?
//         }
//         Err(_) => {
//             logging::info!("Feature extraction timed out. Running timeout schedule");
//             timeout_schedule(args, cores)
//                 .await
//                 .map_err(|e| logging::error!(e.into()))?
//         }
//     };

//     match static_schedule_finished {
//         Some(Ok(())) => {}
//         Some(Err(errors)) => {
//             let error_len = errors.len();
//             handle_schedule_errors(errors);
//             if error_len == initial_schedule.len() {
//                 return Err(());
//             }
//         }
//         None => {
//             logging::info!("applying static schedule timed out");
//         }
//     }
//     Ok(schedule)
// }

async fn start_without_ai(
    args: &Args,
    scheduler: &mut Scheduler,
    schedule: Portfolio,
) -> Result<Portfolio, ()> {
    let static_runtime = Duration::from_secs(args.static_runtime);
    let cancellation_token = CancellationToken::new();

    let fut = scheduler.apply(schedule.clone(), Some(cancellation_token.clone()));
    tokio::pin!(fut);

    let apply_result = tokio::select! {
        result = &mut fut => {
            result
        }
        _ = sleep(static_runtime) => {
            cancellation_token.cancel();
            logging::info!("applying static schedule timed out");
            fut.await
        }
    };

    match apply_result {
        Ok(()) => {}
        Err(errors) => {
            let error_len = errors.len();
            handle_schedule_errors(errors);
            if error_len == schedule.len() {
                return Err(());
            }
        }
    }
    Ok(schedule)
}

fn get_cores(args: &Args, ai: &Option<impl Ai>) -> (usize, usize) {
    let mut cores = args.cores;

    let initial_solver_cores = if args.pin_yuck && ai.is_some() {
        if cores <= 1 {
            logging::warning!("Too few cores are set. Using 2 cores");
            cores = 2;
        }
        cores - 1 // We subtract one because we are going to be extracting features in the background for the feature extractor
    } else {
        cores
    };
    (cores, initial_solver_cores)
}

async fn get_features(args: &Args, token: CancellationToken) -> Result<Vec<f32>, ()> {
    let conversion = convert_mzn(args, FEATURES_SOLVER, Some(token))
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
