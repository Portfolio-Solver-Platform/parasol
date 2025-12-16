#!/bin/bash

# Run all fast-food problem instances
# Usage: ./run_fastfood.sh [additional args...]

PROBLEM_DIR="../../psp/problems/fast-food"
MODEL_FILE="$PROBLEM_DIR/fastfood.mzn"

# Get all .dzn files (data files)
DATA_FILES=("$PROBLEM_DIR"/ff*.dzn)

echo "Running ${#DATA_FILES[@]} fast-food problem instances..."
echo

for data_file in "${DATA_FILES[@]}"; do
    instance=$(basename "$data_file" .dzn)
    echo "========================================="
    echo "Running instance: $instance"
    echo "========================================="

    cargo run --release -- "$MODEL_FILE" "$data_file" -p 10 --debug-verbosity quiet "$@"

    exit_code=$?
    if [ $exit_code -ne 0 ]; then
        echo "Warning: Instance $instance exited with code $exit_code"
    fi
    echo
done

echo "All instances completed!"
