#!/bin/bash

# Run all problem instances with 10-second timeout
# Usage: ./run_all_problems.sh [additional args...]

PROBLEMS_DIR="../../psp/problems"
TIMEOUT_SECONDS=10
SOLVER_PATH="./target/release/portfolio-solver-framework"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Build the solver first
echo "Building solver in release mode..."
cargo build --release
if [ $? -ne 0 ]; then
    echo -e "${RED}Build failed!${NC}"
    exit 1
fi
echo

# Counters
total_instances=0
solved_instances=0
timeout_instances=0
failed_instances=0

# Iterate through all problem directories
for problem_dir in "$PROBLEMS_DIR"/*/; do
    problem_name=$(basename "$problem_dir")

    # Skip if it's not a directory or starts with . or _
    [[ ! -d "$problem_dir" ]] && continue
    [[ "$problem_name" =~ ^[._] ]] && continue

    # Find .mzn files (model files)
    mzn_files=("$problem_dir"*.mzn)

    # Skip if no .mzn files found
    [[ ! -e "${mzn_files[0]}" ]] && continue

    # Find data files (.dzn and .json)
    data_files=()
    while IFS= read -r -d $'\0' file; do
        data_files+=("$file")
    done < <(find "$problem_dir" -maxdepth 1 \( -name "*.dzn" -o -name "*.json" \) -print0 2>/dev/null)

    if [[ ${#data_files[@]} -eq 0 ]]; then
        # No data files found - data is embedded in each .mzn file
        echo -e "${YELLOW}========================================${NC}"
        echo -e "${YELLOW}Problem: $problem_name${NC}"
        echo -e "${YELLOW}Instances: ${#mzn_files[@]} (embedded data)${NC}"
        echo -e "${YELLOW}========================================${NC}"

        # Run each .mzn file without a data file
        for model_file in "${mzn_files[@]}"; do
            instance=$(basename "$model_file")
            total_instances=$((total_instances + 1))

            echo -n "  [$total_instances] $instance ... "

            # Run with timeout (no data file argument)
            timeout --signal=SIGTERM ${TIMEOUT_SECONDS}s "$SOLVER_PATH" "$model_file" -p 10 --debug-verbosity quiet "$@" > /dev/null 2>&1
            exit_code=$?

            if [ $exit_code -eq 124 ]; then
                # Timeout
                echo -e "${YELLOW}TIMEOUT${NC}"
                timeout_instances=$((timeout_instances + 1))
            elif [ $exit_code -eq 0 ]; then
                # Success
                echo -e "${GREEN}SOLVED${NC}"
                solved_instances=$((solved_instances + 1))
            else
                # Error
                echo -e "${RED}FAILED (exit $exit_code)${NC}"
                failed_instances=$((failed_instances + 1))
            fi
        done
    else
        # Data files found - use first .mzn file as model
        model_file="${mzn_files[0]}"

        echo -e "${YELLOW}========================================${NC}"
        echo -e "${YELLOW}Problem: $problem_name${NC}"
        echo -e "${YELLOW}Model: $(basename "$model_file")${NC}"
        echo -e "${YELLOW}Instances: ${#data_files[@]}${NC}"
        echo -e "${YELLOW}========================================${NC}"

        # Run each instance with data file
        for data_file in "${data_files[@]}"; do
            instance=$(basename "$data_file")
            total_instances=$((total_instances + 1))

            echo -n "  [$total_instances] $instance ... "

            # Run with timeout
            timeout --signal=SIGTERM ${TIMEOUT_SECONDS}s "$SOLVER_PATH" "$model_file" "$data_file" -p 10 --debug-verbosity quiet "$@" > /dev/null 2>&1
            exit_code=$?

            if [ $exit_code -eq 124 ]; then
                # Timeout
                echo -e "${YELLOW}TIMEOUT${NC}"
                timeout_instances=$((timeout_instances + 1))
            elif [ $exit_code -eq 0 ]; then
                # Success
                echo -e "${GREEN}SOLVED${NC}"
                solved_instances=$((solved_instances + 1))
            else
                # Error
                echo -e "${RED}FAILED (exit $exit_code)${NC}"
                failed_instances=$((failed_instances + 1))
            fi
        done
    fi
    echo
done

# Summary
echo -e "${YELLOW}========================================${NC}"
echo -e "${YELLOW}SUMMARY${NC}"
echo -e "${YELLOW}========================================${NC}"
echo "Total instances:   $total_instances"
echo -e "Solved:            ${GREEN}$solved_instances${NC}"
echo -e "Timeout (>10s):    ${YELLOW}$timeout_instances${NC}"
echo -e "Failed:            ${RED}$failed_instances${NC}"
echo -e "${YELLOW}========================================${NC}"
