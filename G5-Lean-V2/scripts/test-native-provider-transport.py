#!/usr/bin/env python3
"""Exercise agent-turn with pinned native CLIs and synthetic local providers.

Requires a built g5d and the pinned Claude/Codex binaries. No provider account or
Internet access is needed: all credentials are synthetic, homes are isolated,
and the local HTTP server refuses external proxy requests. This checks the real
launch path, request authentication, endpoint prefixes, streaming and usage.
"""

import argparse
import base64
from datetime import datetime, timezone
import hashlib
import http.server
import json
import os
from pathlib import Path
import queue
import signal
import subprocess
import tempfile
import threading
import time


SECRET = "g5-native-transport-fixture-secret"
REPLY = "Direct provider response"


def sse(events):
    return "".join(
        f"event: {event['type']}\ndata: {json.dumps(event)}\n\n" for event in events
    ).encode()


def claude_response(model):
    message = {
        "id": "msg_fixture",
        "type": "message",
        "role": "assistant",
        "model": model,
        "content": [],
        "stop_reason": None,
        "stop_sequence": None,
        "usage": {"input_tokens": 11, "output_tokens": 0},
    }
    return sse([
        {"type": "message_start", "message": message},
        {"type": "content_block_start", "index": 0, "content_block": {"type": "text", "text": ""}},
        {"type": "content_block_delta", "index": 0, "delta": {"type": "text_delta", "text": REPLY}},
        {"type": "content_block_stop", "index": 0},
        {"type": "message_delta", "delta": {"stop_reason": "end_turn", "stop_sequence": None}, "usage": {"output_tokens": 3}},
        {"type": "message_stop"},
    ])


def codex_response(model):
    item = {"id": "msg_fixture", "type": "message", "role": "assistant", "status": "completed", "content": [{"type": "output_text", "text": REPLY, "annotations": []}]}
    response = {"id": "resp_fixture", "object": "response", "created_at": 1, "model": model, "status": "completed", "output": [item], "usage": {"input_tokens": 11, "output_tokens": 3, "total_tokens": 14, "input_tokens_details": {"cached_tokens": 0}}}
    return sse([
        {"type": "response.created", "response": {**response, "status": "in_progress", "output": []}},
        {"type": "response.output_item.added", "output_index": 0, "item": {**item, "status": "in_progress", "content": []}},
        {"type": "response.content_part.added", "item_id": item["id"], "output_index": 0, "content_index": 0, "part": {"type": "output_text", "text": "", "annotations": []}},
        {"type": "response.output_text.delta", "item_id": item["id"], "output_index": 0, "content_index": 0, "delta": REPLY},
        {"type": "response.output_item.done", "output_index": 0, "item": item},
        {"type": "response.completed", "response": response},
    ])


def codex_tool_response(model):
    item = {"id": "call_item_fixture", "type": "function_call", "call_id": "call_fixture",
            "name": "exec_command", "arguments": json.dumps({"cmd": "printf fixture-tool-output"})}
    response = {"id": "resp_tool_fixture", "object": "response", "created_at": 1,
                "model": model, "status": "completed", "output": [item],
                "usage": {"input_tokens": 11, "output_tokens": 3, "total_tokens": 14,
                          "input_tokens_details": {"cached_tokens": 0}}}
    return sse([
        {"type": "response.created", "response": {**response, "status": "in_progress", "output": []}},
        {"type": "response.output_item.added", "output_index": 0, "item": {**item, "arguments": ""}},
        {"type": "response.function_call_arguments.delta", "item_id": item["id"], "output_index": 0, "delta": item["arguments"]},
        {"type": "response.output_item.done", "output_index": 0, "item": item},
        {"type": "response.completed", "response": response},
    ])


def has_cache(value):
    if isinstance(value, dict):
        return "cache_control" in value or any(has_cache(item) for item in value.values())
    return isinstance(value, list) and any(has_cache(item) for item in value)


