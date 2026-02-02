#!/bin/bash

# PROBLEMS_DIR_HOST="/home/sofus/speciale/psp/problems"
# PROBLEMS_DIR_CONTAINER="/problems"
# TIMEOUT_SECONDS=15
# # SOLVER_PATH="./../target/release/portfolio-solver-framework"
# SOLVER_PATH="docker run -v ${PROBLEMS_DIR_HOST}:${PROBLEMS_DIR_CONTAINER} framework minizinc"
# if command -v timeout &> /dev/null; then
#     TIMEOUT_CMD="timeout"
# elif command -v gtimeout &> /dev/null; then
#     TIMEOUT_CMD="gtimeout"
# else
#     echo "Error: Neither 'timeout' nor 'gtimeout' found."
#     echo "On macOS, install with: brew install coreutils"
#     exit 1
# fi

# RED='\033[0;31m'
# GREEN='\033[0;32m'
# YELLOW='\033[1;33m'
# NC='\033[0m' 

# trap 'echo -e "\n${RED}Interrupted by user${NC}"; exit 130' INT

# echo "Building solver in release mode..."
# # cargo build --release
# docker build -t framework:latest ..
# if [ $? -ne 0 ]; then
#     echo -e "${RED}Build failed!${NC}"
#     exit 1
# fi
# echo

# total_instances=0
# solved_instances=0
# timeout_instances=0
# failed_instances=0

# for problem_dir in "$PROBLEMS_DIR_HOST"/*/; do
#     problem_name=$(basename "$problem_dir")

#     [[ ! -d "$problem_dir" ]] && continue
#     [[ "$problem_name" =~ ^[._] ]] && continue

#     mzn_files=("$problem_dir"*.mzn)

#     [[ ! -e "${mzn_files[0]}" ]] && continue

#     data_files=()
#     while IFS= read -r -d $'\0' file; do
#         data_files+=("$file")
#     done < <(find "$problem_dir" -maxdepth 1 \( -name "*.dzn" -o -name "*.json" \) -print0 2>/dev/null)

#     if [[ ${#data_files[@]} -eq 0 ]]; then
#         echo -e "${YELLOW}========================================${NC}"
#         echo -e "${YELLOW}Problem: $problem_name${NC}"
#         echo -e "${YELLOW}Instances: ${#mzn_files[@]} (embedded data)${NC}"
#         echo -e "${YELLOW}========================================${NC}"

#         for model_file in "${mzn_files[@]}"; do
#             instance=$(basename "$model_file")
#             total_instances=$((total_instances + 1))

#             echo -n "  [$total_instances] $instance ... "

#             container_model="${model_file/$PROBLEMS_DIR_HOST/$PROBLEMS_DIR_CONTAINER}"
#             $TIMEOUT_CMD --signal=SIGTERM ${TIMEOUT_SECONDS}s $SOLVER_PATH "$container_model" -p 6 --verbosity quiet "$@"  > /dev/null 2>&1 
#             exit_code=$?

#             if [ $exit_code -eq 124 ]; then
#                 echo -e "${YELLOW}TIMEOUT${NC}"
#                 timeout_instances=$((timeout_instances + 1))
#             elif [ $exit_code -eq 0 ]; then
                
#                 echo -e "${GREEN}SOLVED${NC}"
#                 solved_instances=$((solved_instances + 1))
#             else
#                 echo -e "${RED}FAILED (exit $exit_code)${NC}"
#                 failed_instances=$((failed_instances + 1))
#             fi
#         done
#     else
#         model_file="${mzn_files[0]}"

#         echo -e "${YELLOW}========================================${NC}"
#         echo -e "${YELLOW}Problem: $problem_name${NC}"
#         echo -e "${YELLOW}Model: $(basename "$model_file")${NC}"
#         echo -e "${YELLOW}Instances: ${#data_files[@]}${NC}"
#         echo -e "${YELLOW}========================================${NC}"

#         for data_file in "${data_files[@]}"; do
#             instance=$(basename "$data_file")
#             total_instances=$((total_instances + 1))

#             echo -n "  [$total_instances] $instance ... "

#             container_model="${model_file/$PROBLEMS_DIR_HOST/$PROBLEMS_DIR_CONTAINER}"
#             container_data="${data_file/$PROBLEMS_DIR_HOST/$PROBLEMS_DIR_CONTAINER}" 
#             $TIMEOUT_CMD --signal=SIGTERM ${TIMEOUT_SECONDS}s $SOLVER_PATH "$container_model" "$container_data" -p 6 --verbosity quiet "$@"  > /dev/null 2>&1
#             exit_code=$?

