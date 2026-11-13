# Agent API Surface — v9

> Status: proposed architecture, draft 9, revised 2026-09-18
>
> Purpose: define the Agent-owned execution contract and shared agent-task lifecycle adopted by [design v9](lean-backend-design-v9.md) and [implementation plan v9](lean-implementation-plan-v9.md). This versioned companion preserves [v8](agent-api-surface-v8.md) and [the historical API note](agent-api-surface.md). Its target behavior is distinct from the currently implemented adapters described in [Provider CLI contract](lean-provider-cli-contract.md).
>
> **Scope amendment, 2026-09-19.** The MVP keeps the original G5's microVM, host, secret and identity mechanisms; credentials are environment variables. Sections and paragraphs marked **Post-MVP (id)** below are specification for the named post-MVP PR and are not adopted by the MVP design. See [v9-mvp-scope-amendment.md](v9-mvp-scope-amendment.md).
>
> Marked in this file, all P4.12: the host-proxies item of the §2 ownership table, in §5 the `network.agent_llm_proxy` capability, the no-inline-credentials rule before `AgentTurnInputV1` and its `owner_proxy_binding_ref` field, the local model broker of §6.1, the deferred provider writes of §6.5, the bootstrap capability of §6.6, the host-proxy sentence of §7, the billing paragraph of §9, the billing check of §12.1 and gate 9 of §13 in part. A marker that names a part after the id, as in **Post-MVP (P4.12: the new scoped authorization)**, covers only that part; the rest of that paragraph or row stays MVP. The graph DTO, receipt and caps of §6.6, the redaction rule and the quota headers stay MVP.

## 1. Provider baseline and evidence

The repository pins Claude Code `2.1.285` and Codex CLI `0.159.3` in `scripts/claude-code-cli-version` and `scripts/codex-cli-version`; an explicitly scoped upgrade raised them from `2.1.270` and `0.153.3`. When this revision was drafted, the Claude runner suppressed `AskUserQuestion`, sent permissions through a custom MCP tool and closed stdin after one input; those were migration targets, not the revised design. The current Codex runner uses `exec --json`. This revision authorizes no Claude Agent SDK runtime, and a later pin bump needs its own scoped upgrade.

| Provider | MVP target | Later change |
|---|---|---|
| Claude Code `2.1.285` | Direct CLI, stream-json input/output, native `AskUserQuestion` and native tool-permission requests. Keep stdin open for correlated control responses during the attempt and while Claude reports background work it owns; one attempt can span Claude's own follow-up turns (§10.1). | A resident CLI for independent live steering/model changes remains separate scope. |
| Codex `0.159.3` | Direct `codex exec --json`, one input followed by EOF, explicit noninteractive approval/sandbox policy. No interactive question or approval UI and no custom MCP fallback. | Replace the adapter with `codex app-server` over stdio under P3.7, with native request/reply support verified against the adopted binary. |

Anthropic's [user-input documentation](https://code.claude.com/docs/en/agent-sdk/user-input) identifies `AskUserQuestion` as the native clarification tool and distinguishes it from ordinary tool approval. Its [official transport source](https://github.com/anthropics/claude-agent-sdk-python/blob/main/src/claude_agent_sdk/_internal/transport/subprocess_cli.py), [permission routing](https://github.com/anthropics/claude-agent-sdk-python/blob/main/src/claude_agent_sdk/types.py) and [control-message handler](https://github.com/anthropics/claude-agent-sdk-python/blob/main/src/claude_agent_sdk/_internal/query.py) show the CLI protocol underlying that SDK. These sources justify a Rust CLI transport implementation; they do not establish that every current schema field has been recorded on `2.1.270`. The local installed CLI inspected during the v8 revision reported `2.1.273`, so it is not evidence at the repository pin. Exact-pin native-control recordings remain an implementation acceptance gate (§13).

