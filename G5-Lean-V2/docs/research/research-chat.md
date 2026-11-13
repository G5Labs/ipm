# G5-Lean research: chat sessions — lifecycle, task/worker/runtime mapping, forking, persistence, recovery

Read-only survey of `/Users/zliu/Workspace/G5-Lean` (no files modified). Every claim marked
**OBSERVED** has a `file:line` citation; **INFERENCE** paragraphs are reasoning on top of
observed code and are labelled as such. Line numbers are from the checkout as of 2026-09-12.

Path aliases used in citations (all under `/Users/zliu/Workspace/G5-Lean/`):

| alias | absolute path |
|---|---|
| `chat_service.rs` | `/Users/zliu/Workspace/G5-Lean/backend/g5-api-core/src/services/chat_service.rs` |
| `sticky_runtime.rs` | `/Users/zliu/Workspace/G5-Lean/backend/g5-api-core/src/services/sticky_runtime.rs` |
| `chat_context_service.rs` | `/Users/zliu/Workspace/G5-Lean/backend/g5-api-core/src/services/chat_context_service.rs` |
| `chat_worker_loss.rs` | `/Users/zliu/Workspace/G5-Lean/backend/g5-api-core/src/services/chat_worker_loss.rs` |
| `session_store_service.rs` | `/Users/zliu/Workspace/G5-Lean/backend/g5-api-core/src/services/session_store_service.rs` |
| `codex_session_artifact_service.rs` | `/Users/zliu/Workspace/G5-Lean/backend/g5-api-core/src/services/codex_session_artifact_service.rs` |
| `task_service.rs` | `/Users/zliu/Workspace/G5-Lean/backend/g5-api-core/src/services/task_service.rs` |
| `lease_reaper.rs` | `/Users/zliu/Workspace/G5-Lean/backend/g5-api-core/src/maintenance/lease_reaper.rs` |
| `routes/chat.rs` | `/Users/zliu/Workspace/G5-Lean/backend/g5-api-routes/src/routes/chat.rs` |
| `routes/internal.rs` | `/Users/zliu/Workspace/G5-Lean/backend/g5-api-routes/src/routes/internal.rs` |
| `scoped.rs` | `/Users/zliu/Workspace/G5-Lean/backend/g5-api-core/src/db/scoped.rs` |
| `ws/events.rs`, `ws/channels.rs`, `ws/worker_hub.rs`, `ws/mod.rs` | `/Users/zliu/Workspace/G5-Lean/backend/g5-api-core/src/ws/…` |
| `models/chat.rs`, `models/chat_sticky_runtime.rs` | `/Users/zliu/Workspace/G5-Lean/backend/g5-api-data/src/models/…` |
| `dtos.rs` | `/Users/zliu/Workspace/G5-Lean/backend/g5-internal-api/src/dtos.rs` |
| `task_context.rs` | `/Users/zliu/Workspace/G5-Lean/backend/g5-contracts/src/types/task_context.rs` |
| `worker/runtime_keeper.rs` (registry) | `/Users/zliu/Workspace/G5-Lean/backend/g5d-worker/src/runtime_keeper.rs` |
| `g5d/worker/runtime_keeper.rs` (park loop) | `/Users/zliu/Workspace/G5-Lean/backend/g5d/src/worker/runtime_keeper.rs` |
| `worker/claude_code.rs`, `worker/codex.rs`, `worker/lifecycle.rs`, `worker/inbox.rs`, `worker/g5_api_session_store_proxy.rs`, `worker/guest_runtime.rs` | `/Users/zliu/Workspace/G5-Lean/backend/g5d-worker/src/…` |
| `host/supervisor.rs` | `/Users/zliu/Workspace/G5-Lean/backend/g5d-host/src/supervisor.rs` |
| `migrations/NNN` | `/Users/zliu/Workspace/G5-Lean/backend/g5-api/src/db/migrations/NNN_*.sql` |
| `frontend …` | `/Users/zliu/Workspace/G5-Lean/frontend/src/…` |

---

## A. Data model

### A.1 `chat_sessions` (OBSERVED)
Base table: `migrations/001:512-565`. Columns: `id`, `workspace_id` (FK, cascade), `session_type IN ('chat','task')`,
`task_id` (FK tasks, SET NULL), `scope_type IN ('workspace','node','task')`, `scope_ref`, `model`, `title`, `summary`,
`tags[]`, `linked_commits[]`, `linked_nodes[]`, token/cost totals, `status IN ('active','completed','archived')`,
`message_count`, `forked_from_id` (self-FK), `forked_at_index`, `branch`. Rust model: `models/chat.rs:15-43`.

Columns added later:
- `agent_provider` (`migrations/070`) — CHECK now `('naive_react','fake','claude_code','codex')` (`migrations/123`).
- `required_worker_id TEXT` (`migrations/072`) — "durable exact-worker session binding" (the worker pin).
- `provider_state JSONB` (`migrations/073`) — "internal-only provider runtime state"; the Claude provider session id is read
  from `provider_state.claudeCode.providerSessionId` (`worker/claude_code.rs:9475-9493`).
- `branch_mode TEXT CHECK IN ('workspace_head','job_branch') DEFAULT 'workspace_head'` (`migrations/077`).
- `created_by UUID NOT NULL` FK users (`migrations/122`).
- `attach_preview_task_id` (preview chat-attach; migration 233, referenced in `chat_service.rs:6738, 6769`).

**What `branch_mode` is (OBSERVED):** a persisted, explicit mode never inferred from `branch` (`migrations/077` header;
`chat_service.rs:6711-6725` validates the value on create). Runtime effects found in code:
- Chat tasks bind their repository cut to the **workspace**, not a job: `assignment_configured_scope` returns
  `AuthorityTarget::Workspace { workspace_id }` for `JobType::Chat` and the snapshot comes from
  `resolve_committed_snapshot` (`task_service.rs:6644-6647, 6726-6733`) — i.e. a chat runs against the workspace head cut.
- A sticky runtime is only granted to sessions with `cs.branch_mode = 'workspace_head' AND cs.status='active'`
  (`sticky_runtime.rs:1244-1246`).
- `continue_in_merged_workspace` / merged-continuation queries require `branch_mode='workspace_head'`
  (`chat_service.rs:9659, 9870`); workspace-fork carry forces `'workspace_head'` (`chat_service.rs:10465`).
- Task DTO projection: `chat_branch_modes_for_tasks` (`task_service.rs:27657-27756`); frontend reads
  `task.chatBranchMode ?? "workspace_head"` (`frontend/src/pages/WorkspacePage.tsx:779-781`).

