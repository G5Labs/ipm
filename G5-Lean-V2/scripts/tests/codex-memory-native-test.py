#!/usr/bin/env python3
"""Check G5's Codex memory binding with the pinned CLI and a local scripted model.

Exercises native tool writes, Git push/clone, fresh-home recall, and read-only
enforcement through G5's App Server transport. No account credentials are used. This
checks memory storage/use, not the quality or timing of background extraction.
Usage: python3 scripts/tests/codex-memory-native-test.py --g5d target/debug/g5d --codex /path/to/codex
"""
import argparse
import hashlib
import http.server
import json
import os
from pathlib import Path
import queue
import runpy
import shlex
import subprocess
import tempfile
import threading
import time
import uuid


REPO = Path(__file__).resolve().parents[2]
FIXTURE = runpy.run_path(str(REPO / "scripts/test-native-provider-transport.py"))
MARKER = "amber-otter-codex-924"
SUMMARY = f"v1\n\nRelease screenshots use {MARKER}.\n"


def git(root, *args):
    return subprocess.check_output(
        ["git", "-C", str(root), *args], stderr=subprocess.PIPE, text=True,
    ).strip()


def tool_response(model, command):
    item = {
        "type": "function_call", "id": "call_memory", "call_id": "call_memory",
        "name": "exec_command", "namespace": "functions",
        "arguments": json.dumps({"cmd": command}),
    }
    response = {
        "id": "resp_memory", "object": "response", "created_at": 1,
        "model": model, "status": "completed", "output": [item],
        "usage": {"input_tokens": 11, "output_tokens": 3, "total_tokens": 14},
    }
    return FIXTURE["sse"]([
        {"type": "response.created", "response": {**response, "status": "in_progress", "output": []}},
        {"type": "response.output_item.added", "output_index": 0, "item": {**item, "arguments": ""}},
        {"type": "response.function_call_arguments.delta", "item_id": item["id"], "output_index": 0, "delta": item["arguments"]},
        {"type": "response.output_item.done", "output_index": 0, "item": item},
        {"type": "response.completed", "response": response},
    ])


class Model(http.server.BaseHTTPRequestHandler):
    def log_message(self, *_):
        pass

    def do_CONNECT(self):
        self.send_error(502)

    def do_GET(self):
        self.send_error(404)

    def do_POST(self):
        body = json.loads(self.rfile.read(int(self.headers["Content-Length"])))
        if not self.path.endswith("/responses"):
            self.send_error(404)
            return
        self.server.requests.append(body)
        payload = FIXTURE["codex_response"](body["model"])
        try:
            mode, memory = self.server.mode, self.server.memory
            if mode != "save":
                assert MARKER in json.dumps(body), "native memory was not injected"
            if mode in ("save", "deny") and len(self.server.requests) == 1:
                if mode == "save":
                    # Only the memory folder may be writable outside cwd.
                    script = (
                        "from pathlib import Path\nimport errno\n"
                        f"memory = Path({str(memory)!r})\n"
                        f"(memory / 'memory_summary.md').write_text({SUMMARY!r})\n"
                        f"(memory / 'MEMORY.md').write_text({SUMMARY!r})\n"
                        "try:\n"
                        "    (memory.parent.parent / 'unrelated.md').write_text('forbidden')\n"
                        "except OSError as error:\n"
                        "    assert error.errno in (errno.EACCES, errno.EPERM, errno.EROFS), error\n"
                        "else:\n"
                        "    raise AssertionError('personal notes became writable')\n"
                    )
                else:
                    script = f"from pathlib import Path; Path({str(memory / 'denied.md')!r}).write_text('forbidden')"
                payload = tool_response(body["model"], "python3 -c " + shlex.quote(script))
            elif mode in ("save", "deny"):
                outputs = [item["output"] for item in body["input"] if item.get("type") == "function_call_output"]
                assert outputs, "native tool result missing"
                if mode == "save":
                    assert "Process exited with code 0" in outputs[-1], outputs[-1]
                    assert (memory / "memory_summary.md").read_text() == SUMMARY
                    assert not (memory.parent.parent / "unrelated.md").exists()
                else:
                    assert "Process exited with code 1" in outputs[-1], outputs[-1]
                    assert not (memory / "denied.md").exists()
        except Exception as error:
            self.server.errors.append(str(error))
        self.send_response(200)
        self.send_header("Content-Type", "text/event-stream")
        self.send_header("Content-Length", str(len(payload)))
        self.end_headers()
        self.wfile.write(payload)


