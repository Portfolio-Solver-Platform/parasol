use crate::ai::Features;
use std::path::PathBuf;
use tokio::process::Command;

#[derive(Debug)]
pub enum Error {
    CommandFailed(std::process::ExitStatus),
    FeatureParseFailed(String, std::num::ParseFloatError),
    Other(String),
}

impl From<tokio::io::Error> for Error {
    fn from(value: tokio::io::Error) -> Self {
        Self::Other("Tokio IO error".to_owned())
    }
}

pub async fn fzn_to_features(fzn_model: &PathBuf) -> Result<Features, Error> {
    let output = run_fzn_to_feat_cmd(fzn_model).await?;
    output
        .replace("\n", "")
        .split(",")
        .map(|s| s.parse::<f32>())
        .collect::<Result<Features, _>>()
        .map_err(|e| Error::FeatureParseFailed(output, e))
}

async fn run_fzn_to_feat_cmd(fzn_model: &PathBuf) -> Result<String, Error> {
    let mut cmd = get_fzn_to_feat_cmd(fzn_model);
    let output = cmd.output().await?;
    if !output.status.success() {
        return Err(Error::CommandFailed(output.status));
    }

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

fn get_fzn_to_feat_cmd(fzn_model: &PathBuf) -> Command {
    let mut cmd = Command::new("mzn2feat");
    cmd.kill_on_drop(true);
    cmd.arg("-i");
    cmd.arg(fzn_model);

    cmd
}