For Codex, [official noninteractive documentation](https://developers.openai.com/codex/noninteractive/) describes `exec` as a scripted interface with preset policies. The [official App Server documentation](https://developers.openai.com/codex/app-server/) describes the future bidirectional stdio interface. Current online documentation is a design reference, not proof of the pinned `0.159.3` App Server schema; the later implementation must generate/probe the schema from its chosen pinned binary.

## 2. Ownership and one shared agent-task model

Agents is a control-plane bounded context. Only its ephemeral runner is the Agent-owned, Workspace-hosted `agent.turn` module.

| Concern | Owner |
|---|---|
| Logical tasks and attempts, jobs/dependencies, shared conversations, editable inbox, transcript, provider-request projection/replies, provider-session files and usage | Agents |
| Workspace owner/read access, lifecycle, transfer, VM incarnation, catalog/journal, admission, flush/restore, ordinary forks | Workspaces |
| Placement, boot, leases, VM isolation and resource supervision, host proxies (post-MVP, P4.12: host proxies) | Worker & Host |
| Claude and Codex native protocol adapters inside `agent.turn` | Agents |

### 2.1 Owner authority and previews

The current `owner_user_id` authorizes user-initiated work: submitting/editing agent input, answering native requests, changing policy, creating/cancelling tasks, terminal use and preview controls. Org admins take ownership first. Workspaces/TaskService and the declared trusted bootstrap module may create scoped child tasks under that recorded owner authorization (§6.6); provider/repository shell descendants cannot. All resulting work passes the same lifecycle admission, explicit policy and dependency validation. Each incarnation uses its fixed owner's provider account. Session identifiers are accounting tags, not grants; transfer changes accounts only after the old incarnation stops and releases.

Non-owners browse repository content only through authorized durable Git snapshots, irrespective of whether the source has a running VM. They may also read permitted durable platform metadata, including sanitized persisted conversations, task state and ordinary task output. They have no access to source VM file reads/watches, module inspection, ports, terminal, raw logs or preview relays; a persisted timeline is not a live VM inspection capability. Reader forks use the durable Git source selected by Workspaces and never wake, flush, interrupt or otherwise invoke the source VM. The child is owned by its creator; code contributions return through the authorized merge flow, never a write grant to the parent.

Owners also have a read-only workspace while no VM is attached. Editing/uploading and execution controls require explicit owner Start/Resume and a ready attachment; reads, Save, uploads, historical questions and ordinary inbox writes cannot secretly wake it. A task API can reserve queued work as part of the same recorded explicit Start/Resume action, but cannot use task creation as an implicit attachment path. Ungraceful VM loss holds replacement execution for current-owner Resume. Deliberate drain/rollout may carry a previously running owner's recorded continuation authorization, but never defeats a later user stop, transfer or cancellation.

A Preview button still creates a durable `kind=preview` task visible and cancelable in the task queue. Its permission is owner-only through every task and preview API; there is no reader exception. It has no bounded-task concurrency charge, wall-clock timeout or automatic restart. [Workspace VM API v9](workspace-vm-api-v9.md) specifies preview replacement and cancellation. Preview work never becomes an `agent.turn` merely because it was launched from a conversation. On incarnation loss, every preview intent bound to that incarnation/start authorization, including an unsent replacement waiting on the old service, becomes terminal and cannot launch on a later Resume. Preserve a prior terminal cancellation; otherwise settle failed with `vm_lost`. A later independently authorized fresh Start is not swept as old work. Confirmed `started` makes the task running; serving readiness requires its configured port-open or declared health-probe fact. Before readiness or after port closure without process exit, relay returns `503 PREVIEW_NOT_READY` and the panel shows not-serving while the task remains running/cancellable.

Tasks use the workspace's member repositories and shared working branch. Native Git operates on those checkouts; managed workspace branches are created only by the checked fork operation. Committing/pushing is native agent or owner work, not a platform save/checkpoint instruction or a separate commit module.

### 2.2 Logical task, conversation and input provenance

`tasks.kind` is `agent | command | preview`. There is no `chat_turn` kind and no separate chat-task state machine. Every agent task, including uplift/downlift, review, explainer and custom workflow work, has a shared conversation through existing `chat_sessions`, `chat_messages` and `chat_session_files`, with one `session_inputs` queue for every input origin. Those historical table names do not restrict them to typed chat. An agent task has a non-null conversation/session link; command and preview tasks need no provider conversation.

A human supplies typed input; a workflow supplies rendered template input. Both enter the same versioned inbox, dispatch, provider, transcript, question and outcome path. Record origin as provenance (`human | workflow`), along with immutable task/job linkage and the frozen template/input revision. Input origin never implies permission mode, question support, sandbox level, provider account or a bypass of approvals. A job can create several agent tasks; each uses this shared machinery and an explicitly chosen session, not a new task-only session store. Serialize one nonterminal logical agent task (`queued`, `running` or `waiting`) per shared session across all entry points, and at most one live attempt for that task. Distinct sessions may execute concurrently in the same workspace.

A logical task can have multiple execution attempts. `task_id`, `job_id`, dependencies, conversation link and original input identity remain fixed across continuation after interruption or VM loss. A continuation increments the attempt and gets a new `exec_id=(task_id, attempt, vm_id)`. An eligible continuation leaves the logical task nonterminal, queued or held `waiting` according to policy; it does not fail a parent and create an independent successor. Job/DAG consumers remain blocked until that same logical task reaches its final outcome. Cancel terminalizes the logical task and prevents later attempts. A later unrelated human instruction is new work, not a disguised continuation.

### 2.2.1 Canonical task and attempt state

The authoritative task fields are the design §7.1 fields: `id, org_id, workspace_id, job_id NULL, kind, status, spec, outcome, attempt, vm_id NULL, epoch NULL, start_confirmed, chat_session_id NULL, continuation_reason NULL, hold_reason NULL, created_by, created_at, started_at, ended_at`. Limits live in `spec.limits`; there is no second `wall_clock_limit_secs` authority. P4.1 adds the target fields beside legacy `task_type`, drops `job_id`'s NOT NULL requirement, and backfills workspace identity before requiring it for new runtime tasks. New `agent` tasks require a shared session. Existing legacy rows that cannot be mapped safely remain historical/non-dispatchable until their owning cutover; do not guess a workspace or revive retired merge/golden workflows.

P4.1 widens the existing status CHECK and shared Rust vocabulary to admit `queued|running|waiting|succeeded|failed|cancelled` alongside legacy values. P4.6 writes only these six target values; it never translates `queued` back to `pending` or `succeeded` back to `completed`. P7.1 narrows after explicit historical backfill and removal of old producers. Chat-message `status` and input `delivery` are separate vocabularies, not alternate task states.

Only `TaskService::apply` changes task/attempt lifecycle fields. `tasks.attempt` selects the current authoritative `task_attempts` row; its `vm_id`, `epoch` and `start_confirmed` task columns are transactionally maintained caches, never independent authority. Attempts own immutable VM/epoch/policy binding and start evidence. `continuation_reason=interrupted|vm_lost|suspended` is non-null only when the prior attempt ended and that nonterminal task is eligible for another attempt; `status=waiting` plus `hold_reason` blocks it, while `status=queued` and valid admission permit dispatch. Set the reason when settling a continuable attempt; clear it atomically when the next attempt is admitted or the task terminalizes. No `continuation_pending` boolean is stored. Workspace Start authorization comes only from current-owner, lifecycle-generation-bound `start_intent`; a task hold is a reason to display, not another workspace authorization flag.

`task_dependencies` is the only dependency authority; a compatibility `depends_on` list is derived, never another writable array. Composite FKs enforce one org and workspace for both endpoints, with the matching unique task key and ordinary RLS. Reject self edges, cycles and missing endpoints; serialize edge creation/validation with admission, and freeze a task's incoming edges once its first attempt is admitted. Prerequisite deletion is restricted while referenced, so deletion cannot silently make work runnable. Retention removes dependents/edges in a safe order. Backfill valid legacy arrays and quarantine/report invalid edges before cutover; update every readiness/cascade/cancellation/import consumer, then retire the array. Optional jobs and jobless DAGs use the same rules; stable logical task IDs retain all edges across attempts.

| `hold_reason` | Set by | Clears by |
|---|---|---|
| `owner_stop`, `idle_stop`, `transfer`, `start_required` | Corresponding lifecycle/admission event | Explicit current-owner Resume after valid attachment admission, or final cancellation |
| `response_timeout` | Permission wait deadline settles the attempt `suspended` | Explicit owner Continue/Resume, or final cancellation |
| `awaiting_answer` | Native question deferred and session harvested | Current owner answers on attached ready compute, or explicitly cancels the turn |
| `vm_lost` | Incarnation loss with eligible retry | Explicit owner Resume, or final cancellation |
| `delivery_unknown` | Input handoff cannot be reconciled | Owner-authorized redelivery with uncertainty warning, or cancellation (§8.1) |

A live native callback remains on its running attempt; a confirmed deferred question holds the logical task with `status=waiting, hold_reason=awaiting_answer`. A capacity/dependency wait remains `queued`; `queued_for_capacity|queued_for_slot|dependencies` are derived UI wait reasons, not independent persisted state. Valid drain/rollout continuation authorization may queue the next attempt; it cannot clear a later owner/transfer/cancellation hold. MVP dependency-failure behavior is fixed `fail_dependents`: a terminal failed/cancelled prerequisite causes not-yet-running downstream tasks to fail with `dependency_failed`, recursively, for job and jobless DAGs. Persist that value in resolved `spec`/job configuration; reject other policy values in MVP. Held prerequisites never trigger the cascade. This uses the retained cascade boundary rather than introducing configurable indefinite dependency holds.

The normalized edge shape is:

```text
task_dependencies(org_id, workspace_id, task_id, depends_on_task_id,
  PRIMARY KEY(task_id, depends_on_task_id), CHECK(task_id <> depends_on_task_id),
  FOREIGN KEY(org_id, workspace_id, task_id) REFERENCES tasks(org_id, workspace_id, id),
  FOREIGN KEY(org_id, workspace_id, depends_on_task_id) REFERENCES tasks(org_id, workspace_id, id))
-- UNIQUE tasks(org_id, workspace_id, id); reverse index (depends_on_task_id, task_id)
```

### 2.3 Permission policy is explicit

The typed `AgentPermissionPolicyV1` in §5.1 is stored in session/requested task configuration and the immutable attempt snapshot; there is no implicit permissive fallback for an absent policy. The server resolves the requested task/session/workflow configuration and freezes the effective permission/sandbox policy before each dispatch in the attempt’s immutable `policy_snapshot_ref`. A continuation may resolve newly authorized configuration, but never overwrites an earlier attempt’s policy evidence. Both input origins can select supported interactive or noninteractive behavior; neither gets automatic `bypassPermissions` because it came from a template. Provider support is checked before launch. A request requiring interactive Codex decisions is rejected as unsupported in the MVP rather than silently downgraded or routed through MCP.

Claude native clarification and tool approval are different semantic requests even when transported by the same control message. The adapter must prove that the chosen policy leaves `AskUserQuestion` available when native questions are enabled; blindly auto-allowing it can skip the user-input path. Permission decisions are owner actions attached to one native request, not changes inferred from a workflow's origin.

### 2.4 MVP scope after the 2026-09-18 decision

The durable, source-versioned review/explainer returned-results workflow is deferred beyond MVP. MVP has no dedicated review-request child lifecycle, designated result producer, result submission/reply API, result-specific database state, publication barrier or automatic completion-triggered stop. An ordinary owner-authorized agent may review or explain code and keep its output in the shared conversation, normal task output and existing artifacts. Ordinary workspace forks remain available. Jobs, dependencies and continuations keep their ordinary contracts; no task receives a special result exception. See design §5.9 for the scope decision.

## 3. Three contract categories

| Category | Meaning | Examples |
|---|---|---|
| C1 | Command or lifecycle fact outside ordered provider output | Ensure runtime, invoke/cancel, native reply delivery, process exit. |
| C2 | Ordered nonterminal observation for one execution | Message/tool progress, native request opened/replied/invalidated, usage. |
| C3 | Exactly one terminal outcome for one execution attempt | Succeeded, failed, cancelled, interrupted, timed out, suspended. |

A C2 message or provider control request never finishes an attempt. Provider result/terminal-looking output is evidence, settled with the transport and process result. TaskService decides whether an attempt outcome ends the logical task or permits another attempt of the same task. An invocation timeout cannot invent failure or launch a new identity merely to disambiguate delivery.

## 4. APIs Agents consumes from Workspaces

All calls use the Workspace Runtime facade with service authentication, owner/intent authorization and the expected incarnation fence. Agents never reaches host tunnels, placement or the dispatcher directly. These are private execution APIs; durable reader-visible task state does not authorize runtime inspect, replay or cancel calls.

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

`workspace.runtime.ensure(request)` dispatches work under a recorded explicit current-owner Start/Resume intent or reconciles that same authorized operation; it is not used for reads, Save/upload, ordinary input admission or provider-request replies. Start/Resume admission is recorded and resolved by the server, not inferred from the `reason` string or a caller-supplied claim of consent: validate workspace, current owner, lifecycle generation, permitted work and the applicable recovery policy. `workspace.runtime.get` is an internal read-only operation and may report that no live handle exists; public clients get permitted persisted workspace state, not a reader handle to the dispatcher. Preview ensure requires the stored preview kind and current owner. All reasons honor stop/transfer admission; only explicit current-owner Resume records a fresh valid `start_intent`. Reconciliation cannot broaden the original intent or release a hold. Ordinary fork creation does not itself create Start/Resume authority.

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

When persisting invocation intent, the API mints scalar `exec_id=task.<task_id>.<attempt>.<vm_id>` using lowercase hyphenated UUIDs and a positive decimal attempt without leading zeros. The API validates its stored triple on every write; the dispatcher/adapter/CLI treat the filesystem-safe token as opaque and never derive or trust it as authorization. Task-less workspace module operations use the distinct `operation.<operation_id>.1.<vm_id>` form minted by the facade with a UUIDv7 operation ID reused on retry; they never fabricate task rows. Only the `task` variant is admitted by agent-turn/provider-request routes. Golden fixtures cover both exact encodings and reject cross-kind substitution.

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
                        task_id, attempt, exec_id, request_id, provider_request_id,
                        response_digest, validated_response, idempotency_key=request_id })
  -> delivered | already_delivered | request_gone | stale_execution
```

This is a narrow declared `agent.turn` input channel, not arbitrary stdin, generic shell, chat steering or an additional module invocation. The task/attempt fields must match `exec_id`; expected incarnation and epoch must match the live execution. `request_id` names the durable row and `response_digest` binds its first accepted, immutable response; there is no re-answer version counter. Workspaces validates the private per-execution control capability and routes this declared input to the already-running module; it cannot launch or address another execution. Agents authenticates the current owner, validates the response against the pending native request and records the reply intent before calling it. The adapter writes only the exact provider response envelope for that request. Identical UI retries use the same request/response digest and return the stored delivery state; conflicting answers are refused once acceptance commits, before delivery starts. A delivery acknowledgement means the response was handed to the live provider transport, not that the tool succeeded. An uncertain write is reconciled within that same live process; process loss never causes reply replay into a new attempt.

### 4.5 End a Claude background wait

```text
modules.background.end({ workspace_id, expected_vm_id, expected_epoch,
                         task_id, attempt, exec_id, request_id, idempotency_key=request_id })
  -> delivered | request_gone | stale_execution | execution_finished
```

This is the owner's End for a live Claude attempt that waits on background work (§10.1), a second declared `agent.turn` input channel beside `input.respond`. `POST /api/v1/sessions/:id/background/end` is owner-only and names the task and attempt whose wait the owner saw (`{taskId, attempt}`); TaskService refuses it with 409 unless that attempt is still the session's running attempt, resolves its live incarnation, mints `request_id` and calls this method. The dispatcher accepts it only for that running execution whose module declares the catalog operation `background.end`, and writes the control line `{"kind":"background.end","request_id":…}` to the module's stdin; nothing is reserved or journaled, and a repeated End is harmless. Any disposition other than `delivered` returns 409 to the owner. The logical task is not cancelled: when Claude then exits cleanly the attempt succeeds and queued input runs next; when Claude has to be stopped the attempt settles interrupted and the task is held for Continue, like Stop.

### 4.6 Native Claude steering

```text
modules.input.steer({ workspace_id, expected_vm_id, expected_epoch,
                     task_id, attempt, exec_id, input_id, reservation_id,
                     revision, delivery_generation, content,
                     idempotency_key="<input_id>:<delivery_generation>" })
  -> shipped | not_steerable | pending
