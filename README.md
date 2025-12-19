# Portfolio Solver Framework

## Building

Use `cargo build --release`. This will place the executable in `./target/release/portfolio-solver-framework`.

We also have a Dockerfile that installs it and sets it as the default `minizinc` solver.
Example usage:
```bash
docker build -t framework:latest . \
&& docker run  -v /my/path/to/problems:/problems framework minizinc /problems/accap/accap.mzn /problems/accap/accap_instance3.dzn
```

## Usage

### Prerequisites

- You need to have `minizinc` installed

### Running It

Use `<executable> --help` to see how to use the program, where `<executable>` is the path to the executable.

Some additional information about select options:
- `--ai`: When you use the `command-line` value, you also need to set `--ai-config command=<path_to_command>`. Also, there is an example Python AI in `command-line-ai/example.py`.
- `--static-schedule-path`: This is used to set the static schedule by path. An example of a static schedule file is provided in `static-schedules/example.csv`.

### As MiniZinc Solver

To make it available as a solver to MiniZinc, take the `solver.msc.template` and copy it to one of these paths:
- `/usr/share/minizinc/solvers` (on Linux only)
- `$HOME/.minizinc/solvers`
- For additional options, see the [MiniZinc documentation](https://docs.minizinc.dev/en/stable/fzn-spec.html#solver-configuration-files)

and remove the `.template` suffix from the file name and replace `${EXECUTABLE_PATH}` in the file with the path to the executable.

