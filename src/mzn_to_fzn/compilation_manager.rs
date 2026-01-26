use std::collections::hash_map::Entry;
use std::{collections::HashMap, sync::Arc};

use tokio::sync::RwLock;
use tokio::sync::watch;
use tokio::sync::watch::Receiver;
use tokio_util::sync::CancellationToken;

use super::Conversion;
use super::compilation;
use crate::args;
use crate::args::RunArgs;
use crate::is_cancelled::IsCancelled;
use crate::logging;

pub struct CompilationManager {
    args: Arc<RunArgs>,
    compilations: Arc<RwLock<HashMap<String, Compilation>>>,
    /// The cancellation token for the manager itself.
    /// If cancelled, the manager will stop working as intended, but it can be used to cancel all
    /// running processes at once.
    cancellation_token: CancellationToken,
}

#[derive(Clone, Debug)]
enum Compilation {
    Done(Arc<Result<Conversion>>),
    Started(Arc<StartedCompilation>),
}

#[derive(Debug)]
struct StartedCompilation {
    cancellation_token: CancellationToken,
    receiver: Receiver<Option<Arc<Result<Conversion>>>>,
}

impl CompilationManager {
    pub async fn is_started(&self, solver_name: &str) -> bool {
        self.compilations.read().await.contains_key(solver_name)
    }

    pub async fn start(&self, solver_name: String) {
        self.start_all([solver_name].into_iter()).await
    }

    pub async fn start_all(&self, solver_names: impl Iterator<Item = String>) {
        let new_solvers: Vec<_> = {
            let compilations = self.compilations.read().await;
            solver_names
                .filter(|name| !compilations.contains_key(name))
                .collect()
        };

        if self.args.verbosity >= args::Verbosity::Info {
            new_solvers.iter().for_each(|solver_name| logging::info!("Attempted to start compiling for '{solver_name}' even though it has already started compilation or is done compiling"));
        }

        let new_compilations = new_solvers.into_iter().map(|solver_name| {
            let cancellation_token = self.cancellation_token.child_token();
            let args = self.args.clone();
            let cancellation_token_clone = cancellation_token.clone();
            let name_clone = solver_name.clone();

            let compilations = self.compilations.clone();

            let (tx, rx) = watch::channel(None);

            tokio::spawn(async move {
                let compilation = Arc::new(
                    compilation::convert_mzn(&args, &solver_name, cancellation_token_clone)
                        .await
                        .map_err(Error::from),
                );

                if !compilation.is_cancelled() {
                    compilations
                        .write()
                        .await
                        .insert(solver_name, Compilation::Done(compilation.clone()));
                }

                let _ = tx.send(Some(compilation));
            });

            (
                name_clone,
                StartedCompilation {
                    cancellation_token,
                    receiver: rx,
                },
            )
        });

        let mut compilations = self.compilations.write().await;
        for (name, compilation) in new_compilations {
            compilations.insert(name, Compilation::Started(Arc::new(compilation)));
        }
    }

    pub async fn wait_for(
        &self,
        solver_name: &str,
        cancellation_token: CancellationToken,
    ) -> Arc<Result<Conversion>> {
        let compilation = {
            tokio::select! {
                compilations = self.compilations.read() => {
                    Cancellable::Done(compilations.get(solver_name).cloned())
                },
                _ = cancellation_token.cancelled() => {
                    Cancellable::Cancelled
                }
            }
        };

        let Cancellable::Done(compilation) = compilation else {
            return Arc::new(Err(Error::Cancelled));
        };

        let Some(compilation) = compilation else {
            return Arc::new(Err(Error::NotStarted(solver_name.to_string())));
        };

        match compilation {
            Compilation::Done(result) => result,
            Compilation::Started(compilation) => {
                let mut rx = compilation.receiver.clone();
                let wait_for_compilation = rx.wait_for(|value| value.is_some());
                let result = tokio::select! {
                    result = wait_for_compilation => Cancellable::Done(result),
                    _ = cancellation_token.cancelled() => Cancellable::Cancelled
                };

                let Cancellable::Done(result) = result else {
                    return Arc::new(Err(Error::Cancelled));
                };

                let Ok(value) = result else {
                    return Arc::new(Err(Error::ChannelClosed(solver_name.to_string())));
                };

                let Some(compilation) = value.clone() else {
                    return Arc::new(Err(Error::CompilationUnfinishedAfterWaiting(
                        solver_name.to_string(),
                    )));
                };

                compilation
            }
        }
    }

    pub async fn stop_all(&self, solver_names: impl Iterator<Item = String>) {
        let mut compilations = self.compilations.write().await;

        for solver_name in solver_names {
            if let Entry::Occupied(compilation) = compilations.entry(solver_name) {
                match compilation.get() {
                    Compilation::Started(started_compilation) => {
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
}

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("compilation was cancelled")]
    Cancelled,
    #[error(
        "a compilation for solver '{0}' was attempted to be retrieved, but one has not been started for that solver"
    )]
    NotStarted(String),
    #[error("the channel closed for the compilation for '{0}' while waiting for the result")]
    ChannelClosed(String),
    #[error("the compilation of solver '{0}' was still unfinished after waiting for it to be done")]
    CompilationUnfinishedAfterWaiting(String),
    #[error(transparent)]
    Compilation(#[from] compilation::Error),
}
pub type Result<T> = std::result::Result<T, Error>;

enum Cancellable<T> {
    Done(T),
    Cancelled,
}

impl IsCancelled for Error {
    fn is_cancelled(&self) -> bool {
        match self {
            Error::Cancelled => true,
            _ => false,
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
