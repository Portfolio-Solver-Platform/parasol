# Parasol

A MiniZinc portfolio solver.

## Usage

### Docker
The easiest approach to using Parasol is through Docker. You run the official Docker image like this:
```bash
docker run -v /my/path/to/problems:/problems jobork/parasol <command>
```
In this command, `-v /my/path/to/problems:/problems` allows you to include your own problems to solve in the container and `<command>` is the command you want to execute inside the container. An example of such a command is: `minizinc /problems/accap/accap.mzn /problems/accap/accap_instance3.dzn`.
Parasol is set as the default solver to `minizinc` in the Dockerfile, so this example runs Parasol on the given model and data file.
Alternatively, if you want to run Parasol directly, use the command `parasol run`, for example: `parasol run /problems/accap/accap.mzn /problems/accap/accap_instance3.dzn`. For additional information on usage, use `parasol --help`.

The official Docker image comes preinstalled with some solvers, but if you want to run it with your own solvers, you need to modify the Dockerfile and build it yourself. You build it by executing the following command in the root of the repository:
```bash
docker build -t parasol .
```
You can use the build argument `MAKE_JOBS` to set the number of jobs `make` is allowed to run concurrently in the Dockerfile. Example: `--build-arg MAKE_JOBS=8`.

If you want to use CPLEX with Parasol, due to licensing reasons, it cannot be included in the Dockerfile by default. Instead, you need to provide CPLEX to the Dockerfile yourself. For details on how to do this, see the note near the bottom of the Dockerfile (search for "CPLEX" to find the note). 

The same can be done for FICO Xpress, instead search for "Xpress".



### Direct

This section will describe how to run Parasol's binary executable.

Prerequisites:
- You are on a Unix system
- You need to have `minizinc` installed
  - The solvers you want to use in the portfolio need to be installed for use in `minizinc`.

When using Parasol directly, you first need to build it with cargo: `cargo build --release`. This will place the executable in `./target/release/parasol`.

Use `<executable> --help` to see how to use the program, where `<executable>` is the path to the executable.

To make it available as a solver to MiniZinc, take the `minizinc/solvers/parasol.msc.template` and copy it to one of these paths:
- `/usr/share/minizinc/solvers` (on Linux only)
- `$HOME/.minizinc/solvers`
- For additional options, see the [MiniZinc documentation](https://docs.minizinc.dev/en/stable/fzn-spec.html#solver-configuration-files)

and remove the `.template` suffix from the file name and replace `${EXE_PATH}` in the file with the path to the executable.

## Options

Some additional information about select options:
- `--ai`: When you use the `command-line` value, you also need to set `--ai-config command=<path_to_command>`. Also, there is an example Python AI in `command-line-ai/example.py`.
- `--static-schedule-path`: This is used to set the static schedule by path. An example of a static schedule file is provided in `static-schedules/example.csv`.
