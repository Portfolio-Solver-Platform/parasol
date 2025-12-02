use clap::{Parser, ValueEnum};

#[derive(Debug, Clone, ValueEnum)]
enum OutputMode {
    Dzn,
}

#[derive(Parser, Debug)]
#[command(author, version, about)]
struct Args {
    #[arg(long, value_enum)]
    output_mode: Option<OutputMode>,

    #[arg(long)]
    output_objective: bool,

    #[arg(short = 'f')]
    ignore_search: bool,

    #[arg(short = 'p')]
    threads: Option<usize>,
}

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
        println!("Running with {} threads", n);
    } else {
        println!("Running with default threads");
    }
}