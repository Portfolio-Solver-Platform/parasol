from pathlib import Path

EXCLUDED_PREFIXES = ("license", "readme")
INSTANCE_EXTENSIONS = (".dzn", ".json")


def discover_problems(base: Path, start_from_instance: str) -> list[tuple[Path, Path | None]]:
    problems = []

    found_instance=False
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
        
        # skip instances until start instance found
        if start_from_instance is not None and not found_instance:
            skip = 0
            for i, instance in enumerate(instances):
                if start_from_instance in instance.name:
                    found_instance = True
                    skip = i
                    break
            if not found_instance: 
                continue
            else:
                instances = instances[skip:]
            
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
