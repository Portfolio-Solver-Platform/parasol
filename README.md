# Parasol

Parasol is a multi-threaded, low-overhead universal portfolio executor for MiniZinc, written in Rust.

Portfolio solvers combine multiple individual solvers to solve problems faster. A _universal portfolio executor_, such as SUNNY-CP, supports many underlying solvers and leaves the choice of portfolio to a separate _portfolio selector_. Parasol handles the execution: it compiles the model for each solver, runs the solvers and coordinates them. The portfolio selector therefore only has to decide which solvers to run, and with how many cores. Parasol can be installed as a normal MiniZinc solver.

Parasol was developed as part of our master's thesis. To show what it can do, we built a portfolio selector on top of it and submitted the complete system to the [MiniZinc Challenge 2026](https://www.minizinc.org/challenge/2026/results/), where it got second place (silver) in the Open category, behind Google's OR-Tools CP-SAT. The submission is described in [MiniZinc Challenge 2026](#minizinc-challenge-2026).

## How it works

No single solver is best on every problem, so a portfolio solver picks which solvers to run on each instance. This can be a single solver on all cores, or several solvers in parallel.

The `sso` command runs Parasol's single-selection orchestrator, which goes through three steps:
1. Parasol starts an optional _static schedule_ of solvers right away. Meanwhile, it compiles the model on one core and extracts the instance features with [fzn2feat](https://github.com/CP-Unibo/mzn2feat).
2. The portfolio selector receives the features and the number of cores, and returns a portfolio, which is a list of solvers with a number of cores each. If feature extraction takes too long, Parasol runs an optional timeout schedule instead.
3. Parasol switches from the static schedule to the selected portfolio. Solvers that appear in both with the same number of cores keep running, so their compilation and search progress is not lost. The remaining solvers are suspended, and the new ones are started.

Parasol also supports _bound sharing_: at a fixed interval, solvers whose best objective is worse than the best one found so far are restarted with that objective as a bound. In our experiments, none of the intervals we tested improved the Borda score of our final portfolios, so bound sharing is disabled in our challenge configurations.

Beyond this, Parasol:
- finds the solvers installed for MiniZinc, or loads them from a cache,
- compiles the model for each solver in parallel, and can compile ahead for solvers the selector is likely to pick (`--compilation-priority`),
- controls each solver's whole process tree, so suspending or killing a solver also reaches its child processes,
- kills solvers when memory runs low (`--enforce-memory`), can pin solvers to dedicated cores (`--pin-solvers`), and falls back to CP-SAT if the orchestrator fails (`--backup-solver`).

## Using your own portfolio selector

The portfolio selector is chosen with `--ai`:

| `--ai` | Portfolio selector |
|---|---|
| `none` | No selector, only the static schedule is run |
| `simple` | One core each for the first solvers Parasol knows (default) |
| `command-line` | An external program, described below |
| `svc-k1`, `svc-ek1` | Our selectors from the MiniZinc Challenge, built into the binary |

With `command-line`, Parasol runs the given program with the number of cores and the features as a comma-separated list:

```bash
parasol sso model.mzn data.dzn --ai command-line --ai-config command=./my-selector
# Parasol then runs: ./my-selector -p <cores> <f1>,<f2>,...,<fn>
```

The program must print one `<solver-id>,<cores>` line per solver:

```
cp-sat,4
org.gecode.gecode,2
org.minizinc.mip.gurobi,2
```

A minimal example is available in [`command-line-ai/example.py`](command-line-ai/example.py). A selector can also be built into Parasol by implementing the `Ai` trait in [`src/ai.rs`](src/ai.rs).

## MiniZinc Challenge 2026

Our portfolio selector chooses, on a per-instance basis, between two static portfolios: CP-SAT on all 8 cores, and an alternative portfolio of other solvers. We chose a small set of static portfolios rather than building a new portfolio for each instance, since a smaller decision space is easier to learn from limited training data, and CP-SAT on 8 cores is already the best choice on a large fraction of the instances. The selector is an ensemble of 30 support vector classifiers (SVCs) with RBF kernels over 95 instance features, which we call BagSVC-MW. Everything else is handled by Parasol. The training code and experiments are available in [ai-tools](https://github.com/Portfolio-Solver-Platform/ai-tools).

Solvers made by the MiniZinc organisers are ineligible for prizes, so we submitted two versions:
- Parasol-e only uses prize-eligible solvers, and is our prize submission.
- Parasol-i also uses Chuffed and Gecode, which are made by the organisers. It cannot win prizes, but shows what is achievable when every solver is available.

The alternative portfolio of each version is shown below, where the numbers are cores. The configurations are available in [`minizinc/solvers/`](minizinc/solvers/).

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

The static schedule is set with `--static-schedule` together with `--static-runtime`. The file is a CSV without a header, with one `<solver-id>,<cores>` line per solver. Examples are available in [`static-schedules/`](static-schedules/).

## Authors

Parasol was developed as part of our master's thesis at the University of Southern Denmark by [Jonas Bork Jørgensen](https://github.com/jonas-bork) and [Sofus Hesseldahl Laubel](https://github.com/sofusl90), supervised by Jacopo Mauro and in collaboration with Alessio Pellegrino. It is part of the [Portfolio Solver Platform](https://github.com/Portfolio-Solver-Platform).
