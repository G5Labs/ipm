# G5-Lean research: job/task lifecycle, scheduling, assignment, recovery

Scope: `/Users/zliu/Workspace/G5-Lean` (read-only). All paths below are relative to `backend/` unless noted. "OBSERVED" = read in code/SQL at the cited line; "INFERENCE" = my interpretation, not verified end-to-end.

---

## A. Data model

### A.1 Tables and lifecycle-carrying columns (OBSERVED)

**`jobs`** (`g5-api/src/db/migrations/001_initial_schema.sql:382-431`, model `g5-api-data/src/models/job.rs:24-95`)
- Lifecycle: `status` (TEXT CHECK), `job_type` (TEXT CHECK), `pause_reason` (031), `started_at`, `completed_at`, `ended_by` (159: user-initiated terminations only), `depends_on_job` (lane ordering), `total_tasks` / `completed_tasks` / `failed_tasks` (denormalized counters).
- Branch lifecycle (014): `branch_name` (`job/<job_id>`, `job_service.rs:3211,3499`), `base_branch`, `start_commit` (046), `head_commit` (documented as legacy single-repo cursor, `job.rs:70-74`).
- Review checkpoints (026): `tasks_since_checkpoint`, `checkpoint_count`, `review_every_k`, `active_checkpoint_id`, `final_review_done`.
- Typed context blobs: `sync_context` (068), `agent_launch` (190), `approval_context` (218, immutable via trigger), `orchestration_context` (229: `JobOrchestrationContextV1 { capability_grants.create_tasks, task_triggers[] }`, `job.rs:107-160`), `golden_builder_context` (289), `mode`, `target_doc_id`.
- Idempotency: `job_submission_receipts` (229) for AQL admission.

**`tasks`** (`001:433-474`, model `job.rs:172-259`)
- Lifecycle: `status`, `task_type`, `worker_id`, `assigned_at`, `retry_count`, `max_retries` (DB default 2 after 047; Rust `DEFAULT_TASK_MAX_RETRIES=2`, `BOOTSTRAP_TASK_MAX_RETRIES=0`, `UPLIFT_SELF_BAIL_FRESH_RETRIES=4` at `job.rs:7-15`), `depends_on UUID[]`, `pruned` (024), `is_checkpoint` (026).
- Lease (039): `lease_expires_at`, `lease_renewals`, `max_duration_secs`.
- Attempt fence (239): `active_execution_attempt_id` (FK to `task_execution_attempts`).
- Policy (225/289): `async_decision_policy` (`interactive_v1` | `autonomous_v1`, stamped by BEFORE INSERT trigger).
- Git/context: `branch_name`, `base_commit`, `workspace_ref` (TEXT, frequently parsed as JSON: `requiredWorkerId`, `sessionId`, `syncHumanInputPolicy`, `aqlBlockingWait`, `explainerContext`), spec columns (005), `task_observations` JSONB (084), encrypted `agent_auth_*` (190, cleared by trigger on terminal status), `agent_execution_receipt`.

**`task_execution_attempts`** (239, `job.rs:230-247`): `attempt_number`, `worker_id`, `provider`, `provider_session_id`, `continuation_summary`, `disposition` (5 values), `reason` (15 values), `retry_policy` (3 values), `started_at`/`ended_at`, `close_evidence` JSONB (298). Unique open attempt per task (`uq_task_execution_attempts_one_open_per_task`).

**`workers`** (`001:631-660`, `worker.rs`): `status` CHECK (`provisioning|idle|assigned|executing|terminated`), `task_id`, `last_heartbeat`, `last_poll_at`, `capabilities` JSONB (042), `max_concurrent_tasks`, `preferred` (126), `worker_session_generation` (241), `host_id`/`external_host_id` (079/222).

**`chat_sticky_runtimes`** (136, 147-248, 269-273): `status` (`active|reaping|expired|lost`), `task_id`, `worker_id`, `host_id`, `microvm_id`, `idle_since`/`expires_at`, `last_liveness_at`, `teardown_*` (generation, operation_id, deadline, reconciliation_kind `registration_retirement|task_workspace_recovery`, receipt, `workspace_released_at`), `worker_session_generation`.

Sibling lifecycle tables: `task_question_windows` (167), `agent_approval_handoffs` (162), `human_escalations` + `human_escalation_questions` (030/137), `task_commit_requests` (161), `task_commit_frontier_cuts`/`_receipts` (279/280), `job_merge_runs`/`job_merge_artifacts` (074), `runtime_attribution_events` (179), `task_completion_constraints` (193), `builder_task_specs` (288), `workspace_head_operations` (196), `job_preview_sessions` (096), `timeline_entries`, `task_checkpoints` (001).

