# Portfolio Solver Framework

## Setup
To make your own portofolio solver available to MiniZinc do the following:
Take the `solver.msc.template` and copy it to one of these paths:
- `/usr/share/minizinc/solvers` (on Linux only)
- `$HOME/.minizinc/solvers`
- For additional options, see the [MiniZinc documentation](https://docs.minizinc.dev/en/stable/fzn-spec.html#solver-configuration-files)

and remove the `.template` suffix from the file name and replace `${EXECUTABLE_PATH}` in the file with the path to the executable.

