# Provider CLI contract: pinned `claude` and `codex`, and their fakes

Status: written by P3.1 on 2026-09-13 for
[lean-backend-design-v6.md](lean-backend-design-v6.md) §8.3 and §8.6. The
provider runner (P3.2) builds its argv from this document; the fake binaries in
`tools/fake-provider-cli/` and the recorded frames under
`tests/fixtures/provider-frames/` are the evidence behind every shape listed
here. Everything marked **observed** was seen on the pinned releases on
2026-09-13; everything marked **from the current sidecar** is behaviour the
Node sidecars handled (deleted in P3.2) that the recordings did not exercise;
the Rust runner (`backend/g5d-worker/src/runner/`) keeps it.

This file describes the **currently implemented direct-CLI runner**. The
proposed successor contract based on `@anthropic-ai/claude-agent-sdk@0.3.273`
and Codex CLI `0.154.0` is [Agent API Surface](agent-api-surface.md). That
proposal requires an explicit architecture and runtime migration; it has not
silently changed the pins or behavior documented here.

## Codex App Server (#809, #910)

The [migration design](../.g5/pr_ontology_diff_design/issue-809/ontology-plan.md),
[exec retirement design](../.g5/pr_ontology_diff_design/issue-910/ontology-plan.md)
and [execution contract](../.g5/ontology/features/chat.g5.md#g5.chat.supervisor)
make App Server the sole G5-managed Codex transport, for ordinary chat and formal
review alike. G5 never launches `codex exec`; §5 records its historical contract. An
attempt frozen on exec before retirement only re-reaches the older guest that admitted it
([retirement boundary](../.g5/ontology/features/chat.g5.md#g5.chat.codex-exec-retirement)).
Codex 0.164.0 was npm's latest stable release when the owner requested the upgrade.
Claude retains stream-json and its background/EOF rules. Manual `codex exec` use in a
workspace shell is unaffected.

`codex app-server --listen stdio://` is attempt-scoped. The handshake explicitly sets
`capabilities.experimentalApi=false`; all experimental/under-development runtime features
are disabled (including the enabled-by-default `instant_interrupt`), along with daemon
auto-start. Checked stable schemas and feature defaults live in
`tests/fixtures/codex-app-server/`; `scripts/test-codex-app-server.py --codex <binary>`
verifies their exact pin and native rollout-only resume against a synthetic Responses API.
Upstream labels the command experimental independently of the stable API opt-out.

Initial acceptance is the matching successful turn/start response with an inProgress
native turn id. Keep stdin open until matching turn/completed, close it, then await exit
and drain. Successful native completion and zero process exit are both necessary for
success. The existing supervisor retains the 15-second exit grace, five-second post-exit
drain, process custody, bounded harvest and finalization. Native completion alone never
publishes authoritative settlement or starts a queued successor. Coverage and unexecuted
acceptance obligations are tracked in the [matrix](../.g5/pr_ontology_diff_design/issue-809/coverage.md).

---

## 1. Pins

| CLI | npm package | Pin file | Pinned version | Verified on |
|---|---|---|---|---|
| Claude Code | `@anthropic-ai/claude-code` | `scripts/claude-code-cli-version` | `2.1.321` | 2.1.321 flag gate, all 352 catalog combinations and ten native lifecycle schedules (Linux x86_64, 2026-11-09, §3.10); 2.1.308 evidence (2026-10-25, §3.9), historical 2.1.285 Linux aarch64 recordings (§3.8) and 2.1.270 evidence remain labeled below |
| Codex | `@openai/codex` | `scripts/codex-cli-version` | `0.164.0` | 0.164.0 stable App Server schemas, runtime features, model/list, native completion and rollout-only resume (Linux x86_64, 2026-10-24); 0.159.3 (`exec --json` events, device login and `model/list`, §5); 0.153.3 for the recordings (npm-installed, macOS arm64) |

`2.1.270` was the newest release on npm on 2026-09-13 (`npm view
@anthropic-ai/claude-code time`); the design's flag set was originally checked
on 2.1.269 and re-verified on 2.1.270 as recorded below.

An explicitly scoped upgrade moved the pins from `2.1.270` and `0.153.3` to `2.1.285` (the npm `stable` tag when the upgrade was planned; `latest` was 2.1.286) and `0.159.3` (the npm `latest`). With it the offered Claude models gained Opus 5.5 and Sonnet 5.5, and the Codex list was re-checked (§5).

The owner's #809 follow-up upgraded Claude from 2.1.285 to **2.1.308**, npm's
most recent `stable` release on 2026-10-25. The
[evaluation made before changing the pin](../.g5/pr_ontology_diff_design/issue-809/ontology-plan.md#claude-upgrade-evaluation-before-changing-the-pin-2026-10-25)
covers native lifecycle, defaults, gateway/authentication, confinement and packaging.
That upgrade left the offered G5 model catalog unchanged; the native `haiku` alias
points to Haiku 5.5, while G5 continues to pass explicit catalog model ids.

The owner's Fable 5.2 request (2026-11-09) upgrades Claude to **2.1.321**, the release
that accepts `claude-fable-5-2` (npm's `latest`; `stable` was 2.1.311). The
[evaluation before changing the pin](../.g5/pr_ontology_diff_design/fable-5-2/ontology-plan.md#claude-upgrade-evaluation-before-changing-the-pin-2026-11-09)
covers model ids, headless exit codes, stream-json escaping, background work, routing
and permissions. With it the offered Claude catalog gains Fable 5.2, lists families
newest first by release day, defaults to Opus 5.5 and offers one plain id per family:
the `[1m]` variants stay resolver-known for saved selections but are no longer offered,
since every offered family runs its 1M window without the suffix.

The pin files are the single source; every installer reads them:

| Consumer | How it reads the pin | Flag gate |
|---|---|---|
| `infra/packer/g5-worker/scripts/40-install-claude.sh` (signed worker image) | `CLAUDE_CODE_VERSION` from `g5-worker.pkr.hcl` locals (`trimspace(file(...))`) | runs `scripts/check-claude-cli-flags.sh`, uploaded by a packer `file` provisioner |
| `scripts/stage-local-worker-runtime.sh` (local microVM guest runtime) | `CLAUDE_CODE_VERSION` defaulted from the file | mounts and runs the same script inside the staging container |
| `infra/docker/Dockerfile.g5d`, `Dockerfile.g5d-prebuilt` (Docker worker) | `COPY scripts/claude-code-cli-version` then `npm install -g @anthropic-ai/claude-code@<pin>` to `/usr/local/bin/claude` | copies and runs the same script; the two blocks are byte-identical |
| `scripts/image-test-gate/boot-and-assert.sh` (assert 7b) | reads the pin file, copies the script into the booted image | runs it against `/opt/g5/bin/claude` with the pin as the expected version |
| `scripts/worker-host-env.sh` | re-stages the guest runtime when `runtime/claude-code.version` differs from the pin | (staging script) |

`scripts/check-claude-cli-flags.sh <claude> [expected-version]` fails closed
when the binary's `--version` does not match the pin, when `--help` does not
advertise every flag in §2, or when `--max-turns` is not recognised (it is
hidden from `--help`, so the script feeds it a non-numeric value and expects
the option parser's rejection). `fake-claude` passes the same script.

**Catalog compatibility gate (both providers).** `just gen-model-catalog` emits
the UI catalog and `tests/fixtures/native-provider-catalog.json` from the same
Rust API/backend source, including exact CLI pins, route ids, native mode inventory
and per-model effort sets. Rust CI rejects stale/missing projections. Required E2E
step `native-provider-catalog` runs `scripts/test-native-provider-catalog.py
--claude <pin-binary> --codex <pin-binary>` against the staged production binaries.
It exhausts the model × native mode × per-model effort matrix on direct and
OpenRouter ids with synthetic loopback providers. Assertions cover native effective
modes, completed turns, process exit and actual outbound models/efforts. UI helpers,
API resolution and frozen worker-receipt validation exercise the same matrix.
The same required gate runs `scripts/test-native-claude-lifecycle.py` on the exact
Claude pin: native questions, allow/deny, cancellation, result-before-EOF,
background report/EOF, and idle/active interrupt. Only the model is scripted;
the native CLI executes tools, controls, background tasks and process shutdown.
When the CI harness runs as root, native probes use an unprivileged identity and
private home, matching the guest owner: Claude refuses bypass mode under root.
`GET /api/v1/instance/chat/models` projects this catalog too, including `agentProvider`,
route, modes, efforts, default effort and CLI version. Capacity and price fields are
nullable because these route/account-dependent facts are not owned by this catalog.
This verifies binary compatibility; it does not prove account-specific upstream
availability or enable native modes G5's dispatcher explicitly rejects.
Offered `modes` are the production dispatch subset; the native matrix also exercises
the separate `nativeModes` inventory. New API admission rejects modes outside the
dispatch subset; saved frozen receipts retain historical validation.

Claude 2.1.321 uses the catalog-derived `modelOverrides` mapping to recognize
older dotted OpenRouter ids and send their effort, as supported by the native
[model configuration contract](https://code.claude.com/docs/en/model-config#override-model-ids-per-version).
Ordinary and formal launches supply it without changing the frozen outbound slug.
Haiku 4.5 has no effort control: G5's `none` absence marker displays “Not supported”
read-only and omits `--effort`. New API resolutions reject out-of-set efforts for
known models; historical frozen resolutions retain their validation contract.
Codex Ultra remains selected in native turn metadata; ordinary Responses calls
encode it with the pin's model-specific multi-agent effort or highest non-Ultra
effort (`codex-rs/protocol/src/openai_models/reasoning_effort.rs` at 0.164.0).
The pin rejects `on-failure`; current catalog choices omit it and default to
`never/workspace-write`. New resolution of a retired configured/explicit mode fails
validation; frozen historical resolutions retain their reserved validation.

**Bumping the Claude pin.** Edit `scripts/claude-code-cli-version`, install the release and point the script at that binary, for example `npm install --global --prefix <dir> @anthropic-ai/claude-code@<new>` and then `scripts/check-claude-cli-flags.sh <dir>/bin/claude <new>`. `npx -y @anthropic-ai/claude-code@<new> --version` runs the new release, but a `which claude` after it finds whatever `claude` is already on `PATH`, not that release. Re-record the exact-pin native fixtures with `scripts/test-native-claude-lifecycle.py --claude <binary> --out tests/fixtures/agent-turn-native/claude-<new>` (see the fixture README); retain historical recordings with their actual versions. Update the current-pin assertions in `backend/g5d-agent/src/protocol.rs` and dispatcher replay path. Re-check the offered Claude models against the new pin, update §2 and §3 for any shape that changed, regenerate the catalog projections, and rebuild the worker images. The older `tests/fixtures/provider-frames/` recordings remain historical evidence.

---

## 2. Claude Code flags the runner passes

Invocation (design §8.3):

```
claude -p --output-format stream-json --input-format stream-json --verbose
       --include-partial-messages
       [--resume <provider_session_id> | --session-id <new uuid>]
       --permission-mode <mode>
       ( --permission-prompts host --permission-prompt-tool stdio | --permission-prompts none )
       [--mcp-config /work/.mcp.json] --strict-mcp-config
       [--settings /work/.claude/settings.json] [--add-dir <harness>] --setting-sources project
       --disallowedTools AskUserQuestion
       [--model <m>] [--effort <e>] [--max-turns <n>] [--append-system-prompt <text>]
```

The repository decides the file flags: its own `.mcp.json` and
`.claude/settings.json` are passed when present, and without `--mcp-config`
`--strict-mcp-config` loads no MCP server. `--add-dir` names the provided
harness (see the table). The CLI refuses a missing file with exit 1
before any frame (probed on 2.1.270):
`Error: Settings file not found: <path>` and
`Error: Invalid MCP configuration:` / `MCP config file not found: <path>`.
`--effort` carries the selected effort, else the model's catalog default, on
new and resumed turns.

Every flag below was verified on **2.1.270** (`claude --help`, the recordings in `tests/fixtures/provider-frames/claude/`, and the probes noted). On **2.1.285** (Linux aarch64, 2026-09-30) `claude --help` lists every flag below except the hidden `--max-turns`, which is still parsed; error texts and recorded behavior in the Notes column are from 2.1.270 unless a note names 2.1.285. Commander conventions apply: `--flag=value` works, variadic options swallow following non-option tokens, `--` ends option parsing, and an unknown option aborts with `error: unknown option '--x'` (exit 1) unless `--version`/`--help` is present, which short-circuit first.

On **2.1.308** (Linux x86_64, 2026-10-25) and again on **2.1.321** (Linux x86_64,
2026-11-09) the flag gate re-verifies the complete runner flag surface and hidden
`--max-turns`; the native catalog gate verifies the four catalog native modes and
each model's offered effort set on actual requests. The Notes column keeps its
historical evidence dates; §3.9 and §3.10 record current behavior.

| Flag | Verified on | Purpose for the runner | Notes |
|---|---|---|---|
| `-p`, `--print` | 2.1.270, 2.1.285 | Non-interactive mode | Required for every other flag here. |
| `--output-format stream-json` | 2.1.270, 2.1.285 | One JSON frame per stdout line | Choices `text`, `json`, `stream-json`. With `--print` it **requires `--verbose`**, else `Error: When using --print, --output-format=stream-json requires --verbose`. |
| `--input-format stream-json` | 2.1.270, 2.1.285 | User message on stdin as JSON | Choices `text`, `stream-json`. The process stays alive after the turn until stdin reaches EOF. |
| `--verbose` | 2.1.270, 2.1.285 | Required with stream-json output | |
| `--include-partial-messages` | 2.1.270, 2.1.285 | `stream_event` deltas for `chat.chunk` | Adds the frames in §3.4. |
| `--resume <id>` | 2.1.270, 2.1.285 | Continue a provider session | Recorded in `claude/resume.jsonl`; the init frame carries the resumed `session_id`. An unknown id fails (`No conversation found with session ID: <id>`, exit 1; observed on a temp config dir). The runner treats that refusal before any frame as a rejected resume, and the adapter starts a new session with the chat history (design §8.4). |
| `--session-id <uuid>` | 2.1.270, 2.1.285 | Runner-generated id for a new session | Must be a UUID (`Error: --session-id must be a valid UUID`). Echoed in every frame's `session_id`. |
| `--permission-mode <mode>` | 2.1.270, 2.1.285 | Chat: server-frozen mode; other tasks: `bypassPermissions` | Choices on 2.1.270 and 2.1.285: `acceptEdits`, `auto`, `bypassPermissions`, `manual`, `dontAsk`, `plan` (plus `default`, accepted and recorded in `permission-allow.jsonl`). |
| `--permission-prompts host` | 2.1.270, 2.1.285 | Route permission prompts to the prompt tool | Choices `host` (default), `none` (auto-deny). The runner passes `host` only under the prompting policy. |
| `--permission-prompt-tool stdio` | 2.1.270, 2.1.285 | Answer permission prompts on stdio | Prompts arrive as `control_request` frames (`can_use_tool`) that the runner answers on stdin. |
| `--mcp-config <configs...>` | 2.1.270, 2.1.285 | Load the repository's MCP servers | Variadic; each value is a JSON file path or an inline JSON string with a top-level `mcpServers` map (`{"command","args","cwd","env"}` per server). A missing file exits 1 before any frame (`Error: Invalid MCP configuration:` / `MCP config file not found: <path>`). |
| `--strict-mcp-config` | 2.1.270, 2.1.285 | Ignore user/project MCP settings | |
| `--settings <file-or-json>` | 2.1.270, 2.1.285 | Load the repository's settings | Omitted when the repository has no settings file; a missing file exits 1 before any frame (`Error: Settings file not found: <path>`). |
| `--add-dir <directories...>` | 2.1.270, 2.1.285 | Read the provided harness | The launcher lays the harness out in `$HOME/.g5/harness`, outside the checkout: the root `AGENTS.md` as `CLAUDE.md`, and the skills under `.claude/skills` and `.agents/skills`, where the instructions link to them. `CLAUDE_CODE_ADDITIONAL_DIRECTORIES_CLAUDE_MD=1` makes the CLI load that `CLAUDE.md`; `@` imports there are not followed (probed on 2.1.245). Variadic, so an option follows its value. |
| `--setting-sources <sources>` | 2.1.270, 2.1.285 | Only `project` settings | Comma-separated subset of `user,project,local`. |
| `--disallowedTools <tools...>` | 2.1.270, 2.1.285 | Remove `AskUserQuestion` | Variadic; alias `--disallowed-tools`. |
| `--append-system-prompt <text>` | 2.1.270, 2.1.285 | Runtime boundary text | |
| `--model <m>` | 2.1.270, 2.1.285 | Model | Alias or full name; echoed in `system/init.model`. |
| `--effort <level>` | 2.1.270, 2.1.285 | Selected effort, else the model's default | Choices `low`, `medium`, `high`, `xhigh`, `max` (unchanged on 2.1.285). An unknown value prints `Warning: Unknown --effort value '<v>' — ignoring it and using the default effort.` and the turn continues (probed). |
| `--max-turns <n>` | 2.1.270, 2.1.285 | Agentic turn cap | **Parsed but absent from `--help` on 2.1.270 and 2.1.285.** Proven by `claude --max-turns not-a-number --version`, which prints `error: option '--max-turns <turns>' argument 'not-a-number' is invalid. must be a number` on both releases and exits 1 on 2.1.270 (an unknown option would print the version). |

Every new Claude attempt freezes the `native_mid_turn_steering` protocol capability.
The runner does not pass `--replay-user-messages`; a completed stdin write/flush is
delivery confirmation. Existing attempts without the capability keep queueing.

Also available and relevant: `--allowedTools`, `--tools`, `--fork-session`,
`--no-session-persistence`, `--include-hook-events`, `--forward-subagent-text`,
`--json-schema`, `--max-budget-usd`, `--bare`, `--restricted`.
The ordinary runner does not pass these. Formal approval reviews use the restricted invocation below.

### Formal approval invocation (contract v1)

Formal reviews use an isolated HOME and working directory outside reviewed
workspace ancestors, the frozen v1 role
prompts and sole review skill, and a closed MCP server exposing `read_review`
and `write_report`. They do not load repository settings or instructions.
Claude adds `--tools ""`, `--allowedTools
mcp__formal_review__read_review,mcp__formal_review__write_report`,
`--system-prompt <frozen system>`, `--permission-mode dontAsk`,
`--permission-prompts none`, `--setting-sources ""`, strict MCP configuration,
and `--settings '{"disableAllHooks":true}'`. The sanitized formal-review
environment forces `ENABLE_TOOL_SEARCH=false` so the native CLI's custom-endpoint
tool-search default cannot add tools outside that assigned surface.

Codex uses `app-server --listen stdio:// --strict-config` in the fresh private
`CODEX_HOME` (only the selected credential and the sole skill), with
`approval_policy="never"`, `project_doc_max_bytes=0`, a named permissions profile
extending `:read-only` with `filesystem={":root"="deny"}`, the frozen system instruction
file, disabled bundled skills and automatic skill catalog, direct MCP tool exposure,
and explicit feature disabling for shell, delegation, apps, plugins,
hooks, shell/undo snapshots, browser/computer/image tools, memories, skill
discovery and goals.
The two assigned MCP tools each have `approval_mode="approve"`; no other MCP
server is loaded. App Server has no exec-only `--ephemeral`/`--ignore-*` flags:
`thread/start` sets `ephemeral: true` and omits `sandbox` (which would replace the
root-denying profile), and the adapter requires an ephemeral, read-only thread with
no loaded instruction sources before sending the turn. The thread is never resumed
([launch contract](../.g5/ontology/features/agent_approvals.g5.md#g5.agent-approval.codex-launch)).

Historical installation flag gates verified this surface on Claude 2.1.285 and
Codex 0.159.3 on macOS arm64 on October 18, 2026. Those results do not verify the
current pins, Claude 2.1.321 and Codex 0.164.0. Current-pin native
confinement passes are recorded separately in the
[migration evidence](../.g5/pr_ontology_diff_design/issue-809/coverage.md#claude-stable-upgrade-2026-10-25)
and enforced by the required `formal-review-native-clients` Cloud Build gate.
`scripts/check-codex-cli-flags.sh` also asks the native feature registry to
validate every disabled feature. Offline `app-server --print-startup-config` probes
load the formal configuration without serving and require each invalid
confinement-config value to fail that load; this detects silently ignored keys
without making a model request. Docker, local runtime staging, signed-image
installation and the boot gate all run the checkers; release bundles include
both scripts. The opt-in process test
`formal_review_real_native_clients_enforce_tools_and_report_transport` runs
both pinned native clients against a scripted local model endpoint. It checks
actual tool advertisement, denies an unadvertised shell/write attempt and an
unassigned report path (plus a native Codex patch context read under its deny-all filesystem
profile), then reads frozen Git objects and writes the assigned
report for both verdicts and every retained assignment schema, under hostile workspace
and owner-home configuration. Its Codex leg asserts the exact native tool surface and
refuses shell, delegation and inherited MCP servers.
`formal_review_real_codex_cancellation_and_missing_report_settle_no_verdict` covers
owner Stop and a turn without a report. These verify native transport and
confinement, not model judgment or a live provider API.

Reproduce with both pinned executables in a private bin directory:

```sh
G5_NATIVE_PROVIDER_TEST_BIN_DIR=/absolute/pinned/bin cargo test -p g5d --no-default-features --test dispatch_modules formal_review_real_native -- --ignored
```

The role prompts and installed skill are versioned under
`backend/g5-contracts/src/prompts/formal_approval/v1/`. The SHA-256 manifest
`agent-approval-contracts/v1.json` and
`scripts/check-agent-approval-contract.py` prevent published v1 assets from
changing in place. New contracts retain old assets so frozen retries remain
replayable. The exact-byte invocation validation deliberately fails closed if
an assignment is paired with another contract version.

### 2.1 Process contract (observed)

The additional 2.1.285 steering recordings (`claude/steer-*.jsonl` and
`steering-recording.json`) use the real CLI with a scripted local model endpoint,
via `tools/fake-provider-cli/record.sh claude-steering`. Caller-supplied user
UUIDs are replayed unchanged with `--replay-user-messages`. Input queued during
a tool call is replayed after its result and consumed in the same turn; input
during final text starts another turn in the same process. Native resume retains
the consumed steering instruction. These are protocol checks, not live-model QA.

The original replay probes are retained as protocol evidence. The production
contract uses completed stdin write/flush as delivery confirmation; it requires no
user-message echo or consumption tracking. The owner accepts loss if Stop or VM loss
prevents Claude from reading a flushed message, and accepts a possible duplicate when
an RPC has no definite answer and the input remains queued for the next turn.

The `steer-tool-flush` and `steer-text-flush` recordings additionally run without
`--replay-user-messages`, close stdin immediately after the steering write, and verify
that the model receives the instruction and Claude completes one or two native turns,
respectively. Refresh with `record-steering.py --flush-only --claude <pinned-cli>
--pin 2.1.285 --out tests/fixtures/provider-frames/claude`.

- **stdin** (`--input-format stream-json`): one JSON object per line. A user
  turn is `{"type":"user","message":{"role":"user","content":<string | [content blocks]>}}`
  where blocks are Anthropic Messages content blocks (`{"type":"text","text":…}`,
  `{"type":"image","source":{…}}`). The turn starts on the first user message.
  The process accepts further user messages in the same attempt, including messages buffered before EOF. The runner keeps stdin open for native control frames and while Claude reports background tasks or sub-agents (§3.8). Every new Claude attempt also accepts eligible ordinary text input through `modules.input.steer`. Steering adds no extra stdin hold: closing requires Claude to be between turns with no background work, pending request or pending write. Already queued input may start another native turn after EOF. Attachments and background-wait input retain the queue fallback.
- **stdout**: one JSON object per line (§3). Nothing else is written to stdout.
- **stderr**: empty on a successful turn (observed on all six recordings).
- **exit code**: 0 when the turn completed, including turns with denied
  permissions. An authentication failure still produces `system/init`,
  `system/status`, an `assistant` frame with `error: "authentication_failed"`
  and `is_api_error_message: true`, and a `result` with `is_error: true`,
  `terminal_reason: "api_error"`, then exit 1 (observed: "Not logged in ·
  Please run /login").
- **session files** (`CLAUDE_CONFIG_DIR`, design §8.4): the transcript is
  `projects/<project key>/<session id>.jsonl`, where the project key is the
  absolute cwd with every character that is not ASCII-alphanumeric or `-`
  replaced by `-` (observed: `/private/tmp/x/G5-Lean-wt-P3.1` →
  `-private-tmp-x-G5-Lean-wt-P3-1`). `--resume` needs that file under the same
  config dir and cwd. Subagent transcripts live in
  `projects/<project key>/<session id>/subagents/`. The runner harvests these
  files into Postgres (`chat_session_files`) after every turn and restores
  them before a turn that resumes a session this host lacks (design §8.4).
- **environment the runner sets** (design §8.3): `CLAUDE_CONFIG_DIR`,
  `ANTHROPIC_BASE_URL` (selected provider endpoint), and the selected native
  API-key, bearer or OAuth environment credential.
  `MCP_TOOL_TIMEOUT` (milliseconds) bounds an MCP tool call; the runner does
  not set it, because G5 registers no MCP server for Claude Code.
- **MCP startup**: on launch the CLI runs `initialize` (protocolVersion
  `2025-11-25`, `clientInfo.name: "claude-code"`), `notifications/initialized`
  and `tools/list` against every configured server and reports each as
  `{"name","status":"connected"}` in `system/init.mcp_servers` (observed).
  MCP tools are **deferred** on 2.1.270 (observed with the retired
  `g5-tools` server of §4): the model first calls the built-in
  `ToolSearch` tool with `select:mcp__g5-tools__ask_user` and receives a
  `tool_result` whose content is `[{"type":"tool_reference","tool_name":"mcp__g5-tools__ask_user"}]`
  before it can call the MCP tool (observed in `mcp-ask-user.jsonl`). The
  runner must treat that pair as an ordinary tool call.

---

## 3. Claude `stream-json` frames the runner relies on

All shapes observed on 2.1.270 in `tests/fixtures/provider-frames/claude/`, except those marked 2.1.285, which come from the exact-pin native recordings in `tests/fixtures/agent-turn-native/claude/` (§3.8).
Every frame carries `type`; every frame after `system/init` carries
`session_id` and most carry `uuid`. Unknown frame types and unknown keys must
be ignored: the CLI adds frames between releases (`system/post_turn_summary`
and `system/thinking_tokens`, for example).

### 3.1 `system` frames

- `{"type":"system","subtype":"init","cwd","session_id","tools":[…],"mcp_servers":[{"name","status"}],"model","permissionMode","slash_commands","terminal_slash_commands","apiKeySource","claude_code_version","output_style","agents","skills","plugins","capabilities","analytics_disabled","product_feedback_disabled","uuid","memory_paths","messaging_socket_path","fast_mode_state","fast_mode_disabled_reason"}`
  — first frame of every turn, resumed or not, including a follow-up turn Claude starts itself (2.1.285, §3.8). The runner records
  `session_id` from it (it equals `--session-id` or `--resume`) and can check
  `mcp_servers` for `g5-tools: connected`. The recorded fixtures drop
  `memory_paths` and `messaging_socket_path` and empty the account-specific
  `skills`/`agents`/`slash_commands`/`plugins` lists.
- `{"type":"system","subtype":"status","status":"requesting","session_id","uuid"}`
  — before each model request.
- `{"type":"system","subtype":"thinking_tokens","estimated_tokens","estimated_tokens_delta","session_id","uuid"}`
  — progress while the model thinks.
- `{"type":"system","subtype":"post_turn_summary","summarizes_uuid","status_category","status_detail","needs_action","uuid","session_id"}`
  — may appear after a turn (seen in a probe, not in the fixtures).
- `{"type":"system","subtype":"background_tasks_changed","tasks":[{"task_id","task_type":"local_bash"|"local_agent","description"}],"uuid","session_id"}` — 2.1.285: the whole live set of background tasks and sub-agents, replacing the previous set; `tasks` is empty when none remain.
- `{"type":"system","subtype":"task_started","task_id","tool_use_id","description","task_type","is_backgrounded",…}` — 2.1.285: a background task started; a sub-agent adds `subagent_type` and `prompt`.
- `{"type":"system","subtype":"task_progress","task_id","description","usage","last_tool_name",…}` — 2.1.285: progress of a background sub-agent.
- `{"type":"system","subtype":"task_updated","task_id","patch":{"status":"completed"|"killed",…},…}` — 2.1.285.
- `{"type":"system","subtype":"task_notification","task_id","status":"completed"|"stopped","output_file","summary",…}` — 2.1.285: a background task ended.

### 3.2 `assistant` frames

```
{"type":"assistant",
 "message":{"model","id":"msg_…","type":"message","role":"assistant",
            "content":[<block>…],"container":null,"stop_reason":null|"end_turn"|…,
            "stop_sequence":null,"stop_details":null,"usage":{…},
            "input_transformations":[],"diagnostics":null,"context_management":null},
 "parent_tool_use_id":null,"session_id","uuid","timestamp","request_id",
 "wire_tool_inputs":{<tool_use id>:<input>},          // only with tool_use blocks
 "tool_use_meta":[{"id","display_name","server_display_name"}]}   // only for MCP tools
```

One `assistant` frame is emitted per completed content block, so a turn with
thinking, a tool call and text yields several `assistant` frames sharing one
`message.id`. Blocks: `{"type":"thinking","thinking","signature"}`,
`{"type":"text","text"}`, `{"type":"tool_use","id":"toolu_…","name","input","caller":{"type":"direct"}}`.
MCP tools are named `mcp__<server>__<tool>`.

### 3.3 `user` frames (tool results)

```
{"type":"user",
 "message":{"role":"user","content":[{"tool_use_id","type":"tool_result","content":<string | [blocks]>,"is_error":true?}]},
 "parent_tool_use_id":null,"session_id","uuid","timestamp",
 "tool_use_result":<tool-specific>,
 "tool_result_meta":[{"id","non_execution_kind":"permission-rule"}]}   // denials only
```

Observed contents: `Write` → string `File created successfully at: <path> …`
with `tool_use_result: {"type":"create","filePath","content","structuredPatch":[],"originalFile":null,"userModified":false}`;
MCP tool → the server's content blocks (`[{"type":"text","text":"yes"}]`)
mirrored in `tool_use_result`; `ToolSearch` → `[{"type":"tool_reference","tool_name"}]`;
denied permission → `content: "<deny message>"`, `is_error: true`,
`tool_use_result: "Error: <deny message>"`, `tool_result_meta` as above.
`is_error` is absent on success.

### 3.4 `stream_event` frames (`--include-partial-messages`)

`{"type":"stream_event","event":<Anthropic streaming event>,"session_id","parent_tool_use_id":null,"uuid"}`
with `event.type` one of `message_start` (carries `message` skeleton and
`ttft_ms` on the frame), `content_block_start` (`index`, `content_block` of
type `thinking`, `text` or `tool_use`), `content_block_delta` (`delta.type`
`thinking_delta`, `signature_delta`, `text_delta` with `text`, or
`input_json_delta` with `partial_json`), `content_block_stop`,
`message_delta` (`delta.stop_reason`, `usage`), `message_stop`. `chat.chunk`
deltas come from `text_delta`.

### 3.5 `rate_limit_event`

`{"type":"rate_limit_event","rate_limit_info":{"status","resetsAt","rateLimitType","overageStatus",…},"session_id":…}`
— informational. Since the 2.1.308 pin the frame includes optional
`unifiedWindows.five_hour` / `seven_day` objects with fractional `utilization`
and epoch-second `resetsAt`, extracted from ordinary response headers. These
feed the live subscription meter without entering the execution journal.
Missing windows are supported; status or a surpassed threshold is not a
percentage. See [investigation and design](../.g5/pr_ontology_diff_design/subscription-meters/ontology-plan.md).

### 3.6 `result` frame (last frame of a turn)

```
{"type":"result","subtype":"success","is_error":false,"result":"<final assistant text>",
 "session_id","num_turns","duration_ms","duration_api_ms","total_cost_usd",
 "usage":{"input_tokens","output_tokens","cache_creation_input_tokens","cache_read_input_tokens",…},
 "modelUsage":{<model>:{"inputTokens","outputTokens","costUSD",…}},
 "permission_denials":[{"tool_name","tool_use_id","tool_input"}],
 "terminal_reason":"completed"|"api_error","stop_reason","api_error_status","uuid",…}
```

Every turn, including a follow-up turn (§3.8), ends with its own `result`. On 2.1.285 it carries `result_index` (0 for the first turn); its `usage` tokens and `duration_ms` are per turn, while `total_cost_usd` and `modelUsage` are cumulative for the session. The attempt outcome needs exit code 0 and `!is_error` on every turn's `result`, with one exception: a turn that the owner's End interrupt aborted is End's effect, not a failure. Such a turn is recognized by its `error_during_execution` result with an `aborted_*` `terminal_reason`, and only when Claude did not refuse the interrupt. The exit status 1 that Claude then reports at EOF is End's effect too (§3.8, [Agent API §10.1](agent-api-surface-v9.md#101-transport-and-launch-target)). Beyond this, nothing else is inspected (design §8.3). `result`, `usage` summed across turns, the latest `total_cost_usd` and `permission_denials` feed the final assistant message.

### 3.7 Frame order of a tool turn (from `write-file.jsonl`)

`system/init` → `system/status` → `stream_event message_start` → thinking
deltas → `assistant{thinking}` → `stream_event content_block_start{tool_use}`
→ `input_json_delta`… → `assistant{tool_use}` → `content_block_stop` →
`user{tool_result}` → `message_delta` → `message_stop` → `rate_limit_event`
→ `system/status` → … `assistant{text}` … → `result`.

### 3.8 Background work and follow-up turns (2.1.285)

Recorded on 2.1.285 (Linux aarch64, 2026-09-30) in `tests/fixtures/agent-turn-native/claude/` with `tests/fixtures/agent-turn-native/record.py`; the frames are listed in §3.1.

- `background-bash-report`: a Bash job started in the background is still running at the first `result`. With stdin open, Claude later emits `background_tasks_changed` with an empty set, `task_updated` (`completed`) and `task_notification`, then reports the job in its own follow-up turn: a second `system/init` with the same `session_id` and a second `result` with `result_index` 1.
- `background-bash-eof`: the same job with stdin closed at the first `result`. Claude stops it about 5 s after EOF (`task_updated` `killed`, `task_notification` `stopped`) and exits 0.
- `background-agent-interrupt`: a background sub-agent (`task_type` `local_agent`). While Claude is idle the host writes `{"type":"control_request","request_id":"…","request":{"subtype":"interrupt"}}`; Claude stops the sub-agent, answers `{"type":"control_response","response":{"subtype":"success","request_id":"…","response":{"still_queued":[]}}}` and starts no report turn. After EOF it exits 0 within about a second.
- `background-agent-interrupt-turn`: the same interrupt while Claude's turn runs a long foreground command next to a background sub-agent. Claude answers at once, stops the sub-agent and the foreground command, ends the turn with an `error_during_execution` result whose `terminal_reason` is `aborted_tools` (probes also saw `aborted_streaming`) and exits 1 at EOF. Claude reports the foreground command with `task_started` and `task_notification` frames that carry `is_backgrounded: false`; it is not background work. Probes show a background Bash job survives the interrupt and stops at EOF.

The adapter (`backend/g5d-agent`) keeps stdin open after a `result` while Claude's background set is non-empty, and closes it once Claude is between turns, the set is empty and no native request is pending, after a 3-second settle window once background work was seen. The owner's End uses the `interrupt` above, at once between turns and after a running turn has had up to 10 seconds; the aborted turn and Claude's exit status 1 after it are End's effect ([Agent API §10.1](agent-api-surface-v9.md#101-transport-and-launch-target)).

### 3.9 Lifecycle on 2.1.308 (2026-10-25)

`tests/fixtures/agent-turn-native/claude-2.1.308/` contains sanitized real-binary
recordings produced by the required native lifecycle gate on Linux x86_64, with
a synthetic loopback Messages API; §3.10 records the current pin and the one
behavior that changed since. The ten schedules recheck the control envelopes,
result indices, question/permission responses, cancellation, background report
turns, idle and active interruption and successful/error process exit.

The changed EOF behavior is material: a background Bash command now finishes after
stdin closes instead of being killed after five seconds. The recorded 15-second
command writes its marker and exits successfully about 15 seconds after EOF. G5
already holds input open while the native background set is non-empty; its normal
settling rule remains valid. Idle interrupt stops the subagent and exits 0; active
interrupt produces `error_during_execution` / `aborted_tools` and exits 1 at EOF.
The production dispatcher/supervisor background replay uses these current frames;
historical 2.1.285 protocol tests remain as backward compatibility evidence.

SIGINT during a pending native question emits its matching cancellation and may
exit 0 without a `result`. The cancellation schedule accepts that interrupted
shape and rejects successful results; every normal schedule still requires native
completion. G5's forced Stop/Cancel outcome and bounded reap own that interruption,
so its zero exit cannot establish ordinary success.

Restricted formal reviews force `ENABLE_TOOL_SEARCH=false` in their sanitized
environment. The new custom-endpoint default otherwise adds a deferred tool
placeholder even when `--tools` excludes ToolSearch. The strict native formal gate
still requires exactly the assigned `read_review` and `write_report` MCP tools.

---

### 3.10 Current-pin lifecycle (2.1.321, 2026-11-09)

`tests/fixtures/agent-turn-native/claude-2.1.321/` holds the same ten schedules
re-recorded on the current pin (Linux x86_64, synthetic loopback Messages API). All
ten pass; the production dispatcher/supervisor background replay and the protocol
current-pin tests read these frames. Question, permission, result-before-EOF,
background report, 15-second EOF and both interrupt shapes match §3.9.

One shape changed: SIGINT during a pending native question now emits an
`error_during_execution` result (`is_error: true`, `terminal_reason` `aborted_*`)
before exiting 0, where 2.1.308 exited without a result. Both are an interrupted
turn: the lifecycle gate and `protocol.rs` accept either shape and reject any
successful result; G5's forced Stop/Cancel outcome and bounded reap stay
authoritative, and the zero exit never establishes ordinary completion.

The native catalog gate on 2.1.321 exercised all 352 Claude model × native mode ×
effort combinations, including `claude-fable-5-2` / `anthropic/claude-fable-5.2`,
the dotted OpenRouter slugs with their `modelOverrides` and the no-longer-offered
`[1m]` ids absent from the matrix (they remain resolver aliases for saved
selections). The [pin evaluation](../.g5/pr_ontology_diff_design/fable-5-2/ontology-plan.md#claude-upgrade-evaluation-before-changing-the-pin-2026-11-09)
lists the 2.1.309–2.1.321 changes considered.

## 4. Claude ↔ MCP: recorded shapes (historical)

**Not a live G5 contract.** G5 registers no review-comment MCP server for either
provider (§5.1). Claude questions and permission
prompts travel over stdio (`--permission-prompt-tool stdio`, §2). This section
documents the recordings kept in `tests/fixtures/provider-frames/`, made
against the retired `g5-tools` server, so the fixtures and the fake CLIs (§7)
stay readable.

### 4.1 `ask_user` (retired)

The model calls `mcp__g5-tools__ask_user` like any MCP tool. On the wire
(observed in `mcp-ask-user.mcp-requests.jsonl`):

```
{"method":"tools/call","params":{"name":"ask_user","arguments":{"question":"Proceed?","options":["yes","no"]},
 "_meta":{"claudecode/toolUseId":"toolu_…","progressToken":2}},"jsonrpc":"2.0","id":2}
```

The server blocks until the answer is committed and returns
`{"content":[{"type":"text","text":"<answer>"}],"isError":false}`; the CLI
renders it as the `user` frame in §3.3. The call waits up to
`MCP_TOOL_TIMEOUT` milliseconds.

### 4.2 `request_permission` (retired `--permission-prompt-tool` target)

When the permission mode would prompt, the CLI calls the tool named by
`--permission-prompt-tool` **before** running the tool (observed in
`permission-allow.mcp-requests.jsonl`):

```
{"method":"tools/call","params":{"name":"request_permission",
 "arguments":{"tool_name":"Write","input":{"file_path":"…/hello.txt","content":"hello"},"tool_use_id":"toolu_…"},
 "_meta":{"claudecode/toolUseId":"toolu_…","progressToken":2}},"jsonrpc":"2.0","id":2}
```

The tool result's text must be a JSON object:

- allow: `{"behavior":"allow","updatedInput":<input to run with>}` — the tool
  runs; the frames are indistinguishable from `bypassPermissions`
  (`permission-allow.jsonl` matches `write-file.jsonl`).
- deny: `{"behavior":"deny","message":"<reason>"}` — no tool runs; the CLI
  emits the denial `user` frame of §3.3 with the message as `content`, and the
  `result` lists the denial in `permission_denials`. The turn still ends with
  `is_error: false`, exit 0 (`permission-deny.jsonl`).

The permission call is not visible as a `tool_use` block; only its effect is.
Under `bypassPermissions` the tool is never called. With
`--permission-prompts none`, or a prompting mode without a prompt tool, the
CLI denies on its own.

---

## 5. Codex `exec --json` contract (historical)

G5 no longer launches this. It documents attempts reserved before retirement and the
item vocabulary that `CodexFrameMapper` still translates for App Server.

Invocation (design §8.3; `runner::codex::build_argv` builds it):

```
codex exec [resume <provider_session_id>] --json --skip-git-repo-check
      --output-last-message <tmp> [--model <m>]
      -c approval_policy="never" -c sandbox_mode="workspace-write" [-c model_reasoning_effort="<e>"]
      [-i <attachment>...]                      # prompt on stdin, then EOF
env: CODEX_HOME=/home/agent/.codex (the provided harness, §5.1)
```

Observed on codex-cli 0.153.3 (`tests/fixtures/provider-frames/codex/`):

- **stdin**: with no positional prompt (or `-`) the CLI prints
  `Reading prompt from stdin...` on stderr and reads until EOF before
  starting. `-i` values are file paths, repeatable and comma-separable.
- **stdout**: one JSON object per line. Frame types and shapes:
  - `{"type":"thread.started","thread_id":"<uuid v7>"}` — first frame; the
    provider session id. `exec resume <id>` re-emits the same id.
  - `{"type":"turn.started"}`
  - `{"type":"item.started","item":{…}}` / `{"type":"item.completed","item":{…}}`
    with `item.id` = `item_<n>` (numbered from 0 per turn) and `item.type`:
    - `agent_message`: `{"id","type","text"}` (only `item.completed`).
    - `command_execution`: `{"id","type","command","aggregated_output","exit_code":null|n,"status":"in_progress"|"completed"|"failed"}`.
    - `mcp_tool_call`: `{"id","type","server","tool","arguments","result":null,"error":null,"status":"in_progress"}` at start; at completion `"result":{"content":[…],"structured_content":null},"error":null,"status":"completed"` or `"result":null,"error":{"message":"…"},"status":"failed"`.
    - from the current sidecar, not recorded: `reasoning`, `file_change`, `error` items; `turn.failed` with an `error` object; `exec_approval_request` / `apply_patch_approval_request` events (mapped to `CodexApprovalUnsupported` because `exec` cannot answer them).
  - `{"type":"turn.completed","usage":{"input_tokens","cached_input_tokens","cache_write_input_tokens","output_tokens","reasoning_output_tokens"}}` — last frame; the current sidecar also accepts `task_complete`/`turn_complete` spellings from older releases.
- **`--output-last-message <file>`**: the final `agent_message` text is written
  to the file at the end of the turn (observed content `OK`).
- **exit code**: 0 when the turn completed. `exec resume <unknown id>` writes
  no frames, prints `Error: thread/resume: thread/resume failed: no rollout found for thread id <id> (code -32600)`
  to stderr and exits 1 (`recording.json`, scenario `codex/resume-missing`).
  The runner treats that refusal as a rejected resume, as for Claude (§2).
- **session files** (`CODEX_HOME`): `sessions/YYYY/MM/DD/rollout-<timestamp>-<thread id>.jsonl`.
  The runner harvests and restores the rollout like the Claude transcript
  (design §8.4).

Re-verified on codex-cli 0.159.3: a command turn's `exec --json` stream had the same event types (`thread.started`, `turn.started`, `item.started`/`item.completed` with `command_execution` and `agent_message`, `turn.completed`) and the same `turn.completed.usage` keys as 0.153.3; `pinned_cli_exec_stream_maps_to_one_completed_turn` in `backend/g5d-worker/src/runner/codex.rs` replays that stream. `codex login --device-auth` prints the same verification URL `https://auth.openai.com/codex/device` and a code line, now with ANSI color and indentation (`parses_the_pinned_cli_device_challenge_layout` in `backend/g5d-agent-turn/src/codex_auth.rs`), and `codex login status` prints `Logged in using ChatGPT`. `codex app-server` `model/list` offers `gpt-6.1-sol` (the default), `gpt-6-astra`, `gpt-6-sol`, `gpt-6-luna`, `gpt-5.6-sol`, `gpt-5.6-terra`, `gpt-5.6-luna` and `gpt-5.5`; it no longer offers `gpt-5.4`, `gpt-5.4-mini` or `gpt-5.3-codex-spark`.

### 5.1 Codex harness and MCP servers

Before subscription login or execution, the native `agent.turn` launcher installs the provided harness into the owner-scoped `$HOME/.codex`: the root `AGENTS.md` and each skill under `skills/<name>`. In workspace VMs its source is `/opt/g5/assets/harness`, mounted from the worker runtime bundle; direct invocations and tests may set `G5_HARNESS_DIR`. Only the provided files and G5's own `g5-tools` entry change; other owner skills, settings, MCP servers, credentials and session files are kept, and nothing is written into customer repositories. Files are published atomically so concurrent turns never see partially written files. A home left by the retired `codex-g5` profile loses that profile's G5 skills on the next launch, and its retired `g5-tools` entry is removed. A missing or incomplete harness fails the turn with a durable `harness_unavailable` notice before the provider starts. Fake provider turns do not read the harness.

The launcher registers no G5 review-comment MCP server. It removes the retired G5 server from retained homes while preserving unrelated MCP configuration. GitHub issue and PR commenting continue through their existing `g5d` commands.

A provider exit without a structured result produces a durable error notice. Startup configuration errors receive fixed diagnostic guidance; arbitrary stderr is kept out of the user-facing notice because it may contain credentials. Chat uses the nested agent outcome, not the module's outer exit status, and renders an error even when only lifecycle rows or an empty assistant error message survived.

`[mcp_servers.<name>]` in `config.toml` takes `command`, `args`, `cwd`,
`env`, `startup_timeout_sec` (default 10), `tool_timeout_sec` (default 60),
`required`. Codex passes only the `env` table to the server (the process
environment is not inherited). Its client sends `initialize` with
protocolVersion `2025-06-18`, `clientInfo.name: "codex-mcp-client"`, then
`tools/list`, then `tools/call` whose `_meta` carries `callId`, `threadId`,
`itemId` and an `x-codex-turn-metadata` object (`session_id`, `thread_id`,
`turn_id`, `model`, `reasoning_effort`, `sandbox_mode`, …).

**Under `approval_policy = "never"` an MCP tool call fails without prompting
unless the tool is pre-approved**: the recording `mcp-ask-user-unapproved.jsonl`
shows `status: "failed"`, `error.message: "MCP tool call requires approval, but approval policy is never"`.
With `[mcp_servers.<name>.tools.<tool>] approval_mode = "approve"` the call
succeeds (`mcp-ask-user.jsonl`) and waits up to `tool_timeout_sec`.

Codex never answers native approval requests (design §8.3): the runner refuses
prompting presets (P3.2/R5), the App Server adapter rejects any native request and
fails the turn, and `fake-codex` never emits approval frames.

---

## 6. `G5_CLAUDE_PATH` and `G5_CODEX_PATH`

Both variables already exist; they name the provider binary to spawn.

| Variable | Resolution today | Set by |
|---|---|---|
| `G5_CLAUDE_PATH` | `claude_code::resolve_claude_path`: explicit provider config, then `G5_CLAUDE_PATH`, then `claude` on `PATH`, then `~/.local/bin/claude`. In fake-provider mode only a binary named `fake-claude` resolves (a pin that names one, else the fake beside `g5d` or on `PATH`); a missing fake fails the launch instead of reaching the real CLI | microVM guest: `/opt/g5/bin/claude` (`g5d-host` `GUEST_CLAUDE_PATH`); Docker worker: `/usr/local/bin/claude` when present, else the Agent SDK's native binary (`infra/docker/g5d-provider-env.sh`) |
| `G5_CODEX_PATH` | `codex::resolve_codex_path`: explicit config, then `G5_CODEX_PATH`, then `codex` on `PATH`. In fake-provider mode only a binary named `fake-codex` resolves, and a missing fake fails closed | microVM guest: `/opt/g5/bin/codex`; Docker worker: `/usr/local/bin/codex` |

Image gates run `<claude> --version`, `<codex> --version` and
`<codex> app-server --help` (must mention `--listen`); both fakes satisfy them. Tests point the variables at the fakes:

```
G5_CLAUDE_PATH=target/debug/fake-claude G5_CODEX_PATH=target/debug/fake-codex \
G5_FAKE_SCENARIO=tests/fixtures/provider-frames/claude/text-only.jsonl …
```

(Cargo integration tests use `env!("CARGO_BIN_EXE_fake-claude")` after adding
`fake-provider-cli` as a dev-dependency or building it in the same workspace.)
`.env.example` documents both variables.

---

## 7. The fake CLIs (`tools/fake-provider-cli`)

Crate `fake-provider-cli` builds three binaries. Run `cargo test -p
fake-provider-cli` for the unit tests, the black-box tests in `tests/cli.rs`
and the fixture replay test.

| Binary | Stands in for | Notes |
|---|---|---|
| `fake-claude` | `claude -p … --output-format stream-json` | Accepts the real argv (§2), rejects unknown options, validates choices, requires `--verbose` with stream-json, and refuses a missing `--settings` or `--mcp-config` file with the CLI's messages (exit 1, no frame); an unknown `--effort` is warned about and ignored. `--version` prints `<pin> (Claude Code)`; `--help` advertises the runner flags so `scripts/check-claude-cli-flags.sh` accepts it. Only stream-json output is implemented. |
| `fake-codex` | `codex app-server --listen stdio://` | Serves the stable JSON-RPC handshake, one thread start/resume and one turn, replaying scenarios as App Server notifications. Accepts the ordinary and formal launch flags (`-c`, `--strict-config`, `--disable`) and refuses `codex exec`. `--version` prints `codex-cli <pin>`. Writes the rollout file unless the thread is ephemeral. |
| `fake-g5-tools` | an MCP stdio server | Scripted MCP stdio server exposing an unrelated `echo` tool, used by the transport tests and by `record.sh`. |

Both fakes embed the pin files at build time, so they always report the pinned
versions.

### 7.1 Scenarios (`G5_FAKE_SCENARIO=<file>`)

Scenario selection: `G5_FAKE_CLAUDE_SCENARIO` / `G5_FAKE_CODEX_SCENARIO`
(one file per provider, for a worker that runs both fakes) win over the
shared `G5_FAKE_SCENARIO`. With neither set the fake replays a **built-in
turn** (`tools/fake-provider-cli/src/default_scenario.rs`): a prompt whose first
paragraph is `hello` (the text before the first blank
line, trimmed, case-insensitive, so context an adapter appends after a blank
line keeps the reply) answers `hello from claude code` /
`hello from codex`, any other prompt answers
`Claude Code fake response: [fake-claude <pin>; session=<id>] <prompt>` /
`Codex fake response: <prompt>`. Prompts that select one of the retired
sidecar's deterministic workflows run that workflow first (§7.5). The
built-in Claude turn is `init` (with the servers named by `--mcp-config`
reported `connected`), the thinking block
`Claude Code fake thinking summary: planning the fake response.` (a
`thinking` `stream_event` pair and its `assistant` frame), a text
`stream_event` pair, the text `assistant` frame and the `result`; the built-in
Codex turn is `thread.started`, `turn.started`, one `agent_message` item and
`turn.completed`. This is what the worker runs under `G5_CLAUDE_CODE_FAKE=1`
/ `G5_CODEX_FAKE=1` in the deterministic docker-ci lane (plan P3.2), where
the worker resolves `fake-claude` / `fake-codex` beside its own `g5d`
executable. A `G5_CLAUDE_PATH` / `G5_CODEX_PATH` pin counts only when it names
the fake, and a missing fake fails the launch rather than reaching the real CLI
(the guest resolves the carrier's `fake_provider` program kind the same way,
§8).

JSON Lines. Each line is either a **raw frame** (an object with `type`,
emitted verbatim after placeholder substitution — so the recorded fixtures are
valid scenarios) or a **step**:

| Step | Effect |
|---|---|
| `{"emit": {…}}` | Emit the object (same as a raw frame; use it for frames without `type`). |
| `{"write_file": {"path": "rel/path", "content": "…", "append": false}}` | Write a file relative to cwd (no absolute paths, no `..`). |
| `{"mcp_call": {"server": "g5-tools", "tool": "ask_user", "arguments": {…}, "id": "toolu_x", "timeout_ms": n}}` | Call the tool on the configured server (Claude: `--mcp-config`; Codex: `$CODEX_HOME/config.toml`). The announcement frame (Claude: `assistant` with the `tool_use` block, §3.2; Codex: `item.started` with the `mcp_tool_call` item, §5) is written **before** the blocking call, and the result frame (Claude: `user` with `tool_result`, §3.3; Codex: `item.completed`) after the server answers, so a consumer sees the in-progress call while `ask_user` waits for a human. Timeout defaults to `MCP_TOOL_TIMEOUT` (Claude) or `tool_timeout_sec` (Codex); a timeout or a server failure becomes an error result. |
| `{"permission": {"tool_name": "Write", "input": {…}, "id": "toolu_y", "on_deny": "<label>"}}` | Claude only. Place it after the raw `assistant` frame that announces the tool's `tool_use` block, as the real CLI announces the tool before asking the prompt tool (§4.2). `bypassPermissions` allows without asking; `--permission-prompts none` or no prompt tool denies; otherwise the prompt tool is called with `{tool_name, input, tool_use_id}` and blocks until it answers. On deny the denial `user` frame is emitted, the next `result` frame gains `permission_denials`, and replay jumps to the label. `fake-codex` rejects this step (exit 1) because `codex exec` cannot prompt. |
| `{"label": "name"}` | Jump target. |
| `{"stderr": "text"}` | Write a line to stderr. |
| `{"exit": {"code": n, "stderr": "text"}}` or `{"exit": n}` | Stop and exit with `n` (frames after it are not emitted). |

Any step (or raw frame) may carry `"delay_ms": n`, slept before it runs.
Placeholders inside any string: `{{session_id}}`, `{{cwd}}`, `{{prompt}}`,
`{{model}}`, `{{timestamp}}`, `{{uuid}}` (fresh each use), `{{uuid:name}}`
(stable per name within a run).

`fake-codex` also refuses to **emit** any frame that carries an approval
`type` anywhere in it (`exec_approval_request`,
`apply_patch_approval_request`, `approval_request`, …), whether it arrives as
a raw frame, through `emit`, or nested in an `item.started` / `event_msg`
wrapper: the scenario fails with exit 1 at that line, so the "never emits
approval frames" invariant of design §8.6 holds regardless of scenario
content.

Behaviour that mirrors the real CLIs: the session id comes from
`--session-id`/`--resume` (Claude) or `exec resume <id>` (Codex), else a fresh
UUID (v4 for Claude, v7 for Codex); `fake-claude` in stream-json mode starts
on the first user message and exits only at stdin EOF, and errors if stdin
closes first; `fake-codex` reads stdin to EOF when no positional prompt is
given and prints `Reading prompt from stdin...`; `fake-claude` appends a
transcript under `CLAUDE_CONFIG_DIR/projects/<project key>/<session>.jsonl`
(unless `--no-session-persistence`) and, with `G5_FAKE_STRICT_RESUME=1`, fails
`--resume` when that file is missing; `fake-codex` appends to
`CODEX_HOME/sessions/YYYY/MM/DD/rollout-<ts>-<id>.jsonl` unless `--ephemeral`
and, with `G5_FAKE_STRICT_RESUME=1`, fails `exec resume <id>` with the real
CLI's `no rollout found for thread id <id>` error (exit 1) when no rollout of
the thread exists.

The built-in `G5_TOOL_ONLY_FACT_E2E:record` probe generates a random fact in
a tool result while keeping it out of the assistant message. A later
`G5_TOOL_ONLY_FACT_E2E:recall` reads that fact from the native transcript or
rollout; without those files it replies that the fact is unavailable. The
`G5_TOOL_ONLY_FACT_E2E:skip-persistence` test directive leaves the first
turn's native file absent, so the G5 value loop can exercise reconstruction
after VM replacement. `G5_TOOL_ONLY_FACT_E2E:refuse-resume` makes a resumed
fake read its input and exit 1 without emitting a provider frame; the standalone
CLI fixture also accepts `G5_FAKE_REFUSE_RESUME=1`. The VM launcher does not
forward that environment variable, so browser tests use the per-turn directive.
`G5_TOOL_ONLY_FACT_E2E:defer-stop-save` tells the fake-only G5 runtime to leave
a forced Stop generation pending for shutdown-flush or next-turn completion.
These directives are fake-provider behavior only.

Dynamic Claude scenarios consume additional user input after a tool-result
frame, echoing its unmodified content and caller UUID when
`--replay-user-messages` is enabled. `{{steered_prompt}}` substitutes consumed
text in the scenario's answer; the original input frame is preserved. Without
a remaining tool boundary, each extra user frame starts a default response turn
in the same process, including after stdin EOF. Native question probes retain
extra user input while waiting for their control response. Raw recordings with
explicit replay frames keep their recorded sequence and do not synthesize echoes.

Known differences: JSON object keys are emitted in sorted order (the real CLIs
put `type` first); `--output-format text|json` and Codex `resume --last` are not implemented;
`stream_event` deltas are emitted only when the scenario contains them.

### 7.2 Receipts (`G5_FAKE_RECEIPT=<file>`)

A JSON file written at exit with `provider`, `version`, `argv`, `cwd`,
`options` (parsed argv), `positional`, `stdin` (raw), `stdin_messages`
(parsed stream-json), `prompt`, `session_id`, `resume`, `env` (an allowlist:
`CLAUDE_CONFIG_DIR`, `CODEX_HOME`, `MCP_TOOL_TIMEOUT`, `ANTHROPIC_BASE_URL`,
`ANTHROPIC_MODEL`, `OPENAI_BASE_URL`, `HOME`, `G5D_SESSION_ID`, `G5D_API_URL`,
`G5_FAKE_SCENARIO`), `credentials_present` (names only of credential
variables that were set), `mcp_calls`, `permissions`, `written_files`,
`frames_emitted`, `exit_code`. Runner tests assert the argv and the prompt
from it.

### 7.3 `fake-g5-tools`

MCP stdio server (JSON-RPC 2.0, one message per line) implementing
`initialize`, `ping`, `tools/list`, `tools/call` and ignoring notifications.
It exposes only an unrelated `echo` tool; provider questions and permissions
travel over the provider's stdio control protocol, never MCP.
Environment: `G5_FAKE_MCP_DELAY_MS=<n>` (delay every call; tests timeouts),
`G5_FAKE_MCP_RESPONSES=<json file>` with
`{"tools": {"<name>": {"result": <tools/call result>, "delay_ms": n}}}`, and
`G5_FAKE_MCP_LOG=<file>` (append every received message; this is how the
`*.mcp-requests.jsonl` fixtures were captured).

### 7.4 Recorded fixtures and re-recording

`tests/fixtures/provider-frames/{claude,codex}/<scenario>.jsonl` are the
sanitised stdout of the real CLIs at the earlier pins, Claude Code 2.1.270 and codex-cli 0.153.3 (the exact-pin native Claude recordings are in `tests/fixtures/agent-turn-native/claude/`, §3.8); `<scenario>.mcp-requests.jsonl` are
the JSON-RPC messages the CLI sent to `fake-g5-tools`; `recording.json` lists
each scenario's argv, prompt, exit code and stderr. `tests/fixtures/provider-frames/README.md`
describes the scenarios and the sanitisation. Re-record with
`tools/fake-provider-cli/record.sh [claude|codex|all]` (needs a logged-in
Claude Code and Codex, or API keys; see the script header). The replay test in
`tests/cli.rs` runs every fixture through the matching fake.

### 7.5 Prompt-selected workflows (the retired sidecar fixtures)

Without a scenario file the fakes first check the prompt for the
deterministic workflows the e2e lanes drove through the retired Node fake
sidecar (`tools/fake-provider-cli/src/workflows.rs`, review round 1 of P3.2),
then fall back to the built-in turn. Directives match where they start the
prompt or follow whitespace.

| Workflow | Selected by | Effect | Reply |
|---|---|---|---|
| Citation (both fakes) | `G5_CITATION_E2E:<observed>\|<canonical>` | Claude: a `Read` `tool_use` for `<observed>` and its `tool_result` (`deterministic citation fixture read succeeded`). Codex: a completed `file_change` item naming `<observed>`. | `Citation proof: <canonical>.` |
| Workspace write | exactly one `G5_FS_WRITE_E2E:<relative path>\|<canonical base64>` | Writes the bytes under cwd; absolute or escaping paths fail. | `Fake workspace write completed: <path>; boundary=…` |
| Workspace write set | exactly one `G5_FS_WRITE_SET_E2E:<canonical base64 JSON>` containing two distinct `{path, contentBase64}` entries | Validates both paths and contents, then writes code and Project Doc bytes under `G5D_WORKSPACE_ROOT` (or cwd in standalone tests). | `Fake workspace write completed: two workspace files; boundary=…` |
| Task completion | exactly one `G5_TASK_COMPLETE_E2E:<canonical base64 UTF-8 summary>` | Runs `$G5_FAKE_G5D_BIN task complete -m <summary>` in cwd (absolute path required, 30 s timeout). | `Fake task completion recorded; boundary=…`, or with a write `Fake workspace write completed: <path>; task completion recorded; boundary=…` |
| Referenced question | exactly one `G5_QUESTION_CLI_E2E:<canonical base64 JSON {path, id, expectedRevision, options}>` | Runs `g5d question list --path <path>` (must show the id) and `g5d question answer` with the options, through `$G5_FAKE_G5D_BIN` when absolute, else `g5d` on PATH. | `Fake question proposal recorded: <answer output>; boundary=…` |
| Streamed reply (Claude) | exactly one `G5_STREAM_DELTAS_E2E:<count>:<bytes_per_delta>[:<delay_ms>]` (count 1..99999, bytes 8..4096, product ≤ 16 MiB, delay ≤ 10000 ms) | The text block streams as `count` `text_delta` frames of exactly `bytes_per_delta` bytes each (`w<five-digit index>` + filler letters + one space), `delay_ms` apart; the `assistant` and `result` frames carry the concatenation. Drives the live relay acceptance flow `frontend/e2e/flows/chat-large-response.spec.ts`. | the concatenated deltas |

`boundary=g5d=<G5_FAKE_G5D_BIN, or unset>,claude=<this executable>` replaces
the retired `sidecar=<bundle path>` provenance token. Claude workflow replies
keep the built-in thinking block. A workflow error fails the turn with exit 1
and the error on stderr, before any frame.

Repeated-commit (`G5_REPEATED_COMMIT_E2E:`) and budget-resume (`G5_BUDGET_RESUME_E2E:`) fixtures use native Git. They read the authenticated `g5d task context` mount/ref map, commit only the requested file, push `HEAD` to the assigned ref, and fetch/rebase on peer advances. Publication failures fail the turn. Budget resume requires `G5_FAKE_BUDGET_RESUME_E2E=1`, emits a typed `error_max_turns` result only after publishing the checkpoint, and retries publication before lifecycle completion. Local real-Git tests cover rejected pushes and lifecycle retries. Live tasks publish on their authorized workspace refs after ownership and permission checks; these fixtures do not override a forbidden ref.


## 8. Runner carrier frames (host-driven microVM path)

On the host-driven microVM path the worker's runner does not spawn the CLI
itself: `CarrierProcessLauncher` (`backend/g5d-worker/src/vsock_transport.rs`)
opens one mux `Stdio` stream per launch and the in-guest
`agent_runner` (`backend/g5d/src/agent/agent_runner.rs`) starts the process.
The stream carries newline-delimited JSON; every line that is not a control
frame is a provider stdout line forwarded verbatim (the CLIs never emit a
`type` starting with `g5.runner.`).

| Frame | Direction | Meaning |
|---|---|---|
| `{"type":"g5.runner.launch","spec":{…}}` | host → guest | First line: the `LaunchSpec` (`program` as `{"kind":"provider","value":"claude_code"\|"codex"}`, `{"kind":"fake_provider","value":"claude_code"\|"codex"}` in fake-provider mode, or `{"kind":"path","value":"…"}`, `args`, `cwd`, `env`, `env_remove`, base64 `stdin`, `run_as`, `env_clear`). The guest resolves the provider to its runtime path (a `fake_provider` launch runs only the `fake-claude` / `fake-codex` beside the guest's own executable and answers `launch_error` when it is missing), re-owns `/work` for the agent uid, and builds the environment from its non-secret allowlist plus the host's per-turn settings (a credential-bearing entry is refused). |
| `{"type":"g5.runner.signal","signal":"INT"\|"KILL"}` | host → guest | Cancel: the runner sends `INT`, then `KILL` 15 s later. The guest applies signal frames until the process exits, including after its output closed. |
| `{"type":"g5.runner.stderr","line":"…"}` | guest → host | One provider stderr line. |
| `{"type":"g5.runner.exit","exit":{"code":n\|null,"signal":n\|null}}` | guest → host | The provider exited; the guest closes the stream next. |
| `{"type":"g5.runner.launch_error","message":"…"}` | guest → host | The process could not be started (the stream closes). |
