use std::sync::Arc;

use crate::args::{StaticArgs, UnpackAiError};
use crate::config::Config;
use crate::fzn_to_features::{self, fzn_to_features};
use crate::mzn_to_fzn::compilation_scheduler::{CompilationScheduler, SolverPriority};
use crate::orchestrator::Orchestrator;
use crate::scheduler::{Portfolio, Scheduler};
use crate::signal_handler::SignalEvent;
use crate::static_schedule::{self, static_schedule, timeout_schedule};
use crate::{ai, logging, solver_config, solver_manager};
use crate::{ai::Ai, args::CommonArgs};
use crate::{args, mzn_to_fzn};
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
    WaitForMznToFzn(#[from] mzn_to_fzn::compilation_executor::WaitForError),
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
    #[error("an IO error occurred")]
    TokioIo(#[from] tokio::io::Error),
    #[error("failed to unpack the AI")]
    UnpackAi(#[from] UnpackAiError),
}

pub struct StaticParallelPortfolio {
    args: StaticArgs,
    ai: Option<Box<dyn Ai + Send>>,
    config: Config,
    solvers: Arc<solver_config::Solvers>,
    program_cancellation_token: CancellationToken,
    suspend_and_resume_signal_rx: tokio::sync::mpsc::UnboundedReceiver<SignalEvent>,
}

impl From<Error> for crate::orchestrator::Error {
    fn from(value: Error) -> Self {
        match value {
            Error::Cancelled => crate::orchestrator::Error::Cancelled,
            e => crate::orchestrator::Error::Other(e.into()),
        }
    }
}

impl StaticParallelPortfolio {
    pub async fn new(
        args: StaticArgs,
        program_cancellation_token: CancellationToken,
        suspend_and_resume_signal_rx: tokio::sync::mpsc::UnboundedReceiver<SignalEvent>,
    ) -> Result<Self, Error> {
        let solvers = Arc::new(
            solver_config::load(&args.solver_config_mode, &args.common.minizinc.minizinc_exe).await,
        );

        let config = Config::new(&solvers);

        let ai = args::unpack_ai(&args.ai)?;

        Ok(Self {
            args,
            ai,
            config,
            solvers,
            program_cancellation_token,
            suspend_and_resume_signal_rx,
        })
    }
}

impl Orchestrator for StaticParallelPortfolio {
    async fn run(self) -> Result<(), crate::orchestrator::Error> {
        let common_args = Arc::new(self.args.common.clone());
        let compilation_priority = get_compilation_priority(&self.args)
            .await
            .map_err(Error::from)?;
        let compilation_manager = Arc::new(CompilationScheduler::new(
            Arc::clone(&common_args),
            compilation_priority,
        ));

        let mut scheduler = Scheduler::new(
            Arc::clone(&common_args),
            &self.config,
            self.solvers,
            Arc::clone(&compilation_manager),
            self.program_cancellation_token.clone(),
            self.suspend_and_resume_signal_rx,
        )
        .await
        .map_err(Error::from)?;

        let (cores, initial_solver_cores) = get_cores(&common_args, self.ai.as_deref());

        let initial_schedule = static_schedule(&self.args, initial_solver_cores)
            .await
            .map_err(Error::from)?;

        let static_runtime = Duration::from_secs(self.args.static_runtime);
        let mut timer = sleep(static_runtime);

        let mut extra_compilations_are_enabled = true;
        let start_cancellation_token = self.program_cancellation_token.child_token();
        let schedule = if let Some(ai) = self.ai {
            start_with_ai(
                &self.args,
                ai,
                &mut scheduler,
                initial_schedule,
                cores,
                start_cancellation_token,
                Arc::clone(&compilation_manager),
            )
            .await
        } else {
            compilation_manager.disable_extra_compilations().await;
            extra_compilations_are_enabled = false;
            start_without_ai(&self.args, &mut scheduler, initial_schedule).await
        }?;

        let restart_interval = Duration::from_secs(self.args.restart_interval);
        // Restart loop, where it share bounds. It runs forever until it finds a solution, where it will then be cancelled by the cancellation token.
        loop {
            tokio::select! {
                _ = timer => {}
                _ = self.program_cancellation_token.cancelled() => {
                    return Err(Error::Cancelled.into())
                }
            }

            if scheduler.solver_manager.all_solvers_failed(&schedule).await {
                return Err(Error::SolverFailure.into());
            }

            if extra_compilations_are_enabled {
                compilation_manager.disable_extra_compilations().await;
                extra_compilations_are_enabled = false;
            }

            let apply_cancellation_token = scheduler.create_apply_token();
            scheduler
                .apply(schedule.clone(), apply_cancellation_token, true)
                .await;

            timer = sleep(restart_interval);
        }
    }
}

async fn start_with_ai(
    args: &StaticArgs,
    mut ai: Box<dyn Ai + Send>,
    scheduler: &mut Scheduler,
    initial_schedule: Portfolio,
    cores: usize,
    cancellation_token: CancellationToken,
    compilation_manager: Arc<CompilationScheduler>,
) -> Result<Portfolio, Error> {
    let static_runtime_duration = Duration::from_secs(args.static_runtime);

    let feature_timeout_duration =
        Duration::from_secs(args.feature_timeout.max(args.static_runtime)); // if static runtime is higher thatn feature_runtime, we anyways have to wait, so we have more time to extract features
    let barrier = async {
        tokio::join!(
            timeout(
                feature_timeout_duration,
                get_features(&args, compilation_manager, cancellation_token.clone())
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
            (feat_res, false)
        }

        _ = &mut scheduler_task => {
            let feat_res = tokio::select! {
                (feat_res, _sleep_res) = barrier => feat_res,
                _ = cancellation_token.cancelled() => return Err(Error::Cancelled),
            };
            (feat_res, true)
        }
    };

    let schedule = match features_result {
        Ok(features_result) => {
            let features = features_result?;
            tokio::task::spawn_blocking(move || ai.schedule(&features, cores)).await??
        }
        Err(_) => {
            logging::info!("Feature extraction timed out. Running timeout schedule");
            timeout_schedule(&args, cores).await?
        }
    };

    if !static_schedule_finished {
        logging::info!("applying static schedule timed out");
    }

    Ok(schedule)
}

async fn start_without_ai(
    args: &StaticArgs,
    scheduler: &mut Scheduler,
    schedule: Portfolio,
) -> Result<Portfolio, Error> {
    let static_runtime = Duration::from_secs(args.static_runtime);

    let apply_cancellation_token = scheduler.create_apply_token();
    let fut = scheduler.apply(schedule.clone(), apply_cancellation_token.clone(), true);
    tokio::pin!(fut);

    tokio::select! {
        _ = &mut fut => {}
        _ = sleep(static_runtime) => {
            apply_cancellation_token.cancel();
            logging::info!("applying static schedule timed out");
            fut.await;
        }
    };

    Ok(schedule)
}

fn get_cores(args: &CommonArgs, ai: Option<&(dyn Ai + Send)>) -> (usize, usize) {
    let mut cores = args.cores;

    let initial_solver_cores = if ai.is_some() {
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
    args: &StaticArgs,
    compilation_manager: Arc<CompilationScheduler>,
    token: CancellationToken,
) -> Result<Vec<f32>, Error> {
    compilation_manager
        .start(args.ai.feature_extraction_solver_id.clone(), 1)
        .await;
    let conversion = token
        .run_until_cancelled(compilation_manager.wait_for(&args.ai.feature_extraction_solver_id))
        .await
        .ok_or(Error::Cancelled)??;

    tokio::select! {
        result = fzn_to_features(conversion.fzn()) => {
            result.map_err(Error::from)
        },
        _ = token.cancelled() => Err(Error::Cancelled)
    }
}

async fn get_compilation_priority(args: &StaticArgs) -> tokio::io::Result<SolverPriority> {
    if let Some(path) = &args.compilation_priority {
        args::read_compilation_priority(path).await
    } else {
        Ok(SolverPriority::empty())
    }
}