### A.2 "Persistent task", "task class", "job context" (OBSERVED)
- **Persistent task** = a task-scoped CRDT draft session: `draft_sessions.task_id` (152), unique `(workspace_branch_repo_id, task_id)` (154); `crdt/mod.rs:310` `persistent_task = is_task_scoped && self.config.task_persistence_enabled`. Also `workspace_branches.authority_scope_kind = 'task'` rows exist (`task_service.rs:2555`, fresh-retry reseed check).
- **Task class** is not a type in code; it appears only in comments, always meaning `golden_builder` (`job_service.rs:314`, `builder_spec_service.rs:4`) or a category ("merge/rebase task — the only task class that writes git objects", `g5d-host/src/supervisor.rs:5990`).
- **Job context**: `resolve_job_context()` route helper returns the job's `workspace_id` (`g5-api-routes/src/routes/jobs.rs:34,2752`); otherwise "context" means the typed JSONB columns listed above.

### A.3 Job types (OBSERVED)
DB CHECK after 288: `compile, uplift, downlift, test, review, approval_review, chat, sync, merge, view_create, preview, explainer_generate, golden_builder, custom` (14). Rust `JobType` (`g5-contracts/src/types/enums.rs:111-151`) matches; `ViewCreate` is legacy (POST /jobs answers 410). `BATCH_TYPES = compile|uplift|downlift|test|custom`; `INTERACTIVE_TYPES = chat|review|approval_review|sync|merge|preview|explainer_generate|golden_builder` (`job_service.rs:296-308`). Interactive types use the "one-task-per-job bootstrap lane" (migration 076/095 comments). `preview` and `golden_builder` are host-claimed, no agent (`enums.rs:54-67`).

### A.4 Task types (OBSERVED)
DB CHECK after 288: `bootstrap, regular, chat, sync, merge, preview, explainer_generate, golden_builder` (8). Rust `TaskType` (`enums.rs:47-68`) is a *different* vocabulary: `Uplift, Downlift, Review, Chat, Sync, Merge, Preview, GoldenBuilder`. Wire `TaskAssignmentType` is only `Bootstrap | Regular` (`task_context.rs:3431`). INFERENCE: three partially overlapping task-type vocabularies (DB column, contracts enum, assignment DTO) coexist; the DB column is what scheduling SQL filters on.

---

## B. State machines

### B.1 Task status (OBSERVED)
DB CHECK after 270: `pending, running, paused, completed, failed, blocked, cancelled, rolled_back, waiting, claimed, recovering, recovery_blocked` (12). Rust `TaskStatus` (`enums.rs:80-99`) has 12 variants but **no `Waiting`** and an extra **`Merged`** that the DB CHECK does not allow. `waiting` is handled as a raw string (57 occurrences in `task_service.rs`); the frontend `TaskDTO.status` is an untyped `string` (`frontend/src/lib/types.ts:2012`).

### B.2 Job status (OBSERVED)
DB CHECK after 074: `pending, bootstrapping, running, paused, completed, blocked, failed, cancelled, rolled_back, merged` (10). Rust `JobStatus` (`enums.rs:163-174`) matches.

### B.3 The kernel (OBSERVED)
Since issue #2402 all posture writes go through one private kernel: `g5-api-core/src/services/execution_lifecycle/` (`mod.rs:1-5`: "apply_on_connection is the only posture writer... There is no public set_status"; `inventory.rs:22` `WRITERS = &[]`).
- `LifecycleCommand` (`command.rs`): 33 variants in 7 families: Admission (`Claim, RejectClaim, Ack`), AttemptOutcome (`Complete, Retry, Park, Fail, RetireTombstonedTask, PreviewServeFailure, SettleAttempt, RepairChatTurn`), HumanControl (`Stop, Cancel, Pause, Resume, HumanRetry, ResumeCompletedSync, Block, Rollback`), RuntimeControl (`BeginTeardown, AcceptTeardown, ReleaseWorkspace, RuntimeLost`), Recovery (`WorkerLoss, ChatWorkerLoss, LeaseExpiry, ProviderStall, RecoveryBlocked, DispatchRecovered`), `JobRollup(JobRollupKind)` (19 kinds), `Liveness`.
- `PhaseIdentity` (`identity.rs:11-22`): `task_id, attempt_id, worker_session_generation, chat_turn_id, chat_message_id, runtime_id, runtime_generation, operation_id, job_id, session_id`. Stale identity is rejected, never coerced (`predict.rs` `reject_stale_identity`; `error.rs` `LifecycleReject`).
- `predict()` (`predict.rs`) computes `TransitionReceipt{before, after, evidence, events, noop}`; `persist_receipt()` (`persist.rs:692-862`) writes `UPDATE tasks SET status=$2, active_execution_attempt_id=$3` (719), closes the open `task_execution_attempts` row with disposition/reason/retry_policy/close_evidence (782), writes `UPDATE jobs SET status` for JobRollup (809), and bulk-cancels children for `CancelRemainingChildren` (830). Ack mints the attempt row (`persist_ack_attempt`, 886/912).
- Locking (`lock_plan.rs`): three plans (`task-first: task→attempt→runtime→session`, `session-first`, `job-first`) over `pg_advisory_xact_lock` namespaces 24021-24025.
- `apply_lifecycle` call sites: 142 non-test (task_service 74, chat_service 25, job_service 7, job_merge_service 7, lease_reaper 6, ...).

