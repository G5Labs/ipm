#!/usr/bin/python3
"""Deterministic native peer for the production supervisor/dispatcher tests."""
import json
import os
from pathlib import Path
import sys
import time

CASE = "normal"
EXPECTED_DISABLED = []
CODEX_VERSION = "injected-by-dispatcher-test"
Path(__file__).with_suffix(".pid").write_text(str(os.getpid()))
trace = Path(__file__).with_suffix(".trace")
release = Path(__file__).with_suffix(".release")
thread = "22222222-2222-4222-8222-222222222222"
turn = "33333333-3333-4333-8333-333333333333"

def mark(phase):
    with trace.open("a") as file:
        file.write(phase + "\n")

def read():
    line = sys.stdin.readline()
    if not line:
        mark("early-eof")
        raise SystemExit(42)
    frame = json.loads(line)
    mark("read:" + frame.get("method", "response"))
    return frame

def send(frame):
    print(json.dumps(frame, ensure_ascii=False), flush=True)

def event(method, **params):
    send({"method": method, "params": {"threadId": thread, "turnId": turn, **params}})

def sleep_forever():
    time.sleep(60)

def fail_phase(frame, phase):
    codes = {phase + "_refusal": -32600, phase + "_method_refusal": -32601,
             phase + "_params_refusal": -32602, phase + "_internal_failure": -32603}
    if CASE in codes:
        send({"id": frame["id"], "error": {"code": codes[CASE], "message": "refused"}})
        assert sys.stdin.read() == ""
        mark("eof")
        raise SystemExit(0)
    if CASE == phase + "_silent":
        mark(CASE)
        sleep_forever()

args = sys.argv[1:]
assert args[:3] == ["app-server", "--listen", "stdio://"], args
assert args.count("--listen") == 1
assert not any(arg.startswith("--listen=") for arg in args)
effective = {}
for index, arg in enumerate(args[1:], 1):
    previous = args[index - 1]
    if previous in ("-c", "--config") and "=" in arg:
        key, value = arg.split("=", 1)
        effective[key.strip()] = value.strip()
    elif previous in ("--enable", "--disable"):
        effective["features." + arg] = "true" if previous == "--enable" else "false"
for feature in EXPECTED_DISABLED + ["memories", "daemon_auto_start"]:
    assert effective.get("features." + feature) == "false", feature
assert "features.instant_interrupt=false" in args
assert "features.daemon_auto_start=false" in args
assert "approval_policy=\"never\"" in args
init = read()
assert init["method"] == "initialize"
assert init["params"]["capabilities"]["experimentalApi"] is False
fail_phase(init, "initialize")
send({"id": init["id"], "result": {"userAgent": f"g5_agent/{CODEX_VERSION} (Linux fixture)"}})
assert read()["method"] == "initialized"
start = read()
assert start["method"] in ("thread/start", "thread/resume")
assert start["params"]["approvalPolicy"] == "never"
assert start["params"]["sandbox"] == "read-only"
assert start["params"]["model"] == "fixture-model"
fail_phase(start, "thread")
configuration = {"thread": {"id": thread}, "model": start["params"]["model"], "approvalPolicy": "never", "sandbox": {"type": "readOnly"}}
if CASE in ("wrong_model", "wrong_approval", "wrong_sandbox"):
    if CASE == "wrong_model":
        configuration["model"] = "wrong-model"
    elif CASE == "wrong_approval":
        configuration["approvalPolicy"] = "on-request"
    else:
        configuration["sandbox"] = {"type": "dangerFullAccess"}
send({"id": start["id"], "result": configuration})
if CASE in ("wrong_model", "wrong_approval", "wrong_sandbox"):
    assert sys.stdin.read() == ""
    mark("eof")
    raise SystemExit(0)
if CASE == "blocked_turn_write":
    mark(CASE)
    sleep_forever()
request = read()
assert request["method"] == "turn/start"
assert request["params"]["threadId"] == thread
if CASE != "image":
    assert request["params"]["effort"] == "high"
fail_phase(request, "turn")
if CASE == "lost_ack":
    raise SystemExit(0)
if CASE == "malformed_ack":
    send({"id": request["id"]})
    assert sys.stdin.read() == ""
    mark("eof")
    raise SystemExit(0)
