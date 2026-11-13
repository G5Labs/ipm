#!/usr/bin/env python3
"""Audit recipe syntax and checked-in entry points without executing recipes."""

from __future__ import annotations

import json
from pathlib import Path
import re
import subprocess
import sys

import workspace_metadata

ROOT = Path(__file__).resolve().parents[1]
SAMPLE_ARGUMENTS = {
    "crate": "g5-contracts",
    "name": "command_audit_probe",
    "repo_id": "command-audit-repository",
    "build_id": "command-audit-build",
}


def main() -> int:
    document = json.loads(subprocess.check_output(
        ["just", "--dump", "--dump-format", "json"], cwd=ROOT,
    ))
    # Routed through the metadata gateway so this audit runs without host Rust
    # (G5-Lean#355): host cargo when present, else a pinned offline container.
    metadata = workspace_metadata.cargo_metadata()
    packages = {package["name"]: package for package in metadata["packages"]}
    problems: list[str] = []
    entry_points: set[str] = set()
    for name, recipe in document["recipes"].items():
        arguments = [
            SAMPLE_ARGUMENTS.get(parameter["name"], "audit-probe")
            for parameter in recipe["parameters"]
            if parameter["default"] is None and parameter["kind"] == "singular"
        ]
        rendered = subprocess.run(
            ["just", "--dry-run", name, *arguments], cwd=ROOT,
            capture_output=True, text=True,
        )
        if rendered.returncode:
            problems.append(f"{name}: cannot render: {rendered.stderr.strip()}")
            continue
        body = rendered.stderr or rendered.stdout
        parsed = subprocess.run(
            ["bash", "-n"], input=body, capture_output=True, text=True,
        )
        if parsed.returncode:
            problems.append(f"{name}: {parsed.stderr.strip()}")
        # Skip comments: old paths can be useful documentation. Only test
        # concrete repository entry points, not generated files or variables.
        commands = "\n".join(
            line for line in body.splitlines() if not line.lstrip().startswith("#")
        )
        for match in re.finditer(
            r"(?<![\w/])(?:\./)?((?:scripts|infra/docker)/[\w./-]+\.(?:sh|py|js|mjs))",
            commands,
        ):
            path = match[1]
            entry_points.add(path)
            if not (ROOT / path).is_file():
                problems.append(f"{name}: missing entry point {path}")
        for package, target in re.findall(
            r"cargo test -p ([\w-]+) --test ([\w-]+)", commands,
        ):
            targets = packages.get(package, {}).get("targets", [])
            if not any(item["name"] == target and "test" in item["kind"] for item in targets):
                problems.append(f"{name}: missing Rust test target {package}/{target}")
    if problems:
        print("Just command audit failed:", file=sys.stderr)
        for problem in problems:
            print(f"- {problem}", file=sys.stderr)
        return 1
    print(
        f"Just command audit passed: {len(document['recipes'])} recipes parse, "
        f"{len(entry_points)} script entry points exist, named Rust test targets resolve."
    )
    print("This does not execute cloud, database, Docker or live-provider operations.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