### B.4 Legal task arcs (OBSERVED, `predict.rs::legal_task_arcs`, 44 arcs) and the command that performs each

```
                       Claim (worker poll/claim; API: task_service 8543,10779,12217)
   pending ─────────────────────────────────────────────► claimed
   waiting ────────────────────────────────────────────► claimed   (chat warm re-delivery)
   claimed ──Ack (POST .../ack; task_service 12622,12880)──► running   [mints task_execution_attempt]
   claimed ──RejectClaim (nack) / LeaseExpiry (reaper)──► pending (if workspace released) | recovering
   running ──Complete──► completed        running ──Fail / PreviewServeFailure──► failed
   running ──Block──► blocked             running ──Park──► waiting   (chat turn end / question)
   running ──Pause──► paused              running ──Stop/Cancel──► cancelled
   running ──WorkerLoss/LeaseExpiry/ProviderStall──► pending | recovering | blocked(manual)
   running ──ChatWorkerLoss──► waiting    (chat only; job stays reusable)
   paused  ──Resume──► running (live attempt) | pending | recovering
   failed/blocked/cancelled/waiting/pending ──Retry / HumanRetry──► pending | recovering
   completed ──ResumeCompletedSync──► pending | recovering   (sync jobs only)
   recovering ──DispatchRecovered (watchdog)──► pending      recovering ──RecoveryBlocked──► recovery_blocked
   recovery_blocked ──DispatchRecovered──► pending
   any non-terminal ──Cancel (API/rollup)──► cancelled       pending/paused/blocked/waiting ──RetireTombstonedTask──► failed
   pending/claimed/paused/waiting/blocked/failed/recovering/recovery_blocked ──Rollback──► rolled_back
```
Invariant: `after == pending && !workspace_released` is rejected (`WorkspaceUnreleased`, `predict.rs` tail) — a task cannot become schedulable while a sticky runtime still holds its workspace. `rolled_back`, `completed`, `cancelled` are terminal for recovery (`is_terminal_task_or_attempt`).

### B.5 Job transitions (OBSERVED)
```
create ──► bootstrapping (started_at=now)  |  pending if depends_on_job predecessor not terminal (job_service.rs:4335-4356)
pending ──LaneSuccessorRelease──► bootstrapping | running (job_service.rs:8654; predict Lane... )
bootstrapping ──Resume rollup in submit_task_graph──► running (job_service.rs:6847)
running ──Pause/TaskDialogPause/SpendLimitPause/GlobalEscalationPause/ManualRecoveryPause──► paused
paused ──Resume──► running
running ──ChildTerminal──► completed | failed | cancelled  (derive_job_status_from_children: any failed→failed, any cancelled→cancelled)
running ──Blocked──► blocked   (sync jobs park at manual merge boundary, task_service.rs:21803)
running ──Stalled / MergePublicationFailed / PostCompletionFailed──► failed
* ──CancelRemainingChildren──► cancelled (+ bulk child cancel)     * ──Rollback──► rolled_back     * ──Merged──► merged
```
Job completion is decided in `task_service.rs:21790-21850`: `failed_tasks>0` → failed; `job_type=='sync'` → blocked; merge job with blocked result → failed; else completed; remaining children cancelled; `completed_at=now()`. A second guard fires `ChildFailed` when `completed_tasks+failed_tasks >= total_tasks` (22280-22300).

### B.6 Who performs transitions (OBSERVED)
- **API (worker-facing internal routes)**: claim/ack/nack, `/tasks/:id/transition` (`internal.rs:8516` → `run_worker_task_transition_with_checkpoint` 2639 → `transition_task_with_rollovers_deferred` `task_service.rs:20327`), heartbeat.
- **API (user routes)**: `tasks.rs:46-75` pause/resume/abort/retry/complete/force-complete/rollback; `jobs.rs:1043-1090` pause/resume/cancel/rollback/merge.
- **Maintenance loops**: lease_reaper (LeaseExpiry/WorkerLoss/RecoveryBlocked/Cancel/BeginTeardown/JobRollup), preview_reaper, workspace_operation_resolver.
- **DB triggers on `tasks`**: `trg_tasks_set_org` (012), `tasks_stamp_async_decision_policy` (225/289), `trg_clear_terminal_task_agent_auth` (190: nulls `agent_auth_*` when status becomes completed/failed/cancelled/rolled_back), `trg_tasks_guard_sticky_execution_admission` (273: raises SQLSTATE 40001 on first `claimed`/`running`/worker_id/lease/attempt acquisition while a same-task sticky runtime is `reaping` or has an unreleased teardown). On `jobs`: `trg_jobs_set_org`, `trg_jobs_approval_context_immutable` (218). On `workers`: `trg_workers_guard_revival_dead_host`, `trg_workers_guard_insert_dead_host` (276). SECURITY DEFINER `authz.retire_prelocked_prior_generation_sticky_runtimes` (273) moves fenced pending tasks to `recovering` during same-id worker re-registration. No trigger performs ordinary status transitions.
- **Worker (g5d)**: never writes status; it reports outcomes (`build_transition_request`, `g5d-worker/src/lifecycle.rs:466`: `Completed→"completed"`, `Failed|ProviderInterrupted|BudgetExhausted→"failed"`, `Blocked→"blocked"`, `Waiting|WaitingForChildren→"waiting"`) plus `execution_end` evidence.

