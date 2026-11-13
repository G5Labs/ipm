#!/usr/bin/env python3
"""Workspace Cargo metadata for local validation, without requiring host Rust.

The preflight audits (check-layout, check-just-commands, test_ci_docker_contract,
test_validation_review) and selector narrowing in local_validation only need
`cargo metadata --no-deps` to enumerate workspace packages and their targets —
no compilation. `just docker-ci` must run on a fresh Docker-capable host with no
Rust toolchain (G5-Lean#355), so this routes that one command through host cargo
when it works and otherwise through a minimal pinned container
(infra/docker/Dockerfile.validation-metadata: single external base, ubuntu +
rustup). Container paths are normalized back to the host checkout so callers are
backend-agnostic. Host Rust/Cargo state is never installed or modified.
"""
from __future__ import annotations

import hashlib
import json
import os
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
DOCKERFILE = ROOT / "infra/docker/Dockerfile.validation-metadata"
CONTAINER_SRC = "/src"
# A host `cargo` that is a rustup proxy auto-installs a missing selected
# toolchain by default; that would modify host Rust state (forbidden) instead of
# falling back to the container. Disable it so an absent/mismatched toolchain
# fails the probe and routes to the container runner. (rustup >= 1.28)
HOST_PROBE_ENV = {**os.environ, "RUSTUP_AUTO_INSTALL": "0"}
# `--no-deps` keeps this to manifest parsing; `--offline --locked` forbid any
# network or lockfile mutation so it is safe against a read-only, networkless
# container mount. The repo always carries a current Cargo.lock.
METADATA_ARGS = ["metadata", "--no-deps", "--offline", "--locked", "--format-version", "1"]


class MetadataError(RuntimeError):
    """Raised when workspace metadata cannot be produced on either backend."""


def image_tag():
    digest = hashlib.sha256(DOCKERFILE.read_bytes()).hexdigest()[:16]
    return f"g5-validation-metadata:{digest}"


def _run(command, env=None):
    return subprocess.run(command, cwd=ROOT, capture_output=True, text=True, env=env)


def _host_metadata():
    """Return parsed host `cargo metadata`, or None if host cargo cannot run it.

    Runs with rustup auto-install disabled so a rustup shim whose selected
    toolchain is absent (or mismatched) exits non-zero instead of installing it;
    that and a missing cargo binary (FileNotFoundError) both fall back to the
    container, never touching host Rust state.
    """
    try:
        result = _run(["cargo", *METADATA_ARGS], env=HOST_PROBE_ENV)
    except FileNotFoundError:
        return None
    return json.loads(result.stdout) if result.returncode == 0 else None


def _rebase_paths(value, prefix, replacement):
    """Rewrite absolute paths rooted at `prefix` to `replacement`, recursively.

    Only whole path components are rebased (an exact match or a `prefix/` start),
    so an interior directory literally named `src` is never touched.
    """
    if isinstance(value, str):
        if value == prefix:
            return replacement
        if value.startswith(prefix + "/"):
            return replacement + value[len(prefix):]
        return value
    if isinstance(value, list):
        return [_rebase_paths(item, prefix, replacement) for item in value]
    if isinstance(value, dict):
        return {key: _rebase_paths(item, prefix, replacement) for key, item in value.items()}
    return value


def _ensure_image(log=None):
    tag = image_tag()
    if subprocess.run(["docker", "image", "inspect", tag],
                      capture_output=True, text=True).returncode == 0:
        return tag
    build = subprocess.run(
        ["docker", "build", "-f", str(DOCKERFILE), "-t", tag, str(ROOT)],
        capture_output=True, text=True)
    if log is not None:
        log.write(build.stdout + build.stderr)
    if build.returncode:
        raise MetadataError(
            f"failed to build the cargo-metadata runner image {tag}: "
            f"{(build.stderr or build.stdout)[-2000:]}")
    return tag


def _container_metadata(log=None):
    tag = _ensure_image(log=log)
    result = _run(["docker", "run", "--rm", "--network", "none",
                   "-v", f"{ROOT}:{CONTAINER_SRC}:ro", "-w", CONTAINER_SRC,
                   tag, "cargo", *METADATA_ARGS])
    if result.returncode:
        raise MetadataError(
            "cargo metadata failed inside the runner container: "
            f"{(result.stderr or result.stdout)[-2000:]}")
    # The container reports paths under /src; rebase them to the host checkout
    # so callers that stat src_path / relativize manifest_path see host paths.
    return _rebase_paths(json.loads(result.stdout), CONTAINER_SRC, str(ROOT))


def cargo_metadata(log=None):
    """Parsed `cargo metadata --no-deps` for the workspace, host or container.

    `log`, when given, receives the container build output on the fallback path.
    """
    metadata = _host_metadata()
    if metadata is None:
        metadata = _container_metadata(log=log)
    return metadata


if __name__ == "__main__":
    json.dump(cargo_metadata(), __import__("sys").stdout)
