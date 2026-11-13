# Abstraction leakiness, round 2 — implementation review

Reviews of the [round-2 plan](ontology-plan.md)'s PRs, one section per PR. Each row reads commitment → actual path → check → result.

## PR 1 — Task and dispatch bugs

Self-review by the implementing agent, before the independent Codex review recorded in the PR. Base: `main` at `f19cc664`.

| Commitment | Actual path | Check | Result |
|---|---|---|---|
| [`g5.agents.dependencies`](../../ontology/features/agents.g5.md#dependencies-belong-to-logical-tasks): edges are the sole dependency authority | `task_service.rs::TASK_DEPENDS_ON_SQL` selects `task_dependencies` for `get_task`, `list_task_pages_for_jobs` and `JobService::list_tasks_for_job` (which the progress graph reads) | `api_provider_accounts.rs::bootstrap_graph_submission_allocates_each_node_through_task_service` now also checks the task view and the job task list | Pass; reading the legacy column fails it (bug 3) |
| [`g5.agents.admission`](../../ontology/features/agents.g5.md#one-admission-policy-covers-jobs-and-individual-work): job caps apply to agents and commands | `agent_dispatch.rs::job_admits_task_sql!` (job running, below its cap) is shared by both reservation queries; a jobless agent task skips it | `api_task_lifecycle.rs::job_agent_tasks_are_admitted_under_the_job_concurrency_cap` (cap 1 in a 4-slot workspace, then a paused job) | Pass; dropping the check from agent reservation fails it (bug 4) |
| [`g5.agents.allocation.specification`](../../ontology/features/agents.g5.md#validate-task-specifications-at-allocation): refuse malformed specs before allocation writes | `check_job_task_spec` runs dispatch's parsers and validators first in `allocate_job_agent_task` and `allocate_job_command_task`; JobService builds specs with `bootstrap_command_spec` and `workflow_agent_spec` | `agent_dispatch::tests::job_task_specs_are_checked_with_the_dispatch_rules`; existing allocation tests | Pass |
| Same block: a bad spec fails its own task | Command reservation settles an unrunnable spec as `dispatch_admission_failed` and returns no reservation, as agent reservation already does | `api_task_lifecycle.rs::unrunnable_command_spec_fails_its_task_without_blocking_agent_dispatch` runs one coordinator pass | Pass; returning the error again fails it (bug 5) |
| [`g5.lifecycle.deadline.rotation`](../../ontology/software_architecture/lifecycle.g5.md#replace-cloud-capacity-before-draining-it), [`g5.architecture.placement`](../../ontology/software_architecture/worker_host.g5.md#capacity-and-runtime-placement): rotation acts on its own drains | The resumability sweep selects `draining` hosts with no `drain_id`; operator drains always record one | `api_worker_hosts_stale.rs::rotation_leaves_an_operator_drained_host_alone` | Pass; without the filter the sweep "resumes" the drain (bug 2) |
| Job output polling ends | `useJobs.ts::useJobTasks` compares task statuses with `TERMINAL_TASK_STATUSES` | `useJobs.test.ts` "polls while a task runs and stops once every task has ended" | Pass; the job status set fails it (bug 8) |

**Mutation runs** (each applied alone, the named test run, then restored): reading the legacy `depends_on` column, dropping the job check from agent reservation, returning the command spec error again, dropping `drain_id IS NULL` from the rotation sweep, and checking task statuses against the job status set. Each failed its test with the bug's symptom.

**As built, compared with the plan:**
- The job-cap test inserts the job's agent tasks directly, like the existing command cap test, rather than allocating them through a bootstrap graph. That keeps it on the reservation predicate.
- Agent reservation now also requires the job to be running, the same rule command reservation already applied.
- Allocation's spec check is unit-tested on the pure function. JobService cannot build a malformed spec through the builders, so no API path produces one.

**Independent review (Codex, round 1).** Approved with no findings. Of its follow-ups, one was fixed in the second commit: the `TaskDependency` model comment still called `tasks.depends_on` the authority. The others are left:
- negative tests through the allocation helpers, since the unit test covers the check and the builders leave no API path to a bad spec;
- retiring the legacy column and trigger, which needs a forward migration.

**Bot review on the PR.** CodeRabbit (Major) noted a race. The sweep reads its leftovers once, and `request_drain` could still stamp a `drain_id` onto a host rotation was draining, so a host could be replaced and torn down after an operator drained it. Fixed in the third commit: a drain belongs to whoever started it.
- `request_drain` refuses a host rotation is draining, and the drain route answers 409.
- `forced_quiesce` never adopts an operator drain.
- Forced host removal still proceeds.

`api_worker_hosts_stale.rs::an_operator_drain_cannot_take_over_a_rotation_drain` fails without the `request_drain` change (200 instead of 409).

**Limits.** The legacy `tasks.depends_on` column and its migration-003 rebuild trigger remain; removing them needs a forward migration. Other allocation paths (enqueue, preview) are not checked here. CI's end-to-end lane and a QA deploy have not run on this branch.

## PR 2 — Host-reported releases settle

Self-review by the implementing agent, before the independent Codex review recorded in the PR. Base: PR 1's branch at `a4a37ed2`.

| Commitment | Actual path | Check | Result |
|---|---|---|---|
| [`g5.runtime.events.org-scope`](../../ontology/software_architecture/runtime.g5.md#host-facts-apply-under-the-hosts-organization): host facts apply under the host's organization | `host_control_state.rs` runs `accept_hello`, and `ingest_vm_event` for the facts `settles_agents_work` names (exit, out of memory, stale kill, boot failure), in the maintenance actor's transaction scoped to the host's organization on the tenant pool (`org_scope`, shared with maintenance as `maintenance_org_scope`). Other facts stay on the system pool | `api_worker_hosts.rs::host_reported_vm_exit_settles_queued_previews_and_notices`, `::host_hello_without_the_vm_settles_queued_previews_and_notices` | Pass; on the system pool the preview stays queued and the notice undelivered |
| [`g5.lifecycle.stop.release`](../../ontology/software_architecture/lifecycle.g5.md#release-only-the-stopped-incarnation), [`g5.preview.service.replace`](../../ontology/features/preview_terminal.g5.md#stop-the-previous-preview-before-replacing-it), [`g5.lifecycle.deadline.notices`](../../ontology/software_architecture/lifecycle.g5.md#deliver-lifecycle-notices-with-ordinary-input): a release cancels queued previews, settles the preview intent and withdraws notices | Host-reported releases reach the shared `settle_agents_after_release` under that scope | The same tests; `forced_host_removal_settles_queued_previews_and_notices` now uses the shared seed and checks | Pass |
| The system role is not widened, and no scope outlives its transaction | The tenant role runs these transactions with the ordinary row rules; `ScopedDb` clears the scope at commit | The exit test runs host control on a one-connection tenant pool and reads `authz.current_org_ids()` afterwards | Pass |
| A release never holds a VM row while it waits for a workspace row | Every release that settles Agents work locks its workspace rows (`KEY SHARE`, in workspace order) before writing (`workspace_vm_release.rs::lock_for_release`), the owner operations' order; its fenced update then locks only the VM rows it releases | Race and lock-order tests below | Pass |

**Mutation runs** (each applied alone, the named test run, then restored and touched): host facts back on the system pool, no workspace lock before a release, a VM pre-lock before the fenced update, a replay applied again, and purge locking the VM before the workspace. Each failed its tests, as recorded below.

**As built, compared with the plan:**
- The plan named `scoped_db_for_maintenance_org`; host control now uses the same transaction. The first build set the scope on host control's own system-pool transaction instead. The system role cannot lock `workspaces` (permission denied), so that version could not take the lock order the review below required.
- Ready is left on the system pool. Its owner effect, `task_lifecycle.rs::release_workspace_tasks_after_resume`, would take effect for the first time and return every task a stop held to the queue on the next Start. That includes a turn the owner stopped from its chat, which carries the same `owner_stop` hold. The design's hold table supports that release and the continue story does not settle it, so the choice is recorded as [`g5.lifecycle.start.held-work`](../../ontology/software_architecture/lifecycle.g5.md#decide-whether-start-continues-held-work). Until it is answered, held work waits for Continue, as it does today.
- A test helper, `TestApp::host_control_backend`, builds host control the way production does; the 22 test call sites use it.

**Independent review (Codex).** Settling under the scope makes host releases lock the tasks and notices they settle. That exposed lock-order cycles, which lease expiry and forced removal already had.
- Round 1 (P2): a release locked the VM row and then the preview tasks it cancels; an owner's cancellation locks the preview task and then the VM row.
- Round 2 rejected the first fix and found more. Taking the VM row first in cancellation created a new inversion with a running preview's readiness, which locks the task and then the VM row. And a three-way cycle ran through notices: hello held the VM row and waited for a notice; agent settlement held the notice and waited for the workspace row (its next task's foreign key); the owner's cancellation held the workspace row and waited for the VM row. A release that writes its VM row twice also re-checks that row's foreign key, taking the workspace row after the VM row; the PostgreSQL deadlock report and a lock dump showed it.
- Round 3 (P2): the first version also locked the workspace's VM row before the fenced update. A delayed exit for a VM that had already stopped then waited for it while holding the host row, and retained-root purge, which locks the stopped VM row and then updates the host row, deadlocked with it. The release now leaves VM locking to its fenced update, which skips a stopped VM without waiting, as on the base; `delayed_exit_does_not_wait_for_a_stopped_vm_being_purged` fails with the pre-lock back.
- Round 4 (P2): a replayed exit records nothing new, so its insert took no workspace lock, but the release then asked for one while holding the host row. With an owner Stop holding the workspace row and waiting for the VM row, and purge holding the VM row and waiting for the host row, the three formed a cycle. Recording an event and applying it commit together, so a replay is now only acknowledged again (`host_control_state.rs::ingest_vm_event`); `exit_replay_waits_for_neither_an_owner_stop_nor_a_purge`, through the real Stop route, fails without that.
- Round 5 (a full wait inventory, P2): a hello or forced removal selects its candidates without locking them while holding the host row. If lease expiry released a candidate meanwhile, purge locked it, and an owner's Stop held the workspace waiting for it, the release's workspace lock closed a cycle through purge, which then waits for the host row. The inventory found purge's final transaction to be the only path locking a VM row before its workspace; it now takes the workspace row first. `retained_root_purge_takes_the_workspace_before_its_vm` runs the real purge and Stop against a transaction holding the host row and then asking for the workspace; it fails with purge locking the VM first.
- The fix keeps every owner path's order and makes releases follow it. A release takes its workspace rows (`KEY SHARE`) before its VM rows, so it serializes with owner operations at the workspace row (they take `FOR UPDATE`). It never holds a VM row while waiting for a workspace row. `KEY SHARE` does not block the foreign-key checks of agent settlement, even while an owner operation waits for the row (probed on PostgreSQL 17). To take that lock, host control runs these transactions on the tenant role, as the plan said.

Tests, each through the real release and the real owner route or projection:
- `*_and_preview_cancellation_do_not_deadlock` for hello, a reported exit and lease expiry: a transaction holds the VM row, and the release and then the cancellation queue on it.
- `*_waits_for_the_workspace_before_its_vm` for the same three: while a transaction holds the workspace row, the waiting release must hold no lock on the VM row (`NOWAIT` probe).
- `api_task_lifecycle.rs::running_preview_cancellation_and_its_readiness_do_not_deadlock`: the readiness projection and then the cancellation queue on the held task row.

A reported exit was already serialized by its event row's foreign key on the workspace, so its variants pass either way; they stay as coverage.

**Limits.** Ready's release step still matches no task row. CI's end-to-end lane and a QA deploy have not run on this branch. That production host control runs under row security is inferred from its roles, which the test harness shares. Two batch releases sharing several VMs (a hello and lease expiry on the same host) still lock VM rows in scan order, as on the base.

## PR 3 — One reader per shared shape

Self-review by the implementing agent, before the independent Codex review recorded in the PR. Base: PR 2's branch at `cc2fd232`.

| Commitment | Actual path | Check | Result |
|---|---|---|---|
| [`g5.architecture.conversations`](../../ontology/software_architecture/agents.g5.md#shared-conversations-and-inbox): the owner of a delivery or outcome decides it, and ChatService writes the conversation's rows and builds its live events | `maintenance/agent_tasks.rs` builds a journal event's live events with `task_lifecycle.rs::journal_live_events` inside the organization-scoped transaction and publishes them after commit and ack. TaskService builds the task status and timeline; ChatService builds the chat events (`agent_turn_live_events`) | `api_task_lifecycle.rs::live_agent_events_carry_the_placeholder_position_and_timeline_entry`, through a real coordinator pass and WebSockets | Pass. On the old sources the timeline event never arrives and a chunk carries no `messageIndex` (bug 6) |
| [`g5.architecture.task_owner`](../../ontology/software_architecture/agents.g5.md#one-logical-task-owner): TaskService alone decodes the attempt snapshot | `attempt_snapshot.rs` holds the decoders of the `agent_launch` member (`snapshot_launch_value`, `resolved_launch`, the envelope decoder) and the session-launch query. Dispatch, device login, the attempt view and ChatService's `agent_launch_for_session` use them; dispatch still reads the other members it writes | Moved unit tests; `resolved_launch_is_read_from_either_snapshot_shape` | Pass |
| Same blocks: dispatch stops reading ChatService's notice rows, and ChatService stops reading TaskService's inbox | Dispatch reads notice contents through `chat_service.rs::reserved_notice_contents`; TaskService reads an auth-failed turn's reserved inputs and passes them to `record_auth_failure_turn` | `codex_device_auth::terminal_auth_failure_withdraws_reserved_platform_notices`, `::new_input_wins_login_race_and_dispatches_after_terminal_outcome` | Pass |
| Guard | `backend/g5-api/tests/shared_shape_reader_guard.rs`: `policy_snapshot_ref` only in TaskService files and the row model; `platform_notice` only in `chat_service.rs`; every listed owner still names its shape | The guard | Pass |

**Mutation runs** (each applied alone, the named test run, then restored and touched). Each failed its test:
- passing no inputs to the auth-failure turn (one message instead of two);
- returning no notice contents (dispatch refuses the turn);
- skipping the live chat events (no chunk arrives);
- naming the snapshot column in `chat_service.rs`;
- naming the notice artifact in `agent_dispatch.rs`.

**As built, compared with the plan:**
- The decoders keep their two strictness levels. Dispatch and device login read only the resolved contract, so a later change to the preferences' shape cannot block a frozen launch. Chat reads the whole envelope, as before.
- Device login used to compare the raw `authMode` string; it now uses the typed resolved contract. A running attempt was dispatched only after that contract decoded, so the outcome is the same.
- An auth-failed turn's inputs keep the old rule, sharing one message index, which matters only if an attempt ever reserved more than one input.
- The coordinator now builds live events before commit, so a failed read rolls back the journal event; it is retried on the next pass. Live events were best-effort before.

**Independent review (Codex, round 1).** No runtime defect found. It asked for narrower wording, now applied. Only the `agent_launch` member's decoders were consolidated; dispatch still reads the snapshot members it writes. The conversation owner decides; ChatService writes rows and builds live events. ChatService still reads `task_events` for the session timeline, so "each side reads only its own rows" was too broad. Its follow-ups are left:
- a journal event whose commit succeeds but whose acknowledgement fails loses its live events, because the replay is suppressed as a duplicate. This predates the PR;
- coordinator coverage for `input.accepted`, final and outcome events, duplicates and failed-ack recovery.

**Limits.** CI's end-to-end lane and a QA deploy have not run on this branch. The live-event test covers `message.delta`; `input.accepted` and `invocation.outcome` keep their old event construction, moved unchanged.

## PR 4 — Authority checks in their owners

Self-review by the implementing agent, before the independent Codex review recorded in the PR. Base: PR 3's branch at `34f64aec`.

| Commitment | Actual path | Check | Result |
|---|---|---|---|
| [`g5.architecture.flush_owner`](../../ontology/software_architecture/workspace.g5.md#flush-owns-the-durable-snapshot), [`g5.git.receipt`](../../ontology/software_architecture/durability.g5.md#only-verified-complete-capture-advances-recovery): the flush service proves a receipt itself | `WorkspaceFlushService::complete` reads the authority, proves the receipt with the Git commands the route ran, then records it; the route only translates | `api_workspace_flush_receipts.rs::service_completion_refuses_a_receipt_its_flush_refs_do_not_prove`: a direct service call with a receipt its flush leaf does not prove; the route's receipt tests are unchanged | Pass. Without the proof in the service, the forged receipt completes |
| [`g5.security.admission`](../../ontology/concepts/security.g5.md#admit-actions-against-current-authority): one reader decides whether a VM token is current | `workspace_incarnation.rs`, held by the workspace lifecycle, serves internal routes (`require_live_workspace_incarnation`), Git reads and ref updates (`current_incarnation_repo_target`), push admission, JobService's bootstrap graph and the flush service | One stale-epoch test per purpose: `api_pull_request_reads.rs::stored_reads_enforce_visibility_and_current_workspace_incarnation`, `api_git_push_policy.rs::incarnation_pushes_are_confined_to_its_branch_and_flush_leaves` (push, and now read), `api_task_lifecycle.rs::bootstrap_graph_is_authorized_before_it_is_validated`, `api_workspace_flush_receipts.rs::flush_refuses_a_token_for_a_replaced_incarnation` (new) | Pass. With the reader ignoring the epoch, each fails: the internal read returns 200, the old push and read succeed, the graph reaches validation, the flush completes |
| Same block: push admission re-checks the token under its lock | Admission asks the reader with the VM row `FOR SHARE` after the unlocked ref-update check | `api_git_push_policy.rs::incarnation_push_is_rechecked_under_the_admission_lock`: a newer epoch commits while the push waits for the VM row | Pass. Without the lock the push never waits; with admission accepting what the reader refuses, the push lands |
| Same block: a bootstrap graph also needs a running VM and no pending transfer | JobService locks job, workspace, VM, task and attempt in one statement (the lifecycle order), then asks the reader and applies its two rules | The bootstrap test's stopping-VM and pending-transfer cases | Pass. Removing either rule lets that case reach validation (422) |
| [`g5.design.authority`](../../ontology/concepts/system_invariants.g5.md#each-fact-has-one-source-of-truth): push permission has one source | `routes/git.rs::user_can_write_workspace` keeps its access check and asks `PermissionService::user_holds_workspace_scope` for `workspaces:write` | `api_git_push_policy.rs::push_write_permission_is_the_permission_service_union`: owner and member roles, a viewer, an org default, an expired and a live user grant, a workspace override | Pass |
| [`g5.architecture.api_layers`](../../ontology/software_architecture/boundaries.g5.md#api-adapters-depend-on-domain-operations): routes translate, inbound hatches are closed | `PostgresHostControlState::new`, `WorkspaceVmService::stop_as_system` and `record_forced_vm_loss` are crate-private, with test-helpers entries for the first two. Routes call `stop` with the actor or `stop_for_capacity`. Forced host removal is `WorkerHostService::remove_host`: custody check, drain, forced losses and termination in one transaction | `crate_private_fences.rs` pins all three; `route_sql_guard.rs` lost three exemptions; the host-removal tests in `api_worker_hosts.rs` and `api_worker_hosts_cloud.rs` | Pass |
| [`g5.architecture.commands.writers`](../../ontology/software_architecture/boundaries.g5.md#declare-table-writers-and-check-them): `worker_hosts` is guarded | Already in `core_table_writers_guard.rs` since round 1 | The guard | No change needed |

**Mutation runs** (each applied alone, the named tests run, then restored and touched). Each failed its tests:
- the reader ignoring the epoch (the four stale-epoch tests above);
- push admission without the VM row lock, and push admission accepting a token the reader refuses (the race test);
- JobService without the running rule, and without the transfer rule (the bootstrap test);
- `complete` without the proof (the service-level test);
- a locking read without its second check (the lease test).

**As built, compared with the plan:**
- The reader has two entry points: an organization-scoped connection (internal routes, JobService) and the system pool (Git, flush), because the tenant role cannot call `authz.lookup_workspace_runtime_authority` and the system role cannot read `workspaces`. Both share one predicate. Leases are now compared with the database clock (`clock_timestamp()`); the internal routes used the API's clock and the others `now()`.
- The reader also returns the workspace's repositories, so the Git read check needs no SQL of its own, and flush start takes its member vector from it instead of a second query.
- JobService keeps one locking statement so its lock order does not change, minus the incarnation predicates, which the reader now applies to the locked rows. A stale token whose bootstrap task still runs now briefly locks those rows before it is refused.
- A stale token's push or read is refused with "workspace incarnation is no longer current" where it was sometimes a lease or status message. The HTTP status (403) is unchanged.
- The flush service takes repository storage and the Git binary. Git still runs between the authority read and the record, never under row locks. `load_for_verification` is private; the test that held it to the guest RPC fence now calls `complete` with an empty receipt, which fails at the member vector after the same read.
- The test runtime peer completes flushes through the service, so it now creates its flush leaves first, as receive-pack does for a guest.
- Three route SQL exemptions are dropped (26 to 23): push permission left `user_can_write_workspace` without SQL too. `route_auth_static.rs` matches the new `require_live_workspace_incarnation(&mut db, …)` call. `selected_org_scope_audit.rs` drops its three entries for the token's organization in those route checks, which the reader now compares. The `git.rs` source test that required `deleted_at IS NULL` in `current_incarnation_repo_target` leaves that check to the reader's own test of its rule.
- Forced host removal now terminates the host in the same transaction and delivers the drain after it commits. Delivery uses the live tunnel, which the route then closes, as before.
- `worker_hosts` was already in the writer guard; the round-2 review was wrong about that.

**Independent review (Codex).**
- Round 1 (P2): the reader's lease condition ran before PostgreSQL granted the VM row lock. A transaction holding the row without changing it gets no recheck after the wait, so a lease that ended during the wait read as current; the internal routes used to compare the lease after locking. A locking read now checks the rule again once it holds the rows (`workspace_incarnation.rs::read`). `api_pull_request_reads.rs::a_lease_that_expires_during_the_row_lock_wait_is_refused` holds the VM row while the lease ends; without the second check it returns 200.
- Round 1 (P3): the admission metadata cited `api_token_principal_matrix` as flush evidence; it now cites the flush test that checks `STALE_INCARNATION`.
- Not taken: a Git-route test for the permission union (a follow-up, as the review said); the service test covers each grant source.

**Bot review on the PR (CodeRabbit).**
- Major: `remove_host` counted the host's VMs before anything locked the host. Placement holds the host row while it puts a VM on the host, so a VM placed in that gap went uncounted, and a removal without force could terminate a host that owned it. The route's two transactions had the same gap. `remove_host` now locks the host row first. `api_worker_hosts.rs::removal_counts_a_vm_placed_while_it_waits_for_the_host` holds the row the way placement does, writes a VM, and commits once the removal waits: the removal answers 409, and 204 without the lock.
- Minor: the owner push path checked the lease in its locking statement, the same gap as round 1's. It now reads the lease once the VM row is locked. `api_git_push_policy.rs::owner_push_checks_the_lease_once_it_holds_the_vm_row` lets the lease end while the push waits for a held row; without the second read the push lands.
- Trivial, not changed: the flush progress logs at `warn` and the proof's `safe.directory=*` came unchanged from the route.
- Other paths check a lease in a statement that can wait for a row lock (for example the runtime facade's call admission). This PR does not touch them; they are a follow-up.

**Limits.** `api_workspace_vms.rs::api_token_principal_matrix` asserts only 409 for its stale tokens, which an open flush also returns; the new flush test asserts the code. CI's end-to-end lane and a QA deploy have not run on this branch. The flush proof's Git commands are a copy of the route's, kept with the service; consolidating Git subprocess helpers is left out on purpose.

## PR 5 — Preview and terminal contracts

Self-review by the implementing agent, before the independent Codex review recorded in the PR. Base: PR 4's branch at `37a3b28c`.

| Commitment | Actual path | Check | Result |
|---|---|---|---|
| [`g5.story.preview-my-application`](../../ontology/features/preview_terminal.g5.md#preview-my-application), kebab-case names (approved with this plan): one profile name rule | `g5_contracts::types::validate_preview_profile_name` is kebab-case; the guest's `preview.start` validator calls it instead of its own copy | `g5d-dispatch` `preview_start_accepts_only_kebab_case_profile_names`; the contracts unit test now refuses `web_client` and `Web` | Pass. A guest without the check fails its test, and the old shared rule fails the contracts and listing tests (bug 7) |
| Same story: the profile list offers what a launch accepts | `WorkspaceService::list_preview_profiles` reads each file with launch's parser (`task_lifecycle.rs::read_preview_application`) | `api_workspace_vms.rs::committed_preview_profiles_list_only_what_launch_accepts`: of `web`, `unknown-key` and `web_client`, only `web` is listed, and a launch of either refused one is a 422 | Pass. With the old rule `web_client` is listed; with a parser that takes any YAML, `unknown-key` is |
| [`g5.preview.transport`](../../ontology/features/preview_terminal.g5.md#preview-transport-preserves-owner-authority), [`g5.architecture.runtime_facade`](../../ontology/software_architecture/workspace.g5.md#one-live-runtime-facade): a preview stream is bound to its task's incarnation | `workspace_preview_target` returns the task's VM and epoch; `WorkspaceRuntime::open_preview_stream` refuses a workspace now on another incarnation and writes the port header (`stream_mux::encode_http_port`), which the guest's `serve_http` reads with `decode_http_port`. `open_owner_stream` opens terminal streams only | `api_task_lifecycle.rs::preview_stream_opens_only_on_the_preview_incarnation`; `stream_mux` `http_port_header_round_trips` | Pass. Without the epoch check the stale target is served; with the header little-endian the guest side reads another port |
| [`g5.terminal`](../../ontology/features/preview_terminal.g5.md#terminal-is-the-owner-escape-hatch): one terminal message definition | `g5-contracts::terminal` holds `TerminalClientMessage` (`open`, `input`, `resize`, `close`) and `TerminalServerMessage` (`opened`, `rejected`); the API builds `open` and binary `input` from them, the guest parses and writes them | `g5-contracts` `terminal_messages_keep_their_json`: the JSON both ends and the browser exchanged, including `open` without an `offset` key | Pass. Serializing an absent offset as `null` fails it |

**Mutation runs** (each applied alone, the named test run, then restored and touched). Each failed its test:
- the shared rule allowing `_` and upper case (the listing test);
- the guest skipping the shared rule (the guest test);
- the listing accepting any YAML (the listing test);
- the preview stream ignoring the epoch (the stale-target test);
- `open` always carrying an `offset` key (the JSON test);
- the port header in little-endian order (the stale-target test).

**As built, compared with the plan:**
- The rule is kebab-case, the guest's, as approved. Names the API accepted and the guest refused (`_`, upper case) are now refused when saved, started or listed. Such a profile could never run as a preview. `g5d preview-serve` applies the same rule (`g5d-cli/src/config.rs::load_preview_profile`), so running one directly in the VM, which worked, now needs a kebab-case name (for example `web_client.yaml` renamed to `web-client.yaml`).
- The typed parser stays in TaskService, which launches previews; WorkspaceService's lenient copy is gone. `WorkspaceService::list_preview_applications` used that copy and had no caller (the route lists the live checkout through TaskService), so it went too.
- A listed profile's `envKeys` now come back in key order, the typed parser's map order.
- A stale target is refused with `PREVIEW_NOT_READY` (503), the error the route already returned when the task's incarnation is not live, so the browser retries as before.
- The guest's terminal test hook is a field of the shared `open` message only when the `g5-contracts` feature `terminal-test-hooks` is on; `g5d-dispatch`'s `test-support` enables it, production builds do not.
- No wire change: the port header bytes, the `open` fields (no `offset` until the browser saw an `opened` with one) and the guest's control lines are the same, so guests on the current worker image keep working and the new guest works with the current API.

**Independent review (Codex).** Round 1 asked for two documentation corrections, applied above: running a profile with `_` or upper case directly with `g5d preview-serve` worked and now needs a rename, so the compatibility claim is qualified; and the guest mutation run removed the check, so the row says so (the guest's own copy was already kebab-case). Its mixed-version probes of the terminal frames and the port header passed.

**Rollout.** This changes worker-image inputs (`g5-contracts`, `g5-internal-api`, `g5d-dispatch`), so QA needs a worker bake before the guest side ships. After the deploy, check that the worker-image pin did not go backwards (it did on 2026-09-30).

**Limits.** CI's end-to-end lane, a worker bake and a QA deploy have not run on this branch. The stale-target test calls the facade directly; the route's resolve-then-open window was not raced through HTTP. The browser's terminal literals are pinned by the contracts test, not by a browser run.

## PR 6 — Lease checks after lock waits

Self-review by the implementing agent, before the independent Codex review recorded in the PR. Base: PR 5's branch at `1d536c92`.

| Commitment | Actual path | Check | Result |
|---|---|---|---|
| [`g5.security.admission`](../../ontology/concepts/security.g5.md#admit-actions-against-current-authority): a VM lease that ended does not admit work | `workspace_runtime.rs::lease_running` reads `lease_expires_at > clock_timestamp()` once the caller holds the VM row; the runtime call admission and `lock_owner_handle` use it | `api_workspace_vms.rs::runtime_call_checks_the_lease_once_it_holds_the_vm_row`: a transaction holds the VM row without changing it while the lease ends, and the preview-applications route's runtime call is refused without reaching the guest. `::owner_work_admission_checks_the_lease_once_it_holds_the_rows`: the same with `lock_owner_handle` | Pass. With the check removed, or read against the transaction start time, the call reaches the guest and the admission succeeds |
| Same block: preview start | `task_lifecycle.rs::start_workspace_preview` reads the lease with `lease_running` after its `FOR UPDATE OF workspace, vm` read | `api_workspace_vms.rs::preview_start_checks_the_lease_once_it_holds_the_vm_row`: a share lock on the workspace row holds back only the authority read; the start answers `WORKSPACE_NOT_RUNNING` and creates no task | Pass. Without the check the preview task is created |
| Same block: device login | `codex_auth.rs::session_codex_auth` compares the lease with `clock_timestamp()`; its transaction may have waited for the workspace row in its authorization | `api_task_lifecycle.rs::codex_device_auth::challenge_read_checks_the_lease_after_its_transaction_waits`: the challenge reads as ended, without its code | Pass. With `now()` it still shows the code |
| Same block: the runtime attachment, which terminal streams and owner commands open on without a runtime call | `workspace_runtime.rs::get` reads the lease of the incarnation its authority lookup found, in a second statement after that lookup, which can wait for the workspace row. The Changes view's live check next to it compares with the clock | `api_workspace_vms.rs::runtime_attachment_checks_the_lease_after_its_authority_lookup`: a transaction holds the workspace row without changing it while the lease ends; `owner_handle` answers not running | Pass. With the lease back in the lookup statement, the handle is returned |
| Same block: preview tickets | `task_lifecycle.rs::workspace_preview_target` compares the lease with `clock_timestamp()`; its transaction may have waited in its authorization | `api_task_lifecycle.rs::preview_ticket_checks_the_lease_after_its_transaction_waits`: the mint waits in its authorization, then answers 503 `PREVIEW_NOT_READY` without a ticket cookie | Pass. With `now()` the ticket is minted |
| Same block: dispatch | Both queued reservations in `agent_dispatch.rs` compare the lease with `clock_timestamp()`. The pass reserves an agent turn in the transaction in which it may first have settled an unrunnable command, which updates that command's dependents | `api_task_lifecycle.rs::dispatch_checks_the_lease_after_its_pass_waits`: a dependent's row is held while the lease ends; the command fails, the agent turn stays queued and nothing reaches the guest | Pass. With `now()` the turn is reserved |

**Mutation runs** (each applied alone, the named tests run, then restored and touched). Each failed its tests:
- the post-lock lease read using `now()` (the runtime call, owner work admission and preview start tests);
- the runtime call admission without the lease check;
- owner work admission without the lease check;
- preview start without the lease check;
- the device-login read using `now()`;
- the runtime attachment checking the lease in its lookup statement, as before;
- the preview target read using `now()`;
- the agent reservation using `now()`.

**As built, compared with the plan:**
- One helper, `lease_running`, serves the facade's two admissions and preview start. It returns `None` for a row without a lease, so a call that finds its VM released keeps today's answer (503 from the call admission, not running elsewhere).
- The device-login read keeps one statement and switches to `clock_timestamp()`: the wait happens earlier in its transaction (authorization or the owner check), not in that statement.
- The tests match the waiting statement by the start of its text, because `pg_stat_activity` truncates long statements. Preview start's test holds the workspace row rather than the VM row: a share lock on the VM row first stops the runtime call's activity update, which is not the read under test.
- Host lease renewal is unchanged, as the plan says.
- The command reservation uses the clock as well, though nothing in its transaction waits before it today, so no test can tell the two apart. The Changes view's live check can wait in its own authority lookup; it uses the clock and only decides whether to ask `get`, which checks again, so `get`'s test covers it.

**Independent review (Codex).** Round 1 found three more paths of the same race, each confirmed with a PostgreSQL probe, and asked to correct the plan's claim that statements which never wait are unaffected: the runtime attachment (`get`), the preview target read, and the agent reservation after a failed command's settlement. All three are fixed above, with a test each; the sweep of every lease comparison added the command reservation and the Changes check. Round 2 approved `6550878b` with no findings and asked for the wording about the Changes check that is now above.

**Limits.** CI's end-to-end lane and a QA deploy have not run on this branch. The statements this PR changes still read `now()` elsewhere in their transactions for timestamps they write; only the lease comparisons moved. In the full `g5-api` run after round 1's fixes, `api_home.rs::organization_home_attention_batches_approval_reads_at_large_visible_workspace_count` failed once and passed three times alone: its query observations sit in one process-wide list that tests running in parallel both clear and read, and this PR does not touch Home.

## PR 7 — Chat Stop ends the turn

Self-review by the implementing agent, before the independent Codex review recorded in the PR. Base: PR 6's branch at `0daf7278`. Tests are in `backend/g5-api/tests/api_chat_turn_stop.rs`: messages go through the chat routes, dispatch through the maintenance pass to the test runtime peer, and each attempt's events through `TaskService::apply`.

| Commitment | Actual path | Check | Result |
|---|---|---|---|
| [`g5.agents.lifecycle.chat-stop`](../../ontology/features/agents.g5.md#stop-in-a-chat-ends-the-turn): Stop ends a running turn, its reply keeps its partial output marked stopped, and queued messages run next | `task_lifecycle.rs::end_chat_turn` cancels the attempt (`request_workspace_task_cancel`); cancellation's settlement marks the reply stopped and `ensure_next_queued_agent_task` starts the queued messages | `::stop_ends_a_running_turn_and_the_queued_message_runs_next` | Pass. With the old pause the turn is held and the queued message waits |
| Same block: the next message's turn carries the stopped reply | Each reservation freezes the chat history, which includes stopped replies | `::the_next_message_after_stop_carries_the_stopped_reply` | Pass |
| Same block: Stop on a waiting message withdraws only it | `withdraw_waiting_input_for_stop` for a queued message that is not a held turn's own, typed by the owner or sent by the platform (a merge-conflict chat's first message); the owner's own withdraw action still takes typed messages only | `::stop_on_a_waiting_message_withdraws_only_that_message`, `::stop_withdraws_a_waiting_platform_message` | Pass. Without that branch, Stop on the waiting message cancels the running turn; limited to typed messages, Stop on a platform message answers 409 |
| Same block: a late Stop never reaches a later turn | The named input or reply resolves to its own turn; a turn that ended answers `stopped: false` | `::a_late_stop_never_reaches_a_later_turn` | Pass. Taking the session's newest turn, as before, stops the later one |
| Same block: a held or queued turn ends at once, its undelivered message is withdrawn, and queued messages run next | `end_chat_turn_now` with `ChatTurnEnd::Stopped`, then `ensure_next_queued_agent_task` | `::stop_ends_a_held_turn_and_withdraws_its_undelivered_message` | Pass. Keeping the message, or not starting the next turn, fails it |
| Same block: a job's turn is held for Continue | `request_workspace_task_pause` for a job's turn | `::a_job_turn_keeps_the_hold` | Pass. Without the job check the turn is cancelled |
| Same block: Stop is final, whatever ends the attempt first ([`g5.agents.lifecycle.cancel`](../../ontology/features/agents.g5.md#make-cancellation-terminal-for-the-logical-task)) | `outcome_decision` makes a requested cancellation final after an `interrupted` or `suspended` result too, not only `cancelled` | `::stop_is_final_whatever_ends_the_attempt_first` (an unanswered question, then an interruption, reach the attempt before the stop); the `outcome_decision` unit test | Pass. Without it the turn is held and the queued message waits |
| Same block: a turn held before any attempt reserved its message | `turn_message` takes a turn's own message by input order (the earliest one not withdrawn), whether or not an attempt reserved it | `::stop_ends_a_turn_held_before_its_first_attempt` (a queued turn paused by the task route) | Pass. Judging by the attempt, Stop withdraws the message and leaves the turn held |
| Same block, and [`g5.story.queue-a-follow-up`](../../ontology/features/conversations.g5.md#queue-a-follow-up)'s FIFO: a turn's own message is its earliest | Dispatch reserves a turn's earliest queued message; while another transaction (an edit) holds it, that turn waits for a later pass and the pass tries the next turn, where it used to skip ahead to a later message (`agent_dispatch.rs::reserve_queued_agent_dispatch`) | `::messages_run_in_order_while_the_earliest_is_being_edited`, `::a_held_message_does_not_stall_other_chats`, `::a_passed_over_turn_is_released_before_the_next_one` | Pass. With the old pick, the later message runs first, and a Stop naming either message then ends the wrong turn or withdraws the wrong message (Codex round 2); if the pass stops at the held turn instead, other chats in the organization wait (Codex round 3); if it keeps the passed-over turn locked, a withdrawal of that turn's message and the next turn's failed admission deadlock (Codex round 4) |
| Same block: a message carried into a new turn | The held-turn rule carries an undelivered message into the new turn, where it runs first; Stop naming it ends that run | `::stop_naming_a_carried_message_ends_the_turn_that_runs_it` | Pass. The newer message still runs next |
| [`g5.agents.lifecycle.chat-held-turn`](../../ontology/features/agents.g5.md#a-new-message-ends-a-held-chat-turn) | `agent_dispatch.rs::enqueue_agent_input` ends a typed chat's held turn (`end_chat_turn_now` with `ChatTurnEnd::NextMessage`) and moves its queued messages into the new turn, ahead of the new message | `::a_new_message_ends_an_idle_held_turn_and_its_undelivered_message_runs_first`, `::a_new_message_ends_a_turn_held_after_an_unanswered_question`, `::a_turn_of_unknown_delivery_still_waits_for_redelivery`, `::a_job_turn_keeps_the_hold` | Pass. Without the rule the message waits behind the held turn; without the exceptions a turn of unknown delivery or a job's turn ends |
| [`g5.conversation.providers.claude-background.end`](../../ontology/features/conversations.g5.md#let-the-owner-end-background-waiting): a bounded stop ends the turn as Stop does | The outcome settlement turns a chat turn's `background_wait_ended` result into a cancelled turn; a job's turn stays held | `::claude_end_that_stops_claude_ends_a_chat_turn`, `::claude_end_that_stops_claude_holds_a_job_turn` | Pass |

**Mutation runs** (each applied alone, the named tests run, then restored and touched). Each failed its tests:
- Stop pausing a running chat turn, as before;
- Stop taking the session's newest active turn instead of the one it names;
- no separate withdrawal for a waiting message;
- a job's turn ended like a chat's;
- Stop on a held turn not starting the next turn;
- ending a turn never withdrawing its undelivered message, and always withdrawing it;
- Claude's End holding a chat turn, as before, and ending a job's turn;
- a new message never ending a held turn, ending one of unknown delivery, and ending a job's (the end-turn helper's own job guard then refuses the send with 409);
- the ended turn's queued messages left on it;
- after Codex round 1: a requested cancellation held after an interruption or an unanswered question; a held turn's own message judged by its attempt; Stop not withdrawing an ended turn's own message, and a new message withdrawing it;
- after Codex round 2: dispatch's old pick, which skips a held earliest message;
- after Codex round 3: the pass stopping at a turn whose earliest message is held;
- after Codex round 4: the passed-over turn kept locked (no rollback to the candidate's savepoint); the pass is then the deadlock's victim and the next turn's settlement is lost;
- after Codex round 5: Stop's withdrawal limited to typed messages;
- after the bot review: a job's pause applied before the named message is classified, a queued turn's own message judged as for held turns only, a platform turn ended by the owner's next message, Stop and End ending a platform turn (one mutant of the shared owner check), and no hand-off when settlement holds a turn with a message behind it.

**As built, compared with the plan:**
- Stop takes the session's turn allocation lock before its row locks, in the order a send takes them.
- `stopped` is false when the named turn had already ended; before, a Stop with no active turn answered 409. `request_session_agent_task_pause` is gone.
- A turn's own message is the earliest one bound to its task that is not withdrawn (`turn_message`), whether or not an attempt reserved it.
- A turn ended this way is `cancelled`, with outcome reason `chat_stop` or `next_message`. A continuation's own input is withdrawn with it, and platform notices its last attempt reserved are withdrawn, as at any early end.
- The new message's turn uses the new message's launch settings; the ended turn's undelivered message runs in it first.
- Frontend: the Stop and End comments in `useChat.ts`, and the e2e cleanup helper, which no longer cancels after Stop.

**Independent review (Codex).** Round 1 asked for changes:
- Stop could still leave a turn held: settlement made a requested cancellation final only for a `cancelled` result, so an unanswered question or an interruption reaching the attempt first held it. Fixed above. This also makes the task Cancel final in that race, as `g5.agents.lifecycle.cancel` requires.
- Stop on a turn held before its first reservation withdrew only the message, because its own message had no attempt. Fixed above.
- A late Stop naming a message carried into a new turn ends that turn. Pushed back: the carried message runs first there, so the turn the Stop ends is that message's own, and the newer message still runs next. A Stop that reached the held turn before the new message has the same result. A test now pins this.

Round 2 closed the first two and found the gap in the pushback's premise: dispatch locked the earliest queued message with `SKIP LOCKED`, so while an edit held it, a later message ran first. Then a Stop naming the carried message ended the later message's held turn, and a Stop naming the later turn withdrew the carried message. Fixed at the root, as Codex suggested: dispatch now waits for the earliest message, which keeps the order every rule here assumes.

Round 3 found that waiting stalled the organization: the pass stopped at the oldest turn while its earliest message was held, so other chats waited too. The pass now leaves that turn for later and tries the next one.

Round 4 found that the passed-over turn stayed locked: a withdrawal of its last message holds the workspace row and then needs the turn's row, while the next turn's failed admission inserts a follow-up turn that needs the workspace row, and PostgreSQL aborts one. Each candidate is now tried under a savepoint, and rolling back to it releases a passed-over turn's rows.

Round 5 closed round 4's finding and found that Stop answered 409 for a waiting message the platform sent, because it reused the owner's withdraw action, which takes typed messages only. Stop now has its own withdrawal for either kind. Round 5 also reproduced a deadlock that predates this PR: the dispatch pass settles a failed command or preview and then reserves an agent turn in one transaction, so the settlement's locks are held while the agent turn's failed admission needs the workspace row that an owner cancellation holds. It is recorded as a follow-up.

Round 6, which the owner allowed past the five-round cap, approved `d5b7da3b` with no findings.

**Bot review on the PR (Codex bot, three P2s on `97a1ded5`, two on `cf27c4b4` and two on `a75efaf1`).**
- Fixed: Stop now classifies the message it names before it applies a job's pause. A message waiting behind a job's turn is withdrawn alone, and the turn keeps running (`::stop_on_a_waiting_message_in_a_job_chat_withdraws_only_it`).
- Fixed: a queued turn's own message is its earliest, as a held turn's is. Stop naming it ends the queued turn, and the message behind it starts the next turn instead of running in the stopped one (`::stop_ends_a_queued_turn_and_its_follow_up_starts_the_next`).
- Fixed (raised on `cf27c4b4`): a turn the platform started keeps its hold when the owner sends a message, like a job's turn. A merge-conflict resolution is one: its merge run follows that task (`job_merge_runs.resolution_task_id`), and ending it would carry its message into a new task the merge run does not follow (`::a_held_platform_turn_waits_for_continue`). The held-turn rule in the ontology gains this exception.
- Fixed (raised on `a75efaf1`): Stop cancelled a merge-conflict resolution turn, which leaves its merge run following a cancelled task, and `enqueue_resolution` starts no second attempt. A turn another owner follows, a job's or one the platform started (`turn_kept_for_its_owner`), now keeps today's hold on Stop and on Claude's End too, as it does on a new message (`::stop_holds_a_turn_the_platform_started`, `::claude_end_that_stops_claude_holds_a_platform_turn`).
- Fixed (raised on `a75efaf1`): a message sent while a turn was still stopping joined the running turn, then waited behind the hold. Settlement that holds a chat turn now ends it at once when an owner's message is queued behind it (`hand_off_held_chat_turn`; `::a_message_sent_while_the_turn_stops_ends_it_once_held`). So a typed chat's held turn no longer has an owner's message behind it, and `::stop_ends_a_held_turn_and_withdraws_its_undelivered_message` now covers Stop on a held turn alone. The two queued-turn tests cover the follow-up starting the next turn after Stop.
- Not changed (raised on `cf27c4b4`): the lock-order comment. Settlement takes the session's turn allocation lock before the task row (`TaskService::apply`), as Stop does, and every caller of `ensure_next_queued_agent_task` holds it first, so the Stop and settlement cycle it describes cannot form.
- Not changed: Stop during a Codex turn's device login ends the turn as an authentication failure. The device-login rule treats any stop before the prompt reaches the provider that way, and Stop's guarantee holds: the turn ends and the queued messages run next. Showing it as stopped would change that rule, so it is left as a follow-up.

**Limits.** The browser's Stop, CI's end-to-end lane and a QA deploy have not run on this branch. Holds after a workspace stop, VM loss, transfer or a needed restart take the held-turn path but were tested through the idle-stop and unanswered-question holds only. Stopped turns still record no usage (an existing gap of `g5.agents.lifecycle.owner-stop`). Found by reading, not reproduced: authorization keeps a share lock on the workspace row (`authz.lock_user_scope_authority`), and a send, the old Stop and this Stop each upgrade it to an update lock, so two such requests on one workspace can deadlock, and PostgreSQL then aborts one. This PR does not change that; it is flagged as a follow-up.
