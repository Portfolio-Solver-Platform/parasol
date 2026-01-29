use std::sync::Arc;

use crate::config::Config;
use crate::fzn_to_features::{self, fzn_to_features};
use crate::mzn_to_fzn;
use crate::mzn_to_fzn::compilation_manager::CompilationManager;
use crate::scheduler::{Portfolio, Scheduler};
use crate::signal_handler::SignalEvent;
use crate::static_schedule::{self, static_schedule, timeout_schedule};
use crate::{ai, logging, solver_config, solver_manager};
use crate::{ai::Ai, args::RunArgs};
use tokio::time::{Duration, sleep, timeout};
use tokio_util::sync::CancellationToken;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("conversion was cancelled")]
    Cancelled,
    #[error("failed converting flatzinc to features")]
    FznToFeatures(#[from] fzn_to_features::Error),
    #[error("failed converting minizinc to flatzinc")]
    MznToFzn(#[from] mzn_to_fzn::Error),
    #[error("failed to wait for the compilation")]
    WaitForMznToFzn(#[from] mzn_to_fzn::compilation_manager::WaitForError),
    #[error("Schedule error")]
    Schedule(#[from] static_schedule::Error),
    #[error("Ai error")]
    Ai(#[from] ai::Error),
    #[error("Task join error")]
    JoinError(#[from] tokio::task::JoinError),
    #[error("Solver manager error")]
    SolverManager(#[from] solver_manager::Error),
    #[error("All solvers failed, could not continue")]
    SolverFailure,
}

pub async fn sunny<T: Ai + Send + 'static>(
    args: &RunArgs,
    ai: Option<T>,
    config: Config,
    solvers: Arc<solver_config::Solvers>,
    program_cancellation_token: CancellationToken,
    suspend_and_resume_signal_rx: tokio::sync::mpsc::UnboundedReceiver<SignalEvent>,
) -> Result<(), Error> {
    let compilation_manager = Arc::new(CompilationManager::new(Arc::new(args.clone())));

    let mut scheduler = Scheduler::new(
        args,
        &config,
        solvers,
        compilation_manager.clone(),
        program_cancellation_token.clone(),
        suspend_and_resume_signal_rx,
    )
    .await?;

    let (cores, initial_solver_cores) = get_cores(args, &ai);
    // let solver_priority_order = get_priority_schedule()

    let initial_schedule = static_schedule(args, initial_solver_cores).await?;

    let static_runtime = Duration::from_secs(args.static_runtime);
    let mut timer = sleep(static_runtime);

    let start_cancellation_token = program_cancellation_token.child_token();
    let schedule = if let Some(ai) = ai {
        start_with_ai(
            args,
            ai,
            &mut scheduler,
            initial_schedule,
            cores,
            start_cancellation_token,
            compilation_manager,
        )
        .await
    } else {
        start_without_ai(args, &mut scheduler, initial_schedule).await
    }?;

    let restart_interval = Duration::from_secs(args.restart_interval);
    // Restart loop, where it share bounds. It runs forever until it finds a solution, where it will then be cancelled by the cancellation token.
    loop {
        tokio::select! {
            _ = timer => {}
            _ = program_cancellation_token.cancelled() => {
                return Err(Error::Cancelled)
            }
        }

        let schedule_len = schedule.len();

        let apply_cancellation_token = scheduler.create_apply_token();
        if let Err(errors) = scheduler
            .apply(schedule.clone(), apply_cancellation_token, true)
            .await
        {
            let errorlen = errors.len();
            handle_schedule_errors(errors);
            if errorlen == schedule_len {
                return Err(Error::SolverFailure);
            }
        }

        timer = sleep(restart_interval);
    }
}

async fn start_with_ai<T: Ai + Send + 'static>(
    args: &RunArgs,
    mut ai: T,
    scheduler: &mut Scheduler,
    initial_schedule: Portfolio,
    cores: usize,
    cancellation_token: CancellationToken,
    compilation_manager: Arc<CompilationManager>,
) -> Result<Portfolio, Error> {
    let static_runtime_duration = Duration::from_secs(args.static_runtime);

    let feature_timeout_duration =
        Duration::from_secs(args.feature_timeout.max(args.static_runtime)); // if static runtime is higher thatn feature_runtime, we anyways have to wait, so we have more time to extract features
    let barrier = async {
        tokio::join!(
            timeout(
                feature_timeout_duration,
                get_features(args, compilation_manager, cancellation_token.clone())
            ),
            sleep(static_runtime_duration)
        )
    };
    tokio::pin!(barrier);
    let apply_cancellation_token = scheduler.create_apply_token();
    let scheduler_task = scheduler.apply(
        initial_schedule.clone(),
        apply_cancellation_token.clone(),
        false,
    );
    tokio::pin!(scheduler_task);

    let (features_result, static_schedule_finished) = tokio::select! {
        (feat_res, _sleep_res) = &mut barrier => {
            apply_cancellation_token.cancel();
            (feat_res, None)
        }

        sched_res = &mut scheduler_task => {
            let feat_res = tokio::select! {
                (feat_res, _sleep_res) = barrier => feat_res,
                _ = cancellation_token.cancelled() => return Err(Error::Cancelled),
            };
            (feat_res, Some(sched_res))
        }
    };

    let schedule = match features_result {
        Ok(features_result) => {
            let features = features_result?;
            tokio::task::spawn_blocking(move || ai.schedule(&features, cores)).await??
        }
        Err(_) => {
            logging::info!("Feature extraction timed out. Running timeout schedule");
            timeout_schedule(args, cores).await?
        }
    };

    match static_schedule_finished {
        Some(Ok(())) => {}
        Some(Err(errors)) => {
            let error_len = errors.len();
            handle_schedule_errors(errors);
            if error_len == initial_schedule.len() {
                return Err(Error::SolverFailure);
            }
        }
        None => {
            logging::info!("applying static schedule timed out");
        }
    }
    Ok(schedule)
}

async fn start_without_ai(
    args: &RunArgs,
    scheduler: &mut Scheduler,
    schedule: Portfolio,
) -> Result<Portfolio, Error> {
    let static_runtime = Duration::from_secs(args.static_runtime);

    let apply_cancellation_token = scheduler.create_apply_token();
    let fut = scheduler.apply(schedule.clone(), apply_cancellation_token.clone(), true);
    tokio::pin!(fut);

    let apply_result = tokio::select! {
        result = &mut fut => {
            result
        }
        _ = sleep(static_runtime) => {
            apply_cancellation_token.cancel();
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
                return Err(Error::SolverFailure);
            }
        }
    }
    Ok(schedule)
}

fn get_cores(args: &RunArgs, ai: &Option<impl Ai>) -> (usize, usize) {
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

async fn get_features(
    args: &RunArgs,
    compilation_manager: Arc<CompilationManager>,
    token: CancellationToken,
) -> Result<Vec<f32>, Error> {
    compilation_manager
        .start(args.feature_extraction_solver_id.clone())
        .await;
    let conversion = token
        .run_until_cancelled(compilation_manager.wait_for(&args.feature_extraction_solver_id))
        .await
        .ok_or(Error::Cancelled)??;

    tokio::select! {
        result = fzn_to_features(conversion.fzn()) => {
            result.map_err(Error::from)
        },
        _ = token.cancelled() => Err(Error::Cancelled)
    }
}

fn handle_schedule_errors(errors: Vec<solver_manager::Error>) {
    logging::error_msg!("got the following errors when applying the schedule:");
    errors.into_iter().for_each(|e| logging::error!(e.into()));
}
