use std::{collections::HashSet, path::Path, process::ExitStatus};

use serde_json::{Map, Value};
use tokio::process::Command;

use crate::logging;
use crate::solver_config::{Executable, Solver, SolverInputType, Solvers, SupportedStdFlags};

pub async fn discover(minizinc_exe: &Path) -> Result<Solvers> {
    let output = run_discover_command(minizinc_exe).await?;
    let json = serde_json::from_slice::<Value>(&output)?;
    Solvers::from_json(json)
}

impl Solvers {
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
                Err(SolverParseError::UnknownInputType {
                    solver_id,
                    input_type,
                }) => {
                    if input_type == "MZN" || input_type == "NL" {
                        let msg = format!(
                            "Solver with ID '{solver_id}' has unsupported input type '{input_type}'"
                        );
                        if solver_id == crate::solvers::SELF_ID {
                            logging::info!("{msg}");
                        } else {
                            logging::warning!("{msg}");
                        }
                    } else {
                        logging::error!(
                            SolverParseError::UnknownInputType {
                                solver_id,
                                input_type
                            }
                            .into()
                        );
                    }
                }
                Err(e) => logging::error!(e.into()),
            }
        }
        logging::info!("Discovered solvers: {solvers:?}");

        Ok(Self(solvers))
    }
}

impl Solver {
    fn from_json(json: Value) -> SolverParseResult<Self> {
        let Value::Object(mut object) = json else {
            return Err(SolverParseError::NotAnObject(json));
        };

        let id = Self::string_from_json("id", &mut object)?.to_lowercase();
        Ok(Self {
            executable: Self::executable_from_json(&mut object).transpose()?,
            input_type: Self::input_type_from_json(&id, &mut object)?,
            supported_std_flags: Self::std_flags_from_json(&id, &mut object)?,
            id,
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

    fn executable_from_json(
        object: &mut Map<String, Value>,
    ) -> Option<SolverParseResult<Executable>> {
        const FIELD_NAME: &str = "executable";

        Self::field_from_json(FIELD_NAME, object)
            .ok()
            .map(|json| match json {
                Value::String(s) => Ok(Executable(s.into(), Vec::new())),
                Value::Array(values) => Self::executable_from_array(values),
                _ => Err(SolverParseError::FieldNotAString(
                    FIELD_NAME.to_string(),
                    json,
                )),
            })
    }

    fn executable_from_array(array: Vec<Value>) -> SolverParseResult<Executable> {
        let mut values = array
            .into_iter()
            .map(|value| {
                if let Value::String(s) = value {
                    Ok(s)
                } else {
                    Err(SolverParseError::ExecutableNonStringArrayElement(
                        value.clone(),
                    ))
                }
            })
            .collect::<SolverParseResult<Vec<_>>>()?
            .into_iter();

        let first = values
            .next()
            .ok_or(SolverParseError::ExecutableArrayEmpty)?;

        let rest = values.collect::<Vec<_>>();
        Ok(Executable(first.into(), rest))
    }

    fn input_type_from_json(
        solver_id: &str,
        object: &mut Map<String, Value>,
    ) -> SolverParseResult<SolverInputType> {
        let input_type_str = Self::string_from_json("inputType", object)?;

        match input_type_str.as_str() {
            "FZN" => Ok(SolverInputType::Fzn),
            "JSON" => Ok(SolverInputType::Json),
            _ => Err(SolverParseError::UnknownInputType {
                solver_id: solver_id.to_owned(),
                input_type: input_type_str,
            }),
        }
    }

    fn std_flags_from_json(
        solver_id: &str,
        object: &mut Map<String, Value>,
    ) -> SolverParseResult<SupportedStdFlags> {
        const FIELD_NAME: &str = "stdFlags";
        let flags_json_result = Self::array_from_json(FIELD_NAME, object);
        let Ok(flags_json) = flags_json_result else {
            logging::warning!(
                "solver with ID '{solver_id}' does not state its supported standard flags (the '{FIELD_NAME}' field) in its configuration. We assume that it supports '-i' and '-p'. If you mean that it does not support any standard flags, please set '{FIELD_NAME}' to the empty array"
            );
            return Ok(SupportedStdFlags {
                i: true,
                p: true,
                ..Default::default()
            });
        };

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

    #[error("Solver with ID '{solver_id}' has unknown input type: {input_type}")]
    UnknownInputType {
        solver_id: String,
        input_type: String,
    },

    #[error("Solver's field 'executable' is an array but an element was not a string: {0}")]
    ExecutableNonStringArrayElement(Value),

    #[error("Solver's field 'executable' is an empty array")]
    ExecutableArrayEmpty,

    #[error("A std flag is not a string: {0}")]
    StdFlagNotAString(Value),
}
type SolverParseResult<T> = std::result::Result<T, SolverParseError>;

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

impl Solver {
    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn input_type(&self) -> &SolverInputType {
        &self.input_type
    }

    pub fn executable(&self) -> Option<&Executable> {
        self.executable.as_ref()
    }

    pub fn supported_std_flags(&self) -> &SupportedStdFlags {
        &self.supported_std_flags
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