---

## C. Assignment

### C.1 How a worker gets a task (OBSERVED)
1. Worker registers: `POST /api/internal/workers/register` → row `status='idle'`, `worker_session_generation` bumped on same-id re-registration (`task_service.rs:8554`, `9017`, `9125`), guarded by `authz.worker_identity_bound_to_terminated_host` (`task_service.rs:8967`; migrations 275/276). Server returns `heartbeat_interval_secs = 30` (`task_service.rs:8572`).
2. Worker polls: `POST /workers/:id/poll` body `{mode: Any|ZeroSlotOnly}` (`dtos.rs:769-786`) → `poll_for_task_with_mode` (`task_service.rs:9640`). Eligibility SQL (`9700-9800`, mirrored in the claim CTE 10380-10460): `t.status='pending'`, job `bootstrapping|running`, `job_type = ANY(allowed_job_types)` derived from worker `capabilities` (`classify_worker_capabilities` 7684: `chat→chat`, `uplift→uplift`, `downlift→downlift,compile,test,review,custom`, `explainer_create→explainer_generate`, `approval_review:vN→approval_review`, ...), dependency gate (`NOT EXISTS dep.status != 'completed'`), `max_concurrency` per job, worker slot cap, `requiredWorkerId` pin, agent-provider capability, `async_autonomy_v1`, `sync_interactive_question_v1`, approval-review schema capability, workspace `mutation_policy` read-only/freezing gate, retiring-runtime fence, claimable-attempt fence, advisory predecessor fence; ordered `j.priority DESC, t.created_at ASC LIMIT 1 FOR UPDATE OF j SKIP LOCKED`.
3. Claim: the CTE `UPDATE tasks SET worker_id=$1, lease_expires_at=now()+INTERVAL '30 seconds'` (10418-10425) followed by `apply_lifecycle(cmd_claim)` → `claimed` (10779). Direct claim also exists: `POST /workers/:id/tasks/:task_id/claim` (`claim_task_for_worker` 12217).
4. Response is a `TaskAssignment` (`task_context.rs:3659`: task/job ids, `execution_attempt_id`, branch_name, base_commit, workspace_git_url, repo_paths, `task_crdt_head_commits`, chat/sync/merge/explainer/approval contexts, guardrails, scm credentials...).
5. ACK: `POST /workers/:id/tasks/:task_id/ack` → `cmd_ack` mints `task_execution_attempts` row and sets `running`; then `UPDATE tasks SET assigned_at, max_duration_secs=$2, lease_expires_at=now()+make_interval(secs=>$3)` (12897-12906) with lease 60s (chat) / 90s (others) (`assignment_lease_secs` 5487) and `max_duration_secs` 43,200s for agent tasks, 7,200s golden_builder, `i32::MAX` preview (5442-5447). NACK → `cmd_reject_claim` (13155).
6. Push channels: worker inbox WebSocket `GET /workers/:id/inbox-ws` + dead-man poll `/sessions/:id/inbox` (`internal.rs:12506`, `2016`); heartbeat response carries `cancelled_task_ids`, `teardown_instructions`, `workspace_reconciliation_instructions` (`dtos.rs:1466-1541`). Chat follow-ups are re-delivered "warm" to a held sticky runtime (`held_runtime_id`, `WARM_RESUME_PRE_ACK_CLAIM_LEASE_SECS=30`, `task_timing.rs:7`).

### C.2 Timers (OBSERVED)
| Item | Value | Where |
|---|---|---|
| Worker poll interval | 3s default + ±200ms jitter | `g5d-cli/src/lib.rs:707`, `host_up.rs:390`, `g5d-worker/src/poll.rs:250-260` |
| Heartbeat interval | 30s (server-issued) | `task_service.rs:8572` |
| Claim lease (pre-ACK) | 30s | `task_service.rs:10418` |
| Execution lease | 60s chat / 90s other, renewed by heartbeat only while `status='running'` and not already expired | `task_service.rs:16453-16475` |
| Reaper sweep | 30s | `lease_reaper.rs:47` |
| Dead worker threshold | 270s (3×90) no heartbeat, `idle|assigned` → `terminated` | `lease_reaper.rs:50,505` |
| Max duration | 43,200s agent; 7,200s golden; ∞ preview | `task_service.rs:5442` |
| Task-workspace recovery grace | 600s | `task_service.rs:28759` |
| Sticky runtime liveness stale | 30s | `sticky_runtime.rs:5563` |
| Sticky idle window | 60 min default (0-240) | migration 136 |

