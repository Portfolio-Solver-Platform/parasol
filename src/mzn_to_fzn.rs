use std::path::{Path, PathBuf};
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use crate::input::DebugVerbosityLevel;

#[derive(Debug)]
pub enum ConversionError {
    CommandFailed(std::process::ExitStatus),
    Other(String),
}

impl From<tokio::io::Error> for ConversionError {
    fn from(value: tokio::io::Error) -> Self {
        Self::Other("Tokio IO error".to_owned())
    }
}

pub async fn convert_mzn_to_fzn(
    model: &Path,
    data: Option<&Path>,
    solver_name: &str,
    verbosity: DebugVerbosityLevel,
) -> Result<PathBuf, ConversionError> {
    let fzn_file_path = get_new_model_file_name(model, solver_name);
    run_mzn_to_fzn_cmd(&model, data, solver_name, &fzn_file_path, verbosity).await?;
    Ok(fzn_file_path)
}

fn get_new_model_file_name(model: &Path, solver_name: &str) -> PathBuf {
    let new_file_name = format!("_portfolio-model-{solver_name}.fzn");
    model.with_file_name(new_file_name)
}

async fn run_mzn_to_fzn_cmd(
    model: &Path,
    data: Option<&Path>,
    solver_name: &str,
    fzn_result_path: &Path,
    verbosity: DebugVerbosityLevel,
) -> Result<(), ConversionError> {
    let mut cmd = get_mzn_to_fzn_cmd(model, data, solver_name, fzn_result_path);
    cmd.stderr(Stdio::piped());

    let mut child = cmd.spawn()?;

    if let Some(stderr) = child.stderr.take() {
        tokio::spawn(async move {
            let reader = BufReader::new(stderr);
            let mut lines = reader.lines();
            while let Ok(Some(line)) = lines.next_line().await {
                if verbosity >= DebugVerbosityLevel::Warning {
                    eprintln!("MiniZinc compilation: {}", line);
                }
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
) -> Command {
    let mut cmd = Command::new("minizinc");

    cmd.args(["-c", "--no-output-ozn"]);
    cmd.arg(model);
    if let Some(data) = data {
        cmd.arg(data);
    }
    cmd.args(["--solver", solver_name]);
    cmd.arg("-o").arg(fzn_result_path);

    cmd
}
