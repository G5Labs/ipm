#!/usr/bin/env python3
"""Run the actual Codex seed and printed curl against a live isolated TestApp."""

import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import urllib.request


source = Path(__file__).resolve().parents[3]
api_url = os.environ["SEED_TEST_API"]


def run(args, **kwargs):
    result = subprocess.run(args, text=True, capture_output=True, timeout=90, **kwargs)
    if result.returncode:
        raise AssertionError(f"{args[0]} failed:\n{result.stdout}\n{result.stderr}")
    return result.stdout


with tempfile.TemporaryDirectory(prefix="g5-codex-seed-") as directory:
    root = Path(directory)
    scripts = root / "scripts"
    scripts.mkdir()
    for name in ["seed-codex.sh", "seed-workspace-model.sh"]:
        shutil.copyfile(source / "scripts" / name, scripts / name)
    tools = root / "bin"
    tools.mkdir()
    docker = tools / "docker"
    docker.write_text(
        f"#!{sys.executable}\n" + Path(__file__).with_name("docker.py").read_text()
    )
    docker.chmod(0o755)
    upstream = root / "upstream"
    (upstream / ".semlift/specs").mkdir(parents=True)
    (upstream / ".semlift/specs/root.semlift.md").write_text(
        "---\ncomponent_id: codex-rs_project_aggregate\ntitle: Codex\n---\n"
    )
    for crate in ["core", "cli", "tui", "common", "exec"]:
        (upstream / crate / "src").mkdir(parents=True)
        (upstream / crate / "src/lib.rs").write_text("pub fn tools() {}\n")
    env = {
        **os.environ,
        "PATH": f"{tools}:{os.environ['PATH']}",
        "SEED_TEST_SQL": str(root / "ontology.sql"),
        "GIT_AUTHOR_NAME": "Seed test",
        "GIT_AUTHOR_EMAIL": "seed@test.local",
        "GIT_COMMITTER_NAME": "Seed test",
        "GIT_COMMITTER_EMAIL": "seed@test.local",
    }
    run(["git", "init", "-q", "-b", "main", str(upstream)], env=env)
    run(["git", "-C", str(upstream), "add", "."], env=env)
    run(["git", "-C", str(upstream), "commit", "-qm", "Seed upstream"], env=env)

    token_request = urllib.request.Request(
        api_url + "/api/v1/auth/test-token",
        data=json.dumps({"email": "demo@g5labs.ai"}).encode(),
        headers={"Content-Type": "application/json"},
    )
    with urllib.request.urlopen(token_request, timeout=10) as response:
        env["TOKEN"] = json.load(response)["data"]["jwt"]

    outputs = []
    # Fresh setup without the optional archive, then a rerun that adds the
    # instruction, then another rerun proving existing docs are retained.
    for attempt in range(3):
        if attempt == 1:
            instructions = root / "archived_aux_files_readonly/demo_plan"
            instructions.mkdir(parents=True)
            (instructions / "tool_system_refactor.md").write_text(
                "# Tool refactor\nPreserve this instruction content.\n"
            )
        printed = run(
            ["bash", str(scripts / "seed-codex.sh"), api_url, str(upstream)], env=env
        )
        # Read the instruction through the real, repository-qualified Docs API;
        # no cache row represents a file on the contracted schema.
        def scalar(sql):
            query = urllib.request.Request(
                env["SEED_TEST_DB"], data=sql.encode(),
                headers={"Content-Type": "text/plain"},
            )
            with urllib.request.urlopen(query, timeout=10) as response:
                return response.read().decode().strip()

        workspace_id = scalar("SELECT id::text FROM workspaces WHERE name='codex'")
        docs_repo_id = scalar(
            "SELECT m.repo_id::text FROM workspace_repos m "
            "JOIN workspaces w ON w.id=m.workspace_id WHERE w.name='codex' AND m.role='docs'"
        )
        query = urllib.request.Request(
            f"{api_url}/api/v1/workspaces/{workspace_id}/docs",
            headers={"Authorization": f"Bearer {env['TOKEN']}"},
        )
        with urllib.request.urlopen(query, timeout=10) as response:
            documents = json.load(response)["data"]["docs"]
        instructions = [doc for doc in documents if doc["repoId"] == docs_repo_id
                        and doc["filePath"] == "docs/tool_system_refactor.md"]
        assert len(instructions) == (0 if attempt == 0 else 1), (
            f"phantom or missing instruction: run={attempt}, docs={instructions}"
        )
        command = "curl -X POST" + printed.split("curl -X POST", 1)[1].split("\n\n", 1)[0]
        result = json.loads(run(["bash", "-eu", "-c", command], env=env))
        assert result.get("data", {}).get("taskId"), result
        assert result["data"].get("session", {}).get("id"), result
        if attempt == 0:
            # Keep the old preservation assertion independent of agent execution:
            # seed an existing explainer before rerunning workspace provisioning.
            query = urllib.request.Request(
                env["SEED_TEST_DB"],
                data=(b"INSERT INTO docs (org_id, workspace_id, repo_id, name, doc_type, file_path, content) "
                      b"SELECT w.org_id, w.id, m.repo_id, 'Retained explainer', 'explainer', "
                      b"'docs/' || w.id || '/explainers/retained.html', '<html>Retained</html>' "
                      b"FROM workspaces w JOIN workspace_repos m ON m.workspace_id=w.id "
                      b"WHERE w.name='codex' AND m.role='docs'"),
                headers={"Content-Type": "text/plain"},
            )
            with urllib.request.urlopen(query, timeout=10) as response:
                response.read()
        outputs.append(result["data"])
    # A vanished lookup must stop before Step 3 replaces the local workspace.
    workspace = root / "codex-workspace"
    marker = workspace / "keep-local-work.txt"
    marker.write_text("local work survives failed lookup")
    manifest = (workspace / ".g5/workspace.yaml").read_bytes()
    for query, diagnostic in [
        ("SELECT name FROM workspaces", "Cannot resolve Codex workspace name"),
        ("SELECT root_path FROM workspace_repos", "Cannot resolve Codex docs root path"),
    ]:
        result = subprocess.run(
            ["bash", str(scripts / "seed-codex.sh"), api_url, str(upstream)],
            env={**env, "SEED_TEST_EMPTY_QUERY": query},
            text=True, capture_output=True, timeout=90,
        )
        assert result.returncode != 0, result.stdout
        assert diagnostic in result.stderr, result.stderr
        assert "=== Step 3:" not in result.stdout, result.stdout
        assert marker.read_text() == "local work survives failed lookup"
        assert (workspace / ".g5/workspace.yaml").read_bytes() == manifest
    print(json.dumps({"requests": outputs, "manifest": (root / "codex-workspace/.g5/workspace.yaml").read_text()}))
