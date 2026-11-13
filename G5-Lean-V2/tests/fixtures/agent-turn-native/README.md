# Native agent-turn fixtures

The current **2.1.321** fixtures are in [`claude-2.1.321/`](claude-2.1.321/), recorded
on Linux x86_64 on 2026-11-09 with a real pinned CLI and a synthetic loopback Messages
API. Native tools, control requests/replies, background tasks and process exit execute
normally. Reproduce them without credentials using:

```sh
python3 scripts/test-native-claude-lifecycle.py --claude /path/to/pinned/claude \
  --out tests/fixtures/agent-turn-native/claude-2.1.321
```

[`claude-2.1.308/`](claude-2.1.308/) holds the same ten schedules recorded on the
previous pin (2026-10-25); it is retained as historical evidence and nothing replays it.

The required native catalog CI gate reruns all ten schedules against the staged
production pin, including under a root driver with unprivileged native children.
The current EOF schedule waits for its background command to finish and write its
marker. This differs from the historical five-second kill below. The production
supervisor replay uses the current background/report/interrupt recordings.

## Historical 2.1.285 evidence

The `claude/` directory contains sanitized stdout recordings from Claude Code `2.1.285` on Linux aarch64 (a clean container, uid 999), recorded on 2026-09-30 (the Bypass question scenario was added on 2026-10-11) with [`record.py`](record.py). The recordings use the production bidirectional `stream-json` flags; the prompting scenarios add `--permission-prompt-tool stdio`.

The scenarios cover:

- a Bypass turn that runs Bash without approval, waits for a native `AskUserQuestion` answer, and uses it (`bypass-question`, with `--permission-mode bypassPermissions --permission-prompts host --permission-prompt-tool stdio`);
- a native `AskUserQuestion`, permission allow and deny, cancellation while a question is pending, and the provider's result-before-stdin-EOF behavior;
- a background Bash job whose completion Claude reports in its own follow-up turn while stdin stays open (`background-bash-report`);
- a background Bash job when stdin closes at the first result (`background-bash-eof`): Claude stops the job about five seconds after EOF and exits;
- a background sub-agent stopped by the host's in-band `interrupt` control request, followed by EOF (`background-agent-interrupt`): Claude stops the sub-agent, answers the request, starts no report turn and exits;
- the same interrupt while Claude's turn runs a long foreground command next to a background sub-agent (`background-agent-interrupt-turn`): Claude answers the request, stops the sub-agent and the command, ends the turn with an `error_during_execution` result and exits 1 after EOF. Its frames include `task_started`/`task_notification` for the foreground command with `is_backgrounded: false`.

Matching `*.responses.jsonl` files contain the exact host frames sent for answered requests and the interrupt. `recording.json` records hashes, counts and externally observed process behavior per scenario. Session IDs, the working directory (`{{cwd}}`, and `{{cwd_key}}` for Claude's per-project directory) and the local Claude configuration inventory are sanitized; native request IDs and provider frames remain as recorded. Streamed tool-input fragments can split a path across deltas, so a fragment of the recorder's temporary directory name may remain.

To record authenticated upstream evidence separately, run `record.py` with the
selected binary in a clean authenticated environment (its docstring has an example).
Preserve the actual version and provenance; do not relabel historical recordings.

The older fixtures under `tests/fixtures/provider-frames/` remain the historical P3.1 CLI evidence. They are intentionally not rewritten by the native-control cutover.
