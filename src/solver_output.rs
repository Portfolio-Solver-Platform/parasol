use crate::model_parser::{ObjectiveType, ObjectiveValue};

#[derive(Debug)]
pub struct Parser {
    input: String,
    objective: Option<ObjectiveValue>,
    objective_type: ObjectiveType,
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
    pub objective: Option<ObjectiveValue>,
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

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Failed to parse JSON")]
    JsonParsing(#[from] serde_json::Error),
    #[error("Solution is missing objective")]
    SolutionMissingObjective,
    #[error("Failed to parse objective")]
    ObjectiveParse,
}

pub type Result<T> = std::result::Result<T, Error>;

impl Parser {
    pub fn new(objective_type: ObjectiveType) -> Self {
        Self {
            input: "".to_owned(),
            objective: None,
            objective_type,
        }
    }

    fn take_solution(&mut self) -> Result<Solution> {
        let objective = match self.objective {
            None => {
                if self.objective_type == ObjectiveType::Satisfy {
                    None
                } else {
                    return Err(Error::SolutionMissingObjective);
                }
            }
            val => val,
        };

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
            Ok(Some(Output::Solution(self.take_solution()?)))
        } else if line == DONE_TERMINATOR {
            Ok(Some(Output::Status(Status::OptimalSolution)))
        } else if line == UNSATISFIABLE_TERMINATOR {
            Ok(Some(Output::Status(Status::Unsatisfiable)))
        } else if line == UNBOUNDED_TERMINATOR {
            Ok(Some(Output::Status(Status::Unbounded)))
        } else if line == UNKNOWN_TERMINATOR {
            Ok(Some(Output::Status(Status::Unknown)))
        } else if self.objective_type != ObjectiveType::Satisfy
            && line.starts_with(OBJECTIVE_PREFIX)
        {
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
