#!/usr/bin/env python3
"""Replay Claude's native control/background schedules against a loopback model.

Uses the existing native recorder's host interactions. The real CLI owns tools,
permissions, tasks, control replies and exit; only model responses are scripted.
No account credentials or external network access are used.
"""

import argparse
import http.server
import importlib.util
import json
from pathlib import Path
import re
import runpy
import shutil
import subprocess
import sys
import tempfile
import threading
import uuid


REPO = Path(__file__).resolve().parents[1]
FIXTURE = runpy.run_path(str(REPO / "scripts/test-native-provider-catalog.py"), run_name="fixture")
SSE = FIXTURE["FIXTURE"]["sse"]


def text_content(content):
    if isinstance(content, str):
        return content
    return "\n".join(block.get("text", "") for block in content if block.get("type") == "text")


def response(body):
    messages = body["messages"]
    prompt = text_content(next(message["content"] for message in messages if message["role"] == "user"))
    prompt = re.sub(r"<system-reminder>.*?</system-reminder>", "", prompt, flags=re.S).strip()
    calls = [block for message in messages if message["role"] == "assistant"
             and isinstance(message["content"], list) for block in message["content"]
             if block.get("type") == "tool_use"]
    tools = {tool["name"] for tool in body.get("tools", [])}
    name, arguments, text = None, None, "ready"
    if not calls:
        if "AskUserQuestion" in prompt and not prompt.startswith("First use Bash"):
            question = "Continue?" if "Continue?" in prompt else "Choose a color"
            labels = ("Yes", "No") if question == "Continue?" else ("Red", "Blue")
            name, arguments = "AskUserQuestion", {"questions": [{
                "question": question, "header": "Color", "multiSelect": False,
                "options": [{"label": label, "description": label} for label in labels],
            }]}
        elif "general-purpose subagent whose instructions" in prompt:
            job = re.search(r"instructions are: run `(.+?)` in the foreground", prompt).group(1)
            name = "Agent" if "Agent" in tools else "Task"
            arguments = {"description": "Native background probe", "subagent_type": "general-purpose",
                         "prompt": f"Use Bash to run: {job}", "run_in_background": True}
        elif re.search(r"(?:to run:|Bash to run:) ", prompt):
            command = re.search(r"(?:to run:|Bash to run:) (.+?)(?:\n| with the description|\. Then|$)", prompt).group(1)
            name, arguments = "Bash", {"command": command, "description": "Native lifecycle probe",
                                      "run_in_background": "run_in_background set to true" in prompt}
    elif prompt.startswith("First use Bash") and len(calls) == 1:
        name, arguments = "AskUserQuestion", {"questions": [{
            "question": "Choose a color", "header": "Color", "multiSelect": False,
            "options": [{"label": label, "description": label} for label in ("Red", "Blue")],
        }]}
    elif "run_in_background false" in prompt and len(calls) == 1:
        name, arguments = "Bash", {"command": "python3 -c 'import time; time.sleep(60)'",
                                  "description": "Native foreground probe", "run_in_background": False}
    else:
        text = "Blue" if "AskUserQuestion" in prompt else "started" if "background" in prompt else "done"
    if name:
        assert name in tools, (name, sorted(tools))
        block = {"type": "tool_use", "id": "tool_" + uuid.uuid4().hex, "name": name, "input": {}}
        delta = {"type": "input_json_delta", "partial_json": json.dumps(arguments)}
    else:
        block = {"type": "text", "text": ""}
        delta = {"type": "text_delta", "text": text}
    return SSE([
        {"type": "message_start", "message": {
            "id": "msg_" + uuid.uuid4().hex, "type": "message", "role": "assistant",
            "model": body["model"], "content": [], "stop_reason": None, "stop_sequence": None,
            "usage": {"input_tokens": 11, "output_tokens": 0},
        }},
        {"type": "content_block_start", "index": 0, "content_block": block},
        {"type": "content_block_delta", "index": 0, "delta": delta},
        {"type": "content_block_stop", "index": 0},
        {"type": "message_delta", "delta": {"stop_reason": "tool_use" if name else "end_turn",
                                             "stop_sequence": None}, "usage": {"output_tokens": 3}},
        {"type": "message_stop"},
    ])


