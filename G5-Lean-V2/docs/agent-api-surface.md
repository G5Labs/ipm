# Agent API Surface

> Status: proposed architecture amendment, 2026-09-16
>
> Purpose: define the contract cut that lets Agents, Workspaces, and Worker &
> Host be developed independently. This document refines the `agent.turn`
> module in [Workspace VM API Surface](workspace-vm-api.md) and proposes a
> provider-runtime change to [G5-Lean backend design, draft 6](lean-backend-design-v6.md).

## 1. Exact provider baseline

This contract is derived from these exact stable artifacts, not from provider
documentation in the abstract:

| Artifact | Exact version | Role |
|---|---:|---|
| `@anthropic-ai/claude-agent-sdk` | `0.3.273` | Claude runner API and `SDKMessage` source of truth. `0.3.272` was superseded on 2026-09-15. |
| Claude Code bundled by that SDK | `2.1.273` | Provider process launched by the SDK. It is not installed or versioned independently. |
| `@anthropic-ai/sdk` | `0.126.0` | Exact peer used to freeze nested Anthropic message and stream-event unions. |
| `@openai/codex` / Codex CLI | `0.154.0` | Codex runner and JSONL source of truth. |

The Claude contract is frozen from the published `sdk.d.ts` in
`@anthropic-ai/claude-agent-sdk@0.3.273` and the nested beta Messages types in
`@anthropic-ai/sdk@0.126.0`. The Codex contract is frozen from tag
`rust-v0.154.0`, especially `codex-rs/exec/src/exec_events.rs` and
`event_processor_with_jsonl_output.rs`.

This is a target contract, not a claim about the current implementation. The
current Lean runner still uses direct Rust CLI adapters pinned to Claude Code
`2.1.270` and Codex `0.153.3`. Adopting this document therefore requires an
explicit amendment to draft 6's “No Claude Agent SDK” decision and restoration
of a small pinned Node runtime component for Claude. The existing Rust runner
must not be silently described as SDK-based.

## 2. Decision: Agents is not a Workspace module, but its runner is

The durable Agents bounded context is an independent control-plane service.
Only its ephemeral in-VM execution projection is the Agent-owned,
Workspace-hosted `agent.turn` module.

| Concern | Owner |
|---|---|
| Tasks, attempts, chats, delivered transcript, editable inbox, questions, permissions, usage, provider-session bindings | Agents |
| Workspace lifecycle, VM incarnation, module catalog, module journal, stop barrier, durable workspace sync | Workspaces |
| VM placement, boot, lease, isolation, resources, process supervision, opaque network attachment | Worker & Host |
| Claude SDK host and Codex CLI adapter inside the VM | `agent.turn`, owned and released by Agents |

The module boundary is useful because it gives Agents the workspace-native
files, shell, Git, and tests it needs without giving the Agents control plane a
host shell or VM lifecycle client. It also gives Workspaces one uniform,
versioned invocation and journal contract.

The trade-off is that a module execution is ephemeral. It cannot own chats,
questions, inbox messages, transcript truth, provider credentials, or recovery
policy. Treating all of Agents as a module would couple durable product state
to a replaceable VM, make sleep/resume lossy, and make Workspaces understand
provider semantics. The chosen cut keeps only execution in the module.

## 3. Three exhaustive contract categories

Every cross-boundary fact belongs to exactly one category:

| Category | Meaning | Examples |
|---|---|---|
| **C1 — control-plane command or lifecycle fact** | Request/response outside the ordered turn event stream. It may start, inspect, cancel, wake, restore, harvest, or reconcile work. | `workspace.runtime.ensure`, `modules.invoke`, `Query.interrupt()`, process exit. |
| **C2 — ordered nonterminal turn event** | A replayable observation within one `exec_id`. It can update projections but can never finish the attempt. | Message delta, tool completion, question opened, usage update, provider retry. |
| **C3 — exactly one terminal turn outcome** | The sole durable completion authority for an `exec_id`. | Succeeded, failed, cancelled, interrupted, timed out, suspended. |

Consequences:

1. `AgentTurnEventV1` contains only C2 members. There is no `turn.completed`
   or `turn.failed` event.
2. `AgentTurnOutcomeV1` is the only C3 value and is written once.
3. Provider terminal-looking signals are C3 *candidates*. The adapter waits for
   the provider-specific settlement rule, then emits one normalized outcome.
4. An invocation disposition, RPC error, VM lifecycle transition, process exit,
   or SDK method response is C1, never a fabricated turn event.
5. Unknown provider variants fail the compatibility gate. They are not silently
   reclassified as success.

## 4. APIs Agents consumes from Workspaces

Agents calls only the Workspace Runtime facade. It never calls the dispatcher,
host, lease service, or VM tunnel directly.

