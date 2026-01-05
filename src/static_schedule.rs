use std::path::{Path, PathBuf};

use crate::{
    args::{Args, DebugVerbosityLevel},
    logging,
    scheduler::{Portfolio, SolverInfo},
};

pub async fn static_schedule(args: &Args, cores: usize) -> Result<Portfolio> {
    let schedule = match args.static_schedule_path.as_ref() {
        Some(path) => get_schedule_from_file(path).await?,
        None => default_schedule(cores),
    };

    if args.debug_verbosity >= DebugVerbosityLevel::Warning {
        let schedule_cores = schedule_cores(&schedule);
        if schedule_cores != cores {
            logging::warning!(
                "The static schedule cores ({schedule_cores}) does not match the framework's designated cores ({cores})"
            );
        }
    }

    Ok(schedule)
}

fn schedule_cores(schedule: &Portfolio) -> usize {
    schedule.iter().map(|solver_info| solver_info.cores).sum()
}

async fn get_schedule_from_file(path: &Path) -> Result<Portfolio> {
    let contents = tokio::fs::read_to_string(path)
        .await
        .map_err(|e| Error::FileError {
            path: path.to_path_buf(),
            source: e,
        })?;
    parse_schedule(&contents).map_err(Into::into)
}

pub fn parse_schedule(s: &str) -> std::result::Result<Portfolio, ParseError> {
    s.lines()
        .filter(|line| !line.is_empty())
        .map(parse_schedule_line)
        .collect()
}

fn parse_schedule_line(line: &str) -> std::result::Result<SolverInfo, ParseError> {
    let (solver, cores_str) =
        line.split_once(',')
            .ok_or_else(|| ParseError::LineDoesNotContainComma {
                line: line.to_owned(),
            })?;

    let cores = cores_str
        .parse::<usize>()
        .map_err(|_| ParseError::CoresNotANumber {
            line: line.to_owned(),
            cores_str: cores_str.to_owned(),
        })?;

    Ok(SolverInfo::new(solver.to_owned(), cores))
}

fn default_schedule(cores: usize) -> Portfolio {
    vec![SolverInfo::new("cp-sat".to_string(), cores)]
}

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("IO failed when reading file '{path}'")]
    FileError {
        path: PathBuf,
        #[source]
        source: tokio::io::Error,
    },
    #[error("Parsing of the static schedule failed")]
    ParseError(#[from] ParseError),
}

#[derive(Debug, thiserror::Error)]
pub enum ParseError {
    #[error("Schedule line does not contain a ',': '{line}'")]
    LineDoesNotContainComma { line: String },
    #[error(
        "A solver's cores in the schedule is not an unsigned integer: '{cores_str}' on the following line: {line}"
    )]
    CoresNotANumber { line: String, cores_str: String },
}
