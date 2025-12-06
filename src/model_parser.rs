use std::fs;
use std::path::Path;

use regex::Regex;

#[derive(Debug)]
pub enum ModelParseError {
    NoSolveStatement,
    IoError(std::io::Error),
    RegexError(regex::Error),
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

pub fn parse_objective_type(model_path: &Path) -> Result<ObjectiveType, ModelParseError> {
    let content = fs::read_to_string(model_path)?;
    let re = Regex::new(r"solve([\S\s]*?;)")?;

    if let Some(cap) = re.captures(&content) {
        let solve_stmt = &cap[1];
        if solve_stmt.contains("minimize") {
            Ok(ObjectiveType::Minimize)
        } else if solve_stmt.contains("maximize") {
            Ok(ObjectiveType::Maximize)
        } else {
            Ok(ObjectiveType::Satisfy)
        }
    } else {
        Err(ModelParseError::NoSolveStatement)
    }
}
