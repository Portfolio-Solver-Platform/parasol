pub mod static_parallel_portfolio;

pub trait Orchestrator {
    async fn run(self) -> Result<(), Error>;
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("conversion was cancelled")]
    Cancelled,
    #[error("error occurred in orchestrator: {0}")]
    Other(anyhow::Error),
}
