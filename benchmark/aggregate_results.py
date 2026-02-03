#!/usr/bin/env python3
"""Aggregate benchmark results by calculating averages across repetitions."""

import argparse
import csv
import sys
from collections import defaultdict
from pathlib import Path


def main():
    parser = argparse.ArgumentParser(
        description="Calculate average benchmark results across repetitions"
    )
    parser.add_argument("input_file", type=Path, help="Input CSV file with benchmark results")
    parser.add_argument(
        "-o", "--output", type=Path, help="Output CSV file (default: stdout)"
    )
    args = parser.parse_args()

    if not args.input_file.exists():
        print(f"Error: File not found: {args.input_file}", file=sys.stderr)
        sys.exit(1)

    # Group results by (schedule, problem, name, model)
    groups = defaultdict(list)

    with open(args.input_file, newline="") as f:
        reader = csv.DictReader(f)
        for row in reader:
            key = (row["schedule"], row["problem"], row["name"], row["model"])
            groups[key].append(row)

    # Calculate aggregates
    results = []
    for key, rows in groups.items():
        schedule, problem, name, model = key

        # Parse time_ms values
        times = [int(r["time_ms"]) for r in rows if r["time_ms"]]

        # Parse objective values (may be empty)
        objectives = []
        for r in rows:
            if r["objective"]:
                try:
                    objectives.append(float(r["objective"]))
                except ValueError:
                    pass

        # Get optimal (should be same across repetitions)
        optimal = rows[0]["optimal"] if rows else "Unknown"

        result = {
            "schedule": schedule,
            "problem": problem,
            "name": name,
            "model": model,
            "repetitions": len(rows),
            "avg_time_ms": round(sum(times) / len(times), 2) if times else "",
            "min_time_ms": min(times) if times else "",
            "max_time_ms": max(times) if times else "",
            "avg_objective": round(sum(objectives) / len(objectives), 2) if objectives else "",
            "optimal": optimal,
        }
        results.append(result)

    # Sort by schedule, problem, name
    results.sort(key=lambda r: (r["schedule"], r["problem"], r["name"]))

    # Output
    fieldnames = [
        "schedule", "problem", "name", "model", "repetitions",
        "avg_time_ms", "min_time_ms", "max_time_ms", "avg_objective", "optimal"
    ]

    if args.output:
        with open(args.output, "w", newline="") as f:
            writer = csv.DictWriter(f, fieldnames=fieldnames)
            writer.writeheader()
            writer.writerows(results)
        print(f"Wrote aggregated results to {args.output}")
    else:
        writer = csv.DictWriter(sys.stdout, fieldnames=fieldnames)
        writer.writeheader()
        writer.writerows(results)


if __name__ == "__main__":
    main()
