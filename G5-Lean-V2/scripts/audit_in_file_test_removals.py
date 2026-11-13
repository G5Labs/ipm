#!/usr/bin/env python3
"""Verify the exact-name ledger for tests removed inside retained source files."""

from __future__ import annotations

import argparse
import csv
import io
import pathlib
import re
import subprocess
import sys


ROOT = pathlib.Path(__file__).resolve().parent.parent
BASELINE_FILE = ROOT / "docs/lean-mvp-test-removal-baseline.txt"
LEDGER_FILE = ROOT / "docs/lean-mvp-in-file-test-removals.tsv"
RUST_TEST = re.compile(
    r"#\s*\[(?:tokio::)?test(?:\([^\]]*\))?\]\s*"
    r"(?:#\s*\[[^\]]+\]\s*)*(?:async\s+)?fn\s+([A-Za-z_][A-Za-z0-9_]*)",
    re.S,
)
FRONTEND_TEST = re.compile(
    r"(?<![A-Za-z0-9_])(?:it|test)(?:\.(?:skip|only|todo|concurrent|fails))?"
    r"\s*\(\s*([\"'`])((?:\\.|(?!\1).)*?)\1",
    re.S,
)


def git(*args: str, capture: bool = True) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        ["git", *args],
        cwd=ROOT,
        text=True,
        capture_output=capture,
        check=False,
    )


def baseline_text(base: str, path: str) -> str | None:
    result = git("show", f"{base}:{path}")
    return result.stdout if result.returncode == 0 else None


def working_text(path: str) -> str | None:
    try:
        return (ROOT / path).read_text()
    except (OSError, UnicodeDecodeError):
        return None


def rust_tests(source: str) -> set[str]:
    return set(RUST_TEST.findall(source))


def frontend_tests(source: str) -> set[str]:
    return {
        re.sub(r"\s+", " ", match[1]).strip()
        for match in FRONTEND_TEST.findall(source)
    }


def removed_tests(base: str) -> list[tuple[str, str, str]]:
    changed = git("diff", "--name-only", base, "--")
    if changed.returncode != 0:
        raise RuntimeError(changed.stderr.strip())
    rows: list[tuple[str, str, str]] = []
    for path in changed.stdout.splitlines():
        if path.endswith(".rs"):
            kind, extractor = "rust", rust_tests
        elif path.endswith((".ts", ".tsx", ".js", ".jsx")):
            kind, extractor = "frontend", frontend_tests
        else:
            continue
        old = baseline_text(base, path)
        new = working_text(path)
        if old is None or new is None:
            continue
        for name in sorted(extractor(old) - extractor(new)):
            rows.append((kind, path, name))
    return sorted(rows)


def render(rows: list[tuple[str, str, str, str, str]]) -> str:
    lines = ["kind\tpath\ttest\tdisposition\tevidence"]
    for row in rows:
        lines.append(
            "\t".join(value.replace("\t", " ").replace("\n", " ") for value in row)
        )
    return "\n".join(lines) + "\n"


def read_ledger() -> list[tuple[str, str, str, str, str]]:
    if not LEDGER_FILE.exists():
        return []
    reader = csv.DictReader(io.StringIO(LEDGER_FILE.read_text()), delimiter="\t")
    expected_header = ["kind", "path", "test", "disposition", "evidence"]
    if reader.fieldnames != expected_header:
        raise RuntimeError(f"ledger header must be {expected_header}")
    return [
        (
            row["kind"],
            row["path"],
            row["test"],
            row["disposition"],
            row["evidence"],
        )
        for row in reader
    ]


def validate_ledger(
    removed: list[tuple[str, str, str]],
    rows: list[tuple[str, str, str, str, str]],
) -> list[str]:
    errors: list[str] = []
    allowed = {
        "explicit_validation_gap",
        "rehomed_boundary",
        "retired_internal_helper",
        "retired_protocol_or_surface",
    }
    keyed = {(kind, path, test): (disposition, evidence) for kind, path, test, disposition, evidence in rows}
    if len(keyed) != len(rows):
        errors.append("ledger contains duplicate kind/path/test rows")
    removed_set = set(removed)
    missing = sorted(removed_set - set(keyed))
    stale = sorted(set(keyed) - removed_set)
    if missing:
        errors.append(f"ledger is missing {len(missing)} removed tests; first: {missing[0]}")
    if stale:
        errors.append(f"ledger contains {len(stale)} restored or stale tests; first: {stale[0]}")
    for key, (disposition, evidence) in keyed.items():
        if disposition not in allowed:
            errors.append(f"{key}: unknown disposition {disposition!r}")
        if len(evidence.strip()) < 40:
            errors.append(f"{key}: evidence must state a concrete replacement or gap")
    return errors


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--write", action="store_true")
    parser.add_argument("--base")
    args = parser.parse_args()
    base = args.base or BASELINE_FILE.read_text().strip()
    removed = removed_tests(base)
    rows = read_ledger()
    if args.write:
        existing = {
            (kind, path, test): (disposition, evidence)
            for kind, path, test, disposition, evidence in rows
        }
        updated = [
            (*key, *existing.get(key, ("UNCLASSIFIED", "REVIEW REQUIRED")))
            for key in removed
        ]
        LEDGER_FILE.write_text(render(updated))
        print(f"wrote {LEDGER_FILE.relative_to(ROOT)}")
        rows = updated
    errors = validate_ledger(removed, rows)
    if errors:
        for error in errors:
            print(error, file=sys.stderr)
        return 1
    count = len(removed)
    print(f"verified {count} in-file test removals against {base}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
