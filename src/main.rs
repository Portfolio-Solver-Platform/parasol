mod input;
use clap::Parser;
use input::{Args, OutputMode};
mod minizinc_runner;
use minizinc_runner::run;
mod solver_output;

fn main() {
    let args = Args::parse();

    match args.output_mode {
        Some(OutputMode::Dzn) => println!("Mode set to DZN"),
        None => println!("No output mode specified"),
    }

    if args.output_objective {
        println!("Outputting objective value...");
    }

    if args.ignore_search {
        println!("-f flag received. Search strategy ignored (or acknowledged).");
    }

    if let Some(n) = args.threads {
        println!("Running with {n} threads");
    } else {
        println!("Running with default threads");
    }

    run(
        // "/Users/sofus/speciale/psp/problems/nfc/nfc.mzn",
        // "/Users/sofus/speciale/psp/problems/nfc/12_2_10.dzn",
        "coinbc", &args,
    );
}
