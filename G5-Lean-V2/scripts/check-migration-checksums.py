#!/usr/bin/env python3
"""Verify the reviewed SQLx migration catalog without database/GitHub access."""

import argparse
import hashlib
from pathlib import Path
import re
import sys


def check(directory: Path) -> list[str]:
    manifest = directory / "checksums.sha384"
    entries = {}
    for number, line in enumerate(manifest.read_text().splitlines(), 1):
        match = re.fullmatch(r"([0-9a-f]{96})  ([0-9]+_[A-Za-z0-9_.-]+\.sql)", line)
        if not match:
            raise ValueError(f"{manifest}:{number}: expected SHA384, two spaces, SQL filename")
        digest, name = match.groups()
        if name in entries:
            raise ValueError(f"{manifest}:{number}: duplicate entry for {name}")
        entries[name] = digest
    if not entries:
        raise ValueError(f"{manifest}: empty migration catalog")

    files = {path.name: path for path in directory.glob("*.sql")}
    problems = []
    versions = {}
    for name in sorted(files):
        version = int(name.split("_", 1)[0])
        if version in versions:
            problems.append(f"{name}: duplicate migration version {version} ({versions[version]})")
        versions[version] = name
    for name in sorted(entries.keys() - files.keys()):
        problems.append(f"{name}: recorded migration is missing (deletion/rename forbidden)")
    for name in sorted(files.keys() - entries.keys()):
        problems.append(f"{name}: new migration needs a reviewed checksums.sha384 entry")
    for name in sorted(files.keys() & entries.keys()):
        path = files[name]
        if path.is_symlink() or not path.is_file():
            problems.append(f"{name}: migration must be a regular file")
        elif hashlib.sha384(path.read_bytes()).hexdigest() != entries[name]:
            problems.append(f"{name}: bytes changed; restore the file and add a new migration")
    return problems


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--directory", type=Path,
                        default=Path("backend/g5-api/src/db/migrations"))
    args = parser.parse_args()
    try:
        problems = check(args.directory)
    except (OSError, ValueError) as exc:
        problems = [str(exc)]
    if problems:
        print("Migration checksum validation failed:", file=sys.stderr)
        for problem in problems:
            print(f"  - {problem}", file=sys.stderr)
        return 1
    print("SQLx migration checksums match the reviewed catalog.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