### 4.1 Resolve or wake a runtime

```typescript
type WorkspaceRuntimeEnsureRequestV1 = {
  contract_version: "workspace-agent-control/v1";
  request_id: string;
  idempotency_key: string;
  workspace_id: string;
  reason: "agent_turn" | "agent_question_resume" | "agent_reconcile";
  deadline_at: string;
};

type WorkspaceRuntimeHandleV1 = {
  workspace_id: string;
  execution_instance_id: string;
  vm_id: string;
  workspace_epoch: number;
  state: "starting" | "ready" | "stopping" | "failed";
  module_catalog_version: string;
  dispatcher_contract_version: string;
};
```

```text
workspace.runtime.ensure(request) -> WorkspaceRuntimeHandleV1
workspace.runtime.get(workspace_id) -> WorkspaceRuntimeHandleV1
```

The reasons are deliberately narrow:

- `agent_turn`: run the next eligible inbox reservation, including queued work
  submitted while the VM was asleep.
- `agent_question_resume`: an answer is durably accepted for a suspended
  question and Agents is ready to restore state and launch its successor.
- `agent_reconcile`: resolve an ambiguous prior operation, for example a lost
  `modules.invoke` response, an unacknowledged outcome, a reservation left in
  `delivery_unknown` after runner loss, or an incomplete stop receipt. It does
  not mean ordinary queued-message replay.

### 4.2 Resolve and invoke the module

```text
modules.describe(workspace_id, expected_vm_id, "agent.turn")
  -> ModuleDescriptionV1

modules.invoke(AgentTurnInvokeRequestV1)
  -> ModuleInvokeDispositionV1<AgentTurnOutcomeV1>

modules.status(workspace_id, expected_vm_id, exec_id)
  -> unknown | queued | running | finished(outcome)

modules.cancel(workspace_id, expected_vm_id, exec_id, reason)
  -> signalling | already_finished(outcome) | not_found
```

```typescript
type AgentTurnInvokeRequestV1 = {
  contract_version: "workspace-agent-control/v1";
  request_id: string;
  idempotency_key: string; // exec_id
  workspace_id: string;
  expected_vm_id: string;
  exec_id: string;         // (task_id, attempt, vm_id)
  module_id: "agent.turn";
  module_version: string;
  manifest_hash: string;
  operation: "run";
  input_schema: "g5.agent.turn.input/v1";
  input: AgentTurnInputV1;
  grant_id: string;
  deadline_at: string;
};

type ModuleInvokeDispositionV1<T> =
  | { disposition: "started"; exec_id: string; vm_id: string }
  | { disposition: "already_running"; exec_id: string; vm_id: string }
  | { disposition: "already_finished"; exec_id: string; vm_id: string; outcome: T };
```

Agents does not mark an attempt running until `started` or `already_running`.
After an ambiguous response it retries the same `exec_id`. It never creates a
new attempt merely to disambiguate delivery.

### 4.3 Replay, acknowledge, and lifecycle

```text
modules.events.replay(workspace_id, expected_vm_id, exec_id, from_seq)
  -> stream<ModuleEventEnvelopeV1>
modules.events.ack(workspace_id, expected_vm_id, exec_id, up_to_seq)
  -> ModuleEventAckV1
modules.outcomes.pending(workspace_id, cursor)
  -> Page<ModuleOutcomeEnvelopeV1>
workspace.runtime.events(cursor)
  -> stream<WorkspaceRuntimeEventV1>
```

Events are monotonic per `exec_id`. Live delivery is queued behind replay.
Acknowledgement may advance only through a contiguous prefix persisted by
Agents. Workspaces retains events and outcomes until acknowledged. Every
request is fenced by `expected_vm_id`; a replacement incarnation rejects it.

## 5. The `agent.turn` module contract

