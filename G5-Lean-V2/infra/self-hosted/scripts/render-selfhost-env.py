#!/usr/bin/env python3
"""Render a next self-hosted .env from an existing .env and release manifest."""

from __future__ import annotations

import json
import sys
from pathlib import Path


def read_env(path: Path) -> list[tuple[str | None, str]]:
    rows: list[tuple[str | None, str]] = []
    for line in path.read_text().splitlines():
        stripped = line.strip()
        if not stripped or stripped.startswith("#") or "=" not in line:
            rows.append((None, line))
            continue
        key, value = line.split("=", 1)
        rows.append((key, value))
    return rows


def set_env(rows: list[tuple[str | None, str]], key: str, value: str) -> None:
    for idx, (existing_key, _) in enumerate(rows):
        if existing_key == key:
            rows[idx] = (key, value)
            return
    rows.append((key, value))


def main() -> int:
    if len(sys.argv) != 4:
        print("usage: render-selfhost-env.py <env-in> <manifest> <env-out>", file=sys.stderr)
        return 2

    env_in = Path(sys.argv[1])
    manifest_path = Path(sys.argv[2])
    env_out = Path(sys.argv[3])
    manifest = json.loads(manifest_path.read_text())
    rows = read_env(env_in)

    images = manifest.get("images") or {}
    migration = manifest.get("migration") or {}

    updates = {
        "G5_RELEASE_VERSION": str(manifest.get("version", "")),
        "G5_IMAGE_API": str(images.get("api", "")),
        "G5_IMAGE_EDGE": str(images.get("edge", "")),
        "G5_IMAGE_WORKER": str(images.get("worker", "")),
        "G5_IMAGE_MIGRATION": str(migration.get("image") or images.get("api", "")),
    }
    for key, value in updates.items():
        if value:
            set_env(rows, key, value)

    env_out.write_text("\n".join(f"{key}={value}" if key else value for key, value in rows) + "\n")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