### C.3 Retry policy (OBSERVED)
- `max_retries` default 2 (DB) — bootstrap tasks 0.
- A worker-reported `failed` transition goes straight to `LifecycleCommand::Fail` → `failed` (`task_service.rs:21055-21080`), increments `jobs.failed_tasks`, and cascade-blocks pending/blocked dependents (`collect_cascade_dependency_failure_statuses` 27833). The "fresh session retry / uplift self-bail" path is dead: `fresh_retry_mode()` is stubbed to return `None` (`task_service.rs:12116-12123`). So no automatic retry on agent-reported failure; `suppress_retry` is still sent by the worker when a commit was published (`lifecycle.rs:526-535`).
- Automatic retry exists only in the lease reaper for lost/expired running tasks: `manual = max_duration_exceeded || retry_count >= max_retries`; otherwise `cmd_worker_loss`/`cmd_lease_expiry` with `Automatic` → `pending` (or `recovering` if workspace unreleased), `retry_count += 1` (`lease_reaper.rs:1040-1160`). No backoff delay; the task is immediately claimable.
- Human retry: `POST /tasks/:id/retry` from `failed|blocked|cancelled` (`task_service.rs:25774`) → `requeue_task_from_current_head` → `cmd_retry` (4034).

### C.4 `task_execution_attempts` — why (OBSERVED from migration 239 + `predict.rs`)
"durable execution-attempt evidence for stable logical tasks": the task row is the stable identity; each ACK mints an attempt (`persist_ack_attempt`); every close records disposition/reason/retry_policy and `close_evidence` (298). The `active_execution_attempt_id` pointer plus `worker_session_generation` are the fences that reject stale reports from a previous worker/attempt (`reject_stale_identity`). The reaper explicitly repairs "stale active-attempt pointers on expired claimed tasks (issue #2185)" (`lease_reaper.rs:775-790`).

---

## D. Recovery

### D.1 Lease reaper per-org sweep, every 30s (OBSERVED `lease_reaper.rs:605-1243`)
1. `sweep_blocking_aql_waiters_for_org` (417): `waiting` tasks whose `workspace_ref` contains `"aqlBlockingWait"` are reactivated when satisfied.
2. `cancel_claims_for_cancelled_jobs` (441): `claimed` tasks of cancelled jobs → `cmd_cancel`.
3. `reap_dead_workers_for_org` (505): `UPDATE workers SET status='terminated'` when `idle|assigned` and no heartbeat for 270s.
4. `mark_terminated_owner_runtimes_lost_for_org` (526): sticky runtimes on terminated workers → `lost`.
5. Expired `claimed` (lease 30s): teardown any active runtime, release the attempt with reason `state_corruption`, `cmd_lease_expiry` → `pending` (workspace released) or `recovering`; clear worker/lease/auth columns.
6. Expired `running` **chat** tasks: `terminalize_chat_task_for_worker_loss` → assistant rows errored, approvals cancelled, task → `waiting` (job stays reusable) — skipped if the session already has a fresh active runtime.
7. Expired/lost **non-chat** `running` tasks (lease expired OR max_duration OR worker terminated OR worker heartbeat > 270s old): `cmd_worker_loss`/`cmd_lease_expiry` with `Manual` if `max_duration_exceeded || retry_count >= max_retries` (→ `blocked` if released, else `recovering` → `cmd_recovery_blocked` → `recovery_blocked`), `Automatic` otherwise (→ `pending`/`recovering`); `retry_count += 1`; if no live work remains, `JobRollupKind::ManualRecoveryPause` → job `paused`.

Sticky-runtime sweep (`do_sticky_runtime_sweep_inner` 1879): `converge_task_workspace_recovery_for_org` (watchdog: `DispatchRecovered` → `pending` once the host releases the workspace, or `RecoveryBlocked` after the 600s grace, `task_service.rs:29200-29520`), `reap_idle_sticky_runtimes` (idle expiry → `cmd_begin_teardown`; pending/recovering tasks get `cmd_lease_expiry`, `sticky_runtime.rs:7424-7530`), pending-question runtimes, stale (`last_liveness_at` > 30s) → `lost`, acknowledged-teardown convergence. Question-window sweep (`sweep_task_question_windows_for_org` 2056) settles `idle_timeout` and issues the platform receipt.

### D.2 Worker loss vs host death vs API restart (OBSERVED + INFERENCE)
- Worker loss: detected by heartbeat staleness (270s) or `workers.status='terminated'` (worker `/terminate`, host `/host/terminate` cascade `internal.rs:4454`); handled by step 7 above. `ChatWorkerLoss` keeps chat jobs alive.
- Host death: `worker_hosts` lifecycle is separate (078-171, `worker_host_service.rs`); migration 275/276 forbid reviving a worker identity bound to a terminated host; `revival_trigger` names `worker_revival_dead_host`. INFERENCE: host death propagates to tasks only through worker heartbeat staleness and the runtime-loss path — I did not find a direct host→task transition.
- API restart: all state is in Postgres; maintenance loops are re-spawned by `MaintenanceRunner::spawn_all` (`maintenance/mod.rs:239`) under `pg_try_advisory_xact_lock` (`lease_reaper.rs:41`). INFERENCE: no in-memory scheduler state exists to lose; in-flight HTTP transitions are transactional.
- Orphaned microVM: `task_observations` (084) records exit status; `record_released_microvm_exit_observation` (`task_service.rs:15498`); sticky teardown `teardown_operation_id`/`deadline_at = +30s` and `task_workspace_recovery` reconciliation (269-271) carry the obligation through heartbeat `teardown_acks` / `workspace_reconciliation_receipts` (`dtos.rs:1237-1246`).
- `worker_session_generation` (241): monotonically increasing per registration; embedded in worker JWT (`auth_middleware.rs`), compared in `reject_stale_identity`, in the claim CTE (`claim_worker.worker_session_generation = $13`), and in sticky-runtime grants. Used 163× in task_service, 142× in sticky_runtime.
- `recovery_status` is not a column; the recovery statuses are `recovering` and `recovery_blocked` (270).