```yaml
module_id: agent.turn
module_version: 1.0.0
lifecycle: bounded_run
entrypoint: agent-turn-runner
operations:
  run:
    input_schema: g5.agent.turn.input/v1
    event_schema: g5.agent.turn.event/v1
    output_schema: g5.agent.turn.outcome/v1
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

The manifest requests capabilities; Workspaces grants their intersection with
runtime policy. The module input cannot contain arbitrary `argv`, environment,
absolute host paths, signals, credentials, or host endpoints.

```typescript
type AgentTurnInputV1 = {
  schema_version: "g5.agent.turn.input/v1";
  task: {
    task_id: string;
    attempt: number;
    session_id: string;
    initial_input?: AgentSessionInputReservationV1;
  };
  provider: {
    provider_id: "claude_code" | "codex";
    model_id: string;
    permission_mode: "non_prompting" | "tool_approval";
    options: Record<string, unknown>;
  };
  prompt_ref: { artifact_ref: string; sha256: string };
  workspace_target: { repo_id?: string; relative_path?: string };
  proxy_binding_ref: string;
  session_binding: {
    provider_session_id?: string;
    expected_manifest_generation?: number;
    restore_ref?: string;
  };
  resume?: {
    reason: "question_answered";
    prior_exec_id: string;
    question_id: string;
    answer_ref: string;
    answer_version: number;
    question_delivery_generation: number;
    session_manifest_generation: number;
  };
  limits: {
    wall_clock_ms: number;
    max_turns?: number;
    max_output_bytes: number;
  };
};
```

An ordinary turn has one FIFO-head input reservation. A question-only
successor may omit it. Reservation freezes an input revision for delivery but
does not establish that it reached the provider; only `input.accepted` does.

## 6. APIs Agents provides to Workspaces

These are private control-plane APIs. They do not let Workspaces mutate Agent
tasks or interpret provider state.

### 6.1 Bind and revoke the LLM proxy

```text
agents.runtime_bindings.ensure(AgentProxyBindingRequestV1)
  -> AgentProxyBindingV1
agents.runtime_bindings.revoke(binding_id, expected_vm_id)
  -> revoked | already_revoked | stale_incarnation
```

The returned `secret_ref` is inject-only. It binds provider access to one
workspace, `vm_id`, `exec_id`, provider account, module hash, and expiry. The
real provider credential remains in Agents.

### 6.2 Restore and persist provider sessions

```text
agents.session_bundles.get(AgentSessionRestoreRequestV1)
  -> AgentSessionBundleRefV1 | no_session
agents.session_harvests.persist(AgentSessionHarvestV1)
  -> AgentSessionPersistAckV1
```

The bundle is sealed and opaque to Workspaces. Agents owns provider paths,
format, compatibility, manifest generations, hashes, and recovery. A stale VM
cannot overwrite a newer manifest. If restore is unavailable or incompatible,
Agents creates a new provider session with a bounded history containing only
delivered transcript entries.

Claude SDK `sessionStore` is alpha in `0.3.273`; this contract does not expose
it across the domain boundary. The Claude adapter may use it behind an
Agent-owned sealed-bundle implementation only after crash, timeout, and mirror
failure tests prove equivalent semantics. `SDKMirrorErrorMessage` prevents a
failed mirror from being mistaken for durability.

### 6.3 Coordinate workspace stop

```text
agents.workspace_stops.prepare(stop) -> AgentStopPlanV1
agents.workspace_stops.get(stop_id) -> AgentStopStatusV1
agents.workspace_stops.abort(stop_id, expected_vm_id) -> AgentStopStatusV1
agents.workspace_stops.commit(stop, workspace_runtime_event_id)
  -> AgentStopStatusV1
```

`prepare` closes Agent dispatch for the workspace, settles every input
reservation as delivered or released, resolves or suspends active executions,
persists the event prefix and session bundles, records open questions and
queued inbox state, and revokes proxy bindings. It returns a durable receipt.
Workspaces pairs that receipt with its `SyncReceipt` before releasing the VM.

Inputs submitted during drain remain durably queued and editable, but are not
reserved into the draining VM. An answer accepted during drain becomes
`resume_pending` and wakes the next incarnation only after release completes.

### 6.4 Read-only activity projection

```typescript
type AgentWorkspaceActivityV1 = {
  workspace_id: string;
  agent_admission: "open" | "closed";
  active_exec_ids: string[];
  open_question_ids: string[];
  active_task_count: number;
  queued_input_count: number;
  reserved_input_count: number;
  delivery_unknown_input_count: number;
  latest_agent_event_id?: string;
};
```

```text
agents.workspace_activity.get(workspace_id) -> AgentWorkspaceActivityV1
```

This is display and stop-planning data, not a second source of Agent truth.

## 7. Worker & Host dependency

Agents has no Worker & Host API for placement, create, wake, lease, status,
stop, release, shell, process launch, or networking. Workspaces alone uses
those APIs.

Worker & Host supplies only domain-neutral facilities behind Workspaces:

- a signed execution-incarnation attestation used when binding the proxy;
- a pinned module artifact and Node/runtime image capable of running the
  Claude SDK package and bundled Claude Code binary;
- a pinned Codex `0.154.0` binary;
- bounded process supervision, resources, filesystem, and network grants;
- the module event journal and opaque sealed session-bundle transport.

It must not learn Agent task, transcript, question, model, usage, or provider
credential schemas. Process exit and resource-limit facts are C1 inputs to the
module adapter; the adapter, not Worker & Host, converts them into C3 outcomes.

## 8. Inbox, steering, questions, and replay

### 8.1 Durable input states

```text
queued -> reserved -> delivered
   |         |
   |         +-> delivery_unknown -> delivered | released
   +-> withdrawn
