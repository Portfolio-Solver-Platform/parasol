# Parasol

A multi-threaded, low-overhead universal portfolio executor for MiniZinc, written in Rust.

No single constraint solver is best on every problem, so portfolio solvers choose which solvers to run for each problem: one solver on all cores, or several in parallel. Parasol does the execution: it compiles the model for each solver, runs the solvers and coordinates them. A portfolio selector then only has to decide *which solvers to run, on how many cores*. Parasol installs as a regular MiniZinc solver.

**Showcase:** an SVC selector built on Parasol won **silver in the [MiniZinc Challenge 2026](https://www.minizinc.org/challenge/2026/results/) (Open category)**, second only to Google's OR-Tools CP-SAT. [More below](#showcase-minizinc-challenge-2026).

## How it works

1. **Pre-solving.** An optional static schedule of solvers starts right away. Meanwhile, Parasol compiles the model on one core and extracts its features with [fzn2feat](https://github.com/CP-Unibo/mzn2feat).
2. **Selection.** The selector receives the features and the core budget, and returns a portfolio: a list of solvers with a number of cores each. If feature extraction takes too long, an optional timeout schedule is used instead.
3. **Execution.** Parasol switches from the static schedule to the selected portfolio, starting, suspending and resuming solvers as needed. At a fixed interval, solvers that are behind are restarted with the best objective found so far as a bound.

Along the way, Parasol also:
- discovers the solvers installed for MiniZinc, or loads them from a prebuilt cache
- compiles the model for each solver in parallel, and pre-compiles for solvers a selector is likely to pick (`--compilation-priority`) without taking cores from running solvers
- manages each solver as a whole process tree, so suspending or killing a solver also reaches its child processes
- kills solvers when memory runs low (`--enforce-memory`), can pin solvers to dedicated cores (`--pin-solvers`), and falls back to CP-SAT if the orchestrator fails (`--backup-solver`)

## Plug in your own selector

Choose the selector with `--ai`:

| `--ai` | Selector |
|---|---|
| `none` | Only the static schedule |
| `simple` | One core each for the first known solvers (default) |
| `command-line` | Any external program, see below |
| `svc-k1`, `svc-ek1` | Our challenge selectors, embedded in the binary (see [Showcase](#showcase-minizinc-challenge-2026)) |

With `command-line`, Parasol runs your program with the core budget and the comma-separated features:

```bash
parasol sso model.mzn data.dzn --ai command-line --ai-config command=./my-selector
# Parasol then calls: ./my-selector -p <cores> <f1>,<f2>,...,<fn>
```

The program prints one `<solver-id>,<cores>` line per solver to stdout:

```
cp-sat,4
org.gecode.gecode,2
org.minizinc.mip.gurobi,2
```

See [`command-line-ai/example.py`](command-line-ai/example.py) for a minimal selector. To build a selector into Parasol itself, implement the `Ai` trait in [`src/ai.rs`](src/ai.rs).

## Showcase: MiniZinc Challenge 2026

To show what Parasol can do, we built a selector on top of it and entered it in the [MiniZinc Challenge 2026](https://www.minizinc.org/challenge/2026/results/), where it won silver in the Open category, second only to Google's OR-Tools CP-SAT.

The selector makes a single choice per instance: run CP-SAT on all 8 cores, or run a portfolio of complementary solvers. It is a bagged ensemble of 30 support vector classifiers (SVCs) with RBF kernels over 95 instance features. Everything else, from compiling the model to running and coordinating the solvers, is Parasol. Training code and experiments are in [ai-tools](https://github.com/Portfolio-Solver-Platform/ai-tools).

We entered two configurations, which differ in the portfolio the selector can choose:

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

Numbers are cores. The exact configurations are in [`minizinc/solvers/`](minizinc/solvers/).

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

`--static-schedule` (together with `--static-runtime`) sets the schedule that runs before the selector's portfolio. The file is a CSV without a header, with one `<solver-id>,<cores>` line per solver. Examples are in [`static-schedules/`](static-schedules/).

## Authors

Parasol was developed as part of a master's thesis at the University of Southern Denmark by [Jonas Bork Jørgensen](https://github.com/jonas-bork) and [Sofus Hesseldahl Laubel](https://github.com/sofusl90), supervised by Jacopo Mauro and in collaboration with Alessio Pellegrino. It is part of the [Portfolio Solver Platform](https://github.com/Portfolio-Solver-Platform).
