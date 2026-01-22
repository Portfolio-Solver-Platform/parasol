use std::{collections::HashMap, sync::Arc};

use tokio::{sync::RwLock, task::JoinHandle};
use tokio_util::sync::CancellationToken;

use crate::args;
use crate::args::Args;
use crate::logging;
use super::Conversion;
use super::compilation;

pub struct CompilationManager {
    args: Arc<Args>,
    compilations: RwLock<HashMap<String, Compilation>>,
}

enum Compilation {
    Done(Arc<Conversion>),
    Started(StartedCompilation)
}

struct StartedCompilation(CancellationToken, JoinHandle<compilation::Result<Conversion>>);

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
            solver_names.filter(|name| !compilations.contains_key(name)).collect()
        };

        if self.args.verbosity >= args::Verbosity::Info {
            new_solvers.iter().for_each(|solver_name| logging::info!("Attempted to start compiling for '{solver_name}' even though it has already started compilation or is done compiling"));
        }

        let new_compilations = new_solvers.into_iter().map(|solver_name| {
            let cancellation_token = CancellationToken::new();
            let args = self.args.clone();
            let cancellation_token_clone = cancellation_token.clone();
            let name_clone = solver_name.clone();
            let compilation = tokio::spawn( async move {
                compilation::convert_mzn(&args, &solver_name, cancellation_token_clone).await
            });
            (name_clone, StartedCompilation(cancellation_token, compilation))
        });

        let mut compilations = self.compilations.write().await;
        for (name, compilation) in new_compilations {
            compilations.insert(name, compilation.into());
        }
    }

    pub async fn get(&self, solver_name: &str, cancellation_token: Option<CancellationToken>) -> compilation::Result<Conversion> {
        todo!()
    }

    pub async fn stop_all(&self) {
        todo!()
    }
}

pub enum Cancellable<T> {
    Done(T),
    Cancelled
}

impl From<StartedCompilation> for Compilation {
    fn from(value: StartedCompilation) -> Self {
        Self::Started(value)
    }
}