**INFERENCE:** no dispatch-time consumer of `'job_branch'` was found in `task_service.rs`, `job_service` or the worker
(`rg` for `'job_branch'` in non-test code hits only the create-time validator, the trigger matcher for
`job_branch_commit` events and the CLI self-context fixture). The frontend never sends `branchMode` (only the type/schema
mention it: `frontend/src/hooks/useChat.ts:577-629` forwards it if present; no caller sets it). `job_branch` looks
vestigial/API-only today.

**What "owner visibility" is (OBSERVED):** `migrations/122` adds `created_by` "so public chat, chat-job, and chat-task
visibility can be restricted to the user who created the session". Enforcement is in the route guards, not SQL:
- `scoped_db_for_chat_session_view` = workspace membership only, "does NOT require the caller to be the session's
  `created_by` owner: any user who can access the workspace may view the session's conversation history"
  (`scoped.rs:1359-1369`). Used by `get_session`, `get_messages`, `get_session_timeline`, `get_session_task`
  (`routes/chat.rs:2051,2088,2172,2256`).
- `scoped_db_for_chat_session` additionally calls `enforce_chat_session_owner_for_user` → `SELECT EXISTS … WHERE id=$1 AND
  created_by=$2`, non-owner ⇒ 403 (`scoped.rs:1338-1356, 175-204`). Used by every mutation route: `send_message`
  (`routes/chat.rs:2286`), `update_session` (2118), `stop`, `resume`, `continue-in-merged-workspace`, **`fork`
  (`routes/chat.rs:4675`)**, `summarize`, `inject`, `attach`, `provider-control`, `approvals` (4532-4940).
- `list_sessions` SQL filters only by `workspace_id` (`chat_service.rs:6558-6600`) — lists everybody's sessions in the
  workspace (index `idx_session_workspace(workspace_id, created_by, status, updated_at)` from 122 is not used as a filter).

### A.2 `chat_messages` (OBSERVED)
`migrations/001:567-592`: `id`, `session_id` (cascade), `role IN ('user','assistant','system','tool')`, `content`,
`artifacts JSONB`, `citations JSONB`, `input_tokens`, `output_tokens`, `status IN ('pending','streaming','complete','error','stopped')`,
`message_index`, `created_at`; `org_id` added by 009-011. Unique `(session_id, message_index)`
(`migrations/149`, audited by 148, old index dropped in 150). Model `models/chat.rs:54-67`.
A turn = one `user` row (`complete`) + one placeholder `assistant` row (`pending`) inserted together
(`chat_service.rs:8258-8300`).

### A.3 `chat_sticky_runtimes` (OBSERVED)
Created by `migrations/136`; full column list in `models/chat_sticky_runtime.rs:82-141`:
- identity/ownership: `id` (the fencing token; "a superseding runtime is always a NEW row"), `org_id`, `workspace_id`,
  `chat_session_id`, `task_id` (loose, no FK), `worker_id TEXT`, `host_id`, `microvm_id`, `provider IN ('claude_code','codex')`,
  `worker_session_generation` (241), `worker_workspace_root_lease_v1` (247), `host_discard_fence_receipt` (294).
- state: `status IN ('active','reaping','expired','lost')`; `lost_reason IN ('worker_terminated','failed_redeposit',
  'stranded_recovery','stale_liveness','expired_claimed_redelivery')` (169).
- clocks: `idle_since`/`expires_at` (turn-end idle window), `last_liveness_at` (heartbeat), `warm_resume_claimed_at` (227),
  `pending_question_since`/`pending_question_expires_at` (165), `busy_pending_count`/`busy_reported_at` (175, observability only).
- teardown operation (211): `teardown_generation`, `teardown_operation_id`, `teardown_requested_at`, `teardown_deadline_at`
  (= requested + 30 s by CHECK), `teardown_repository_scopes` (array of `{workspaceBranchRepoId, crdtSessionId}`),
  `teardown_scope_provenance` (242), `teardown_disposition IN ('durablyFlushed','preservedForRecovery')`,
  `teardown_outcome IN ('tornDown','alreadyAbsent','forceEvicted')`, `teardown_evidence`, `teardown_acknowledged_at`,
  `teardown_delivery_started_at` (248).
