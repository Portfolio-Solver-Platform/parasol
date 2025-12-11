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

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let args = Args::parse();
    let config = Config::default();
    sunny(args, SimpleAi {}, config).await;
}
