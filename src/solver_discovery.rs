use std::{
    collections::HashSet,
    path::{Path, PathBuf},
    process::ExitStatus,
};

use serde_json::{Map, Value};
use tokio::process::Command;

use crate::logging;

pub struct Solver {
    name: String,
    executable: Executable,
    supported_std_flags: SupportedStdFlags,
    input_type: SolverKind,
}

pub struct SupportedStdFlags {
    a: bool,
    i: bool,
    f: bool,
    p: bool,
}

pub enum SolverKind {
    Fzn,
    Json,
}

pub struct Executable(PathBuf);

pub struct Solvers(Vec<Solver>);

impl Solvers {
    pub fn iter(&self) -> impl Iterator<Item = &Solver> {
        self.0.iter()
    }

    fn from_json(json: Value) -> Result<Self> {
        let Value::Array(array) = json else {
            return Err(Error::InvalidOutputFormat(
                "JSON does not start with an array".to_owned(),
            ));
        };

        let mut solvers: Vec<Solver> = vec![];
        for solver_json in array {
            match Solver::from_json(solver_json) {
                Ok(solver) => solvers.push(solver),
                Err(e) => logging::error!(e.into()),
            }
        }

        Ok(Self(solvers))
    }
}

impl Solver {
    fn from_json(json: Value) -> std::result::Result<Self, SolverParseError> {
        let Value::Object(mut object) = json else {
            return Err(SolverParseError::NotAnObject(json));
        };

        Ok(Self {
            name: Self::string_from_json("name", &mut object)?,
            executable: Self::executable_from_json(&mut object)?,
            supported_std_flags: Self::std_flags_from_json(&mut object)?,
            input_type: Self::input_type_from_json(&mut object)?,
        })
    }

    fn field_from_json(
        field_name: &str,
        object: &mut Map<String, Value>,
    ) -> SolverParseResult<Value> {
        match object.remove(field_name) {
            Some(value) => Ok(value),
            None => Err(SolverParseError::FieldNotPresent(
                field_name.to_string(),
                object.clone(),
            )),
        }
    }

    fn string_from_json(
        field_name: &str,
        object: &mut Map<String, Value>,
    ) -> SolverParseResult<String> {
        let value = Self::field_from_json(field_name, object)?;
        let Value::String(s) = value else {
            return Err(SolverParseError::FieldNotAString(
                field_name.to_string(),
                value,
            ));
        };

        Ok(s)
    }

    fn array_from_json(
        field_name: &str,
        object: &mut Map<String, Value>,
    ) -> SolverParseResult<Vec<Value>> {
        let value = Self::field_from_json(field_name, object)?;
        let Value::Array(array) = value else {
            return Err(SolverParseError::FieldNotAnArray(
                field_name.to_string(),
                value,
            ));
        };

        Ok(array)
    }

    fn executable_from_json(object: &mut Map<String, Value>) -> SolverParseResult<Executable> {
        let s = Self::string_from_json("executable", object)?;
        Ok(Executable(s.into()))
    }

    fn input_type_from_json(object: &mut Map<String, Value>) -> SolverParseResult<SolverKind> {
        let input_type_str = Self::string_from_json("inputType", object)?;

        match input_type_str.as_str() {
            "FZN" => Ok(SolverKind::Fzn),
            "JSON" => Ok(SolverKind::Json),
            _ => Err(SolverParseError::UnknownSolverKind(input_type_str)),
        }
    }

    fn std_flags_from_json(
        object: &mut Map<String, Value>,
    ) -> SolverParseResult<SupportedStdFlags> {
        let flags_json = Self::array_from_json("stdFlags", object)?;
        let mut supported_flags = HashSet::<String>::new();
        for flag_json in flags_json {
            let Value::String(flag) = flag_json else {
                logging::error!(SolverParseError::StdFlagNotAString(flag_json).into());
                continue;
            };
            supported_flags.insert(flag);
        }

        Ok(SupportedStdFlags {
            a: supported_flags.contains("-a"),
            i: supported_flags.contains("-i"),
            f: supported_flags.contains("-f"),
            p: supported_flags.contains("-p"),
        })
    }
}

#[derive(Debug, thiserror::Error)]
enum SolverParseError {
    #[error("JSON is not an object: {0}")]
    NotAnObject(Value),

    #[error("Solver's field '{0}' is not present: {1:?}")]
    FieldNotPresent(String, serde_json::Map<String, Value>),
    #[error("Solver's field '{0}' is not a string: {1}")]
    FieldNotAString(String, Value),
    #[error("Solver's field '{0}' is not an array: {1}")]
    FieldNotAnArray(String, Value),

    #[error("Solver has unknown solver kind: {0}")]
    UnknownSolverKind(String),

    #[error("A std flag is not a string: {0}")]
    StdFlagNotAString(Value),
}
type SolverParseResult<T> = std::result::Result<T, SolverParseError>;

pub async fn discover(minizinc_exe: &Path) -> Result<Solvers> {
    let output = run_discover_command(minizinc_exe).await?;
    let json = serde_json::from_slice::<Value>(&output)?;
    Solvers::from_json(json)
}

async fn run_discover_command(minizinc_exe: &Path) -> Result<Vec<u8>> {
    let mut cmd = Command::new(minizinc_exe);
    cmd.arg("--solvers-json");
    let output = cmd.output().await.map_err(Error::CommandFailed)?;
    if !output.status.success() {
        Err(Error::CommandUnsuccessful(output.status))
    } else {
        Ok(output.stdout)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Discover command failed: {0}")]
    CommandFailed(std::io::Error),
    #[error("Discover command exited with unsuccessful exit code: {0}")]
    CommandUnsuccessful(ExitStatus),
    #[error("Error occurred while parsing solver discovery output as JSON: {0}")]
    JsonParsing(#[from] serde_json::Error),
    #[error("The solver discovery output did not have the expected format: {0}")]
    InvalidOutputFormat(String),
}

pub type Result<T> = std::result::Result<T, Error>;
