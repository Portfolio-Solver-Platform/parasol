use itertools::Itertools;

use super::{Error, Features, Result};
use crate::{logging, scheduler::Portfolio, static_schedule::parse_schedule};
use std::process::Command;

pub struct Ai {
    pub command_name: String,
}

impl Ai {
    pub fn new(command_name: String) -> Self {
        Self {
            command_name,
        }
    }
}

impl super::Ai for Ai {
    fn schedule(&mut self, features: &Features, cores: usize) -> Result<Portfolio> {
        logging::info!("AI info: Using command {}", self.command_name);
        let mut cmd = Command::new(&self.command_name);
        cmd.arg("-p").arg(cores.to_string());
        cmd.arg(features_to_arg(features));

        let output = cmd.output().map_err(|e| {
            Error::Other(format!(
                "Failed to get command output for '{}': {e}",
                self.command_name
            ))
        })?;

        log_stderr(output.stderr);

        if !output.status.success() {
            return Err(Error::Other(format!(
                "Command exited with non-zero status code: {}",
                output.status
            )));
        }

        let sched = parse_output_as_schedule(output.stdout);
        logging::info!("AI schedule: {:?}", sched);
        sched
    }
}

fn features_to_arg(features: &Features) -> String {
    features.iter().map(|feat| feat.to_string()).join(",")
}

fn parse_output_as_schedule(output: Vec<u8>) -> Result<Portfolio> {
    let output = String::from_utf8(output)
        .map_err(|_| Error::Other("Failed to convert command's stdout into a string".to_owned()))?;

    parse_schedule(&output).map_err(|e| Error::Other(format!("Failed to parse as schedule: {e}")))
}

fn log_stderr(stderr: Vec<u8>) {
    if stderr.is_empty() {
        return;
    }

    if logging::is_log_level(logging::LEVEL_ERROR) {
        match String::from_utf8(stderr) {
            Ok(stderr) => stderr
                .lines()
                .for_each(|line| logging::error_msg!("AI error: {line}")),
            Err(_) => logging::error_msg!("AI error: Failed to convert stderr to string"),
        }
    }
}
