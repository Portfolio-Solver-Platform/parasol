use clap::{Parser, ValueEnum};
use std::path::PathBuf;

#[derive(Debug, Clone, ValueEnum)]
pub enum OutputMode {
    Dzn,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, ValueEnum)]
pub enum DebugVerbosityLevel {
    Quiet = 0,
    Error = 1,
    Warning = 2,
    Info = 3,
}

#[derive(Parser, Debug, Clone)]
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
    pub cores: Option<usize>,

    #[arg(long, value_enum, default_value = "warning")]
    pub debug_verbosity: DebugVerbosityLevel,
}
