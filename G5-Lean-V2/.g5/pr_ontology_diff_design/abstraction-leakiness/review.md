# Abstraction leakiness — implementation review

Reviews of the [refactor plan](ontology-plan.md)'s PRs, one section per PR. Each row reads commitment → actual path → check → result.

## PR 2 — Compile-time fences

Self-review by the implementing agent, before the independent Codex review recorded in the PR. Base: PR 1's branch at `bbb37f17`.

| Commitment | Actual path | Check | Result |
|---|---|---|---|
| [`g5.architecture.runtime_facade`](../../ontology/software_architecture/workspace.g5.md#one-live-runtime-facade): features reach the VM through typed operations with an authorized actor | `RuntimeRequest` fields private; `RuntimeRequest::owner` public, `RuntimeRequest::lifecycle` and `WorkspaceRuntime::call` crate-private; `HostTunnel::open_workspace_rpc`/`open_workspace_stream` crate-private | `crate_private_fences.rs`; workspace-wide `cargo clippy --all-targets` builds every caller | Pass. Widening `call` or a request field fails the contract test (mutation runs below). |
| Same, wire unchanged (plan rollout constraint: the guest dispatcher rejects unknown context fields) | `RuntimeRequest::context` still sends all eight keys, `inspection_grant_id` as `null` | `workspace_runtime::tests::context_does_not_mix_actor_and_lifecycle_authority` pins the exact JSON for both constructors | Pass |
| Same, no behavior change at the 24 construction sites | Owner sites take the incarnation from the handle; each handle comes from `get`/`owner_handle` of the same workspace, and the claim-built sites (`agent_dispatch.rs` ×2) already compare `vm_id`/`epoch` before invoking. Lifecycle sites pass their values unchanged. Deadlines and idempotency keys are unchanged. | Inspection of each site; 20 database-backed API suites (415 tests) that drive dispatch, stop, fork, flush, Changes, files and native replies through a runtime peer | Pass |
| [`g5.design.minimal-api`](../../ontology/concepts/system_design_principles.g5.md#expose-minimal-sufficient-apis): no unvalidated authority | The inspection-grant field is gone, so `call` has no grant branch that skips the owner check | Inspection; no caller set a grant | Pass |
| [`g5.architecture.placement`](../../ontology/software_architecture/worker_host.g5.md#capacity-and-runtime-placement): WorkerHostService supplies drain operations | `HostCommandClient::execute*` crate-private; `WorkerHostService::deliver_drain` takes the `request_drain` record and is used by `routes/worker_hosts.rs` (2 s × 2 tries); the reconciler (`maintenance/host_registry.rs`, 1 s × 1) uses the crate-private form for drains it reads from `worker_hosts` | `api_worker_hosts.rs::drain_reaches_the_live_host_from_the_route_and_the_reconciler` | Pass; skipping either delivery fails it. |
| [`g5.architecture.api_layers`](../../ontology/software_architecture/boundaries.g5.md#api-adapters-depend-on-domain-operations): owners decide authorization-sensitive transitions | Bootstrap authorization query moved unchanged into `job_service.rs`; `submit_bootstrap_task_graph` takes the claims; `BootstrapGraphAuthority` private | `route_auth_static.rs` (route passes `job_id` and `&claims`; service authorizes before validation and any statement), `route_sql_guard.rs` (26 functions) | Pass |
| [`g5.agents.bootstrap`](../../ontology/features/agents.g5.md#bootstrap-submits-one-atomic-dependency-graph): current-incarnation admission before graph work | Same locked query, first statement of the service call, same transaction the route opens and commits | `api_task_lifecycle.rs::bootstrap_graph_is_authorized_before_it_is_validated`: 403 for an invalid graph before the bootstrap command runs, 422 (validation) once it runs, no task created | Pass; validating before authorizing fails it. |
| [`g5.quality.evidence`](../../ontology/concepts/system_design_principles.g5.md#test-real-boundaries-and-report-exact-evidence): checks discriminate | New tests and guards | Mutation runs below | See below |

**Mutation runs** (each applied alone, the named test run, then restored):

- `WorkspaceRuntime::call` made `pub`, or one `RuntimeRequest` field made `pub(crate)`: `crate_private_fences::owner_entry_points_stay_crate_private` fails.
- `lifecycle_call_for_test` without its `test-helpers` gate, or a route function calling it: `crate_private_fences::test_only_entries_stay_out_of_production` fails.
- Graph validation moved before the authorization in `submit_bootstrap_task_graph`: both `route_auth_static` checks and the bootstrap API test fail.
- The drain route or the host reconciler skipping delivery: the drain API test fails.

**Independent review (Codex, round 1).** One P2: the contract test read only the first declaration with a pinned name, so a `#[cfg(not(test))] pub` variant after a test-only one, or an earlier same-named item in an inline module, hid a widening. Fixed: every declaration counts, an item inside test-only code (its own or an enclosing `cfg`) does not, and fixtures cover both shapes.

**Bot review on the PR.** The Codex bot (P2) noted that a public `deliver_drain` taking raw ids still let a route send `host.drain` for a drain it never recorded. Fixed: the public form takes the `HostDrainRecord` that `request_drain` returns, whose host and drain id are private (a later P2 noted the drain id was still mutable), and the raw-id form is crate-private; the contract test pins all three.

**Limits.** The successful bootstrap graph path (a job with a frozen launch, profile and template) still has no API test; the metadata of `g5.agents.bootstrap` records it. CI's end-to-end lane and a QA deploy have not run on this branch yet.

## PR 3 — One home for VM release

Self-review by the implementing agent, before the independent Codex review recorded in the PR. Base: PR 2's branch at `80c9654e`.

| Commitment | Actual path | Check | Result |
|---|---|---|---|
| [`g5.lifecycle.stop.release`](../../ontology/software_architecture/lifecycle.g5.md#release-only-the-stopped-incarnation): release atomically settles Agents | All eight custody-clearing statements live in `services/workspace_vm_release.rs`; each release that ends a VM's lifecycle calls `task_lifecycle.rs::settle_agents_after_release` on the same connection, so inside the releasing transaction | `api_worker_hosts.rs::forced_host_removal_settles_queued_previews_and_notices`; existing stop, loss, lease and host-event suites | Pass on the org-scoped paths. Gap on host-reported releases, see below |
| [`g5.lifecycle.deadline.notices`](../../ontology/software_architecture/lifecycle.g5.md#deliver-lifecycle-notices-with-ordinary-input): withdraw undelivered notices on release | Same settle function on every release that ends a VM's lifecycle | Same test asserts the queued notice is withdrawn with reason `host_force_terminated` | Pass on the org-scoped paths. Same gap |
| [`g5.preview.service.replace`](../../ontology/features/preview_terminal.g5.md#stop-the-previous-preview-before-replacing-it): no automatic restart after a loss | Queued previews are cancelled and the preview intent settled on every loss path that settles | Same test asserts the preview is cancelled and its intent settled | Pass on the org-scoped paths. Same gap |
| [`g5.architecture.placement`](../../ontology/software_architecture/worker_host.g5.md#capacity-and-runtime-placement): WorkerHostService does not decide task outcomes | Forced loss moved out of WorkerHostService; `release_placement` keeps its operation and runs the module's SQL without settling, because the VM never started | Inspection; `api_worker_hosts.rs` placement tests | Pass |
| [`g5.design.transaction-composition`](../../ontology/concepts/system_design_principles.g5.md#compose-owners-without-splitting-atomic-effects): no new commit points | Each caller keeps its transaction; the module takes the caller's connection and never commits | Inspection of every call site | Pass |
| Plan: differences kept, not merged | Per-cause status, loss, error, start-intent and retained-root expressions copied verbatim; the differences are listed in the plan's PR 3 section | Statement-by-statement comparison with the removed SQL | Pass |
| Writer guard | `core_table_writers_guard.rs` declares the new file as a lifecycle owner of `workspace_vms` (56 writers, 15 exceptions) | The guard found all eight statements in the new file before the entry was added | Pass |

**Independent review (Codex, round 1).** One P2: the new metadata claimed every loss settles Agents. Host control runs on the system connection (`g5_runtime_system`), and `tasks` and `chat_messages` enforce `org_id = ANY(authz.current_org_ids())`, which is empty without a scoped session, so the host-reported releases' settlement matches no row. It did before this PR too: the removed host-event code called the same helpers on the same connection. Confirmed from the policies and the role memberships. Fixed by stating where settlement takes effect; the root fix is a follow-up.

**Before and after.** The new test failed on PR 2's head: the VM was marked failed, but the preview and its intent stayed `queued`. It passes after the change.

**Mutation runs** (each applied alone, the named test run, then restored): removing the settle step from forced loss fails `forced_host_removal_settles_queued_previews_and_notices`; removing it from lease expiry fails `api_workspace_vms.rs::lease_expiry_releases_a_pre_command_stop_with_recorded_loss`.

**Validation.** 17 database-backed and static `g5-api` suites (230 tests), 97 core unit tests for the touched modules, workspace-wide `cargo clippy --all-targets -D warnings` and `cargo fmt --check`.

**Limits.** The host-event, failed-start and completed-stop releases moved unchanged; whether each one's settlement is asserted depends on the existing suites, and the mutation runs covered forced loss and lease expiry only. CI's end-to-end lane and a QA deploy have not run on this branch yet.

## PR 4 — Task and inbox rows only through TaskService

Self-review by the implementing agent, before the independent Codex review recorded in the PR. Base: PR 3's branch at `ad85b217`. Draft G5Labs/G5-Lean#201 was not used, by the human's decision.

| Commitment | Actual path | Check | Result |
|---|---|---|---|
| [`g5.architecture.allocation_composition`](../../ontology/software_architecture/agents.g5.md#compose-allocation-through-the-owning-services) and [`g5.architecture.job_owner`](../../ontology/software_architecture/agents.g5.md#jobs-compose-ordinary-tasks): JobService supplies inputs without raw session, task or inbox writes | `agent_dispatch.rs::allocate_job_agent_task` / `allocate_job_command_task` (TaskService) compose `chat_service.rs::create_workflow_session`; JobService keeps `jobs` and dependency edges | Writer guard: `job_service.rs` no longer writes `tasks`, `session_inputs` or `chat_sessions` | Pass |
| [`g5.agents.allocation.input-identity`](../../ontology/features/agents.g5.md#bind-initial-workflow-input-to-its-task): the first workflow input is bound to its task before commit | The allocation inserts the input with `task_id` after the task | `api_provider_accounts.rs::profile_workflow_agent_task_is_dispatchable_once_its_job_commits` | Pass. Failed before the change: the input had no `task_id` and dispatch reserved nothing |
| [`g5.agents.bootstrap.atomic`](../../ontology/features/agents.g5.md#commit-the-whole-bootstrap-graph-atomically): one transaction for receipt, sessions, tasks and edges | Every node allocated on JobService's connection; no commit inside the allocation | `api_provider_accounts.rs::bootstrap_graph_submission_allocates_each_node_through_task_service` (conversation, bound input and edges per node) | Pass |
| [`g5.agents.bootstrap.replay`](../../ontology/features/agents.g5.md#replay-a-bootstrap-graph-by-canonical-body) | Unchanged receipt path | Same test replays the body | Pass |
| [`g5.architecture.task_owner`](../../ontology/software_architecture/agents.g5.md#one-logical-task-owner): stop interrupt markers through TaskService | `task_lifecycle.rs::mark_running_tasks_interrupted` / `commit_task_interrupts`, statements moved verbatim | Stop suites (`api_workspace_vms`, `api_chat_stop_before_acceptance`, `api_workspace_approvals`) | Pass |
| [`g5.architecture.conversations`](../../ontology/software_architecture/agents.g5.md#shared-conversations-and-inbox): TaskService owns the inbox | `agent_dispatch.rs::set_queued_input_artifacts`; the conflict task's name passes through `EnqueueAgentInput::task_name` | Chat and conflict-resolution suites | Pass; the conflict flow now asserts the task's name |
| [`g5.agents.allocation.specification`](../../ontology/features/agents.g5.md#validate-task-specifications-at-allocation) | Not changed: the allocation writes the specifications JobService builds without a structural validator | — | Gap retained in the metadata |
| [`g5.design.transaction-composition`](../../ontology/concepts/system_design_principles.g5.md#compose-owners-without-splitting-atomic-effects) | Every moved write runs on the caller's connection | Inspection of each call site | Pass |

Values kept per path: a job's own task keeps `max_retries` 2 (the column default it always got), graph nodes keep 0; graph sessions and tasks are created by the workspace owner, now taken from the claims the locked authorization row verified.

**Mutation runs** (each applied alone, the named test run, then restored): inserting the workflow input without `task_id` fails both new allocation tests; ignoring `EnqueueAgentInput::task_name` fails the conflict-resolution flow.

**Validation.** 20 database-backed and static `g5-api` suites (279 tests), 46 core unit tests for the touched modules, workspace-wide `cargo clippy --all-targets -D warnings` and `cargo fmt --check`. Writer guard: 50 writers, 9 cross-owner exceptions.

**Limits.** The allocation adds no structural validation of the specifications JobService builds (`g5.agents.allocation.specification` keeps its gap). Workflow agent tasks were never dispatched before this change, so their end-to-end execution (provider run, settlement, job completion) is exercised here only up to reservation; CI's end-to-end lane and a QA deploy have not run on this branch.

## PR 5 — Transcript and usage through their owners

Self-review by the implementing agent, before the independent Codex review recorded in the PR. Base: PR 4's branch at `d58bdb5e`.

| Commitment | Actual path | Check | Result |
|---|---|---|---|
| [`g5.architecture.conversations`](../../ontology/software_architecture/agents.g5.md#shared-conversations-and-inbox): other owners decide, ChatService writes transcript and notice rows in the caller's transaction | `chat_service.rs` gains the transcript and notice operations; `task_lifecycle.rs` (12 functions), `agent_dispatch.rs::reserve_system_notices`/`session_title_wanted` and `workspace_vm_service.rs` (deadline nudge, aborted stop) call them on their own connection. Statements moved unchanged; TaskService keeps every decision (statuses, collected content, notice budget, when to withdraw) | Writer guard: ChatService is the only `chat_messages` writer; besides it, `chat_attachments` has the attachment-retention owner and `chat_sessions` has SpendService's usage counters and their repair (`usage_aggregation.rs`); transcript suites in `api_task_lifecycle` | Pass |
| ChatService's title fragment | `PLATFORM_OWNED_TITLE_SQL` is private; its one outside read (`session_title_wanted`) moved into ChatService | Compiles; title tests | Pass |
| [`g5.architecture.usage_owner`](../../ontology/software_architecture/platform.g5.md#usage-and-spend-limits): SpendService records usage | `spend_service.rs::record_agent_turn_usage` holds the turn writer's usage-row and counter statements unchanged; the provider session id and manifest generation go to `chat_service.rs::record_provider_session_state` | `api_task_lifecycle.rs::completed_turn_records_usage_counters_and_provider_session_state`, written and passing on PR 4's head first | Pass |
| Plan: keep one usage implementation | The two writers differ in cost type, session cost counter, total-token rule and validation, so the turn writer moved as it is (the plan's stated fallback); `record_usage` keeps no caller | Comparison of the two functions | Reported, not merged |
| [`g5.design.transaction-composition`](../../ontology/concepts/system_design_principles.g5.md#compose-owners-without-splitting-atomic-effects) | Every moved statement runs on the caller's connection; the session row is still locked first in the outcome path | Inspection of each call site | Pass |

**Statement split.** The outcome path's single `chat_sessions` update became two: ChatService sets the provider session id, generation and `updated_at` every time, and SpendService adds the token counters when the turn reported usage. Adding zero tokens was the only effect of the old statement's counter part on a turn without usage, so the rows end up the same.

**Mutation runs** (each applied alone, the named tests run, then restored): dropping the session token counters from the SpendService turn writer, or the provider session id from ChatService's settled-turn update, fails the new usage test; ignoring the final status in ChatService's assistant settlement fails two existing transcript tests in `api_task_lifecycle`.

**Validation.** 21 database-backed and static `g5-api` suites (297 tests), 77 core unit tests for the touched modules, workspace-wide `cargo clippy --all-targets -D warnings` and `cargo fmt --check`. Writer guard: 43 writers, 2 cross-owner exceptions (`routes/workspaces.rs` on `workspaces` and `doc_service.rs` on `jobs`, both left out of the plan on purpose).

**Independent review (Codex, round 1).** Approved with no findings. One follow-up narrowed this record's writer-guard check: SpendService and its usage repair still write `chat_sessions` counters, and attachment retention still deletes from `chat_attachments`.

**Limits.** The host-reported release gap recorded for PR 3 still applies: ChatService's notice withdrawal runs on host control's system connection there and matches no row. CI's end-to-end lane and a QA deploy have not run on this branch.
