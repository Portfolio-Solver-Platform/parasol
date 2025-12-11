pub mod dzn;

use crate::args::DebugVerbosityLevel;
use serde_json::{Map, Value};
use std::fmt;

#[derive(Debug)]
pub enum Output {
    Solution(Solution),
    Status(Status),
    Ignore,
}

#[derive(Debug, PartialEq)]
pub enum Status {
    OptimalSolution,
    Unsatisfiable,
    Unbounded,
    Unknown,
}

#[derive(Debug)]
pub struct Solution {
    pub solution: String,
    pub objective: f64,
}

impl Status {
    const UNSATISFIABLE_STR: &str = "UNSATISFIABLE";
    const UNBOUNDED_STR: &str = "UNBOUNDED";
    const UNKNOWN_STR: &str = "UNKNOWN";
    const OPTIMAL_SOLUTION_STR: &str = "OPTIMAL_SOLUTION";
}

#[derive(Debug)]
pub enum OutputParseError {
    JsonParsing(serde_json::Error),
    MissingObjective,
    Field(String),
    ObjectiveParse,
}

impl From<serde_json::Error> for OutputParseError {
    fn from(value: serde_json::Error) -> Self {
        OutputParseError::JsonParsing(value)
    }
}

impl fmt::Display for OutputParseError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "failed to parse output")
    }
}

impl Output {
    pub fn parse(output: &str, verbosity: DebugVerbosityLevel) -> Result<Self, OutputParseError> {
        let Value::Object(json) = serde_json::from_str(output)? else {
            return Err(OutputParseError::Field(
                "Output is not a JSON object".to_owned(),
            ));
        };

        let kind = parse_string_field(&json, "type")?;

        match kind.as_str() {
            "solution" => Ok(Self::Solution(parse_solution(&json)?)),
            "status" => Ok(Self::Status(parse_status(&json)?)),
            "comment" => Ok(Self::Ignore),
            "warning" => {
                if verbosity >= DebugVerbosityLevel::Warning {
                    eprintln!("Solver warning: {:?}", json);
                }
                Ok(Self::Ignore)
            }

            "error" => {
                if verbosity >= DebugVerbosityLevel::Error {
                    eprintln!("Solver error: {:?}", json);
                }
                Ok(Self::Ignore)
            }

            "message" => {
                if verbosity >= DebugVerbosityLevel::Info {
                    eprintln!("Solver message: {:?}", json);
                }
                Ok(Self::Ignore)
            }

            _ => Err(OutputParseError::Field(format!(
                "'type' = '{kind}' is not supported"
            ))),
        }
    }
}

fn parse_solution(json: &Map<String, Value>) -> Result<Solution, OutputParseError> {
    const OBJECTIVE_PREFIX: &str = "objective = ";
    let output = parse_object_field(json, "output")?;
    let solution = parse_string_field(output, "dzn")?;
    let objective_line = solution
        .split(';')
        .next()
        .ok_or(OutputParseError::MissingObjective)?;
    if !objective_line.starts_with(OBJECTIVE_PREFIX) {
        return Err(OutputParseError::MissingObjective);
    }

    let objective_str = &objective_line[OBJECTIVE_PREFIX.len()..];

    let objective = objective_str
        .parse::<f64>()
        .map_err(|_| OutputParseError::ObjectiveParse)?;

    Ok(Solution {
        solution: solution.clone(),
        objective,
    })
}

fn parse_status(json: &Map<String, Value>) -> Result<Status, OutputParseError> {
    let status = parse_string_field(json, "status")?;
    match status.as_str() {
        Status::OPTIMAL_SOLUTION_STR => Ok(Status::OptimalSolution),
        Status::UNSATISFIABLE_STR => Ok(Status::Unsatisfiable),
        Status::UNBOUNDED_STR => Ok(Status::Unbounded),
        Status::UNKNOWN_STR => Ok(Status::Unknown),
        _ => Err(OutputParseError::Field(format!(
            "'status' = '{status}' is an unknown status"
        ))),
    }
}

fn parse_field<'a>(
    json: &'a Map<String, Value>,
    field: &str,
) -> Result<&'a Value, OutputParseError> {
    match json.get(field) {
        Some(value) => Ok(value),
        None => Err(OutputParseError::Field(format!(
            "field '{field}' is missing from json"
        ))),
    }
}

