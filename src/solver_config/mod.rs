use std::path::PathBuf;

use tokio::process::Command;

pub mod discovery;

#[derive(Debug, Clone)]
pub struct Solver {
    id: String,
    executable: Option<Executable>,
    supported_std_flags: SupportedStdFlags,
    input_type: SolverInputType,
}

#[derive(Debug, Clone, Default)]
pub struct SupportedStdFlags {
    pub a: bool,
    pub i: bool,
    pub f: bool,
    pub p: bool,
}

#[derive(Debug, Clone)]
pub enum SolverInputType {
    Fzn,
    Json,
}

#[derive(Debug, Clone)]
pub struct Executable(PathBuf, Vec<String>);

#[derive(Debug, Clone)]
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
}
