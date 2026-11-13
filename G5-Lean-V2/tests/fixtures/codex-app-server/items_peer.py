#!/usr/bin/python3
"""Minimal stable App Server peer for dispatcher fixtures.

Its shell wrapper runs per-test checks first and then execs this peer with a
JSON configuration path. The peer answers the stable handshake, records what
the supervisor sent under $HOME (thread request, prompt text, image paths),
replays the configured App Server items and completes the one turn.
"""
import json
import os
from pathlib import Path
import sys
import time

config = json.loads(Path(sys.argv[1]).read_text())
home = Path(os.environ["HOME"])


def read():
    line = sys.stdin.readline()
    if not line:
        raise SystemExit(42)
    return json.loads(line)


def send(frame):
    print(json.dumps(frame, ensure_ascii=False), flush=True)


init = read()
assert init["method"] == "initialize", init
assert init["params"]["capabilities"]["experimentalApi"] is False, init
send({"id": init["id"], "result": {"userAgent": f"g5_agent/{config['version']} (fixture Linux)"}})
assert read()["method"] == "initialized"
start = read()
(home / "thread-request.json").write_text(json.dumps(start))
assert start["method"] in ("thread/start", "thread/resume"), start
thread = start["params"].get("threadId") or config["thread"]
sandbox = {"workspace-write": "workspaceWrite", "danger-full-access": "dangerFullAccess"}.get(
    start["params"].get("sandbox"), "readOnly")
send({"id": start["id"], "result": {
    "thread": {"id": thread, "ephemeral": start["params"].get("ephemeral", False)},
    "model": start["params"]["model"], "approvalPolicy": start["params"]["approvalPolicy"],
    "sandbox": {"type": sandbox}, "instructionSources": [],
}})
turn_start = read()
assert turn_start["method"] == "turn/start" and turn_start["params"]["threadId"] == thread
parts = turn_start["params"]["input"]
prompt = "\n".join(part["text"] for part in parts if part["type"] == "text")
(home / "received-prompt").write_text(prompt)
(home / "received-images").write_text(
    "\n".join(part["path"] for part in parts if part["type"] == "localImage"))
if config.get("block_if_prompt_contains") and config["block_if_prompt_contains"] in prompt:
    # Never confirm acceptance; the supervisor's setup deadline must end it.
    time.sleep(config.get("block_seconds", 10))
    raise SystemExit(0)
turn = "33333333-3333-4333-8333-333333333333"
send({"id": turn_start["id"], "result": {"turn": {"id": turn, "items": [], "status": "inProgress"}}})
# Work continuing after acceptance is not bounded by the setup deadline.
time.sleep(config.get("delay_after_acceptance", 0))
for item in config["items"]:
    # Like the pinned CLI, a started item carries no output yet.
    started = dict(item, status="inProgress") if "status" in item else dict(item)
    if "aggregatedOutput" in started:
        started["aggregatedOutput"] = ""
    for method, frame in (("item/started", started), ("item/completed", item)):
        sys.stdout.write(json.dumps({"method": method, "params": {
            "threadId": thread, "turnId": turn, "item": frame}}, ensure_ascii=False) + "\n")
    sys.stdout.flush()
usage = {"inputTokens": 2, "cachedInputTokens": 0, "outputTokens": 3, "reasoningOutputTokens": 0, "totalTokens": 5}
send({"method": "thread/tokenUsage/updated", "params": {
    "threadId": thread, "turnId": turn, "tokenUsage": {"last": usage, "total": usage}}})
send({"method": "turn/completed", "params": {
    "threadId": thread, "turn": {"id": turn, "items": [], "status": config.get("status", "completed"), "error": None}}})
assert sys.stdin.read() == ""
