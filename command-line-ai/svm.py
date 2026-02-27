#!/usr/bin/env python3.13

import argparse
import joblib
import numpy as np
import os

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
CHUFFED_ID = "org.chuffed.chuffed"
DEXTER_ID = "dexter"
CPLEX_ID = "org.minizinc.mip.cplex"
XPRESS_ID = "org.minizinc.mip.xpress"
IZPLUS_ID = "izplus"


def parse_comma_separated_floats(input_str):
    return np.array([float(x) for x in input_str.split(",")])


def main():
    parser = argparse.ArgumentParser(description="AI Solver Scheduler")
    parser.add_argument("features", type=parse_comma_separated_floats)
    parser.add_argument("-p", required=True, type=int, dest="cores")
    args = parser.parse_args()

    sched = schedule(args.features, args.cores)
    # with open('/app/results/ai_choice.txt', 'a') as file:
    #     file.write(f'{sched}\n\n\n')
        
        
    for solver, cores in sched:
        print(f"{solver},{cores}")


def schedule(features: np.ndarray, total_cores: int) -> list[tuple[str, int]]:
    script_dir = os.path.dirname(os.path.abspath(__file__))
    model_path = os.path.join(script_dir, 'data', 'svm_model.joblib')
    model = joblib.load(model_path)

    features_2d = features.reshape(1, -1)
    predicted_portfolio = model.predict(features_2d)[0]

    if predicted_portfolio == 0:
        return [(CP_SAT_ID, 8)]
    elif predicted_portfolio == 1:
        return [
            (CP_SAT_ID, 1),
            (CPLEX_ID, 1),
            (GECODE_ID, 2),
            (PICAT_ID, 1),
            (HUUB_ID, 1),
            (CHOCO_ID, 1),
            (CHUFFED_ID, 1),
        ]


if __name__ == "__main__":
    main()