class Provider(http.server.BaseHTTPRequestHandler):
    def log_message(self, *_):
        pass

    def do_CONNECT(self):
        self.send_error(502)

    def do_GET(self):
        self.send_error(404)

    def do_POST(self):
        body = json.loads(self.rfile.read(int(self.headers.get("Content-Length", "0"))))
        path = self.path.split("?")[0]
        self.server.requests.append((path, dict(self.headers), body))
        if path.endswith("/messages"):
            payload = claude_response(body["model"])
        elif path.endswith("/responses"):
            payload = (codex_tool_response(body["model"])
                       if getattr(self.server, "tool_turn", False) and len(self.server.requests) == 1
                       else codex_response(body["model"]))
        else:
            self.send_error(404)
            return
        self.send_response(200)
        self.send_header("Content-Type", "text/event-stream")
        self.send_header("Content-Length", str(len(payload)))
        self.end_headers()
        self.wfile.write(payload)


def run_case(args, provider, auth, credential, model, effort="high"):
    server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Provider)
    server.requests = []
    server.tool_turn = args.codex_tool_turn and provider == "codex"
    threading.Thread(target=server.serve_forever, daemon=True).start()
    process = None
    try:
        with tempfile.TemporaryDirectory(prefix="g5-native-transport-") as scratch:
            root = Path(scratch)
            home = root / "home"
            home.mkdir()
            harness = root / "harness"
            skill = harness / ".agents/skills/fixture"
            skill.mkdir(parents=True)
            (harness / "AGENTS.md").write_text("Reply to the request.\n")
            (skill / "SKILL.md").write_text("---\nname: fixture\ndescription: Test fixture\n---\nNo additional instructions.\n")
            codex_home = home / ".codex"
            codex_home.mkdir()
            # A stale provider and saved key must never replace the selection.
            (codex_home / "config.toml").write_text('model_provider="stale"\nmodel_reasoning_effort="low"\n[model_providers.stale]\nname="stale"\nbase_url="http://127.0.0.1:1"\nenv_key="STALE_KEY"\nwire_api="responses"\n')
            claims = {"sub": "fixture-user", "email": "fixture@example.invalid", "exp": 4102444800, "https://api.openai.com/auth": {"chatgpt_account_id": "fixture-account", "chatgpt_plan_type": "plus"}}
            payload = base64.urlsafe_b64encode(json.dumps(claims).encode()).decode().rstrip("=")
            fixture_token = "eyJhbGciOiJSUzI1NiJ9." + payload + ".fixture"
            cached_auth = json.dumps({"auth_mode": "chatgpt", "tokens": {"id_token": fixture_token, "access_token": fixture_token, "refresh_token": "fixture-refresh", "account_id": "fixture-account"}, "last_refresh": datetime.now(timezone.utc).isoformat()}).encode()
            (codex_home / "auth.json").write_bytes(cached_auth)
            base = f"http://127.0.0.1:{server.server_port}"
            endpoint = base + ("/custom" if provider == "claude_code" else "/custom/v1")
            env = {
                "PATH": "/opt/g5/bin:/usr/local/bin:/usr/bin:/bin",
                "HOME": str(home), "TMPDIR": str(root), "LANG": "C.UTF-8", "CI": "1",
                "G5D_WORKSPACE_ROOT": str(home), "G5_HARNESS_DIR": str(harness),
                "G5_CLAUDE_PATH": args.claude, "G5_CODEX_PATH": args.codex,
                "CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC": "1", "DISABLE_AUTOUPDATER": "1",
                "HTTP_PROXY": base, "HTTPS_PROXY": base, "ALL_PROXY": base,
                "NO_PROXY": "127.0.0.1,localhost",
                "ANTHROPIC_BASE_URL": "http://127.0.0.1:1", "OPENAI_BASE_URL": "http://127.0.0.1:1",
                "ANTHROPIC_API_KEY": "stale-key", "ANTHROPIC_AUTH_TOKEN": "stale-bearer",
                "CLAUDE_CODE_OAUTH_TOKEN": "stale-oauth", "ANTHROPIC_UNIX_SOCKET": "/missing/stale.sock",
                "OPENAI_API_KEY": "stale-openai", "OPENROUTER_API_KEY": "stale-router",
                "CODEX_API_KEY": "stale-codex", "STALE_KEY": "stale-other-provider",
                credential: SECRET, "G5D_SECRET_ENV_NAMES": credential,
            }
            if provider == "codex":
                login_env = {key: env[key] for key in ["PATH", "HOME", "HTTP_PROXY", "HTTPS_PROXY", "NO_PROXY"]}
                login_env["CODEX_HOME"] = str(codex_home)
                status = subprocess.run([args.codex, "login", "status"], env=login_env, capture_output=True, text=True, timeout=10)
                assert status.returncode == 0 and "ChatGPT" in status.stdout + status.stderr, "fixture must be recognized as cached ChatGPT login"
            limits = {"setup_timeout_ms": 30000, "max_output_bytes": 1048576}
            policy = {"schema_version": "g5.agent.permission-policy/v1", "provider": provider, "questions": "disabled", "tool_approval": "auto_deny", "sandbox": "none" if provider == "claude_code" else "read-only", "allowed_tools": [], "limits": limits}
            prompt = "Reply with a short greeting."
            record = {"input": {
                "schema_version": "g5.agent.turn.input/v1",
                "task": {"task_id": "task-fixture", "attempt": 1, "session_id": "session-fixture", "input_origin": "human", "initial_input": {"reservation_id": "r1", "input_id": "i1", "revision": 1, "delivery_generation": 1, "input_origin": "human", "system_notice_ids": []}},
                "provider": {"provider_id": provider, "auth_mode": auth, "base_url": endpoint, "model_id": model, "policy_ref": "fixture", "policy_snapshot": policy, "options": {"session_title": provider == "claude_code"}},
                "prompt": {"text": prompt, "sha256": hashlib.sha256(prompt.encode()).hexdigest()},
                "workspace_target": {}, "session_binding": {"session_files_state": "absent"}, "limits": limits,
            }}
            record["input"]["provider"]["options"]["effort"] = effort
            if provider == "codex":
                # G5 launches Codex only over App Server.
                record["input"]["provider"]["options"]["codex_transport"] = "app_server_stdio_v1"
            events = queue.Queue()
            with tempfile.TemporaryFile() as stderr:
                process = subprocess.Popen([args.g5d, "agent-turn"], env=env, cwd=home, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=stderr, text=True, start_new_session=True)
                threading.Thread(target=lambda: [events.put(line) for line in process.stdout], daemon=True).start()
                process.stdin.write(json.dumps(record) + "\n")
                process.stdin.flush()
                frames = []
                deadline = time.monotonic() + 45
                outcome = None
                while time.monotonic() < deadline:
                    try:
                        frame = json.loads(events.get(timeout=0.2))
                    except queue.Empty:
                        if process.poll() is not None:
                            break
                        continue
                    frames.append(frame)
                    if frame.get("kind") == "outcome":
                        outcome = frame
                        break
                if outcome is None:
                    stderr.seek(0)
                    raise AssertionError(f"{auth}: no outcome; events={frames}; stderr={stderr.read().decode(errors='replace')[-2000:]}")
                assert SECRET not in json.dumps(frames), "credential leaked into module events"
                result = outcome.get("payload", outcome.get("result", outcome))
                assert result["status"] == "succeeded", (auth, outcome, frames)
                assert result["usage"]["input_tokens"] >= 11, (auth, outcome)
                assert result["usage"]["output_tokens"] >= 3, (auth, outcome)
                assert any(frame.get("kind") == "message.final" for frame in frames), frames
                requests = [(path, {k.lower(): v for k,v in headers.items()}, body) for path,headers,body in server.requests]
                expected_path = "/custom/v1/messages" if provider == "claude_code" else "/custom/v1/responses"
                assert requests and all(path == expected_path for path,_,_ in requests), requests
                for _, headers, body in requests:
                    if auth == "anthropic_api_key":
                        assert headers.get("x-api-key") == SECRET
                        assert "authorization" not in headers
                    else:
                        assert headers.get("authorization") == "Bearer " + SECRET
                    if provider == "codex":
                        assert body.get("reasoning", {}).get("effort") == effort, "frozen effort did not override inherited configuration"
                    if provider == "claude_code":
                        assert has_cache(body), "native Claude cache markers missing"
                    if auth == "anthropic_oauth":
                        assert "oauth" in headers.get("anthropic-beta", "")
                if server.tool_turn:
                    assert len(requests) == 2, "native tool did not produce a second model call"
                    assert result["usage"]["input_tokens"] == 22, result
                    assert result["usage"]["output_tokens"] == 6, result
                    assert any(frame.get("kind") == "tool.completed" for frame in frames), frames
                if provider == "claude_code":
                    assert len(requests) >= 2, "title generation did not use the direct endpoint"
                    # The completed task's first call carries its frozen effort.
                    # The later title prompt also quotes the user prompt, but has
                    # an independent effort policy and must not satisfy this check.
                    main_request = requests[0][2]
                    assert main_request["model"] == model, (model, main_request["model"])
                    assert main_request.get("output_config", {}).get("effort") == (None if effort == "none" else effort), (
                        model, "native Claude silently omitted/clamped the frozen effort")
                assert (codex_home / "auth.json").read_bytes() == cached_auth, "cached native login was changed"
                assert SECRET not in (codex_home / "config.toml").read_text(), "key persisted in provider config"
                print(f"PASS {provider}/{auth}/{'app-server' if provider == 'codex' else 'stream-json'}: {len(requests)} direct requests, streaming and usage", flush=True)
    finally:
        if process is not None:
            if process.poll() is None:
                os.killpg(process.pid, signal.SIGTERM)
                try:
                    process.wait(timeout=3)
                except subprocess.TimeoutExpired:
                    os.killpg(process.pid, signal.SIGKILL)
                    process.wait()
        server.shutdown()
        server.server_close()


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--g5d", default=str(Path(__file__).resolve().parents[1] / "target/debug/g5d"))
    parser.add_argument("--claude", default="/opt/g5/bin/claude")
    parser.add_argument("--codex", default="/opt/g5/bin/codex")
    parser.add_argument("--auth", help="Run only this authentication mode")
    parser.add_argument("--codex-tool-turn", action="store_true", help="Exercise multiple native model calls and cumulative usage in one tool turn")
    args = parser.parse_args()
    cases = [
        ("claude_code", "anthropic_api_key", "ANTHROPIC_API_KEY", "claude-sonnet-4-6"),
        ("claude_code", "anthropic_bearer", "ANTHROPIC_AUTH_TOKEN", "claude-sonnet-4-6"),
        ("claude_code", "anthropic_oauth", "CLAUDE_CODE_OAUTH_TOKEN", "claude-sonnet-4-6"),
        ("claude_code", "open_router_api_key", "ANTHROPIC_AUTH_TOKEN", "anthropic/claude-opus-4.8"),
        ("claude_code", "anthropic_api_key", "ANTHROPIC_API_KEY", "claude-haiku-4-5", "none"),
        ("claude_code", "open_router_api_key", "ANTHROPIC_AUTH_TOKEN", "anthropic/claude-haiku-4.5", "none"),
        ("codex", "open_ai_api_key", "OPENAI_API_KEY", "gpt-5.4"),
        ("codex", "open_router_api_key", "OPENROUTER_API_KEY", "openai/gpt-5.4"),
    ]
    for case in cases:
        if args.auth is None or args.auth == case[1]:
            run_case(args, *case)