```

- `queued`: editable, withdrawable, and steerable. It is not in the delivered
  transcript and may be combined, reordered only by an explicit Agent policy,
  or redirected to a new branch.
- `reserved`: one immutable revision is assigned to an execution. UI mutation
  creates a new queued revision or withdrawal; it does not rewrite the
  reservation.
- `delivered`: provider acceptance was durably observed. The exact revision is
  append-only transcript truth and can never be deleted or rewritten.
- `delivery_unknown`: runner loss occurred after send but before a conclusive
  acceptance record. Reconcile the provider session before releasing or
  redelivering it.
- `withdrawn`: the user removed the message before delivery. It is absent from
  provider replay but retained as an audit fact according to retention policy.

Typing three messages while the first turn runs creates three queued inbox
rows. Claude may receive selected queued rows through live `streamInput`; Codex
cannot because `codex exec` closes its one-shot input. Any rows not accepted
remain queued across VM sleep. They are not blindly replayed: the next dispatch
selects their then-current revisions and branch/steering disposition.

`input.accepted` must cite `input_id`, `revision`, `reservation_id`, and
`delivery_generation`. For Claude, acceptance requires the SDK's correlated
user echo or a reply frame carrying the submitted UUID. A successful
`streamInput()` call alone is not acceptance. For Codex, the adapter records
acceptance only after the process has started with the frozen prompt and the
write/close boundary is durably known; an ambiguous crash becomes
`delivery_unknown`.

Provider-side retraction such as Claude's
`model_refusal_fallback.retracted_message_uuids` may retract provider-generated
assistant/tool projection rows. It never deletes a delivered Agent user input.

### 8.2 Questions

An open question is durable Agent state, not a live callback:

```text
open -> answered -> consumed
  |        |
  |        +-> resume_pending -> consumed
  +-> expired | cancelled
```

If the user answers while the same runner is alive, the Agent callback consumes
the answer and emits `question.consumed`. If the runner wait deadline passes,
the execution emits C3 `suspended`, its provider session is harvested, and the
VM may sleep. A later answer is persisted first, then
`workspace.runtime.ensure(reason = "agent_question_resume")` starts a new
execution linked to the suspended one. Answer delivery is fenced by question
id, answer version, and delivery generation so answer-versus-suspend races are
idempotent.

### 8.3 When replay is relevant

Replay means one of four different operations and the implementation must not
conflate them:

1. Workspace journal replay: redeliver unacknowledged C2/C3 envelopes.
2. Provider session restore: restore opaque Claude/Codex session artifacts.
3. Delivered-history recovery: start a new provider session from delivered
   transcript only when native restore is unavailable.
4. Inbox dispatch: select currently queued revisions for new work. This is not
   transcript replay and respects edits, withdrawals, and steering.

## 9. Provider-neutral event and outcome schemas

The complete C2 union is:

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
  | { kind: "question.opened"; question_id: string; question_kind: string;
      payload_ref: string; runner_wait_deadline_at: string; answer_expires_at?: string }
  | { kind: "question.consumed"; question_id: string; answer_version: number;
      delivery_generation: number }
  | { kind: "permission.opened"; question_id: string; tool: string;
      sanitized_input_ref: string; expires_at: string }
  | { kind: "permission.resolved"; question_id: string;
      decision: "allow" | "deny" | "cancel"; source: string }
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
  | { kind: "terminal.progress"; phase: "harvesting" | "finalizing" }
  | { kind: "session.title"; title: string; source: "provider_summary" };
```

`session.title` is emitted once, after a successful turn, only when the
dispatch asked for it (`provider.options.session_title = true`, set while
`chat_sessions.title_source` is still `derived`: the provisional title taken
from the first user message). The API applies it only to such platform-owned
titles and records `title_source = 'summary'`; a title the user typed or
cleared (`title_source = 'user'`) is never replaced. The call runs isolated
from the workspace: no tools, no MCP servers, no hooks or project settings,
no session persistence, from a scratch directory under `$HOME`, within the
turn's remaining `wall_clock_ms` budget, and its usage is folded into the
turn outcome. Claude Code sessions only: `codex exec` cannot be run
tool-less, so Codex sessions keep the first-message title.

Large or sensitive bodies use immutable, access-controlled references. A C2
`notice` may describe an error, but it does not terminate the execution.

```typescript
type AgentTurnOutcomeV1 = {
  schema_version: "g5.agent.turn.outcome/v1";
  status: "succeeded" | "failed" | "cancelled" | "interrupted" |
          "timed_out" | "suspended";
  code: string;
  retryable: boolean;
  final_message_ref?: string;
  blocker?: { kind: "question" | "permission"; id: string };
  provider_session_id?: string;
  session_manifest_generation?: number;
  usage?: UsageV1;
  last_event_seq: number;
};
```

