use crate::args::Args;
use crate::logging;
use std::collections::HashMap;
use std::path::Path;
use std::process::Stdio;
use std::sync::Arc;
use tempfile::NamedTempFile;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::RwLock;
use tokio_util::sync::CancellationToken;

pub struct CachedConverter {
    args: Args,
    cache: RwLock<HashMap<String, Arc<Conversion>>>,
}

pub struct Conversion {
    fzn_file: NamedTempFile,
    ozn_file: NamedTempFile,
}

impl Conversion {
    pub fn fzn(&self) -> &Path {
        self.fzn_file.path()
    }

    pub fn ozn(&self) -> &Path {
        self.ozn_file.path()
    }
}

impl CachedConverter {
    pub fn new(args: Args) -> Self {
        Self {
            args,
            cache: RwLock::new(HashMap::new()),
        }
    }

    pub async fn convert(&self, solver_name: &str, cancellation_token: Option<CancellationToken>) -> Result<Arc<Conversion>> {
        {
            let cache = self.cache.read().await;
            if let Some(conversion) = cache.get(solver_name) {
                return Result::Ok(conversion.clone());
            }
        }

        let conversion = Arc::new(convert_mzn(&self.args, solver_name, cancellation_token).await?);
        let mut cache = self.cache.write().await;
        cache.insert(solver_name.to_owned(), conversion.clone());
        Ok(conversion)
    }
}

pub async fn convert_mzn(args: &Args, solver_name: &str, cancellation_token: Option<CancellationToken>) -> Result<Conversion> {
    let fzn_file = tempfile::Builder::new()
        .suffix(".fzn")
        .tempfile()
        .map_err(ConversionError::TempFile)?;
    let ozn_file = tempfile::Builder::new()
        .suffix(".ozn")
        .tempfile()
        .map_err(ConversionError::TempFile)?;

    run_mzn_to_fzn_cmd(args, solver_name, fzn_file.path(), ozn_file.path(), cancellation_token).await?;

    Ok(Conversion { fzn_file, ozn_file })
}

async fn run_mzn_to_fzn_cmd(
    args: &Args,
    solver_name: &str,
    fzn_result_path: &Path,
    ozn_result_path: &Path,
    cancellation_token: Option<CancellationToken>,
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

    let status = if let Some(token) = cancellation_token {
        tokio::select! {
            _ = token.cancelled() => {
                Err(Error::Cancelled)
            }
            result = child.wait() => {
                result.map_err(|e| Error::Conversion(ConversionError::from(e)))
            }
        }
    } else {
        child.wait().await.map_err(|e| Error::Conversion(ConversionError::from(e)))
    };

    let status = status?;
    if !status.success() {
        return Err(ConversionError::CommandFailed(status).into());
    }
    Ok(())
}

fn get_mzn_to_fzn_cmd(
    args: &Args,
    solver_name: &str,
    fzn_result_path: &Path,
    ozn_result_path: &Path,
) -> Command {
    let mut cmd = Command::new(&args.minizinc_exe);
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
    #[error("conversion was cancelled")]
    Cancelled,
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
