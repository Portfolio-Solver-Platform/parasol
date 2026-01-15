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
SOLVER_PARALLEL_CAPABILITIES = {
    CHUFFED_ID: [1],
    CP_SAT_ID: [1, 8],
    COIN_BC_ID: [1],
    HUUB_ID: [1],
    PICAT_ID: [1]
}

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

def _calculate_allocations(weights, total_cores):
    """Calculate proportional core allocations with remainder to best solver."""
    total = weights.sum()
    if total == 0:
        return np.zeros_like(weights, dtype=int)
    normalized = weights / total
    floored = np.floor(normalized * total_cores).astype(int)
    floored[np.argmax(normalized)] += total_cores - floored.sum()
    return [int(x) for x in floored]


def distribuite_cores(preds_log, total_cores):
    remaining_cores = total_cores
    final_allocations = [0] * len(preds_log)
    weights = preds_log.copy()

    while remaining_cores > 0:
        allocations = _calculate_allocations(weights, remaining_cores)
        for i, solver_id in enumerate(SOLVER_ORDER):
            allocation = allocations[i]

            if allocation == 0:
                continue
            
            restrictions = SOLVER_PARALLEL_CAPABILITIES.get(solver_id)
            
            if restrictions is None:
                final_allocations[i] = allocation
                continue
            if solver_id == CP_SAT_ID and allocation >= 5 and remaining_cores >= 8:
                cores = max(8, allocation)
            else:
                cores = 1
            
            final_allocations[i] = cores
            remaining_cores -= cores
            weights[i] = 0
            break
        else: 
            left_over = total_cores - sum(final_allocations) # due to core restrictions we might not have used all cores
            if left_over > 0:
                cp_sat_idx = SOLVER_ORDER.index(CP_SAT_ID)
                if final_allocations[cp_sat_idx] + left_over >= 8:
                    final_allocations[cp_sat_idx] += left_over
                else:
                    gecode_idx = SOLVER_ORDER.index(GECODE_ID)
                    final_allocations[gecode_idx] += left_over
            remaining_cores = 0
                
    return final_allocations


def schedule(features: np.ndarray, total_cores: int) -> list[tuple[str, int]]:
    script_dir = os.path.dirname(os.path.abspath(__file__))
    model_path = os.path.join(script_dir, 'data', 'knn_model.pkl')
    loaded_pipeline = joblib.load(model_path)

    features_2d = features.reshape(1, -1)
    preds_log = loaded_pipeline.predict_proba(features_2d)[0]
    final_allocations = distribuite_cores(preds_log, total_cores)

    return [
        (SOLVER_ORDER[i], cores)
        for i, cores in enumerate(final_allocations)
        if cores > 0
    ]


if __name__ == "__main__":
    main()