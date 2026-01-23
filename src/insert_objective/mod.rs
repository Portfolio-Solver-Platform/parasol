mod fzn;
mod json;

use crate::logging;
use crate::model_parser::{ObjectiveType, ObjectiveValue};
use crate::solver_config::{self, SolverInputType};
use async_tempfile::TempFile;
use std::path::Path;
use std::sync::Arc;

pub struct ObjectiveInserter {
    solvers: Arc<solver_config::Solvers>,
}

impl ObjectiveInserter {
    pub fn new(solvers: Arc<solver_config::Solvers>) -> Self {
        Self { solvers }
    }

    pub async fn insert_objective(
        &self,
        solver_name: &str,
        input_path: &Path,
        objective_type: &ObjectiveType,
        objective: ObjectiveValue,
    ) -> Result<TempFile> {
        let input_type = self
            .solvers
            .get_by_id(solver_name)
            .map(|solver| solver.input_type())
            .unwrap_or_else(|| {
                logging::error_msg!(
                    "Solver metadata could not be found for solver with name '{solver_name}'. Perhaps it is not supported or not installed"
                );
                &SolverInputType::Fzn
            });

        match input_type {
            SolverInputType::Fzn => fzn::insert_objective(input_path, objective_type, objective)
                .await
                .map_err(Into::into),
            SolverInputType::Json => json::insert_objective(input_path, objective_type, objective)
                .await
                .map_err(Into::into),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Failed to insert objective into FlatZinc file: {0}")]
    Fzn(#[from] fzn::Error),
    #[error("Failed to insert objective into JSON file: {0}")]
    Json(#[from] json::Error),
}
pub type Result<T> = std::result::Result<T, Error>;