```

This private lifecycle method delivers ordinary text to a running `agent.turn` that declares `input.steer`; it never starts or interrupts a provider. TaskService commits the queued input and an immutable revision/generation binding under the session lock before one RPC with a 45-second deadline. Eligibility requires the current owner, a started Claude attempt with initial input accepted and the frozen steering capability, and no attachments, older queued/reserved/uncertain input, hold, cancellation, pause, native request or background wait. Every new Claude attempt enables steering, independently of organization settings. Other sends retain the ordinary queue.

The dispatcher binds the idempotency key to the input identity and content digest before forwarding. Only the worker's journaled `input.shipped`, after a complete write and flush to Claude's stdin, confirms delivery. Applying that event atomically marks the input delivered, appends its exact user row and starts a new assistant segment within the same task and attempt. Native text blocks and tool calls already in progress retain their original assistant segment through completion; new units use the new segment. The runner does not request replayed user messages and there is no separate consumption event. The inbox shows Waiting until this projection, then removes the entry.

Same-key retries never write twice. Definite pre-write rejection or no definite answer by the deadline clears the binding and leaves the input queued for the next turn. An ambiguous write may therefore cause a rare duplicate, which the owner accepts. The API makes no background steering retries and adds no attempt-end recovery or Redeliver/Cancel choices. After flush the input remains delivered even if Stop or VM loss prevents Claude from reading it; the user may resend. Stdin closure and attempt settlement use the existing native multiple-turn rules. Codex retains its existing behavior.

## 5. The `agent.turn` module contract

```yaml
module_id: agent.turn
module_version: 1.0.0
lifecycle: bounded_run
limits:
  wall_clock_ms: 3600000
  cpu_cores: 2
  memory_mib: 2048
  max_output_bytes: 8388608
  max_log_bytes: 8388608
  concurrency: 2
operations:
  run:
    input_schema: g5.agent.turn.input/v1
    event_schema: g5.agent.turn.event/v1
    output_schema: g5.agent.turn.outcome/v1
  input.respond:
    input_schema: g5.agent.native-response/v1
  background.end: # since 1.0.2: the owner's End of a Claude background wait (§4.5)
    input_schema: g5.agent.background-end/v1
  # 1.0.2 also adds limits.wall_clock_scope (below). Under the rollout rule
  # (design v9 §6.10) the control plane deploys first and sends 1.0.1, without
  # wall_clock_scope, to a VM whose recorded module catalog is the previous
  # one; that VM keeps the attempt-wide default until a drain replaces it.
requested_capabilities:
  - workspace.native_files
  - workspace.native_git
  - workspace.native_shell
  - workspace.native_tests
  - network.agent_control_plane # trusted supervisor service operations only; never inherited by provider/shell
  - network.agent_llm_proxy # (post-MVP, P4.12: the MVP delivers the owner's provider key with the dispatch the way TaskAssignment.secrets travel today; the Agents owner confirms the field in P4.11)
observability:
  logs: bounded_redacted
  events: journaled
```

These are initial catalog caps, not permission to exceed the VM allocation: resolved limits take the minimum of catalog, VM and explicitly configured org/workspace caps. Every bounded-run module declares limits; missing limits fail catalog validation. Workspaces supplies `bounded_execution_slots` (default 16, positive and capped by org policy; the catalog admits that many `agent.turn` executions, `g5.agents.admission.vm-slots`) per incarnation; TaskService reserves a slot for every `agent` **and** `command` invoke intent until definitive refusal or terminal settlement, including live native waits. Dispatcher enforces the same ceiling. Preview alone is exempt. Effective job fan-out is bounded by `jobs.max_concurrency`, available workspace slots and module concurrency; excess eligible work shows `queued_for_slot`, separate from VM placement's `queued_for_capacity`. These settings do not grant a new host scheduler.

**Post-MVP (P4.12: no inline credentials).** The MVP delivers the owner's provider key with the dispatch, the way `TaskAssignment.secrets` travel today; the Agents owner confirms the field in P4.11. The manifest requests capabilities and Workspaces grants their intersection with runtime policy. Inputs cannot contain caller-selected argv, environment, host paths, signals, credentials or host endpoints.

```typescript
type AgentTurnInputV1 = {
  schema_version: "g5.agent.turn.input/v1";
  task: {
    task_id: string;
    attempt: number;
    session_id: string; // existing shared chat_sessions identity
    input_origin: "human" | "workflow" | "continuation";
    initial_input: AgentSessionInputReservationV1;
  };
  provider: {
    provider_id: "claude_code" | "codex";
    model_id: string;
    policy_ref: string; // immutable AgentPermissionPolicyV1 snapshot, independent of origin
    options: Record<string, unknown>;
  };
  prompt: { text: string; sha256: string }; // exact bounded UTF-8 rendering of the frozen reservation
  workspace_target: { repo_id?: string; relative_path?: string };
  owner_proxy_binding_ref: string; // (post-MVP, P4.12: the MVP delivers the owner's provider key with the dispatch the way TaskAssignment.secrets travel today; the Agents owner confirms the field in P4.11)
  session_binding: {
    session_files_state: "present" | "absent" | "persist_failed";
    provider_session_id?: string;
    expected_manifest_generation?: number;
    restore_ref?: string;
  };
  continuation?: {
    prior_exec_id: string;
    reason: "interrupted" | "vm_lost" | "suspended";
  };
  native_question_reply?: {
    source_attempt: number;
    provider_request_id: string; // stable native tool-use ID
    provider_session_id: string;
    request_id: string;
    response_digest: string;
    validated_response: Record<string, unknown>; // validated positional answer
  };
  limits: { wall_clock_ms: number; wall_clock_scope?: "attempt" | "input_delivery"; // absent = "attempt"
            max_turns?: number; max_output_bytes: number };
};
```

`limits.wall_clock_scope` says what `wall_clock_ms` bounds, for Claude and Codex alike. `attempt`, which an absent scope also means, marks a limit the task explicitly requested and bounds the whole attempt, including provider execution. `input_delivery` marks the platform's default limit: it bounds setup and input delivery and lifts once the provider accepts the input. The task spec's `limits.wallClockMs` (`spec.limits`, §2.2.1) is optional; when it is absent the dispatch input carries the default 1 h with scope `input_delivery`, and the dispatcher sets no settlement deadline and no `G5D_ATTEMPT_DEADLINE_UNIX_MS`, so the catalog `wall_clock_ms` cap above does not end the attempt either, while the dispatcher's setup steps keep the default bound. Harvest after a normal exit has its own 5-minute bound. Without a requested limit, a hung or runaway turn ends only through Stop or Cancel, the output bound or a VM stop, and a Claude background wait also ends through the owner's End (§4.5).

Every attempt consumes exactly one frozen reservation, using `AgentSessionInputReservationV1` below. The server renders the bounded prompt inline from that reservation and any authorized notices/history, hashes its exact UTF-8 bytes, and the module verifies the hash before spawn. Maximum prompt bytes are 512 KiB, within the declared invoke transport cap; larger history is deterministically truncated with a visible marker, never silently changed after hashing. No new prompt-artifact table or fetch route is needed. A continuation uses its system-authored continuation reservation, not a replay of delivered input. A deferred-question answer freezes `native_question_reply` in that reservation: source attempt, stable native tool-use ID (`provider_request_id`), native session ID, accepted request ID, response digest and positional answer body. This reservation has no replacement user prompt; the adapter restores the native session, matches the pending tool, and writes the answer through its fresh callback. Missing or mismatched pending state fails restoration. An old control-request ID is never reused.

### 5.1 Policy and input schemas

```typescript
type AgentPermissionPolicyV1 = {
  schema_version: "g5.agent.permission-policy/v1";
  provider: "claude_code" | "codex";
  questions: "enabled" | "disabled";
  tool_approval: "prompt" | "auto_allow" | "auto_deny";
  sandbox: "none" | "read-only" | "workspace-write" | "danger-full-access";
  allowed_tools: string[]; // explicit selection; unknown names rejected
  limits: { wall_clock_ms: number; max_turns?: number; max_output_bytes: number; native_response_timeout_ms?: number };
};
type AgentSessionInputReservationV1 = {
  reservation_id: string; input_id: string; revision: number; // session_inputs identity/revision
  delivery_generation: number;
  input_origin: "human" | "workflow" | "continuation";
  system_notice_ids: string[];
};
```

| Admitted policy | CLI mapping and gate |
|---|---|
| Claude, questions enabled/disabled, `tool_approval=prompt`, `sandbox=none` | `--permission-mode default --permission-prompts host --permission-prompt-tool stdio`; enabled questions preserve `AskUserQuestion`, disabled questions explicitly disallow that tool. Native request behavior must pass exact-pin recordings before enabling this row. |
| Claude, questions disabled, `auto_deny`, `sandbox=none` | `--permission-mode dontAsk --permission-prompts none`, `AskUserQuestion` disabled. Denial means actions needing approval are denied, not that all allowed tools are disabled. |
| Claude, questions enabled/disabled, `auto_allow`, `sandbox=none` | `--permission-mode bypassPermissions`; requires explicitly stored elevated choice. Enabled questions use `--permission-prompts host --permission-prompt-tool stdio` and preserve `AskUserQuestion`; disabled questions retain `--permission-prompts none` and explicitly disallow that tool. Origin never selects bypass. The exact-pin Bypass recording in §10.4 verifies this combination. |
| Codex MVP, questions disabled, `auto_deny`, sandbox `read-only\|workspace-write\|danger-full-access` | `exec --json -c approval_policy="never" -c sandbox_mode="<explicit value>"`; requested escalation is denied while operations inside the chosen sandbox may run. Danger-full-access is an explicit elevated sandbox choice, not a side effect of noninteractive mode. |

The optional policy limit `native_response_timeout_ms` is positive and at most 900000 and applies only to permission requests. Their effective wait is the minimum of this value (default 900000), the resolved workspace response setting and remaining attempt wall time. Questions have no automatic response deadline; an explicit attempt limit may end execution but preserves a confirmed deferred unanswered question.

All other combinations fail `UNSUPPORTED_PERMISSION_POLICY` before spawn, including enabled questions with Claude deny mode and any Codex MVP questions/prompt/auto-allow policy. `allowed_tools` can narrow but never broaden the resolved policy; a Claude tool-availability restriction must use the pinned availability controls rather than confusing permission auto-allow flags with a tool allowlist; unsupported provider/tool combinations fail rather than ignoring a restriction. `policy_snapshot_ref` resolves to this exact versioned document and its digest. Raw legacy `permission_mode` strings are translated only through the explicit migration/validation map; the target invocation carries no ambiguous two-valued permission mode. Exact-pin transport support remains a P4.11 verification gate, not a claim about the currently shipped runner.

## 6. APIs Agents provides to Workspaces

### 6.1 Owner-bound provider access

`agents.runtime_bindings.ensure` resolves the current owner/workspace/incarnation/provider binding through authoritative Workspaces state; `revoke(binding_id, expected_vm_id)` fences it. A binding never selects credentials from task origin, chat creator, preview launcher or session id. Current-owner credential rotation may update a running incarnation; transfer releases it before the new owner can receive a new binding. The module capability grant remains separate from owner provider-account selection.

**Post-MVP (P4.12: the dispatcher-owned local model broker and the `g5-llm` host listener).** MVP behaviour: `agent.turn` receives the owner's provider key with the dispatch, starts today's in-guest `llm_proxy` with it and builds the provider environment as the P3.2 runner does. `agent.turn` configures only the dispatcher-owned local model broker: `ANTHROPIC_BASE_URL=http://127.0.0.1:<broker_port>/s/<session_id>` or `OPENAI_BASE_URL=http://127.0.0.1:<broker_port>/s/<session_id>/v1`, with provider-required placeholders. The dispatcher forwards through its private `g5-llm` host listener credential; no host proxy secret reaches a provider process. The local broker exposes model/internal-Git operations only, never a generic `/api` forwarder or platform-write capability. Execution accounting comes from trusted supervisor registration, not a self-asserted session path. Restoring an old conversation does not restore its former owner's account authority. [Worker Host API v9](worker-host-api-v9.md) owns the proxy boundary.

