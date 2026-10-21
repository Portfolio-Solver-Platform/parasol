# Parasol

Parasol is a portfolio executor for MiniZinc, written in Rust. It takes care of running a portfolio of solvers, so a portfolio selector only has to decide which solvers to run and with how many cores. Parasol can be installed as a normal MiniZinc solver.

To show what Parasol can do, we built a selector on top of it and got second place (silver) in the Open category of the [MiniZinc Challenge 2026](https://www.minizinc.org/challenge/2026/results/), only behind Google's OR-Tools CP-SAT. See [MiniZinc Challenge 2026](#minizinc-challenge-2026).

## How it works

No single solver is best on every problem, so a portfolio solver picks which solvers to run for each problem. This can be one solver on all cores or several solvers in parallel.

A run has three steps:
1. A static schedule of solvers can start right away. Meanwhile Parasol compiles the model on one core and extracts its features with [fzn2feat](https://github.com/CP-Unibo/mzn2feat).
2. The selector gets the features and the number of cores, and returns a portfolio, which is a list of solvers with a number of cores each. If extracting the features takes too long, a timeout schedule can be used instead.
3. Parasol switches from the static schedule to the portfolio, and starts, suspends and resumes solvers as needed. Every few seconds, solvers that are behind are restarted with the best objective found so far as a bound.

Parasol also:
- finds the solvers installed for MiniZinc, or loads them from a cache
- compiles the model for each solver in parallel, and can compile ahead for solvers the selector is likely to pick (`--compilation-priority`)
- controls each solver's whole process tree, so suspending or killing a solver also hits its child processes
- kills solvers if memory runs low (`--enforce-memory`), can pin solvers to their own cores (`--pin-solvers`), and falls back to CP-SAT if something fails (`--backup-solver`)

## Using your own selector

You choose the selector with `--ai`:

| `--ai` | Selector |
|---|---|
| `none` | Only runs the static schedule |
| `simple` | One core each for the first solvers it knows (default) |
| `command-line` | Your own program, see below |
| `svc-k1`, `svc-ek1` | Our selectors from the MiniZinc Challenge, built into the binary |

With `command-line`, Parasol runs your program with the number of cores and the features as a comma separated list:

```bash
parasol sso model.mzn data.dzn --ai command-line --ai-config command=./my-selector
# Parasol then runs: ./my-selector -p <cores> <f1>,<f2>,...,<fn>
```

Your program should print one `<solver-id>,<cores>` line per solver:

```
cp-sat,4
org.gecode.gecode,2
org.minizinc.mip.gurobi,2
```

There is a minimal example in [`command-line-ai/example.py`](command-line-ai/example.py). You can also build a selector into Parasol by implementing the `Ai` trait in [`src/ai.rs`](src/ai.rs).

## MiniZinc Challenge 2026

Our selector picks between running CP-SAT on all 8 cores and running a portfolio of other solvers. It is a bagged ensemble of 30 support vector classifiers (SVCs) with RBF kernels, trained on 95 features. Everything else is handled by Parasol. The training code and experiments are in [ai-tools](https://github.com/Portfolio-Solver-Platform/ai-tools).

We submitted two versions, which have different solvers in the portfolio:

| Solver | Parasol-e (`svc-ek1`) | Parasol-i (`svc-k1`) |
|---|:-:|:-:|
| OR-Tools CP-SAT | 1 | 1 |
| Gurobi | 2 | 2 |
| Picat | 1 | 1 |
| Yuck | 1 | 1 |
| Pumpkin | 1 | |
| Dexter | 1 | |
| Choco | 1 | |
| Chuffed | | 1 |
| Gecode | | 2 |

The numbers are cores. The configurations are in [`minizinc/solvers/`](minizinc/solvers/).

## Usage

### Docker
The easiest approach to using Parasol is through Docker. You run the official Docker image like this:
```bash
docker run -v /my/path/to/problems:/problems ghcr.io/portfolio-solver-platform/parasol:latest <command>
```
In this command, `-v /my/path/to/problems:/problems` allows you to include your own problems to solve in the container and `<command>` is the command you want to execute inside the container. An example of such a command is: `minizinc /problems/accap/accap.mzn /problems/accap/accap_instance3.dzn`.
Parasol is set as the default solver to `minizinc` in the Dockerfile, so this example runs Parasol on the given model and data file.
Alternatively, if you want to run Parasol directly, use the command `parasol sso`, for example: `parasol sso /problems/accap/accap.mzn /problems/accap/accap_instance3.dzn`. For additional information on usage, use `parasol sso --help`.

The official Docker image comes preinstalled with some solvers, but if you want to run it with your own solvers, you need to modify the Dockerfile and build it yourself. You build it by executing the following command in the root of the repository:
```bash
docker build -t parasol .
```
You can use the build argument `MAKE_JOBS` to set the number of jobs `make` is allowed to run concurrently in the Dockerfile. Example: `--build-arg MAKE_JOBS=8`.

If you want to use CPLEX with Parasol, due to licensing reasons, it cannot be included in the Dockerfile by default. Instead, you need to provide CPLEX to the Dockerfile yourself. For details on how to do this, see the note near the bottom of the Dockerfile (search for "CPLEX" to find the note). 

The same can be done for FICO Xpress and Gurobi, instead search for "Xpress" and "Gurobi", respectively.

### Direct

This section will describe how to run Parasol's binary executable.

Prerequisites:
- You are on a Unix system
- You need to have `minizinc` installed
  - The solvers you want to use in the portfolio need to be installed for use in `minizinc`.
- To use a selector, you need to have `fzn2feat` from [mzn2feat](https://github.com/CP-Unibo/mzn2feat) on your `PATH`.

When using Parasol directly, you first need to build it with cargo: `cargo build --release`. This will place the executable in `./target/release/parasol`.

Use `<executable> sso --help` to see how to use the program, where `<executable>` is the path to the executable.

To make it available as a solver to MiniZinc, take the `minizinc/solvers/parasol.msc.template` and copy it to one of these paths:
- `/usr/share/minizinc/solvers` (on Linux only)
- `$HOME/.minizinc/solvers`
- For additional options, see the [MiniZinc documentation](https://docs.minizinc.dev/en/stable/fzn-spec.html#solver-configuration-files)

and remove the `.template` suffix from the file name and replace `${EXE_PATH}` in the file with the path to the executable.

### Static schedules

You set the static schedule with `--static-schedule` together with `--static-runtime`. The file is a CSV without a header, with one `<solver-id>,<cores>` line per solver. There are examples in [`static-schedules/`](static-schedules/).

## Authors

Parasol was made as part of our master's thesis at the University of Southern Denmark by [Jonas Bork Jørgensen](https://github.com/jonas-bork) and [Sofus Hesseldahl Laubel](https://github.com/sofusl90), supervised by Jacopo Mauro and in collaboration with Alessio Pellegrino. It is part of the [Portfolio Solver Platform](https://github.com/Portfolio-Solver-Platform).
