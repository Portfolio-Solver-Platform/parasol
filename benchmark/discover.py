from pathlib import Path

EXCLUDED_PREFIXES = ("license", "readme")
INSTANCE_EXTENSIONS = (".dzn", ".json")


def discover_problems(base: Path) -> list[tuple[Path, Path | None]]:
    problems = []

    for folder in sorted(base.iterdir()):
        if not folder.is_dir():
            continue

        models = sorted(folder.glob("*.mzn"))
        instances = sorted(
            f for f in folder.iterdir()
            if f.suffix in INSTANCE_EXTENSIONS
            and not f.name.lower().startswith(EXCLUDED_PREFIXES)
        )

        if not models:
            continue

        if len(models) == 1 and instances:
            for instance in instances:
                problems.append((models[0], instance))
        elif not instances:
            for model in models:
                problems.append((model, None))
        else:
            for model in models:
                for instance in instances:
                    problems.append((model, instance))

    return problems
