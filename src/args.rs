use clap::{Parser, ValueEnum};
use std::{collections::HashMap, fmt, path::PathBuf, process::exit};

use crate::logging;

#[derive(Parser, Debug, Clone)]
#[command(author, version, about)]
pub struct Args {
    /// The MiniZinc model file
    pub model: PathBuf,

    /// The MiniZinc data file corresponding to the model file
    pub data: Option<PathBuf>,

    /// The AI used to determine the solver schedule dynamically
    #[arg(long, value_enum, default_value = "simple")]
    pub ai: Ai,
    /// Configuration for the AI. This is only relevant when the AI documentation says
    /// configuration should be added here.
    /// The format is: <key1>=<value1>,<key2>=<value2>,...
    #[arg(long)]
    pub ai_config: Option<String>,

    #[arg(long, value_enum, ignore_case = true)]
    pub output_mode: Option<OutputMode>,

    /// Whether to output the objective in the `_objective` format
    #[arg(long)]
    pub output_objective: bool,

    #[arg(long, short = 'f')]
    pub ignore_search: bool,

    /// The number of cores the framework should use
    #[arg(short = 'p')]
    pub cores: Option<usize>,

    #[arg(long, value_enum, default_value = "warning")]
    pub debug_verbosity: DebugVerbosityLevel,

    /// The path to the minizinc executable.
    #[arg(long, default_value = "minizinc")]
    pub minizinc_exe: PathBuf,

    /// The path to the static schedule file.
    /// The file needs to be a CSV (without a header) in the format of `<solver>,<cores>`.
    /// If not provided, a default static schedule will be used.
    #[arg(long)]
    pub static_schedule_path: Option<PathBuf>,

    /// Pin solver processes to specific CPU cores
    #[arg(long)]
    pub pin_cores: bool,
}

#[derive(Debug, Clone, ValueEnum)]
pub enum Ai {
    /// Use the simple AI
    Simple,
    /// Use the command line AI. MUST specify ai-config with `command=<command-path>`.
    CommandLine,
}

#[derive(Debug, Clone, ValueEnum)]
pub enum OutputMode {
    Dzn,
}

impl fmt::Display for OutputMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            OutputMode::Dzn => write!(f, "dzn"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, ValueEnum)]
pub enum DebugVerbosityLevel {
    Quiet = 0,
    Error = 1,
    Warning = 2,
    Info = 3,
}

pub fn parse_ai_config(config: Option<&str>) -> HashMap<String, String> {
    config
        .unwrap_or_default()
        .split(',')
        .map(|key_value| {
            let Some((key, value)) = key_value.split_once('=') else {
                logging::error_msg!("Key-value pair is missing '=' in the AI configuration. The key-value: '{key_value}'");
                exit(1);
            };
            (key.to_owned(), value.to_owned())
        })
        .collect()
}
