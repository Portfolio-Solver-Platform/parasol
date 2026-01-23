use crate::solver_config::{Solvers, discovery};
use directories::BaseDirs;
use std::path::{Path, PathBuf};
use std::{fs, io};

fn cache_path() -> Result<PathBuf> {
    let base_dirs = BaseDirs::new().ok_or(Error::NoHomeDirectory)?;
    Ok(base_dirs.cache_dir().join("parasol").join("cache.json"))
}

pub fn save_solvers_config(solvers: &Solvers) -> Result<()> {
    let path = cache_path()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let content = serde_json::to_string_pretty(solvers)?;
    fs::write(&path, content)?;
    Ok(())
}

pub fn load_solvers_config() -> Result<Solvers> {
    let path = cache_path()?;
    let content = fs::read_to_string(&path)?;
    let solvers: Solvers = serde_json::from_str(&content)?;
    Ok(solvers)
}

pub async fn build_solvers_config_cache(minizinc_exe: &Path) -> Result<()> {
    let solvers = discovery::discover(minizinc_exe).await?;
    save_solvers_config(&solvers)
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("could not determine home directory")]
    NoHomeDirectory,
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Discovery(#[from] discovery::Error),
}

pub type Result<T> = std::result::Result<T, Error>;