## 10. Claude Agent SDK `0.3.273` mapping

### 10.1 Required SDK interactions

| SDK interaction | Category | Contract use |
|---|---|---|
| `query({prompt: AsyncIterable<SDKUserMessage>, options})` | C1 | Start or resume the long-lived streaming-input session. Always stamp human input with `origin: {kind: "human"}`. |
| `options.resume`, `sessionId`, `persistSession`, Agent-owned session storage | C1 | Bind or restore the provider session. Never infer durable restore from VM-local files alone. |
| `options.cwd`, `env`, `executable`, `pathToClaudeCodeExecutable`, `mcpServers`, `model`, `thinking`, `effort`, `maxTurns`, `maxBudgetUsd`, `permissionMode` | C1 | Construct the validated execution. `env` replaces rather than merges the environment, so the adapter must explicitly copy its allowlisted base. |
| `options.canUseTool`, `onUserDialog`, `onElicitation` | C1→C2 | Bridge callbacks to durable permission/question records. Callback IDs are idempotency keys. |
| `options.includePartialMessages`, `includeHookEvents`, `forwardSubagentText` | C1 | Enable only the signal families this mapping consumes. |
| `initializationResult()`, `supportedCommands()`, `supportedModels()`, `supportedAgents()`, `mcpServerStatus()` | C1→C2 snapshot | Preflight capabilities and emit `provider.capabilities`. |
| `streamInput()` | C1 | Deliver an immutable reservation. Resolution is not acceptance; wait for correlated provider evidence. `shouldQuery:false` appends context for the next querying message and must still be bound to a reservation. |
| `interrupt()` | C1→C3 candidate | Graceful user/workspace cancellation. Reconcile any returned `still_queued` UUIDs before settling reservations. |
| `setPermissionMode()`, `setMcpPermissionModeOverride()`, `setModel()`, `applyFlagSettings()` | C1 | Explicit live session changes. Persist the desired Agent state before calling. |
| `getContextUsage()` | C1→C2 | On-demand `context.usage`. The experimental usage method is not a required dependency. |
| `reinitialize()` | C1 | Repair a transport gap and redeliver pending callbacks. Callback implementations must dedupe by request id. |
| `rewindFiles()` | C1 | Optional explicit user action; never automatically follows chat message deletion. |
| `backgroundTasks()`, `stopTask()`, `backgroundTasksChanged` | C1/C2 | Allowed only within a live execution in v1. Cross-VM background continuation is not supported. |
| `close()` | C1 | Final teardown after outcome arbitration; it is not success or cancellation evidence. |

The adapter sets `CLAUDE_CODE_EMIT_SESSION_STATE_EVENTS=1`. It waits for the
terminal `SDKResultMessage`, `session_state_changed: idle`, iterator settlement,
and subprocess settlement. A missing or contradictory member of that set is a
protocol failure, not guessed success. Prompt suggestions may arrive after the
result and before idle.

### 10.2 Complete top-level `SDKMessage` union

