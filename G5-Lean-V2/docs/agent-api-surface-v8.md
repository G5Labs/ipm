# Agent API Surface — v8

> Status: proposed architecture, draft 8, revised 2026-09-17
>
> Purpose: define the Agent-owned execution contract and shared agent-task lifecycle adopted by [design v8](lean-backend-design-v8.md) and [implementation plan v8](lean-implementation-plan-v8.md). This versioned companion preserves [the historical API note](agent-api-surface.md). Its target behavior is distinct from the currently implemented adapters described in [Provider CLI contract](lean-provider-cli-contract.md).

## 1. Provider baseline and evidence

The repository pins Claude Code `2.1.270` and Codex CLI `0.153.3` in `scripts/claude-code-cli-version` and `scripts/codex-cli-version`. The current Claude runner suppresses `AskUserQuestion`, sends permissions through a custom MCP tool and closes stdin after one input; those are migration targets, not the revised design. The current Codex runner uses `exec --json`. No pin bump or Claude Agent SDK runtime is authorized by this revision.

| Provider | MVP target | Later change |
|---|---|---|
| Claude Code `2.1.270` | Direct CLI, stream-json input/output, native `AskUserQuestion` and native tool-permission requests. Keep stdin open for correlated control responses during the attempt. | A resident CLI for independent live steering/model changes remains separate scope. |
| Codex `0.153.3` | Direct `codex exec --json`, one input followed by EOF, explicit noninteractive approval/sandbox policy. No interactive question or approval UI and no custom MCP fallback. | Replace the adapter with `codex app-server` over stdio under P3.7, with native request/reply support verified against the adopted binary. |

Anthropic's [user-input documentation](https://code.claude.com/docs/en/agent-sdk/user-input) identifies `AskUserQuestion` as the native clarification tool and distinguishes it from ordinary tool approval. Its [official transport source](https://github.com/anthropics/claude-agent-sdk-python/blob/main/src/claude_agent_sdk/_internal/transport/subprocess_cli.py), [permission routing](https://github.com/anthropics/claude-agent-sdk-python/blob/main/src/claude_agent_sdk/types.py) and [control-message handler](https://github.com/anthropics/claude-agent-sdk-python/blob/main/src/claude_agent_sdk/_internal/query.py) show the CLI protocol underlying that SDK. These sources justify a Rust CLI transport implementation; they do not establish that every current schema field has been recorded on `2.1.270`. The local installed CLI inspected during this revision reported `2.1.273`, so it is not evidence at the repository pin. Exact-pin native-control recordings remain an implementation acceptance gate (§13).

