#!/usr/bin/env python3
"""Exercise the pinned Claude CLI through g5d agent-turn with a scripted model.

No account credentials or network model are used. This verifies native memory
prompt loading and native tool writes, not autonomous learning quality. Usage:
  python3 scripts/tests/agent-memory-native-test.py --g5d target/debug/g5d --claude /path/to/claude
"""
import argparse
import hashlib
import http.server
import json
import os
from pathlib import Path
import subprocess
import tempfile
import threading
import uuid


def git(root, *args):
    return subprocess.check_output(["git", "-C", str(root), *args], stderr=subprocess.DEVNULL).decode().strip()


class Model(http.server.BaseHTTPRequestHandler):
    mode = "save"
    memory = None
    calls = []
    errors = []
    write_available = True
    target = "MEMORY.md"
    questions = False

    def log_message(self, *_):
        pass

    def do_POST(self):
        body = json.loads(self.rfile.read(int(self.headers["Content-Length"])))
        if "count_tokens" in self.path:
            self.send_response(200)
            self.end_headers()
            self.wfile.write(b'{"input_tokens":100}')
            return
        if "/messages" not in self.path:
            self.send_response(404)
            self.end_headers()
            return
        system = json.dumps(body.get("system", []))
        messages = body["messages"]
        Model.calls.append(system)
        try:
            assert str(Model.memory) in system, "native memory directory absent from prompt"
            names = [tool["name"] for tool in body.get("tools", [])]
            assert ("Write" in names) == Model.write_available, ("wrong native tool availability", names)
            assert ("AskUserQuestion" in names) == Model.questions, ("wrong native question availability", names)
            results = [part for msg in messages for part in (msg["content"] if isinstance(msg["content"], list) else []) if part.get("type") == "tool_result"]
            if Model.mode == "recall":
                assert "amber-otter-714" in json.dumps(body), "fresh runtime did not inject restored native memory"
                block = {"type": "text", "text": "amber-otter-714"}
                stop = "end_turn"
            elif results:
                if Model.mode == "deny":
                    assert results[-1].get("is_error"), results[-1]
                    assert not Model.memory.joinpath(Model.target).exists(), "restricted native tool wrote memory"
                else:
                    assert not results[-1].get("is_error"), results[-1]
                    assert Model.memory.joinpath(Model.target).read_text() == "Release screenshots use amber-otter-714.\n"
                block = {"type": "text", "text": "Memory saved."}
                stop = "end_turn"
            else:
                block = {"type": "tool_use", "id": "toolu_memory", "name": "Write", "input": {"file_path": str(Model.memory / Model.target), "content": "Release screenshots use amber-otter-714.\n"}}
                stop = "tool_use"
        except Exception as error:
            Model.errors.append(str(error))
            block = {"type": "text", "text": "Native memory assertion failed."}
            stop = "end_turn"
        message = {"id": "msg_memory", "type": "message", "role": "assistant", "model": body["model"], "content": [block], "stop_reason": stop, "stop_sequence": None, "usage": {"input_tokens": 100, "output_tokens": 20}}
        self.send_response(200)
        if body.get("stream"):
            self.send_header("Content-Type", "text/event-stream")
            self.end_headers()
            def event(kind, **value):
                data = json.dumps({"type": kind, **value})
                self.wfile.write(f"event: {kind}\ndata: {data}\n\n".encode())
            event("message_start", message={**message, "content": [], "stop_reason": None})
            if stop == "tool_use":
                event("content_block_start", index=0, content_block={**block, "input": {}})
                event("content_block_delta", index=0, delta={"type": "input_json_delta", "partial_json": json.dumps(block["input"])})
            else:
                event("content_block_start", index=0, content_block={"type": "text", "text": ""})
                event("content_block_delta", index=0, delta={"type": "text_delta", "text": block["text"]})
            event("content_block_stop", index=0)
            event("message_delta", delta={"stop_reason": stop, "stop_sequence": None}, usage={"output_tokens": 20})
            event("message_stop")
        else:
            self.send_header("Content-Type", "application/json")
            self.end_headers()
            self.wfile.write(json.dumps(message).encode())