| Exact SDK member | Category | Normalization / projection |
|---|---|---|
| `SDKAssistantMessage` | C2 | Decompose content into message, thought, tool, fallback, usage, and artifact events. |
| `SDKUserMessage` | C2 | Tool result or correlated accepted user input; never append an uncorrelated user echo as human transcript. |
| `SDKUserMessageReplay` | C2 | Reconciliation evidence for an already identified input; dedupe by UUID and delivery generation. |
| `SDKResultMessage` | C3 candidate | Supplies subtype, `is_error`, terminal reason, usage, permission denials, and result. Never emit as C2 completion. |
| `SDKSystemMessage` (`system/init`) | C2 | `provider.session` plus capabilities/model snapshot. |
| `SDKPartialAssistantMessage` | C2 | Decompose the nested stream union in §10.3. |
| `SDKCompactBoundaryMessage` | C2 | `context.compacted`. |
| `SDKStatusMessage` | C2 | `provider.status`; a null/clear state clears the prior status. |
| `SDKAPIRetryMessage` | C2 | `provider.status: retrying` and retry notice. |
| `SDKControlRequestProgressMessage` | C2 | `provider.control_progress`. |
| `SDKModelRefusalFallbackMessage` | C2 | Model/session notice; emit `message.retracted` for `retracted_message_uuids`. Never retract delivered Agent user input. |
| `SDKModelRefusalNoFallbackMessage` | C3 candidate | Refusal failure candidate; result/idle settlement remains authoritative. |
| `SDKLocalCommandOutputMessage` | C2 | Bounded/redacted `notice` or tool progress associated with the command. |
| `SDKHookStartedMessage` | C2 | Tool/hook progress. |
| `SDKHookProgressMessage` | C2 | Tool/hook progress. |
| `SDKHookResponseMessage` | C2 | Tool/hook completion or notice. |
| `SDKPluginInstallMessage` | C2 | `provider.status: installing`; completion clears it. |
| `SDKToolProgressMessage` | C2 | `tool.progress`. |
| `SDKAuthStatusMessage` | C2 | `provider.status: authenticating`; `isAuthenticating:false` clears it. |
| `SDKTaskNotificationMessage` | C2 | `subtask.completed` or task notice. |
| `SDKTaskStartedMessage` | C2 | `subtask.started`. |
| `SDKTaskUpdatedMessage` | C2 | `subtask.progress`. |
| `SDKTaskProgressMessage` | C2 | `subtask.progress`. |
| `SDKBackgroundTasksChangedMessage` | C2 | `background_tasks.replaced`; replace the entire process-local live set. Reset to empty on process restart. |
| `SDKThinkingTokensMessage` | C2 | `thinking.progress`; approximate UI progress, never billed usage. |
| `SDKSessionStateChangedMessage` | C2 or C3 candidate | `running`/`requires_action` are status; `idle` is terminal-settlement evidence. |
| `SDKWorkerShuttingDownMessage` | C2 | Live-tail notice only. Historical replay is possible and absence is not host-death evidence. |
| `SDKCommandsChangedMessage` | C2 | `commands.replaced`; replace the cached list. |
| `SDKNotificationMessage` | C2 | `notice`. |
| `SDKFilesPersistedEvent` | C2 | `artifact.persisted`. |
| `SDKToolUseSummaryMessage` | C2 | Tool summary/progress; do not parse prose into completion. |
| `SDKMemoryRecallMessage` | C2 | `memory.recalled`. |
| `SDKRateLimitEvent` | C2 | `usage.updated` quota snapshot and notice. |
| `SDKElicitationCompleteMessage` | C2 | Question/elicitation resolution. |
| `SDKPermissionDeniedMessage` | C2 | `permission.resolved: deny`; advisory only because result permission denials are authoritative. |
| `SDKPromptSuggestionMessage` | C2 | `suggestion.available`. |
| `SDKMirrorErrorMessage` | C2 plus C3 candidate | Durability notice; fail/suspend if required session state cannot be durably harvested. |
| `SDKInformationalMessage` | C2 or C3 candidate | `notice`; `prevent_continuation:true` is a failure candidate, not an immediate outcome. |
| `SDKConversationResetMessage` | C2 | `conversation.reset`; update provider binding without deleting the Agent transcript. |

This table has all 39 members of the `0.3.273` `SDKMessage` union.

### 10.3 Complete nested Anthropic stream and content unions

`BetaRawMessageStreamEvent` has six variants:

| Variant | Category and mapping |
|---|---|
| `message_start` | C2: initialize message/usage correlation. |
| `content_block_start` | C2: open the mapped block. |
| `content_block_delta` | C2: dispatch the six delta variants below. |
| `content_block_stop` | C2: close the mapped block; not turn completion. |
| `message_delta` | C2: stop/usage metadata; not turn completion. |
| `message_stop` | C2: provider-message boundary; not C3. |

The delta union is exhaustive: `text_delta` → `message.delta`,
`thinking_delta` → `thought.delta`, `input_json_delta` → buffered tool input,
`citations_delta` → message metadata, `signature_delta` → sealed thought
metadata, and `compaction_delta` → `context.compacted` progress.

`BetaContentBlock` has 17 variants:

| Exact block | C2 mapping |
|---|---|
| `text` | `message.final` and citation metadata. |
| `thinking` | `thought.final`. |
| `redacted_thinking` | Sealed thought metadata; never expose plaintext. |
| `tool_use` | `tool.started`. |
| `server_tool_use` | `tool.started`. |
| `web_search_tool_result` | `tool.completed`. |
| `web_fetch_tool_result` | `tool.completed`. |
| `advisor_tool_result` | `tool.completed`. |
| `code_execution_tool_result` | `tool.completed`. |
| `bash_code_execution_tool_result` | `tool.completed`. |
| `text_editor_code_execution_tool_result` | `tool.completed` plus `workspace.change` when applicable. |
| `tool_search_tool_result` | `tool.completed`. |
| `mcp_tool_use` | `tool.started`. |
| `mcp_tool_result` | `tool.completed` and referenced artifacts. |
| `container_upload` | `artifact.persisted`. |
| `compaction` | `context.compacted`. |
| `fallback` | Model-fallback notice marking the boundary from the refusing model to the fallback model; not completion. |

### 10.4 Claude terminal mapping

`TerminalReason` is closed over these 19 values:

