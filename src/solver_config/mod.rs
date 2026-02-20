use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tokio::process::Command;

use crate::args::{CommonArgs, SolverConfigMode};
use crate::logging;

pub mod cache;
pub mod discovery;

async fn load(mode: &SolverConfigMode, minizinc_exe: &Path) -> Solvers {
    match mode {
        SolverConfigMode::Cache => match cache::load_solvers_config() {
            Ok(solvers) => return solvers,
            Err(e) => {
                logging::error_with_msg!(
                    e.into(),
                    "Failed to load solver cache. Falling back to discovery"
                );
            }
        },
        SolverConfigMode::Discover => {}
    }

    discovery::discover(minizinc_exe).await.unwrap_or_else(|e| {
        logging::error!(e.into());
        Solvers::empty()
    })
}

pub async fn load_from_args(args: &CommonArgs) -> Solvers {
    load(&args.solver_config_mode, &args.minizinc.minizinc_exe).await
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Solver {
    id: String,
    executable: Option<Executable>,
    supported_std_flags: SupportedStdFlags,
    input_type: SolverInputType,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SupportedStdFlags {
    pub a: bool,
    pub i: bool,
    pub f: bool,
    pub p: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SolverInputType {
    Fzn,
    Json,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Executable(PathBuf, Vec<String>);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Solvers(Vec<Solver>);

impl Solvers {
    pub fn empty() -> Self {
        Self(Vec::new())
    }

    pub fn iter(&self) -> impl Iterator<Item = &Solver> {
        self.0.iter()
    }

    pub fn get_by_id(&self, name: &str) -> Option<&Solver> {
        let lowered_id = name.to_lowercase();
        self.0.iter().find(|solver| solver.id == lowered_id)
    }
}

impl Executable {
    pub fn into_command(self) -> Command {
        let mut cmd = Command::new(self.0);
        cmd.args(self.1);
        cmd
    }

    pub fn path_is_relative(&self) -> bool {
        self.0.is_relative()
    }
}