### 6.2 Shared provider-session files

`agents.session_bundles.get` and `agents.session_harvests.persist` use the existing `chat_session_files` storage for every agent task's conversation. Agents owns provider paths, format, generation, hashes and compatibility; Workspace and Host carry opaque bundles. Harvests from stale incarnations or older generations cannot overwrite a newer manifest. A failed/partial/over-cap harvest persists `session_files_state=persist_failed` on the attempt and session generation and emits `notice(code=session_files_persist_failed, level=warning, retryable=false)` for deterministic cap/format failures (transient upload failures remain retryable). Never restore an older stored generation as though it includes newer delivered turns: if the required generation is unrestorable or absent on the next incarnation, start a fresh provider session. Exact raw provider bundles remain supervisor-only under the retained worker-session authorization replacement and are never served through reader routes; they are not redacted because byte-changing a resume artifact is unsafe. For an ordinary conversation turn, native restore failure starts a fresh provider session using delivered conversation history with clear interrupted-attempt markers. A deferred-question answer requires its saved native pending call: missing or mismatched state fails that restoration, without converting the answer into a new user prompt.

### 6.3 Stop and transfer

```text
agents.workspace_stops.prepare(stop)
agents.workspace_stops.get(stop_id)
agents.workspace_stops.abort(stop_id, expected_vm_id)
agents.workspace_stops.commit(stop, workspace_runtime_event_id)
```

`prepare` closes Agent dispatch, settles reservations without guessing acceptance, interrupts/reaps live executions, invalidates their process-bound requests, preserves confirmed deferred questions and freezes the final journal/session-file manifest. Final harvest persists that frozen manifest and ordered event prefix; `commit` records Agent stop completion in the same control-plane transaction as workspace release only after confirmed host exit. Deterministic over-cap/invalid bundles may be recorded unrestorable with durable notice and delivered-history fallback. Required journal/outcome or stop-receipt persistence failure aborts user/transfer/idle shutdown; deadline-bound operational teardown alone may release with explicit recorded loss. Stale-incarnation uploads are always fenced, never treated as successful harvest. On an abort reopen eligible admission without reviving dead requests or discarded session state. Workspaces pairs this with its teardown flush receipt before release. Existing queued inputs and logical task identities remain; allowed continuation increments their attempts only after lifecycle admission opens.

Transfer holds admission through host exit/release and atomic owner/audit completion. Failed flush retains the old owner and reopens admission, but does not revive the native requests of already terminated processes. Successful transfer revokes the old `start_intent`; only the new owner's explicit Resume permits retained agent/command work to continue. No reader action can wake that workspace or release the hold. Confirmed deferred questions remain visible and answerable by the current owner after ready compute is attached; their saved native tool resumes through §8.2. Ordinary process-bound requests remain read-only history.

### 6.4 Activity projection

```typescript
type AgentWorkspaceActivityV1 = {
  workspace_id: string;
  agent_admission: "open" | "closed";
  resume_required: boolean; // derived from current start_intent/admission, never a stored owner flag
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

### 6.5 Trusted session-file operations and deferred provider writes

P4.5 owns incarnation-authenticated versions of the existing `GET /api/internal/chat-sessions/:id/session-files/manifest`, `GET .../session-files/content`, and `PUT .../session-files` routes. Preserve provider-session/path/hash/body codec semantics, adding expected `vm_id`, `epoch`, task/attempt and manifest generation; stale writes return `409 stale_incarnation`, accepted writes return `session_manifest_generation`. Authorization derives from the trusted supervisor's registered current execution and session, not guest-supplied environment IDs. P4.11 records exact request/response fixtures and tests its client through the real internal-router boundary when P4.5 lands; it cannot claim production integration against an independently invented proxy mock. Inline `prompt` removes the former undefined `prompt_ref` server dependency. Retained session routes today use worker-session auth and lack those fences; they are reuse evidence, not proof of v9 support.

**Post-MVP (P4.12: no platform-write transport for provider and repository-shell processes, the rule that no environment variable confers authority, fail-closed in-VM `g5d` mutations and the model/internal-Git brokers).** MVP behaviour: the incarnation token is an environment variable and existing `g5d` and MCP platform tools keep authenticating with it. Provider and repository-shell processes have no MVP platform-write transport. `G5D_EXEC_ID`/session IDs are correlation only; a same-UID socket, environment variable or guest API proxy does not confer authority. In-VM `g5d` platform mutations fail closed. Generic provider-initiated PR/task writes are a post-MVP capability requiring its own named transport and isolation proof; no undefined bridge gates the retained MVP. Model/internal-Git brokers do not expose platform reads or writes. Unrelated local MCP tools remain available without platform credentials; native Claude questions use only the live provider response protocol, and Codex MVP stays noninteractive.

**Post-MVP (P4.12: keeping this authority out of provider and repository environments).** Session restore/harvest, journal acknowledgement and other declared lifecycle operations are performed by the trusted supervisor over its existing authenticated service client, scoped to the current incarnation/execution/session. They are not provider-callable tools. The runtime keeps this authority out of provider and repository environments, FDs and accessible state. User/device-flow CLI commands outside the VM retain their separate user authorization; that is not an in-VM provider transport.

### 6.6 Trusted bootstrap graph delegation

**Post-MVP (P4.12: the new scoped authorization).** `workspace.bootstrap` is a platform-shipped deterministic `command` module. Its trusted supervisor may submit the declared initial graph for an owner-authorized job; it is not a provider shell or generic agent tool. Reuse the typed task-graph service behind the retained `POST /api/internal/jobs/:job_id/tasks` route with the new scoped authorization below, rather than exposing the legacy worker-token contract. Ordinary `agent.turn` providers receive no such delegation.

**Post-MVP (P4.12).** MVP behaviour: graph submission is authorized API-side with the incarnation token and the API's existing check that the caller owns the running bootstrap task, re-keyed from worker to incarnation. P4.2b lands the capability plumbing as planned and P4.8 removes it. At `modules.invoke`, the API registers a random 256-bit capability bound to the admitted `workspace.bootstrap` module version/hash, command task/attempt, org/workspace/job, current owner, lifecycle generation, VM/epoch, exact HTTP method/route and resolved graph limits. The trusted dispatcher delivers it only to its platform module supervisor's private memory/control channel. The module uses a private supervisor HTTPS client through the dispatcher-authenticated host platform broker, which forwards only `POST /api/internal/jobs/:job_id/tasks` with this scoped capability. The broker does not inject a generic workspace token or permit arbitrary API destinations/methods; direct guest access to the control plane remains denied by host egress policy. This private runtime client/transport is inaccessible to provider/repository endpoints and is never inherited by child scripts. There is no repository-accessible proxy/socket endpoint, provider callback or credential-bearing environment. The API resolves all identity from the capability and persisted job/attempt, not body IDs or `G5D_EXEC_ID`. An incarnation token alone does not grant graph creation. Revalidate current owner, admission, attempt and module scope before effects; stop, cancel, transfer or incarnation loss revokes the delegation.

The narrowed body reuses the existing `SubmitTaskGraphRequest.tasks: Vec<TaskNodePayload>` analysis-node payload under the version tag `g5.bootstrap.task-graph/v1`, called `BootstrapTaskGraphV1` here. Retain component identity/name, optional objective and mapped ontology node, dependencies by component ID, source paths, node level/language, parent/child component IDs, declared completion constraints and analysis method. Freeze exact wire field names against the retained DTO fixtures; this is not a free-form tool or template-registration interface. Only nonblocking initial job-graph submission is admitted; omit/reject additive task-owned graph and blocking `wait_mode`/`wait_for` operations in this delegation.

The job's saved profile/template revision and explicit permission/model policy select execution and render the submitted analysis data into each frozen initial input. The service creates one shared session and `kind=agent` task per node, and normalized same-workspace dependency edges. Validate mapped ontology IDs and normalized source paths against the workspace's member repositories. Reject duplicate/missing component IDs, self/cyclic edges, undeclared fields and any caller-selected owner/workspace/account/session/task kind, template or policy override. Every new child also depends on the current bootstrap logical task, so graph capture cannot run children before its final success; bootstrap failure follows `fail_dependents`. Draft caps are 1 MiB canonical JSON, 1,024 nodes and 4,096 submitted edges, further reduced by the invocation/job policy; reject the complete graph before writes when exceeded. An empty graph is an explicit no-op.

Serialize the submission with the job/bootstrap task and lifecycle admission. MVP permits one initial graph per job: atomically store its RFC 8785 canonical-JSON SHA-256 digest and component-ID-to-task-ID map in server-owned `jobs.spec.bootstrap_graph_receipt`, alongside all created sessions/tasks/edges. Client job-spec updates cannot set, replace or remove this receipt. Identical retries, including a continued bootstrap attempt, return that same map; a changed graph returns `409 BOOTSTRAP_GRAPH_CONFLICT` without partial writes. A retry still requires a currently authorized bootstrap attempt. The receipt records the initial command task and accepted attempt; it does not complete that task, expose credentials or become a second task-state writer. A task-less setup module and a command without an authorized job have no graph-submission capability. General provider-initiated graph mutation remains deferred.

**Post-MVP (P4.12: the capability clauses and the copied-environment and provider-shell denial tests).** The implementation reuses the deterministic graph-construction code and InternalApiClient route shape, not its legacy broad worker authorization. Repository setup/build commands run as unprivileged descendants without this capability or privileged FDs; the supervisor parses their bounded output as data and never forwards their arbitrary HTTP, method or argv requests. P4.2b owns the trusted built-in module boundary; P4.6/P4.8 own the scoped service/capability and atomic graph admission. Test legitimate graph creation and idempotent retry, child release only after bootstrap success, changed-graph/cycle/cap refusals, stop/transfer races, and copied environment/guessed task/sibling/provider-shell attempts producing no child tasks.

## 7. Worker & Host dependency

Agents has no direct host API for placement, wake, lease, stop, networking or shell. Workspaces supplies a pinned module/runtime image, the actual CLI binaries, declared resources/capabilities and journal transport. It does not add a Claude SDK/Node adapter. Provider schemas and stdout/stdin control parsing remain inside `agent.turn`.

**Post-MVP (P4.12: the owner-bound credential references and usage tags seen by the host proxy).** The host sees owner-bound credential references and usage tags needed by its proxy, not a separate conversation or native question state machine. Resource and process facts reach the adapter/reconciler; they do not themselves establish logical task success. No raw user-supplied provider response is forwarded through a host shell or HTTP port relay.

## 8. Inbox, native requests and continuation

### 8.1 Shared inbox

```text
session_inputs(id, org_id, session_id, input_index bigint,
               content, artifacts jsonb, revision int, input_origin human|workflow|continuation,
               template_ref NULL, template_revision NULL,
               delivery queued|reserved|delivered|delivery_unknown|withdrawn,
               steered boolean, steering_retry_at NULL,
               reservation_id NULL, delivery_generation int, task_id NULL, attempt NULL,
               created_at, delivered_at NULL, UNIQUE (session_id, input_index))

