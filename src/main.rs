mod ai;
mod args;
mod backup_solvers;
mod config;
mod fzn_to_features;
mod insert_objective;
mod is_cancelled;
mod logging;
mod model_parser;
mod mzn_to_fzn;
mod orchestrator;
mod process_tree;
mod scheduler;
mod signal_handler;
mod solver_config;
mod solver_manager;
mod solver_output;
mod solvers;
mod static_schedule;

use std::process::exit;

use crate::args::{Cli, Command, CommonArgs};
use crate::backup_solvers::run_backup_solver;
use crate::orchestrator::Orchestrator;
// use crate::orchestrator::StaticParrallelPortfolio;
use crate::signal_handler::{SignalEvent, spawn_signal_handler};
use clap::Parser;
use tokio_util::sync::CancellationToken;

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let cli = Cli::parse();
    let program_cancellation_token = CancellationToken::new();
    let suspend_and_resume_signal_rx: tokio::sync::mpsc::UnboundedReceiver<SignalEvent> =
        spawn_signal_handler(program_cancellation_token.clone());

    match cli.command {
        Command::BuildSolverCache(cache_args) => {
            if let Err(e) =
                solver_config::cache::build_solvers_config_cache(&cache_args.minizinc.minizinc_exe)
                    .await
            {
                logging::error_with_msg!(e.into(), "Failed to build solver cache");
                exit(1);
            }
        }
        Command::Static(args) => {
            let common_args = args.common.clone();

            let orchestrator_result =
                orchestrator::static_parallel_portfolio::StaticParallelPortfolio::new(
                    args,
                    program_cancellation_token.clone(),
                    suspend_and_resume_signal_rx,
                )
                .await
                .map_err(orchestrator::Error::from);

            run(
                orchestrator_result,
                &common_args,
                program_cancellation_token,
            )
            .await;
        }
    }
}

async fn run(
    orchestrator: Result<impl Orchestrator, orchestrator::Error>,
    common_args: &CommonArgs,
    program_cancellation_token: CancellationToken,
) {
    logging::init(common_args.verbosity);

    let result = match orchestrator {
        Ok(orchestrator) => orchestrator.run().await,
        Err(e) => Err(e),
    };

    match result {
        Ok(()) => {}
        Err(orchestrator::Error::Cancelled) => {
            // User cancelled, don't run backup solver
        }
        Err(e) => {
            let cores = common_args.cores;

            logging::error!(e.into());
            logging::error_msg!("Portfolio solver failed, falling back to backup solver");
            tokio::select! {
                _ = program_cancellation_token.cancelled() => {},
                result = run_backup_solver(common_args, cores) => {
                    if let Err(e) = result {
                        logging::error!(e.into());
                        exit(1);
                    }
                }
            }
        }
    }
}