#             if [ $exit_code -eq 124 ]; then
#                 echo -e "${YELLOW}TIMEOUT${NC}"
#                 timeout_instances=$((timeout_instances + 1))
#             elif [ $exit_code -eq 0 ]; then
#                 echo -e "${GREEN}SOLVED${NC}"
#                 solved_instances=$((solved_instances + 1))
#             else
#                 echo -e "${RED}FAILED (exit $exit_code)${NC}"
#                 failed_instances=$((failed_instances + 1))
#             fi
#         done
#     fi
#     echo
# done

# echo -e "${YELLOW}========================================${NC}"
# echo -e "${YELLOW}SUMMARY${NC}"
# echo -e "${YELLOW}========================================${NC}"
# echo "Total instances:   $total_instances"
# echo -e "Solved:            ${GREEN}$solved_instances${NC}"
# echo -e "Timeout (>10s):    ${YELLOW}$timeout_instances${NC}"
# echo -e "Failed:            ${RED}$failed_instances${NC}"
# echo -e "${YELLOW}========================================${NC}"







# #!/bin/bash

PROBLEMS_DIR="../../../psp/problems"
TIMEOUT_SECONDS=15
SOLVER_PATH="./../target/release/parasol"

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
NC='\033[0m' 

trap 'echo -e "\n${RED}Interrupted by user${NC}"; exit 130' INT

echo "Building solver in release mode..."
cargo build --release
if [ $? -ne 0 ]; then
    echo -e "${RED}Build failed!${NC}"
    exit 1
fi
echo

total_instances=0
solved_instances=0
timeout_instances=0
failed_instances=0

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

    if [[ ${#data_files[@]} -eq 0 ]]; then
        echo -e "${YELLOW}========================================${NC}"
        echo -e "${YELLOW}Problem: $problem_name${NC}"
        echo -e "${YELLOW}Instances: ${#mzn_files[@]} (embedded data)${NC}"
        echo -e "${YELLOW}========================================${NC}"

        for model_file in "${mzn_files[@]}"; do
            instance=$(basename "$model_file")
            total_instances=$((total_instances + 1))

            echo -n "  [$total_instances] $instance ... "

            $TIMEOUT_CMD --signal=SIGTERM ${TIMEOUT_SECONDS}s "$SOLVER_PATH" run "$model_file" -p 6 -v quiet "$@"  > /dev/null 2>&1
            exit_code=$?

            if [ $exit_code -eq 124 ]; then
                echo -e "${YELLOW}TIMEOUT${NC}"
                timeout_instances=$((timeout_instances + 1))
            elif [ $exit_code -eq 0 ]; then
                
                echo -e "${GREEN}SOLVED${NC}"
                solved_instances=$((solved_instances + 1))
            else
                echo -e "${RED}FAILED (exit $exit_code)${NC}"
                failed_instances=$((failed_instances + 1))
            fi
        done
    else
        model_file="${mzn_files[0]}"

        echo -e "${YELLOW}========================================${NC}"
        echo -e "${YELLOW}Problem: $problem_name${NC}"
        echo -e "${YELLOW}Model: $(basename "$model_file")${NC}"
        echo -e "${YELLOW}Instances: ${#data_files[@]}${NC}"
        echo -e "${YELLOW}========================================${NC}"

        for data_file in "${data_files[@]}"; do
            instance=$(basename "$data_file")
            total_instances=$((total_instances + 1))

            echo -n "  [$total_instances] $instance ... "

            $TIMEOUT_CMD --signal=SIGTERM ${TIMEOUT_SECONDS}s "$SOLVER_PATH" run "$model_file" "$data_file" -p 6 -v quiet "$@"  > /dev/null 2>&1
            exit_code=$?

            if [ $exit_code -eq 124 ]; then
                echo -e "${YELLOW}TIMEOUT${NC}"
                timeout_instances=$((timeout_instances + 1))
            elif [ $exit_code -eq 0 ]; then
                echo -e "${GREEN}SOLVED${NC}"
                solved_instances=$((solved_instances + 1))
            else
                echo -e "${RED}FAILED (exit $exit_code)${NC}"
                failed_instances=$((failed_instances + 1))
            fi
        done
    fi
    echo
done

echo -e "${YELLOW}========================================${NC}"
echo -e "${YELLOW}SUMMARY${NC}"
echo -e "${YELLOW}========================================${NC}"
echo "Total instances:   $total_instances"
echo -e "Solved:            ${GREEN}$solved_instances${NC}"
echo -e "Timeout (>10s):    ${YELLOW}$timeout_instances${NC}"
echo -e "Failed:            ${RED}$failed_instances${NC}"
echo -e "${YELLOW}========================================${NC}"
