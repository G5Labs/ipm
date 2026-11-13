#!/usr/bin/env python3
"""Check every offered model x native mode x per-model effort on the exact pins.

The Rust catalog emits this matrix together with the UI catalog. Real Claude and
Codex binaries perform completed turns against loopback-only synthetic endpoints;
assert native effective modes and outbound model/effort, not just flag parsing.
This establishes CLI compatibility, not upstream model/account availability.
"""

import argparse
from concurrent.futures import ThreadPoolExecutor
import http.server
import itertools
import json
import os
from pathlib import Path
import queue
import runpy
import signal
import subprocess
import sys
import tempfile
import threading
import time

REPO = Path(__file__).resolve().parents[1]
CATALOG = REPO / "tests/fixtures/native-provider-catalog.json"
FIXTURE = runpy.run_path(str(REPO / "scripts/test-native-provider-transport.py"), run_name="fixture")


def environment(home, endpoint):
    return {
        "PATH": os.environ["PATH"], "HOME": str(home), "CODEX_HOME": str(home),
        "CLAUDE_CONFIG_DIR": str(home), "LANG": "C.UTF-8", "CI": "1",
        "ANTHROPIC_BASE_URL": endpoint, "ANTHROPIC_API_KEY": "synthetic-catalog-key",
        "CODEX_API_KEY": "synthetic-catalog-key",
        "CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC": "1", "DISABLE_AUTOUPDATER": "1",
        "HTTP_PROXY": endpoint, "HTTPS_PROXY": endpoint, "ALL_PROXY": endpoint,
        "NO_PROXY": "127.0.0.1,localhost",
    }


def native_identity(home):
    # Cloud Build runs the test harness as root; the VM provider runs as an
    # unprivileged owner. Claude deliberately refuses bypassPermissions as root.
    # Use a private, writable native home under that same privilege boundary.
    if os.geteuid() == 0:
        os.chown(home, 65534, 65534)
        return {"user": 65534, "group": 65534, "extra_groups": []}
    return {}


def stop(process):
    if process.poll() is None:
        os.killpg(process.pid, signal.SIGTERM)
        try:
            process.wait(timeout=3)
        except subprocess.TimeoutExpired:
            os.killpg(process.pid, signal.SIGKILL)
            process.wait(timeout=3)


def cases(provider):
    for model in provider["models"]:
        assert model["efforts"] and model["defaultEffort"] in model["efforts"], model
        assert set(provider["modes"]) <= set(provider["nativeModes"]), provider
        for mode, effort in itertools.product(provider["nativeModes"], model["efforts"]):
            yield model["id"], mode, effort


def claude_case(cli, endpoint, server, case, settings):
    model, mode, effort = case
    with tempfile.TemporaryDirectory(prefix="g5-claude-catalog-") as scratch:
        # Each process has its own URL prefix as well as an isolated native home.
        # This lets parallel cases prove their own outbound request independently.
        prefix = "/" + Path(scratch).name
        args = [
            cli, "-p", "Reply with a greeting.", "--output-format", "stream-json",
            "--verbose", "--tools", "", "--setting-sources", "", "--settings", json.dumps(settings),
            "--strict-mcp-config", "--mcp-config", '{"mcpServers":{}}',
            "--permission-prompts", "none", "--permission-mode", mode,
            "--model", model,
        ]
        if effort != "none":
            args += ["--effort", effort]
        process = subprocess.Popen(args, cwd=scratch, env=environment(scratch, endpoint + prefix),
            stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True, start_new_session=True,
            **native_identity(scratch))
        try:
            stdout, stderr = process.communicate(timeout=45)
            assert process.returncode == 0, (case, stderr[-1500:], stdout[-1500:])
            assert "Unknown --effort" not in stderr + stdout, (case, stderr)
            frames = [json.loads(line) for line in stdout.splitlines()]
            init = next(frame for frame in frames if frame.get("subtype") == "init")
            assert init["model"] == model and init["permissionMode"] == mode, (case, init)
            result = next(frame for frame in frames if frame.get("type") == "result")
            assert not result["is_error"] and result["subtype"] == "success", (case, result)
            requests = [(headers, body) for path, headers, body in server.requests
                        if path == prefix + "/v1/messages"]
            assert requests, (case, "no native model request")
            for headers, body in requests:
                assert body["model"] == model.removesuffix("[1m]"), (case, body["model"])
                assert body.get("output_config", {}).get("effort") == (None if effort == "none" else effort), (
                    case, "effort omitted, clamped or ignored", body.get("output_config"))
                if model.endswith("[1m]"):
                    beta = next((value for key, value in headers.items()
                                 if key.lower() == "anthropic-beta"), "")
                    assert "context-1m" in beta, (case, "1M variant lost its context flag")
        finally:
            stop(process)