def turn(options, server, root, home, transport, mode):
    server.requests, server.errors, server.mode = [], [], mode
    server.memory = root / "personal/memory/codex"
    home.mkdir(exist_ok=True)
    codex_home = home / ".codex"
    codex_home.mkdir(exist_ok=True)
    # The admitted binding must override inherited native opt-outs.
    (codex_home / "config.toml").write_text(
        "[features]\nmemories=false\n[memories]\nuse_memories=false\ngenerate_memories=false\n"
    )
    harness = home / "harness"
    harness.mkdir(exist_ok=True)
    (harness / "AGENTS.md").write_text("Use the native memory folder for lessons.\n")
    skill = harness / ".agents/skills/fixture"
    skill.mkdir(parents=True, exist_ok=True)
    (skill / "SKILL.md").write_text("---\nname: fixture\ndescription: Test fixture\n---\nNo additional instructions.\n")
    limits = {"setup_timeout_ms": 30000, "max_output_bytes": 1048576}
    policy = {
        "schema_version": "g5.agent.permission-policy/v1", "provider": "codex",
        "questions": "disabled", "tool_approval": "auto_deny",
        "sandbox": "read-only" if mode == "deny" else "workspace-write",
        "allowed_tools": [], "limits": limits,
    }
    ident = lambda: str(uuid.uuid4())
    prompt = "Remember the screenshot preference." if mode == "save" else "Use the saved screenshot preference."
    endpoint = f"http://127.0.0.1:{server.server_port}"
    record = {
        "schema_version": "g5.agent.turn.input/v1",
        "task": {
            "task_id": ident(), "session_id": ident(), "attempt": 1, "input_origin": "human",
            "initial_input": {"reservation_id": ident(), "input_id": ident(), "revision": 1,
                              "delivery_generation": 1, "input_origin": "human", "system_notice_ids": []},
        },
        "provider": {
            "provider_id": "codex", "model_id": "gpt-5.6-sol", "auth_mode": "open_ai_api_key",
            "base_url": endpoint + "/v1", "policy_ref": "fixture", "policy_snapshot": policy,
            "options": {"codex_transport": "app_server_stdio_v1"},
        },
        "prompt": {"text": prompt, "sha256": hashlib.sha256(prompt.encode()).hexdigest()},
        "workspace_target": {"relative_path": "code"},
        "session_binding": {"session_files_state": "absent"}, "limits": limits,
    }
    env = {key: os.environ[key] for key in (
        "PATH", "LANG", "LD_LIBRARY_PATH", "DYLD_LIBRARY_PATH", "DYLD_FALLBACK_LIBRARY_PATH",
    ) if key in os.environ}
    env.update(
        HOME=str(home), G5D_WORKSPACE_ROOT=str(root), G5D_AGENT_PERSONAL_ROOT="personal",
        G5_HARNESS_DIR=str(harness), G5_CODEX_PATH=str(options.codex), OPENAI_API_KEY="synthetic-fixture",
        HTTP_PROXY=endpoint, HTTPS_PROXY=endpoint, ALL_PROXY=endpoint, NO_PROXY="127.0.0.1,localhost",
    )
    incoming = queue.Queue()
    frames = []
    with tempfile.TemporaryFile(mode="w+t") as stderr:
        process = subprocess.Popen(
            [str(options.g5d), "agent-turn"], cwd=root, env=env, text=True,
            stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=stderr,
        )
        def read():
            for line in process.stdout:
                incoming.put(json.loads(line))
            incoming.put(None)
        threading.Thread(target=read, daemon=True).start()
        try:
            process.stdin.write(json.dumps({"input": record}) + "\n")
            process.stdin.flush()
            deadline = time.monotonic() + 60
            while True:
                frame = incoming.get(timeout=max(.01, deadline - time.monotonic()))
                assert frame is not None, "supervisor exited without outcome"
                frames.append(frame)
                if frame.get("kind") == "outcome":
                    assert frame["payload"]["status"] == "succeeded", frames
                    break
            process.stdin.close()
            assert process.wait(timeout=10) == 0
            assert not server.errors, server.errors
            assert len(server.requests) >= (1 if mode == "recall" else 2)
            assert (codex_home / "memories").is_symlink()
            assert (codex_home / "memories").resolve() == server.memory
            assert (codex_home / "sessions").is_dir(), "native sessions must stay at home"
        except Exception:
            stderr.seek(0)
            print(stderr.read()[-4000:])
            raise
        finally:
            if process.poll() is None:
                process.kill()
            process.wait(timeout=10)
            process.stdin.close()
            process.stdout.close()
    print(f"PASS {transport}: {mode}", flush=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--g5d", type=Path, required=True)
    parser.add_argument("--codex", type=Path, required=True)
    options = parser.parse_args()
    options.g5d, options.codex = options.g5d.resolve(), options.codex.resolve()
    # /tmp is writable in native sandbox modes; use a different parent so the
    # memory-root permission and unrelated-personal-file refusal are exercised.
    with tempfile.TemporaryDirectory(prefix="codex-memory-native-", dir=REPO) as scratch:
        scratch = Path(scratch)
        server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Model)
        threading.Thread(target=server.serve_forever, daemon=True).start()
        try:
            for transport in ("app-server",):  # G5's only Codex transport
                root = scratch / transport
                (root / "code").mkdir(parents=True)
                personal = root / "personal"
                personal.mkdir()
                git(personal, "init", "-q", "-b", "personal")
                git(personal, "config", "user.name", "Memory fixture")
                git(personal, "config", "user.email", "memory@example.invalid")
                turn(options, server, root, root / "home", transport, "save")
                git(personal, "add", "memory")
                git(personal, "commit", "-qm", "Save native memory")
                remote = root / "remote.git"
                git(root, "init", "--bare", "-q", str(remote))
                git(personal, "push", "-q", str(remote), "personal")
                fresh = root / "fresh"
                (fresh / "code").mkdir(parents=True)
                git(fresh, "clone", "-q", "-b", "personal", str(remote), "personal")
                tracked = git(fresh / "personal", "ls-files").splitlines()
                assert sorted(tracked) == ["memory/codex/MEMORY.md", "memory/codex/memory_summary.md"], tracked
                turn(options, server, fresh, fresh / "home", transport, "recall")
                turn(options, server, fresh, fresh / "home", transport, "deny")
        finally:
            server.shutdown()
            server.server_close()


if __name__ == "__main__":
    main()
