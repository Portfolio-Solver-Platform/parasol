use std::ffi::OsStr;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::ExitStatus;
use tempfile::NamedTempFile;
use tokio::process::Command;

pub type ObjectiveValue = i64;

#[derive(Debug)]
pub enum ModelParseError {
    MethodParseError(String),
    IoError(std::io::Error),
    RegexError(regex::Error),
    CommandFailed(ExitStatus),
    CommandOutputError(CommandOutputError),
}

#[derive(Debug)]
pub enum CommandOutputError {
    NonJsonOutput,
    JsonIsNotObject,
}

impl From<CommandOutputError> for ModelParseError {
    fn from(value: CommandOutputError) -> Self {
        Self::CommandOutputError(value)
    }
}

impl From<std::io::Error> for ModelParseError {
    fn from(e: std::io::Error) -> Self {
        Self::IoError(e)
    }
}

impl From<regex::Error> for ModelParseError {
    fn from(e: regex::Error) -> Self {
        Self::RegexError(e)
    }
}

#[derive(Debug, Clone, Copy)]
pub enum ObjectiveType {
    Satisfy,
    Minimize,
    Maximize,
}

impl ObjectiveType {
    pub fn is_better(&self, old: Option<ObjectiveValue>, new: ObjectiveValue) -> bool {
        match (self, old) {
            (_, None) => true,
            (Self::Maximize, Some(val)) => val < new,
            (Self::Minimize, Some(val)) => val > new,
            (Self::Satisfy, _) => true,
        }
    }
}

pub fn insert_objective(
    fzn_path: &Path,
    objective_type: &ObjectiveType,
    objective: ObjectiveValue,
) -> Result<NamedTempFile, ()> {
    // NOTE: The FlatZinc grammar always ends with a "solve-item" and all statements end with a ';': https://docs.minizinc.dev/en/latest/fzn-spec.html#grammar
    // TODO: Optimise: don't read the entire file, but only read from the end.
    let content = fs::read_to_string(fzn_path).map_err(|_| ())?;
    let mut statements: Vec<_> = content.split(';').collect();
    let solve_statement = statements.last().ok_or(())?.trim();

    if !solve_statement.starts_with("solve") {
        return Err(());
    }

    let objective_name = solve_statement.split_whitespace().next_back().ok_or(())?; // NOTE: split should never return an empty iterator
    let objective_constraint = get_objective_constraint(objective_type, objective_name, objective)?;

    statements.insert(statements.len() - 1, &objective_constraint);

    let new_content = statements.join(";"); // Add back ';' after split

    let mut file = tempfile::Builder::new()
        .suffix(".fzn")
        .tempfile()
        .map_err(|_| ())?;
    write!(file, "{new_content}").map_err(|_| ())?;
    file.flush().map_err(|_| ())?;

    Ok(file)
}

fn get_objective_constraint(
    objective_type: &ObjectiveType,
    objective_name: &str,
    objective: ObjectiveValue,
) -> Result<String, ()> {
    fn int_le(left: &str, right: &str) -> String {
        format!("constraint int_le({left}, {right});")
    }
    match objective_type {
        ObjectiveType::Satisfy => Err(()),
        ObjectiveType::Minimize => Ok(int_le(objective_name, &objective.to_string())),
        ObjectiveType::Maximize => Ok(int_le(&objective.to_string(), objective_name)),
    }
}

pub async fn get_objective_type(model_path: &Path) -> Result<ObjectiveType, ModelParseError> {
    let output = run_model_interface_cmd(model_path).await?;
    let json: serde_json::Value =
        serde_json::from_str(&output).map_err(|_| CommandOutputError::NonJsonOutput)?;
    let serde_json::Value::Object(object) = json else {
        return Err(CommandOutputError::JsonIsNotObject.into());
    };

    parse_method_from_json_object(object)
}

fn parse_method_from_json_object(
    object: serde_json::Map<String, serde_json::Value>,
) -> Result<ObjectiveType, ModelParseError> {
    let Some(method_json) = object.get("method") else {
        return Err(ModelParseError::MethodParseError(
            "'method' field does not exist".to_owned(),
        ));
    };

    let serde_json::Value::String(method) = method_json else {
        return Err(ModelParseError::MethodParseError(
            "'method' field is not a string".to_owned(),
        ));
    };

    match method.as_str() {
        "min" => Ok(ObjectiveType::Minimize),
        "max" => Ok(ObjectiveType::Maximize),
        "sat" => Ok(ObjectiveType::Satisfy),
        _ => Err(ModelParseError::MethodParseError(
            "Method not recognised".to_owned(),
        )),
    }
}

async fn run_model_interface_cmd(model_path: &Path) -> Result<String, ModelParseError> {
    let mut cmd = get_model_interface_cmd(model_path);
    let output = cmd.output().await?;
    if !output.status.success() {
        return Err(ModelParseError::CommandFailed(output.status));
    }

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

fn get_model_interface_cmd(model_path: &Path) -> Command {
    let mut cmd = Command::new("minizinc");
    cmd.kill_on_drop(true);
    cmd.arg(model_path);
    cmd.arg("--model-interface-only");

    cmd
}
