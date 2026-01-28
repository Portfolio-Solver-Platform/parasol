use super::Conversion;
use crate::args::RunArgs;
use crate::is_cancelled::IsCancelled;
use crate::logging;
use std::path::Path;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio_util::sync::CancellationToken;

pub async fn convert_mzn(
    args: &RunArgs,
    solver_name: &str,
    cancellation_token: CancellationToken,
) -> Result<Conversion> {
    let fzn_file = tempfile::Builder::new()
        .suffix(".fzn")
        .tempfile()
        .map_err(ConversionError::TempFile)?;
    let ozn_file = tempfile::Builder::new()
        .suffix(".ozn")
        .tempfile()
        .map_err(ConversionError::TempFile)?;

    run_mzn_to_fzn_cmd(
        args,
        solver_name,
        fzn_file.path(),
        ozn_file.path(),
        cancellation_token,
    )
    .await?;

    Ok(Conversion { fzn_file, ozn_file })
}

async fn run_mzn_to_fzn_cmd(
    args: &RunArgs,
    solver_name: &str,
    fzn_result_path: &Path,
    ozn_result_path: &Path,
    cancellation_token: CancellationToken,
) -> Result<()> {
    let mut cmd = get_mzn_to_fzn_cmd(args, solver_name, fzn_result_path, ozn_result_path);
    cmd.stderr(Stdio::piped());

    let mut child = cmd.spawn().map_err(ConversionError::from)?;

    if args.verbosity >= crate::args::Verbosity::Warning
        && let Some(stderr) = child.stderr.take()
    {
        tokio::spawn(async move {
            let reader = BufReader::new(stderr);
            let mut lines = reader.lines();
            while let Ok(Some(line)) = lines.next_line().await {
                logging::warning!("MiniZinc compilation: {}", line);
            }
        });
    }

    let status = tokio::select! {
        _ = cancellation_token.cancelled() => {
            Err(Error::Cancelled(solver_name.to_owned()))
        }
        result = child.wait() => {
            result.map_err(|e| Error::Conversion(ConversionError::from(e)))
        }
    };

    let status = status?;
    if !status.success() {
        return Err(ConversionError::CommandFailed(status).into());
    }
    Ok(())
}

fn get_mzn_to_fzn_cmd(
    args: &RunArgs,
    solver_name: &str,
    fzn_result_path: &Path,
    ozn_result_path: &Path,
) -> Command {
    let mut cmd = Command::new(&args.minizinc.minizinc_exe);
    cmd.kill_on_drop(true);
    #[cfg(unix)]
    cmd.process_group(0);
    cmd.arg("-c");
    cmd.arg(&args.model);
    if let Some(data) = &args.data {
        cmd.arg(data);
    }
    cmd.args(["--solver", solver_name]);
    cmd.arg("-o").arg(fzn_result_path);
    cmd.arg("--output-objective");
    cmd.arg("--output-mode");
    cmd.arg(args.output_mode.to_string());

    cmd.arg("--ozn");
    cmd.arg(ozn_result_path);

    cmd
}

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("conversion was cancelled for solver '{0}'")]
    Cancelled(String),
    #[error(transparent)]
    Conversion(#[from] ConversionError),
}

#[derive(Debug, thiserror::Error)]
pub enum ConversionError {
    #[error("command failed: {0}")]
    CommandFailed(std::process::ExitStatus),
    #[error("IO error during temporary file use")]
    TempFile(std::io::Error),
    #[error("IO error")]
    Io(#[from] tokio::io::Error),
}

impl IsCancelled for Error {
    fn is_cancelled(&self) -> bool {
        match self {
            Error::Cancelled(_) => true,
            Error::Conversion(_) => false,
        }
    }
}

impl<T> IsCancelled for Result<T> {
    fn is_cancelled(&self) -> bool {
        match self {
            Ok(_) => false,
            Err(e) => e.is_cancelled(),
        }
    }
}
