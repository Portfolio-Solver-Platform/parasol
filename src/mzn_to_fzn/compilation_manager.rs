use std::collections::hash_map::Entry;
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use tokio::sync::RwLock;
use tokio::sync::watch;
use tokio::sync::watch::Receiver;
use tokio::sync::watch::error::SendError;
use tokio_util::sync::CancellationToken;

use super::Conversion;
use super::compilation;
use crate::args::RunArgs;
use crate::is_cancelled::{IsCancelled, IsErrorCancelled};
use crate::logging;

pub struct CompilationManager {
    args: Arc<RunArgs>,
    /// Invariant that needs to be upheld: If a started compilation is cancelled, it also needs to be removed.
    compilations: Arc<RwLock<HashMap<String, Compilation>>>,
    /// The cancellation token for the manager itself.
    /// If cancelled, the manager will stop working as intended, but it can be used to cancel all
    /// running processes at once.
    cancellation_token: CancellationToken,
}

#[derive(Clone, Debug)]
enum Compilation {
    Done(WaitForResult),
    Running(Arc<RunningCompilation>),
}

#[derive(Debug)]
struct RunningCompilation {
    cancellation_token: CancellationToken,
    receiver: Receiver<Option<WaitForResult>>,
}

impl CompilationManager {
    pub fn new(args: Arc<RunArgs>) -> Self {
        Self {
            args,
            cancellation_token: CancellationToken::new(),
            compilations: Default::default(),
        }
    }

    pub async fn start(&self, solver_name: String) {
        self.start_many([solver_name].into_iter()).await
    }

    pub async fn start_many(&self, solver_names: impl Iterator<Item = String>) {
        let mut compilations = self.compilations.write().await;
        let new_solvers = solver_names.filter(|name| !compilations.contains_key(name));

        let new_compilations: Vec<_> = new_solvers
            .map(|solver_name| {
                let cancellation_token = self.cancellation_token.child_token();
                let args = self.args.clone();
                let cancellation_token_clone = cancellation_token.clone();
                let name_clone = solver_name.clone();

                let compilations = self.compilations.clone();

                let (tx, rx) = watch::channel(None);

                tokio::spawn(async move {
                    logging::info!("starting compilation for solver '{solver_name}'");
                    let compilation =
                        compilation::convert_mzn(&args, &solver_name, cancellation_token_clone)
                            .await
                            .map_err(|e| {
                                let error = WaitForError::from(&e);
                                logging::error!(e.into());
                                error
                            })
                            .map(Arc::new);

                    if !compilation.is_error_cancelled() {
                        compilations
                            .write()
                            .await
                            .insert(solver_name.clone(), Compilation::Done(compilation.clone()));
                    }
                    // NOTE: If the compilation is cancelled, we do not here remove the started compilation from the
                    //       self.compilations map, because the only way the compilation gets cancelled is in stop_all,
                    //       which also removes it from the map.

                    let _ = tx.send(Some(compilation)).map_err(|e| {
                        logging::error!(Error::SendError(solver_name.clone(), e).into())
                    });
                    logging::info!("Compilation for solver '{solver_name}' is done");
                });

                (
                    name_clone,
                    RunningCompilation {
                        cancellation_token,
                        receiver: rx,
                    },
                )
            })
            .collect();

        for (name, compilation) in new_compilations {
            compilations.insert(name, Compilation::Running(Arc::new(compilation)));
        }
    }

    /// Cancellation safe
    pub async fn wait_for(&self, solver_name: &str) -> WaitForResult {
        let compilation = { self.compilations.read().await.get(solver_name).cloned() };

        let Some(compilation) = compilation else {
            return Err(WaitForError::NotStarted(solver_name.to_string()));
        };

        match compilation {
            Compilation::Done(result) => result,
            Compilation::Running(compilation) => {
                let mut rx = compilation.receiver.clone();
                let result = rx.wait_for(|value| value.is_some()).await;

                let Ok(value) = result else {
                    return Err(WaitForError::ReadChannelClosed(solver_name.to_string()));
                };

                let Some(compilation) = value.clone() else {
                    return Err(WaitForError::CompilationUnfinishedAfterWaiting(
                        solver_name.to_string(),
                    ));
                };

                compilation
            }
        }
    }

    pub async fn stop_many(&self, solver_names: impl Iterator<Item = String>) {
        let mut compilations = self.compilations.write().await;

        for solver_name in solver_names {
            if let Entry::Occupied(compilation) = compilations.entry(solver_name) {
                match compilation.get() {
                    Compilation::Running(started_compilation) => {
                        started_compilation.cancellation_token.cancel();
                        let (solver_name, _) = compilation.remove_entry();
                        logging::info!("stopped the compilation for solver '{solver_name}'");
                    }
                    Compilation::Done(_) => {
                        logging::info!("attempted to stop a finished compilation for a solver");
                    }
                }
            } else {
                logging::error_msg!(
                    "attempted to stop the compilation for a solver but a compilation is not registered for that solver (neither as started or finished)"
                );
            }
        }
    }

    /// Stop all running compilations except for the given solvers.
    pub async fn stop_all_except(&self, exception_solver_names: HashSet<String>) {
        let solvers_to_stop = {
            self.compilations
                .read()
                .await
                .iter()
                .filter(|(name, _)| !exception_solver_names.contains(*name))
                .filter(|(_, compilation)| !matches!(compilation, Compilation::Done(_)))
                .map(|(name, _)| name)
                .cloned()
                .collect::<Vec<_>>()
        };
        self.stop_many(solvers_to_stop.into_iter()).await;
    }
}

impl Drop for CompilationManager {
    fn drop(&mut self) {
        self.cancellation_token.cancel()
    }
}

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("failed to send to the channel for solver '{0}'")]
    SendError(String, #[source] SendError<Option<WaitForResult>>),
    #[error(transparent)]
    Compilation(#[from] compilation::Error),
}

#[derive(thiserror::Error, Debug, Clone)]
pub enum WaitForError {
    #[error("compilation was cancelled")]
    Cancelled,
    #[error(
        "a compilation for solver '{0}' was attempted to be retrieved, but one has not been started for that solver"
    )]
    NotStarted(String),
    #[error("the channel closed for the compilation for '{0}' while waiting for the result")]
    ReadChannelClosed(String),
    #[error("the compilation of solver '{0}' was still unfinished after waiting for it to be done")]
    CompilationUnfinishedAfterWaiting(String),
    #[error("waited for a failed compilation")]
    Conversion,
}
pub type WaitForResult = std::result::Result<Arc<Conversion>, WaitForError>;

impl IsCancelled for WaitForError {
    fn is_cancelled(&self) -> bool {
        matches!(self, Self::Cancelled)
    }
}

impl From<&compilation::Error> for WaitForError {
    fn from(value: &compilation::Error) -> Self {
        match value {
            super::Error::Cancelled(_) => Self::Cancelled,
            super::Error::Conversion(_) => Self::Conversion,
        }
    }
}
