use crate::args::Args;
use crate::config::Config;
use crate::logging;
use crate::mzn_to_fzn::ConversionError;
use tokio::process::Command;

pub async fn run_backup_solver(args: &Args, cores: usize) {
    let config = Config::new(args);
    let mut cmd = Command::new(&args.minizinc_exe);
    cmd.arg("--solver").arg("cp-sat");

    cmd.arg(&args.model);
    if let Some(data) = &args.data {
        cmd.arg(data);
    }

    if let Some(solver_args) = config.solver_args.get("cp-sat") {
        for arg in solver_args {
            cmd.arg(arg);
        }
    }

    if args.output_objective {
        cmd.arg("--output-objective");
    }

    cmd.args(["--output-mode", "dzn"]);
    cmd.arg("-p").arg(cores.to_string());

    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            logging::error!(ConversionError::Io(e.into()).into());
            return;
        }
    };

    let status = match child.wait().await {
        Ok(s) => s,
        Err(e) => {
            logging::error!(ConversionError::Io(e.into()).into());
            return;
        }
    };

    if !status.success() {
        logging::error!(ConversionError::CommandFailed(status).into());
    }
}
