#!/usr/bin/env python3
"""Exercise the real pinned Claude CLI against a deterministic model endpoint.

No provider credential is used. These are CLI protocol recordings, not live-model
QA. Run through `record.sh claude-steering`. The paused-process case deliberately
exercises the gap between an upstream request and the CLI's stdout replay.
"""
import argparse
import json
import os
from pathlib import Path
import queue
import signal
import subprocess
import tempfile
import threading
import time
import uuid
from hashlib import sha256
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer


class Model(BaseHTTPRequestHandler):
    def log_message(self, *args):
        pass

    def do_POST(self):
        body = json.loads(self.rfile.read(int(self.headers.get("Content-Length", 0))))
        if "count_tokens" in self.path:
            self.send_response(200)
            self.end_headers()
            self.wfile.write(b'{"input_tokens":100}')
            return
        run = self.server.run
        run["requests"].append({"time": time.monotonic(), "messages": body.get("messages", [])})
        first = len(run["requests"]) == 1
        case = run["case"]
        if case == "kill-request" and not first:
            os.killpg(run["pid"], signal.SIGSTOP)
        if case in ("stop-request", "kill-request") and not first:
            time.sleep(4)
        block = {"type": "text", "text": "DONE1" if first else "STEERED"}
        if first and case not in ("text", "stop-request", "kill-request"):
            block = {"type": "tool_use", "id": "toolu_probe", "name": "Bash", "input": {
                "command": "sleep 6 && echo first-done", "description": "Wait for the steering probe"}}
            if case == "background":
                block["input"]["run_in_background"] = True
            if case == "permission":
                block.update(name="Write", input={"file_path": str(run["cwd"] / "permission.txt"), "content": "probe"})
            if case == "question":
                block.update(name="AskUserQuestion", input={"questions": [{
                    "question": "Choose a color", "header": "Color", "multiSelect": False,
                    "options": [{"label": "Red", "description": "Red color"},
                                {"label": "Blue", "description": "Blue color"}]}]})
        def event(kind, **fields):
            value = {"type": kind, **fields}
            self.wfile.write(("event: " + kind + "\ndata: " + json.dumps(value) + "\n\n").encode())
            self.wfile.flush()

        try:
            self.send_response(200)
            self.send_header("Content-Type", "text/event-stream")
            self.end_headers()
            event("message_start", message={"id": "msg_" + uuid.uuid4().hex, "type": "message",
                  "role": "assistant", "model": body.get("model", "claude-haiku-4-5"),
                  "content": [], "stop_reason": None, "stop_sequence": None,
                  "usage": {"input_tokens": 100, "output_tokens": 1}})
            start = dict(block)
            if block["type"] == "text":
                start["text"] = ""
                delta = {"type": "text_delta", "text": block["text"]}
            else:
                start["input"] = {}
                delta = {"type": "input_json_delta", "partial_json": json.dumps(block["input"])}
            event("content_block_start", index=0, content_block=start)
            event("content_block_delta", index=0, delta=delta)
            if first and case in ("text", "stop-request", "kill-request"):
                time.sleep(2)
            event("content_block_stop", index=0)
            event("message_delta", delta={"stop_reason": "tool_use" if block["type"] == "tool_use" else "end_turn",
                                           "stop_sequence": None}, usage={"output_tokens": 10})
            event("message_stop")
        except (BrokenPipeError, ConnectionResetError):
            pass


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--claude", required=True)
    ap.add_argument("--pin", required=True)
    ap.add_argument("--out", type=Path, required=True)
    cases = ["tool", "text", "permission", "question", "background", "stop", "stop-request", "kill-request", "image", "resume"]
    ap.add_argument("--only", nargs="+", choices=cases)
    ap.add_argument("--flush-only", action="store_true", help="Record tool/text delivery after flush and EOF without user replay")
    args = ap.parse_args()
    version = subprocess.check_output([args.claude, "--version"], text=True, timeout=15).strip()
    if version.split()[0] != args.pin:
        raise SystemExit(f"expected Claude {args.pin}, found {version}")
    args.out.mkdir(parents=True, exist_ok=True)
    summaries = []
    with tempfile.TemporaryDirectory(prefix="g5-steering-") as root:
        for case in (["tool", "text"] if args.flush_only else args.only or cases):
            started = time.monotonic()
            run = {"case": case, "requests": []}
            server = ThreadingHTTPServer(("127.0.0.1", 0), Model)
            server.run = run
            threading.Thread(target=server.serve_forever, daemon=True).start()
            cwd = Path(root) / case
            cwd.mkdir()
            run["cwd"] = cwd
            env = {k: v for k, v in os.environ.items() if k not in (
                "CLAUDECODE", "CLAUDE_CODE_ENTRYPOINT", "CLAUDE_CODE_SESSION_ID")}
            for key in ("ANTHROPIC_AUTH_TOKEN", "CLAUDE_CODE_OAUTH_TOKEN", "ANTHROPIC_API_KEY_FILE"):
                env.pop(key, None)
            env.update(ANTHROPIC_BASE_URL=f"http://127.0.0.1:{server.server_port}",
                       ANTHROPIC_API_KEY="protocol-probe-no-credential", CLAUDE_CONFIG_DIR=str(cwd / "config"))
            sid, initial, steer = [str(uuid.uuid4()) for _ in range(3)]
            argv = [args.claude, "-p", "--output-format", "stream-json", "--input-format", "stream-json",
                    "--verbose", "--include-partial-messages", "--replay-user-messages", "--model", "haiku",
                    "--strict-mcp-config", "--setting-sources", "project", "--session-id", sid,
                    "--permission-mode", "default" if case in ("permission", "question") else "bypassPermissions"]
            if args.flush_only:
                argv.remove("--replay-user-messages")
            if case in ("permission", "question"):
                argv += ["--permission-prompt-tool", "stdio"]
            err = (cwd / "stderr").open("w")
            proc = subprocess.Popen(argv, cwd=cwd, env=env, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                    stderr=err, text=True, start_new_session=True)
            run["pid"] = proc.pid
            frames, incoming, writes, signals = [], queue.Queue(), [], []
            pending_request_observed = None

            def read():
                for line in proc.stdout:
                    try:
                        frame = json.loads(line)
                    except ValueError:
                        continue
                    frames.append({"time": time.monotonic(), "frame": frame})
                    incoming.put(frame)
                incoming.put(None)

            reader = threading.Thread(target=read, daemon=True)
            reader.start()

            def send(content, identity):
                frame = {"type": "user", "uuid": identity, "message": {"role": "user", "content": content}}
                if args.flush_only:
                    frame.pop("uuid")
                proc.stdin.write(json.dumps(frame) + "\n")
                proc.stdin.flush()
                writes.append({"time": time.monotonic(), "frame": frame})

            prompt = "Run Bash `sleep 6 && echo first-done`, then reply DONE1."
            if case == "text":
                prompt = "Write a 400 word story without tools."
            elif case == "background":
                prompt = "Run Bash `sleep 6 && echo first-done` with run_in_background true. Reply STARTED without waiting."
            elif case == "question":
                prompt = "Use AskUserQuestion to ask me to choose Red or Blue."
            elif case == "permission":
                prompt = f"Use Write to create {cwd}/permission.txt containing probe."
            send(prompt, initial)
            sent, results, failure = False, 0, None
            deadline = time.monotonic() + 70
            try:
                while time.monotonic() < deadline:
                    if case in ("stop-request", "kill-request") and len(run["requests"]) >= 2:
                        time.sleep(0.3)
                        signals.append({"time": time.monotonic(), "signal": "SIGINT"})
                        os.killpg(proc.pid, signal.SIGINT)
                        # Same grace as the provider adapter's stop policy.
                        time.sleep(0.2)
                        if proc.poll() is None:
                            signals.append({"time": time.monotonic(), "signal": "SIGKILL"})
                            os.killpg(proc.pid, signal.SIGKILL)
                        break
                    try:
                        frame = incoming.get(timeout=0.1)
                    except queue.Empty:
                        continue
                    if frame is None:
                        break
                    tool = any(b.get("type") == "tool_use" for b in frame.get("message", {}).get("content", []) if isinstance(b, dict))
                    result = frame.get("type") == "result"
                    results += int(result)
                    trigger = tool
                    if case in ("text", "stop-request", "kill-request"):
                        trigger = frame.get("event", {}).get("type") == "content_block_delta"
                    elif case in ("permission", "question"):
                        trigger = frame.get("type") == "control_request"
                    elif case == "background":
                        trigger = result
                    if trigger and not sent:
                        content = "Remember the exact secret word STEERED. Include STEERED in your answer."
                        if case == "image":
                            content = [{"type": "text", "text": content}, {"type": "image", "source": {
                                "type": "base64", "media_type": "image/png", "data":
                                "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+aS3cAAAAASUVORK5CYII="}}]
                        send(content, steer)
                        sent = True
                        if args.flush_only:
                            proc.stdin.close()
                        if case == "stop":
                            time.sleep(0.1)
                            signals.append({"time": time.monotonic(), "signal": "SIGINT"})
                            os.killpg(proc.pid, signal.SIGINT)
                            break
                        if case in ("permission", "question"):
                            time.sleep(0.3)
                            pending_request_observed = (frame.get("request", {}).get("subtype") == "can_use_tool"
                                and not any(f["frame"].get("isReplay") and f["frame"].get("uuid") == steer for f in frames))
                            response = {"type": "control_response", "response": {"subtype": "success",
                                "request_id": frame["request_id"], "response": {
                                    "behavior": "deny", "message": "Probe declines this request"}}}
                            proc.stdin.write(json.dumps(response) + "\n")
                            proc.stdin.flush()
                            writes.append({"time": time.monotonic(), "frame": response})
                    if result and sent and (case not in ("text", "background", "stop-request", "kill-request") or results >= 2):
                        break
                else:
                    failure = "timed out"
            finally:
                try:
                    proc.stdin.close()
                except BrokenPipeError:
                    pass
                try:
                    proc.wait(timeout=12)
                except subprocess.TimeoutExpired:
                    signals.append({"time": time.monotonic(), "signal": "SIGKILL"})
                    os.killpg(proc.pid, signal.SIGKILL)
                    proc.wait()
                reader.join(timeout=3)
                err.close()
            resumed_with_steer = None
            if case == "resume" and failure is None:
                before = len(run["requests"])
                resume_argv = argv.copy()
                resume_argv[resume_argv.index("--session-id")] = "--resume"
                resumed = subprocess.run(resume_argv, cwd=cwd, env=env, text=True, capture_output=True, timeout=30,
                    input=json.dumps({"type": "user", "message": {"role": "user", "content": "Recall my earlier secret word."}}) + "\n")
                resumed_with_steer = any("STEERED" in json.dumps(m) for r in run["requests"][before:]
                                        for m in r["messages"] if m.get("role") == "user")
                for line in resumed.stdout.splitlines():
                    try:
                        frame = json.loads(line)
                    except json.JSONDecodeError:
                        continue
                    frames.append({"time": time.monotonic(), "frame": frame})
            server.shutdown()
            echoes = [f for f in frames if f["frame"].get("isReplay")]
            echo = next((f for f in echoes if f["frame"].get("uuid") == steer), None)
            requests_with_steer = [r for r in run["requests"] if any(
                m.get("role") == "user" and "STEERED" in json.dumps(m) for m in r["messages"])]
            image_received = any(b.get("type") == "image" for r in requests_with_steer for m in r["messages"]
                                 for b in m.get("content", []) if isinstance(b, dict))
            echo_index = next((i for i, f in enumerate(frames) if f is echo), None)
            tool_results = [i for i, f in enumerate(frames) if f["frame"].get("type") == "user" and any(
                isinstance(b, dict) and b.get("type") == "tool_result" for b in f["frame"].get("message", {}).get("content", []))]
            tool_order = bool(echo_index is not None and any(i < echo_index for i in tool_results)
                              and any(f["frame"].get("event", {}).get("type") == "message_start" for f in frames[echo_index + 1:]))
            summary = {"case": case, "cli_version": args.pin, "model_endpoint": "scripted-local",
                       "sent": sent, "exit_code": proc.returncode, "results": sum(f["frame"].get("type") == "result" for f in frames),
                       "initial_uuid_echoed": any(f["frame"].get("uuid") == initial for f in echoes),
                       "steer_uuid_echoed": echo is not None, "model_requests_with_steer": len(requests_with_steer),
                       "echo_observed_before_model_request": bool(echo and requests_with_steer and echo["time"] < requests_with_steer[0]["time"]),
                       "native_resume_includes_steer": resumed_with_steer,
                       "pending_native_request_observed": pending_request_observed,
                       "model_received_image": image_received,
                       "tool_result_before_echo_before_continued_output": tool_order,
                       "stdout_eof": not reader.is_alive(),
                       "failure": failure}

            def scrub(value):
                if isinstance(value, str):
                    return value.replace(str(cwd), "{{cwd}}").replace(sid, "{{session_id}}").replace(initial, "{{initial_uuid}}").replace(steer, "{{steer_uuid}}")
                if isinstance(value, list):
                    return [scrub(v) for v in value]
                if isinstance(value, dict):
                    result = {k: scrub(v) for k, v in value.items() if k not in ("memory_paths", "messaging_socket_path")}
                    if value.get("type") == "system" and value.get("subtype") == "init":
                        for key in ("skills", "agents", "slash_commands", "plugins"):
                            if key in result:
                                result[key] = []
                    return result
                return value

            recording_name = f"steer-{case}" + ("-flush" if args.flush_only else "")
            frame_path = args.out / f"{recording_name}.jsonl"
            with frame_path.open("w") as out:
                for frame in frames:
                    out.write(json.dumps(scrub(frame["frame"]), separators=(",", ":")) + "\n")
            summary["frames_sha256"] = sha256(frame_path.read_bytes()).hexdigest()
            trace = [{"time": f["time"], "event": "stdout", "frame_index": i,
                      "type": f["frame"].get("type"), "uuid": f["frame"].get("uuid") if f["frame"].get("isReplay") else None}
                     for i, f in enumerate(frames)]
            trace += [{"time": w["time"], "event": "stdin", "frame": w["frame"]} for w in writes]
            trace += [{"event": "signal", **s} for s in signals]
            trace += [{"time": r["time"], "event": "model.request", "contains_steer": r in requests_with_steer}
                      for r in run["requests"]]
            for event in trace:
                event["time"] = round(event["time"] - started, 6)
            trace.sort(key=lambda e: e["time"])
            (args.out / f"{recording_name}.trace.json").write_text(json.dumps(scrub(trace), indent=2) + "\n")
            # These checks make a refresh fail visibly if the pinned protocol
            # changes. Observed cross-thread timestamps alone are not a proof
            # of stdout-before-request ordering; the kill case is decisive.
            valid = sent and summary["initial_uuid_echoed"] and summary["stdout_eof"] and failure is None
            if case == "kill-request":
                valid &= not summary["steer_uuid_echoed"] and bool(requests_with_steer) and proc.returncode == -9
            elif case == "stop":
                valid &= not summary["steer_uuid_echoed"] and not requests_with_steer
            else:
                valid &= summary["steer_uuid_echoed"] and bool(requests_with_steer)
                if case != "stop-request":
                    valid &= proc.returncode == 0
                    valid &= summary["results"] == (2 if case in ("text", "background", "resume") else 1)
            if case == "resume":
                valid &= resumed_with_steer
            if case in ("tool", "image", "resume"):
                valid &= tool_order
            if case == "image":
                valid &= image_received
            if case in ("question", "permission"):
                valid &= pending_request_observed
            if args.flush_only:
                valid = (sent and not echoes and summary["stdout_eof"] and failure is None
                         and bool(requests_with_steer) and proc.returncode == 0
                         and summary["results"] == (2 if case == "text" else 1))
                summary["stdin_closed_after_flush"] = True
                summary["replay_requested"] = False
            summary["verified"] = bool(valid)
            summaries.append(summary)
            print(json.dumps(summary), flush=True)
    manifest = args.out / ("steering-flush-recording.json" if args.flush_only else "steering-recording.json")
    previous = json.loads(manifest.read_text()) if manifest.exists() else []
    updated = {s["case"]: s for s in previous + summaries}
    manifest.write_text(json.dumps(sorted(updated.values(), key=lambda s: s["case"]), indent=2) + "\n")
    return int(any(not s["verified"] for s in summaries))


if __name__ == "__main__":
    raise SystemExit(main())
