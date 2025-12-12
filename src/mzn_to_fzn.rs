use crate::args::DebugVerbosityLevel;
use dashmap::DashMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::RwLock;

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

pub struct CachedConverter {
    cache: DashMap<String, PathBuf>,
    ozn_cache: RwLock<Option<PathBuf>>,
    debug_verbosity: DebugVerbosityLevel,
}

impl CachedConverter {
    pub fn new(debug_verbosity: DebugVerbosityLevel) -> Self {
        Self {
            cache: DashMap::new(),
            ozn_cache: RwLock::new(None),
            debug_verbosity,
        }
    }

    pub async fn convert(
        &self,
        model: &Path,
        data: Option<&Path>,
        solver_name: &str,
    ) -> Result<PathBuf, ConversionError> {
        if let Some(fzn) = self.cache.get(solver_name) {
            // TODO: Avoid cloning by making a use_fzn_file function that implicitly converts if necessary
            return Ok(fzn.clone());
        }

        let output_ozn_file = !self.ozn_file_exists().await;
        let conversion = convert_mzn(
            model,
            data,
            solver_name,
            output_ozn_file,
            self.debug_verbosity,
        )
        .await?;
        self.cache
            .insert(solver_name.to_owned(), conversion.fzn.clone());

        if let Some(ozn) = conversion.ozn {
            self.set_ozn_file(ozn).await;
        }

        Ok(conversion.fzn)
    }

    pub async fn use_ozn_file(&self, f: impl FnOnce(Option<&Path>)) {
        let path = self.ozn_cache.read().await;
        f(path.as_deref());
    }

    async fn ozn_file_exists(&self) -> bool {
        self.ozn_cache.read().await.is_some()
    }

    async fn set_ozn_file(&self, path: PathBuf) {
        *self.ozn_cache.write().await = Some(path);
    }
}

pub struct Conversion {
    pub fzn: PathBuf,
    pub ozn: Option<PathBuf>,
}

pub async fn convert_mzn(
    model: &Path,
    data: Option<&Path>,
    solver_name: &str,
    output_ozn_file: bool,
    verbosity: DebugVerbosityLevel,
) -> Result<Conversion, ConversionError> {
    let fzn_file_path = get_new_model_file_name(model, solver_name);
    let ozn_file_path = output_ozn_file.then(|| get_new_ozn_file_name(model, solver_name));
    run_mzn_to_fzn_cmd(
        model,
        data,
        solver_name,
        &fzn_file_path,
        ozn_file_path.as_deref(),
        verbosity,
    )
    .await?;
    Ok(Conversion {
        fzn: fzn_file_path,
        ozn: ozn_file_path,
    })
}

fn get_new_model_file_name(model: &Path, solver_name: &str) -> PathBuf {
    let new_file_name = format!("_portfolio-model-{solver_name}.fzn");
    model.with_file_name(new_file_name)
}

fn get_new_ozn_file_name(model: &Path, solver_name: &str) -> PathBuf {
    let new_file_name = format!("_portfolio-model-{solver_name}.ozn");
    model.with_file_name(new_file_name)
}

async fn run_mzn_to_fzn_cmd(
    model: &Path,
    data: Option<&Path>,
    solver_name: &str,
    fzn_result_path: &Path,
    ozn_result_path: Option<&Path>,
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
    ozn_result_path: Option<&Path>,
) -> Command {
    let mut cmd = Command::new("minizinc");

    cmd.arg("-c");
    cmd.arg(model);
    if let Some(data) = data {
        cmd.arg(data);
    }
    cmd.args(["--solver", solver_name]);
    cmd.arg("-o").arg(fzn_result_path);
    cmd.args(["--output-objective", "--output-mode", "dzn"]);

    match ozn_result_path {
        Some(ozn_result_path) => {
            cmd.arg("--ozn");
            cmd.arg(ozn_result_path);
        }
        None => {
            cmd.arg("--no-output-ozn");
        }
    }

    cmd
}
