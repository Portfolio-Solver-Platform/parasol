mod ai;
mod args;
mod backup_solvers;
mod config;
mod fzn_to_features;
mod insert_objective;
mod logging;
mod model_parser;
mod mzn_to_fzn;
mod process_tree;
mod scheduler;
mod solver_manager;
mod solver_output;
mod static_schedule;
mod sunny;

use std::process::exit;

use crate::ai::SimpleAi;
use crate::args::{Ai, parse_ai_config};
use crate::backup_solvers::run_backup_solver;
use crate::config::Config;
use crate::sunny::sunny;
use args::Args;
use clap::Parser;
use tokio_util::sync::CancellationToken;

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let args = Args::parse();
    logging::init(args.debug_verbosity);

    // // Pin the framework to the first core when --pin-cores is enabled
    // if args.pin_cores {
    //     if let Some(core_ids) = core_affinity::get_core_ids() {
    //         if let Some(first_core) = core_ids.first() {
    //             core_affinity::set_for_current(*first_core);
    //         }
    //     }
    // }
    let config = Config::new(&args);
    let token = CancellationToken::new();
    let token_signal = token.clone();

    ctrlc::set_handler(move || {
        token_signal.cancel();
    })
    .expect("Error setting Ctrl-C handler");

    let cores = args.cores.unwrap_or(2);

    let result = match args.ai {
        Ai::Simple => tokio::select! {
            result = sunny(&args, SimpleAi {}, config, token.clone()) => result,
            _ = token.cancelled() => Ok(())
        },
        Ai::CommandLine => {
            let ai_config = parse_ai_config(args.ai_config.as_deref());
            let Some(command) = ai_config.get("command") else {
                logging::error_msg!(
                    "'command' not provided in AI configuration when basic commandline AI has been specified"
                );
                exit(1);
            };

            let ai = crate::ai::commandline::Ai::new(command.clone(), args.debug_verbosity);
            tokio::select! {
                result = sunny(&args, ai, config, token.clone()) => result,
                _ = token.cancelled() => Ok(())
            }
        }
    };

    if result.is_err() {
        logging::error_msg!("Portfolio solver failed, falling back to backup solver");
        run_backup_solver(&args, cores).await;
    }
}
