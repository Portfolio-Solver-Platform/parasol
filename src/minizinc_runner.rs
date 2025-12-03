use crate::input::{Args, OutputMode};
use crate::solver_output;
use std::fmt::format;
use std::io;
use std::process::Command;

pub fn run(solver: &str, args: &Args) -> io::Result<()> {
    // let output = Command::new("minizinc")
    //     .arg("--solver")
    //     .arg(solver)
    //     .arg(model_path)
    //     .arg(data_path)
    //     .output()?;
    let mut cmd = Command::new("minizinc");
    cmd.arg("--solver").arg(solver);
    cmd.arg(&args.model);

    if let Some(data_path) = &args.data {
        cmd.arg(data_path);
    }

    match args.output_mode {
        Some(OutputMode::Dzn) => {
            cmd.arg("--output-mode dzn");
        }
        None => {}
    }

    if args.output_objective {
        cmd.arg("--output-objective");
    }

    if args.ignore_search {
        cmd.arg("-f");
    }

    if let Some(threads) = &args.threads {
        cmd.arg(format!("-p {threads}"));
    }

    let output = cmd.output()?;
    if output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        println!("Solver Output:\n{}", stdout);

        let parsed_output = solver_output::Output::parse(stdout);

        // Optional: Parse the output here.
        // Tip: Use the JSON output flag (--json-stream) for easier parsing in Rust!
    } else {
        let stderr: std::borrow::Cow<'_, str> = String::from_utf8_lossy(&output.stderr);
        eprintln!("Error running solver:\n{}", stderr);
    }

    Ok(())
}
