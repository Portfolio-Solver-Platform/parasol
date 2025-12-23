use crate::model_parser::{ObjectiveType, ObjectiveValue};
use async_tempfile::TempFile;
use serde_json::json;
use std::path::{Path, PathBuf};
use tokio::fs::File;
use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt, SeekFrom};
use uuid::Uuid;

pub async fn insert_objective(
    fzn_path: &Path,
    objective_type: &ObjectiveType,
    objective: ObjectiveValue,
) -> Result<TempFile> {
    // NOTE: The FlatZinc grammar always ends with a "solve-item" and all statements end with a ';': https://docs.minizinc.dev/en/latest/fzn-spec.html#grammar
    let mut file = File::open(fzn_path)
        .await
        .map_err(|e| Error::ReadFznFile(fzn_path.to_path_buf(), e))?;

    let file_len = file.metadata().await?.len();
    if file_len == 0 {
        return Err(Error::NoStatements(String::new()));
    }

    // We look for the second ';' from the end.
    // the structure is expected to be: "...; solve ...;"
    const BUFFER_SIZE: usize = 1024;
    let mut buffer = [0u8; BUFFER_SIZE];
    let mut cursor = file_len;
    let mut solve_start_pos = 0;
    let mut semi_colon_count = 0;
    let mut found_split = false;

    while cursor > 0 {
        let read_size = std::cmp::min(cursor as usize, BUFFER_SIZE);
        cursor -= read_size as u64;

        file.seek(SeekFrom::Start(cursor)).await?;
        file.read_exact(&mut buffer[..read_size]).await?;

        // scan bufer backwards
        for i in (0..read_size).rev() {
            if buffer[i] == b';' {
                semi_colon_count += 1;
                if semi_colon_count == 2 {
                    solve_start_pos = cursor + (i as u64) + 1;
                    found_split = true;
                    break;
                }
            }
        }

        if found_split {
            break;
        }

        if cursor == 0 && semi_colon_count <= 1 {
            solve_start_pos = 0;
        }
    }

    file.seek(SeekFrom::Start(solve_start_pos)).await?;
    let mut solve_bytes = Vec::new();
    file.read_to_end(&mut solve_bytes).await?;

    let solve_statement = String::from_utf8_lossy(&solve_bytes);
    let solve_trimmed = solve_statement.trim();

    if !solve_trimmed.starts_with("solve") {
        return Err(Error::LastStatementNotSolve(solve_statement.to_string()));
    }

    let solve_content_only = solve_trimmed.strip_suffix(';').unwrap_or(solve_trimmed);
    let objective_name = solve_content_only
        .split_whitespace()
        .next_back()
        .ok_or(Error::SplitReturnedEmptyIterator)?;

    let objective_constraint = get_objective_constraint(objective_type, objective_name, objective)?;

    let uuid = Uuid::new_v4();
    let mut temp_file = TempFile::new_with_name(format!("temp-{uuid}.fzn")).await?;

    file.seek(SeekFrom::Start(0)).await?;
    let mut limited_reader = file.take(solve_start_pos);
    tokio::io::copy(&mut limited_reader, &mut temp_file).await?;

    temp_file.write_all(objective_constraint.as_bytes()).await?;
    if !objective_constraint.trim_end().ends_with(';') {
        temp_file.write_all(b";").await?;
    }

    temp_file.write_all(&solve_bytes).await?;

    temp_file.flush().await?;

    Ok(temp_file)
}

