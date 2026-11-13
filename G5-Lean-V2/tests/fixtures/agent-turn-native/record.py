#!/usr/bin/env python3
"""Record the exact-pin native Claude fixtures in this directory.

usage: record.py --claude PATH --out DIR [--model haiku] [--only NAME ...]

Runs the pinned Claude Code binary with the production agent-turn transport
(`-p`, bidirectional stream-json, partial messages, `--permission-prompt-tool
stdio` for prompting scenarios), plays the host side of each scenario, and
writes sanitized stdout recordings, the host responses it sent, and
`recording.json` with hashes, counts and observed process behavior.

Run it where the pinned CLI is installed and authenticated, ideally a clean
container so no local Claude configuration is recorded, for example:

  docker run --rm --user 999 -e CLAUDE_CODE_OAUTH_TOKEN -e DISABLE_AUTOUPDATER=1 \\
    -v "$PWD/tests/fixtures/agent-turn-native:/fixtures" <image-with-pinned-claude> \\
    python3 /fixtures/record.py --claude /path/to/claude --out /fixtures/claude

Scenario prompts ask a small model for exact tool calls; rerun a scenario
with --only when the model does not follow its prompt.
"""
import argparse
import json
import os
import shutil
import signal
import subprocess
import tempfile
import threading
import time
from datetime import datetime, timezone
from hashlib import sha256

QUESTION = (
    "Use the AskUserQuestion tool exactly once. Ask one question whose question text is exactly "
    "'Choose a color', with header 'Color', multiSelect false, and exactly two options: label 'Red' "
    "with description 'Red color', and label 'Blue' with description 'Blue color'. "
    "After you get the answer, reply with the chosen color only."
)
CANCEL_QUESTION = (
    "Use the AskUserQuestion tool exactly once. Ask one question whose question text is exactly "
    "'Continue?', with header 'Confirm', multiSelect false, and exactly two options: label 'Yes' "
    "with description 'Proceed', and label 'No' with description 'Stop'."
)
PROBE_FILE = "g5-pinned-permission-probe"


def permission_prompt(cwd, description):
    return (
        f"Use the Bash tool exactly once to run: touch {cwd}/{PROBE_FILE} "
        f"with the description '{description}'. Then reply with the single word: done"
    )


def background_bash_prompt(marker, seconds):
    job = f"python3 -c \"import time; time.sleep({seconds}); open('{marker}', 'w').write('finished')\""
    return (
        f"Use the Bash tool exactly once with run_in_background set to true to run: {job}\n"
        "Do not wait for it or check on it. After starting it, reply with the single word: started"
    )


def background_agent_prompt(marker, seconds):
    job = f"python3 -c \"import time; time.sleep({seconds}); open('{marker}', 'w').write('finished')\""
    return (
        "Use your subagent tool (Agent/Task) exactly once with run_in_background set to true to launch "
        f"a general-purpose subagent whose instructions are: run `{job}` in the foreground with the "
        "Bash tool, then reply 'done'. Do not wait for it. After launching it, reply with the single "
        "word: started"
    )


def background_agent_turn_prompt(marker, seconds):
    return (
        background_agent_prompt(marker, seconds).split(" Do not wait for it.")[0]
        + " Then, without checking on it, use the Bash tool in the foreground "
        "(run_in_background false) to run: python3 -c 'import time; time.sleep(60)'\n"
        "After that command returns, reply with the single word: finished"
    )


SCENARIOS = ["question", "bypass-question", "permission-allow", "permission-deny", "cancel", "result-before-eof",
             "background-bash-report", "background-bash-eof", "background-agent-interrupt",
             "background-agent-interrupt-turn"]


