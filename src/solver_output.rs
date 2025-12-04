use std::fmt;

use serde_json::{Map, Value};

#[derive(Debug)]
pub enum Output {
    Solution(Solution),
    Status(Status),
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
    pub objective: i64,
}

impl Status {
    const UNSATISFIABLE_STR: &str = "UNSATISFIABLE";
    const UNBOUNDED_STR: &str = "UNBOUNDED";
    const UNKNOWN_STR: &str = "UNKNOWN";
    const OPTIMAL_SOLUTION_STR: &str = "OPTIMAL_SOLUTION";
}

#[derive(Debug)]
pub enum ParseError {
    JsonParsing(serde_json::Error),
    MissingObjective,
    Field(String),
}

impl From<serde_json::Error> for ParseError {
    fn from(value: serde_json::Error) -> Self {
        ParseError::JsonParsing(value)
    }
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "failed to parse output")
    }
}

impl Output {
    const SOLUTION_TERMINATOR: &str = "----------";
    const DONE_TERMINATOR: &str = "==========";
    const UNSATISFIABLE_TERMINATOR: &str = "=====UNSATISFIABLE=====";
    const UNBOUNDED_TERMINATOR: &str = "=====UNBOUNDED=====";
    const UNKNOWN_TERMINATOR: &str = "=====UNKNOWN=====";

    pub fn parse(output: &str) -> Result<Self, ParseError> {
        let Value::Object(json) = serde_json::from_str(output)? else {
            return Err(ParseError::Field("Output is not a JSON object".to_owned()));
        };

        let kind = parse_string_field(&json, "type")?;

        match kind.as_str() {
            "solution" => Ok(Self::Solution(parse_solution(&json)?)),
            "status" => Ok(Self::Status(parse_status(&json)?)),
            _ => Err(ParseError::Field(format!(
                "'type' = '{kind}' is not supported"
            ))),
        }
    }
}

fn parse_solution(json: &Map<String, Value>) -> Result<Solution, ParseError> {
    let output = parse_object_field(&json, "output")?;
    let solution = parse_string_field(&json, "default")?;
    let objective = parse_i64_field(&json, "_objective")?;

    Ok(Solution {
        solution: solution.clone(),
        objective,
    })
}

fn parse_status(json: &Map<String, Value>) -> Result<Status, ParseError> {
    let status = parse_string_field(json, "status")?;
    match status.as_str() {
        Status::OPTIMAL_SOLUTION_STR => Ok(Status::OptimalSolution),
        Status::UNSATISFIABLE_STR => Ok(Status::Unsatisfiable),
        Status::UNBOUNDED_STR => Ok(Status::Unbounded),
        Status::UNKNOWN_STR => Ok(Status::Unknown),
        _ => Err(ParseError::Field(format!(
            "'status' = '{status}' is an unknown status"
        ))),
    }
}

fn parse_field<'a>(json: &'a Map<String, Value>, field: &str) -> Result<&'a Value, ParseError> {
    match json.get(field) {
        Some(value) => Ok(value),
        None => Err(ParseError::Field(format!(
            "field '{field}' is missing from json"
        ))),
    }
}

fn parse_string_field<'a>(
    json: &'a Map<String, Value>,
    field: &str,
) -> Result<&'a String, ParseError> {
    match parse_field(json, field)? {
        Value::String(value) => Ok(value),
        _ => Err(ParseError::Field(format!(
            "field '{field}' is not a string"
        ))),
    }
}

fn parse_i64_field(json: &Map<String, Value>, field: &str) -> Result<i64, ParseError> {
    match parse_field(json, field)? {
        Value::Number(value) => match value.as_i64() {
            Some(num) => Ok(num),
            None => Err(ParseError::Field(format!(
                "field '{field}' is a number but not an i64"
            ))),
        },
        _ => Err(ParseError::Field(format!(
            "field '{field}' is not a number"
        ))),
    }
}

fn parse_object_field<'a>(
    json: &'a Map<String, Value>,
    field: &str,
) -> Result<&'a Map<String, Value>, ParseError> {
    match parse_field(json, field)? {
        Value::Object(value) => Ok(value),
        _ => Err(ParseError::Field(format!(
            "field '{field}' is not an object"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ARITHEMETIC_TARGET_SOLUTION: &str = r#"{"type": "solution", "output": {"default": "yCoor = [29, 1, 8, 6, 31, 15, 11, 6, 6, 1, 42, 11, 40, 26, 37, 16, 16, 43, 21, 33];\nS = [22, 41, 29];\nD = 45;\nobjective = 137;\n", "raw": "yCoor = [29, 1, 8, 6, 31, 15, 11, 6, 6, 1, 42, 11, 40, 26, 37, 16, 16, 43, 21, 33];\nS = [22, 41, 29];\nD = 45;\nobjective = 137;\n", "json": {  "yCoor" : [29, 1, 8, 6, 31, 15, 11, 6, 6, 1, 42, 11, 40, 26, 37, 16, 16, 43, 21, 33],  "objective" : 137,  "S" : [22, 41, 29],  "D" : 45,  "_objective" : 137}}, "sections": ["default", "raw", "json"]}"#;
    const ARITHMETIC_TARGET_STATUS: &str = r#"{"type": "status", "status": "UNKNOWN"}"#

    // #[test]
    // fn test_parse_solution() {
    //     let input = ARITHEMETIC_TARGET_SOLUTION;
    //     let num = get_first_number("hel 654; uhte\nueou");
    //     assert_eq!(num, Some("654".to_owned()))
    // }

    // #[test]
    // fn test_arithmetic_target_output_parsing() {
    //     let s = ARITHEMETIC_TARGET_OUTPUT;
    //     let output = Output::parse(s);
    //     assert_eq!(output.kind, OutputKind::Unknown);
    //     assert_eq!(output.original_output, s);

    //     let solutions: Vec<_> = output.solutions().collect();
    //     assert_eq!(solutions.len(), 5);
    //     let solution = solutions.first().unwrap();
    //     assert_eq!(solution.objective, Some(125));
    // }
}
