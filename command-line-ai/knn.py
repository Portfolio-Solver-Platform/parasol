#!/usr/bin/env python3.13

import argparse
import joblib
import numpy as np
import os
from scipy.special import softmax 

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


SOLVER_ORDER = [CHOCO_ID, CHUFFED_ID, CP_SAT_ID, GECODE_ID, HUUB_ID, PICAT_ID]

def parse_comma_separated_floats(input_str):
    try:
        return np.array([float(x) for x in input_str.split(",")])
    except ValueError:
        raise argparse.ArgumentTypeError(
            f"'{input_str}' contains invalid values. Expected comma-separated floats."
        )

def main():
    parser = argparse.ArgumentParser(description="AI Solver Scheduler")
    parser.add_argument("features", type=parse_comma_separated_floats)
    parser.add_argument("-p", required=True, type=int, dest="cores")
    args = parser.parse_args()

    sched = schedule(args.features, args.cores)
    
    for solver, cores in sched:
        print(f"{solver},{cores}")

def schedule(features: np.ndarray, total_cores: int) -> list[tuple[str, int]]:
    script_dir = os.path.dirname(os.path.abspath(__file__))
    model_path = os.path.join(script_dir, 'data', 'knn_model.pkl')

    loaded_pipeline = joblib.load(model_path)


    features_2d = features.reshape(1, -1)
    preds_log = loaded_pipeline.predict_proba(features_2d)[0] 

    # temperature = 1.0
    # probabilities = softmax(-preds_log / temperature)

    raw_allocations = preds_log * total_cores
    final_allocations = np.floor(raw_allocations).astype(int)
    remainder = total_cores - np.sum(final_allocations)
    best_solver_idx = np.argmax(preds_log)
    final_allocations[best_solver_idx] += remainder

    schedule_list = []
    for i, core_count in enumerate(final_allocations):
        if core_count > 0:
            solver_id = SOLVER_ORDER[i]
            schedule_list.append((solver_id, int(core_count)))

    return schedule_list

if __name__ == "__main__":
    main()