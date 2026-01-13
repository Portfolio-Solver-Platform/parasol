use crate::args::Args;
use tokio::process::Command;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Command failed")]
    CommandFailed,
    #[error("IO error while running backup solver: {0}")]
    Io(#[from] std::io::Error),
}
pub type Result<T> = std::result::Result<T, Error>;

pub async fn run_backup_solver(args: &Args, cores: usize) -> Result<()> {
    let mut cmd = Command::new(&args.minizinc_exe);
    cmd.arg("--solver").arg("cp-sat");

    cmd.arg(&args.model);
    if let Some(data) = &args.data {
        cmd.arg(data);
    }

    cmd.arg("-i").arg("-f");

    if args.output_objective {
        cmd.arg("--output-objective");
    }

    if let Some(output_mode) = &args.output_mode {
        cmd.arg("--output-mode");
        cmd.arg(output_mode.to_string());
    } else {
        cmd.args(["--output-mode", "dzn"]);
    }
    cmd.arg("-p").arg(cores.to_string());

    let mut child = cmd.spawn()?;

    let status = child.wait().await?;

    if status.success() {
        Ok(())
    } else {
        Err(Error::CommandFailed)
    }
}