if CASE == "oversized_ack":
    send({"id": request["id"], "result": {"padding": "x" * (17 * 1024 * 1024),
          "turn": {"id": turn, "items": [], "status": "inProgress"}}})
    assert sys.stdin.read() == ""
    raise SystemExit(0)
send({"id": request["id"], "result": {"turn": {"id": turn, "items": [], "status": "inProgress"}}})
mark("accepted")
if CASE == "active_gate":
    while not release.exists():
        time.sleep(.01)
if CASE == "output_pressure":
    while True:
        event("item/agentMessage/delta", itemId="partial", delta="preserved partial output " * 100)
if CASE in ("active_silent", "active_cancel"):
    sleep_forever()
if CASE == "malformed":
    print("{broken", flush=True)
    assert sys.stdin.read() == ""
    mark("eof")
    raise SystemExit(0)
if CASE == "malformed_identity":
    event("turn/completed", turn={"status": "completed", "items": []})
    assert sys.stdin.read() == ""
    mark("eof")
    raise SystemExit(0)
if CASE == "unsupported_missing_id":
    send({"method": "item/commandExecution/requestApproval", "params": {"threadId": thread, "turnId": turn}})
    assert sys.stdin.read() == ""
    mark("eof")
    raise SystemExit(0)
if CASE in ("unsupported", "unsupported_string_id"):
    request_id = "request-809" if CASE == "unsupported_string_id" else 999
    send({"id": request_id, "method": "item/commandExecution/requestApproval", "params": {"threadId": thread, "turnId": turn}})
    answer = read()
    assert answer["id"] == request_id and answer["error"]["code"] == -32601
    mark("rejected")
    assert sys.stdin.read() == ""
    mark("eof")
    raise SystemExit(0)
if CASE in ("tool_only", "multi_tool", "large_tool"):
    for index in range(3 if CASE == "multi_tool" else 1):
        item = {"id": f"tool-{index}", "type": "commandExecution", "command": "printf tool-output", "status": "inProgress"}
        event("item/started", item=item)
        event("item/completed", item={**item, "status": "completed", "aggregatedOutput": ("large héllo 🦀 " * 100_000) if CASE == "large_tool" else f"tool-output-{index}", "exitCode": 0})
if CASE != "tool_only":
    text = os.environ["APP_SERVER_SECRET"] if CASE in ("secret", "split_secret") else "complete héllo 🦀"
    if CASE == "nul":
        text += "\0preserved after NUL"
    for part in ([text[:12], text[12:]] if CASE == "split_secret" else [text]):
        event("item/agentMessage/delta", itemId="message-1", delta=part)
    event("item/completed", item={"id": "message-1", "type": "agentMessage", "text": text})
if CASE in ("no_terminal", "failed_no_terminal"):
    raise SystemExit(9 if CASE == "failed_no_terminal" else 0)
if CASE == "wrong_identity":
    event("turn/completed", turn={"id": "stale-turn", "status": "completed", "items": []})
    mark("wrong-completion")
    while not release.exists():
        time.sleep(.01)
usage = {"inputTokens": 10, "outputTokens": 5, "cachedInputTokens": 2, "totalTokens": 15, "reasoningOutputTokens": 0}
for _ in range(2):
    event("thread/tokenUsage/updated", tokenUsage={"last": usage, "total": {**usage, "inputTokens": 1000}})
status = "interrupted" if CASE.startswith("interrupted") else ("failed" if CASE == "failed" else "completed")
terminal = {"id": turn, "status": status, "items": [], "error": None}
if CASE == "oversized_completed":
    terminal["padding"] = "x" * (17 * 1024 * 1024)
event("turn/completed", turn=terminal)
if CASE == "duplicate":
    event("turn/completed", turn={"id": turn, "status": status, "items": [], "error": None})
mark("native-completed")
assert sys.stdin.read() == ""
mark("eof")
if CASE == "delayed_exit":
    while not release.exists():
        time.sleep(.01)
if CASE == "ignore_eof":
    sleep_forever()
if CASE == "trailing":
    event("item/completed", item={"id": "message-trailing", "type": "agentMessage", "text": "trailing output"})
    print("trailing stderr", file=sys.stderr, flush=True)
if CASE == "held_pipe" and os.fork() == 0:
    time.sleep(12)
    os._exit(0)
mark("exiting")
raise SystemExit(9 if CASE in ("nonzero", "interrupted_nonzero") else 0)
