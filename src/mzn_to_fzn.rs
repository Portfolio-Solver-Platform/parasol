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

#[derive(Debug, thiserror::Error)]
pub enum ConversionError {
    #[error("command failed: {0}")]
    CommandFailed(std::process::ExitStatus),
    #[error("IO error during temporary file use")]
    TempFile(std::io::Error),
    #[error("IO error")]
    Io(#[from] tokio::io::Error),
}

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

    pub async fn convert(&self, solver_name: &str) -> Result<Arc<Conversion>, ConversionError> {
        {
            let cache = self.cache.read().await;
            if let Some(conversion) = cache.get(solver_name) {
                return Ok(conversion.clone());
            }
        }

        let conversion = Arc::new(convert_mzn(&self.args, solver_name).await?);
        let mut cache = self.cache.write().await;
        cache.insert(solver_name.to_owned(), conversion.clone());

        Ok(conversion)
    }
}

pub async fn convert_mzn(args: &Args, solver_name: &str) -> Result<Conversion, ConversionError> {
    let fzn_file = tempfile::Builder::new()
        .suffix(".fzn")
        .tempfile()
        .map_err(ConversionError::TempFile)?;
    let ozn_file = tempfile::Builder::new()
        .suffix(".ozn")
        .tempfile()
        .map_err(ConversionError::TempFile)?;

    run_mzn_to_fzn_cmd(args, solver_name, fzn_file.path(), ozn_file.path()).await?;

    Ok(Conversion { fzn_file, ozn_file })
}

async fn run_mzn_to_fzn_cmd(
    args: &Args,
    solver_name: &str,
    fzn_result_path: &Path,
    ozn_result_path: &Path,
) -> Result<(), ConversionError> {
    let mut cmd = get_mzn_to_fzn_cmd(args, solver_name, fzn_result_path, ozn_result_path);
    cmd.stderr(Stdio::piped());

    let mut child = cmd.spawn()?;

    if args.debug_verbosity >= crate::args::DebugVerbosityLevel::Warning
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

    let status = child.wait().await?;
    if !status.success() {
        return Err(ConversionError::CommandFailed(status));
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
    if let Some(output_mode) = &args.output_mode {
        cmd.arg("--output-mode");
        cmd.arg(output_mode.to_string());
    } else {
        cmd.args(["--output-mode", "dzn"]);
    }

    cmd.arg("--ozn");
    cmd.arg(ozn_result_path);

    cmd
}