class AppServer:
    def __init__(self, cli, home, endpoint):
        disabled = (REPO / "backend/g5d-agent/src/runner/codex_disabled_features.txt").read_text().splitlines() + ["memories", "daemon_auto_start"]
        args = [cli, "app-server", "--listen", "stdio://", "-c", 'model_provider="fixture"', "-c",
                f'model_providers.fixture={{name="fixture",base_url="{endpoint}/v1",'
                'env_key="CODEX_API_KEY",wire_api="responses",supports_websockets=false}']
        args += [part for feature in disabled for part in ("-c", f"features.{feature}=false")]
        args += ["-c", "memories.generate_memories=false", "-c", "memories.use_memories=false"]
        self.process = subprocess.Popen(args, cwd=home, env=environment(home, endpoint),
                                        stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                        stderr=subprocess.DEVNULL, text=True, start_new_session=True,
                                        **native_identity(home))
        self.frames = queue.Queue()
        self.request_id = 0

        def read():
            try:
                for line in self.process.stdout:
                    self.frames.put(json.loads(line))
            finally:
                self.frames.put(None)

        threading.Thread(target=read, daemon=True).start()
        self.request("initialize", {"clientInfo": {"name": "g5_catalog_test", "version": "1"},
                                    "capabilities": {"experimentalApi": False}})
        self.send({"method": "initialized"})

    def send(self, frame):
        self.process.stdin.write(json.dumps(frame) + "\n")
        self.process.stdin.flush()

    def until(self, predicate):
        deadline = time.monotonic() + 45
        while True:
            frame = self.frames.get(timeout=max(.01, deadline - time.monotonic()))
            assert frame is not None, "Codex exited before native evidence"
            # An unsupported native interaction must not hang a catalog test.
            assert not ("method" in frame and "id" in frame), frame
            if predicate(frame):
                return frame
            assert time.monotonic() < deadline, "native evidence deadline exceeded"

    def request(self, method, params):
        self.request_id += 1
        request_id = self.request_id
        self.send({"id": request_id, "method": method, "params": params})
        frame = self.until(lambda frame: frame.get("id") == request_id)
        assert "result" in frame, (method, params, frame)
        return frame["result"]

    def close(self):
        try:
            self.process.stdin.close()
            assert self.process.wait(timeout=15) == 0, "Codex failed after completed turns"
        finally:
            stop(self.process)
            self.process.stdout.close()