chat_messages(id, org_id, session_id, message_index, role user|assistant|system|tool,
              input_id NULL UNIQUE REFERENCES session_inputs(id), content, artifacts jsonb,
              status pending|streaming|complete|error|stopped,
              task_id NULL, attempt NULL, input_tokens, output_tokens, created_at)
```

`session_inputs` owns the mutable FIFO queue, exact content/attachment references, revision, provenance and reservation state; `chat_messages` owns delivered input history and streaming assistant/tool output. Queue order is `input_index`, transcript order is `message_index`, both allocated under the session lock. A journaled `input.accepted` atomically inserts the delivered user/system transcript row with `id=input_id`, sets its unique `input_id` link, freezes that accepted content, marks the matching input generation delivered and allocates its transcript index. Assistant events for the attempt apply after acceptance. Replayed acceptance returns the same row; a late prior-generation acceptance may add factual history but cannot settle a newer reservation. Unknown/withdrawn inputs do not enter provider fallback until positive acceptance evidence exists. The queue/timeline UI combines these projections without copying queued content into the transcript. Add `chat_attachments.input_id` for queued attachments and set their existing `message_id` link in that acceptance transaction; never create a second attachment body. Same-org/session constraints bind input, message, task and attachment links. Existing delivered history remains in place during migration; only genuinely pending input is backfilled into the queue. Chat fork/carry copies delivered transcript, not another owner's mutable queue.

The linked transcript row has `CHECK(input_id IS NULL OR id=input_id)` and same-org/session ownership; `input_id` is unique. The acceptance transaction compares exact frozen revision/content/digest before reuse. Input rows and accepted message content become immutable after acceptance; only delivery audit may gain factual late evidence. Forked historical messages clear `input_id` rather than linking a different session's queue. Pending queue input and delivered transcript are separate API lists merged for presentation, so repeated acceptance cannot duplicate a visible human message.

```text
queued -> reserved -> delivered
   |         |
   |         +-> delivery_unknown -> delivered | reserved (owner redelivery) | withdrawn
   |         +-> queued (proven pre-handoff failure)
   +-> withdrawn

steering: queued -> delivered (journaled complete stdin write/flush)
          queued remains queued on rejection or no definite RPC answer
```

Queued human or workflow inputs are editable/withdrawable by the owner while owner mutation admission is open, except while an immutable steering binding is awaiting its flush or RPC disposition. A stopped/unattached workspace is browse-only: new input or edits require explicit owner Start/Resume; the server may reserve the launch input with that explicit operation, but an ordinary inbox route cannot create wake authority. Reservation freezes the version for a particular attempt; delivered input is immutable transcript. Ambiguous acceptance is reconciled using the existing provider session before release/redelivery. `input.accepted` names session-input id, revision, reservation and delivery generation; applying it materializes the linked transcript atomically. A native response is separate from queued ordinary input and cannot be substituted for it.

Initial-input reconciliation uses positive evidence: journaled acceptance means `delivered`; proven pre-handoff failure releases the reservation to `queued`; missing evidence alone stays `delivery_unknown`, excludes that row from fallback history, and holds the task `waiting` with `hold_reason=delivery_unknown`. The owner sees the uncertain input and can explicitly authorize redelivery (possible duplicate effects warning), re-reserving the same immutable input id/revision for a new attempt with `delivery_generation+1`, or Cancel, recording the input `withdrawn` with its uncertainty history retained and the task `cancelled`. Ordinary Continue cannot silently choose redelivery. New messages remain queued behind the held task. A late old-generation acceptance may fill historical delivery evidence but never settle the new generation or send another prompt.

An ordinary input arriving while a session task is running or held stays in that session inbox behind the nonterminal task, except for eligible native Claude steering (§4.6). Pending cancellation or interruption makes steering ineligible even while the task still reports running. Absence of a live provider does not authorize another task to bypass a continuation hold. Explicit current-owner Continue/Resume clears ordinary continuation holds; an unanswered deferred question instead requires its answer or cancellation; after terminal settlement, the next ordinary input creates a new agent task in the same session.

Both providers consume one initial reserved input per attempt, including a system-authored continuation or native deferred-question answer. Every new Claude attempt freezes the `native_mid_turn_steering` protocol capability and may consume further eligible individual inputs under §4.6. Queued inputs are never concatenated. Claude keeps stdin open for native control exchanges and owned background work (§10.1), then closes it between turns when no writes are pending. Queued native user frames can still produce subsequent turns after stdin closes. Messages sent during a background wait or to an older process without the steering capability still queue. Codex receives its one prompt then EOF immediately, as `exec` requires.

### 8.2 Native request projection and reply

```text
provider_requests(id, org_id, task_id, attempt, provider_request_id,
                  kind question|permission, tool_name, payload jsonb, display jsonb,
                  status pending|reply_pending|responded|invalidated,
                  response jsonb NULL, response_author_id NULL, response_deadline_at NULL, resume_session_id NULL,
                  created_at, ended_at NULL, end_reason NULL,
                  UNIQUE (task_id, attempt, provider_request_id),
                  FOREIGN KEY (org_id, task_id, attempt) REFERENCES task_attempts(org_id, task_id, attempt))
