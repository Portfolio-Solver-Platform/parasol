mod ai;
mod input;
mod scheduler;
mod solver_output;
mod sunny;

use crate::ai::SimpleAi;
use crate::sunny::sunny;
use clap::Parser;
use input::Args;

#[tokio::main]
async fn main() {
    let args = Args::parse();
    sunny(args, SimpleAi {}, 5);
}
