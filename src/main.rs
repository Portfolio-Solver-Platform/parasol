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
mod process_tree;
mod scheduler;
mod signal_handler;
mod solver_config;
mod solver_manager;
mod solver_output;
mod solvers;
mod static_schedule;
mod sunny;

use std::process::exit;
use std::sync::Arc;

use crate::ai::SimpleAi;
use crate::args::{Ai, Cli, Command, RunArgs, parse_ai_config};
use crate::backup_solvers::run_backup_solver;
use crate::config::Config;
use crate::sunny::sunny;
use clap::Parser;
use tokio_util::sync::CancellationToken;

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let cli = Cli::parse();

    match cli.command {
        Command::BuildSolverCache(cache_args) => {
            if let Err(e) =
                solver_config::cache::build_solvers_config_cache(&cache_args.minizinc.minizinc_exe)
                    .await
            {
                logging::error_msg!("Failed to build solver cache: {e}");
                exit(1);
            }
        }
        Command::Run(args) => run(args).await,
    }
}

async fn run(args: RunArgs) {
    logging::init(args.verbosity);

    let solvers = solver_config::load(&args.solver_config_mode, &args.minizinc.minizinc_exe).await;

    let config = Config::new(&args, &solvers);
    let token = CancellationToken::new();

    let cores = args.cores;

    let result = match args.ai {
        Ai::None => {
            sunny(
                &args,
                None::<SimpleAi>,
                config,
                Arc::new(solvers),
                token.clone(),
            )
            .await
        }
        Ai::Simple => {
            sunny(
                &args,
                Some(SimpleAi {}),
                config,
                Arc::new(solvers),
                token.clone(),
            )
            .await
        }
        Ai::CommandLine => {
            let ai_config = parse_ai_config(args.ai_config.as_deref());
            let Some(command) = ai_config.get("command") else {
                logging::error_msg!(
                    "'command' not provided in AI configuration when basic commandline AI has been specified"
                );
                exit(1);
            };

            let ai = crate::ai::commandline::Ai::new(command.clone(), args.verbosity);
            sunny(&args, Some(ai), config, Arc::new(solvers), token.clone()).await
        }
    };

    match result {
        Ok(()) => {}
        Err(sunny::Error::Cancelled) => {
            // User cancelled, don't run backup solver
        }
        Err(e) => {
            logging::error!(e.into());
            logging::error_msg!("Portfolio solver failed, falling back to backup solver");
            tokio::select! {
                _ = token.cancelled() => {},
                result = run_backup_solver(&args, cores) => {
                    if let Err(e) = result {
                        logging::error!(e.into());
                        exit(1);
                    }
                }
            }
        }
    }
}
