#!/usr/bin/env python3
"""Verify the pinned stable protocol, effective features and native continuation.

By default inference uses a synthetic local Responses API. Passing an existing
owner-native subscription auth home additionally exercises authenticated upstream
turns. Credentials are copied only into private temporary homes and never logged.
"""
import argparse
import http.server
import json
import os
from pathlib import Path
import queue
import runpy
import shutil
import subprocess
import tempfile
import threading
import time

REPO = Path(__file__).resolve().parents[1]
FIXTURES = REPO / "tests/fixtures/codex-app-server"
# A rollout the retired managed `codex exec` launch wrote at the 0.164.0 pin
# (originator codex_exec). Historical chats continue from it under App Server.
EXEC_HISTORY = FIXTURES / "exec-history"
EXEC_HISTORY_FACT = "exec-history-fact-910"
MODELS = [model["id"] for model in json.loads(
    (REPO / "tests/fixtures/native-provider-catalog.json").read_text()
)["providers"]["codex"]["models"] if model["route"] == "direct"]


def verify_configuration(cli, disabled):
    version = (REPO / "scripts/codex-cli-version").read_text().strip()
    assert subprocess.check_output([cli, "--version"], text=True).strip() == f"codex-cli {version}"
    with tempfile.TemporaryDirectory() as generated:
        subprocess.run([cli, "app-server", "generate-json-schema", "--out", generated], check=True)
        assert (Path(generated) / "codex_app_server_protocol.schemas.json").read_bytes() == (
            FIXTURES / "protocol.schemas.json"
        ).read_bytes(), "stable schema drift"
    with tempfile.TemporaryDirectory() as hostile:
        # Every non-stable inherited setting is deliberately enabled here.
        (Path(hostile) / "config.toml").write_text(
            "[features]\n" + "\n".join(f"{feature}=true" for feature in disabled) + "\n"
        )
        overrides = [part for feature in disabled for part in ("-c", f"features.{feature}=false")]
        effective = subprocess.check_output(
            [cli, *overrides, "features", "list"],
            env=dict(os.environ, HOME=hostile, CODEX_HOME=hostile), text=True,
        )
        for line in effective.splitlines():
            columns = line.split()
            if " ".join(columns[1:-1]) in ("experimental", "under development"):
                assert columns[-1] == "false", line
    print(f"PASS codex-cli {version}: stable schema and hostile inherited feature overrides", flush=True)