### D.3 "advance" / "advance projection" / "advance handoff" (OBSERVED)
An **advance** is one committed `workspace_head_operations` row (196: `status preparing|prepared|finalizing|committed|failed`; per-repo `transition_kind advance|initialize|reseed`, 212) that atomically moves workspace-branch heads. `workspace_finalizer.rs` prepares/finalizes it (`acquire_workspace_finalization_fence` 300, `finalize_with_task_crdt_rollovers` 1780, `OPERATION_LEASE_SECONDS=900` 57). **Advance projection** = post-commit derived state (git ref projection, search reindex, branch-integration enqueue) reconstructed by `advance_notifier.rs::reconcile` (277) from the operation id alone. **Advance handoff** = the sink-owned idempotency keys (`search_reindex_queue.advance_id`, `branch_integration_queue.advance_id`, `workspace_advance_adapter_receipts`, migration 198). `advance_consumer.rs` registers consumer inputs before authority application and can apply a bound **task lifecycle effect atomically with the authority receipt** (`attach_and_apply_committed_task_lifecycle_consumer` 1770). `workspace_operation_resolver` (3s git projection / 30s maintenance) repairs unprojected or expired operations.

---

## E. Completion and work recording (OBSERVED)
- Worker commits through `POST /tasks/:id/drafts/commit-all` (`commit_task_workspace_drafts` `internal.rs:9020`) with a `task_commit_requests` row (161: `pending|succeeded|failed`, PK `(org,task,request_id)`) as the replay fence; `task_commit_frontier_cuts` (279) freeze the first accepted CRDT cut per repo; `task_commit_frontier_receipts` (280, 10-minute TTL) reserve a snapshot under the document commit gate.
- Completion transition may carry `commit_results` (`TaskTransitionRequest`, `dtos.rs:2043-2123`), which forces the route-owned two-stage finalizer (`transition_task` rejects them, `task_service.rs:20273`): `prepare_publish_branch_state` / `finalize_publish_branch_state` (`internal.rs:7974/8011`), `task_completion_authority_preflight` (7920), then `finalize_authority_followup_with_task_crdt_rollovers` (2278) and `transition_task_with_rollovers_deferred`.
- Branches: **workspace branch** (`workspace_branches`, tracking/pinned); **job branch** `job/<job_id>` forked from `jobs.base_branch` at `start_commit` (014/046); **task branch** columns `tasks.branch_name`/`base_commit` and `TaskAssignment.branch_name` exist (001, `task_context.rs`), and task-scoped authority rows `workspace_branches.authority_scope_kind='task'` exist. INFERENCE: tasks commit onto the job branch with a base-commit CAS (`base_commit` field doc, `dtos.rs:2052-2058`); I did not trace per-task git branch creation.
- Job merge: `POST /jobs/:id/merge{/preflight,/direct-clean}` → `job_merge_runs` (`pending|running|waiting_for_human|rolling_back|completed|failed|cancelled`; phases `job_rebase_preflight`, `job_direct_clean_preflight`, `post_rebase_alignment`, `workspace_source_rebase_workflow`, `reset_below_baseline`) → job status `merged`; merge job publication is a `JobRollupKind::ZeroTaskMergePublicationComplete|MergePublicationFailed|RetireTombstonedMerge`.
- Counters: `completed_tasks`/`failed_tasks` incremented inline (`task_service.rs:20715,21063`); child tasks spawned from `child_tasks` payload (`spawn_child_tasks` 28002); `cascade_prune` auto-completes tasks whose deps were all pruned; review checkpoints reset `tasks_since_checkpoint`.
- `has_more_work` = `EXISTS pending task in job` (`task_service.rs:19677`).

---