fn parse_string_field<'a>(
    json: &'a Map<String, Value>,
    field: &str,
) -> Result<&'a String, OutputParseError> {
    match parse_field(json, field)? {
        Value::String(value) => Ok(value),
        _ => Err(OutputParseError::Field(format!(
            "field '{field}' is not a string"
        ))),
    }
}

fn parse_i64_field(json: &Map<String, Value>, field: &str) -> Result<i64, OutputParseError> {
    match parse_field(json, field)? {
        Value::Number(value) => match value.as_i64() {
            Some(num) => Ok(num),
            None => Err(OutputParseError::Field(format!(
                "field '{field}' is a number but not an i64"
            ))),
        },
        _ => Err(OutputParseError::Field(format!(
            "field '{field}' is not a number"
        ))),
    }
}

fn parse_object_field<'a>(
    json: &'a Map<String, Value>,
    field: &str,
) -> Result<&'a Map<String, Value>, OutputParseError> {
    match parse_field(json, field)? {
        Value::Object(value) => Ok(value),
        _ => Err(OutputParseError::Field(format!(
            "field '{field}' is not an object"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ARITHMETIC_TARGET_SOLUTION: &str = r#"{"type": "solution", "output": {"default": "yCoor = [29, 1, 8, 6, 31, 15, 11, 6, 6, 1, 42, 11, 40, 26, 37, 16, 16, 43, 21, 33];\nS = [22, 41, 29];\nD = 45;\nobjective = 137;\n", "raw": "yCoor = [29, 1, 8, 6, 31, 15, 11, 6, 6, 1, 42, 11, 40, 26, 37, 16, 16, 43, 21, 33];\nS = [22, 41, 29];\nD = 45;\nobjective = 137;\n", "json": {  "yCoor" : [29, 1, 8, 6, 31, 15, 11, 6, 6, 1, 42, 11, 40, 26, 37, 16, 16, 43, 21, 33],  "objective" : 137,  "S" : [22, 41, 29],  "D" : 45,  "_objective" : 137}}, "sections": ["default", "raw", "json"]}"#;
    const ARITHMETIC_TARGET_SOLUTION_DZN: &str = "yCoor = [29, 1, 8, 6, 31, 15, 11, 6, 6, 1, 42, 11, 40, 26, 37, 16, 16, 43, 21, 33];\nS = [22, 41, 29];\nD = 45;\nobjective = 137;\n";
    const ARITHMETIC_TARGET_STATUS: &str = r#"{"type": "status", "status": "UNKNOWN"}"#;
    const COMMENT: &str = r#"{"type": "comment", "comment": "% obj = 848\n"}"#;

    const NFC_STATUS: &str = r#"{"type": "status", "status": "OPTIMAL_SOLUTION"}"#;

    #[test]
    fn test_parse_solution() {
        let input = ARITHMETIC_TARGET_SOLUTION;
        let output = Output::parse(input, DebugVerbosityLevel::Quiet).unwrap();
        let Output::Solution(solution) = output else {
            panic!("Output is not a solution");
        };
        assert_eq!(solution.objective, 137.0);
        assert_eq!(solution.solution, ARITHMETIC_TARGET_SOLUTION_DZN);
    }

    #[test]
    fn test_parse_unknown_status() {
        let input = ARITHMETIC_TARGET_STATUS;
        let output = Output::parse(input, DebugVerbosityLevel::Quiet).unwrap();
        let Output::Status(status) = output else {
            panic!("Output is not a status");
        };
        assert_eq!(status, Status::Unknown);
    }

    #[test]
    fn test_parse_optimal_status() {
        let input = NFC_STATUS;
        let output = Output::parse(input, DebugVerbosityLevel::Quiet).unwrap();
        let Output::Status(status) = output else {
            panic!("Output is not a status");
        };
        assert_eq!(status, Status::OptimalSolution);
    }

    #[test]
    fn test_parse_comment() {
        let input = COMMENT;
        let output = Output::parse(input, DebugVerbosityLevel::Quiet).unwrap();
        let Output::Ignore = output else {
            panic!("Output is not a status");
        };
    }
}