def turn(g5d, claude, root, workspace, endpoint, label, tools=None, expected_status="succeeded", write_available=None, questions=False, extra_env=None):
    Model.calls = []
    Model.errors = []
    Model.questions = questions
    tools = ["Read", "Write"] if tools is None else tools
    Model.write_available = ("Write" in tools) if write_available is None else write_available
    home = root / label
    home.mkdir()
    ident = lambda: str(uuid.uuid4())
    policy = {"schema_version": "g5.agent.permission-policy/v1", "provider": "claude_code", "questions": "enabled" if questions else "disabled", "tool_approval": "prompt" if questions else "auto_deny", "sandbox": "none", "allowed_tools": tools, "limits": {"setup_timeout_ms": 30000, "max_output_bytes": 1048576}}
    prompt = "Remember the supplied project preference." if Model.mode == "save" else "What is the saved screenshot label?"
    record = {"schema_version": "g5.agent.turn.input/v1", "task": {"task_id": ident(), "attempt": 1, "session_id": ident(), "input_origin": "human", "initial_input": {"reservation_id": ident(), "input_id": ident(), "revision": 1, "delivery_generation": 1, "input_origin": "human", "system_notice_ids": []}}, "provider": {"provider_id": "claude_code", "model_id": "claude-sonnet-4-6", "auth_mode": "anthropic_api_key", "base_url": endpoint, "options": {}, "policy_ref": "sha256:" + "a" * 64, "policy_snapshot": policy}, "prompt": {"text": prompt, "sha256": hashlib.sha256(prompt.encode()).hexdigest()}, "workspace_target": {"relative_path": "code"}, "session_binding": {"session_files_state": "absent"}, "limits": {"setup_timeout_ms": 30000, "max_output_bytes": 1048576}}
    env = {key: os.environ[key] for key in ("PATH", "DYLD_LIBRARY_PATH", "DYLD_FALLBACK_LIBRARY_PATH", "LD_LIBRARY_PATH") if key in os.environ}
    env.update(HOME=str(home), G5D_WORKSPACE_ROOT=str(workspace), G5_CLAUDE_PATH=str(claude), G5D_AGENT_PERSONAL_ROOT="personal", ANTHROPIC_API_KEY="local-fixture", ANTHROPIC_BASE_URL=endpoint, CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC="1")
    env.update(extra_env or {})
    # Keep the supervisor control pipe open through native completion.
    child = subprocess.Popen([str(g5d), "agent-turn"], stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE, env=env, text=True)
    child.stdin.write(json.dumps({"input": record}) + "\n")
    child.stdin.flush()
    lines = []
    try:
        for line in child.stdout:
            lines.append(json.loads(line))
            if lines[-1].get("kind") == "outcome":
                break
        child.stdin.close()
        child.wait(timeout=40)
        assert child.returncode == 0, child.stderr.read()
        outcomes = [line for line in lines if line.get("type") == "outcome"]
        # g5d's native module protocol uses kind/payload envelopes.
        if not outcomes:
            outcomes = [line for line in lines if line.get("kind") == "outcome"]
        assert outcomes, lines
        assert outcomes[-1]["payload"]["status"] == expected_status, (outcomes[-1], child.stderr.read())
    finally:
        if child.poll() is None:
            child.kill()
        child.stdin.close()
    assert not Model.errors, Model.errors
    if expected_status == "succeeded":
        assert len(Model.calls) >= (1 if Model.mode == "recall" else 2), "native tool round trip missing"
    else:
        assert not Model.calls, "invalid mapping reached the provider"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--g5d", type=Path, required=True)
    parser.add_argument("--claude", type=Path, required=True)
    args = parser.parse_args()
    g5d, claude = args.g5d.resolve(), args.claude.resolve()
    pin = Path(__file__).resolve().parents[1].joinpath("claude-code-cli-version").read_text().strip()
    assert subprocess.check_output([str(claude), "--version"]).decode().startswith(pin + " "), "use the repository's pinned Claude version"
    server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Model)
    threading.Thread(target=server.serve_forever, daemon=True).start()
    try:
        with tempfile.TemporaryDirectory(prefix="g5-mem-", dir="/tmp") as scratch:
            root = Path(scratch).resolve()
            workspace = root / "worker-a"
            repo = workspace / "code"
            repo.mkdir(parents=True)
            git(repo, "init", "-q", "-b", "main")
            # Memory lives in the owner's personal repository, outside the
            # launch directory (g5.personal.memory).
            personal = workspace / "personal"
            personal.mkdir()
            git(personal, "init", "-q", "-b", "users/me")
            Model.memory = personal / "memory/claude"
            endpoint = f"http://127.0.0.1:{server.server_port}"
            turn(g5d, claude, root, workspace, endpoint, "home-a")
            assert not (repo / ".g5").exists() and not (repo / "memory").exists(), "memory reached the code repository"
            git(personal, "add", "memory")
            git(personal, "-c", "user.name=Memory Test", "-c", "user.email=memory@example.test", "commit", "-qm", "native memory")
            fresh = root / "worker-b"
            fresh.mkdir()
            git(fresh, "init", "-q", "-b", "main", "code")
            git(fresh, "clone", "-q", str(personal), "personal")
            Model.memory = fresh / "personal/memory/claude"
            Model.mode = "recall"
            turn(g5d, claude, root, fresh, endpoint, "home-b")
            turn(g5d, claude, root, fresh, endpoint, "home-question-conflict", tools=["Read"], questions=True, expected_status="failed")
            turn(g5d, claude, root, fresh, endpoint, "home-question-enabled", tools=["Read", "AskUserQuestion"], questions=True)
            Model.mode = "deny"
            Model.target = "restricted.md"
            turn(g5d, claude, root, fresh, endpoint, "home-restricted", tools=["Read"])

            settings = fresh / "code/.claude/settings.json"
            settings.parent.mkdir()
            settings.write_text('{"permissions":{"deny":["Write"]}}')
            Model.target = "project-denied.md"
            turn(g5d, claude, root, fresh, endpoint, "home-project-deny", write_available=False)
            assert json.loads(settings.read_text())["permissions"]["deny"] == ["Write"]

            settings.write_text('\ufeff' + json.dumps({"env":{"CLAUDE_COWORK_MEMORY_INDEX_CONTENT":"foreign-index"}}))
            turn(g5d, claude, root, fresh, endpoint, "home-index-override", expected_status="failed")
            assert "foreign-index" in settings.read_text()
            settings.write_text('{"env":{"CLAUDE_COWORK_MEMORY_INDEX_CONTENT":"foreign-index","EXAMPLE":1e400}}')
            turn(g5d, claude, root, fresh, endpoint, "home-large-number-index", expected_status="failed")
            Model.mode = "recall"
            settings.write_text('{"example":{"env":{"CLAUDE_COWORK_MEMORY_INDEX_CONTENT":"documentation"}},"env":{"EXAMPLE":1e400}}')
            turn(g5d, claude, root, fresh, endpoint, "home-settings-extension", tools=["Read"])
            settings.write_text("")
            turn(g5d, claude, root, fresh, endpoint, "home-env-index", tools=["Read"], extra_env={"CLAUDE_COWORK_MEMORY_INDEX_CONTENT":"foreign-index"})

            Model.mode = "save"
            Model.target = "MEMORY.md"
            settings.write_text("")
            turn(g5d, claude, root, fresh, endpoint, "home-empty-settings")
            assert settings.read_text() == ""

            outside = root / "redirected"
            outside.mkdir()
            settings.write_text(json.dumps({"env": {
                "CLAUDE_COWORK_MEMORY_PATH_OVERRIDE": str(outside),
                "CLAUDE_CODE_REMOTE_MEMORY_DIR": str(outside),
                "CLAUDE_MEMORY_STORES": str(outside),
                "CLAUDE_CONFIG_DIR": str(outside)
            }}))
            original = settings.read_text()
            turn(g5d, claude, root, fresh, endpoint, "home-redirect")
            assert settings.read_text() == original
            assert not list(outside.iterdir()), "native settings redirected managed state"

            if os.getuid() != 0:
                Model.memory.chmod(0o555)
                try:
                    turn(g5d, claude, root, fresh, endpoint, "home-unwritable", expected_status="failed")
                    Model.mode = "recall"
                    turn(g5d, claude, root, fresh, endpoint, "home-readonly", tools=["Read"])
                finally:
                    Model.memory.chmod(0o755)
            print(f"PASS: Claude {pin} native save, fresh-home Git recall, restricted tools, question policy, project deny, empty settings, path/index override protection and write access (scripted model).")
    finally:
        server.shutdown()


if __name__ == "__main__":
    main()
