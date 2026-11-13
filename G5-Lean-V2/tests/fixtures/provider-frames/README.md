# Recorded provider frames

The `claude/steer-*.jsonl` additions are from **Claude Code 2.1.285**, running
against a scripted local model endpoint. Reproduce them with
`tools/fake-provider-cli/record.sh claude-steering`; `--only tool text` selects
cases. `claude/steering-recording.json` records assertions and hashes, and each
`steer-*.trace.json` records the relative times of stdin, stdout, model requests
and stop signals. They do not replace live-model QA.

The tool case has one result, with the extra user's replay between the tool
result and continued answer. The text case has two results in one process.
The resume case also runs `--resume` and checks that the model's user context
contains the earlier instruction. Image, question, permission and background
cases exercise their respective native input paths.

`steer-kill-request` deliberately pauses the CLI when the second model request
arrives, then sends SIGINT followed by SIGKILL after the adapter's 200 ms grace.
The request contains the steered input, but stdout reaches EOF without its
replay. This disproves the proposed EOF-only safe-redelivery test in issue #544.
Cross-thread timestamp comparisons in the other cases are observations, not
proof of a pre-request barrier. The production runner uses completed stdin flush
as delivery, with no echo dependency.

`steer-tool-flush` and `steer-text-flush` run without replay enabled and close stdin
immediately after sending the correction. The model receives it and Claude finishes
one result during a tool or two results during final text. Their manifest is
`claude/steering-flush-recording.json`. Reproduce with the same recorder and
`--flush-only`; these are the production delivery/EOF protocol cases.

Stdout of the provider CLIs pinned when they were recorded (Claude Code
2.1.270, Codex 0.153.3; see `recording.json`), recorded by
`tools/fake-provider-cli/record.sh` and replayed by `fake-claude` /
`fake-codex` (`G5_FAKE_SCENARIO=<file>`). The shapes are documented in
`docs/lean-provider-cli-contract.md`; `recording.json` records when each
scenario was captured, with which CLI version, argv, prompt, exit code and
stderr.

| Fixture | What the real CLI was asked | What it shows |
|---|---|---|
| `claude/text-only.jsonl` | reply `OK` | init, status, thinking and text `stream_event`s, `assistant`, `rate_limit_event`, `result` |
| `claude/resume.jsonl` | `--resume` of the text-only session, reply `AGAIN` | the resumed init frame carries the same `session_id` |
| `claude/write-file.jsonl` | write `hello.txt` with the `Write` tool (`bypassPermissions`) | `tool_use` block with `caller`, `wire_tool_inputs`, `input_json_delta` events, `user` tool_result with `tool_use_result` |
| `claude/mcp-ask-user.jsonl` | call `ask_user` on the `g5-tools` MCP server | deferred-tool `ToolSearch` + `tool_reference`, then the MCP `tool_use` with `tool_use_meta` and its `tool_result` |
| `claude/permission-allow.jsonl` | write `hello.txt` in `--permission-mode default` with `--permission-prompt-tool mcp__g5-tools__request_permission` answering allow | frames identical in shape to `write-file`; the prompt call is only visible in `*.mcp-requests.jsonl` |
| `claude/permission-deny.jsonl` | same, answering deny | the denial `tool_result` (`is_error`, `tool_result_meta`) and `result.permission_denials` |
| `claude/*.mcp-requests.jsonl` | — | JSON-RPC messages Claude Code sent to the MCP server: `initialize`, `notifications/initialized`, `tools/list`, `tools/call` (`ask_user`, `request_permission`) |
| `codex/text-only.jsonl` | reply `OK` | `thread.started`, `turn.started`, `agent_message`, `turn.completed` |
| `codex/resume.jsonl` | `exec resume <thread id>` | the same `thread_id` re-emitted |
| `codex/write-file.jsonl` | create `hello.txt` | `command_execution` item started/completed |
| `codex/mcp-ask-user.jsonl` | call `ask_user` with `approval_mode = "approve"` | `mcp_tool_call` item started/completed with `result.content` |
| `codex/mcp-ask-user-unapproved.jsonl` | same without `approval_mode` | `mcp_tool_call` failed: "requires approval, but approval policy is never" |
| `codex/*.mcp-requests.jsonl` | — | JSON-RPC messages Codex sent to the MCP server |
| `recording.json` scenario `codex/resume-missing` | `exec resume` of an unknown id | no frames; exit 1 and the stderr error text are in the manifest |

Sanitisation applied by `record.sh`: the session/thread id is replaced by
`{{session_id}}` and the temporary working directory by `{{cwd}}` everywhere;
in Claude's `system/init` frame `memory_paths` and `messaging_socket_path` are
removed and the account-specific `skills`, `agents`, `slash_commands` and
`plugins` lists are emptied. Model names, token usage, costs and timings are
kept as recorded. The fakes substitute the placeholders at replay time from
their argv and cwd.

To refresh these recordings, run `tools/fake-provider-cli/record.sh all` (see
the script header for credentials and prerequisites). A pin bump re-records the
exact-pin native fixtures in `tests/fixtures/agent-turn-native/` instead; the
2.1.285 and 0.159.3 upgrade left these recordings unchanged.
