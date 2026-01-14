#!/usr/bin/env python3

import argparse

HUUB_ID = "solutions.huub"
CP_SAT_ID = "cp-sat"
COIN_BC_ID = "org.minizinc.mip.coin-bc"
CHOCO_ID = "org.choco.choco"
GECODE_ID = "org.gecode.gecode"
GUROBI_ID = "org.minizinc.mip.gurobi"
HIGHS_ID = "org.minizinc.mip.highs"
PICAT_ID = "org.picat-lang.picat"
PUMPKIN_ID = "nl.tudelft.algorithmics.pumpkin"
SCIP_ID = "org.minizinc.mip.scip"
YUCK_ID = "yuck"



def parse_comma_separated_floats(input_str):
    try:
        return [float(x) for x in input_str.split(",")]
    except ValueError:
        raise argparse.ArgumentTypeError(
            f"'{input_str}' contains invalid values. Expected comma-separated floats."
        )


def main():
    parser = argparse.ArgumentParser(description="Example commandline AI")

    parser.add_argument(
        "features",
        type=parse_comma_separated_floats,
        help="the features (comma-separated, e.g. 1.0,2.5,3.3)",
    )

    parser.add_argument(
        "-p",
        required=True,
        type=int,
        metavar="CORES",
        dest="cores",
        help="how many cores the schedule should allocate for",
    )

    args = parser.parse_args()

    sched = schedule(args.features, args.cores)
    for solver, cores in sched:
        print(f"{solver},{cores}")


def schedule(features: list[float], cores: int) -> list[tuple[str, int]]:
    return [(GECODE_ID, cores // 2), (COIN_BC_ID, cores // 2)]


if __name__ == "__main__":
    main()