## F. Human-in-the-loop (OBSERVED)
- **Escalations** (`human_escalation_service.rs`): `create_task_escalation` (584) inserts `human_escalations` (`open|in_review|resolved|cancelled`) and `block_task_for_escalation` → `cmd_block` → task `blocked`; global scope also `pause_job_for_global_escalation` (`JobRollupKind::GlobalEscalationPause`, `pause_reason='global_escalation'`). Resolution (`/escalations/:id/resolve`) → `orchestrate_post_resolution` (`internal.rs:14485`): `unblock_task_with_resume_message` → `requeue_task_from_current_head` → `cmd_retry` → `pending` (+ resume message pushed), and `resume_job` when scope is `job`.
- **Question windows** (`task_question_window.rs`, 167): one `open` window per sync task (`question_v1` policy); states `open|answered|idle_timeout|cancelled`; timeout settled by the reaper with a `platform_receipt_id`; a worker `waiting` transition for such a task must present `question_timeout_receipt_id` + `question_window_id` (`dtos.rs:2108-2120`), consumed once (`consume_timeout_receipt` 742). Chat/sync tasks with `interactive_v1` may ask; `autonomous_v1` tasks may not (`question_surface_open` 262).
- **`waiting`**: chat turn end (`Park`), question idle timeout, `WaitingForChildren`, AQL blocking waits, and chat worker loss all land here; the task stays alive and a new user message requeues it (`requeue_earliest_pending_chat_turn` 3743 → `pending`/warm `claimed`). Sticky runtime idle window keeps the VM alive between turns (136).
- **Approval handoffs** (`agent_approval_handoffs`, 162: `resolving|applied|failed|cancelled`): immutable decision persisted before transport delivery; redelivered after worker reconnect (`claim_resolving_agent_approval_handoffs` `chat_service.rs:17138`, `push_replayed_handoff_with_timeout` `internal.rs:12907`); cancelled on chat worker loss (`cancel_pending_question_approvals_for_session`). They do not change task status themselves (INFERENCE from absence of `apply_lifecycle` in that path).
- Job pause reasons: `global_escalation | spend_limit | user_initiated` (031); `JobRollupKind::TaskDialogPause|SpendLimitPause|GlobalEscalationPause|ManualRecoveryPause`.
- Worker-side control channel (`g5d-worker/src/inbox.rs:42-57`): `Pause, Resume, Abort, Checkpoint, SetPermissionMode, SetModel, SetEffort, TeardownSticky`.

---

## G. Maintenance loops (`g5-api-core/src/maintenance/mod.rs:239-684`) (OBSERVED)
| Loop | Interval | Purpose |
|---|---|---|
| approval_request_retention | 24h | delete terminal approval coordination rows |
| attribution_retention | 24h | prune `runtime_attribution_events` |
| agent_attachment_retention | 24h | prune agent screenshot attachments |
| crdt_compaction | 300s (`crdt_compaction_interval`) | compact Yjs state |
| draft_cleanup | min(retention/4, 1h); stale after 24h | stale draft sessions |
| draft_tombstone_reconciliation | one-shot | legacy tombstones |
| usage_aggregation | 300s | token/cost roll-ups |
| build_telemetry_aggregation | 300s | warm-cache hints |
| golden_builder_triggers | 60s (env-gated, default off) | create `golden_builder` jobs |
| search_indexer | 10s poll; 300s reaper; 1h cleanup | search queue |
| invitation_gc | 1h | expire invitations |
| timeline_cleanup | 24h | prune timeline entries |
| session_pruning | 60s | authz sessions (300s stale) |
| **lease_reaper** | **30s** | task/claim/worker/sticky/question recovery (D.1) |
| preview_reaper | 60s | cancel preview jobs past `job_preview_sessions.expires_at` |
| repo_gc | 1h (`repo_gc_interval`) | git repack |
| issue_inference | 5s | issue component inference |
| issue_attachment_scanner | 3s | attachment scan states |
| platform_issue_outbox | 10s (feature-gated) | GitHub delivery |
| branch_integration_worker | 5s sweep; 600s cycle lease | remote-tracking queue |
| branch_integration_scheduler | 30s | enqueue background inbound |
| base_auto_pull_scheduler | `base_auto_pull_interval` | exact-tip preparation / g5d publication |
| remote_advisory_pinger | 60s | ls-remote advisory SHA |
| workspace_operation_resolver | 3s (git projection) / 30s (maintenance) | ack recovery, orphan staging cleanup, ref projection repair |
`repo_fetcher` is intentionally not spawned (`mod.rs:452`). `scheduler/` directory is empty — there is no separate scheduler process; scheduling is the poll SQL.

---

## H. Counts (OBSERVED unless noted)
- Distinct status vocabularies: task 12 (DB) / 12 (Rust, mismatched set); job 10; worker 5; sticky runtime 4; question window 4; commit request 3; approval handoff 4; escalation 4; job_merge_run 7 (+5 phases); builder_task_spec 7; workspace_head_operation 5; attempt disposition 5 / reason 15 / retry_policy 3 / posture 4. Total ≈ 118 enumerated status/close values across 14 vocabularies.
- Kernel: 33 `LifecycleCommand` variants, 19 `JobRollupKind`, 7 families, 44 legal task arcs, 3 lock plans.
- Mutation paths: 142 `apply_lifecycle` call sites (task_service 74, chat_service 25, ...); raw `UPDATE tasks` statements ≈77 across 14 files (task_service 33, chat_service 16, job_service 10, internal.rs 4, persist.rs 3, ...); raw `UPDATE jobs` ≈53 across 12 files; `UPDATE workers` 15. `task_service.rs` has 64 `pub async fn` and ~395 functions in 33,023 lines.
- Lifecycle code footprint (task/job services, kernel, reaper/resolver, sticky/chat-loss/question/escalation, advance/finalizer/merge, routes, g5d worker poll/inbox/lifecycle/task_agent/attribution/runtime_keeper): ≈120k lines; kernel alone 4,075 lines; `internal.rs` 15,051; `sticky_runtime.rs` 8,073; `lease_reaper.rs` 2,273; `g5d-worker/src/lifecycle.rs` 7,965.