| Reason | Normalized C3 disposition |
|---|---|
| `completed` | `succeeded`, only when result subtype/`is_error`, idle, iterator, and process agree. |
| `tool_deferred` | `suspended` only when a durable question or permission blocker exists; otherwise protocol failure. |
| `aborted_streaming`, `aborted_tools` | `cancelled` for a matching requested cancellation; otherwise `interrupted`. |
| `max_turns`, `budget_exhausted`, `blocking_limit`, `rapid_refill_breaker` | `failed`, policy/limit code; retryability comes from the exact condition. |
| `prompt_too_long`, `image_error`, `malformed_tool_use_exhausted`, `structured_output_retry_exhausted` | `failed`, non-retryable unless the Agent changes input. |
| `model_error`, `api_error`, `turn_setup_failed` | `failed`; retryability follows structured provider error classification. |
| `stop_hook_prevented`, `hook_stopped` | `failed`, hook-specific code. |
| `background_requested` | `failed: PROVIDER_BACKGROUND_TASK_UNSUPPORTED` in v1. Background work cannot outlive the bounded module without a future durable protocol. |
| `tool_deferred_unavailable` | `failed`; no durable continuation was established. |

Authentication, organization, account hold, verification, billing, rate limit,
overload, invalid request, model not found, server, maximum output token, cloud
credential, and unknown assistant errors refine failure codes. They do not add
another terminal channel.

## 11. Codex CLI `0.154.0` mapping

### 11.1 Required CLI interactions

| Interaction | Category | Contract use |
|---|---|---|
| `codex exec --json --strict-config --ignore-user-config ...` | C1 | Start a fresh one-shot turn with a frozen prompt and validated cwd/config. |
| `codex exec resume <thread_id> --json ...` | C1 | Resume an exact provider thread after its artifacts are restored. |
| stdin prompt write and close | C1 | Deliver the one reserved input. Codex has no SDK-equivalent live steering channel. |
| stdout JSONL parse | C2/C3 candidates | Decode only the closed unions below. Unknown/malformed records fail the protocol. |
| stderr | C2 | Bounded/redacted notice only; never completion authority. |
| signal/kill escalation | C1→C3 candidate | Graceful cancel, then bounded force. Settle only after event and process arbitration. |
| rollout/session file restore and harvest | C1 | Agent-owned provider-session durability. |
| process exit/EOF | C1→C3 candidate | Required settlement evidence, not success by itself. |

Do not use `--approve-for-me` as a substitute for a user-mediated permission
contract. Do not use `--dangerously-bypass-approvals-and-sandbox` or
`--dangerously-bypass-hook-trust`. `exec fork` and `exec review` are outside the
v1 turn contract. Every admitted flag must be in a version-gated argv test.

### 11.2 Complete public JSONL top-level union

| `ThreadEvent` variant | Category | Normalization |
|---|---|---|
| `thread.started` | C2 | `provider.session` with the thread id. |
| `turn.started` | C2 | `provider.status: running`. |
| `turn.completed` | C3 candidate | Success candidate; wait for EOF and exit zero. |
| `turn.failed` | C3 candidate | Failure candidate; wait for EOF and process settlement. |
| `item.started` | C2 | Open the mapped item. |
| `item.updated` | C2 | Update the mapped item. |
| `item.completed` | C2 | Close the mapped item; never finish the turn. |
| `error` | C3 candidate | Record structured failure candidate and optional C2 notice. Later turn/process settlement arbitrates once. |

The JSONL event processor treats an interrupted internal turn as shutdown
without a public `turn.*` terminal event. Therefore cancellation/interruption
also requires the C1 signal receipt, EOF, and process status; absence of
`turn.completed` is never success.

### 11.3 Complete public item union

| `ThreadItemDetails` variant | C2 mapping |
|---|---|
| `agent_message` | `message.delta`/`message.final`. |
| `reasoning` | `thought.delta`/`thought.final`. |
| `command_execution` | `tool.started`/`tool.progress`/`tool.completed`; statuses are `in_progress`, `completed`, `failed`, `declined`. |
| `file_change` | `workspace.change`; public JSONL emits it completed even though the schema includes `in_progress`, `completed`, `failed`. |
| `mcp_tool_call` | Tool events; statuses are `in_progress`, `completed`, `failed`. |
| `collab_tool_call` | Subtask events; public tools are `spawn_agent`, `send_input`, `wait`, `close_agent`, with agent statuses `pending_init`, `running`, `interrupted`, `completed`, `errored`, `shutdown`, `not_found`. |
| `web_search` | Tool events. |
| `todo_list` | `plan.updated`. |
| `error` | C2 notice attached to the item; top-level terminal arbitration remains separate. |

The internal processor also handles collaboration variants that are not in
`exec_events.rs`'s public JSONL contract. They are outside adapter input, not
missing normalized event kinds. The upgrade gate diffs the public Rust enums
and processor together so a newly exposed variant cannot pass unnoticed.

