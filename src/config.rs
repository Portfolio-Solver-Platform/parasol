use std::{collections::HashMap, sync::Arc};

use crate::solver_config;

#[derive(Debug, Clone)]
pub struct Config {
    pub solver_args: Arc<HashMap<String, Vec<String>>>,
}

impl Config {
    pub fn new(solvers: &solver_config::Solvers) -> Self {
        let mut solver_args = HashMap::new();

        for solver in solvers.iter() {
            let supported_flags = solver.supported_std_flags();
            let mut args: Vec<String> = vec![];

            if supported_flags.i {
                args.push("-i".to_owned());
            } else if supported_flags.a {
                args.push("-a".to_owned());
            }

            if supported_flags.f {
                args.push("-f".to_string());
            }

            solver_args.insert(solver.id().to_owned(), args);
        }

        Self {
            solver_args: Arc::new(solver_args),
        }
    }
}