class Session:
    """One Claude process with the agent-turn transport and a frame log."""

    def __init__(self, claude, cwd, model, prompting, session_id, bypass_questions=False,
                 environment=None, identity=None):
        args = [
            claude, "-p", "--output-format", "stream-json", "--input-format", "stream-json",
            "--verbose", "--include-partial-messages", "--session-id", session_id,
        ]
        if prompting or bypass_questions:
            args += ["--permission-mode", "bypassPermissions" if bypass_questions else "default", "--permission-prompts", "host",
                     "--permission-prompt-tool", "stdio"]
        else:
            args += ["--permission-mode", "bypassPermissions", "--permission-prompts", "none",
                     "--disallowedTools", "AskUserQuestion"]
        args += ["--strict-mcp-config", "--setting-sources", "project", "--model", model]
        self.proc = subprocess.Popen(args, cwd=cwd, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                     stderr=subprocess.DEVNULL, text=True, start_new_session=True,
                                     env=environment, **(identity or {}))
        self.frames = []
        self.responses = []
        self.lock = threading.Lock()
        self.changed = threading.Condition(self.lock)
        self.output_closed = False
        self.reader = threading.Thread(target=self._read, daemon=True)
        self.reader.start()

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
            self.output_closed = True
            self.changed.notify_all()

    def send(self, frame, response=False):
        self.proc.stdin.write(json.dumps(frame, separators=(",", ":")) + "\n")
        self.proc.stdin.flush()
        if response:
            self.responses.append(frame)

    def close_input(self):
        try:
            self.proc.stdin.close()
        except BrokenPipeError:
            pass

    def wait_for(self, predicate, timeout):
        """The first frame matching `predicate`, waiting up to `timeout` seconds."""
        deadline = time.time() + timeout
        seen = 0
        with self.changed:
            while True:
                for frame in self.frames[seen:]:
                    if predicate(frame):
                        return frame
                seen = len(self.frames)
                remaining = deadline - time.time()
                # Exit can precede reading the final buffered result. Drain
                # stdout before deciding that a native frame is missing.
                if remaining <= 0 or (self.proc.poll() is not None and self.output_closed):
                    return None
                self.changed.wait(min(remaining, 0.5))

    def results(self):
        with self.lock:
            return [frame for frame in self.frames if frame.get("type") == "result"]

    def wait_exit(self, timeout):
        try:
            code = self.proc.wait(timeout=timeout)
        except subprocess.TimeoutExpired:
            os.killpg(self.proc.pid, signal.SIGKILL)
            self.proc.wait()
            code = None
        self.reader.join(timeout=5)
        assert not self.reader.is_alive(), "native stdout did not drain after exit"
        return code


def is_result(frame):
    return frame.get("type") == "result"


def is_request(tool):
    return lambda frame: (frame.get("type") == "control_request"
                          and frame.get("request", {}).get("subtype") == "can_use_tool"
                          and frame["request"].get("tool_name") == tool)


def allow(request, extra=None):
    updated = dict(request["request"].get("input") or {})
    updated.update(extra or {})
    return {"type": "control_response", "response": {
        "subtype": "success", "request_id": request["request_id"],
        "response": {"behavior": "allow", "updatedInput": updated}}}


def deny(request, message):
    return {"type": "control_response", "response": {
        "subtype": "success", "request_id": request["request_id"],
        "response": {"behavior": "deny", "message": message}}}


def nth_result(n):
    def predicate(frame, _count=[0]):
        if frame.get("type") == "result":
            _count[0] += 1
            return _count[0] >= n
        return False
    return predicate