def attempt(cli_args, home, auth_home, resume=None):
    env = {key: os.environ[key] for key in ("PATH", "LANG") if key in os.environ}
    env.update(HOME=str(home), CODEX_HOME=str(home), NO_PROXY="127.0.0.1")
    if auth_home:
        auth = home / "auth.json"
        if not auth.exists():
            auth.write_bytes((auth_home / "auth.json").read_bytes())
            auth.chmod(0o600)
    else:
        env["CODEX_API_KEY"] = "synthetic-fixture"
    process = subprocess.Popen(
        cli_args, cwd=home, env=env, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
        stderr=subprocess.DEVNULL, text=True,
    )
    incoming = queue.Queue()
    observed = []

    def read_frames():
        try:
            for line in process.stdout:
                incoming.put(json.loads(line))
        finally:
            incoming.put(None)

    threading.Thread(target=read_frames, daemon=True).start()

    def send(frame):
        process.stdin.write(json.dumps(frame) + "\n")
        process.stdin.flush()

    def until(predicate):
        deadline = time.monotonic() + 60
        while time.monotonic() < deadline:
            frame = incoming.get(timeout=max(.01, deadline - time.monotonic()))
            assert frame is not None, "native process exited before expected evidence"
            observed.append(frame)
            if predicate(frame):
                return frame
        raise AssertionError("native probe deadline exceeded")

    try:
        send({"id": 1, "method": "initialize", "params": {
            "clientInfo": {"name": "g5_agent", "version": "1"},
            "capabilities": {"experimentalApi": False},
        }})
        assert "result" in until(lambda frame: frame.get("id") == 1)
        send({"method": "initialized"})
        # The exact pin rejects invalid request/thread identities before any
        # model input can be accepted. Internal errors remain ambiguous in G5.
        missing = "00000000-0000-4000-8000-000000000809"
        send({"id": 8, "method": "thread/resume", "params": {"threadId": missing}})
        refused = until(lambda frame: frame.get("id") == 8)
        assert refused["error"]["code"] in (-32600, -32602)
        send({"id": 9, "method": "turn/start", "params": {
            "threadId": missing, "input": [{"type": "text", "text": "must not be accepted"}],
        }})
        refused = until(lambda frame: frame.get("id") == 9)
        assert refused["error"]["code"] in (-32600, -32602)
        send({"id": 4, "method": "model/list", "params": {}})
        models = until(lambda frame: frame.get("id") == 4)["result"]["data"]
        advertised = [model["model"] for model in models]
        if auth_home:
            # Upstream can filter an account's subscription list independently
            # of the bundled/custom-provider catalog. Preserve API-route ids.
            assert "gpt-5.6-sol" in advertised and set(advertised) <= set(MODELS)
        else:
            assert advertised == MODELS
        print("MODELS", advertised, flush=True)
        params = {"model": "gpt-5.6-sol", "sandbox": "read-only", "approvalPolicy": "never"}
        if resume:
            params["threadId"] = resume
            params["excludeTurns"] = True
        send({"id": 2, "method": "thread/resume" if resume else "thread/start", "params": params})
        response = until(lambda frame: frame.get("id") == 2)
        assert "result" in response, "native thread start/resume refused"
        thread = response["result"]["thread"]["id"]
        assert resume is None or thread == resume
        if resume:
            assert not response["result"]["thread"]["turns"], "resume re-emitted native transcript history"
        prompt = (
            "Recall the earlier fact. Reply exactly the fact and nothing else." if resume else
            "Remember fixture-fact-809. Reply exactly fixture-fact-809."
        )
        send({"id": 3, "method": "turn/start", "params": {
            "threadId": thread, "input": [{"type": "text", "text": prompt}],
        }})
        acceptance = until(lambda frame: frame.get("id") == 3)
        turn = acceptance["result"]["turn"]["id"]
        assert acceptance["result"]["turn"]["status"] == "inProgress"
        terminal = until(lambda frame: frame.get("method") == "turn/completed")
        assert terminal["params"]["threadId"] == thread
        assert terminal["params"]["turn"]["id"] == turn
        assert terminal["params"]["turn"]["status"] == "completed"
        process.stdin.close()
        assert process.wait(timeout=15) == 0
        if auth_home:
            replies = [
                frame["params"]["item"].get("text", "") for frame in observed
                if frame.get("method") == "item/completed"
                and frame.get("params", {}).get("item", {}).get("type") == "agentMessage"
            ]
            assert replies and "fixture-fact-809" in replies[-1], "native turn lost its objective/history"
        return thread
    finally:
        if process.poll() is None:
            process.terminate()
            try:
                process.wait(timeout=3)
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait()
        process.stdout.close()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--codex", required=True)
    parser.add_argument("--subscription-auth-home", type=Path)
    options = parser.parse_args()
    # The synthetic CI driver is root; its native turns must run with the
    # guest owner's unprivileged boundary. Manual auth-home probes retain
    # the caller's access to the supplied private login.
    if os.geteuid() == 0 and options.subscription_auth_home is None:
        os.setgroups([])
        os.setgid(65534)
        os.setuid(65534)
    disabled = (REPO / "backend/g5d-agent/src/runner/codex_disabled_features.txt").read_text().splitlines() + ["memories", "daemon_auto_start"]
    verify_configuration(options.codex, disabled)
    fixture = runpy.run_path(str(REPO / "scripts/test-native-provider-transport.py"), run_name="fixture")
    server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), fixture["Provider"])
    server.requests = []
    threading.Thread(target=server.serve_forever, daemon=True).start()
    args = [options.codex, "app-server", "--listen", "stdio://", "-c", "features.daemon_auto_start=false"]
    if options.subscription_auth_home:
        args += ["-c", 'model_provider="openai"', "-c", 'cli_auth_credentials_store="file"']
    else:
        args += ["-c", 'model_provider="fixture"', "-c", (
            f'model_providers.fixture={{name="fixture",base_url="http://127.0.0.1:{server.server_port}/v1",'
            'env_key="CODEX_API_KEY",wire_api="responses",supports_websockets=false}'
        )]
    args += [part for feature in disabled for part in ("-c", f"features.{feature}=false")]
    args += ["-c", "memories.generate_memories=false", "-c", "memories.use_memories=false"]
    try:
        with tempfile.TemporaryDirectory() as scratch:
            original, replacement = Path(scratch) / "original", Path(scratch) / "replacement"
            original.mkdir()
            replacement.mkdir()
            thread = attempt(args, original, options.subscription_auth_home)
            assert attempt(args, original, options.subscription_auth_home, thread) == thread
            for file in (original / "sessions").rglob("*.jsonl"):
                target = replacement / file.relative_to(original)
                target.parent.mkdir(parents=True, exist_ok=True)
                shutil.copyfile(file, target)
            assert attempt(args, replacement, options.subscription_auth_home, thread) == thread
            if not options.subscription_auth_home:
                assert server.requests and "fixture-fact-809" in json.dumps(server.requests[-1][2])
                # An exec-written session in a fresh home resumes natively; its
                # history reaches the model once and is not replayed as input.
                historical = Path(scratch) / "exec-history"
                shutil.copytree(EXEC_HISTORY, historical)
                [rollout] = historical.rglob("*.jsonl")
                exec_thread = json.loads(rollout.read_text().splitlines()[0])["payload"]["id"]
                before = len(server.requests)
                assert attempt(args, historical, None, exec_thread) == exec_thread
                sent = json.dumps(server.requests[before:][-1][2])
                assert sent.count(f"Remember {EXEC_HISTORY_FACT}") == 1, "exec history missing or replayed"
            route = "authenticated ChatGPT" if options.subscription_auth_home else "synthetic Responses"
            print(f"PASS {route}: native completion/EOF/exit, same-home and rollout-only replacement-home resume"
                  + ("" if options.subscription_auth_home else ", exec-written history resume"), flush=True)
    finally:
        server.shutdown()
        server.server_close()


if __name__ == "__main__":
    main()
