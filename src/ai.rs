pub mod commandline;
use crate::{
    scheduler::{Portfolio, SolverInfo},
    solvers,
};
pub type Features = Vec<f32>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, Error>;

pub trait Ai {
    fn schedule(&mut self, features: &Features, cores: usize) -> Result<Portfolio>;
}

pub struct SimpleAi {}

impl Ai for SimpleAi {
    fn schedule(&mut self, _features: &Features, cores: usize) -> Result<Portfolio> {
        Ok(solvers::ALL_IDS
            .iter()
            .take(cores)
            .map(|solver| SolverInfo::new(solver.to_string(), 1))
            .collect())
    }
}