def run_scenario(name, claude, model):
    """Play one scenario; return (session, cwd, session_id, probe notes)."""
    cwd = tempfile.mkdtemp(prefix="g5-native-")
    session_id = str(__import__("uuid").uuid4())
    prompting = name in ("question", "permission-allow", "permission-deny", "cancel")
    session = Session(claude, cwd, model, prompting, session_id, name == "bypass-question")
    note = {}
    marker = os.path.join(cwd, f"{name}.done")

    def user(text):
        session.send({"type": "user", "message": {"role": "user", "content": text}})

    if name in ("question", "bypass-question"):
        prompt = QUESTION
        if name == "bypass-question":
            prompt = (
                f"First use Bash to run: touch {cwd}/{PROBE_FILE}. "
                "Then perform the following clarification, waiting for the user's answer: " + QUESTION
            )
        user(prompt)
        request = session.wait_for(is_request("AskUserQuestion"), 180)
        assert request, "no AskUserQuestion request"
        if name == "bypass-question":
            assert os.path.exists(os.path.join(cwd, PROBE_FILE)), "ordinary tool did not execute"
            assert not any(is_request("Bash")(frame) for frame in session.frames), "ordinary tool prompted"
            time.sleep(1)
            assert session.proc.poll() is None and not any(is_result(frame) for frame in session.frames), "question did not wait"
            note["ordinary_tool_executed_without_prompt"] = True
            note["question_waited_for_owner"] = True
        session.send(allow(request, {"answers": {"Choose a color": "Blue"}}), response=True)
        result = session.wait_for(is_result, 180)
        assert result, "no result"
        if name == "bypass-question":
            assert not result.get("is_error") and result.get("result", "").strip() == "Blue", "answer not used"
            note["answer_used"] = True
        session.close_input()
    elif name in ("permission-allow", "permission-deny"):
        description = "Create a probe file" if name == "permission-allow" else "Create a permission probe file"
        user(permission_prompt(cwd, description))
        request = session.wait_for(is_request("Bash"), 180)
        assert request, "no Bash permission request"
        if name == "permission-allow":
            session.send(allow(request), response=True)
        else:
            session.send(deny(request, "Denied by pinned fixture"), response=True)
        assert session.wait_for(is_result, 180), "no result"
        session.close_input()
        created = os.path.exists(os.path.join(cwd, PROBE_FILE))
        note["probe_file_created"] = created
    elif name == "cancel":
        user(CANCEL_QUESTION)
        request = session.wait_for(is_request("AskUserQuestion"), 180)
        assert request, "no AskUserQuestion request"
        os.kill(session.proc.pid, signal.SIGINT)
        cancelled = session.wait_for(
            lambda frame: frame.get("type") == "control_cancel_request"
            and frame.get("request_id") == request["request_id"], 60)
        assert cancelled, "pending native request was not cancelled after SIGINT"
        note["cancel_request"] = True
        session.close_input()
    elif name == "result-before-eof":
        user("Reply with the single word: ready")
        assert session.wait_for(is_result, 180), "no result"
        time.sleep(1)
        note["alive_one_second_after_result"] = session.proc.poll() is None
        session.close_input()
    elif name == "background-bash-report":
        user(background_bash_prompt(marker, 15))
        assert session.wait_for(is_result, 180), "no first result"
        note["report_turn"] = session.wait_for(nth_result(2), 120) is not None
        session.close_input()
    elif name == "background-bash-eof":
        user(background_bash_prompt(marker, 15))
        assert session.wait_for(is_result, 180), "no first result"
        closed = time.time()
        session.close_input()
        code = session.wait_exit(60)
        note["exit_after_eof_s"] = round(time.time() - closed, 1)
        note["exit_code"] = code
    elif name == "background-agent-interrupt":
        user(background_agent_prompt(marker, 30))
        assert session.wait_for(is_result, 180), "no first result"
        time.sleep(3)
        session.send({"type": "control_request", "request_id": "g5-end-fixture",
                      "request": {"subtype": "interrupt"}}, response=True)
        reply = session.wait_for(lambda frame: frame.get("type") == "control_response"
                                 and frame.get("response", {}).get("request_id") == "g5-end-fixture", 30)
        note["interrupt_answered"] = reply is not None
        closed = time.time()
        session.close_input()
        code = session.wait_exit(60)
        note["exit_after_eof_s"] = round(time.time() - closed, 1)
        note["exit_code"] = code
    elif name == "background-agent-interrupt-turn":
        user(background_agent_turn_prompt(marker, 30))
        # Claude registers a long foreground command as a task after a few
        # seconds; interrupt while it runs, before the turn's result.
        foreground = session.wait_for(lambda frame: frame.get("subtype") == "task_started"
                                      and frame.get("is_backgrounded") is False
                                      and not frame.get("owned_by_subagent"), 180)
        assert foreground, "no foreground command"
        assert not session.results(), "the turn ended before the interrupt"
        session.send({"type": "control_request", "request_id": "g5-end-fixture",
                      "request": {"subtype": "interrupt"}}, response=True)
        reply = session.wait_for(lambda frame: frame.get("type") == "control_response"
                                 and frame.get("response", {}).get("request_id") == "g5-end-fixture", 30)
        note["interrupt_answered"] = reply is not None
        aborted = session.wait_for(is_result, 30)
        note["turn_result"] = aborted and aborted.get("subtype")
        closed = time.time()
        session.close_input()
        code = session.wait_exit(60)
        note["exit_after_eof_s"] = round(time.time() - closed, 1)
        note["exit_code"] = code
    else:
        raise SystemExit(f"unknown scenario {name}")
    note.setdefault("exit_code", session.wait_exit(60))
    if name == "cancel":
        note["cancel_result_received"] = bool(session.results())
    note["background_marker_written"] = os.path.exists(marker) if name.startswith("background") else None
    return session, cwd, session_id, note


