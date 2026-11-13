#!/usr/bin/env python3
"""Exercise real template CLI self-launch on Linux with a reflink-capable scratch root.

No registry, credentials, root mount or KVM is required. The tiny OCI fixture is
only for filesystem preparation; its marker files are deliberately not runnable.
Use a release g5d binary to cover behavior hidden by cfg(test) helper fixtures.
"""
import argparse
import gzip
import hashlib
import io
import json
import os
from pathlib import Path
import subprocess
import sys
import tarfile
import tempfile
import uuid


def qualify(binary, scratch):
    if sys.platform != "linux":
        raise RuntimeError("This integration test requires Linux")
    binary = Path(binary).resolve(strict=True)
    environment = dict(os.environ, GIT_CONFIG_GLOBAL=os.devnull, GIT_CONFIG_NOSYSTEM="1")
    with tempfile.TemporaryDirectory(prefix="g5-template-cli-", dir=scratch) as temporary:
        root = Path(temporary)
        state = root / "state"
        environment.update(G5D_ROOTFS_STATE_DIR=str(state), G5D_OCI_CACHE_DIR=str(root / "layers"))

        def run(*arguments):
            result = subprocess.run([str(arg) for arg in arguments], env=environment,
                                    capture_output=True, text=True, timeout=120)
            if result.returncode:
                raise RuntimeError(f"{arguments[0]} exited {result.returncode}: {result.stderr}")
            return result.stdout.strip()

        layout = root / "image"
        blobs = layout / "blobs/sha256"
        blobs.mkdir(parents=True)

        def blob(content, media_type):
            digest = hashlib.sha256(content).hexdigest()
            (blobs / digest).write_bytes(content)
            return {"mediaType": media_type, "digest": "sha256:" + digest, "size": len(content)}

        raw = io.BytesIO()
        with tarfile.open(fileobj=raw, mode="w") as archive:
            for name in ("bin/sh", "usr/bin/rsync", "usr/bin/sha256sum"):
                data = b"preparation fixture: never execute image files on the host\n"
                entry = tarfile.TarInfo(name)
                entry.size, entry.mode = len(data), 0o644
                archive.addfile(entry, io.BytesIO(data))
        layer = blob(gzip.compress(raw.getvalue(), mtime=0), "application/vnd.oci.image.layer.v1.tar+gzip")
        config = blob(json.dumps({"architecture": "amd64", "os": "linux", "config": {},
                                  "rootfs": {"type": "layers", "diff_ids": [
                                      "sha256:" + hashlib.sha256(raw.getvalue()).hexdigest()]}}).encode(),
                      "application/vnd.oci.image.config.v1+json")
        manifest = blob(json.dumps({"schemaVersion": 2, "config": config, "layers": [layer]}).encode(),
                        "application/vnd.oci.image.manifest.v1+json")
        (layout / "index.json").write_text(json.dumps({"schemaVersion": 2, "manifests": [manifest]}))
        (layout / "oci-layout").write_text('{"imageLayoutVersion":"1.0.0"}')

        source = root / "source"
        source.mkdir()
        run("git", "-C", source, "init", "-q")
        (source / "tracked").write_text("committed seed\n")
        run("git", "-C", source, "add", "tracked")
        run("git", "-C", source, "-c", "user.name=Fixture", "-c", "user.email=fixture@example.invalid",
            "commit", "-qm", "seed")
        commit = run("git", "-C", source, "rev-parse", "HEAD")
        bundle = root / "seed.bundle"
        run("git", "-C", source, "bundle", "create", bundle, "HEAD")
        repo_id = str(uuid.uuid4())
        seed = root / "seed.json"
        seed.write_text(json.dumps({"scope": {"org_id": str(uuid.uuid4()),
            "repos": [{"repo_id": repo_id, "root_path": "code", "git_url": "https://fixture.invalid/source"}]},
            "repositories": [{"repo_id": repo_id, "bundle": bundle.name, "commit": commit}]}))
        run(binary, "oci", "prepare-workload", layout, "--size-mib", "64", "--repo-seed-manifest", seed)
        cache = state / "templates/workload-block"
        disks = [cache / variant / "image.ext4" for variant in ("current", "repo-current")]
        before = [(path.stat().st_ino, path.stat().st_size) for path in disks]
        assert all(size == 64 * 1024 * 1024 for _, size in before)
        assert all(path.stat().st_mode & 0o222 == 0 for path in disks)
        # Inspect only the pristine builder output, never a guest-written disk.
        assert run("debugfs", "-R", "cat /work/code/tracked", disks[1]) == "committed seed"
        assert json.loads((cache / "repo-current/identity.json").read_text())["commits"] == [[repo_id, commit]]
        # Host activation uses generic prewarm and must preserve both baked disks.
        run(binary, "oci", "prepare-workload", layout, "--size-mib", "64")
        assert before == [(path.stat().st_ino, path.stat().st_size) for path in disks]
        assert not list(state.glob(".delivery-*"))
    return {"passed": True, "seed_prepared": True, "prewarm_reused_both_disks": True}


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", required=True, type=Path)
    parser.add_argument("--scratch-root", required=True, type=Path)
    args = parser.parse_args()
    print(json.dumps(qualify(args.binary, args.scratch_root)))
