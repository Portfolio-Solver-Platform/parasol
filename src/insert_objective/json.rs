use crate::model_parser::{ObjectiveType, ObjectiveValue};
use async_tempfile::TempFile;
use serde_json::json;
use std::path::{Path, PathBuf};
use tokio::fs::File;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use uuid::Uuid;

pub async fn insert_objective(
    json_path: &Path,
    objective_type: &ObjectiveType,
    objective: ObjectiveValue,
) -> Result<TempFile> {
    let mut file = File::open(json_path)
        .await
        .map_err(|e| Error::ReadFile(json_path.to_path_buf(), e))?;

    let mut content = String::new();
    file.read_to_string(&mut content).await?;
    if content.trim().is_empty() {
        return Err(Error::NoStatements(content));
    }

    let mut json: serde_json::Value = serde_json::from_str(&content).map_err(Error::JsonParse)?;

    let objective_name = get_objective_name_from_json(objective_type, &json)?;

    let constraint =
        get_objective_constraint_json_value(objective_type, &objective_name, objective)?;

    let constraints = json
        .get_mut("constraints")
        .ok_or(Error::ConstraintsNotFound)?;
    let serde_json::Value::Array(constraints) = constraints else {
        return Err(Error::InvalidConstraintsType);
    };
    constraints.push(constraint);

    let uuid = Uuid::new_v4();
    let mut temp_file = TempFile::new_with_name(format!("temp-{uuid}.fzn.json")).await?;
    temp_file.write_all(content.as_bytes()).await?;
    temp_file.flush().await?;
    Ok(temp_file)
}

fn get_objective_name_from_json(
    objective_type: &ObjectiveType,
    json: &serde_json::Value,
) -> Result<String> {
    if matches!(objective_type, ObjectiveType::Satisfy) {
        return Err(Error::GetObjectiveOnSatisfyType);
    }

    let solve = json
        .get("solve")
        .or_else(|| json.get("Solve"))
        .ok_or_else(|| Error::ObjectiveNotFound("'solve' field was not found".to_owned()))?;

    let objective = solve
        .get("objective")
        .or_else(|| solve.get("objective_name"))
        .or_else(|| solve.get("objectiveName"))
        .ok_or_else(|| {
            Error::ObjectiveNotFound(
                "'objective' field was not found inside the solve field".to_owned(),
            )
        })?;

    objective
        .as_str()
        .map(|s| s.to_owned())
        .ok_or_else(|| Error::ObjectiveNotFound(format!("objective is not a string: {objective}")))
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

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("tried to create the objective constraint on a satisfaction problem")]
    GetObjectiveOnSatisfyType,
    #[error("failed to read the JSON file: {0}")]
    ReadFile(PathBuf, #[source] tokio::io::Error),
    #[error("failed to find the constraints array")]
    ConstraintsNotFound,
    #[error("the constraints field is not an array")]
    InvalidConstraintsType,
    #[error("failed to parse JSON: {0}")]
    JsonParse(#[from] serde_json::Error),
    #[error("failed to find objective name in JSON: {0}")]
    ObjectiveNotFound(String),
    #[error(transparent)]
    Io(#[from] tokio::io::Error),
    #[error("JSON contains no statements: {0}")]
    NoStatements(String),
    #[error(transparent)]
    TempFile(#[from] async_tempfile::Error),
}

pub type Result<T> = std::result::Result<T, Error>;
