mod ai;
mod args;
mod config;
mod fzn_to_features;
mod model_parser;
mod mzn_to_fzn;
mod scheduler;
mod solver_manager;
mod solver_output;
mod sunny;

use crate::ai::SimpleAi;
use crate::config::Config;
use crate::sunny::sunny;
use args::Args;
use clap::Parser;
use futures::channel::oneshot::Cancellation;
use tokio_util::sync::CancellationToken;

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let args = Args::parse();
    let config = Config::default();
    let token = CancellationToken::new();
    let token_signal = token.clone();

    ctrlc::set_handler(move || {
        token_signal.cancel();
    })
    .expect("Error setting Ctrl-C handler");

    tokio::select! {
        _ = sunny(args, SimpleAi {}, config, token.clone()) => {},
        _ = token.cancelled() => {}
    }
}
