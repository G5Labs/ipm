#!/usr/bin/env python3
"""Qualify Claude's native question deferral vs. queued input on the pinned CLI.

Reproduces issue G5Labs/G5-Lean#968 against a loopback Messages API, with no
account credentials or external network. Drives the real pinned CLI with G5's
exact bidirectional stream-json arguments and the PreToolUse AskUserQuestion
initialize hook, then asserts the boundary the fix relies on:

  A  defer, close stdin, resume with initialize only   -> question reissued
  B  follow-up queued before the deferring hook reply   -> session advances; the
                                                           resumed session never
                                                           reissues the question
  C  follow-up queued before a live (non-deferred) hook  -> both delivered in one
                                                           native turn
  C2 the same plus a second live question                -> both answered

Only model responses are scripted; the real CLI owns tools, the hook, control
replies, session files and exit. Establishes native protocol behaviour on
Linux, not full G5 correctness.

  python3 scripts/test-native-claude-question-overlap.py --claude /path/to/claude
"""

import argparse
import http.server
import json
import os
import signal
import subprocess
import sys
import tempfile
import threading
import time
import uuid
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
INIT_ID = "g5-native-question-init"
HOOK_ID = "g5-native-question"
QUESTION_1 = "Choose a color"
QUESTION_2 = "Choose a size"


def sse(events):
    return "".join(
        f"event: {e['type']}\ndata: {json.dumps(e)}\n\n" for e in events
    ).encode()


def text_of(content):
    if isinstance(content, str):
        return content
    return "\n".join(
        b.get("text", "")
        for b in content
        if isinstance(b, dict) and b.get("type") == "text"
    )


def scripted_response(body):
    """Reads conversation shape only; never account state."""
    messages = body["messages"]
    tools = {t["name"] for t in body.get("tools", [])}
    first_user = next(m for m in messages if m["role"] == "user")
    two_questions = "TWO_QUESTIONS" in text_of(first_user["content"])
    last = messages[-1]
    last_content = last["content"]
    tool_results = (
        [b for b in last_content if isinstance(b, dict) and b.get("type") == "tool_result"]
        if isinstance(last_content, list)
        else []
    )
    asked = [
        b
        for m in messages
        if m["role"] == "assistant" and isinstance(m["content"], list)
        for b in m["content"]
        if b.get("type") == "tool_use" and b.get("name") == "AskUserQuestion"
    ]
    name, arguments, text = None, None, None
    if last["role"] == "user" and tool_results:
        if two_questions and len(asked) == 1:
            name, arguments = "AskUserQuestion", {
                "questions": [
                    {
                        "question": QUESTION_2,
                        "header": "Size",
                        "multiSelect": False,
                        "options": [
                            {"label": "Small", "description": "S"},
                            {"label": "Large", "description": "L"},
                        ],
                    }
                ]
            }
        else:
            text = "used the answer"
    elif not asked:
        name, arguments = "AskUserQuestion", {
            "questions": [
                {
                    "question": QUESTION_1,
                    "header": "Color",
                    "multiSelect": False,
                    "options": [
                        {"label": "Red", "description": "Red"},
                        {"label": "Blue", "description": "Blue"},
                    ],
                }
            ]
        }
    else:
        text = "ack follow-up"
    if name:
        assert name in tools, (name, sorted(tools))
        block = {"type": "tool_use", "id": "toolu_" + uuid.uuid4().hex[:12], "name": name, "input": {}}
        delta = {"type": "input_json_delta", "partial_json": json.dumps(arguments)}
    else:
        block = {"type": "text", "text": ""}
        delta = {"type": "text_delta", "text": text}
    return sse(
        [
            {
                "type": "message_start",
                "message": {
                    "id": "msg_" + uuid.uuid4().hex[:12],
                    "type": "message",
                    "role": "assistant",
                    "model": body["model"],
                    "content": [],
                    "stop_reason": None,
                    "stop_sequence": None,
                    "usage": {"input_tokens": 11, "output_tokens": 0},
                },
            },
            {"type": "content_block_start", "index": 0, "content_block": block},
            {"type": "content_block_delta", "index": 0, "delta": delta},
            {"type": "content_block_stop", "index": 0},
            {
                "type": "message_delta",
                "delta": {"stop_reason": "tool_use" if name else "end_turn", "stop_sequence": None},
                "usage": {"output_tokens": 3},
            },
            {"type": "message_stop"},
        ]
    )


