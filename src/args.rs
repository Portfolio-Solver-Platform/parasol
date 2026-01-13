use clap::{Parser, ValueEnum};
use std::{collections::HashMap, fmt, path::PathBuf, process::exit};

use crate::logging;

#[derive(Parser, Debug, Clone)]
#[command(author, version, about)]
pub struct Args {
    // === Input Files ===
    /// The MiniZinc model file
    pub model: PathBuf,

    /// The MiniZinc data file corresponding to the model file
    pub data: Option<PathBuf>,

    // === AI Configuration ===
    /// The AI used to determine the solver schedule dynamically
    #[arg(
        long,
        short = 'a',
        value_enum,
        default_value = "simple",
        help_heading = "AI Configuration"
    )]
    pub ai: Ai,

    /// Configuration for the AI. This is only relevant when the AI documentation says
    /// configuration should be added here.
    /// The format is: <key1>=<value1>,<key2>=<value2>,...
    #[arg(long, help_heading = "AI Configuration")]
    pub ai_config: Option<String>,

    // === Output ===
    #[arg(
        long,
        short = 'o',
        value_enum,
        ignore_case = true,
        help_heading = "Output"
    )]
    pub output_mode: Option<OutputMode>,

    /// Whether to output the objective in the `_objective` format
    #[arg(long, help_heading = "Output")]
    pub output_objective: bool,

    // === Execution ===
    /// The number of cores the framework should use
    #[arg(short = 'p', default_value = "2", help_heading = "Execution")]
    pub cores: usize,

    /// Pin solver processes to specific CPU cores. This garantuees that we never use more than the allowed cpu (except for printing to stdout)
    #[arg(long, help_heading = "Execution")]
    pub pin_cores: bool,

    /// Enable free search for all solvers
    #[arg(long, short = 'f', help_heading = "Execution")]
    pub ignore_search: bool,

    // === Timing ===
    /// The minimum time (in seconds) the initial static schedule will be run before using the AI's schedule
    #[arg(long, default_value = "5", help_heading = "Timing")]
    pub static_runtime: u64,

    /// Number of seconds between how often the solvers are restarted to share the upper bound found
    #[arg(long, default_value = "7", help_heading = "Timing")]
    pub restart_interval: u64,

    /// The time (in seconds) before we skip extracting the features and stop using the static schedule, and instead use the timeout schedule
    #[arg(long, default_value = "10", help_heading = "Timing")]
    pub feature_timeout: u64,

    // === Paths ===
    /// The path to the minizinc executable.
    #[arg(long, default_value = "minizinc", help_heading = "Paths")]
    pub minizinc_exe: PathBuf,

    /// The path to the static schedule file.
    /// The file needs to be a CSV (without a header) in the format of `<solver>,<cores>`.
    /// If not provided, a default static schedule will be used.
    #[arg(long, help_heading = "Paths")]
    pub static_schedule: Option<PathBuf>,

    /// The path to the timeout schedule file. This schedule will be run if the compilation or the feature extraction takes too long
    /// The file needs to be a CSV (without a header) in the format of `<solver>,<cores>`.
    /// If not provided, a default timeout schedule will be used.
    #[arg(long, help_heading = "Paths")]
    pub timeout_schedule: Option<PathBuf>,

    // === Debugging ===
    #[arg(
        long,
        short = 'v',
        value_enum,
        default_value = "warning",
        help_heading = "Debugging"
    )]
    pub verbosity: Verbosity,
}

#[derive(Debug, Clone, ValueEnum)]
pub enum Ai {
    /// Dont use an AI, aka. only use the static schedule
    None,
    /// Use the simple AI
    Simple,
    /// Use the command line AI. MUST specify ai-config with `command=<command-path>`.
    CommandLine,
}

#[derive(Debug, Clone, ValueEnum)]
pub enum OutputMode {
    Dzn,
    Json,
}

impl fmt::Display for OutputMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            OutputMode::Dzn => write!(f, "dzn"),
            OutputMode::Json => write!(f, "json"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, ValueEnum)]
pub enum Verbosity {
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