def sanitize(value, cwd, session_id):
    real_cwd = os.path.realpath(cwd)
    if isinstance(value, dict):
        out = {}
        for key, item in value.items():
            if key in ("memory_paths", "messaging_socket_path"):
                continue
            out[key] = sanitize(item, cwd, session_id)
        if out.get("type") == "system" and out.get("subtype") == "init":
            for key in ("slash_commands", "skills", "agents", "plugins"):
                if key in out:
                    out[key] = []
        return out
    if isinstance(value, list):
        return [sanitize(item, cwd, session_id) for item in value]
    if isinstance(value, str):
        for path in sorted({real_cwd, cwd}, key=len, reverse=True):
            value = value.replace(path, "{{cwd}}")
        # Claude derives per-project directories from the working directory.
        for path in sorted({real_cwd, cwd}, key=len, reverse=True):
            key = "".join(c if c.isascii() and (c.isalnum() or c == "-") else "-" for c in path)
            value = value.replace(key, "{{cwd_key}}")
        return value.replace(session_id, "{{session_id}}")
    return value


def write_jsonl(path, frames):
    body = "".join(json.dumps(frame, separators=(",", ":"), ensure_ascii=False) + "\n" for frame in frames)
    with open(path, "w") as handle:
        handle.write(body)
    return sha256(body.encode()).hexdigest()


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--claude", required=True)
    parser.add_argument("--out", required=True)
    parser.add_argument("--model", default="haiku")
    parser.add_argument("--only", nargs="*")
    args = parser.parse_args()
    version = subprocess.run([args.claude, "--version"], capture_output=True, text=True).stdout.split()[0]
    names = args.only or SCENARIOS
    manifest_path = os.path.join(args.out, "recording.json")
    manifest = {"scenarios": [], "probes": {}}
    if os.path.exists(manifest_path) and args.only:
        with open(manifest_path) as handle:
            manifest = json.load(handle)
    os.makedirs(args.out, exist_ok=True)
    for name in names:
        session, cwd, session_id, note = run_scenario(name, args.claude, args.model)
        frames = [sanitize(frame, cwd, session_id) for frame in session.frames]
        digest = write_jsonl(os.path.join(args.out, f"{name}.jsonl"), frames)
        response_fixture = None
        if session.responses:
            response_fixture = f"{name}.responses.jsonl"
            write_jsonl(os.path.join(args.out, response_fixture),
                        [sanitize(frame, cwd, session_id) for frame in session.responses])
        entry = {
            "name": name,
            "cli_version": version,
            "fixture": f"{name}.jsonl",
            "sha256": digest,
            "frame_count": len(frames),
            "control_request_count": sum(f.get("type") == "control_request" for f in frames),
            "control_cancel_count": sum(f.get("type") == "control_cancel_request" for f in frames),
            "result_count": sum(f.get("type") == "result" for f in frames),
            "host_response_fixture": response_fixture,
        }
        manifest["scenarios"] = [s for s in manifest["scenarios"] if s["name"] != name] + [entry]
        manifest["probes"][name] = note
        print(name, json.dumps(note), flush=True)
        shutil.rmtree(cwd, ignore_errors=True)
    order = {name: index for index, name in enumerate(SCENARIOS)}
    manifest["scenarios"].sort(key=lambda s: order.get(s["name"], len(order)))
    manifest.update({
        "recorded_at": datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
        "package": f"@anthropic-ai/claude-code@{version}",
        "platform": f"{os.uname().sysname.lower()}-{os.uname().machine}",
    })
    with open(manifest_path, "w") as handle:
        json.dump(manifest, handle, indent=2)
        handle.write("\n")


if __name__ == "__main__":
    main()