pub async fn insert_objective_json(
    json_path: &Path,
    objective_type: &ObjectiveType,
    objective: ObjectiveValue,
) -> Result<TempFile> {
    let mut file = File::open(json_path)
        .await
        .map_err(|e| Error::ReadFznFile(json_path.to_path_buf(), e))?;

    let mut content = String::new();
    file.read_to_string(&mut content).await?;
    if content.trim().is_empty() {
        return Err(Error::NoStatements(String::new()));
    }

    let objective_name = get_objective_name_from_json(objective_type, &content)?;

    let mut json: serde_json::Value =
        serde_json::from_str(&content).map_err(|e| Error::JsonParse(e))?;

    let constraint = get_objective_constraint_json_value(objective_type, &objective_name, objective)?;

    let constraints = json
        .get_mut("constraints")
        .ok_or(Error::JsonConstraintsNotFound)?;
    let serde_json::Value::Array(constraints) = constraints else {
        return Err(Error::JsonConstraintsNotFound);
    };
    constraints.push(constraint);

    let uuid = Uuid::new_v4();
    let mut temp_file = TempFile::new_with_name(format!("temp-{uuid}.fzn.json")).await?;
    temp_file.write_all(content.as_bytes()).await?;
    temp_file.flush().await?;
    Ok(temp_file)
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

fn get_objective_constraint_json(
    objective_type: &ObjectiveType,
    objective_name: &str,
    objective: ObjectiveValue,
) -> Result<String> {
    let (left, right) = match objective_type {
        ObjectiveType::Satisfy => return Err(Error::GetObjectiveOnSatisfyType),
        ObjectiveType::Minimize => (json!(objective_name), json!(objective)),
        ObjectiveType::Maximize => (json!(objective), json!(objective_name)),
    };
    let constraint = json!({"id": "int_le", "args": [left, right]});
    Ok(format!("\n{}", constraint.to_string()))
}

fn get_objective_constraint_json_value(
    objective_type: &ObjectiveType,
    objective_name: &str,
    objective: ObjectiveValue,
) -> Result<serde_json::Value> {
    let (left, right) = match objective_type {
        ObjectiveType::Satisfy => return Err(Error::GetObjectiveOnSatisfyType),
        ObjectiveType::Minimize => (json!(objective_name), json!(objective)),
        ObjectiveType::Maximize => (json!(objective), json!(objective_name)),
    };
    Ok(json!({"id": "int_le", "args": [left, right]}))
}

fn get_objective_name_from_json(
    objective_type: &ObjectiveType,
    content: &str,
) -> Result<String> {
    if matches!(objective_type, ObjectiveType::Satisfy) {
        return Err(Error::GetObjectiveOnSatisfyType);
    }

    let json: serde_json::Value =
        serde_json::from_str(content).map_err(|e| Error::JsonParse(e))?;

    let solve = json
        .get("solve")
        .or_else(|| json.get("Solve"))
        .ok_or(Error::JsonObjectiveNotFound)?;

    let objective = solve
        .get("objective")
        .or_else(|| solve.get("objective_name"))
        .or_else(|| solve.get("objectiveName"))
        .ok_or(Error::JsonObjectiveNotFound)?;

    objective
        .as_str()
        .map(|s| s.to_owned())
        .ok_or(Error::JsonObjectiveNotFound)
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Failed to read FlatZinc file: {0}")]
    ReadFznFile(PathBuf, #[source] tokio::io::Error),
    #[error("FlatZinc contains no statements: {0}")]
    NoStatements(String),
    #[error("Failed to find JSON constraints array")]
    JsonConstraintsNotFound,
    #[error("Failed to parse JSON: {0}")]
    JsonParse(#[from] serde_json::Error),
    #[error("Failed to find objective name in JSON")]
    JsonObjectiveNotFound,
    #[error("the last statement is not a solve statement: {0}")]
    LastStatementNotSolve(String),
    #[error("split returned an empty iterator (should be impossible)")]
    SplitReturnedEmptyIterator,
    #[error(transparent)]
    TempFile(#[from] async_tempfile::Error),
    #[error(transparent)]
    Io(#[from] tokio::io::Error),
    #[error("tried to create the objective constraint on a satisfaction problem")]
    GetObjectiveOnSatisfyType,
}

pub type Result<T> = std::result::Result<T, Error>;