```

This row is the native request's timeline projection and durable reply outbox, not an independent questionnaire engine. `(task_id, attempt)` references the immutable attempt; derive VM/epoch/exec identity, session and provider from that binding and its frozen policy, revalidating current authority on every live action. Same-org FKs/RLS prevent cross-tenant bindings. `payload` is bounded private native JSON; `display` is its bounded redacted projection safe for the authorized timeline. `response` is one bounded server-owned envelope containing the accepted body, SHA-256 digest, acceptance time and later factual delivery evidence. Draft per-body maximum is 64 KiB UTF-8 JSON; reject oversized native requests/answers explicitly, never truncate semantic provider input. No payload-reference table is needed. First response acceptance atomically writes that envelope/author and `reply_pending` before any native write. Identical retries return its current state; a different answer returns `409 response_conflict`. `reply_pending` survives a crash or RPC timeout between DB commit and delivery. Only TaskService adds delivery evidence; accepted body/digest/author never change. Render text/options as data and validate against the recorded native schema.

Original native question keys and option labels remain in the live adapter and private native session files. Journal redaction also applies to stored request payloads; the UI and reply API correlate answers by question/option position (§10.2). Public timeline reads overlay only the authoritative request status on its opening event, never the private accepted response body. The accepted response digest is lowercase SHA-256 of canonical JSON after schema validation.

A single serialization point coordinates answer acceptance, native delivery, cancellation and execution loss. Ordinary permission replies require their exact live execution and deadline. Unanswered native questions have no automatic deadline. For confirmed native deferral, `resume_session_id` binds the stored question to its saved Claude session and `provider_request_id` is the native tool-use ID. Workspace Stop, VM loss, transfer and elapsed time preserve it; current-owner authorization is checked on every answer. Explicit task/turn cancellation or provider withdrawal closes it. Other messages stay queued behind it, and Start alone does not bypass an unanswered question. A live callback remains process-bound; active provider background work is not interrupted merely to defer a question. The selected restoration path requires the native pending call to exist; no generated question, transcript rewrite or generic chat-message fallback is allowed.

Only `TaskService::apply` consuming journaled `provider_request.replied` with the exact request ID and response digest changes `reply_pending` to `responded`. The adapter emits it after a complete native response write with `evidence=transport_write`; this proves a write, not provider acknowledgement or tool success. For a restored answer, the event names its source attempt and carries the current input reservation acceptance. The API verifies the source against the current attempt’s immutable `native_question_reply` snapshot and settles the original question and current input in one transaction. Ordinary replies retain `delivered|already_delivered` transport results; a deferred answer returns `queued` after durable acceptance. Unknown or partial writes retain delivery uncertainty and never authorize a blind resend into another process. Invalidation preserves accepted answer and factual delivery evidence.

Question `response_deadline_at` is null. Permission requests retain the 15-minute default deadline, shortened by policy and attempt limits. Native PreToolUse deferral ends one provider turn, harvests through the existing fenced private bundle owner and settles the attempt as `suspended/native_question_deferred`; it does not succeed the logical task or release dependents. The task waits for an answer on attached ready compute. If the owner answers while the bundle is still saving, acceptance is frozen immediately and native resumption queues only after settlement. An explicit attempt limit may end execution but does not expire a confirmed deferred unanswered question.

### 8.3 Continuation and replay

Conversation continuity belongs to the same agent task and its shared session. When lifecycle/retry policy allows continuation after interruption, suspension or VM loss, TaskService atomically closes the prior attempt and retains the same logical task, preserving job/dependency edges. User Stop, transfer, native-request timeout and replacement after ungraceful VM loss hold it for explicit current-owner Continue/Resume; eligible drain/rollout recovery may queue its next attempt under the recorded continuation authorization. `waiting` distinguishes a live native request from a held task with no process. Only final logical success releases dependents. Final failure/cancellation follows the fixed MVP fail_dependents rule for jobs and jobless DAGs. It may restore provider session files or use delivered-history recovery. A confirmed deferred question follows §8.2: restore the same native tool and deliver its accepted answer through the restored callback. Ordinary dead-process requests cannot receive replies.

At admission of an eligible next attempt, TaskService atomically allocates its attempt number and exactly one continuation `session_inputs` row with `input_origin=continuation`, immutable content rendered from `{reason, stop_reason, prior_exec_id, prior_attempt, interrupted_at}`, `task_id` unchanged, `attempt` the new number, delivery initially queued then reserved as that attempt's `initial_input`; acceptance materializes its system-role transcript row. It is created at next-attempt admission, not repeatedly at reconciler ticks or before an attempt exists. A unique `(task_id, attempt, input_origin=continuation)` constraint prevents duplicates. The adapter sends a fixed continuation instruction containing this reason and prior identity, asks it to continue without repeating completed work, states that background tasks and sub-agents from the earlier attempt are no longer running, and never copies a delivered original human prompt. Ordinary continuation sends a native user frame; deferred-question continuation sends only the accepted native response after restoration, without a replacement prompt. The product timeline renders continuation provenance as system-authored and noneditable. Its own `input.accepted` names only that continuation reservation. For explicitly authorized `delivery_unknown` redelivery, the uncertain original row is the sole reservation; continuation facts are prompt metadata, not a second reserved input.

A stop/drain nudge is a different system notice: no new agent task and no separate ordinary provider frame. Store a system `chat_messages` notice with distinct queued notice-delivery metadata in `artifacts` (never an ordinary `session_inputs` row) and carry its ID in the next authorized reservation's `system_notice_ids`; the prompt preamble carries it, and the same journaled `input.accepted` marks only those declared notices delivered. Stop release/cancellation withdraws undelivered nudges with audit history. A nudge cannot bypass a hold or create wake authority.

Journal replay repairs projections; provider-session restore restores provider history; inbox dispatch selects a new authorized input; task continuation preserves interrupted work identity. Those are distinct operations. A cancellation wins against continuation creation under the same logical task lock; duplicate recovery cannot create multiple attempts. Tasks reach final failure when continuation is unavailable or exhausted, not merely because an intermediate attempt was interrupted.

## 9. Provider-neutral event and outcome schemas

The C2 union below is normalized platform data, not a list of capabilities guaranteed by both providers. The adapters admit only variants proved by their pinned transport fixtures; unused SDK-only discriminants are not silently promoted into CLI behavior. Native-request events are correlated by the envelope's `exec_id` and their provider request id.

```typescript
type UsageV1 = {
  input_tokens: number; output_tokens: number;
  cache_creation_input_tokens: number; cache_read_input_tokens: number;
  model?: string; total_cost_usd?: number; duration_ms?: number; finish_reason?: string;
}; // token/duration fields: nonnegative safe integers; absent cost is unknown, not zero
type QuotaSnapshotV1 = {
  provider: "anthropic" | "openai" | "gateway";
  captured_at: string;
  headers: Record<string, string>; // validated allowlist only, never arbitrary response headers
};
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
  | { kind: "message.delta"; message_id: string; text_delta: string; text_truncated?: boolean }
  | { kind: "message.final"; message_id: string; content_ref: string;
      content?: string; content_truncated?: boolean;
      in_reply_to_input_ids: string[]; usage?: UsageV1 }
  | { kind: "message.retracted"; provider_message_ids: string[]; reason: string }
  | { kind: "thought.delta"; thought_id: string; text_delta: string }
  | { kind: "thought.final"; thought_id: string; content_ref: string;
      content?: string; content_truncated?: boolean }
  | { kind: "thinking.progress"; estimated_tokens: number; delta?: number }
  | { kind: "input.accepted"; reservation_id: string; input_id: string;
      revision: number; delivery_generation: number; system_notice_ids: string[] }
  | { kind: "tool.started"; call_id: string; name: string; input_ref: string;
      input?: JsonValue; input_truncated?: boolean; content?: string }
  | { kind: "tool.progress"; call_id: string; progress_ref: string }
  | { kind: "tool.completed"; call_id: string; name?: string;
      status: "ok" | "error" | "declined"; output_ref?: string;
      output?: string; output_truncated?: boolean; content?: string }
  | { kind: "plan.updated"; plan_ref: string }
  | { kind: "subtask.started"; task_id: string; description?: string; content: string }
      // the payload also carries the task's own kind: "shell" | "agent" | "other"
  | { kind: "subtask.progress"; task_id: string; progress_ref: string;
      description?: string; content: string }
  | { kind: "subtask.completed"; task_id: string;
      status: "ok" | "error" | "stopped"; summary?: string; result_ref?: string; content: string }
  | { kind: "background_tasks.replaced"; tasks_ref: string;
      tasks: { task_id: string; kind: "shell" | "agent" | "other"; description: string;
               started_at_unix_ms: number }[];
      count: number; truncated: boolean; content: string }
  | { kind: "workspace.change"; change_ref: string }
  | { kind: "provider_request.opened"; provider_request_id: string;
      request_kind: "question" | "permission"; tool_name: string;
      payload_ref?: string; payload: Record<string, unknown>; payload_truncated?: boolean;
      display: Record<string, unknown> }
  | { kind: "provider_request.replied"; provider_request_id: string;
      request_id: string; response_digest: string; disposition: "allowed" | "denied" | "answered";
      evidence: "transport_write" }
  | { kind: "provider_request.invalidated"; provider_request_id: string;
      reason: "provider_cancelled" | "process_exited" | "execution_ended"
        | "background_wait_ended" | "provider_input_failed" }
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

