#!/usr/bin/env python3
"""Cheap migration and local fixture identity contracts; no Docker daemon needed."""

from pathlib import Path
import re
import sys

ROOT = Path(__file__).resolve().parents[1]


def fixture_identity_lines(text, rust=False):
    """Inspect quoted literals without interpreting URL slashes as comments."""
    quoted = r'''"(?:\\.|[^"\\])*"|`(?:\\.|[^`\\])*`'''
    quoted += r"|'(?:\\.|[^'\\])'" if rust else r"|'(?:\\.|[^'\\])*'"
    if rust:
        quoted = r'r(?P<hashes>#*)"[\s\S]*?"(?P=hashes)|' + quoted
    tokens = re.compile(r'(?P<comment>//[^\n]*|/\*[\s\S]*?\*/)|(?P<literal>' + quoted + ')')
    for match in tokens.finditer(text):
        # Exact Compose identities only: a hyphenated suffix (g5-test-client-pa) is a
        # Cloud Build-only name, not a local service, so it is not a fixture identity.
        if match.group('literal') and re.search(r'\bg5-test-(?:api|postgres|client|worker|g5d-worker)(?![\w-])', match.group('literal')):
            yield text.count('\n', 0, match.start()) + 1


def check(root):
    problems = []
    versions = {}
    for path in sorted((root / "backend/g5-api/src/db/migrations").glob("*.sql")):
        match = re.match(r"([0-9]+)_", path.name)
        if not match:
            problems.append(f"invalid migration version: {path.name}")
            continue
        number = int(match[1])
        if number in versions:
            problems.append(f"duplicate migration version {number}: {versions[number]} and {path.name}")
        versions[number] = path.name
    if not versions:
        problems.append("migration catalog is empty")
    # Ignore explanatory comments; executable fixtures must receive identity.
    for directory in [root / "frontend/e2e", root / "tests/e2e/src"]:
        for path in directory.rglob("*"):
            if path.suffix not in {".ts", ".rs"}:
                continue
            for number in fixture_identity_lines(path.read_text(), rust=path.suffix == ".rs"):
                problems.append(f"{path.relative_to(root)}:{number}: pass the Compose identity through the environment")
    return problems


if __name__ == "__main__":
    failures = check(ROOT)
    if failures:
        print("\n".join(failures), file=sys.stderr)
    else:
        print("Migration versions and fixture identities checked.")
    sys.exit(bool(failures))
