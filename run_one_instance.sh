#!/bin/bash

PROBLEMS_DIR="../../psp/problems"
TIMEOUT_SECONDS=21
SOLVER_PATH="./target/release/portfolio-solver-framework"

if command -v timeout &> /dev/null; then
    TIMEOUT_CMD="timeout"
elif command -v gtimeout &> /dev/null; then
    TIMEOUT_CMD="gtimeout"
else
    echo "Error: Neither 'timeout' nor 'gtimeout' found."
    echo "On macOS, install with: brew install coreutils"
    exit 1
fi

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
CYAN='\033[0;36m'
NC='\033[0m' 

trap 'echo -e "\n${RED}Interrupted by user${NC}"; exit 130' INT

echo "Building solver in release mode..."
cargo build --release
if [ $? -ne 0 ]; then
    echo -e "${RED}Build failed!${NC}"
    exit 1
fi
echo

for problem_dir in "$PROBLEMS_DIR"/*/; do
    problem_name=$(basename "$problem_dir")

    [[ ! -d "$problem_dir" ]] && continue
    [[ "$problem_name" =~ ^[._] ]] && continue

    mzn_files=("$problem_dir"*.mzn)

    [[ ! -e "${mzn_files[0]}" ]] && continue

    data_files=()
    while IFS= read -r -d $'\0' file; do
        data_files+=("$file")
    done < <(find "$problem_dir" -maxdepth 1 \( -name "*.dzn" -o -name "*.json" \) -print0 2>/dev/null)

    echo -e "${YELLOW}========================================${NC}"
    echo -e "${YELLOW}Problem: $problem_name${NC}"
    echo -e "${YELLOW}========================================${NC}"

    if [[ ${#data_files[@]} -eq 0 ]]; then
        model_file="${mzn_files[0]}"
        instance=$(basename "$model_file")

        echo -e "${CYAN}Running: $instance${NC}"
        echo -e "${CYAN}Command: $SOLVER_PATH $model_file -p 10 --debug-verbosity error${NC}"
        echo

        $TIMEOUT_CMD --signal=SIGTERM ${TIMEOUT_SECONDS}s "$SOLVER_PATH" "$model_file" -p 10 --debug-verbosity quiet "$@"  > /dev/null 2>&1
        exit_code=$?

        echo
        if [ $exit_code -eq 124 ]; then
            echo -e "${YELLOW}Result: TIMEOUT${NC}"
        elif [ $exit_code -eq 0 ]; then
            echo -e "${GREEN}Result: SOLVED (exit 0)${NC}"
        else
            echo -e "${RED}Result: FAILED (exit $exit_code)${NC}"
        fi
    else
        model_file="${mzn_files[0]}"
        data_file="${data_files[0]}"
        instance=$(basename "$data_file")

        echo -e "${CYAN}Model: $(basename "$model_file")${NC}"
        echo -e "${CYAN}Instance: $instance${NC}"
        echo -e "${CYAN}Command: $SOLVER_PATH $model_file $data_file -p 10 --debug-verbosity error${NC}"
        echo

        $TIMEOUT_CMD --signal=SIGTERM ${TIMEOUT_SECONDS}s "$SOLVER_PATH" "$model_file" "$data_file" -p 10 --debug-verbosity quiet "$@"  > /dev/null 2>&1
        exit_code=$?

        echo
        if [ $exit_code -eq 124 ]; then
            echo -e "${YELLOW}Result: TIMEOUT${NC}"
        elif [ $exit_code -eq 0 ]; then
            echo -e "${GREEN}Result: SOLVED (exit 0)${NC}"
        else
            echo -e "${RED}Result: FAILED (exit $exit_code)${NC}"
        fi
    fi
    echo
done
