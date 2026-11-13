#!/usr/bin/env python3
"""Export committed, explicitly scoped repository inputs for a worker-image bake.

Configuration: {"scope": {"org_id": ..., "cache_generation": ..., "repos":
[{"repo_id": ..., "root_path": ..., "git_url": ...}]}, "repositories":
[{"repo_id": ..., "source": ".", "revision": "HEAD"}]}. Sources must be local
trusted delivery checkouts under --source-root. No checkout files/config are copied.
The output manifest pins each revision to an object ID and names a local Git bundle.
"""

import argparse
import json
import os
from pathlib import Path
import subprocess
import tempfile
import uuid


def git(directory, *args):
    environment = {
        "PATH": os.environ.get("PATH", "/usr/bin:/bin"),
        "GIT_CONFIG_NOSYSTEM": "1",
        "GIT_CONFIG_GLOBAL": os.devnull,
        "GIT_TERMINAL_PROMPT": "0",
    }
    return subprocess.run(
        ["git", "-c", "core.hooksPath=/dev/null", "-c", "protocol.file.allow=always",
         "-C", str(directory), *args],
        env=environment, check=True, text=True, stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    ).stdout.strip()


def prepare(config, source_root, output):
    source_root = Path(source_root).resolve(strict=True)
    output = Path(output)
    output.mkdir(parents=True, exist_ok=True)
    if any(output.iterdir()):
        raise ValueError("seed output directory must be empty")
    if set(config) != {"scope", "repositories"}:
        raise ValueError("seed config requires only scope and repositories")
    scope = config["scope"]
    uuid.UUID(scope["org_id"])
    members = {str(uuid.UUID(member["repo_id"])) for member in scope["repos"]}
    sources = config["repositories"]
    if not members or len(members) != len(scope["repos"]) or len(sources) != len(members):
        raise ValueError("repository sources must cover the scoped members exactly")
    if {str(uuid.UUID(source["repo_id"])) for source in sources} != members:
        raise ValueError("repository sources do not match scoped members")
    result = {"scope": scope, "repositories": []}
    for source in sources:
        if set(source) != {"repo_id", "source", "revision"}:
            raise ValueError("source requires repo_id, source and revision")
        repo_id = str(uuid.UUID(source["repo_id"]))
        checkout = (source_root / source["source"]).resolve(strict=True)
        if not checkout.is_relative_to(source_root):
            raise ValueError("seed source escapes trusted delivery checkout")
        revision = source["revision"]
        if not revision or revision.startswith("-") or any(c in revision for c in "\0\n\r"):
            raise ValueError("invalid seed revision")
        commit = git(checkout, "rev-parse", "--verify", revision + "^{commit}")
        bundle = output / (repo_id + ".bundle")
        # Create a fresh repository with only the selected reachable history.
        # Dirty files, config, remotes, hooks and unrelated refs never cross.
        with tempfile.TemporaryDirectory(prefix=".bundle-", dir=output) as temporary:
            git(temporary, "init", "--bare", "--quiet")
            git(temporary, "fetch", "--quiet", "--no-tags", "--no-write-fetch-head",
                str(checkout), commit)
            git(temporary, "update-ref", "refs/heads/seed", commit)
            git(temporary, "bundle", "create", str(bundle.resolve()), "refs/heads/seed")
            git(temporary, "bundle", "verify", str(bundle.resolve()))
        result["repositories"].append({"repo_id": repo_id, "bundle": bundle.name, "commit": commit})
    manifest = output / "seed.json"
    manifest.write_text(json.dumps(result, indent=2) + "\n")
    return manifest


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("config", type=Path)
    parser.add_argument("output", type=Path)
    parser.add_argument("--source-root", type=Path, default=Path.cwd())
    args = parser.parse_args()
    print(prepare(json.loads(args.config.read_text()), args.source_root, args.output))


if __name__ == "__main__":
    main()