class Provider(http.server.BaseHTTPRequestHandler):
    def log_message(self, *_):
        pass

    def do_CONNECT(self):
        self.send_error(502)

    def do_GET(self):
        self.send_error(404)

    def do_POST(self):
        if self.path.split("?")[0] != "/v1/messages":
            self.send_error(404)
            return
        try:
            body = json.loads(self.rfile.read(int(self.headers["Content-Length"])))
            payload = response(body)
        except Exception as error:
            self.server.errors.append(repr(error))
            print(f"Synthetic model failed: {error!r}", flush=True)
            self.send_error(400)  # Fail promptly; do not exercise native retry backoff.
            return
        self.send_response(200)
        self.send_header("Content-Type", "text/event-stream")
        self.send_header("Content-Length", str(len(payload)))
        self.end_headers()
        try:
            self.wfile.write(payload)
        except (BrokenPipeError, ConnectionResetError):
            pass  # An in-band interrupt may cancel a model request.


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--claude", required=True)
    parser.add_argument("--only", nargs="+")
    parser.add_argument("--out", help="Retain sanitized recordings for inspection")
    args = parser.parse_args()
    cli = str(Path(args.claude).absolute())
    expected = (REPO / "scripts/claude-code-cli-version").read_text().strip()
    version = subprocess.check_output([cli, "--version"], text=True, timeout=15).split()[0]
    assert version == expected, (version, expected)
    server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Provider)
    server.errors = []
    threading.Thread(target=server.serve_forever, daemon=True).start()
    try:
        with tempfile.TemporaryDirectory(prefix="g5-claude-lifecycle-") as scratch:
            home = Path(scratch)
            env = FIXTURE["environment"](home, f"http://127.0.0.1:{server.server_port}")
            FIXTURE["native_identity"](home)
            out = home / "recordings"
            spec = importlib.util.spec_from_file_location(
                "native_recorder", REPO / "tests/fixtures/agent-turn-native/record.py")
            recorder = importlib.util.module_from_spec(spec)
            spec.loader.exec_module(recorder)
            sessions = []

            class NativeSession(recorder.Session):
                def __init__(self, binary, cwd, *positional, **keywords):
                    super().__init__(binary, cwd, *positional, **keywords,
                                     environment=env, identity=FIXTURE["native_identity"](cwd))
                    sessions.append(self)

            recorder.Session = NativeSession
            command = [str(spec.origin), "--claude", cli, "--out", str(out), "--model", "claude-haiku-4-5"]
            if args.only:
                command += ["--only", *args.only]
            previous_argv = sys.argv
            try:
                sys.argv = command
                recorder.main()
            except Exception:
                for session in sessions[-1:]:
                    print(f"Native lifecycle exit: {session.proc.poll()}; last frames: "
                          + json.dumps(session.frames[-8:]), flush=True)
                raise
            finally:
                sys.argv = previous_argv
                for session in sessions:
                    FIXTURE["stop"](session.proc)
            assert not server.errors, server.errors
            manifest = json.loads((out / "recording.json").read_text())
            assert manifest["package"] == "@anthropic-ai/claude-code@" + expected
            for name, notes in manifest["probes"].items():
                expected_exit = 1 if name == "background-agent-interrupt-turn" else 0
                assert notes["exit_code"] == expected_exit, (name, notes)
                if name == "result-before-eof":
                    assert notes["alive_one_second_after_result"], notes
                if name == "background-bash-report":
                    assert notes["report_turn"] and notes["background_marker_written"], notes
                if name == "background-bash-eof":
                    # Unlike 2.1.285, this pin waits for background commands at
                    # EOF. A successful result alone must not assert process exit.
                    assert notes["background_marker_written"], notes
                if name.startswith("background-agent-interrupt"):
                    assert notes["interrupt_answered"] and not notes["background_marker_written"], notes
                if name == "background-agent-interrupt-turn":
                    assert notes["turn_result"] == "error_during_execution", notes
                if name == "cancel":
                    assert notes["cancel_request"], notes
                if name == "permission-allow":
                    assert notes["probe_file_created"], notes
                if name == "permission-deny":
                    assert not notes["probe_file_created"], notes
            for scenario in manifest["scenarios"]:
                frames = [json.loads(line) for line in (out / scenario["fixture"]).read_text().splitlines()]
                assert frames[0]["claude_code_version"] == expected, scenario
                results = [frame for frame in frames if frame.get("type") == "result"]
                if scenario["name"] == "cancel":
                    # SIGINT cancels the pending native request and may exit
                    # without a result. This is owner interruption, not normal
                    # successful completion; G5 owns the forced outcome/reap.
                    assert all(frame["is_error"] and frame["terminal_reason"].startswith("aborted_")
                               for frame in results), scenario
                else:
                    assert results, scenario
                assert [frame["result_index"] for frame in results] == list(range(len(results))), scenario
                if scenario["name"] not in ("cancel", "background-agent-interrupt-turn"):
                    assert all(not frame["is_error"] for frame in results), scenario
            manifest["model_source"] = "synthetic loopback Messages API; native tools and control transport"
            (out / "recording.json").write_text(json.dumps(manifest, indent=2) + "\n")
            if args.out:
                shutil.copytree(out, args.out, dirs_exist_ok=True)
            print(f"PASS Claude {expected}: {len(manifest['scenarios'])} native lifecycle schedules", flush=True)
    finally:
        server.shutdown()
        server.server_close()


if __name__ == "__main__":
    main()
