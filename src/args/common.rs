use clap::{Parser, ValueEnum};
use std::{
    collections::HashMap,
    fmt,
    path::{Path, PathBuf},
    process::exit,
};

use crate::{ai::SimpleAi, logging, mzn_to_fzn::compilation_scheduler::SolverPriority};

#[derive(Parser, Debug, Clone)]
#[command(author, version, about)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[allow(clippy::large_enum_variant)]
#[derive(clap::Subcommand, Debug, Clone)]
pub enum Command {
    /// Run with the static parallel portfolio orchestrator
    Static(StaticArgs),
    /// Build the solver config cache and exit
    BuildSolverCache(BuildSolverCacheArgs),
}

const ORCHESTRATOR_HEADING: &str = "Orchestrator";
#[derive(clap::Args, Debug, Clone)]
pub struct StaticArgs {
    #[command(flatten)]
    pub ai: AiArgs,

    // === Timing ===
    /// The minimum time (in seconds) the initial static schedule will be run before using the AI's schedule
    #[arg(long, default_value = "5", help_heading = ORCHESTRATOR_HEADING)]
    pub static_runtime: u64,

    /// Number of seconds between how often the solvers are restarted to share the upper bound found
    #[arg(long, default_value = "5", help_heading = ORCHESTRATOR_HEADING)]
    pub restart_interval: u64,

    /// The time (in seconds) before we skip extracting the features and stop using the static schedule, and instead use the timeout schedule.
    /// Warning: if static_runtime set higher than feature_timeout, then static_runtime will be used instead.
    #[arg(long, default_value = "10", help_heading = ORCHESTRATOR_HEADING)]
    pub feature_timeout: u64,

    /// The path to the static schedule file.
    /// The file needs to be a CSV (without a header) in the format of `<solver>,<cores>`.
    /// If not provided, a default static schedule will be used.
    #[arg(long, help_heading = ORCHESTRATOR_HEADING)]
    pub static_schedule: Option<PathBuf>,

    /// The path to the timeout schedule file. This schedule will be run if the compilation or the feature extraction takes too long.
    /// The file needs to be a CSV (without a header) in the format of `<solver>,<cores>`.
    /// If not provided, a default timeout schedule will be used.
    #[arg(long, help_heading = ORCHESTRATOR_HEADING)]
    pub timeout_schedule: Option<PathBuf>,

    #[command(flatten)]
    pub common: CommonArgs,
}

const AI_HELP_HEADING: &str = "AI Configuration";
#[derive(clap::Args, Debug, Clone)]
pub struct AiArgs {
    // === AI Configuration ===
    /// The AI used to determine the solver schedule dynamically
    #[arg(
        long = "ai",
        short = 'a',
        value_enum,
        default_value = "simple",
        help_heading = AI_HELP_HEADING
    )]
    pub kind: Ai,

    /// Configuration for the AI. This is only relevant when the AI documentation says
    /// configuration should be added here.
    /// The format is: <key1>=<value1>,<key2>=<value2>,...
    #[arg(long = "ai-config", help_heading = AI_HELP_HEADING)]
    pub config: Option<String>,

    /// The ID of the solver that should be used for the MiniZinc to FlatZinc conversion for feature extraction.
    #[arg(long, help_heading = AI_HELP_HEADING, default_value = crate::solvers::GECODE_ID)]
    pub feature_extraction_solver_id: String,
}

#[derive(clap::Args, Debug, Clone)]
pub struct CommonArgs {
    // === Input Files ===
    /// The MiniZinc model file
    pub model: PathBuf,

    /// The MiniZinc data file corresponding to the model file
    pub data: Option<PathBuf>,

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

    /// When a solution is found, outputs the ID of the solver that found the solution
    #[arg(long, help_heading = "Output")]
    pub output_solver: bool,

    // === Execution ===
    /// The number of cores parasol should use
    #[arg(long, short = 'p', default_value = "2", help_heading = "Execution")]
    pub cores: usize,

    /// Pin the java based solvers to specific CPU cores as the java runtime makes them use extra cpu. The current solvers it pins is yuck and choco. Note: this degrades the performance of the two solvers
    #[arg(long, help_heading = "Execution")]
    pub pin_java_solvers: bool,

    /// Whether it should kill solvers if you are nearing the system memory limit
    #[arg(long, help_heading = "Execution")]
    pub enforce_memory: bool,

    /// Whether to discover solvers at startup or load from a pre-generated cache. Loading from cache is faster.
    #[arg(long, default_value = "discover", help_heading = "Execution")]
    pub solver_config_mode: SolverConfigMode,

    /// Optional path to a compilation priority configuration CSV file.
    /// When possible without a runtime cost, the problem model and data will be compiled for the
    /// solvers in this file in the order they are given.
    /// This may be useful for attempting to pre-compile the model for solvers that an AI might choose,
    /// before the AI has decided on a portfolio.
    /// The path should be to a text file with one solver ID per line.
    /// Lines starting with # are treated as comments, and empty lines are ignored.
    #[arg(long, help_heading = "Execution")]
    pub compilation_priority: Option<PathBuf>,

    // === Paths ===
    #[command(flatten)]
    pub minizinc: MiniZincArgs,

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
    /// Don't use an AI, i.e., only use the static schedule
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

pub async fn read_compilation_priority(path: &Path) -> tokio::io::Result<SolverPriority> {
    let s = tokio::fs::read_to_string(path).await?;
    Ok(parse_compilation_priority(&s))
}

pub fn parse_compilation_priority(s: &str) -> SolverPriority {
    let solvers = s
        .lines()
        .map(str::trim)
        .filter(|line| !line.starts_with('#') && !line.is_empty())
        .map(|line| line.to_string())
        .collect::<Vec<_>>();

    logging::info!(
        "parsed the following solver compilation priority (first has highest priority): {solvers:?}"
    );
    SolverPriority::from_descending_priority(solvers)
}

pub fn unpack_ai(ai: &AiArgs) -> Result<Option<Box<dyn crate::ai::Ai + Send>>, UnpackAiError> {
    Ok(match ai.kind {
        Ai::None => None,
        Ai::Simple => Some(Box::new(SimpleAi {})),
        Ai::CommandLine => {
            let ai_config = parse_ai_config(ai.config.as_deref());
            let Some(command) = ai_config.get("command") else {
                return Err(UnpackAiError::CommandLineAiConfigMissing(
                    "command".to_owned(),
                ));
            };

            Some(Box::new(crate::ai::commandline::Ai::new(command.clone())))
        }
    })
}

#[derive(Debug, thiserror::Error)]
pub enum UnpackAiError {
    #[error("'{0}' not provided in AI configuration when commandline AI has been specified")]
    CommandLineAiConfigMissing(String),
}
