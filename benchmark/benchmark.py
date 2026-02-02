#!/usr/bin/env python3
import argparse
import csv
import glob
import re
import subprocess
import sys
import time
from pathlib import Path

from discover import discover_problems

PROBLEMS = [
    # # --- Original problems ---
    # ("test_unsat/test_unsat.mzn", None),
    # ("sudoku_fixed/sudoku_fixed.mzn", "sudoku_fixed/sudoku_p20.dzn"),
    # ("accap/accap.mzn", "accap/accap_instance6.dzn"),
    # ("rcpsp/rcpsp.mzn", "rcpsp/00.dzn"),
    # ("gbac/gbac.mzn", "gbac/UD2-gbac.dzn"),
    # ("amaze/amaze.mzn", "amaze/2012-03-08.dzn"),
    # ("bacp/bacp-1.mzn", None),
    # ("bacp/bacp-2.mzn", None),
    # ("steelmillslab/steelmillslab.mzn", "steelmillslab/bench_2_0.dzn"),

    # --- Stress tests (specifically designed to stress solvers) ---
    ("search_stress/search_stress.mzn", "search_stress/08_08.dzn"),  # Search stress
    ("slow_convergence/slow_convergence.mzn", "slow_convergence/0300.dzn"),  # Slow bound convergence

    # --- Pure SAT puzzles (different constraint structures) ---
    ("hitori/hitori.mzn", "hitori/h11-1.dzn"),  # Grid-based SAT
    ("nonogram/non.mzn", "nonogram/non_fast_3.dzn"),  # Line-based constraints
    ("fillomino/fillomino.mzn", "fillomino/6x6_0.dzn"),  # Region-based SAT

    # --- Hard combinatorial problems ---
    ("costas-array/CostasArray.mzn", "costas-array/14.dzn"),  # All-different + math
    ("ghoulomb/ghoulomb.mzn", "ghoulomb/3-8-20.dzn"),  # Golomb ruler variant

    # --- Geometric/packing problems ---
    ("rectangle-packing/rect_packing.mzn", "rectangle-packing/rpp09_false.dzn"),  # 2D packing (UNSAT)
    ("rectangle-packing/rect_packing.mzn", "rectangle-packing/rpp12_true.dzn"),  # 2D packing (SAT)
    ("pentominoes/pentominoes-int.mzn", "pentominoes/03.dzn"),  # Polyomino placement

    # --- Routing/TSP variants ---
    ("atsp/atsp.mzn", "atsp/instance5_0p15.dzn"),  # Asymmetric TSP
    ("cvrp/cvrp.mzn", "cvrp/simple2.dzn"),  # Capacitated VRP (small)
    ("tsptw/tsptw.mzn", "tsptw/n20w160.001.dzn"),  # TSP with time windows

    # --- Job shop scheduling variants ---
    ("fjsp/fjsp.mzn", "fjsp/easy01.dzn"),  # Flexible job shop (easy)
    ("fjsp/fjsp.mzn", "fjsp/med04.dzn"),  # Flexible job shop (medium)
    ("openshop/openshop.mzn", "openshop/gp10-4.dzn"),  # Open shop scheduling

    # --- Global constraint heavy ---
    ("multi-knapsack/mknapsack.mzn", "multi-knapsack/mknap1-5.dzn"),  # Multi-dim knapsack
    ("black-hole/black-hole.mzn", "black-hole/4.dzn"),  # Card game (global constraints)

    # --- Classic puzzles ---
    ("mqueens/mqueens2.mzn", "mqueens/n12.dzn"),  # N-queens variant
]


SOLVERS = ["fzn-cp-sat", "fzn-gecode", "fzn-chuffed", "fzn-huub", "cplex", "choco", "fzn-choco.sh", "picat", "fzn-picat", "java", "minizinc"]


def kill_solvers():
    for solver in SOLVERS:
        subprocess.run(["pkill", "-9", solver], capture_output=True)