- release/reconciliation: `teardown_workspace_released_at` ("server-derived proof that task-keyed workspace cleanup
  completed", 244), `teardown_reconciliation_kind IN ('registration_retirement','task_workspace_recovery')`,
  `teardown_retired_worker_session_generation`, `teardown_reconciliation_receipt` (247/269).
- indexes: unique live-session `(chat_session_id) WHERE status IN ('active','reaping')` (136), worker-live, worker-lost (147),
  active-task (170), pending-question expiry (166), reaping-teardown (211), unreleased-teardown-task (245),
  registration-retirement (247), task-workspace-recovery (271).
Related: `workspace_settings.chat_sticky_idle_timeout_min INT DEFAULT 60 CHECK 0..240` (136; 0 = kill switch);
`workers.worker_session_generation` (241); `tasks.status` gains `'recovering','recovery_blocked'` (270); SQL functions
`authz.retire_prior_generation_sticky_runtimes`, `authz.prepare_prior_generation_sticky_runtime_retirement`,
`authz.retire_prelocked_prior_generation_sticky_runtimes` and trigger `authz.guard_sticky_task_execution_admission`
(247/273).

### A.4 `claude_session_store_entries` (OBSERVED)
`migrations/110-112`: `id BIGSERIAL` (replay order), `chat_session_id` (cascade), `project_key`, `session_id` (the Claude
SDK session id), `subpath` ('' = main transcript), `entry JSONB`, `org_id` + RLS (111), `client_batch_id`/`batch_position`
with partial unique index for retry idempotency (112). Purpose per the migration header: mirror the Claude Code SDK JSONL
transcript "so a chat that started in one microVM can resume in another … each G5 chat turn spawns a fresh microvm-agent
VM with an empty filesystem". Service `session_store_service.rs:56-267`; reads are project-key-agnostic
(`session_store_service.rs:106-151`).

### A.5 `codex_session_artifacts` (OBSERVED)
`migrations/138`: `(chat_session_id, provider_session_id, relative_path)` unique, `relative_path LIKE 'sessions/%'`,
`artifact_bytes BYTEA` (≤128 MiB), sha256/size checks. "Mirrors the opaque Codex CODEX_HOME/sessions/** rollout artifacts
that `codex exec resume <provider_session_id>` needs after a worker/runtime replacement". Service
`codex_session_artifact_service.rs:35-158`.

### A.6 `chat_session_context_links` + `chat_message_context_resolutions` (OBSERVED)
`migrations/249`: link row per (target_session, source_session) with `mode IN ('snapshot','live')`, `status IN
('active','removed')`, snapshot pointer (`snapshot_through_message_index`, `snapshot_payload`, `snapshot_digest`),
UI projections (`last_resolved_*`), unique active pair; `include_agent_activity` (250). Receipts table
`chat_message_context_resolutions`: per (target_assistant_message_id, context_link_id) immutable receipt with
`resolution_kind IN ('baseline','delta','unchanged','full_refresh','snapshot','removal_notice')` (252), UPDATE/DELETE
revoked and cascades on both sessions (251).

### A.7 Question windows (OBSERVED)
`task_question_windows` (`migrations/167`): one row per sync `AskUserQuestion`, `state IN ('open','answered','idle_timeout',
'cancelled')`, one open per task, `platform_receipt_id` consumed once by the `running → waiting` transition. The sticky
pending-question pair on `chat_sticky_runtimes` (165/166) is a separate bound for sticky chats.

### A.8 Tasks/jobs used by chat (OBSERVED)
`tasks.task_type` admits `'chat'` (`migrations/004`). `create_chat_job` inserts a `jobs` row `job_type='chat', status='running',
max_concurrency=1, total_tasks=1` plus one `tasks` row `task_type='chat', status='pending', branch_name=NULL, base_commit=NULL,
workspace_ref=<ChatTaskContext JSON>` (`chat_service.rs:11497-11580`). `ChatTaskContext` fields (`task_context.rs:634-720`):
`session_id`, `message_id`, `assistant_message_index`, `user_content`, `conversation_history` ("copied from G5's
chat_messages table at assignment time so workers can be stateless across per-turn workspace cleanup"), `attachments`,
`model`, `max_tokens`, `permission_mode`, `effort`, `agent_provider`, `required_worker_id`, `attach_vm`, merge/continuation
fields, `external_chat_contexts`, `historical_chat_contexts`.

---

## B. Lifecycle of one chat turn

### B.1 Public routes (OBSERVED, `routes/chat.rs:1267-1372`, mounted at `routes/mod.rs:328-331`)
`/api/v1/workspaces/:workspaceId/sessions` → `GET /` list, `POST /` create, `POST /start` (create + first message),
`GET /search`. `/api/v1/sessions/:id` → `GET`, `PATCH`, `GET /task`, `GET|POST /messages`, `POST /messages/:mid/stop`,
`GET /timeline`, `POST /approvals/:approvalId`, `POST /provider-control`, `POST /attachments`, `GET /attachments/:id/content`,
`POST /resume`, `POST /continue-in-merged-workspace`, `POST /fork`, `POST /summarize`, `POST /inject`, `POST /attach`,
`GET|POST /context-links`, `PATCH|DELETE /context-links/:linkId`.

### B.2 Send path, inside one transaction (OBSERVED, `routes/chat.rs:2279-3480`)
1. Resolve the immutable per-turn launch contract (`job_service.resolve_job_agent_launch`, `routes/chat.rs:2604-2616`).
2. `ensure_task_session_can_accept_message` (task-scoped sessions; `chat_service.rs:11833`).
3. Discover the session's current chat task (`find_running_chat_task_for_session`), take the parent-job/task advisory cut
   (`lock_chat_dispatch_parent_task_cut`), and for a `waiting` task pre-acquire the LiveLease host-admission guard
   (`routes/chat.rs:2664-2716`).
4. Classify the worker route (`classify_chat_worker_route[_prelocked]`, `chat_service.rs:15249-15298`) →
   `ChatWorkerRoute::{LiveLease{runtime_id,worker_id} | OriginalWorkerEligible{worker_id} | ReplacementEligible{reason,candidates} | Blocked{reason}}`
   (`chat_service.rs:4795-4834`). `Blocked` ⇒ 409 before anything is written (`routes/chat.rs:2755-2759`).
5. `ChatService::send_message` (`chat_service.rs:8134-8365`): takes `lock_session_turn_allocation` (per-session
   `pg_advisory_xact_lock`, `chat_service.rs:546-560`), inserts the user row and a `pending` assistant row, bumps counters.
6. Persist attachments, build `ChatTaskContext` (`routes/chat.rs:2914-2966`), then re-read the task and dispatch by status
   (`routes/chat.rs:2998-3480`):
   - `pending|claimed|running`: optional replacement handoff for a pending pinned task; else
     `refresh_active_chat_task_context_prelocked` and queue an **active-turn steer** frame (resolved receipts, published
     post-commit) unless `cold_delivery_only` (`routes/chat.rs:3135-3260`).
   - `waiting|blocked`: if route is `LiveLease` → `resume_chat_task_on_live_runtime_prelocked` (`chat_service.rs:14080`),
     which locks the task, re-checks host admission and the same-task retiring-runtime fence, transitions
     `waiting → claimed`, `cas_resume_active`s the runtime and stages an inbox push; else replacement handoff
     (`handoff_chat_task_to_replacement_prelocked`, `chat_service.rs:15449`); else
     `reactivate_dormant_task_prelocked` (`chat_service.rs:13690`, task back to `pending` for the cold scheduler); on
     failure `create_chat_job` fallback (`routes/chat.rs:3278-3448`).
   - `recovering|recovery_blocked`: only attach fresh context to the existing task (`routes/chat.rs:3450-3480`).
   - none: `create_chat_job` (new job+task, `chat_service.rs:11458`).
7. Commit, then `publish_chat_turn_effects` (`routes/chat.rs:2340-2440`): publish `task.status`, push the LiveLease frame
   or steer frame via `push_to_inbox` (WorkerHub), publish context events.

### B.3 Who runs the agent, where (OBSERVED)
- The worker polls/claims/ACKs the chat task (`/api/internal/workers/:id/poll|…/claim|…/ack`, `routes/internal.rs:3113-3123`; heartbeat 3143, transition 3168).
  On ACK the API may grant a sticky runtime (see B.5).
- The provider sidecar (Claude Code SDK node process or `codex exec`) is spawned by `run_sidecar`
  (`worker/claude_code.rs:5053-5100`) either locally (LocalPipeTransport) or, on the microvm-agent path, **in-VM** driven
  over vsock (`remote_transport`). Guest constants: cwd/workspace `/work` (`GUEST_WORKSPACE_DIR`), `CODEX_HOME=/opt/g5/codex-home`,
  agent uid 1000 (`worker/guest_runtime.rs:10-13`). The same file states "Both run one task per VM"
  (`worker/guest_runtime.rs:31-32`); the supervisor config speaks of "the per-task VMs" (`host/supervisor.rs:572`).
- Working directory persistence: the worker lifecycle is Phase 1 PREPARE (checkout base), 2 CLONE+PATCH, 3 EXECUTE,
  4 COLLECT, 5 CLEANUP (`worker/lifecycle.rs:2178-2260, 1940-1975`). Filesystem writes made by the agent are bridged to
  CRDT before the turn ends (`host/supervisor.rs:11831 bridge_microvm_agent_fs_to_crdt`; test
  `worker/lifecycle.rs:6064 phase_execute_bridges_provider_filesystem_writes_to_crdt_before_waiting`).
  **INFERENCE:** the durable home of a chat's edits between cold turns is the workspace draft/CRDT layer, not the VM disk;
  the exact draft/commit mechanics are outside this survey.
- Between turns, two regimes:
  - **Cold (no sticky grant):** Phase 5 runs, the VM/sidecar is torn down; the next turn is a new claim, new VM, and the
    provider transcript is replayed from Postgres (A.4/A.5).
  - **Sticky (grant at ACK):** the turn ends with `status="waiting"` + `held_runtime_id` (`dtos.rs:2096-2107`); the worker
    deposits the live `HeldClaudeSidecar` (child, process group, transport halves, live state incl. `provider_session_id`,
    LLM proxy, session-store proxy; `worker/claude_code.rs:7527-7545`) and Phase 5 CLEANUP is skipped when
    `held_runtime_probe` says the hold is active (`worker/lifecycle.rs:1947-1962`). The worker's `hold_runtime` spawns the
    park loop `run_held_runtime` which drains the retained inbox for a resume `Message` frame or a teardown signal
    (`g5d/worker/runtime_keeper.rs:302-420`); on the microvm-agent path the host keeps the VM `RunHandle`, carrier, proxies
    and CRDT peer in a `ParkedRuntime` and the VM keeps counting against `max_concurrent_microvms`
    (`host/supervisor.rs:2933-2985`). A warm resume ACKs the re-delivered claimed task and runs Phases 3+4 only
    (`g5d/worker/runtime_keeper.rs:700-760`, `worker/lifecycle.rs:1978-2005`).

### B.4 Vocabulary (OBSERVED)
- **Sticky runtime**: one `chat_sticky_runtimes` row = "one held provided-agent runtime (sidecar process, background
  tasks, PTYs, ports, log streams, workstorage, microVM) kept alive on its worker between user turns" (`migrations/136` header).
- **Warm resume / LiveLease**: re-delivery of the next turn onto the still-live runtime via `waiting → claimed` + inbox push
  (`chat_service.rs:4801-4809`; `task_service.rs:21227-21390` for the queued-follow-up variant with
  `WARM_RESUME_PRE_ACK_CLAIM_LEASE_SECS`; `sticky_runtime.rs:1641-1700 cas_resume_active`).
- **Teardown generation / operation**: monotonic `teardown_generation` + `teardown_operation_id` stamped by
  `begin_or_resume_teardown` (`sticky_runtime.rs:2785-2830`), delivered as `StickyRuntimeTeardownInstructionV1` in the
  heartbeat response (`dtos.rs:1058-1070`, `HeartbeatResponse.teardown_instructions/teardown_runtime_ids` 1519-1530), acked
  with `StickyRuntimeTeardownAckV1 {disposition, outcome, evidence}` (`dtos.rs:1073-1081`); "prevents a delayed teardown or
  acknowledgement from acting on a newer runtime state" (`migrations/211`).
- **Release proof**: `teardown_workspace_released_at` — separate from the ACK; "a typed teardown acknowledgement can be
  terminal preservation evidence; it does not necessarily prove that the task-keyed workspace path was reclaimed"
  (`migrations/244`). It is what the same-task cold-claim fence waits on (`RETIRING_RUNTIME_FENCE_PREDICATE_SQL`,
  `sticky_runtime.rs:355-383`).
- **Registration retirement**: when a worker re-registers with the same id, `workers.worker_session_generation` rotates and
  prior-generation `active`/pre-operation `reaping` rows are moved to `reaping` with
  `teardown_reconciliation_kind='registration_retirement'` by a BYPASSRLS SECURITY DEFINER function (`migrations/247`),
  split into prepare/rotate/retire steps in 273. Caller: `sticky_runtime.rs:7248-7419`.
- **Claim transitions**: every execution-admission write on a task with an unresolved runtime fence is serialized by the
  advisory lock `g5:sticky-task-claim-transition:<task_id>` (`sticky_runtime.rs:386-400`) and backstopped by the trigger
  `authz.guard_sticky_task_execution_admission` raising SQLSTATE 40001 (`migrations/273`, function body near the end).
- **Task-workspace recovery**: `teardown_reconciliation_kind='task_workspace_recovery'` obligation + task statuses
  `recovering` / `recovery_blocked` (`migrations/269-272`; watchdog `lease_reaper.rs:1769-1790`).

### B.5 Sticky runtime state machine (OBSERVED transitions with code owners)

| from → to | trigger | code |
|---|---|---|
| (none) → `active` | chat ACK, iff worker advertises `sticky_runtime:v1`, `workspace_settings.chat_sticky_idle_timeout_min > 0`, session `workspace_head` & active, no live row for the session (`ON CONFLICT … DO NOTHING`) | `task_service.rs:12770-12792` → `sticky_runtime.rs:957-1295 grant_at_ack`; returned as `AckTaskAssignmentResponse.granted_sticky_runtime_id` (`dtos.rs:973`) |
| `active` (turn) → `active` (parked) | worker transition `waiting` with `held_runtime_id`: sets `idle_since=now()`, `expires_at=now()+timeout`, clears question window | `task_service.rs:20010-20035, 21434-21450` → `sticky_runtime.rs:1297-1325 stamp_turn_end_idle` |
| parked → `active` (turn) | LiveLease send or queued-follow-up redelivery: clears idle window, stamps `warm_resume_claimed_at`; requires fresh `last_liveness_at` (300 s), matching worker generation, fresh worker heartbeat | `sticky_runtime.rs:1641-1700 cas_resume_active`; callers `chat_service.rs:14307,14915`, `task_service.rs:21311` |
| liveness renew | heartbeat `held_runtime_ids` (parked) / provider timeline events (in-flight) | `sticky_runtime.rs:5346 renew_liveness_and_list_teardowns_for_worker_session`, `5553 renew_liveness_for_active_task`; DTO `dtos.rs:1201` |
| idle deferral | heartbeat `busy_runtime_ids` re-stamps `expires_at`, capped by `BUSY_DEFERRAL_CAP_SQL` (≥30 min, ≤6 h) | `sticky_runtime.rs:6836-6870` |
| question bound | `arm_pending_question_idle_for_task` at question post; reaped by `reap_pending_question_expiry` (turn still running ⇒ inbox abort `idle_timeout`) | `sticky_runtime.rs:1718, 7725-7770`; `lease_reaper.rs:1806-1840` |
| parked `active` → `reaping` | idle expiry (`expires_at < now()`, task not claimed/running) | `sticky_runtime.rs:7424-7535 reap_idle_expiry` (task `pending` ⇒ lease-expiry + `recovering`) |
| `active` → `reaping` | pressure eviction (LRU idle) on poll/claim | `sticky_runtime.rs:6122 evict_lru_idle_for_worker`; `task_service.rs:10108,12328` |
| `active` → `reaping` | draft discard on the workspace fences every parked runtime | `sticky_runtime.rs:2182 fence_parked_runtimes_for_draft_discard`; `workspace_ops.rs:1889` |
| `active`/`reaping` → op stamped | `begin_or_resume_teardown` (gen+1, op id, deadline +30 s, repo scopes) | `sticky_runtime.rs:2785-2860` |
| `reaping` → `expired` | only after a typed ACK (`confirm_or_backstop_reaping`: "elapsed time and heartbeat omission are never teardown proof") | `sticky_runtime.rs:7772-7850`; ack ingestion `4409 ack_teardown_for_worker_session` |
| `active`/`reaping` → `lost` | worker terminated (`mark_lost_for_workers`), failed re-deposit (`lost_runtime_id`), stranded recovery, stale liveness (300 s), expired claimed re-delivery | `sticky_runtime.rs:5592, 5671, 5849, 6003`; `task_service.rs:16865`; `lease_reaper.rs:586,1854` |
| `lost` echo → host force-evict | only `lost_reason IN (worker_terminated, failed_redeposit, stale_liveness)` on the `HostRelay` channel | `sticky_runtime.rs:1964-2020` |
| prior generation → `reaping`(registration_retirement) | same-id worker re-registration | `migrations/247/273`; `sticky_runtime.rs:7248-7419` |

Reaper tick per org (`lease_reaper.rs:1900-1935`): task-workspace recovery → idle expiry → pending-question expiry →
stale-liveness lost → acknowledged-reaping convergence → task question windows.

Worker-side keeper states: `KeeperSlotState::{Parked, Resuming, TearingDown}` (`worker/runtime_keeper.rs:255-266`);
teardown signals `TeardownSignal::{Idle, ServerLegacy, LocalPreserve, Typed(instruction)}` (336-343).
Heartbeat riders: `held_runtime_ids`, `wake_runtime_ids`, `wake_completions`, `busy_runtime_ids`, `busy_runtime_pending`,
`teardown_acks`, `workspace_reconciliation_receipts/obligations`, `parked_runtime_identities` (`dtos.rs:1195-1262`).

### B.6 Background wakes (OBSERVED)
A parked sidecar can emit `g5.claude.wake` frames (`worker/runtime_keeper.rs:96-135`); the heartbeat carries
`wake_runtime_ids` and the API dispatches a system-originated wake turn (`chat_service.rs:14551-14620
dispatch_background_wake_with_entries`, guarded by `load_parked_owned`, session active, task waiting) with a prompt
prefixed `[G5 background-work wake]` (`chat_service.rs:104 BACKGROUND_WAKE_PROMPT`; worker tag
`worker/claude_code.rs:4857`).

---

## C. Forking

### C.1 What "fork" means in this codebase (OBSERVED)
Two distinct features share the word:
1. **Chat-session fork**: `POST /api/v1/sessions/:id/fork {fromMessageIndex}` → `fork_session`
   (`chat_service.rs:9916-10135`): creates a new `chat_sessions` row in the **same workspace** with `forked_from_id`,
   `forked_at_index`, same `branch`/`branch_mode`/`agent_provider`/`model`, `task_id = NULL` ("forked session is not driven
   by original task's worker"), and copies `chat_messages` with index ≤ N re-indexed from 0 (pending/streaming assistant
   rows normalized). `provider_state` is not copied (INSERT column list at 10005-10015 omits it) — the fork has no provider
   session id and cold-reconstructs from `conversation_history`. UI: per-message "fork" affordance
   (`frontend/src/components/chat/ChatMessage.tsx:46-47 onFork`) → `forkSession.mutate` (`WorkspacePage.tsx:12570,12688`) →
   `useForkSession` (`frontend/src/hooks/useChat.ts:898-925`). The route uses the owner-only guard (`routes/chat.rs:4675`).
2. **Workspace fork/clone with chat carry** (`CloneDialog`, flag `forkChatEnabled`, `useChat.ts:72-82`):
   `copy_sessions_to_workspace` (`chat_service.rs:10380-10515`) copies selected **Claude Code** sessions into the new
   workspace, drops task/worker binding, forces `branch=NULL, branch_mode='workspace_head'`, and seeds a
   "history-bootstrap" provider-state marker so the first turn rebuilds context from copied `chat_messages`
   "rather than a worker-bound native transcript".
   The CLI `g5d fork` (`/Users/zliu/Workspace/G5-Lean/backend/g5d-cli/src/fork.rs`) is a local workspace fork, unrelated to chats.

### C.2 Is a fork ever *forced*? (OBSERVED + INFERENCE)
- No code path was found that refuses a chat turn and demands a session fork. Searches for per-workspace chat exclusivity
  ("one active chat", "same workspace", etc.) in `chat_service.rs`, `task_service.rs`, `sticky_runtime.rs`,
  `host/supervisor.rs` found nothing of that kind.
- The only "fork to proceed" prompt is for **read-only workspaces**: the UI says "workspace is read-only; prompts the user to
  fork to make edits" (`WorkspacePage.tsx:11862`), and server-side the agent's message writes are refused via
  `ensure_workspace_allows_mutation(…, DirectAgentWrite, "chat.update_message_status")` (`chat_service.rs:11033-11038`,
  also 11313). That is a *workspace* fork (feature 2), not a chat fork.
- **INFERENCE (why the design leans on copying/forking):** the conflicts it avoids are (a) a chat is pinned to one worker/
  runtime (`required_worker_id`, one live sticky row per session), so continuing a conversation elsewhere requires a new
  session that rebuilds context from `chat_messages`; (b) chats bind to the *workspace* authority scope
  (`AuthorityTarget::Workspace`) and their edits flow through the workspace draft/CRDT, so read-only or foreign workspaces
  cannot host them; (c) the provider transcript is per-worker/VM unless mirrored. None of these is a "one chat per
  workspace" rule.

### C.3 Can two chats run concurrently in the same workspace today? (OBSERVED)
Nothing prevents it: turn allocation is locked per **session** (`lock_session_turn_allocation`), the sticky uniqueness is per
**session** (`uq_chat_sticky_runtimes_live_session`), each chat task is its own job/task and — on the microVM path — its own
VM (`guest_runtime.rs:31`). Limits are capacity-based: `workspace_settings.max_concurrent_tasks` (`migrations/001:671`),
worker `max_concurrent_tasks` (`migrations/042`), sticky capacity per worker (`STICKY_CAPACITY_SUBQUERY_SQL`,
`sticky_runtime.rs:788-800`), host `max_concurrent_microvms`. Cross-chat coupling inside a workspace does exist:
a draft discard fences *all* parked runtimes of the workspace (`fence_parked_runtimes_for_draft_discard`), and
`lock_sticky_workspace_transition` serializes grants with discards (`sticky_runtime.rs:858-871`).
**INFERENCE:** two concurrent chats each edit their own VM's `/work` copy of the same workspace head and reconcile through
the CRDT/draft bridge; whether that reconciliation is conflict-free is not established by this survey.

### C.4 A second user opens a chat in the same workspace (OBSERVED)
They see every session in the workspace (`list_sessions` has no owner filter) and can read messages/timeline
(`scoped_db_for_chat_session_view`), but every mutation — send, stop, fork, resume, rename — is 403 unless
`created_by == user` (`scoped.rs:175-204`). Their own new session is an independent job/task/VM. WS `chat:<id>` channel
authorization is workspace-scoped (`ws/channels.rs:318, 391` → `resolve_chat_session_context`, `scoped.rs:518-535`).

---

## D. Persistence between turns and recovery

### D.1 State needed to resume a chat (OBSERVED)
| state | where | writer/reader |
|---|---|---|
| conversation (user/assistant rows) | Postgres `chat_messages` | API; copied into `ChatTaskContext.conversation_history` on every dispatch (`task_context.rs:644-649`) |
| provider session id + settings | Postgres `chat_sessions.provider_state` (`claudeCode.providerSessionId`) | worker via `GET/PUT /api/internal/sessions/:id/provider-state` (`routes/internal.rs:3300`, handlers 10009/10027); read `worker/claude_code.rs:9475-9493` |
| Claude SDK transcript | Postgres `claude_session_store_entries` | sidecar `G5ApiSessionStore` → `/api/internal/chat-sessions/:id/session-store/{append,load,sessions,entries,subkeys}` (`routes/internal.rs:3307-3327`; Codex artifacts at 3334-3339); in the no-identity guest through the host-side loopback proxy (`worker/g5_api_session_store_proxy.rs:1-45`); access requires a running owned task **or** an active/reaping sticky row (`routes/internal.rs:1766-1820`) |
| Codex rollouts | Postgres `codex_session_artifacts` (BYTEA) | worker upserts after a turn and restores before `codex exec resume <id>` if local `CODEX_HOME/sessions/**` misses (`worker/codex.rs:3666-3682`, preflight 4881-4893) |
| agent trace | Postgres `timeline_entries` (per task) | `POST /api/internal/tasks/:id/timeline`; served by `GET /sessions/:id/timeline` |
| cross-session context | `chat_session_context_links` + receipts | `chat_context_service.rs` |
| open questions/approvals | `task_question_windows`, `agent_approval_handoffs`, sticky pending-question columns | `task_question_window.rs`, `chat_worker_loss.rs:642` |
| attachments | `chat_attachments` (+ blob) | `create_attachment`, carried on `ChatTaskContext.attachments` |
| live process state | worker memory (`HeldClaudeSidecar`) + host `ParkedRuntime` (VM) | only while a sticky row is `active`; never persisted |
| working tree | guest `/work` per task | not persisted across cold turns except through the CRDT/draft bridge (INFERENCE, see B.3) |

### D.2 Resume after the worker/VM is gone (OBSERVED)
1. Loss detection: worker terminate/heartbeat-stale → `mark_lost_for_workers` (`task_service.rs:16865`; reaper
   `lease_reaper.rs:586`) and `terminalize_chat_task_for_worker_loss_prelocked` (`chat_worker_loss.rs:912-1010`): errors
   `pending/streaming` assistant rows, appends a replayable `provided_agent.worker_unavailable` timeline entry, cancels
   pending question approvals, and returns the task to **`waiting`** "without generic task-failure accounting"
   (module header lines 1-6). Reasons: `GracefulTerminate|LeaseExpired|MaxDurationExceeded|AttachedPreviewLost` (83-97).
2. A `lost` runtime "terminalizes nothing — the task stays `waiting` and the next send takes the #786 cold-recovery path"
   (`models/chat_sticky_runtime.rs:31-34`).
3. Next send: the classifier no longer sees an `active` runtime, so it returns `OriginalWorkerEligible` /
   `ReplacementEligible` (handoff clears the stale `required_worker_id`, `chat_service.rs:15449`) / `Blocked`; the
   `waiting` task is reactivated to `pending` (`reactivate_dormant_task_prelocked`) and any eligible worker claims it cold.
   The cold turn gets the full `conversation_history`, and the provider resumes by session id from the mirrored transcript
   (Claude) or restored rollouts (Codex); if the store has no rows the SDK takes its fresh-session path
   (`session_store_service.rs:102-105`).
4. Stranded `running` tasks (terminal assistant, no live provider owner within 300 s) are reconciled to `waiting` on send
   (`reconcile_stranded_running_chat_task_prelocked`, `chat_service.rs:13235`, classifier 13007) and by the reaper.
5. If the lost runtime left an unresolved teardown fence, the task sits in `recovering`/`recovery_blocked` until the
   worker/host produces the release receipt (`migrations/269-273`; `lease_reaper.rs:1769-1790`).

---

## E. Context links / `chat_context_service.rs` (OBSERVED)
4 700 lines (tests from 3522). Implements cross-session "context links": a target chat attaches other sessions as
`snapshot` (frozen cut) or `live` (delta per turn) sources; per-turn resolution writes immutable receipts and injects
`external_chat_contexts` into the `ChatTaskContext` (`chat_context_service.rs:1-30, 1508 resolve_receipts_for_turn`).
Caps: 5 links/target, 200 messages, 64 KiB/link, 128 KiB/turn (56-80). A per-target advisory lock serializes link config
against resolution (91-125). It runs inside the dispatch transaction on send, wake, steer, warm and cold paths and
"must fail closed" (`chat_service.rs:14098-14112`). Surfaces: 4 REST routes, WS `chat.context_updated`, internal
`/tasks/:id/chat-context/read` (`routes/internal.rs:3199`), CLI `g5d chat context …`
(`/Users/zliu/Workspace/G5-Lean/backend/g5d-cli/src/chat.rs:1-16`, 711 lines), frontend `ChatContextLinks.tsx` (1 053 lines).
**INFERENCE:** not essential to the chat loop — nothing in send/resume/recovery needs a link to exist; removing it deletes
two tables, one service, one WS event, four routes, one CLI verb and one component, and simplifies `send_message`'s
transaction (the resolver + savepoint logic in `resume_chat_task_on_live_runtime_prelocked`).

---

## F. Streaming (OBSERVED)
- Worker → API (internal, worker JWT): `POST /messages/:id/provider-accepted` (first valid frame; also renews sticky
  liveness), `POST /messages/:id/complete`, `POST /messages/:id/error`, `POST /sessions/:id/complete-pending`,
  `POST /sessions/:id/user-message-content`, `POST /tasks/:id/timeline` (`routes/internal.rs:3275-3295, 3239`).
  Message status updates call `update_message_status_scoped` → `publish_message_status_update` (`chat_service.rs:11179-11240`).
- API → browser: WS hub with `Channel::Chat(session_id)` = `chat:<uuid>` (`ws/channels.rs:15, 47-71`); events
  `chat.chunk {sessionId,messageId,delta}`, `chat.message_updated`, `chat.done {usage,citations}`, `chat.error {error,message}`,
  `chat.context_updated` (`ws/events.rs:186-250`); live trace as `task.timeline {taskId, entry}` published to the task
  channel **and** the chat channel when the entry metadata carries `sessionId` (`task_service.rs:23604-23620`).
  Multi-instance fan-out via Redis pub/sub `EventBus` (`ws/mod.rs:11-13`).
- Browser handling: `frontend/src/lib/ws.ts:1034-1075` (optimistic append on `chat.chunk`, invalidate on `chat.done`/
  `chat.error`); `WorkspacePage.tsx:8071-8114` consumes `task.timeline` for the active chat.
- Browser → running agent (steer/approval/stop): API pushes `InboxItemEnvelope::{Message, Control, ApprovalDecision}`
  (`ws/worker_hub.rs:82-136`) to the worker over `GET /api/internal/workers/:id/inbox-ws` (`routes/internal.rs:3423`)
  or Redis `session:{id}:inbox` (`worker/inbox.rs:1-4`); the parked keeper drains the same inbox (`KeeperInbox`,
  `worker/runtime_keeper.rs:33-93`). Control actions: `pause|resume|abort|checkpoint|set_permission_mode|set_model|set_effort|teardown_sticky`
  (`worker/inbox.rs:42-58`).

---

## G. Size (OBSERVED line counts, `wc -l`; "non-test" = lines before the first `#[cfg(test)] mod tests`)

| file | lines | non-test |
|---|---|---|
| `chat_service.rs` | 20 438 | ~17 745 |
| `sticky_runtime.rs` | 8 073 | ~7 873 |
| `chat_context_service.rs` | 4 700 | ~3 522 |
| `chat_worker_loss.rs` | 1 346 | ~1 200 |
| `session_store_service.rs` + `codex_session_artifact_service.rs` | 267 + 358 | 267 + 304 |
| `task_question_window.rs` | 928 | — |
| `routes/chat.rs` | 5 264 | ~5 004 |
| `routes/internal.rs` (whole file; chat/session-store/sticky handlers are a subset) | 15 051 | — |
| `worker/runtime_keeper.rs` (registry) + `g5d/worker/runtime_keeper.rs` (park loop) | 1 746 + 3 274 | — + ~1 355 |
| `worker/g5_api_session_store_proxy.rs` | 1 215 | 612 |
| `worker/inbox.rs` | 959 | 488 |
| `worker/claude_code.rs` / `worker/codex.rs` | 22 983 / 14 915 | (309 / 161 lines match sticky|keeper|park|warm) |
| `host/supervisor.rs` | 29 684 | (979 lines match sticky|parked|teardown_generation…; 311 mention CRDT) |
| `task_service.rs` | 33 023 | (405 sticky-matching lines) |
| `g5d/src/worker/mod.rs` | 19 397 | (240 sticky-matching lines) |
| `lease_reaper.rs` | 2 273 | (60 sticky-matching lines) |
| frontend `ChatPanel.tsx` / `useChat.ts` / `ChatContextLinks.tsx` / `ChatMessage.tsx` | 7 882 / 1 479 / 1 053 / 756 | — |

Schema that exists **only** for sticky runtimes: table `chat_sticky_runtimes` (≈40 columns, 10 indexes),
`workspace_settings.chat_sticky_idle_timeout_min`, `workers.worker_session_generation`, `workers.host_discard_fence_receipt`,
`chat_sticky_runtimes`-only SQL functions/trigger (247/273), `tasks.status` values `recovering`/`recovery_blocked` (270).
27 migration files mention `sticky`. DTO surface: `StickyRuntime*V1` types, 9 heartbeat rider fields, 3 transition fields,
2 ACK fields, `POST …/sticky-runtime/:runtime_id/decline` (`routes/internal.rs:3133`).
Schema that exists only for transcript mirroring: `claude_session_store_entries`, `codex_session_artifacts`.

---

## Coupling (what the chat loop is wired to today)

1. **Chat ⇄ task/job scheduler.** Every turn is a `tasks` row inside a `jobs` row; status words (`pending/claimed/running/
   waiting/recovering/…`) drive dispatch (`routes/chat.rs:3000-3480`), and the execution-lifecycle kernel commands
   (`Park`, `RepairChatTurn`, `ChatWorkerLoss`, `BeginTeardown`, `RuntimeLost`, …; `execution_lifecycle/command.rs:179-215`)
   are shared with non-chat tasks.
2. **Chat ⇄ worker identity.** `required_worker_id` pin, `worker_session_generation`, host admission guards and
   `ChatWorkerRoute` classification (`chat_service.rs:4795-5545`) exist because a live runtime lives on exactly one worker.
3. **Sticky runtime ⇄ everything.** `task_service.rs` ACK/transition/poll/claim, `routes/internal.rs` heartbeat, the lease
   reaper, `workspace_ops` draft discard, the host supervisor (VM park/evict), the worker keeper and both provider adapters
   all carry sticky-specific branches (counts in G). The heartbeat and transition DTOs are the wire contract.
4. **Sticky teardown ⇄ CRDT.** Teardown scopes are `{workspaceBranchRepoId, crdtSessionId}` and ACK dispositions are about
   durable CRDT flush (`migrations/211`, `sticky_runtime.rs:63 CRDT mentions`, `host/supervisor.rs` `ParkedRuntime` owns the
   `CrdtPeer`). Removing CRDT removes the reason for `teardown_repository_scopes`/`durablyFlushed`.
5. **Provider transcript ⇄ Postgres.** Because the VM is per-task and discarded, the Claude JSONL and Codex rollouts are
   mirrored through internal routes, a host-side proxy, and per-session authorization (`routes/internal.rs:1766`).
6. **Context links ⇄ dispatch transaction** (E).
7. **Streaming ⇄ task timeline.** The chat UI's live trace is `task.timeline` keyed by `task_id` with a `sessionId` in
   metadata (`task_service.rs:23604-23620`).

## Simplification candidates (for "one VM per workspace, tasks are local processes, persist state between turns, concurrent chats, no forced fork, no CRDT")

What the target model **removes** (with what it must keep in its place):

| remove | because | must keep / replace with | risk |
|---|---|---|---|
| `chat_sticky_runtimes` and the whole hold/park/teardown/release/retirement/claim-fence machinery (B.4/B.5), the 27 sticky migrations, `authz.*sticky*` functions + trigger, heartbeat/transition/ACK sticky riders, `RuntimeKeeper` park loop, host `ParkedRuntime`, pressure eviction, busy deferral, background wakes via heartbeat | it exists only to keep a per-task microVM + sidecar alive across turns and to prove its workspace was reclaimed before a cold successor may reuse the task path | idle chats persist their state (D.1 table) and *no* process is required; a workspace VM's dispatcher spawns a provider process per turn (or keeps a lightweight per-chat process at its discretion, locally, with no server-side fencing) | background subagents running while the user is idle (the `wake` feature) need a replacement: either the dispatcher keeps the process and reports completions, or background work is disallowed while idle |
| `required_worker_id` pin, `ChatWorkerRoute`, replacement handoff, `worker_session_generation` for chats | with one VM per workspace the "worker" is the workspace VM; there is nothing to pin to | route by `workspace_id`; keep a liveness check on the workspace VM and a `Blocked` reason when it is down | multi-host placement of one workspace still needs *one* owner; keep a simple lease on the workspace VM |
| `claude_session_store_entries` mirror + host-side session-store proxy; `codex_session_artifacts` | the VM/disk is per workspace and persistent, so `~/.claude/projects/**` and `CODEX_HOME/sessions/**` survive between turns | still need durable backup of the provider transcript if the VM disk is not durable (snapshot/`git`-tracked internal store per goal 3); keep `provider_state.providerSessionId` | if the VM is rebuilt, transcripts are lost unless backed up; decide durability of the workspace disk explicitly |
| `chat_worker_loss` cold-recovery dance, stranded classifier, `recovering`/`recovery_blocked`, task-workspace recovery | loss of a per-task VM mid-turn no longer strands a workspace-keyed checkout | a much smaller "turn failed, mark assistant row error, next send starts a new turn" path | keep the invariant that a turn's partial output is preserved and pending questions are cancelled |
| teardown repository scopes / CRDT flush dispositions | no CRDT | git working tree in the workspace VM is the state; commit/stash semantics owned by the VM | concurrent chats editing the same tree need a policy (see open questions) |
| one job+task per chat turn (`create_chat_job`) | the task abstraction adds a job row, lease, ACK, and poll for what is an RPC to a local dispatcher | a `chat_turns` (or reuse `chat_messages` status) record + an RPC to the workspace dispatcher; keep `lock_session_turn_allocation` semantics (one pending assistant per session) | task-scoped sessions (`session_type='task'`, merge/sync workflows) still need a task; keep that path separate |
| context links (E) | optional feature with heavy transactional coupling | none, or re-add later as a pure prompt-assembly step | product decision |

What must be **kept** regardless of runtime model: `chat_sessions`/`chat_messages` as the authoritative transcript;
`provider_state` (provider session id, settings); per-session turn serialization; owner-vs-viewer authorization
(`created_by`); WS `chat:<id>` events (`chat.chunk/done/error/message_updated`) and the timeline stream; approvals /
questions with timeouts (`task_question_windows` semantics, even if re-keyed by session); attachments; fork-at-index
(cheap: it is a row copy) — and, if a second user should *send* in someone else's session, the owner-only guard must be
relaxed deliberately.

## Open questions (for the user)

1. **Concurrent chats on one tree.** Two chats in one workspace VM sharing one git working tree: do they share the same
   checkout (last-writer-wins, agent conflicts visible in `git status`), or does the dispatcher give each chat its own
   worktree/branch and merge (which re-introduces a fork of sorts)? Today each chat gets its own VM copy of the head.
2. **Background work while idle.** Should an idle chat be allowed to have subagents/bash still running (today's
   `busy`/`wake` feature)? If yes, "idle chat needs no process" is not strictly true and the dispatcher must own that process.
3. **Provider transcript durability.** Is the workspace VM disk durable enough to drop the Postgres transcript mirrors, or
   should the internal git persistence (goal 3) also snapshot `~/.claude/projects` / `CODEX_HOME/sessions`?
4. **Shared chats.** Should non-owners be able to *send* into a session (today: read-only for non-owners, 403 on mutation)?
   If so, what is the identity of a turn's author (`chat_messages` has no author column today)?
5. **`branch_mode = job_branch`.** No runtime consumer found; confirm it can be dropped together with `chat_sessions.branch`.
6. **Task-scoped sessions.** Sync/merge task dialogs (`session_type='task'`, `continue_in_merged_workspace`,
   `completed_sync` resume) are entangled with jobs; keep them on the task model or fold them into plain chats?
7. **Preview chat-attach** (`attach_preview_task_id`, `attach_vm`): keep as a feature of the workspace VM (serve preview +
   chat from the same VM would be natural) or drop?
8. **Approval/question timeouts** currently piggy-back on the sticky idle timeout (`chat_sticky_idle_timeout_min`).
   What owns question expiry when there is no sticky row?
