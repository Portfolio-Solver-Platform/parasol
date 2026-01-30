#!/usr/bin/env python3
import argparse
import csv
import glob
import re
import subprocess
import sys
import time
from pathlib import Path

PROBLEMS = [
    ("test_unsat/test_unsat.mzn", None),                                                                                                                                                                             
    ("sudoku_fixed/sudoku_fixed.mzn", "sudoku_fixed/sudoku_p20.dzn"),
    ("accap/accap.mzn", "accap/accap_instance6.dzn"),
    ("rcpsp/rcpsp.mzn", "rcpsp/00.dzn"),
    ("gbac/gbac.mzn", "gbac/UD2-gbac.dzn"),
    ("amaze/amaze.mzn", "amaze/2012-03-08.dzn"),
    ("bacp/bacp-1.mzn", None),
    ("bacp/bacp-2.mzn", None),
    ("steelmillslab/steelmillslab.mzn", "steelmillslab/bench_2_0.dzn"),
]


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
                timeout: int | None, solver: str) -> tuple[float, str | None, str]:
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

    objectives = re.findall(r'_objective\s*=\s*(-?\d+);', result.stdout)
    objective = objectives[-1] if objectives else None

    if "==========" in result.stdout:
        status = "Optimal"
    elif "=====UNSATISFIABLE=====" in result.stdout:
        status = "Unsat"
    elif "----------" in result.stdout and not objective:
        status = "Optimal"  # SAT problem with solution found
    else:
        status = "Unknown"

    return elapsed_ms, objective, status


def run_benchmark(problems_base: Path, schedules: list[Path], cores: int,
                  timeout: int | None, runs: int, solver: str, output: Path):
    output.parent.mkdir(parents=True, exist_ok=True)

    with open(output, "w", newline="") as f:
        writer = csv.writer(f)
        writer.writerow(["schedule", "name", "model", "time_ms", "objective", "status"])

        for schedule in schedules:
            print(f"\nSchedule: {schedule.name}")

            for model_rel, data_rel in PROBLEMS:
                model = problems_base / model_rel
                data = problems_base / data_rel if data_rel else None
                name = data.stem if data else Path(model_rel).stem
                model_name = Path(model_rel).stem

                print(f"  {name}: ", end="", flush=True)

                for run in range(runs):
                    time_ms, objective, status = run_parasol(model, data, schedule, cores, timeout, solver)
                    writer.writerow([schedule.stem, name, model_name, f"{time_ms:.0f}", objective or "", status])
                    f.flush()
                    short = "US" if status == "Unsat" else status[0]
                    print(f"{time_ms/1000:.1f}s({short}) ", end="", flush=True)

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
    parser.add_argument("--problems-base", type=Path, default=Path("/problems"))
    args = parser.parse_args()

    schedules = resolve_schedules(args.schedules)
    if not schedules:
        print("No schedule files found", file=sys.stderr)
        sys.exit(1)

    print(f"Schedules: {len(schedules)}, Problems: {len(PROBLEMS)}, Runs: {args.runs}")
    run_benchmark(args.problems_base, schedules, args.cores, args.timeout, args.runs, args.solver, args.output)


if __name__ == "__main__":
    main()
