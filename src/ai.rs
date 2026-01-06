pub mod commandline;
use crate::scheduler::{Portfolio, SolverInfo};
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
        Ok(vec![
            SolverInfo::new("coinbc".to_string(), 1),
            SolverInfo::new("picat".to_string(), 1),
            SolverInfo::new("cp-sat".to_string(), 1),
            SolverInfo::new("yuck".to_string(), 1),
            SolverInfo::new("highs".to_string(), 1),
            SolverInfo::new("choco".to_string(), 1),
            SolverInfo::new("pumpkin".to_string(), 1),
        ])
    }
}