class Provider(http.server.BaseHTTPRequestHandler):
    def log_message(self, *_):
        pass

    def do_GET(self):
        self.send_error(404)

    def do_POST(self):
        if self.path.split("?")[0] != "/v1/messages":
            self.send_error(404)
            return
        try:
            body = json.loads(self.rfile.read(int(self.headers["Content-Length"])))
            payload = scripted_response(body)
        except Exception as error:  # noqa: BLE001
            self.server.errors.append(repr(error))
            self.send_error(400)
            return
        self.server.requests.append(body)
        self.send_response(200)
        self.send_header("Content-Type", "text/event-stream")
        self.send_header("Content-Length", str(len(payload)))
        self.end_headers()
        try:
            self.wfile.write(payload)
        except (BrokenPipeError, ConnectionResetError):
            pass


# Every CLI started by this run, so a failed assertion still ends its process
# group before the temporary directory it works in is removed.
SESSIONS = []


class Session:
    def __init__(self, cli, home, cwd, endpoint, session_id=None, resume=None):
        args = [
            cli, "-p", "--output-format", "stream-json", "--input-format", "stream-json",
            "--verbose", "--include-partial-messages",
        ]
        args += ["--resume", resume] if resume else ["--session-id", session_id]
        args += [
            "--permission-mode", "default", "--permission-prompts", "host",
            "--permission-prompt-tool", "stdio", "--strict-mcp-config",
            "--setting-sources", "user,project", "--model", "claude-haiku-4-5",
        ]
        env = {
            "PATH": os.environ["PATH"], "HOME": str(home),
            "CLAUDE_CONFIG_DIR": str(home / ".claude"), "LANG": "C.UTF-8", "CI": "1",
            "ANTHROPIC_BASE_URL": endpoint, "ANTHROPIC_API_KEY": "synthetic-overlap-key",
            "CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC": "1", "DISABLE_AUTOUPDATER": "1",
            "CLAUDE_CODE_DISABLE_AUTO_MEMORY": "1",
            "HTTP_PROXY": endpoint, "HTTPS_PROXY": endpoint, "NO_PROXY": "127.0.0.1,localhost",
        }
        self.proc = subprocess.Popen(
            args, cwd=cwd, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
            stderr=subprocess.DEVNULL, text=True, start_new_session=True, env=env,
        )
        self.frames = []
        self.lock = threading.Lock()
        self.changed = threading.Condition(self.lock)
        self.closed = False
        SESSIONS.append(self)
        threading.Thread(target=self._read, daemon=True).start()

    def kill(self):
        if self.proc.poll() is None:
            try:
                os.killpg(self.proc.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
            self.proc.wait()

    def _read(self):
        for line in self.proc.stdout:
            line = line.strip()
            if not line:
                continue
            try:
                frame = json.loads(line)
            except json.JSONDecodeError:
                continue
            with self.changed:
                self.frames.append(frame)
                self.changed.notify_all()
        with self.changed:
            self.closed = True
            self.changed.notify_all()

    def send(self, frame):
        self.proc.stdin.write(json.dumps(frame, separators=(",", ":")) + "\n")
        self.proc.stdin.flush()

    def close_input(self):
        try:
            self.proc.stdin.close()
        except BrokenPipeError:
            pass

    def wait_for(self, predicate, timeout):
        deadline = time.time() + timeout
        seen = 0
        with self.changed:
            while True:
                for frame in self.frames[seen:]:
                    if predicate(frame):
                        return frame
                seen = len(self.frames)
                remaining = deadline - time.time()
                if remaining <= 0 or (self.proc.poll() is not None and self.closed):
                    return None
                self.changed.wait(min(remaining, 0.5))

    def wait_exit(self, timeout):
        try:
            return self.proc.wait(timeout=timeout)
        except subprocess.TimeoutExpired:
            os.killpg(self.proc.pid, signal.SIGKILL)
            self.proc.wait()
            return None


def initialize():
    return {
        "type": "control_request", "request_id": INIT_ID,
        "request": {
            "subtype": "initialize",
            "hooks": {"PreToolUse": [{"matcher": "AskUserQuestion", "hookCallbackIds": [HOOK_ID]}]},
        },
    }


def user(text):
    return {"type": "user", "message": {"role": "user", "content": text}}


def hook_response(request_id, defer):
    response = (
        {"hookSpecificOutput": {"hookEventName": "PreToolUse", "permissionDecision": "defer"}}
        if defer
        else {}
    )
    return {
        "type": "control_response",
        "response": {"subtype": "success", "request_id": request_id, "response": response},
    }


def is_hook(frame):
    return (
        frame.get("type") == "control_request"
        and frame["request"].get("subtype") == "hook_callback"
        and frame["request"].get("callback_id") == HOOK_ID
    )


def is_can_use(frame):
    return (
        frame.get("type") == "control_request"
        and frame["request"].get("subtype") == "can_use_tool"
        and frame["request"].get("tool_name") == "AskUserQuestion"
    )


def is_init_ack(frame):
    return frame.get("type") == "control_response" and frame["response"].get("request_id") == INIT_ID


def answer(frame, label):
    req = frame["request"]
    updated = dict(req["input"])
    updated["answers"] = {q["question"]: label for q in req["input"]["questions"]}
    return {
        "type": "control_response",
        "response": {
            "subtype": "success",
            "request_id": frame["request_id"],
            "response": {"behavior": "allow", "updatedInput": updated},
        },
    }


def defer_then_resume(cli, endpoint, scratch, steer_before):
    try:
        return _defer_then_resume(cli, endpoint, scratch, steer_before)
    finally:
        # Kill before the caller removes the scratch directory.
        for session in SESSIONS:
            session.kill()


def _defer_then_resume(cli, endpoint, scratch, steer_before):
    home, cwd = scratch / "home", scratch / "work"
    (home / ".claude").mkdir(parents=True)
    cwd.mkdir()
    sid = str(uuid.uuid4())
    s = Session(cli, home, cwd, endpoint, session_id=sid)
    s.send(initialize())
    s.send(user("Ask me one question with AskUserQuestion."))
    hook = s.wait_for(is_hook, 60)
    assert hook, "no hook callback"
    if steer_before:
        s.send(user("FOLLOW-UP queued before the hook reply"))
    s.send(hook_response(hook["request_id"], True))
    deferred = s.wait_for(
        lambda f: f.get("type") == "result" and f.get("stop_reason") == "tool_deferred", 60
    )
    assert deferred, "no tool_deferred result"
    s.close_input()
    s.wait_exit(60)
    r = Session(cli, home, cwd, endpoint, resume=sid)
    r.send(initialize())
    assert r.wait_for(is_init_ack, 30), "resume did not acknowledge initialize"
    deadline = time.time() + 20
    reissued = None
    answered = set()
    while time.time() < deadline:
        f = r.wait_for(
            lambda f: (is_hook(f) and f["request_id"] not in answered) or is_can_use(f),
            max(0.1, deadline - time.time()),
        )
        if f is None:
            break
        if is_hook(f):
            answered.add(f["request_id"])
            r.send(hook_response(f["request_id"], False))
            continue
        if is_can_use(f):
            reissued = f
            break
    if reissued:
        r.send(answer(reissued, "Blue"))
        r.wait_for(lambda f: f.get("type") == "result", 60)
    r.close_input()
    r.wait_exit(30)
    return bool(reissued)


def live_questions(cli, endpoint, scratch, requests, two_questions):
    """Returns the answers given, the results and the model requests made."""
    try:
        return _live_questions(cli, endpoint, scratch, requests, two_questions)
    finally:
        for session in SESSIONS:
            session.kill()


def _live_questions(cli, endpoint, scratch, requests, two_questions):
    first_request = len(requests)
    home, cwd = scratch / "home", scratch / "work"
    (home / ".claude").mkdir(parents=True)
    cwd.mkdir()
    s = Session(cli, home, cwd, endpoint, session_id=str(uuid.uuid4()))
    s.send(initialize())
    s.send(user("Ask me a question with AskUserQuestion." + (" TWO_QUESTIONS" if two_questions else "")))
    hook = s.wait_for(is_hook, 60)
    assert hook, "no hook callback"
    s.send(user("FOLLOW-UP queued before the live question"))
    s.send(hook_response(hook["request_id"], False))
    q1 = s.wait_for(is_can_use, 60)
    assert q1, "no live can_use_tool"
    s.send(answer(q1, "Blue"))
    answered = 1
    if two_questions:
        hook2 = s.wait_for(lambda f: is_hook(f) and f["request_id"] != hook["request_id"], 60)
        assert hook2, "no second hook"
        s.send(hook_response(hook2["request_id"], False))
        q2 = s.wait_for(lambda f: is_can_use(f) and f["request_id"] != q1["request_id"], 60)
        assert q2, "no second live question"
        s.send(answer(q2, "Large"))
        answered = 2
    deadline = time.time() + 60
    while time.time() < deadline:
        if sum(1 for f in s.frames if f.get("type") == "result") >= 1:
            break
        time.sleep(0.3)
    results = [f for f in s.frames if f.get("type") == "result"]
    s.close_input()
    s.wait_exit(60)
    return answered, results, requests[first_request:]


def user_content(request):
    """Everything the user role carried, serialized. The pinned CLI delivers a
    message queued mid-turn inside the next tool result's content (as a
    system reminder), not as its own text block, so search the whole of it."""
    return json.dumps([m["content"] for m in request["messages"] if m["role"] == "user"])


def tool_result_count(request):
    return sum(
        1
        for m in request["messages"]
        if m["role"] == "user" and isinstance(m["content"], list)
        for b in m["content"]
        if isinstance(b, dict) and b.get("type") == "tool_result"
    )


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--claude", default=os.environ.get("G5_NATIVE_CLAUDE_TEST_PATH"))
    args = parser.parse_args()
    if not args.claude:
        print("set --claude or G5_NATIVE_CLAUDE_TEST_PATH", file=sys.stderr)
        return 2
    cli = str(Path(args.claude).absolute())
    expected = (REPO / "scripts/claude-code-cli-version").read_text().strip()
    version = subprocess.check_output([cli, "--version"], text=True, timeout=20).split()[0]
    assert version == expected, (version, expected)
    server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Provider)
    server.errors = []
    server.requests = []
    threading.Thread(target=server.serve_forever, daemon=True).start()
    endpoint = f"http://127.0.0.1:{server.server_port}"
    try:
        with tempfile.TemporaryDirectory(prefix="g5-q968-a-") as scratch:
            assert defer_then_resume(cli, endpoint, Path(scratch), steer_before=False), (
                "A: a plainly deferred question must be reissued on resume"
            )
        with tempfile.TemporaryDirectory(prefix="g5-q968-b-") as scratch:
            assert not defer_then_resume(cli, endpoint, Path(scratch), steer_before=True), (
                "B: a follow-up before the deferral advances the session; the "
                "resumed session must not reissue the question"
            )
        follow_up = "FOLLOW-UP queued before the live question"
        with tempfile.TemporaryDirectory(prefix="g5-q968-c-") as scratch:
            answered, results, requests = live_questions(
                cli, endpoint, Path(scratch), server.requests, two_questions=False
            )
            assert answered == 1 and results and not any(r["is_error"] for r in results), (
                f"C: one live question and a follow-up must complete: {results}"
            )
            # Delivery, not just completion: the model saw the answer's tool
            # result and the queued follow-up text.
            assert any(
                tool_result_count(r) >= 1 and follow_up in user_content(r) for r in requests
            ), f"C: the follow-up and the answer must reach the model: {requests}"
        with tempfile.TemporaryDirectory(prefix="g5-q968-c2-") as scratch:
            answered, results, requests = live_questions(
                cli, endpoint, Path(scratch), server.requests, two_questions=True
            )
            assert answered == 2 and results and not any(r["is_error"] for r in results), (
                f"C2: two live questions and a follow-up must complete: {results}"
            )
            assert any(
                tool_result_count(r) >= 2 and follow_up in user_content(r) for r in requests
            ), f"C2: both answers and the follow-up must reach the model: {requests}"
        assert not server.errors, server.errors
        print(f"PASS Claude {expected}: native question overlap boundary holds")
    finally:
        for session in SESSIONS:
            session.kill()
        server.shutdown()
        server.server_close()
    return 0


if __name__ == "__main__":
    sys.exit(main())