## 12. Durable Agent projections and their category inputs

| Durable projection | Authority | Category inputs | Rule |
|---|---|---|---|
| Task and attempt | Agents | C1 start disposition; C3 outcome | Attempt becomes running only after confirmed module start and terminal exactly once. |
| Delivered transcript | Agents | C2 `input.accepted`, `message.final`, retraction | Delivered user rows are immutable. Provider assistant/tool rows may be tombstoned, never silently erased. |
| Session inbox and revisions | Agents | C1 user mutations; C2 acceptance; C3 release/reconcile | Queued rows are editable/withdrawable; the delivered revision is append-only. |
| Input reservation | Agents | C1 dispatch; C2 acceptance; C3/runner loss | One execution owns a frozen revision and delivery generation. |
| Open question/answer | Agents | C2 question events; C3 suspension; C1 answer/resume | Answers are durable before callbacks or wake. |
| Permission request | Agents | C2 permission events; C3 suspension/failure | Decision is idempotent and correlated; sanitized display is separate from provider input. |
| Provider-session binding | Agents | C2 session/reset; C1 restore/harvest; C3 | Bound to provider, workspace, chat, manifest generation, and last accepted input cut. |
| Session bundle manifest | Agents | C1 harvest acknowledgement; C2 mirror error | Monotonic generation and content hash; stale VM writes fail. |
| Message/tool/subtask live projection | Agents | C2 only | Rebuildable from ordered events; never decides attempt completion. |
| Usage and quota ledger | Agents | C2 usage/rate-limit; C3 final usage | Deduplicated by provider request/event identity; final totals reconcile incremental data. |
| Workspace activity | Agents | C1/C2/C3 | Read-only summary, not task authority or stop proof. |
| Event replay cursor | Agents | C1 replay/ack after durable C2/C3 write | Ack only contiguous durable prefixes. |
| Stop receipt | Agents | C1 stop protocol plus durable projection cuts | Proves no Agent-only state remains solely in the VM. |

## 13. Compatibility and acceptance gates

A provider bump is a schema migration, even if semver labels it patch/minor.
The bump must fail CI until all of the following pass:

1. Exact-package lock and startup checks prove Claude Agent SDK `0.3.273`, its
   bundled Claude Code `2.1.273`, peer Anthropic SDK `0.126.0`, and Codex
   `0.154.0`.
2. Generated exhaustiveness fixtures prove all 39 `SDKMessage` members, six
   stream events, six stream deltas, 17 content blocks, 19 terminal reasons,
   eight Codex events, and nine Codex item kinds are classified.
3. Unknown discriminants, malformed JSONL, contradictory terminal evidence,
   missing result/idle/EOF, and nonzero exits cannot become success.
4. Golden tests cover fresh turn, native resume, restore fallback, live Claude
   steering, queued edits/withdrawal, question answer before and after sleep,
   answer-versus-suspend, permission allow/deny, cancellation, timeout, VM
   loss, refusal retraction, conversation reset, SDK mirror failure, and Codex
   interruption without a terminal JSONL record.
5. The session durability test kills the VM after provider acceptance but
   before harvest acknowledgement and proves the input is neither lost nor
   duplicated.
6. Redaction tests prove raw credentials, proxy tokens, hidden reasoning,
   unsanitized permission inputs, and unrestricted provider files never cross
   the declared contract.

## 14. Required implementation change from the current Lean runner

Adopting this document requires a deliberate migration, not only two pin edits:

1. Amend `lean-backend-design-v6.md` to replace the direct-Claude-CLI decision
   with the exact SDK baseline in §1 while retaining Codex CLI.
2. Add exact pin/lock sources for the Claude Agent SDK and its peer Anthropic
   SDK; bump the Codex pin to `0.154.0`.
3. Add an Agent-owned TypeScript/Node Claude adapter inside `agent.turn`.
   Rust remains the module supervisor, journal client, and Codex adapter; it
   communicates with the Claude adapter over a small framed local protocol.
4. Package Node and all SDK native artifacts in every supported worker image.
   Startup must validate the SDK, bundled CLI, platform artifact, and schema
   fingerprint before accepting work.
5. Preserve the Workspace module input/event/outcome schemas while replacing
   only the Claude provider adapter. No SDK type crosses into Workspaces or
   Worker & Host.
6. Land the exhaustive fixtures and failure-injection tests before switching
   the production Claude path. Keep the old direct adapter only as a bounded
   rollback lane during rollout, then remove it and its obsolete recordings.

Until those steps land, [Provider CLI contract](lean-provider-cli-contract.md)
remains the description of implemented behavior. This document is the target
contract against which the SDK migration should be built.
