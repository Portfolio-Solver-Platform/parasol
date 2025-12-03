use std::fmt;

use serde_json::Value;

#[derive(Debug)]
pub enum Output {
    Solution(Solution),
    Status(Status),
}

#[derive(Debug, PartialEq)]
pub enum Status {
    Done,
    Unsatisfiable,
    Unbounded,
    Unknown,
}

#[derive(Debug)]
pub struct Solution {
    pub solution: String,
    pub objective: Option<i32>,
}

impl Status {
    const UNSATISFIABLE_STR: &str = "UNSATISFIABLE";
    const UNBOUNDED_STR: &str = "UNBOUNDED";
    const UNKNOWN_STR: &str = "UNKNOWN";
}

pub enum ParseError {
    JsonParsing(serde_json::Error),
    MissingObjective,
    JsonMalformed(String),
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
            return Err(ParseError::JsonMalformed(
                "Outermost JSON is not an object".to_owned(),
            ));
        };

        let Some(Value::String(kind)) = json.get("type") else {
            return Err(ParseError::JsonMalformed(
                "type field is missing from json or it is not a string".to_owned(),
            ));
        };

        todo!()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ARITHEMETIC_TARGET_OUTPUT: &str = r#"{"type": "solution", "output": {"default": "yCoor = [29, 1, 8, 6, 31, 15, 11, 6, 6, 1, 42, 11, 40, 26, 37, 16, 16, 43, 21, 33];\nS = [22, 41, 29];\nD = 45;\nobjective = 137;\n", "raw": "yCoor = [29, 1, 8, 6, 31, 15, 11, 6, 6, 1, 42, 11, 40, 26, 37, 16, 16, 43, 21, 33];\nS = [22, 41, 29];\nD = 45;\nobjective = 137;\n", "json": {  "yCoor" : [29, 1, 8, 6, 31, 15, 11, 6, 6, 1, 42, 11, 40, 26, 37, 16, 16, 43, 21, 33],  "objective" : 137,  "S" : [22, 41, 29],  "D" : 45,  "_objective" : 137}}, "sections": ["default", "raw", "json"]}
{"type": "solution", "output": {"default": "yCoor = [29, 1, 8, 6, 31, 15, 11, 6, 6, 1, 42, 11, 40, 26, 37, 16, 43, 16, 21, 33];\nS = [22, 44, 14];\nD = 45;\nobjective = 125;\n", "raw": "yCoor = [29, 1, 8, 6, 31, 15, 11, 6, 6, 1, 42, 11, 40, 26, 37, 16, 43, 16, 21, 33];\nS = [22, 44, 14];\nD = 45;\nobjective = 125;\n", "json": {  "yCoor" : [29, 1, 8, 6, 31, 15, 11, 6, 6, 1, 42, 11, 40, 26, 37, 16, 43, 16, 21, 33],  "objective" : 125,  "S" : [22, 44, 14],  "D" : 45,  "_objective" : 125}}, "sections": ["default", "raw", "json"]}
{"type": "solution", "output": {"default": "yCoor = [29, 1, 9, 7, 31, 15, 11, 6, 7, 1, 42, 11, 40, 26, 37, 16, 43, 16, 21, 33];\nS = [21, 44, 14];\nD = 45;\nobjective = 124;\n", "raw": "yCoor = [29, 1, 9, 7, 31, 15, 11, 6, 7, 1, 42, 11, 40, 26, 37, 16, 43, 16, 21, 33];\nS = [21, 44, 14];\nD = 45;\nobjective = 124;\n", "json": {  "yCoor" : [29, 1, 9, 7, 31, 15, 11, 6, 7, 1, 42, 11, 40, 26, 37, 16, 43, 16, 21, 33],  "objective" : 124,  "S" : [21, 44, 14],  "D" : 45,  "_objective" : 124}}, "sections": ["default", "raw", "json"]}
{"type": "solution", "output": {"default": "yCoor = [29, 1, 8, 6, 31, 11, 33, 6, 6, 1, 43, 11, 41, 26, 12, 16, 18, 15, 21, 37];\nS = [22, 42, 15];\nD = 43;\nobjective = 122;\n", "raw": "yCoor = [29, 1, 8, 6, 31, 11, 33, 6, 6, 1, 43, 11, 41, 26, 12, 16, 18, 15, 21, 37];\nS = [22, 42, 15];\nD = 43;\nobjective = 122;\n", "json": {  "yCoor" : [29, 1, 8, 6, 31, 11, 33, 6, 6, 1, 43, 11, 41, 26, 12, 16, 18, 15, 21, 37],  "objective" : 122,  "S" : [22, 42, 15],  "D" : 43,  "_objective" : 122}}, "sections": ["default", "raw", "json"]}
{"type": "solution", "output": {"default": "yCoor = [29, 1, 8, 6, 31, 11, 33, 6, 6, 1, 43, 11, 41, 26, 12, 16, 15, 18, 21, 37];\nS = [22, 42, 14];\nD = 43;\nobjective = 121;\n", "raw": "yCoor = [29, 1, 8, 6, 31, 11, 33, 6, 6, 1, 43, 11, 41, 26, 12, 16, 15, 18, 21, 37];\nS = [22, 42, 14];\nD = 43;\nobjective = 121;\n", "json": {  "yCoor" : [29, 1, 8, 6, 31, 11, 33, 6, 6, 1, 43, 11, 41, 26, 12, 16, 15, 18, 21, 37],  "objective" : 121,  "S" : [22, 42, 14],  "D" : 43,  "_objective" : 121}}, "sections": ["default", "raw", "json"]}
{"type": "solution", "output": {"default": "yCoor = [29, 1, 9, 7, 31, 11, 33, 6, 7, 1, 43, 11, 41, 26, 12, 16, 15, 18, 21, 37];\nS = [21, 42, 14];\nD = 43;\nobjective = 120;\n", "raw": "yCoor = [29, 1, 9, 7, 31, 11, 33, 6, 7, 1, 43, 11, 41, 26, 12, 16, 15, 18, 21, 37];\nS = [21, 42, 14];\nD = 43;\nobjective = 120;\n", "json": {  "yCoor" : [29, 1, 9, 7, 31, 11, 33, 6, 7, 1, 43, 11, 41, 26, 12, 16, 15, 18, 21, 37],  "objective" : 120,  "S" : [21, 42, 14],  "D" : 43,  "_objective" : 120}}, "sections": ["default", "raw", "json"]}
{"type": "status", "status": "UNKNOWN"}"#;

    #[test]
    fn test_get_first_number() {
        let num = get_first_number("hel 654; uhte\nueou");
        assert_eq!(num, Some("654".to_owned()))
    }

    #[test]
    fn test_arithmetic_target_output_parsing() {
        let s = ARITHEMETIC_TARGET_OUTPUT;
        let output = Output::parse(s);
        assert_eq!(output.kind, OutputKind::Unknown);
        assert_eq!(output.original_output, s);

        let solutions: Vec<_> = output.solutions().collect();
        assert_eq!(solutions.len(), 5);
        let solution = solutions.first().unwrap();
        assert_eq!(solution.objective, Some(125));
    }
}
