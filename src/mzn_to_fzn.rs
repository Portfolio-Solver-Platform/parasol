use crate::args::DebugVerbosityLevel;
use std::collections::HashMap;
use std::path::Path;
use std::process::Stdio;
use std::sync::Arc;
use tempfile::NamedTempFile;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::RwLock;

#[derive(Debug)]
pub enum ConversionError {
    CommandFailed(std::process::ExitStatus),
    TempFile(std::io::Error),
    Other(String),
}

impl From<tokio::io::Error> for ConversionError {
    fn from(value: tokio::io::Error) -> Self {
        Self::Other("Tokio IO error".to_owned())
    }
}

pub struct CachedConverter {
    cache: RwLock<HashMap<String, Arc<Conversion>>>,
    debug_verbosity: DebugVerbosityLevel,
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
    pub fn new(debug_verbosity: DebugVerbosityLevel) -> Self {
        Self {
            cache: RwLock::new(HashMap::new()),
            debug_verbosity,
        }
    }

    pub async fn convert(
        &self,
        model: &Path,
        data: Option<&Path>,
        solver_name: &str,
    ) -> Result<Arc<Conversion>, ConversionError> {
        {
            let cache = self.cache.read().await;
            if let Some(conversion) = cache.get(solver_name) {
                return Ok(conversion.clone());
            }
        }

        let conversion =
            Arc::new(convert_mzn(model, data, solver_name, self.debug_verbosity).await?);
        let mut cache = self.cache.write().await;
        cache.insert(solver_name.to_owned(), conversion.clone());

        Ok(conversion)
    }
}

pub async fn convert_mzn(
    model: &Path,
    data: Option<&Path>,
    solver_name: &str,
    verbosity: DebugVerbosityLevel,
) -> Result<Conversion, ConversionError> {
    let fzn_file = tempfile::Builder::new()
        .suffix(".fzn")
        .tempfile()
        .map_err(ConversionError::TempFile)?;
    let ozn_file = tempfile::Builder::new()
        .suffix(".ozn")
        .tempfile()
        .map_err(ConversionError::TempFile)?;

    run_mzn_to_fzn_cmd(
        model,
        data,
        solver_name,
        fzn_file.path(),
        ozn_file.path(),
        verbosity,
    )
    .await?;

    Ok(Conversion { fzn_file, ozn_file })
}

async fn run_mzn_to_fzn_cmd(
    model: &Path,
    data: Option<&Path>,
    solver_name: &str,
    fzn_result_path: &Path,
    ozn_result_path: &Path,
    verbosity: DebugVerbosityLevel,
) -> Result<(), ConversionError> {
    let mut cmd = get_mzn_to_fzn_cmd(model, data, solver_name, fzn_result_path, ozn_result_path);
    cmd.stderr(Stdio::piped());

    let mut child = cmd.spawn()?;

    if verbosity >= DebugVerbosityLevel::Warning
        && let Some(stderr) = child.stderr.take()
    {
        tokio::spawn(async move {
            let reader = BufReader::new(stderr);
            let mut lines = reader.lines();
            while let Ok(Some(line)) = lines.next_line().await {
                eprintln!("MiniZinc compilation: {}", line);
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
    model: &Path,
    data: Option<&Path>,
    solver_name: &str,
    fzn_result_path: &Path,
    ozn_result_path: &Path,
) -> Command {
    let mut cmd = Command::new("minizinc");
    cmd.kill_on_drop(true);
    cmd.arg("-c");
    cmd.arg(model);
    if let Some(data) = data {
        cmd.arg(data);
    }
    cmd.args(["--solver", solver_name]);
    cmd.arg("-o").arg(fzn_result_path);
    cmd.args(["--output-objective", "--output-mode", "dzn"]);

    cmd.arg("--ozn");
    cmd.arg(ozn_result_path);

    cmd
}
