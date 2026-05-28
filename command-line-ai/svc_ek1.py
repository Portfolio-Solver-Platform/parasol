#!/usr/bin/env python3.13
"""Parasol scheduler for the cpsat8 vs ek1-8c-8s-v2 portfolio decision.

Loads data/svc_ek1_model.joblib and prints the chosen schedule.

Class 0 ("cpsat wins"): cp-sat on all 8 cores.
Class 1 ("ek1 wins"): the actual ek1-8c-8s-v2 portfolio (matches
~/speciale/ai/ucloud/schedules-eligible/ek1-8c-8s-v2.csv).
"""

import argparse
import os

import joblib
import numpy as np

from svc_common import BagSVCPredictor, SignedLog1p  # noqa: F401 - needed at unpickle time

# Solver IDs (ek1-8c-8s-v2 portfolio composition)
CP_SAT_ID = "cp-sat"
DEXTER_ID = "dexter"
PUMPKIN_ID = "nl.tudelft.algorithmics.pumpkin"
CHOCO_ID = "org.choco.choco"
GUROBI_ID = "org.minizinc.mip.gurobi"
PICAT_ID = "org.picat-lang.picat"
YUCK_ID = "yuck"


def parse_comma_separated_floats(input_str):
    return np.array([float(x) for x in input_str.split(",")])


def schedule(features, total_cores):
    script_dir = os.path.dirname(os.path.abspath(__file__))
    model_path = os.path.join(script_dir, "data", "svc_ek1_model.joblib")
    model = joblib.load(model_path)

    features_2d = features.reshape(1, -1)
    predicted_portfolio = int(model.predict(features_2d)[0])

    if predicted_portfolio == 0:
        return [(CP_SAT_ID, 8)]
    elif predicted_portfolio == 1:
        # ek1-8c-8s-v2 portfolio (8 cores total = 1+1+1+1+2+1+1).
        return [
            (CP_SAT_ID, 1),
            (DEXTER_ID, 1),
            (PUMPKIN_ID, 1),
            (CHOCO_ID, 1),
            (GUROBI_ID, 2),
            (PICAT_ID, 1),
            (YUCK_ID, 1),
        ]


def main():
    parser = argparse.ArgumentParser(description="Bagged SVC scheduler (cpsat8 vs ek1)")
    parser.add_argument("features", type=parse_comma_separated_floats)
    parser.add_argument("-p", required=True, type=int, dest="cores")
    args = parser.parse_args()
    sched = schedule(args.features, args.cores)
    for solver, cores in sched:
        print(f"{solver},{cores}")


if __name__ == "__main__":
    main()
