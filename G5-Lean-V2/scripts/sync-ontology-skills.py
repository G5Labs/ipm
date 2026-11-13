#!/usr/bin/env python3
"""Generate or check local skills and the documentation guidance bundle."""

import argparse
from pathlib import Path
import shutil


MIRRORS = (".codex/skills", ".claude/skills",
           "g5_docs/docs/lane_1_ontology_design/ontology_experiment_harness/skills")
RETIRED = {"ontology-review", "ontology-design-review", "ontology-review-learning",
           "ontology-experiment", "ontology-authoring", "ontology-uplift", "ontology-downlift"}
DOC_BUNDLE = Path(MIRRORS[-1]).parent


def files(directory):
    return {p.relative_to(directory): p.read_bytes()
            for p in directory.rglob("*") if p.is_file() and "__pycache__" not in p.parts}


def expected_skill(source, *, documentation=False):
    contents = files(source)
    if documentation:
        # skills/<name> is one level shallower than .agents/skills/<name>.
        contents = {path: data.replace(b"../../../.agents/guidance/", b"../../.agents/guidance/")
                    if path.suffix == ".md" else data for path, data in contents.items()}
    return contents


def write_files(target, contents):
    for relative, data in contents.items():
        path = target / relative
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(data)


def sync(root, *, check=False):
    sources = {}
    for entry in sorted((root / ".agents/skills").glob("*/SKILL.md")):
        if entry.parent.is_symlink() or any(p.is_symlink() for p in entry.parent.rglob("*")):
            raise ValueError(f"skill source must contain regular files: {entry.parent}")
        sources[entry.parent.name] = entry.parent
    if not sources:
        raise ValueError("no skill sources found")
    errors = []
    for relative in MIRRORS:
        base = root / relative
        if base.is_symlink():
            raise ValueError(f"skill mirror must be a regular directory: {base}")
        for name, source in sources.items():
            target = base / name
            expected = expected_skill(source, documentation=relative == MIRRORS[-1])
            if target.is_symlink() or any(p.is_symlink() for p in target.rglob("*")):
                raise ValueError(f"skill mirror must contain regular files: {target}")
            if check:
                if not target.is_dir() or expected != files(target):
                    errors.append(f"missing or mismatched skill: {relative}/{name}")
            else:
                if target.exists():
                    shutil.rmtree(target)
                write_files(target, expected)
        for entry in base.glob("*/SKILL.md"):
            if entry.parent.name in sources:
                continue
            if not check and entry.parent.name in RETIRED and not entry.parent.is_symlink():
                shutil.rmtree(entry.parent)
            else:
                errors.append(f"unexpected skill: {entry.parent.relative_to(root)}")
    guidance = root / ".agents/guidance"
    if guidance.is_dir():
        target = root / DOC_BUNDLE / ".agents/guidance"
        if target.is_symlink() or any(p.is_symlink() for p in target.rglob("*")):
            raise ValueError(f"guidance mirror must contain regular files: {target}")
        if check:
            if not target.is_dir() or files(guidance) != files(target):
                errors.append("missing or mismatched documentation guidance")
        else:
            if target.exists():
                shutil.rmtree(target)
            write_files(target, files(guidance))
    return errors


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    mode = parser.add_mutually_exclusive_group(required=True)
    mode.add_argument("--check", action="store_true", help="report drift without writing")
    mode.add_argument("--write", action="store_true", help="regenerate maintained skill copies")
    args = parser.parse_args()
    try:
        errors = sync(Path(__file__).resolve().parents[1], check=args.check)
    except (OSError, ValueError) as error:
        errors = [str(error)]
    if errors:
        print("\n".join(errors))
        return 1
    print("Skill and guidance copies match the maintained sources.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
