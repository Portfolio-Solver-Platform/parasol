use clap::{Parser, ValueEnum};
use std::{collections::HashMap, fmt, path::PathBuf, process::exit};

use crate::logging;

#[derive(Parser, Debug, Clone)]
#[command(author, version, about)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[allow(clippy::large_enum_variant)]
#[derive(clap::Subcommand, Debug, Clone)]
pub enum Command {
    /// Run the parasol framework
    Run(RunArgs),
    /// Build the solver config cache and exit
    BuildSolverCache(BuildSolverCacheArgs),
}

#[derive(clap::Args, Debug, Clone)]
pub struct RunArgs {
    // === Input Files ===
    /// The MiniZinc model file
    pub model: PathBuf,

    /// The MiniZinc data file corresponding to the model file
    pub data: Option<PathBuf>,

    /// Optional path to a solver compiler priority configuration file
    #[arg(long, help_heading = "Input Files")]
    pub solver_compiler_priority: Option<PathBuf>,

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
        default_value = "dzn",
        ignore_case = true,
        help_heading = "Output"
    )]
    pub output_mode: OutputMode,

    /// This is only there for the competition, it will always output objective
    #[arg(long, help_heading = "Output")]
    pub output_objective: bool,

    // === Execution ===
    /// The number of cores parasol should use
    #[arg(short = 'p', default_value = "2", help_heading = "Execution")]
    pub cores: usize,

    /// Pin the yuck solver processes to specific CPU cores. The yuck is written in java, hence it can use more cpu than it was given. This guarantees that we never use more than the allowed cpu (except for printing to stdout)
    #[arg(long, help_heading = "Execution")]
    pub pin_yuck: bool,

    /// Enable free search for all solvers
    #[arg(long, short = 'f', help_heading = "Execution")]
    pub ignore_search: bool,

    /// The ID of the solver that should be used for the MiniZinc to FlatZinc conversion for feature extraction.
    #[arg(long, help_heading = "Execution", default_value = crate::solvers::GECODE_ID)]
    pub feature_extraction_solver_id: String,

    /// Whether to discover solvers at startup or load from a pre-generated cache. Loading from cache is faster.
    #[arg(long, default_value = "discover", help_heading = "Execution")]
    pub solver_config_mode: SolverConfigMode,

    /// Whether it should kill solvers if you are nearing the system memory limit
    #[arg(long, help_heading = "Execution")]
    pub enforce_memory: bool,

    // === Timing ===
    /// The minimum time (in seconds) the initial static schedule will be run before using the AI's schedule
    #[arg(long, default_value = "5", help_heading = "Timing")]
    pub static_runtime: u64,

    /// Number of seconds between how often the solvers are restarted to share the upper bound found
    #[arg(long, default_value = "7", help_heading = "Timing")]
    pub restart_interval: u64,

    /// The time (in seconds) before we skip extracting the features and stop using the static schedule, and instead use the timeout schedule.
    /// Warning: if static_runtime set higher than feature_timeout, then static_runtime will be used instead.
    #[arg(long, default_value = "10", help_heading = "Timing")]
    pub feature_timeout: u64,

    // === Paths ===
    #[command(flatten)]
    pub minizinc: MiniZincArgs,

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

#[derive(clap::Args, Debug, Clone)]
pub struct MiniZincArgs {
    /// The path to the minizinc executable.
    #[arg(long, default_value = "minizinc", help_heading = "Paths")]
    pub minizinc_exe: PathBuf,
}

#[derive(clap::Args, Debug, Clone)]
pub struct BuildSolverCacheArgs {
    #[command(flatten)]
    pub minizinc: MiniZincArgs,
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
}

impl fmt::Display for OutputMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            OutputMode::Dzn => write!(f, "dzn"),
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

#[derive(Debug, Clone, ValueEnum)]
pub enum SolverConfigMode {
    /// Use the cached solver configs (The cache must be generated beforehand)
    Cache,
    /// Run automatic solver discovery to find solver configs.
    Discover,
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
