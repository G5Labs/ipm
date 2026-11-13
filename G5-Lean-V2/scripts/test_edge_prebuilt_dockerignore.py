#!/usr/bin/env python3
"""Regression test: the prebuilt edge image sees frontend/dist in its context.

The repository-root .dockerignore excludes every `dist` directory so a stale
build output never lands in the source-built images. Dockerfile.edge-prebuilt
packages a prebuilt frontend/dist (the CI `frontend-build` step writes it), so
it carries its own `Dockerfile.edge-prebuilt.dockerignore`, which BuildKit reads
from the Dockerfile's directory in preference to the root file.

The test builds two `FROM scratch` images from a synthetic context with
BuildKit's local exporter and inspects what `COPY . /ctx` saw:

* a Dockerfile named like the real one, next to a copy of its ignore file, sees
  exactly the paths the real Dockerfile.edge-prebuilt COPYs and nothing else;
* any other Dockerfile in the same directory still gets the root rules:
  frontend/dist stays excluded while source files stay included.

Needs a Docker daemon with BuildKit (`docker build --output`), like the other
Docker-backed suites run by scripts/run-ci-script-tests.py; the test is skipped
without one.
"""
from pathlib import Path
import os
import shlex
import shutil
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]
DOCKERFILE = ROOT / "infra/docker/Dockerfile.edge-prebuilt"
IGNORE = ROOT / "infra/docker/Dockerfile.edge-prebuilt.dockerignore"
ROOT_IGNORE = ROOT / ".dockerignore"

# Files placed in the synthetic context. The first three are what the prebuilt
# build needs; the rest are decoys it must never see.
CONTEXT_FILES = [
    "frontend/dist/index.html",
    "frontend/dist/assets/app-abc123.js",
    "infra/docker/Caddyfile",
    "infra/docker/caddy-ci-tls.caddyfile",
    "infra/docker/edge-entrypoint.sh",
    "frontend/src/main.tsx",
    "frontend/package.json",
    "frontend/node_modules/left-pad/index.js",
    "backend/g5-api/dist/stale.txt",
    "dist/stale.txt",
    "Cargo.toml",
    ".env",
]
SCRATCH_DOCKERFILE = "FROM scratch\nCOPY . /ctx\n"


def copy_sources(dockerfile: Path) -> list[str]:
    """Return the build-context sources of every COPY/ADD that is not --from."""
    text = dockerfile.read_text().replace("\\\n", " ")
    sources = []
    for line in text.splitlines():
        if not line.startswith(("COPY ", "ADD ")) or "--from=" in line:
            continue
        tokens = shlex.split(line)
        sources.extend(token for token in tokens[1:-1] if not token.startswith("--"))
    return sources


def docker_available() -> bool:
    try:
        return subprocess.run(["docker", "info"], capture_output=True, timeout=120).returncode == 0
    except (OSError, subprocess.TimeoutExpired):
        return False


def exported_context(context: Path, dockerfile: Path) -> set[str]:
    """Build `dockerfile` against `context` and return the files COPY saw."""
    out = context.parent / f"out-{dockerfile.name}"
    result = subprocess.run(
        ["docker", "build", "--output", f"type=local,dest={out}",
         "--file", str(dockerfile), str(context)],
        env={**os.environ, "DOCKER_BUILDKIT": "1"},
        text=True, capture_output=True,
    )
    if result.returncode:
        raise AssertionError(f"docker build failed for {dockerfile.name}:\n{result.stderr}")
    ctx = out / "ctx"
    return {path.relative_to(ctx).as_posix() for path in ctx.rglob("*") if path.is_file()}


@unittest.skipUnless(docker_available(), "docker daemon unavailable")
class EdgePrebuiltDockerignore(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.context = Path(self.temporary.name) / "context"
        for relative in CONTEXT_FILES:
            path = self.context / relative
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(f"{relative}\n")
        shutil.copy(ROOT_IGNORE, self.context / ".dockerignore")
        docker_dir = self.context / "infra/docker"
        shutil.copy(IGNORE, docker_dir / IGNORE.name)
        self.prebuilt = docker_dir / DOCKERFILE.name
        self.prebuilt.write_text(SCRATCH_DOCKERFILE)
        self.other = docker_dir / "Dockerfile.other"
        self.other.write_text(SCRATCH_DOCKERFILE)

    def test_prebuilt_context_is_exactly_what_the_dockerfile_copies(self):
        sources = copy_sources(DOCKERFILE)
        self.assertIn("frontend/dist", sources)
        seen = exported_context(self.context, self.prebuilt)
        for source in sources:
            self.assertTrue(
                any(path == source or path.startswith(source.rstrip("/") + "/") for path in seen),
                f"{DOCKERFILE.name} copies {source} but the build context hides it: {sorted(seen)}",
            )
        expected = {
            relative for relative in CONTEXT_FILES
            if any(relative == source or relative.startswith(source.rstrip("/") + "/") for source in sources)
        }
        self.assertEqual(seen, expected)

    def test_other_dockerfiles_keep_the_root_rules(self):
        seen = exported_context(self.context, self.other)
        self.assertIn("frontend/src/main.tsx", seen)
        self.assertIn("infra/docker/Caddyfile", seen)
        self.assertFalse(
            {path for path in seen if path.startswith("frontend/dist/")},
            f"root .dockerignore no longer excludes frontend/dist: {sorted(seen)}",
        )
        self.assertNotIn("dist/stale.txt", seen)
        self.assertNotIn("backend/g5-api/dist/stale.txt", seen)
        self.assertNotIn("frontend/node_modules/left-pad/index.js", seen)


if __name__ == "__main__":
    unittest.main()
