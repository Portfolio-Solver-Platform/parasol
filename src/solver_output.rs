use crate::args::DebugVerbosityLevel;
use crate::model_parser::ObjectiveValue;
use std::fmt;

#[derive(Debug)]
pub struct Parser {
    input: String,
    objective: Option<ObjectiveValue>,
}

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

pub const SOLUTION_TERMINATOR: &str = "----------";
pub const DONE_TERMINATOR: &str = "==========";
pub const UNSATISFIABLE_TERMINATOR: &str = "=====UNSATISFIABLE=====";
pub const UNBOUNDED_TERMINATOR: &str = "=====UNBOUNDED=====";
pub const UNKNOWN_TERMINATOR: &str = "=====UNKNOWN=====";

#[derive(Debug)]
pub struct Solution {
    pub solution: String,
    pub objective: ObjectiveValue,
}

impl Status {
    pub fn to_dzn_string(&self) -> &str {
        match self {
            Status::OptimalSolution => DONE_TERMINATOR,
            Status::Unsatisfiable => UNSATISFIABLE_TERMINATOR,
            Status::Unbounded => UNBOUNDED_TERMINATOR,
            Status::Unknown => UNKNOWN_TERMINATOR,
        }
    }
}

#[derive(Debug)]
pub enum Error {
    JsonParsing(serde_json::Error),
    SolutionMissingObjective,
    Field(String),
    ObjectiveParse,
}

impl From<serde_json::Error> for Error {
    fn from(value: serde_json::Error) -> Self {
        Error::JsonParsing(value)
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "failed to parse output")
    }
}

pub type Result<T> = std::result::Result<T, Error>;

impl Parser {
    pub fn new() -> Self {
        Self {
            input: "".to_owned(),
            objective: None,
        }
    }

    fn to_solution(&mut self) -> Result<Solution> {
        let objective = self
            .objective
            .take()
            .ok_or(Error::SolutionMissingObjective)?;

        // Clear state
        self.objective = None;
        let input = std::mem::take(&mut self.input);

        Ok(Solution {
            solution: input,
            objective,
        })
    }

    pub fn next_line(&mut self, line: &str) -> Result<Option<Output>> {
        const OBJECTIVE_PREFIX: &str = "_objective = ";

        let line = line.trim();

        self.input += line;
        self.input += "\n";

        if line == SOLUTION_TERMINATOR {
            Ok(Some(Output::Solution(self.to_solution()?)))
        } else if line == DONE_TERMINATOR {
            Ok(Some(Output::Status(Status::OptimalSolution)))
        } else if line == UNSATISFIABLE_TERMINATOR {
            Ok(Some(Output::Status(Status::Unsatisfiable)))
        } else if line == UNBOUNDED_TERMINATOR {
            Ok(Some(Output::Status(Status::Unbounded)))
        } else if line == UNKNOWN_TERMINATOR {
            Ok(Some(Output::Status(Status::Unknown)))
        } else if line.starts_with(OBJECTIVE_PREFIX) {
            let objective_str: String = line[OBJECTIVE_PREFIX.len()..]
                .chars()
                .take_while(|c| *c != ';')
                .collect();
            let objective = objective_str
                .parse::<ObjectiveValue>()
                .map_err(|_| Error::ObjectiveParse)?;
            self.objective = Some(objective);
            Ok(None)
        } else {
            Ok(None)
        }
    }
}
