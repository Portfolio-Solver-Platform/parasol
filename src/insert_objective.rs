use std::path::{Path, PathBuf};

use async_tempfile::TempFile;
use tokio::io::AsyncWriteExt;
use uuid::Uuid;

use crate::model_parser::{ObjectiveType, ObjectiveValue};

pub async fn insert_objective(
    fzn_path: &Path,
    objective_type: &ObjectiveType,
    objective: ObjectiveValue,
) -> Result<TempFile> {
    // NOTE: The FlatZinc grammar always ends with a "solve-item" and all statements end with a ';': https://docs.minizinc.dev/en/latest/fzn-spec.html#grammar
    // TODO: Optimise: don't read the entire file, but only read from the end.
    let content = tokio::fs::read_to_string(fzn_path)
        .await
        .map_err(|e| Error::ReadFznFile(fzn_path.to_path_buf(), e))?;
    let mut statements: Vec<_> = content.split(';').collect();
    let solve_statement = statements
        .last()
        .ok_or_else(|| Error::NoStatements(content.clone()))?
        .trim();

    if !solve_statement.starts_with("solve") {
        return Err(Error::LastStatementNotSolve(solve_statement.to_owned()));
    }

    let objective_name = solve_statement
        .split_whitespace()
        .next_back()
        .ok_or(Error::SplitReturnedEmptyIterator)?; // NOTE: split should never return an empty iterator
    let objective_constraint = get_objective_constraint(objective_type, objective_name, objective)?;

    statements.insert(statements.len() - 1, &objective_constraint);

    let new_content = statements.join(";"); // Add back ';' after split

    let uuid = Uuid::new_v4();
    let mut file = TempFile::new_with_name(format!("temp-{uuid}.fzn")).await?;

    file.write_all(new_content.as_bytes()).await?;
    file.flush().await?;

    Ok(file)
}

fn get_objective_constraint(
    objective_type: &ObjectiveType,
    objective_name: &str,
    objective: ObjectiveValue,
) -> Result<String> {
    fn int_le(left: &str, right: &str) -> String {
        format!("constraint int_le({left}, {right});")
    }
    match objective_type {
        ObjectiveType::Satisfy => Err(Error::GetObjectiveOnSatisfyType),
        ObjectiveType::Minimize => Ok(int_le(objective_name, &objective.to_string())),
        ObjectiveType::Maximize => Ok(int_le(&objective.to_string(), objective_name)),
    }
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Failed to read FlatZinc file: {0}")]
    ReadFznFile(PathBuf, #[source] tokio::io::Error),
    #[error("FlatZinc contains no statements: {0}")]
    NoStatements(String),
    #[error("the last statement is not a solve statement: {0}")]
    LastStatementNotSolve(String),
    #[error("split returned an empty iterator (should be impossible)")]
    SplitReturnedEmptyIterator,
    #[error(transparent)]
    TempFile(#[from] async_tempfile::Error),
    #[error(transparent)]
    Io(#[from] tokio::io::Error),
    #[error("tried to convert create the objective constraint on a satisfaction problem")]
    GetObjectiveOnSatisfyType,
}

pub type Result<T> = std::result::Result<T, Error>;
