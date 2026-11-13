#!/usr/bin/env python3
"""Verify native Claude harness loading through g5d and a terminal-style launch.

Uses the pinned CLI, isolated homes and a local scripted API; no account or model
service is used. Run with --g5d target/debug/g5d --claude /path/to/pinned/claude.
"""
import argparse
import hashlib
import http.server
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import threading
import uuid


REPO = Path(__file__).resolve().parents[2]


class Model(http.server.BaseHTTPRequestHandler):
    requests = []

    def log_message(self, *_):
        pass

    def do_POST(self):
        body = json.loads(self.rfile.read(int(self.headers["Content-Length"])))
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.end_headers()
        if "count_tokens" in self.path:
            self.wfile.write(b'{"input_tokens":100}')
            return
        Model.requests.append(body)
        self.wfile.write(json.dumps({
            "id": "msg_harness", "type": "message", "role": "assistant",
            "model": body["model"], "content": [{"type": "text", "text": "Harness loaded."}],
            "stop_reason": "end_turn", "stop_sequence": None,
            "usage": {"input_tokens": 100, "output_tokens": 10},
        }).encode())


def turn(g5d, env, expected_status="succeeded"):
    ident = lambda: str(uuid.uuid4())
    prompt = "Confirm the workspace is ready."
    policy = {"schema_version": "g5.agent.permission-policy/v1", "provider": "claude_code", "questions": "disabled", "tool_approval": "auto_deny", "sandbox": "none", "allowed_tools": [], "limits": {"wall_clock_ms": 30000, "max_output_bytes": 1048576}}
    record = {
        "schema_version": "g5.agent.turn.input/v1",
        "task": {"task_id": ident(), "attempt": 1, "session_id": ident(), "input_origin": "human", "initial_input": {"reservation_id": ident(), "input_id": ident(), "revision": 1, "delivery_generation": 1, "input_origin": "human", "system_notice_ids": []}},
        "provider": {"provider_id": "claude_code", "model_id": "claude-sonnet-4-6", "policy_ref": "sha256:" + "a" * 64, "policy_snapshot": policy},
        "prompt": {"text": prompt, "sha256": hashlib.sha256(prompt.encode()).hexdigest()},
        "workspace_target": {"relative_path": "code"},
        "session_binding": {"session_files_state": "absent"},
        "limits": {"wall_clock_ms": 30000, "max_output_bytes": 1048576},
    }
    # Keep stdin open until the turn finishes; EOF cancels the native session.
    with tempfile.TemporaryFile(mode="w+") as errors:
        child = subprocess.Popen([str(g5d), "agent-turn"], stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=errors, env=env, text=True)
        timer = threading.Timer(40, child.kill)
        timer.start()
        try:
            child.stdin.write(json.dumps({"input": record}) + "\n")
            child.stdin.flush()
            outcome = None
            for line in child.stdout:
                event = json.loads(line)
                if event.get("kind") == "outcome":
                    outcome = event["payload"]
                    break
            child.stdin.close()
            child.wait(timeout=5)
            errors.seek(0)
            assert outcome and outcome["status"] == expected_status, (outcome, errors.read())
        finally:
            timer.cancel()
            if child.poll() is None:
                child.kill()
            child.wait()
            child.stdout.close()


def assert_loaded(marker):
    assert Model.requests, "native CLI never reached the local model"
    context = json.dumps(Model.requests)
    assert marker in context, "native CLI did not load the installed user instructions"
    for skill in (REPO / ".agents/skills").iterdir():
        if skill.is_dir():
            assert skill.name in context, f"native CLI did not discover {skill.name}"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--g5d", type=Path, required=True)
    parser.add_argument("--claude", type=Path, required=True)
    args = parser.parse_args()
    g5d, claude = args.g5d.resolve(), args.claude.resolve()
    version = subprocess.check_output([str(claude), "--version"], text=True).split()[0]
    assert version == (REPO / "scripts/claude-code-cli-version").read_text().strip(), version
    server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Model)
    threading.Thread(target=server.serve_forever, daemon=True).start()
    try:
        # macOS Unix-domain socket paths cannot fit its long default temp root.
        with tempfile.TemporaryDirectory(prefix="g5h-", dir="/tmp") as tmp:
            root = Path(tmp)
            home, workspace, bundle = root / "home", root / "workspace", root / "bundle"
            home.mkdir()
            cwd = workspace / "code"
            cwd.mkdir(parents=True)
            subprocess.run(["git", "init", "-q", str(cwd)], check=True)
            shutil.copytree(REPO / ".agents/skills", bundle / ".agents/skills")
            shutil.copytree(REPO / ".agents/guidance", bundle / ".agents/guidance")
            source = (REPO / ".agents/AGENTS.md").read_bytes()
            env = {key: os.environ[key] for key in ("PATH", "DYLD_LIBRARY_PATH", "DYLD_FALLBACK_LIBRARY_PATH", "LD_LIBRARY_PATH") if key in os.environ}
            env.update(HOME=str(home), CLAUDE_CONFIG_DIR=str(home / ".claude"), G5D_WORKSPACE_ROOT=str(workspace), G5_CLAUDE_PATH=str(claude), G5_HARNESS_DIR=str(bundle), ANTHROPIC_API_KEY="local-fixture", ANTHROPIC_BASE_URL=f"http://127.0.0.1:{server.server_port}", CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC="1", CLAUDE_CODE_DISABLE_AUTO_MEMORY="1")
            env["G5D_AGENT_REPOSITORIES"] = json.dumps([{"repo_id": "app", "root_path": "code", "role": "code"}])
            for marker in ["G5_HARNESS_FIRST_8342", "G5_HARNESS_REFRESH_8342"]:
                contents = source + f"\n{marker}\n".encode()
                (bundle / "AGENTS.md").write_bytes(contents)
                Model.requests = []
                turn(g5d, env)
                assert (home / ".claude/CLAUDE.md").read_bytes() == contents
                assert_loaded(marker)
            assert not (home / ".g5/harness").exists()
            assert not (cwd / "CLAUDE.md").exists()
            # Newly enabled user settings must not replace repository memory.
            settings = home / ".claude/settings.json"
            original = '{"env":{"CLAUDE_COWORK_MEMORY_INDEX_CONTENT":"foreign"}}'
            settings.write_text(original)
            Model.requests = []
            turn(g5d, env, expected_status="failed")
            assert not Model.requests, "incompatible user settings reached the provider"
            assert settings.read_text() == original
            settings.unlink()
            # A manual launch reads the same installed harness without --add-dir.
            Model.requests = []
            subprocess.run([str(claude), "-p", "--output-format", "json", "--strict-mcp-config", "--model", "claude-sonnet-4-6", "--max-turns", "1", "--no-session-persistence", "Confirm the workspace is ready."], cwd=cwd, env=env, check=True, capture_output=True, timeout=30)
            assert_loaded("G5_HARNESS_REFRESH_8342")
        print(f"PASS: Claude {version} loads instructions and skills on initial, refreshed and terminal launches")
    finally:
        server.shutdown()
        server.server_close()


if __name__ == "__main__":
    main()
