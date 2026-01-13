use std::collections::HashMap;

use crate::{args::Args, solver_discovery};

#[derive(Debug, Clone)]
pub struct Config {
    pub dynamic_schedule_interval: u64,
    pub memory_enforcer_interval: u64,
    pub memory_threshold: f64,
    pub solver_args: HashMap<String, Vec<String>>,
}

impl Config {
    pub fn new(program_args: &Args, solvers: &solver_discovery::Solvers) -> Self {
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
            dynamic_schedule_interval: 5,
            memory_enforcer_interval: 3,
            memory_threshold: 0.9,
            solver_args,
        }
    }
}
