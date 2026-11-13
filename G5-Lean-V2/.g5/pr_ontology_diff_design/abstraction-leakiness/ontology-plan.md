# Abstraction leakiness — five-PR refactor plan

This plan picks the critical, cheap fixes from an audit of the 2026-10-01 abstraction leakiness review (kept outside the repository) and fits them into the stated budget: five PRs, each under 5,000 changed lines. It is a refactor toward ownership the ontology already states; it adds no product behavior, no schema change and no new stored state.

How to read it: [PR sequence](#pr-sequence) is the plan itself. [Outcome and scope](#outcome-and-scope) says how the five were chosen and what "done" measures. [Left out on purpose](#left-out-on-purpose) lists what the budget does not cover. All code facts were re-checked on `main` at `8a3ec325`.

## Ontology changes and governance

**Comparison base:** `origin/main` at `8a3ec325`.

**Ontology changes:** two small edits; existing ontology covers everything else.

- Added `g5.architecture.commands.writers` in `software_architecture/boundaries.g5.md`: core tables get a declared writer list checked by a source guard. This is the one new mechanism in the plan.
- Clarified `g5.architecture.conversations` in `software_architecture/agents.g5.md` with one sentence and one table cell: other owners decide a delivery or notice, ChatService writes the transcript rows in their transaction. Its metadata now records where the code differs.

```diff
diff --git a/.g5/ontology/software_architecture/agents.g5.md b/.g5/ontology/software_architecture/agents.g5.md
@@ -19 +19 @@
-| ChatService | Session create/read/fork/search | [Conversation owner](#shared-conversations-and-inbox) |
+| ChatService | Session create/read/fork/search; transcript and notice rows in the caller's transaction | [Conversation owner](#shared-conversations-and-inbox) |
@@ -59,0 +60,2 @@
+When a delivery, outcome or lifecycle notice adds to a conversation, its owner decides it
+and ChatService writes the transcript rows in that caller's transaction.
@@ -164,2 +166,2 @@
-- [Agent API §2.2 and §8](../../../docs/agent-api-surface-v9.md#22-logical-task-conversation-and-input-provenance); agent clarification from the authorized review preserves ChatService session ownership inside composed allocation.
-- ~70% of inspected conversation paths · `backend/g5-api-core/src/services/chat_service.rs`, `agent_dispatch.rs`; frozen recovery prompt, delivered-history fallback, continuation provenance and provider-neutral fork copy are implemented. Workflow edit/withdraw and composed session creation remain unassessed.
+- [Agent API §2.2 and §8](../../../docs/agent-api-surface-v9.md#22-logical-task-conversation-and-input-provenance); agent clarification from the authorized review preserves ChatService session ownership inside composed allocation. The transcript-writer sentence is an agent clarification for the [leakiness refactor request](../../chat_intent/abstraction-leakiness/request.md).
+- ~70% of inspected conversation paths · `backend/g5-api-core/src/services/chat_service.rs`, `agent_dispatch.rs`; frozen recovery prompt, delivered-history fallback, continuation provenance and provider-neutral fork copy are implemented. Workflow edit/withdraw and composed session creation remain unassessed. Transcript and notice rows are still written outside ChatService: `task_lifecycle.rs` (20 statements), `agent_dispatch.rs::reserve_system_notices` and `workspace_vm_service.rs::append_deadline_stop_nudge`; ChatService in turn writes the inbox row's `artifacts` (`persist_inline_session_input_attachments`). Source-counted on `main` at `8a3ec325`; the percentage is the retained baseline, not a reassessment.
diff --git a/.g5/ontology/software_architecture/boundaries.g5.md b/.g5/ontology/software_architecture/boundaries.g5.md
@@ -25,0 +26,12 @@
+### Declare table writers and check them
+
+[D · g5.architecture.commands.writers]
+
+Any module holding a database connection can run SQL, so Rust visibility cannot fence
+table writes. A source guard therefore lists the modules that may write each core
+platform table (workspace, VM, task, job, conversation and usage records) and fails on
+any other writer; a cross-owner entry is a named exception with its reason. A database
+role per owner was rejected because owners must write inside one caller transaction.
+
+Enforces [complete operations](#keep-complete-operations-with-their-state-owner) under [transaction composition](../concepts/system_design_principles.g5.md#compose-owners-without-splitting-atomic-effects).
+
@@ -57,0 +70,6 @@
+### g5.architecture.commands.writers
+
+- agent
+- Agent design for the [leakiness refactor request](../../chat_intent/abstraction-leakiness/request.md); the [refactor plan](../../pr_ontology_diff_design/abstraction-leakiness/ontology-plan.md) records the alternatives.
+- Not implemented · existing guards cover route SQL by file (`backend/g5-api-routes/src/routes/guards.rs`) and the remote-integration tables (`backend/g5-api/tests/workspace_branch_resolution_guard.rs`); no guard lists the writers of the core tables.
+
```

**Relevant and governing ontology:**

| Block | Why it is relevant | What implementation must enforce |
|---|---|---|
| [`g5.architecture.commands`](../../ontology/software_architecture/boundaries.g5.md#keep-complete-operations-with-their-state-owner) | Says every entry path uses the owner's operations and that "access and dependency boundaries should prevent such bypasses". Its metadata rates this at ~50%. | PRs 2–5 replace raw cross-owner writes with owner operations. PR 2 uses Rust visibility where it can reach. |
| [`g5.architecture.commands.writers`](../../ontology/software_architecture/boundaries.g5.md#declare-table-writers-and-check-them) (new) | Rust visibility cannot fence SQL strings. | PR 1 adds the guard. Each later PR deletes the exceptions it closes; the guard fails if a listed file stops writing, so the list cannot go stale. |
| [`g5.architecture.api_layers`](../../ontology/software_architecture/boundaries.g5.md#api-adapters-depend-on-domain-operations) | "Adapters translate requests and errors; domain owners decide authorization-sensitive transitions." Routes hold 43 raw SQL calls today. | PR 1 freezes route SQL at today's 27 functions. PR 2 moves the bootstrap authorization and the host-drain send out of routes. |
| [`g5.architecture.runtime_facade`](../../ontology/software_architecture/workspace.g5.md#one-live-runtime-facade), [`g5.design.minimal-api`](../../ontology/concepts/system_design_principles.g5.md#expose-minimal-sufficient-apis) | The facade's listed surface is typed operations with an authorized actor. The generic `call` is not in it, and its metadata already flags "low-level `call` consumers". | PR 2: no code outside `g5-api-core` can make a lifecycle-authority request, call a dispatcher method by name or open a raw VM stream. |
| [`g5.agents.bootstrap`](../../ontology/features/agents.g5.md#bootstrap-submits-one-atomic-dependency-graph), [`.atomic`](../../ontology/features/agents.g5.md#commit-the-whole-bootstrap-graph-atomically) | Graph submission needs current-incarnation admission and one transaction for receipt, sessions, tasks and edges. | PR 2 keeps the same admission query, inside JobService. PR 4 keeps one transaction and no per-task commit. |
| [`g5.lifecycle.stop.release`](../../ontology/software_architecture/lifecycle.g5.md#release-only-the-stopped-incarnation), [`g5.lifecycle.deadline.notices`](../../ontology/software_architecture/lifecycle.g5.md#deliver-lifecycle-notices-with-ordinary-input), [`g5.preview.service.replace`](../../ontology/features/preview_terminal.g5.md#stop-the-previous-preview-before-replacing-it) | Release must "atomically settle Agents release", withdraw undelivered notices, and not restart a preview automatically after a loss. Forced host termination does none of this today. | PR 3: every release path, including forced termination, settles Agents through one function in the same transaction. |
| [`g5.architecture.placement`](../../ontology/software_architecture/worker_host.g5.md#capacity-and-runtime-placement) | WorkerHostService offers `place`, `bind_boot_record`, `release_placement`, `request_drain`, "without … deciding task outcomes". `record_forced_vm_loss` is outside that surface. | PR 3 moves forced loss to the lifecycle owner. PR 2 puts the drain send behind the placement owner. |
| [`g5.architecture.allocation_composition`](../../ontology/software_architecture/agents.g5.md#compose-allocation-through-the-owning-services), [`g5.architecture.job_owner`](../../ontology/software_architecture/agents.g5.md#jobs-compose-ordinary-tasks) | JobService must supply inputs "without raw session, task or inbox writes". Twelve metadata entries across the ontology name this gap. | PR 4: JobService calls TaskService allocation, which composes ChatService session creation. |
| [`g5.architecture.conversations`](../../ontology/software_architecture/agents.g5.md#shared-conversations-and-inbox) | ChatService owns history and attachments; TaskService owns the inbox. | PR 5 moves transcript and notice rows behind ChatService. PR 4 moves the one inbox write out of ChatService. |
| [`g5.architecture.usage_owner`](../../ontology/software_architecture/platform.g5.md#usage-and-spend-limits) | SpendService "supplies usage recording", but `record_usage` has no caller and TaskService writes `usage_records` itself. | PR 5: one usage writer, owned by SpendService. |
| [`g5.design.transaction-composition`](../../ontology/concepts/system_design_principles.g5.md#compose-owners-without-splitting-atomic-effects) | Owners may join a caller's transaction while keeping their own validation and writes. | PRs 3–5 move code, not commit points: no new transaction, no new commit, no reordering of locks. |
| [`g5.quality.evidence`](../../ontology/concepts/system_design_principles.g5.md#test-real-boundaries-and-report-exact-evidence) | Regression tests exercise real authority boundaries. | New tests run at the API boundary against PostgreSQL, not against mocks of the owner being moved. |

## Outcome and scope

**Requested outcome:** reduce leakiness with at most five PRs under 5,000 changed lines each, fixing critical leaks only.

**How the five were chosen.** A leak is in scope when it is critical and cheap:

- *Critical* means it can cause wrong behavior or wrong authority today, or it lets new leaks appear unnoticed.
- *Cheap* means small, mostly moving existing code, and checkable by existing tests plus one or two new ones.

**What "done" measures** (writers counted in production code only, the way PR 1's guard counts them):

| Measure on `main` at `8a3ec325` | Today | After the plan |
|---|---|---|
| Cross-owner writer entries on the 12 core tables (file writes a table another service owns) | 15 files, 39 statements | 2 files, 5 statements |
| Public entry points that let a route skip an owner | 5 | 0 |
| VM failure paths that skip the Agents settle step | 1 of 6 (forced host termination) | 0 |
| Route functions allowed to run raw SQL | any function in 5 files | 26 named functions |
| Guard that fails on a new cross-owner table write | none | yes |

The 15 entries, by table:

| Table | Cross-owner writers today (statements) | Closed by |
|---|---|---|
| `tasks` | `job_service.rs` (2), `workspace_vm_service.rs` (2), `git_conflict_resolution.rs` (1) | PR 4 |
| `session_inputs` | `job_service.rs` (2), `chat_service.rs` (1) | PR 4 |
| `chat_sessions` | `job_service.rs` (2), `task_lifecycle.rs` (6), `workspace_vm_service.rs` (1) | PR 4, PR 5 |
| `chat_messages` | `task_lifecycle.rs` (12), `agent_dispatch.rs` (1), `workspace_vm_service.rs` (1) | PR 5 |
| `chat_attachments` | `task_lifecycle.rs` (2) | PR 5 |
| `usage_records` | `task_lifecycle.rs` (1) | PR 5 |
| `jobs` | `doc_service.rs` (3) | left |
| `workspaces` | `routes/workspaces.rs` (2) | left |

`workspace_vms`, `worker_hosts`, `task_attempts` and `task_dependencies` have no cross-owner writer at file level: every writer is an owner the ontology names for those columns. Their one real problem, forced loss, is a statement inside a sanctioned file (PR 3).

## High-level design

```text
Leak type                         Fence                           PR
--------------------------------  ------------------------------  ------
Any new cross-owner table write   source guard, declared writers  1
Any new SQL in a route            source guard, named functions   1
Public hatch reachable by routes  Rust visibility (pub(crate))    2
Copied release SQL, one drifted   one release module + settle     3
Raw write to another owner's      owner operation, called inside  4, 5
table                             the caller's transaction
```

Three mechanisms, each used where it actually binds:

1. **Rust visibility** where the bypass is a Rust item. `g5-api-routes` is a separate crate from `g5-api-core`, so `pub(crate)` on an item in core is a compile-time fence against every route.
2. **A source guard** where the bypass is a SQL string, which visibility cannot see. The guard reads production SQL literals the way `workspace_branch_resolution_guard.rs` already does, and checks each core table's writers against a declared list. Today's cross-owner writers start on the list as named exceptions, so the PR changes no production code; later PRs delete their exceptions.
3. **Owner operations joined to the caller's transaction** for the writes that move. The caller keeps deciding when; the owner keeps the SQL. Nothing commits earlier or later than today.

Rejected: a `pub(crate)` data-access module per owner (it cannot stop SQL strings elsewhere), and a database role per owner (owners must write inside one shared transaction).

## PR sequence

Do them one at a time, in this order; each starts after the previous one has merged. Sizes are changed lines (additions plus deletions), estimated from reading the code, not from a prototype.

| PR | Title | Closes | Size |
|---|---|---|---|
| 1 | Freeze: declared table writers and route SQL by function | No guard on table writes; route SQL exempt by whole file; stale guards | ~1,000, tests only |
| 2 | Compile-time fences for the runtime facade, host commands and bootstrap authority | 5 public hatches | ~1,400 (two new API tests and the contract test) |
| 3 | One home for VM release; forced loss settles Agents | Copied release SQL; forced-loss gap | ~1,000 |
| 4 | Task and inbox rows only through TaskService | 6 cross-owner entries | ~1,500 |
| 5 | Transcript and usage through their owners | 7 cross-owner entries | ~2,000 |

**Why this order.**

- PR 1 first: it is test-only, and every later PR then proves its progress by deleting its exceptions from the guard's list.
- PR 2 before PR 4: both edit `job_service.rs::submit_bootstrap_task_graph`. PR 2's edits are many but small, so it lands before the larger moves.
- PR 3 before PR 5: PR 3 creates the function that settles Agents after a release; PR 5 then moves the notice part of it behind ChatService.
- PR 4 before PR 5: both rewrite parts of `task_lifecycle.rs` and `chat_service.rs`, and PR 5 builds on PR 4's allocation and inbox operations.

**Rollout.** Kept simple by decision: QA has no active users and manual intervention is acceptable. Merge, let QA redeploy, then send one chat message and start and stop one workspace. Nothing in the five PRs needs a migration, a configuration change or a new worker image. Two constraints keep it that way:

- PR 2 must send VMs exactly the request context it sends today. The guest dispatcher rejects unknown fields (`g5d-dispatch/src/protocol.rs`, `deny_unknown_fields`), so an added key would fail every runtime call on VMs running an older image.
- New guard tests go under `backend/g5-api/tests/`. A change under `backend/g5d*/` or to `Cargo.lock` counts as a worker-image input and would add an image rebuild to the deploy.

### PR 1 — Freeze: declared table writers and route SQL by function

Test code only; no production behavior changes.

- **Table-writer guard.** A new test in `backend/g5-api/tests/` lists, for each of 12 core tables, the files that may write it: `workspace_vms`, `workspaces`, `worker_hosts`, `tasks`, `task_attempts`, `task_dependencies`, `session_inputs`, `jobs`, `chat_sessions`, `chat_messages`, `chat_attachments`, `usage_records`. The list starts as today's 55 writer entries; the 15 cross-owner ones are marked as exceptions with a reason. The test fails on an unlisted writer and on a listed file that no longer writes.
- **Route SQL by function.** The existing guard in `routes/guards.rs` exempts five whole files and attributes each call to the last function declaration it saw. Replace it with a test in `backend/g5-api/tests/` that uses a Rust parser to find each call's enclosing function, and allow only the 27 functions that hold the 43 raw SQL calls today, keeping each reason. The exemption for `force_complete_task`, a function that no longer exists, goes away.
- **Repair one vacuous check.** `route_auth_static.rs` checks that `push_to_remote` authorizes before `INSERT INTO branch_integration_queue`, SQL that no longer exists; the helper passes when the statement is missing. Point it at the current enqueue call and make a missing target fail.
- **Evidence:** both guards carry self-tests with a passing and a failing fixture, as the existing guard does.
- **Known limit:** the guard sees SQL literals, not SQL assembled with `format!`. No write to these tables is assembled that way today.

### PR 2 — Compile-time fences for the runtime facade, host commands and bootstrap authority

Five hatches, all closed with visibility, so the compiler refuses the bypass:

| Hatch today | Change |
|---|---|
| `RuntimeRequest` has public fields. `lifecycle: true` skips the facade's owner check (`workspace_runtime.rs`, `OWNER_REQUIRED`), and so does any `inspection_grant_id`. The guest dispatcher still applies its per-method rule, so this reaches the private lifecycle methods (admission, flush, event acknowledgment) and six diagnostic reads, not owner file access. | Private fields. A public owner constructor built from the runtime handle; a crate-private lifecycle constructor that names the exact incarnation it targets. The unvalidated inspection-grant field goes until the facade validates grants; the context still sends `inspection_grant_id: null`, so the wire is unchanged. The 24 construction sites use the two constructors. |
| `WorkspaceRuntime::call` is public and takes the dispatcher method as a string. | Crate-private. Its 7 outside callers are all in core (fork 3, lifecycle 4). |
| `HostTunnel::open_workspace_rpc` and `open_workspace_stream` are public and check nothing. | Crate-private. Only `workspace_runtime.rs` calls them. |
| `HostCommandClient::execute` is public; `routes/worker_hosts.rs::deliver_host_drain` sends `host.drain` itself, a copy of the loop in `maintenance/host_registry.rs`. | Crate-private. One drain-delivery function in the placement owner (`WorkerHostService::deliver_drain`), called by the route and the loop with their current wait and retry counts. Its public form takes the record `request_drain` returns, so a route cannot drain a host without recording the drain. |
| `BootstrapGraphAuthority` has public fields; `routes/internal.rs::lock_and_authorize_bootstrap_task_mutation` runs the authorization query and builds it. | JobService takes the incarnation claims and authorizes inside `submit_bootstrap_task_graph`; the type becomes private. The query moves unchanged; splitting out its VM-liveness part belongs to the admission follow-up. |

- **Evidence:** the compiler is the fence. A source-contract test (`crate_private_fences.rs`) pins these declarations as crate-private and `RuntimeRequest`'s fields as private, so they cannot be widened unnoticed (CI ignores doctest failures, so compile-fail doctests would not gate). The static route tests that name the moved function (`route_auth_static.rs`, `selected_org_scope_audit.rs`) move to the new seam without weakening what they assert. Two integration tests reached a fenced call directly: `api_workspace_vms.rs` with a lifecycle `call` and `api_worker_hosts.rs` with a host command. They switch to `test-helpers` entries, which the contract test keeps out of production code. Two new API tests cover the moved code: drain delivery over a live tunnel from the route and the reconciler, and bootstrap graph authorization before the graph is validated.
- **Guard list:** one route function leaves PR 1's list.

### PR 3 — One home for VM release; forced loss settles Agents

- **Today.** Eight statements in three files clear a VM's custody columns (`host_control_state.rs` 3, `worker_host_service.rs` 2, `workspace_vm_service.rs` 3). Six record a failure or loss, one is the clean stop and one returns a refused placement to the queue. Five of the six failure paths then cancel queued previews and withdraw queued notices. The sixth, `record_forced_vm_loss`, runs when an operator force-deletes a host with live VMs; it does neither, and no test covers `force=true`.
- **Change.** Release SQL lives in one module of the lifecycle owner (`services/workspace_vm_release.rs`), with the custody column list written once. "Settle Agents after a release" becomes one TaskService function (`task_lifecycle.rs::settle_agents_after_release`) that every release ending a VM's lifecycle calls; a refused placement does not, because its VM never started and its work waits for the next placement. Forced loss leaves WorkerHostService and becomes a lifecycle loss path that settles with reason `host_force_terminated`. `host_control_state.rs`, the lifecycle service and the deallocate route call the release functions inside their own transactions; `WorkerHostService::release_placement` keeps its operation and runs the module's SQL.
- **Differences between the copies** are kept as explicit per-cause expressions or reported, never silently merged. Kept and reported:
  - lease expiry is the only release that keeps `start_deadline_at`, and it completes a stopping VM whether or not the stop command reached the host;
  - the loss timestamps differ by cause;
  - a host-reported exit of a stopping VM settles as a loss (`vm_lost`, queued previews cancelled), while a stop the API completes only withdraws notices;
  - a failed start and a completed stop settle Agents even when their fenced update matched no row.
- **Evidence:** a new API test force-deletes a host that holds a running VM with a queued preview and a queued notice, and asserts both are settled. It failed before the change (the preview and its intent stayed queued) and passes after it. Existing stop, loss and lease tests pass unchanged.
- **Found in review.** The host-reported releases (inventory missing, exit, out of memory, stale kill, boot failure) settle on host control's system connection, where row-level security hides tasks and chat messages, so their settlement matched no row before this PR and still does. Fixing it changes host control's database authority, so it is a follow-up rather than part of this refactor.

### PR 4 — Task and inbox rows only through TaskService

- **Job allocation.** `job_service.rs` creates session, input and task rows with raw SQL in `create_shared_agent_session_job` and `submit_bootstrap_task_graph`. Replace with TaskService's job allocation (`agent_dispatch.rs::allocate_job_agent_task` and `allocate_job_command_task`), which composes ChatService's workflow session creation (`chat_service.rs::create_workflow_session`) inside JobService's transaction. JobService keeps writing `jobs` and the dependency edges. The overlapping draft G5Labs/G5-Lean#201 is not used: the human decided to close it ([request](../../chat_intent/abstraction-leakiness/request.md)).
- **Stop markers.** `workspace_vm_service.rs::quiesce_agent_tasks` writes interrupt markers into `tasks.spec` twice. Move both statements into TaskService stop-and-hold functions.
- **Two one-line writes.** `git_conflict_resolution.rs` renames its task with a raw `UPDATE`; pass the name through `enqueue_agent_input`. `chat_service.rs::persist_inline_session_input_attachments` updates `session_inputs.artifacts`; make it a TaskService inbox operation.
- **Found while implementing.** JobService inserted each workflow agent task's first input without `task_id`, and dispatch reserves an agent task only through a bound input, so a profile workflow's agent task and every bootstrap graph child could not be dispatched. The allocation binds the input, as `g5.agents.allocation.input-identity` requires.
- **Evidence:** a new API test creates a profile workflow job and reserves its agent task; it failed before the change. Another submits a bootstrap graph through `POST /api/internal/jobs/:id/tasks`, checks each node's conversation, bound input and edges, and replays it. Stop, transfer and chat tests pass unchanged.
- **Guard list:** `tasks` and `session_inputs` lose all 5 exceptions; `chat_sessions` loses 1.

### PR 5 — Transcript and usage through their owners

- **Transcript.** ChatService gains transcript and notice operations that run in the caller's transaction. They replace 20 statements in 11 functions of `task_lifecycle.rs` (input acceptance, title, provider session, assistant message refresh, streaming mark, settle, final message, auth failure, notice withdrawal, session outcome), `agent_dispatch.rs::reserve_system_notices` and `workspace_vm_service.rs::append_deadline_stop_nudge`. Several of these are already whole chat-only functions and move unchanged. ChatService stops exporting its SQL fragment `PLATFORM_OWNED_TITLE_SQL`, and its module comment ("owns all database queries") becomes true for writes.
- **Usage.** `task_lifecycle.rs::apply_agent_session_outcome` inserts `usage_records` and updates the counters on `tasks`, `jobs` and `chat_sessions`. `SpendService::record_usage` writes the same four tables and has no caller. Keep one implementation in SpendService and call it from TaskService in the same transaction. The same TaskService statement also stores the provider session id on `chat_sessions`; that part goes to ChatService.
  - As built: the two writers differ (`record_usage` uses decimal cost, adds cost to the session, leaves cache tokens out of the total and validates request types), so the turn writer's statements moved unchanged into `spend_service.rs::record_agent_turn_usage`. `record_usage` is left as it is: it has no caller, and removing it is outside this plan.
- **Evidence:** existing task-lifecycle and conversation API tests pass unchanged. One new test, written and passing before the move, pins the usage row, the task, job and session counters and the provider session state of a completed turn; it passes unchanged after the move.
- **Guard list:** the remaining 7 exceptions on `chat_*` and `usage_records` go.

## Left out on purpose

These are real, and the budget does not cover them. PR 1 freezes the first four so they cannot grow.

1. **Route liveness and admission SQL.** `routes/internal.rs` and `routes/git.rs` each run their own "is this VM live" query; `codex_auth.rs`, the flush service and dispatch have three more. Fixing it needs one admission reader and touches push and internal-token security.
2. **Route settings write** (`set_workspace_push_mode`).
3. **Worker-host route reads** (3 functions) and the documents and ontology writes in `routes/internal.rs` (9 statements).
4. **`doc_service.rs` writing `jobs.target_doc_id`** (3 statements).
5. **Git:** the raw `git2` ref write in `git_conflict_resolution.rs`, raw `git2` reads, three GitHub REST client stacks.
6. **Preview and terminal:** shared wire types for the port prefix and terminal frames, hand-built preview URLs, the re-auth scope drift.
7. **Status enums and a single VM transition owner.** `TaskStatus` and `JobStatus` match the database but are unused; VM status has three writers by design.
8. **Read-side leaks.** Other services read chat tables directly (12 files at the audited commit); the guard covers writes only.
9. **Frontend:** direct `api.*` calls outside hooks, and the job-task polling bug.

## Observable acceptance and risks

**Acceptance**

- After PR 1, a new `UPDATE tasks` in an unlisted file, or a new `sqlx::query` in an unlisted route function, fails the Rust test lane.
- After PR 2, a route that builds a lifecycle request, calls `WorkspaceRuntime::call`, opens a VM stream, sends a host command or builds a `BootstrapGraphAuthority` does not compile.
- After PR 3, force-deleting a host leaves no queued preview and no queued notice for its workspaces.
- After PR 5, the guard list holds 2 cross-owner exceptions, down from 15.
- Every PR: no migration, no API or wire change, and the existing Rust and end-to-end lanes pass. Each PR runs its own ontology review and updates the metadata of the blocks it closes.

**Risks**

- **Open PRs touch the same files.** [G5Labs/G5-Lean#225](https://github.com/G5Labs/G5-Lean/pull/225) changes `agent_dispatch.rs` (386 lines) and `task_lifecycle.rs` (87); [G5Labs/G5-Lean#286](https://github.com/G5Labs/G5-Lean/pull/286) changes `routes/internal.rs` (97) and `agent_dispatch.rs`. Whichever side lands second rebases; the overlaps are small.
- **PR 3 edits the most delicate SQL in the plan.** The mitigation is to keep each cause's expressions verbatim, move statements before merging any, and rely on the lifecycle test suite plus the new forced-loss test.
- **PRs 4 and 5 claim to preserve behavior.** That rests on existing test coverage of allocation and transcript paths. The `g5.architecture.task_owner` metadata already says "assistant-message settlement can diverge"; moving that code must neither hide nor quietly change it.
- **Allocation draft.** Decided on 2026-10-03: the human is closing the overlapping draft G5Labs/G5-Lean#201, and PR 4 does not build on it.

## Review summary

Assumptions challenged before handoff, and what is still uncertain:

- **Is a guard the simplest sufficient mechanism?** Checked against `g5.design.minimal-mechanism`. It adds no production state and reuses an existing test pattern. Database roles and private data modules were considered and do not work with shared transactions and SQL strings.
- **Is forced host termination covered by existing rules, or does it need a new commitment?** No block names it. `g5.lifecycle.deadline.notices` and `g5.preview.service.replace` apply to any release or loss, so the plan adds none. If the human reads forced termination as deliberately exempt, PR 3 shrinks to the shared module.
- **Does a later reconciler already settle previews and notices after forced loss?** Answered by PR 3's test: before the change, forced removal left the queued preview and its intent queued and the notice undelivered. The released row is no longer active, so the lease reconciler never revisits it, and the only other callers that cancel queued previews prepare an admitted stop or hold work for a transfer (`prepare_workspace_tasks_for_stop`, `hold_workspace_tasks_for_transfer`). PR 3 therefore fixes behavior, not only drift.
- **Reuse to verify during coding:**
  - `SpendService::record_usage` has no caller, so its validation, cost type and events may differ from the inline writer. If they do, move the inline statements into SpendService as they are.
  - The job allocation was designed from today's code, not from the draft.
  - The dispatcher design (`docs/dispatch-runtime.md`) reserves `inspection_grant_id` for read-only inspection. PR 2 keeps the wire field and only removes the unvalidated public way to set it.
- **Classification not assessed:** TaskService writing `jobs.status`, `org_service.rs` inserting the Base workspace row and the attachment-retention delete are listed as existing writers without a verdict.
- **Sizes are estimates.** The largest PR is about 2,000 lines, so each has room under the 5,000 limit.
- **Implementation reviews** are recorded per PR in [review.md](review.md).