---

## Coupling (OBSERVED unless noted)
1. **Task status ⟷ sticky runtime custody**: `recovering`/`recovery_blocked`, the `WorkspaceUnreleased` invariant, trigger 273, migrations 247-273 and ~8k lines of `sticky_runtime.rs` exist only because a VM-resident runtime can outlive the task that owns its workspace. The claim SQL embeds `retiring_runtime_claim_fence`.
2. **Task status ⟷ chat turn status**: every outcome/human command carries `terminalize_chat_turn` and `persist_chat_terminal` writes `chat_messages` in the same transaction; `ChatWorkerLoss`, `Park`, warm re-delivery (`waiting → claimed`) are chat-only arcs.
3. **Task completion ⟷ CRDT/authority advance**: completion with `commit_results` must run the two-stage finalizer (`prepare/finalize_publish_branch_state`, frontier cuts/receipts, advance consumer/notifier). `transition_task` refuses commit results outside the route.
4. **Scheduling SQL ⟷ everything**: the poll predicate joins jobs, workspaces, `workspace_ref` JSON, sticky runtimes, attempts, workers, capabilities (12+ fences) — every feature added a fence.
5. **Job counters and status derived twice**: inline counters (`completed_tasks`/`failed_tasks`) plus `derive_job_status_from_children` on locked child rows.
6. **Vocabulary drift**: Rust `TaskStatus` lacks `waiting`, DB lacks `merged` for tasks; `TaskType` enum ≠ DB `task_type`; frontend status is untyped.
7. **Human-in-the-loop is spread across four tables** (escalations, question windows, approval handoffs, timeline approval entries) with three different timeout/settlement mechanisms.

## Simplification candidates (for the target architecture: one VM per workspace, local processes, git persistence, no CRDT)
1. **Drop `claimed`, `recovering`, `recovery_blocked`, sticky runtimes, worker_session_generation fencing.** With a per-workspace VM dispatcher, "workspace custody" is the VM itself; a task is either running as a local process or not. Risk: losing the explicit fence that stops two executions from touching one working tree — must be replaced by a single-dispatcher-per-workspace invariant (one mutex per workspace VM).
2. **Collapse `LifecycleCommand` (33) to ~8 events** (`start, complete, fail, block, wait, cancel, pause, resume`) and `JobRollupKind` (19) to a derived function of child states. Risk: losing typed close evidence (`task_execution_attempts.reason`) that ops rely on; keep the attempt table but populate it from the simplified events.
3. **Remove the two-stage completion finalizer, frontier cuts/receipts, advance consumer/notifier, workspace_head_operations** if the VM commits to git itself and pushes to internal git; completion becomes "commit SHA reported and verified reachable". Risk: multi-repo atomic cuts and post-commit projections (search reindex, branch integration) currently depend on the advance record; they need a git-hook or post-push event instead.
4. **Replace lease/heartbeat/reaper with dispatcher liveness**: the dispatcher RPC knows process exit; heartbeat only needed VM-level. Risk: API restart loses nothing today because Postgres holds everything; a dispatcher-held process table must be reconcilable from git checkpoints + DB.
5. **Unify HITL on one durable "task question" table** with one timeout mechanism; keep escalations as a UI over it. Risk: question-v1 receipt semantics were added to prevent forged `waiting` transitions.
6. **Delete dead paths**: `fresh_retry_mode` (stubbed), `ViewCreate`, `pruned`/cascade-prune, review checkpoints (`review_every_k`, `is_checkpoint`) if unused by AQL, `head_commit` legacy cursor.
7. **Make `waiting` a first-class enum variant** (or remove it) and align DB CHECK, contracts enum, and frontend type.

## Open questions (for the user)
1. Do chat sessions still need a resident agent process between turns, or is "idle chat has no running process" absolute? (Determines whether any warm-resume/`held_runtime_id` path survives.)
2. Should a task be retried automatically at all after a worker-reported failure? Today it is not (only lease/worker loss retries, max 2).
3. Which job types survive: `compile/uplift/downlift/test/custom` (batch DAG with `depends_on`, `max_concurrency`, checkpoints) vs interactive one-task jobs (`chat/sync/merge/preview/explainer/approval_review/golden_builder`)? A single "task with optional DAG" model may suffice.
4. Is job-level pause/resume/rollback (and `rolled_back` statuses) needed once persistence is git (rollback = reset branch)?
5. Who owns multi-repo atomicity after CRDT removal: one git repo per workspace, or a manifest of repos committed together?
6. Does governance at the git boundary replace `task_completion_constraints` and `approval_review` jobs, or do those stay as pre-push checks?
7. Should `task_execution_attempts` remain the audit record (recommended) and what minimum `reason` vocabulary do operators actually use?
