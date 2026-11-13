#!/usr/bin/env python3
"""Check physical build inputs after repository moves, without starting services."""

from __future__ import annotations

import glob
import json
from pathlib import Path
import re
import shlex
import subprocess
import sys

import workspace_metadata
import tomllib

ROOT = Path(__file__).resolve().parents[1]


def main() -> int:
    errors: list[str] = []
    manifest = tomllib.loads((ROOT / "Cargo.toml").read_text())
    members = manifest["workspace"]["members"]
    if not members:
        errors.append("Cargo workspace has no members")
    for member in members:
        if not (ROOT / member / "Cargo.toml").is_file():
            errors.append(f"missing workspace member: {member}")
        if not list((ROOT / member / "src").glob("*.rs")):
            errors.append(f"empty source root: {member}")

    # Routed through the metadata gateway so this audit runs without host Rust
    # (G5-Lean#355): host cargo when present, else a pinned offline container.
    try:
        packages = workspace_metadata.cargo_metadata()["packages"]
    except workspace_metadata.MetadataError as error:
        errors.append(str(error))
        packages = []
    for package in packages:
        for target in package["targets"]:
            if not Path(target["src_path"]).is_file():
                errors.append(f"missing target: {target['src_path']}")

    # These macros resolve paths relative to the source file, independently of cwd.
    include_pattern = re.compile(r'include_(?:str|bytes)!\(\s*"([^"\n]+)"')
    embedded_inputs = 0
    for member in members:
        for source in (ROOT / member).rglob("*.rs"):
            if "fixtures" in source.parts:
                continue
            text = source.read_text()
            for match in include_pattern.finditer(text):
                embedded_inputs += 1
                if not (source.parent / match[1]).is_file():
                    line = text.count("\n", 0, match.start()) + 1
                    errors.append(
                        f"{source.relative_to(ROOT)}:{line}: missing embedded input {match[1]}"
                    )

    # Top-level Dockerfiles use the repo root as context. Prebuilt release inputs
    # are produced by CI and intentionally do not exist in a fresh checkout.
    docker_inputs = 0
    for dockerfile in (ROOT / "infra/docker").glob("Dockerfile*"):
        text = dockerfile.read_text().replace("\\\n", " ")
        # A Dockerfile that stages per-crate manifests (root manifest plus lock
        # file, then one COPY per member) must stage every workspace member, or
        # cargo refuses to load the workspace before building anything.
        copied_manifests = set(re.findall(r"^COPY (\S+/Cargo\.toml) ", text, re.MULTILINE))
        if copied_manifests and re.search(r"^COPY Cargo\.toml Cargo\.lock ", text, re.MULTILINE):
            for member in members:
                if f"{member}/Cargo.toml" not in copied_manifests:
                    errors.append(
                        f"{dockerfile.name}: workspace member {member} is not staged "
                        f"(add COPY {member}/Cargo.toml and its stub sources)"
                    )
        # This is a registry image reference, never a repository folder path.
        for frontend in re.findall(r"^#\s*syntax=(\S+)", text, re.MULTILINE):
            if not frontend.startswith("docker/dockerfile:"):
                errors.append(f"{dockerfile.name}: unexpected Dockerfile frontend {frontend}")
        for line in text.splitlines():
            if line.startswith("RUN rm -rf "):
                for source in shlex.split(line)[3:]:
                    if (ROOT / "backend" / source / "Cargo.toml").is_file():
                        errors.append(f"{dockerfile.name}: placeholder cleanup still targets {source}")
            if not line.startswith(("COPY ", "ADD ")) or "--from=" in line:
                continue
            tokens = shlex.split(line)
            for source in tokens[1:-1]:
                if source.startswith("--") or "$" in source:
                    continue
                if source.startswith(("https://", "http://", ".ci-release-artifacts/")):
                    continue
                if dockerfile.name == "Dockerfile.edge-prebuilt" and source == "frontend/dist":
                    # Produced by frontend-build, like the prebuilt Rust artifacts.
                    # Source/image COPY checks still apply to every other input.
                    continue
                docker_inputs += 1
                if not glob.glob(str(ROOT / source)):
                    errors.append(f"{dockerfile.name}: missing COPY/ADD input {source}")

    if (ROOT / "generation").exists():
        errors.append("retired generation/ directory still exists")

    if errors:
        print("Layout check failed:", file=sys.stderr)
        for error in errors:
            print(f"- {error}", file=sys.stderr)
        return 1
    print(
        f"Layout OK: {len(members)} Cargo members, {embedded_inputs} embedded Rust inputs, "
        f"{docker_inputs} Docker inputs."
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
