use clap::{Parser, ValueEnum};
use std::path::PathBuf;
#[derive(Debug, Clone, ValueEnum)]
pub enum OutputMode {
    Dzn,
}

#[derive(Parser, Debug)]
#[command(author, version, about)]
pub struct Args {
    pub model: PathBuf,

    pub data: Option<PathBuf>,

    #[arg(long, value_enum, ignore_case = true)]
    pub output_mode: Option<OutputMode>,

    #[arg(long)]
    pub output_objective: bool,

    #[arg(short = 'f')]
    pub ignore_search: bool,

    #[arg(short = 'p')]
    pub threads: Option<usize>,
}