def resolve_schedules(args: list[str]) -> list[Path]:
    files = []
    for arg in args:
        path = Path(arg)
        if path.is_file():
            files.append(path)
        elif path.is_dir():
            files.extend(sorted(path.glob("*.csv")))
        else:
            files.extend(Path(m) for m in sorted(glob.glob(arg)))
    return list(dict.fromkeys(f.resolve() for f in files))


def run_parasol(model: Path, data: Path | None, schedule: Path, cores: int,
                timeout: int | None, solver: str) -> tuple[float, str | None, str, str]:
    cmd = []
    if timeout:
        cmd.extend(["timeout", str(timeout)])
    cmd.append("minizinc")
    if solver != "":
        cmd.extend(["--solver", solver])
    cmd.append(str(model))
    if data:
        cmd.append(str(data))
    cmd.extend(["--static-schedule", str(schedule), "-p", str(cores), "--ai", "none", "--verbosity", "quiet", "--solver-config-mode", "cache"])

    start = time.perf_counter()
    result = subprocess.run(cmd, capture_output=True, text=True)
    elapsed_ms = (time.perf_counter() - start) * 1000

    stdout = result.stdout

    objectives = re.findall(r'_objective\s*=\s*(-?\d+);', stdout)
    objective = objectives[-1] if objectives else None

    if "==========" in stdout:
        status = "Optimal"
    elif "=====UNSATISFIABLE=====" in stdout:
        status = "Unsat"
    elif "----------" in stdout and not objective:
        status = "Optimal"  # SAT problem with solution found
    else:
        status = "Unknown"

    return elapsed_ms, objective, status, stdout


def run_benchmark(problems: list[tuple[Path, Path | None]], schedules: list[Path], cores: int,
                  timeout: int | None, runs: int, solver: str, output: Path):
    output.parent.mkdir(parents=True, exist_ok=True)

    with open(output, "w", newline="") as f:
        writer = csv.writer(f)
        writer.writerow(["schedule", "problem", "name", "model", "time_ms", "objective", "optimal"])

        for schedule in schedules:
            print(f"\nSchedule: {schedule.name}")

            for model, data in problems:
                problem = model.parent.name
                name = data.stem if data else model.stem
                model_name = model.stem

                print(f"  {name}: ", end="", flush=True)

                for run in range(runs):
                    kill_solvers()
                    time_ms, objective, status, stdout = run_parasol(model, data, schedule, cores, timeout, solver)
                    writer.writerow([schedule.stem, problem, name, model_name, f"{time_ms:.0f}", objective or "", status])
                    f.flush()
                    short = "US" if status == "Unsat" else status[0]
                    print(f"{time_ms/1000:.1f}s({short}) ", end="", flush=True)
                    # print(f"\n--- stdout ---\n{stdout}--- end ---")

                print()

    print(f"\nResults written to: {output}")


def main():
    parser = argparse.ArgumentParser(description="Benchmark Parasol static schedules")
    parser.add_argument("schedules", nargs="+", help="Schedule CSV files or directories")
    parser.add_argument("-p", "--cores", type=int, default=8)
    parser.add_argument("-t", "--timeout", type=int, default=None)
    parser.add_argument("-r", "--runs", type=int, default=3)
    parser.add_argument("-o", "--output", type=Path, default=Path("results/benchmark_results.csv"))
    parser.add_argument("--solver", default="parasol")
    parser.add_argument("--problems-path", type=Path, default=Path("/problems"))
    parser.add_argument("--discover", action="store_true", help="Discover problems from --problems-path instead of using hardcoded list")
    args = parser.parse_args()

    schedules = resolve_schedules(args.schedules)
    if not schedules:
        print("No schedule files found", file=sys.stderr)
        sys.exit(1)

    if args.discover:
        problems = discover_problems(args.problems_path)
    else:
        problems = [(args.problems_path / m, args.problems_path / d if d else None) for m, d in PROBLEMS]

    print(f"Schedules: {len(schedules)}, Problems: {len(problems)}, Runs: {args.runs}")
    run_benchmark(problems, schedules, args.cores, args.timeout, args.runs, args.solver, args.output)


if __name__ == "__main__":
    main()