For Codex, [official noninteractive documentation](https://developers.openai.com/codex/noninteractive/) describes `exec` as a scripted interface with preset policies. The [official App Server documentation](https://developers.openai.com/codex/app-server/) describes the future bidirectional stdio interface. Current online documentation is a design reference, not proof of the `0.153.3` App Server schema; the later implementation must generate/probe the schema from its chosen pinned binary.

## 2. Ownership and one shared agent-task model

Agents is a control-plane bounded context. Only its ephemeral runner is the Agent-owned, Workspace-hosted `agent.turn` module.

| Concern | Owner |
|---|---|
| Logical tasks and attempts, jobs/dependencies, shared conversations, editable inbox, transcript, provider-request projection/replies, provider-session files and usage | Agents |
| Workspace owner/read access, lifecycle, transfer, VM incarnation, catalog/journal, admission, flush and restore | Workspaces |
| Placement, boot, leases, VM isolation and resource supervision, host proxies | Worker & Host |
| Claude and Codex native protocol adapters inside `agent.turn` | Agents |

### 2.1 Owner authority and previews

The current `owner_user_id` alone may submit/edit agent work, send into a conversation, answer a live native request, change permission policy, cancel non-preview tasks or use a terminal. Org admins must take ownership before writing; readers see the same permitted history, files and task timelines without waking a VM. Each incarnation uses its fixed owner's provider account. Session identifiers are usage/correlation tags, not separate provider grants. Transfer changes accounts only after the old incarnation stops and releases.

Readers may create/cancel only durable `kind=preview` tasks through the fixed `workspace.preview` path; generic task APIs verify stored kind and cannot cascade reader authority into another kind. Preview launch identity is audit only. It has no bounded-task concurrency charge, wall-clock timeout or automatic restart. [Workspace VM API v8](workspace-vm-api-v8.md) specifies preview replacement and cancellation. Preview work never becomes an `agent.turn` merely because it was launched from a conversation.

Tasks use the workspace's member repositories and shared working branch. Native Git operates on those checkouts; managed workspace branches are created only by the checked fork operation. Committing/pushing is native agent or owner work, not a platform save/checkpoint instruction or a separate commit module.

### 2.2 Logical task, conversation and input provenance

`tasks.kind` is `agent | command | preview`. There is no `chat_turn` kind and no separate chat-task state machine. Every agent task, including uplift/downlift, review and custom workflow work, has a shared conversation through existing `chat_sessions`, `chat_messages` and `chat_session_files`. Those historical table names do not restrict them to typed chat. An agent task has a non-null conversation/session link; command and preview tasks need no provider conversation.

A human supplies typed input; a workflow supplies rendered template input. Both enter the same versioned inbox, dispatch, provider, transcript, question and outcome path. Record origin as provenance (`human | workflow`), along with immutable task/job linkage and the frozen template/input revision. Input origin never implies permission mode, question support, sandbox level, provider account or a bypass of approvals. A job can create several agent tasks; each uses this shared machinery and an explicitly chosen session, not a new task-only session store. Serialize one nonterminal logical agent task (`queued`, `running` or `waiting`) per shared session across all entry points, and at most one live attempt for that task. Distinct sessions may execute concurrently in the same workspace.

A logical task can have multiple execution attempts. `task_id`, `job_id`, dependencies, conversation link and original input identity remain fixed across continuation after interruption or VM loss. A continuation increments the attempt and gets a new `exec_id=(task_id, attempt, vm_id)`. An eligible continuation leaves the logical task nonterminal, queued or held `waiting` according to policy; it does not fail a parent and create an independent successor. Job/DAG consumers remain blocked until that same logical task reaches its final outcome. Cancel terminalizes the logical task and prevents later attempts. A later unrelated human instruction is new work, not a disguised continuation.

### 2.3 Permission policy is explicit

The server resolves the requested task/session/workflow configuration and freezes the effective permission/sandbox policy before each dispatch in the attempt’s immutable `policy_snapshot_ref`. A continuation may resolve newly authorized configuration, but never overwrites an earlier attempt’s policy evidence. Both input origins can select supported interactive or noninteractive behavior; neither gets automatic `bypassPermissions` because it came from a template. Provider support is checked before launch. A request requiring interactive Codex decisions is rejected as unsupported in the MVP rather than silently downgraded or routed through MCP.

Claude native clarification and tool approval are different semantic requests even when transported by the same control message. The adapter must prove that the chosen policy leaves `AskUserQuestion` available when native questions are enabled; blindly auto-allowing it can skip the user-input path. Permission decisions are owner actions attached to one native request, not changes inferred from a workflow's origin.

## 3. Three contract categories

| Category | Meaning | Examples |
|---|---|---|
| C1 | Command or lifecycle fact outside ordered provider output | Ensure runtime, invoke/cancel, native reply delivery, process exit. |
| C2 | Ordered nonterminal observation for one execution | Message/tool progress, native request opened/replied/invalidated, usage. |
| C3 | Exactly one terminal outcome for one execution attempt | Succeeded, failed, cancelled, interrupted, timed out, suspended. |

A C2 message or provider control request never finishes an attempt. Provider result/terminal-looking output is evidence, settled with the transport and process result. TaskService decides whether an attempt outcome ends the logical task or permits another attempt of the same task. An invocation timeout cannot invent failure or launch a new identity merely to disambiguate delivery.

## 4. APIs Agents consumes from Workspaces

All calls use the Workspace Runtime facade. Agents never reaches host tunnels, placement or the dispatcher directly.

### 4.1 Resolve or wake a runtime

```typescript
type WorkspaceRuntimeEnsureRequestV1 = {
  contract_version: "workspace-agent-control/v1";
  request_id: string;
  idempotency_key: string;
  workspace_id: string;
  reason: "agent_turn" | "agent_reconcile" | "preview";
  deadline_at: string;
};

type WorkspaceRuntimeHandleV1 = {
  workspace_id: string;
  vm_id: string;
  workspace_epoch: number;
  owner_user_id: string;
  state: "starting" | "ready" | "stopping" | "failed";
  module_catalog_version: string;
  dispatcher_contract_version: string;
};
```

`workspace.runtime.ensure(request)` dispatches authorized work or reconciles an ambiguous operation; it is not used for reads or provider-request replies. `workspace.runtime.get` is read-only and may report that no live handle exists. Preview ensure requires the stored preview kind and current read access. All reasons honor transfer admission; only explicit current-owner Resume clears `owner_resume_required` for retained agent/command work. Reader preview wake cannot release it.

There is no `agent_question_resume` reason. A reply targets an existing live provider request and never boots a VM or creates a continuation.

### 4.2 Invoke, inspect and cancel

```text
modules.describe(workspace_id, expected_vm_id, "agent.turn")
modules.invoke(AgentTurnInvokeRequestV1)
  -> started | already_running | already_finished(outcome) | cancelled
modules.status(workspace_id, expected_vm_id, exec_id)
  -> unknown | queued | running | finished(outcome)
modules.cancel(workspace_id, expected_vm_id, exec_id, reason)
  -> cancelled | signalling | already_finished(outcome)
```

The invocation includes the fixed `agent.turn` module/version/hash, `operation=run`, schema version, input, capability `grant_id`, deadline, expected incarnation and `idempotency_key=exec_id`. Before invocation, Agents persists `status=running`, `start_confirmed=false`, attempt and incarnation as durable intent; a disposition confirms the attempt, and any returned outcome is applied. Lost replies re-invoke the same execution. Cancellation persists a disposition even before spawn, preventing delayed invoke from launching cancelled work. Dispositions survive journal pruning for the lifetime of the incarnation.

### 4.3 Replay and acknowledgement

```text
modules.events.replay(workspace_id, expected_vm_id, exec_id, from_seq)
modules.events.ack(workspace_id, expected_vm_id, exec_id, up_to_seq)
modules.outcomes.pending(workspace_id, cursor)
workspace.runtime.events(cursor)
```

Replay is ordered per execution with live events queued behind it. Acknowledge only contiguous prefixes durably persisted by Agents. Fence every request and reply by incarnation and attempt. Replayed native-request events rebuild history; they do not re-enable an invalidated request or prove its provider process is still live.

### 4.4 Reply to a live provider request

```text
modules.input.respond({ workspace_id, expected_vm_id, expected_epoch,
                        task_id, attempt, exec_id, provider_request_id, reply_version,
                        validated_response_ref, idempotency_key })
  -> delivered | already_delivered | request_gone | stale_execution
```

This is a narrow declared `agent.turn` input channel, not arbitrary stdin, generic shell, chat steering or an additional module invocation. The task/attempt fields must match `exec_id`; expected incarnation and epoch must match the live execution. `reply_version` names the durable `provider_requests.response_version`. Workspaces validates the private per-execution control capability and routes this declared input to the already-running module; it cannot launch or address another execution. Agents authenticates the current owner, validates the response against the pending native request and records the reply intent before calling it. The adapter writes only the exact provider response envelope for that request. Duplicate UI delivery uses the same reply version; conflicting edits are refused after delivery starts. A delivery acknowledgement means the response was handed to the live provider transport, not that the tool succeeded. An uncertain write is reconciled within that same live process; process loss never causes reply replay into a new attempt.

## 5. The `agent.turn` module contract

```yaml
module_id: agent.turn
module_version: 1.0.0
lifecycle: bounded_run
operations:
  run:
    input_schema: g5.agent.turn.input/v1
    event_schema: g5.agent.turn.event/v1
    output_schema: g5.agent.turn.outcome/v1
  input.respond:
    input_schema: g5.agent.native-response/v1
requested_capabilities:
  - workspace.native_files
  - workspace.native_git
  - workspace.native_shell
  - workspace.native_tests
  - network.agent_control_plane
  - network.agent_llm_proxy
observability:
  logs: bounded_redacted
  events: journaled
```

The manifest requests capabilities and Workspaces grants their intersection with runtime policy. Inputs cannot contain caller-selected argv, environment, host paths, signals, credentials or host endpoints.

```typescript
type AgentTurnInputV1 = {
  schema_version: "g5.agent.turn.input/v1";
  task: {
    task_id: string;
    attempt: number;
    session_id: string; // existing shared chat_sessions identity
    input_origin: "human" | "workflow";
    initial_input?: AgentSessionInputReservationV1;
  };
  provider: {
    provider_id: "claude_code" | "codex";
    model_id: string;
    permission_mode: "non_prompting" | "tool_approval";
    policy_ref: string; // frozen provider-specific policy, independent of origin
    options: Record<string, unknown>;
  };
  prompt_ref: { artifact_ref: string; sha256: string };
  workspace_target: { repo_id?: string; relative_path?: string };
  owner_proxy_binding_ref: string;
  session_binding: {
    provider_session_id?: string;
    expected_manifest_generation?: number;
    restore_ref?: string;
  };
  continuation?: {
    prior_exec_id: string;
    reason: "interrupted" | "vm_lost" | "suspended";
  };
  limits: { wall_clock_ms: number; max_turns?: number; max_output_bytes: number };
};
```

A first attempt consumes a frozen inbox reservation; a continuation follows the recorded accepted-input cut instead of blindly replaying the original prompt. There is no `resume.question_id`, durable question blocker or answer-bearing successor input. Native provider session restore may retain conversation history; it does not make an old request ID live again.

## 6. APIs Agents provides to Workspaces

### 6.1 Owner-bound provider access

`agents.runtime_bindings.ensure` resolves the current owner/workspace/incarnation/provider binding through authoritative Workspaces state; `revoke(binding_id, expected_vm_id)` fences it. A binding never selects credentials from task origin, chat creator, preview launcher or session id. Current-owner credential rotation may update a running incarnation; transfer releases it before the new owner can receive a new binding. The module capability grant remains separate from owner provider-account selection.

`agent.turn` configures the host proxy with the accounting session id and placeholder credentials, never real keys. Restoring an old conversation does not restore its former owner's account authority. [Worker Host API v8](worker-host-api-v8.md) owns the proxy boundary.

### 6.2 Shared provider-session files

`agents.session_bundles.get` and `agents.session_harvests.persist` use the existing `chat_session_files` storage for every agent task's conversation. Agents owns provider paths, format, generation, hashes and compatibility; Workspace and Host carry opaque bundles. Harvests from stale incarnations or older generations cannot overwrite a newer manifest. On native restore failure, start a fresh provider session using delivered conversation history with clear interrupted-attempt markers. Do not replay an unconsumed native reply as input to that new session.

### 6.3 Stop and transfer

```text
agents.workspace_stops.prepare(stop)
agents.workspace_stops.get(stop_id)
agents.workspace_stops.abort(stop_id, expected_vm_id)
agents.workspace_stops.commit(stop, workspace_runtime_event_id)
```

`prepare` closes Agent dispatch, settles input reservations without guessing acceptance, interrupts live executions, invalidates their pending provider requests, persists the ordered event prefix and session bundles, and returns durable Agent stop state. Workspaces pairs this with its teardown flush receipt before release. Existing queued inputs and logical task identities remain; allowed continuation increments their attempts only after lifecycle admission opens.

Transfer holds admission through host exit/release and atomic owner/audit completion. Failed flush retains the old owner and reopens admission, but does not revive the native requests of already terminated processes. Successful transfer sets `owner_resume_required`; only the new owner's explicit Resume permits retained agent/command work to continue. A reader preview wake does not release the hold. Historical questions remain visible and read-only; the continuing provider may ask a fresh question, which the new owner can answer while its new process is alive.

### 6.4 Activity projection

```typescript
type AgentWorkspaceActivityV1 = {
  workspace_id: string;
  agent_admission: "open" | "closed";
  owner_resume_required: boolean;
  active_exec_ids: string[];
  pending_provider_request_ids: string[];
  active_task_count: number;
  queued_input_count: number;
  reserved_input_count: number;
  delivery_unknown_input_count: number;
  latest_agent_event_id?: string;
};
```

`agents.workspace_activity.get` is a read-only projection, not task authority or stop proof. A pending live native request explains `waiting`; a historical prompt from a dead process neither keeps the VM awake nor authorizes wake.

## 7. Worker & Host dependency

Agents has no direct host API for placement, wake, lease, stop, networking or shell. Workspaces supplies a pinned module/runtime image, the actual CLI binaries, declared resources/capabilities and journal transport. It does not add a Claude SDK/Node adapter. Provider schemas and stdout/stdin control parsing remain inside `agent.turn`.

The host sees owner-bound credential references and usage tags needed by its proxy, not a separate conversation or native question state machine. Resource and process facts reach the adapter/reconciler; they do not themselves establish logical task success. No raw user-supplied provider response is forwarded through a host shell or HTTP port relay.

## 8. Inbox, native requests and continuation

### 8.1 Shared inbox

```text
queued -> reserved -> delivered
   |         |
   |         +-> delivery_unknown -> delivered | released
   +-> withdrawn
```

Queued human or workflow inputs are editable/withdrawable by the owner under the same rules. Reservation freezes the version for a particular attempt; delivered input is immutable transcript. Ambiguous acceptance is reconciled using the existing provider session before release/redelivery. `input.accepted` names input id, revision, reservation and delivery generation. A native response is separate from queued ordinary input and cannot be substituted for it.

An ordinary input arriving while a session task is running or held stays in that session inbox behind the nonterminal task. Absence of a live provider does not authorize another task to bypass a continuation hold. Only explicit current-owner Continue/Resume clears that hold; after terminal settlement, the next ordinary input creates a new agent task in the same session.

Both MVP providers receive one ordinary user input per attempt. Claude keeps stdin open solely for native control exchanges, then closes it at provider completion; ordinary messages typed during execution remain queued. This does not introduce arbitrary live steering. Codex receives its one prompt then EOF immediately, as `exec` requires.

### 8.2 Native request projection and reply

```text
provider_requests(
  id, task_id, attempt, vm_id, epoch, exec_id, session_id, provider, provider_request_id,
  kind question|permission, tool_name, payload_ref, display_ref,
  status pending|reply_pending|responded|invalidated,
  response_ref NULL, response_version, responded_by NULL, response_deadline_at,
  created_at, responded_at NULL, invalidated_at NULL, invalidation_reason NULL,
  UNIQUE (exec_id, provider_request_id)
)
```

This stores timeline/display and delivery bookkeeping for a request owned by a live provider process. It is not an independent HITL engine or a promise that a request survives VM sleep. Raw provider payload and sanitized display remain separate; render text/options as data, validate replies against the recorded shape and authorize current ownership. Questions collect provider-defined selections/text; permissions allow/deny the provider-defined action. Unsupported provider requests fail explicitly; no fallback custom question tool is installed.

A single serialization point coordinates response delivery, provider cancellation, attempt termination and owner transfer. Accept a response only while its exact execution and native request remain live. Recheck before writing to stdin. On `control_cancel_request`, process exit, task cancellation, VM loss or teardown, invalidate pending/reply-pending requests, retain their history and refuse late responses with `request_gone`. If transport delivery raced termination, retain the factual delivery state but never bind or resend the reply to a different process. The next native request gets its own execution/request identity even if the provider reuses a textual id.

A live request has a bounded `response_deadline_at` and remains subject to the overall attempt limit. Response delivery, deadline expiry and process termination share the same arbitration path. When response wait expires, invalidate its requests, interrupt/reap the provider and harvest its session files; settle the attempt once as `suspended` and keep the same logical task held `waiting` for owner Continue. A late provider result cannot overwrite that outcome. This wait deadline bounds the live process; it does not keep a question answerable after exit. There is no answer-after-suspension wake or `successor_task_id`. Legacy `task_questions` and bespoke ask/permission-tool history are migrated or retained read-only when their producers are retired, never converted into live native requests.

### 8.3 Continuation and replay

Conversation continuity belongs to the same agent task and its shared session. When lifecycle/retry policy allows continuation after interruption, suspension or VM loss, TaskService atomically closes the prior attempt and retains the same logical task, preserving job/dependency edges. User Stop, transfer and native-request timeout hold it for explicit current-owner Continue/Resume; eligible drain/rollout/loss recovery may queue its next attempt under the recorded policy. `waiting` distinguishes a live native request from a held task with no process. Only final logical success releases dependents; final failure/cancellation follows the job's explicit dependency-failure policy. It may restore provider session files or use delivered-history recovery. It does not send a previously unconsumed response to a dead native request. If the provider still needs input, it must emit a new native request.

Journal replay repairs projections; provider-session restore restores provider history; inbox dispatch selects a new authorized input; task continuation preserves interrupted work identity. Those are distinct operations. A cancellation wins against continuation creation under the same logical task lock; duplicate recovery cannot create multiple attempts. Tasks reach final failure when continuation is unavailable or exhausted, not merely because an intermediate attempt was interrupted.

## 9. Provider-neutral event and outcome schemas

The C2 union below is normalized platform data, not a list of capabilities guaranteed by both providers. The adapters admit only variants proved by their pinned transport fixtures; unused SDK-only discriminants are not silently promoted into CLI behavior. Native-request events are correlated by the envelope's `exec_id` and their provider request id.

```typescript
type AgentTurnEventV1 =
  | { kind: "provider.session"; provider_id: "claude_code" | "codex";
      provider_session_id: string; phase: "started" | "resumed"; model_id?: string }
  | { kind: "provider.capabilities"; snapshot_ref: string }
  | { kind: "provider.status"; status: "initializing" | "running" |
      "requesting" | "requires_action" | "compacting" | "authenticating" |
      "installing" | "retrying" | "clear"; detail_code?: string }
  | { kind: "provider.control_progress"; request_id: string;
      status: "started" | "api_retry"; attempt?: number; max_retries?: number;
      retry_delay_ms?: number; error_status?: number }
  | { kind: "message.delta"; message_id: string; text_delta: string }
  | { kind: "message.final"; message_id: string; content_ref: string;
      in_reply_to_input_ids: string[]; usage?: UsageV1 }
  | { kind: "message.retracted"; provider_message_ids: string[]; reason: string }
  | { kind: "thought.delta"; thought_id: string; text_delta: string }
  | { kind: "thought.final"; thought_id: string; content_ref: string }
  | { kind: "thinking.progress"; estimated_tokens: number; delta?: number }
  | { kind: "input.accepted"; reservation_id: string; input_id: string;
      revision: number; delivery_generation: number }
  | { kind: "tool.started"; call_id: string; name: string; input_ref: string }
  | { kind: "tool.progress"; call_id: string; progress_ref: string }
  | { kind: "tool.completed"; call_id: string;
      status: "ok" | "error" | "declined"; output_ref?: string }
  | { kind: "plan.updated"; plan_ref: string }
  | { kind: "subtask.started"; task_id: string; description?: string }
  | { kind: "subtask.progress"; task_id: string; progress_ref: string }
  | { kind: "subtask.completed"; task_id: string;
      status: "ok" | "error" | "stopped"; result_ref?: string }
  | { kind: "background_tasks.replaced"; tasks_ref: string }
  | { kind: "workspace.change"; change_ref: string }
  | { kind: "provider_request.opened"; provider_request_id: string;
      request_kind: "question" | "permission"; tool_name: string;
      payload_ref: string; display_ref: string }
  | { kind: "provider_request.replied"; provider_request_id: string;
      reply_version: number; disposition: "allowed" | "denied" | "answered" }
  | { kind: "provider_request.invalidated"; provider_request_id: string;
      reason: "provider_cancelled" | "process_exited" | "execution_ended" }
  | { kind: "context.compacted"; boundary_id: string; metadata_ref?: string }
  | { kind: "context.usage"; snapshot_ref: string }
  | { kind: "memory.recalled"; memories_ref: string }
  | { kind: "artifact.persisted"; artifact_ref: string; source_call_id?: string }
  | { kind: "commands.replaced"; commands_ref: string }
  | { kind: "suggestion.available"; suggestion_ref: string }
  | { kind: "usage.updated"; usage: UsageV1; quota_snapshot?: QuotaSnapshotV1 }
  | { kind: "conversation.reset"; new_provider_session_id: string }
  | { kind: "notice"; code: string; level: "info" | "warning" | "error";
      message: string; retryable: boolean }
  | { kind: "terminal.progress"; phase: "harvesting" | "finalizing" };
```

Large or sensitive bodies use immutable, access-controlled references. A C2 `notice` may describe an error, but it does not terminate the execution.

```typescript
type AgentTurnOutcomeV1 = {
  schema_version: "g5.agent.turn.outcome/v1";
  status: "succeeded" | "failed" | "cancelled" | "interrupted" |
          "timed_out" | "suspended";
  code: string;
  retryable: boolean;
  final_message_ref?: string;
  provider_session_id?: string;
  session_manifest_generation?: number;
  usage?: UsageV1;
  last_event_seq: number;
};
```

The outcome terminates one attempt. It contains no answerable native-request blocker. Interruption/suspension/VM-loss continuation is a TaskService decision for the same task; a tool result, refusal, request opening or EOF alone cannot establish final success.

## 10. Claude Code direct CLI native input/output

### 10.1 Transport and launch target

The source-backed transport target is `claude -p --output-format stream-json --input-format stream-json --verbose --permission-prompt-tool stdio`, with existing exact-pin-supported session/model/policy flags. Anthropic's [permission routing source](https://github.com/anthropics/claude-agent-sdk-python/blob/main/src/claude_agent_sdk/types.py) selects `stdio`, and its [transport](https://github.com/anthropics/claude-agent-sdk-python/blob/main/src/claude_agent_sdk/_internal/transport/subprocess_cli.py) passes it to the CLI. This is not a new invented flag; exact `2.1.270` initialization and policy behavior still need recordings before cutover. Do not blindly copy flags or optional fields from another release.

Remove `--disallowedTools AskUserQuestion` and the custom `mcp__g5-tools__request_permission` prompt target. Keep unrelated MCP tooling configured normally. Follow the pinned native initialization handshake, submit one reserved user message and continue reading stdout while writing only correlated native control responses. Do not close stdin immediately after the prompt. After a valid run-ending result and no remaining provider work, close stdin and settle EOF/process status; cancellation remains bounded signal escalation. Native subagent/background terminal behavior must be proved by the same recordings so result/EOF does not deadlock or abandon provider work.

### 10.2 Native request and response shape

The [official control handler](https://github.com/anthropics/claude-agent-sdk-python/blob/main/src/claude_agent_sdk/_internal/query.py) distinguishes `control_request`/`can_use_tool`, matches `request_id` in `control_response`, and abandons cancelled requests. The minimal reference envelopes are:

```json
{"type":"control_request","request_id":"req-1","request":{"subtype":"can_use_tool","tool_name":"AskUserQuestion","input":{"questions":[]}}}
{"type":"control_response","response":{"subtype":"success","request_id":"req-1","response":{"behavior":"allow","updatedInput":{"questions":[],"answers":{}}}}}
```

These illustrative envelopes omit actual question content; production validation must not treat empty questions/answers as user consent. Decode the complete pinned schema and preserve its opaque identifiers. `AskUserQuestion` carries provider-generated questions/options; the response retains its input and adds the selected answers. An ordinary tool-permission request uses its native allow/deny response instead. This follows Anthropic's [user-input contract](https://code.claude.com/docs/en/agent-sdk/user-input); G5 supplies presentation and response transport, not a replacement question protocol.

### 10.3 Normalization and lifecycle

| Native observation | G5 handling |
|---|---|
| `control_request` for `can_use_tool` / `AskUserQuestion` | Persist `provider_requests(kind=question)`, emit `provider_request.opened`, show provider-defined input. |
| `control_request` for another tool requiring approval | Persist kind `permission` under frozen policy; show the sanitized action. |
| Correlated native response written | Mark reply delivery, emit `provider_request.replied`; tool progress/outcome remains separate. |
| `control_cancel_request` or process/attempt end | Invalidate live pending request; preserve prompt history without reply controls. |
| User echo/input acknowledgement | Reconcile the reserved input using exact-pin evidence, never SDK-only assumptions. |
| Assistant/tool/stream events | Normalize into C2 message, tool, usage and progress events; no prose-to-state inference. |
| Result frame, EOF and exit status | Arbitrate one C3 attempt outcome; contradictory or malformed evidence cannot become success. |

The old SDK `0.3.273` message/method inventories remain in the historical unversioned companion. They are neither an active dependency nor the native schema for the pinned CLI. The implementation owns the actual exhaustiveness fixtures for `2.1.270` including control envelopes.

## 11. Codex MVP and later App Server

### 11.1 MVP: `exec --json`

Keep the recorded `0.153.3` `codex exec [resume <id>] --json` adapter, its supported argv and explicit noninteractive sandbox/approval policy. Write one prompt, close stdin, parse its JSONL events and settle final provider evidence with EOF/exit. `thread.started` binds the provider session; item progress updates the shared timeline. The actual admitted event/item union comes from the pinned fixtures, not a newer reference dump. See [the implemented CLI contract](lean-provider-cli-contract.md).

Codex MVP has no native interactive question/approval response channel. A prose question is ordinary transcript output, so the owner can send a later ordinary conversation message; it is never inferred into an actionable request card. Do not register custom `ask_user`, `g5d ask` or MCP permission substitutes to simulate one. If a policy requires interactive response, reject it before launch. Unexpected interactive requests become an explicit unsupported-capability/protocol failure, not an answerable request row or silent auto-approval. This applies equally to typed and templated agent tasks. Unrelated MCP tools remain supported.

### 11.2 Later: App Server over stdio

P3.7 is the deliberate migration to `codex app-server` using stdio JSONL: initialize, manage the provider thread/turn, parse native server requests and send correlated responses. The [official protocol](https://developers.openai.com/codex/app-server/) documents request IDs, native user-input and permission requests, request-resolution notifications, and CLI-generated version-specific schemas. Keep this adapter behind the same `agent.turn`, shared conversation, `provider_requests` and logical-task attempt contracts. Do not introduce a separate Codex question engine or expose a network listener.

Generate and store the chosen binary's schema; verify exact method names and capability negotiation instead of assuming today's online names exist at `0.153.3`. The design chooses the transport direction, not an untested pin bump. Enable interactive UI only after native question/approval, cancellation, process-death invalidation and same-task continuation fixtures pass. No replies to old App Server requests are replayed into a new process merely because a thread can be resumed.

## 12. Durable projections and authority

| Projection | Authority/input | Rule |
|---|---|---|
| Logical task and job/DAG status | TaskService; attempt outcomes and continuation policy | Same task id through attempts; final task outcome releases dependents. |
| Attempts | TaskService; confirmed start, C3 or reconciled incarnation loss | One execution per identity; no new attempt for ambiguous delivery. |
| Shared conversation and inbox | Existing chat tables; human/workflow inputs | One storage/lifecycle model; origin is provenance. |
| Provider-request history and reply projection | Native control stream plus owner response | Live process owns answerability; terminated requests stay read-only. |
| Provider-session files | Existing `chat_session_files`; fenced harvests | Shared across all agent task origins; native restore does not restore pending request authority. |
| Permission policy | Authorized frozen task/session/workflow specification | Independent of origin; provider support validated. |
| Provider account | Current workspace owner/incarnation | Never session creator, workflow submitter or preview launcher. |
| Transcript/tool/usage projections | Ordered C2 events | Rebuildable, bounded/redacted; not attempt completion authority. |
| Replay watermark | Contiguous persisted event prefix | Ack after durable projection write; old-attempt events never mutate a newer attempt. |
| Agent stop receipt | Admission, execution settlement, harvest and request invalidation | No live request remains answerable after process termination. |

## 13. Compatibility and acceptance gates

1. Pin/image/startup checks prove actual Claude `2.1.270` and Codex `0.153.3`; update fixtures deliberately for any later version. No SDK or extra provider version is installed implicitly.
2. Before Claude cutover, record the exact CLI native initialization, question, permission allow/deny, cancelled request, malformed/unknown control message, result-before-EOF and process-exit behavior. Test both input origins and supported permission policies, including policy configurations that might suppress `AskUserQuestion`.
3. Test owner response racing its deadline, provider cancellation, SIGINT/kill, VM loss, transfer and a reused provider request id in a new attempt. A response-wait timeout reaps and harvests once, holds the same logical task and never turns a late provider result into success. Pending prompts invalidate; stale/duplicate/conflicting replies cannot reach another execution, wake a VM or create a successor task.
4. Run human-typed and workflow-templated inputs through the same production TaskService/module boundary. Interrupt a workflow child with `job_id` and dependents: continuation retains task id and edges; dependents remain blocked until the logical task settles. Cancel racing continuation never launches a later attempt.
5. Test shared session restore and delivered-history recovery for workflow and human tasks; ambiguous initial input is not duplicated, and unconsumed native replies never become continuation prompts. Transfer preserves history but only the new live provider request is answerable by the new owner.
6. Codex `exec` rejects interactive policy/requests, contains no custom ask/permission fallback and still runs unrelated MCP tools. Later App Server gets its own schema, request/reply, invalidation and stdio-only integration gates before interactive support is exposed.
7. Preserve incarnation fences, owner-only mutations, reader preview-only authority, post-transfer resume holds, bounded/redacted events, credential isolation and task cancellation/replay regressions. These gates are required future validation, not claims that current deployed behavior already passes.

## 14. Implementation alignment and retired paths

P4.1 migrates active request storage to `provider_requests` while preserving old question history and the existing shared conversation/session tables. P4.6 owns the unified task/attempt state machine and reply projection. P4.7/P4.8 move typed and workflow inputs onto it; P4.11 owns direct native provider adapters. P3.7 later supplies Codex App Server stdio interaction.

Retire the active `chat_turn` kind, separate task-only conversation storage, `task_questions` bespoke HITL state machine, `ask_user`/`g5d ask`, custom MCP question/permission substitutes, question-wakeup/suspension successors, origin-based permission bypass and post-mortem native replies. Retain unrelated MCP tools and historical records. Update the old CLI contract and recorded fixtures when the implementation changes; it remains evidence of current code, not permission to preserve behavior this design replaces.

This revision changes documentation only. Native protocol direction is source-grounded; exact-pin end-to-end execution, implementation, deployment and observed behavior remain separate milestones.