Other large or sensitive event bodies use immutable, access-controlled references. A reference alone is not renderable, so `tool.started` and `tool.completed` carry a bounded inline copy beside the digest, as `message.final` carries `content` beside `content_ref`: `input` is the tool's input JSON while it fits in 32 KiB (else a truncated pretty rendering with `input_truncated: true`), `output` is the result flattened to text under the same bound, and `content` is the timeline entry's text (`Used <name>` / the output). The module masks the dispatch secrets and the workspace token in the structured values before flattening or bounding them (the dispatcher names them in `G5D_SECRET_ENV_NAMES`), so an escaped or cut secret cannot slip past the dispatcher's exact-match redaction of the emitted line. The digest remains the identity of the full body. `message.final` and `thought.final` `content` and a `message.delta` `text_delta` are masked and bounded at 256 KiB as serialized the same way (`content_truncated` / `text_truncated`), and `provider_request.opened` carries its masked `payload` and `display` under the API's own 64 KiB per-field bound (structure kept, long string leaves cut; a value too wide to keep its structure becomes the record `{"g5_truncated_rendering": "<cut rendering>"}`) beside `payload_ref` and `payload_truncated`, so a request the API accepts is journaled uncut, with the supervisor keeping the complete request for the envelope it writes back to the provider; so no journaled event approaches the dispatcher's 1 MiB event bound, whatever one provider line carried. The Claude adapter's `background_tasks.replaced` and `subtask.*` events likewise carry bounded inline fields beside their references: at most 20 listed tasks (`count` is the full size and `truncated` marks a cut) and 300 bytes per task id, description or summary, masked with the same module secrets, with `content` as the timeline text; `tasks_ref` is the digest of the full set. Each `background_tasks.replaced` replaces Claude's whole reported set and is empty when none remains (§10.1). Native request control payloads follow §8.2’s private inline-body/display projection rule. A C2 `notice` may describe an error, but it does not terminate the execution.

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
  session_files_state: "present" | "absent" | "persist_failed";
  usage?: UsageV1;
  last_event_seq: number;
};
```

**Post-MVP (P4.12: proxy-metered records keyed by a host-minted `request_id` as the sole billing input, host durable metering replay and display-only adapter usage).** MVP behaviour: usage is recorded as today, adapter result usage plus the proxy's provider usage into `usage_records`, with typed quota headers. Proxy-metered records, keyed by a host-minted unique upstream `request_id` with owner/workspace/incarnation/session/task/attempt attribution from trusted registration, are the sole billing input. Several upstream requests may occur in one attempt, so the attempt tuple is not the deduplication key. Host durable replay retains metering across VM loss; missing/unknown provider cost remains unknown rather than invented. Adapter events/outcomes and chat-message token counters and usage display metadata are display projections and never call `spend_service.record_usage` again. Quota capture uses a provider-specific allowlist such as the retained Anthropic unified quota headers; unknown headers and credentials are excluded. VM loss before adapter output does not erase already metered spend.

The attempt `reason` is a cause, distinct from `AgentTurnOutcomeV1.status`: `exit_code|provider_error|vm_lost|timed_out|cancelled|interrupted|suspended`. A normal validated result/exit uses `exit_code`; protocol/provider failures use `provider_error`; the API alone synthesizes `vm_lost`. The canonical spelling is `timed_out`, never `timeout` in persisted attempt state. TaskService settles the assistant message with the attempt transaction:

| Attempt observation | Assistant `chat_messages.status` |
|---|---|
| `succeeded` | `complete` |
| `failed`, `timed_out`, protocol/provider failure or unexplained nonzero exit | `error` |
| `cancelled`, `interrupted`, `suspended`, API-side `vm_lost` | `stopped` |

Task/session UI derives held status from the task and hold reason, never the assistant row alone. Old-attempt events remain history and cannot overwrite current task state.

Before any C2, diagnostic or module-log payload leaves the VM, trusted runtime redaction matches all configured nonempty task-secret values regardless of environment-key name, including short values, overlapping occurrences, JSON string leaves and split stream chunks; retain the existing streaming overlap machinery. Redact any credential value known at its owning proxy boundary without exporting host credentials to the guest merely for filtering. Redaction is protection against accidental output, not a claim to prevent intentional encoding/exfiltration by authorized code. Raw provider-session bundles stay supervisor-only and byte-exact (§6.2), never reader-visible or logged. Regression evidence must distinguish timeline redaction from session-file durability.

The outcome terminates one attempt. It contains no answerable native-request blocker. Interruption/suspension/VM-loss continuation is a TaskService decision for the same task; a tool result, refusal, request opening or EOF alone cannot establish final success.

## 10. Claude Code direct CLI native input/output

### 10.1 Transport and launch target

The source-backed transport target is `claude -p --output-format stream-json --input-format stream-json --verbose --permission-prompt-tool stdio`, with existing exact-pin-supported session/model/policy flags. Anthropic's [permission routing source](https://github.com/anthropics/claude-agent-sdk-python/blob/main/src/claude_agent_sdk/types.py) selects `stdio`, and its [transport](https://github.com/anthropics/claude-agent-sdk-python/blob/main/src/claude_agent_sdk/_internal/transport/subprocess_cli.py) passes it to the CLI. This is not a new invented flag; the repository records the exact `2.1.285` initialization and policy behavior described in §10.4. Do not blindly copy flags or optional fields from another release.

Remove `--disallowedTools AskUserQuestion` and the custom `mcp__g5-tools__request_permission` prompt target. Keep unrelated MCP tooling configured normally. Register the native `PreToolUse` callback for `AskUserQuestion` through the SDK initialization handshake. Defer a foreground question when no background work is active, and publish it only when Claude confirms `stop_reason=tool_deferred`. Otherwise retain its live callback. Submit one reserved user message for ordinary turns; for a frozen native answer, launch `--resume` without a user message, match the native tool-use ID, and continue reading stdout while writing only correlated native control responses and the owner's End interrupt below. Do not close stdin immediately after the prompt. A `result` ends one Claude turn, not the attempt: while Claude reports background tasks or background sub-agents that it owns, stdin stays open. The adapter closes stdin only when Claude is between turns, its background set (replaced whole by each `system/background_tasks_changed`) is empty and no native request is pending; once background work was seen it first waits 3 s, so the report turn Claude starts right after the set empties is not cut off. With nothing in the background this happens right after the first result. The 15-second provider exit grace runs while Claude is idle after its input closes, and also during a turn after an End, a request opened after the close, or an undeliverable reply; cancellation remains bounded signal escalation. The §10.4 recordings show this background, follow-up and interrupt behavior at the pin.

Each follow-up turn that Claude starts to report finished background work (a repeated `system/init` with the same session id, ending in its own `result`) belongs to the same attempt and assistant reply. Its completed units follow earlier ones, and a result-only answer of turn N≥2 uses block id `g5:terminal-result:N`. `result.usage` tokens and `duration_ms` are per turn while `total_cost_usd` and `modelUsage` are cumulative, so the adapter sums usage tokens across turns and keeps the latest cumulative cost. Any error result fails the attempt. Native requests from a follow-up turn are answerable while stdin stays open.

The owner ends a background wait through §4.5, which reaches the adapter as the control line `{"kind":"background.end","request_id":…}`. The adapter sends Claude's in-band `{"type":"control_request","request_id":"…","request":{"subtype":"interrupt"}}` at once when Claude is between turns, and after letting a running turn continue for up to 10 s otherwise; it journals `notice(code=background_wait_ended, level=info)` and closes stdin when Claude answers or after 10 s. If Claude reports no remaining background work when End would take effect, End does nothing more and the reply finishes normally. The attempt then settles normally: a clean exit with successful turn results succeeds, while a provider that does not exit within the 15-second exit grace is stopped and the attempt settles `interrupted` with code `background_wait_ended`. A turn the End interrupt aborts and the exit status 1 that Claude reports after it at the pin (§10.4) are End's effect, not a failure. The adapter recognizes such a turn by its `error_during_execution` result with an `aborted_*` `terminal_reason`, and only when Claude did not refuse the interrupt; any other error result fails the attempt. The adapter accepts a `control_response` only for its SDK initialization or an interrupt id it issued; any other control response fails closed. Writes to Claude's input after the prompt, native replies and the End interrupt alike, are queued in order. The adapter writes them without blocking, so Claude's output, cancellations and every clock keep being handled while Claude is slow to read.

- A native reply counts as delivered only once its line is completely written. Until then its request stays pending, retains any permission deadline, and can be cancelled by Claude. A reply not yet started for a cancelled request is dropped.
- A requested attempt limit and permission-request deadlines end the attempt as usual. When both have passed, the attempt limit decides: `timed_out`, with process-bound requests invalidated as `execution_ended`. Otherwise an expired permission request settles `suspended` with code `native_response_timeout`. A confirmed deferred unanswered question survives execution end.
- If writing makes no progress for 10 s, or the pipe is closed, Claude's input is dropped, since a partial line may have been written.
  - If an owner's reply was among the unwritten frames, the pending requests are invalidated with reason `provider_input_failed`. Claude's own output still settles the attempt, so a clean finish succeeds. A provider still running when the exit grace ends is stopped. It settles `suspended` with code `native_response_undeliverable` (the task is held for Continue), or `interrupted` with code `background_wait_ended` when End is in effect.
  - If only the End interrupt was unwritten, the pending requests are invalidated with reason `background_wait_ended`, and End's bounded stop follows.
- End takes effect when the first byte of its interrupt is written. That is when the notice is journaled, the reply wait starts, and an aborted turn becomes End's effect. An interrupt still queued behind other input is withdrawn if Claude reports no remaining background work meanwhile. End is due 10 s after it was received. If its interrupt has not started being written within a second after that, End ends the pending requests, drops the input before publishing its notice, and the bounded stop follows.
- The earliest clock is checked ahead of Claude's output, control lines and writes. After a delayed wakeup or slow event delivery, an expired request is settled before a late reply is written or a buffered cancellation is read. Stop and the clocks are checked again after any handler that waited, before End or the input window act on what changed meanwhile.
- The End notice says when End also stopped a running turn.

### 10.2 Native request and response shape

The [official control handler](https://github.com/anthropics/claude-agent-sdk-python/blob/main/src/claude_agent_sdk/_internal/query.py) distinguishes `control_request`/`can_use_tool`, matches `request_id` in `control_response`, and abandons cancelled requests. The minimal reference envelopes are:

```json
{"type":"control_request","request_id":"req-1","request":{"subtype":"can_use_tool","tool_name":"AskUserQuestion","input":{"questions":[]}}}
{"type":"control_response","response":{"subtype":"success","request_id":"req-1","response":{"behavior":"allow","updatedInput":{"questions":[],"answers":{}}}}}
```

These illustrative envelopes omit actual question content; production validation must not treat empty questions/answers as user consent. G5's owner reply uses `{"g5AnswersByIndex":{"0":{"selectedOptionIndexes":[1]}}}`: every question position has selected option positions and optional nonempty `otherText`. Single-select questions require one choice or free text; multi-select questions allow multiple choices and free text. Under the request lock, the API validates completeness, option bounds, unique selections and selection mode against the stored payload. Question and option positions survive display redaction, including text collisions. The live adapter uses the same validator against its original input, reconstructs the original question-text keys and option labels, and adds the native `answers` string map while preserving the rest of that input. An ordinary tool-permission request uses its native allow/deny response instead. This follows Anthropic's [user-input contract](https://code.claude.com/docs/en/agent-sdk/user-input); G5 supplies presentation and response transport, not a replacement question protocol.

### 10.3 Normalization and lifecycle

| Native observation | G5 handling |
|---|---|
| `control_request` for `can_use_tool` / `AskUserQuestion` | Persist `provider_requests(kind=question)`, emit `provider_request.opened`, show provider-defined input. |
| `control_request` for another tool requiring approval | Persist kind `permission` under frozen policy; show the sanitized action. |
| Correlated native response written | Mark reply delivery, emit `provider_request.replied`; tool progress/outcome remains separate. |
| `control_cancel_request` or process/attempt end | Retire the process response channel. Preserve confirmed deferred questions across execution end; explicit cancellation still closes them. |
| User echo/input acknowledgement | Reconcile the reserved input using exact-pin evidence, never SDK-only assumptions. |
| Assistant/tool/stream events | Normalize into C2 message, tool, usage and progress events; no prose-to-state inference. |
| `system/background_tasks_changed`, `task_started`, `task_progress`, `task_updated`, `task_notification` | Replace the adapter's background set and emit `background_tasks.replaced` or `subtask.*`; the set decides when stdin closes (§10.1). |
| Each turn's result frame, EOF and exit status | Arbitrate one C3 attempt outcome after the last turn; an error result in any turn, or contradictory or malformed evidence, cannot become success. |

The old SDK `0.3.273` message/method inventories remain in the historical unversioned companion. They are neither an active dependency nor the native schema for the pinned CLI. The implementation owns the actual exhaustiveness fixtures for the pinned `2.1.285` including control envelopes.

### 10.4 Exact-pin evidence

The fixtures in `tests/fixtures/agent-turn-native/claude/` were re-recorded on 2026-09-30 from `@anthropic-ai/claude-code@2.1.285` on Linux aarch64 in a clean container, with the production bidirectional flags and the checked-in recorder `tests/fixtures/agent-turn-native/record.py`. They preserve the complete sanitized stdout streams for a multi-option `AskUserQuestion`, a Bash permission allow, the same permission denied, SIGINT while the question is pending, a successful result while stdin remains open, a background Bash job that Claude reports in its own follow-up turn while stdin stays open (`background-bash-report`), the same job with stdin closed at the first result (`background-bash-eof`), a background sub-agent stopped by the host's in-band `interrupt` while Claude is idle (`background-agent-interrupt`), and the same interrupt while Claude's turn runs a foreground command next to a background sub-agent (`background-agent-interrupt-turn`). Matching response fixtures contain the host-to-provider envelopes actually accepted by the pinned binary, including that interrupt.

The additional `bypass-question` scenario was recorded on 2026-10-11 with the same pinned Linux binary and `--permission-mode bypassPermissions --permission-prompts host --permission-prompt-tool stdio`. Bash creates the harmless marker without a permission request; the process then waits for `AskUserQuestion`, receives the host answer `Blue`, uses it in its result and exits successfully.

The additional `deferred-question.json` captures the pinned native SDK hook, confirmed deferred tool, and restored callback after copying the saved Claude home. The restored callback preserves the tool-use ID, accepts the answer `Blue` without a user prompt and produces that result. Its metadata identifies the selected fields and host callback-ID normalization.

The recordings establish these transport facts at the repository pin: native questions and prompting tool permissions arrive as `control_request` with `subtype=can_use_tool`; the response nests the correlated `request_id` under `response` and retains the provider input in `updatedInput`; interrupting a pending request emits `control_cancel_request` before an error result; a denied permission does not execute the tool; and a successful result does not itself close the provider while stdin remains open. Background state arrives as `system/background_tasks_changed` (the whole live set of `local_bash` and `local_agent` tasks, empty when none remain), `task_started`, `task_progress`, `task_updated` and `task_notification`. With stdin open, Claude reports a finished background job in a new turn: a second `system/init` with the same session id, then a second `result` with `result_index` 1. With stdin closed at the first result, Claude stops a background Bash job about 5 s after EOF and exits 0. An `interrupt` while Claude is idle stops a background sub-agent and is answered by a `control_response` success `{"still_queued":[]}`; Claude starts no report turn and exits 0 within about a second of EOF. An `interrupt` during a turn is also answered at once; it stops background sub-agents and the turn's foreground command, ends the turn with an `error_during_execution` result whose `terminal_reason` is `aborted_tools` or `aborted_streaming`, and Claude exits 1 at EOF. Background Bash jobs survive an interrupt and stop at EOF (seen in probes). Claude also emits `task_started` and `task_notification` with `is_backgrounded: false` for a long foreground command, its own or a sub-agent's; those are not background work. `backend/g5d-agent` compiles these fixtures into protocol tests, rejects malformed or unknown control envelopes, and requires every turn's result to succeed, EOF and a successful process exit before reporting success, except for a turn the owner's End aborted and the exit status that follows it (§10.1). The historical P3.1 recordings under `tests/fixtures/provider-frames/` remain unchanged as the prior transport evidence.

## 11. Codex MVP and later App Server

### 11.1 MVP: `exec --json`

Keep the `codex exec [resume <id>] --json` adapter recorded at `0.153.3`, its supported argv and explicit noninteractive sandbox/approval policy. At the `0.159.3` pin a command turn emitted the same event types and `turn.completed.usage` keys; `pinned_cli_exec_stream_maps_to_one_completed_turn` in `backend/g5d-agent/src/runner/codex.rs` replays that stream. Write one prompt, close stdin, parse its JSONL events and settle final provider evidence with EOF/exit. `thread.started` binds the provider session; item progress updates the shared timeline. The actual admitted event/item union comes from the recorded fixtures and that replay, not a newer reference dump. See [the implemented CLI contract](lean-provider-cli-contract.md).

Codex MVP has no native interactive question/approval response channel. A prose question is ordinary transcript output, so the owner can send a later ordinary conversation message; it is never inferred into an actionable request card. Do not register custom `ask_user`, `g5d ask` or MCP permission substitutes to simulate one. If a policy requires interactive response, reject it before launch. Unexpected interactive requests become an explicit unsupported-capability/protocol failure, not an answerable request row or silent auto-approval. This applies equally to typed and templated agent tasks. Unrelated MCP tools remain supported.

### 11.2 Later: App Server over stdio

P3.7 is the deliberate migration to `codex app-server` using stdio JSONL: initialize, manage the provider thread/turn, parse native server requests and send correlated responses. The [official protocol](https://developers.openai.com/codex/app-server/) documents request IDs, native user-input and permission requests, request-resolution notifications, and CLI-generated version-specific schemas. Keep this adapter behind the same `agent.turn`, shared conversation, `provider_requests` and logical-task attempt contracts. Do not introduce a separate Codex question engine or expose a network listener.

Generate and store the chosen binary's schema; verify exact method names and capability negotiation instead of assuming today's online names exist at `0.159.3`. The design chooses the transport direction, not an untested pin bump. Enable interactive UI only after native question/approval, cancellation, process-death invalidation and same-task continuation fixtures pass. No replies to old App Server requests are replayed into a new process merely because a thread can be resumed.

## 12. Durable projections and authority

| Projection | Authority/input | Rule |
|---|---|---|
| Logical task and job/DAG status | TaskService; attempt outcomes and continuation policy | Same task id through attempts; only final logical success releases success dependents. |
| Attempts | TaskService; confirmed start, C3 or reconciled incarnation loss | One execution per identity; no new attempt for ambiguous delivery. |
| Shared conversation and inbox | `session_inputs` queue plus linked delivered `chat_messages`; human/workflow inputs | One shared conversation; queued input and delivered transcript have distinct authorities and stable acceptance links. |
| Provider-request history and reply projection | Native control stream plus owner response | Confirmed deferred questions remain answerable after native restore; ordinary requests require their live process. |
| Provider-session files | Existing `chat_session_files`; fenced harvests | Shared across all agent task origins; deferred-question restore must match its saved native tool identity and current owner. |
| Permission policy | Authorized frozen task/session/workflow specification | Independent of origin; provider support validated. |
| Provider account | Current workspace owner/incarnation | Never session creator, workflow submitter or preview launcher. |
| Transcript/tool/usage projections | Ordered C2 events | Rebuildable, bounded/redacted; not attempt completion authority. |
| Replay watermark | Contiguous persisted event prefix | Ack after durable projection write; old-attempt events never mutate a newer attempt. |
| Agent stop receipt | Admission, execution settlement, harvest and request invalidation | Process channels end; confirmed deferred questions retain their durable identity for later restoration. |

### 12.1 Discriminating contract checks for this revision

Use the existing fake-provider CLI through the real dispatcher and the real API/TaskService database boundary; fakes replace providers/host transport, never the authorization or lifecycle service being asserted. P4.2b delivers the dispatcher harness; P4.5 supplies its facade and real session-file router; P4.11 depends on those contracts and records native CLI fixtures. Unit/model checks supplement boundary checks, not replace them. Keep a negative control or mutation that demonstrates each critical test fails when its guard is removed.

- P4.1/P4.6: seeded legacy task-type/status/session rows, staged target columns and status CHECK, same-org/FK integrity, canonical scalar identities, all hold/cause/message-status tuples, same-org/workspace dependency edges, cycle/self-edge rejection and jobless dependency failure.
- P4.6/P4.7: queue edit/withdraw races with reservation CAS, attachment/input/message links, acceptance replay materializes one transcript row, fork/carry excludes pending queues, and three queued messages yield one reservation/acceptance per attempt; loss before acceptance with no journal gives an owner-visible `delivery_unknown` hold; redelivery increments its generation once, Cancel preserves uncertainty audit, and continuation adds one session input and one linked system transcript row only on acceptance, without replaying the original delivered prompt. Competing Continue/Cancel and stale prior-attempt C2/C3 cannot create a second live attempt.
- P4.11/P4.6: every admitted policy mapping and every rejected combination, true native Claude question/approval recordings, Codex prompt-plus-EOF/no-question behavior, complete/partial/uncertain response writes, RPC timeout followed by replayed reply evidence, and native wait/death/transfer races. The old recorder disabling AskUserQuestion is not evidence of the native transport.
- **Post-MVP (P4.12: the deduplicated proxy billing records and the display-usage replay check).** P4.12/P4.6: N upstream requests then lost VM produce N deduplicated proxy billing records, even without adapter output; a replayed display-usage event adds no billable record. Quota headers are allowlisted.
- P4.11/P4.2b: secrets under neutral keys and short values split across frames are absent from reader-visible logs/events; raw session artifacts restore byte-identically through supervisor-only routes. Over-cap/failed latest session harvest prevents stale-generation native resume and invokes delivered-history fallback with a visible warning.
- P4.6/P5.1: replacement preview waiting during loss never launches later; started-but-not-listening gives PREVIEW_NOT_READY, port closure does not fabricate task completion; two bounded slots admit two agent/command executions, queue others and still admit owner preview.

The retained review test recommendations apply to ordinary task, session, authorization and recovery boundaries. Uncertainty never silently resends or drops an input. Result-workflow acceptance tests are outside the reduced MVP scope (§2.4). These tests are implementation deliverables; no runtime test is claimed to have passed by editing this document.

## 13. Compatibility and acceptance gates

1. Pin/image/startup checks prove actual Claude `2.1.285` and Codex `0.159.3`; update fixtures deliberately for any later version. No SDK or extra provider version is installed implicitly.
2. Before Claude cutover, record the exact CLI native initialization, question, permission allow/deny, cancelled request, malformed/unknown control message, result-before-EOF and process-exit behavior. Test both input origins and supported permission policies, including policy configurations that might suppress `AskUserQuestion`.
3. Test indefinite question waiting separately from permission deadlines. Cover answer-before-harvest, answer-versus-Stop, owner cancellation, VM replacement, transfer, mismatched restored tool identity, immutable competing answers and uncertain writes. Confirm native restoration reopens the same tool and consumes the answer without a replacement prompt; ordinary permission requests still expire and cannot reach another execution.
4. Run human-typed and workflow-templated inputs through the same production TaskService/module boundary. Interrupt a workflow child with `job_id` and dependents: continuation retains task id and edges; dependents remain blocked until the logical task settles. Cancel racing continuation never launches a later attempt.
5. Test shared session restore and delivered-history recovery for workflow and human tasks; ambiguous initial input is not duplicated, and unconsumed native replies never become continuation prompts. Transfer preserves history and deferred questions; only the current owner can answer after ready compute is attached.
6. Codex `exec` rejects interactive policy/requests, contains no custom ask/permission fallback and still runs unrelated MCP tools. Later App Server gets its own schema, request/reply, invalidation and stdio-only integration gates before interactive support is exposed.
7. Exercise owner, reader, org admin before/after transfer, removed reader and stale owner through real Agent APIs. Readers cannot submit/cancel any task kind, inspect/replay VM execution, use terminal/ports or start/cancel/replace/view preview. Their persisted timeline/task-output reads and Git-only browsing never ensure/flush/watch the source VM. A reader fork has its own owner and recorded durable source pin.
8. With no attached VM, owner Save/upload, ordinary input/edit and historical-question routes cannot wake it; explicit Start/Resume may reserve queued launch input but must establish authority before dispatch. Reject stale owner/intents after stop, transfer or cancellation. Preserve incarnation fences, bounded/redacted events, credential isolation and cancellation/replay regressions.
9. **Post-MVP (P4.12: the stale-capability case and fail-closed in-VM platform-write calls).** Prove trusted bootstrap graph creation through the actual scoped API/TaskService boundary (§6.6), its idempotent retry and dependency release. Provider/repository descendants with copied IDs, forged graph bodies or stale capabilities create no tasks; plain in-VM platform-write CLI calls fail closed. Native question replies and session-file operations remain separate scoped supervisor paths.

These gates are required future validation, not claims that current deployed behavior already passes.

## 14. Implementation alignment and retired paths

P4.1 adds normalized dependency/inbox/native-request state while preserving shared conversations and old question history. P4.6 owns unified task/attempt state, dependency release, scoped bootstrap graph admission and the native reply projection. P4.7/P4.8 move typed and templated inputs onto the same session/task model; P4.11 owns direct native provider adapters. P4.2b supplies the trusted module boundary and P4.5 supplies ordinary forks/lifecycle and session-file service routes. P4.10 supplies the owner/read-only workspace shell. P3.7 later supplies Codex App Server stdio interaction. The removed returned-results workflow and generic provider-initiated platform-write transport have no MVP schema, APIs, CLI/skill, UI or acceptance dependency; their later product design is outside this contract. Full governance remains separately scheduled in the implementation plan.

Retire reader preview/runtime authority and hidden input-triggered attachment, together with the active `chat_turn` kind, separate task-only conversation storage, `task_questions` bespoke HITL state machine, `ask_user`/`g5d ask`, custom MCP question/permission substitutes, question-wakeup/suspension successors, origin-based permission bypass and reuse of process-specific native control IDs after exit. Restored deferred questions use fresh native response channels under §8.2. Retain unrelated MCP tools and historical records. Update the old CLI contract and recorded fixtures when the implementation changes; it remains evidence of current code, not permission to preserve behavior this design replaces.

This revision changes documentation only. Native protocol direction is source-grounded; exact-pin end-to-end execution, implementation, deployment and observed behavior remain separate milestones.
