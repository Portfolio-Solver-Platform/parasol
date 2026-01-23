use std::collections::HashMap;

use crate::{args::RunArgs, solver_config};

#[derive(Debug, Clone)]
pub struct Config {
    pub memory_enforcer_interval: u64,
    pub memory_threshold: f64,
    pub solver_args: HashMap<String, Vec<String>>,
}

impl Config {
    pub fn new(program_args: &RunArgs, solvers: &solver_config::Solvers) -> Self {
        let mut solver_args = HashMap::new();

        for solver in solvers.iter() {
            let supported_flags = solver.supported_std_flags();
            let mut args: Vec<String> = vec![];

            if supported_flags.i {
                args.push("-i".to_owned());
            } else if supported_flags.a {
                args.push("-a".to_owned());
            }

            if program_args.ignore_search && supported_flags.f {
                args.push("-f".to_string());
            }

            solver_args.insert(solver.id().to_owned(), args);
        }

        Self {
            memory_enforcer_interval: 3,
            memory_threshold: 0.9,
            solver_args,
        }
    }
}