def codex_cases(cli, provider, endpoint, server):
    with tempfile.TemporaryDirectory(prefix="g5-codex-catalog-") as home:
        raw_models = json.loads(subprocess.check_output(
            [cli, "debug", "models"], env=environment(home, endpoint),
            text=True, stderr=subprocess.DEVNULL, timeout=15, **native_identity(home)))["models"]
        raw_models = {model["slug"]: model for model in raw_models}
        app = AppServer(cli, home, endpoint)
        try:
            native = app.request("model/list", {})["data"]
            offered = [model for model in provider["models"] if model["route"] == "direct"]
            inventory = {model["model"]: model for model in native}
            assert set(inventory) == {model["id"] for model in offered}, "bundled model inventory drift"
            for model in offered:
                efforts = [entry["reasoningEffort"] for entry in inventory[model["id"]]["supportedReasoningEfforts"]]
                assert efforts == model["efforts"], (model["id"], "native effort inventory drift", efforts)
            sandbox_types = {"read-only": "readOnly", "workspace-write": "workspaceWrite",
                             "danger-full-access": "dangerFullAccess"}
            count = 0
            for model, mode, effort in cases(provider):
                approval, sandbox = mode.split("/")
                response = app.request("thread/start", {
                    "model": model, "approvalPolicy": approval, "sandbox": sandbox,
                })
                assert response["model"] == model, (model, response)
                assert response["approvalPolicy"] == approval, (mode, response)
                assert response["sandbox"]["type"] == sandbox_types[sandbox], (mode, response)
                thread = response["thread"]["id"]
                before = len(server.requests)
                accepted = app.request("turn/start", {"threadId": thread, "effort": effort,
                    "input": [{"type": "text", "text": "Reply with a greeting."}]})
                turn = accepted["turn"]["id"]
                terminal = app.until(lambda frame: frame.get("method") == "turn/completed"
                    and frame["params"]["threadId"] == thread and frame["params"]["turn"]["id"] == turn)
                assert terminal["params"]["turn"]["status"] == "completed", (model, mode, effort, terminal)
                requests = server.requests[before:]
                assert requests, (model, mode, effort, "no native request")
                for path, headers, body in requests:
                    assert path == "/v1/responses", path
                    assert body["model"] == model, (model, body["model"])
                    metadata = next(value for key, value in headers.items()
                                    if key.lower() == "x-codex-turn-metadata")
                    assert json.loads(metadata)["reasoning_effort"] == effort, (model, effort, metadata)
                    # Ultra is a native orchestration selection, not a Responses
                    # effort. Upstream 0.164.0 ModelInfo::resolve_reasoning_effort
                    # uses the model's multi-agent effort or highest non-Ultra.
                    wire_effort = effort
                    if effort == "ultra":
                        info = raw_models[model.removeprefix("openai/")]
                        wire_effort = info.get("multi_agent_reasoning_effort") or next(
                            level["effort"] for level in reversed(info["supported_reasoning_levels"])
                            if level["effort"] != "ultra")
                    assert body.get("reasoning", {}).get("effort") == wire_effort, (
                        model, mode, effort, "effort omitted, clamped or ignored", body.get("reasoning"))
                app.request("thread/archive", {"threadId": thread})
                count += 1
                if count % 50 == 0:
                    print(f"Codex: {count} native combinations passed", flush=True)
            return count
        finally:
            app.close()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--claude", required=True)
    parser.add_argument("--codex", required=True)
    parser.add_argument("--jobs", type=int, default=4)
    parser.add_argument("--g5d", help="Also exercise the production supervisor's provider launches")
    options = parser.parse_args()
    catalog = json.loads(CATALOG.read_text())
    assert catalog["schemaVersion"] == 1 and options.jobs > 0
    for provider, cli, expected in [
        ("claude_code", options.claude, "{version} (Claude Code)"),
        ("codex", options.codex, "codex-cli {version}"),
    ]:
        version = catalog["providers"][provider]["version"]
        actual = subprocess.check_output([cli, "--version"], text=True, timeout=15).strip()
        assert actual == expected.format(version=version), (provider, "pin mismatch", actual, version)

    server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), FIXTURE["Provider"])
    server.requests = []
    threading.Thread(target=server.serve_forever, daemon=True).start()
    endpoint = f"http://127.0.0.1:{server.server_port}"
    try:
        matrix = list(cases(catalog["providers"]["claude_code"]))
        with ThreadPoolExecutor(max_workers=options.jobs) as pool:
            futures = [pool.submit(claude_case, options.claude, endpoint, server, case,
                                  catalog["providers"]["claude_code"]["openrouterSettings"]
                                  if case[0].startswith("anthropic/") else {}) for case in matrix]
            try:
                for index, future in enumerate(futures, 1):
                    future.result()
                    if index % 50 == 0:
                        print(f"Claude: {index}/{len(matrix)} native combinations passed", flush=True)
            finally:
                for future in futures:
                    future.cancel()
        print(f"PASS Claude: all {len(matrix)} model/mode/effort combinations", flush=True)
        server.requests.clear()
        count = codex_cases(options.codex, catalog["providers"]["codex"], endpoint, server)
        print(f"PASS Codex: all {count} model/mode/effort combinations and native inventory", flush=True)
    finally:
        server.shutdown()
        server.server_close()
    if options.g5d:
        transport = [
            sys.executable, str(REPO / "scripts/test-native-provider-transport.py"),
            "--g5d", options.g5d, "--claude", options.claude, "--codex", options.codex,
        ]
        subprocess.run(transport + ["--codex-tool-turn"], check=True, timeout=300)


if __name__ == "__main__":
    main()
