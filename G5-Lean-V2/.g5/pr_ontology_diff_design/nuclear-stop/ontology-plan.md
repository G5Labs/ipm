# Stop always completes

The owner wants Stop to work as an emergency option: whatever state its agents or VM are in, the workspace ends up stopped, and the next Start boots a fresh VM ([request](../../chat_intent/nuclear-stop/request.md)). Before this change, a Stop that could not save gave up and put the VM back to running. On QA on 2026-10-07, one agent journal event the API could not store (NUL bytes from a Bash tool) kept a Stop in `stopping` for ten minutes, after which it stopped retrying.

User decisions: every stop forces through with the loss recorded (idle and transfer included); the save window is two minutes; the loss is recorded as today and not shown or announced ("do NOT improve it, keep as-it, we do not need to show").

## Ontology changes and governance

**Comparison base:** `origin/main` at `82685300` (the branch started at `b4b2490a` and was rebased after #333, #335, #337 and #334 merged; the ontology changes did not conflict).

**Changes:**

- **New** `g5.lifecycle.stop.force`, "Stop always completes" (human-provided): every stop ends stopped and released. When saving cannot finish inside the window, Stop records the loss and has the host kill the VM.
- **New** `g5.lifecycle.stop.window`, "Give each stop a bounded save window" (agent): two minutes for owner, idle, transfer and ordinary Delete stops. Deadline stops keep their flush deadline, and force Delete gets no window.
- **New** story `g5.story.stop-always-completes`, "Always be able to stop" (human-provided).
- **Widened** `g5.lifecycle.deadline.loss`: it now covers every forced stop, not only deadline teardown, and states that showing or announcing the loss is not required. Moved next to the stop blocks.
- **Amended** `g5.lifecycle.stop`, `g5.lifecycle.stop.release` (30-second graceful shutdown after a complete save, kill otherwise; an unreachable host keeps the stop pending) and `g5.lifecycle.stop.bundle-exception` (it no longer guarantees task events).
- **Amended** `g5.lifecycle.transfer.failure`: a failed save no longer fails a transfer. This amends human decision H072.
- **Amended** `g5.git.flush-failure`: only a live fork still aborts on a failed capture.
- **Repurposed** `g5.git.flush-failure.block`: a deterministic capture failure ends the save window at once; the failed flush keeps its error, and the workspace keeps no block and gets no repair notice.
- **Retired** `g5.git.flush-failure.cooldown` (an idle stop no longer retries later) and story `g5.story.stop-blocked` (no stop is blocked any more, and the user does not want the loss shown).
- **Reworded** story `g5.story.accept-work-loss` ("Delete without waiting for a final save"). The line "The stop contract permits no force Stop" is removed, and the story index is updated.
- **Metadata only:** `boundaries.g5.md` now counts 22 route SQL exemptions.

v9 (`docs/workspace-vm-api-v9.md` §5, `docs/lean-backend-design-v9.md` §5.8) stays as lineage; these blocks supersede its rule that user, transfer and idle stops abort.

<details>
<summary>Ontology diff, metadata included</summary>

```diff
diff --git a/.g5/ontology/concepts/core_user_stories.g5.md b/.g5/ontology/concepts/core_user_stories.g5.md
index 1013b403..2072ecf8 100644
--- a/.g5/ontology/concepts/core_user_stories.g5.md
+++ b/.g5/ontology/concepts/core_user_stories.g5.md
@@ -32,6 +32,7 @@ These links add no separate commitments or implementation percentages.
 - [Inspect pending changes](../features/workspaces.g5.md#inspect-pending-changes).
 - [Use familiar Git tools](../features/workspaces.g5.md#use-familiar-git-tools).
 - [Stop and return to my work](../features/workspaces.g5.md#stop-and-return-to-my-work).
+- [Always be able to stop](../features/workspaces.g5.md#always-be-able-to-stop).
 - [Transfer responsibility](../features/workspaces.g5.md#transfer-responsibility).
 - [Remove a workspace](../features/workspaces.g5.md#remove-a-workspace).
 - [Share read access](../features/workspaces.g5.md#share-read-access).
@@ -40,8 +41,7 @@ These links add no separate commitments or implementation percentages.
 - [Keep the requested name](../features/workspaces.g5.md#reject-ambiguous-member-branch-names).
 - [Retry an interrupted fork](../features/workspaces.g5.md#freeze-one-recoverable-fork-operation).
 - [Understand a delayed start](../features/workspaces.g5.md#understand-a-delayed-start).
-- [Understand a blocked stop](../features/workspaces.g5.md#understand-a-blocked-stop).
-- [Choose whether to accept work loss](../features/workspaces.g5.md#choose-whether-to-accept-work-loss).
+- [Delete without waiting for a final save](../features/workspaces.g5.md#delete-without-waiting-for-a-final-save).
 - [Resume after receiving ownership](../features/workspaces.g5.md#resume-after-receiving-ownership).
 - [Reboot onto the selected image](../features/workspaces.g5.md#reboot-onto-the-selected-image).
 - [See each repository's remote-tracking state](../features/workspaces.g5.md#see-each-repositorys-remote-tracking-state).
diff --git a/.g5/ontology/features/workspaces.g5.md b/.g5/ontology/features/workspaces.g5.md
index 40aa0f72..ab15c46c 100644
--- a/.g5/ontology/features/workspaces.g5.md
+++ b/.g5/ontology/features/workspaces.g5.md
@@ -58,6 +58,14 @@ As an owner, I can stop my workspace and later start it from its last complete d
 
 Governing contracts: [stop fences future work before releasing custody](../software_architecture/lifecycle.g5.md#stop-fences-future-work-before-releasing-custody), [warm and cold restore mean the same thing](../software_architecture/durability.g5.md#warm-and-cold-restore-mean-the-same-thing).
 
+### Always be able to stop
+
+[I · g5.story.stop-always-completes]
+
+As an owner, I can always stop my workspace: Stop ends with the VM stopped within a few minutes, whatever state its agents or VM are in, and my next Start boots a fresh VM from the last saved state.
+
+Governing contracts: [stop always completes](../software_architecture/lifecycle.g5.md#stop-always-completes), [give each stop a bounded save window](../software_architecture/lifecycle.g5.md#give-each-stop-a-bounded-save-window).
+
 ### Transfer responsibility
 
 [I · g5.story.transfer-responsibility]
@@ -96,23 +104,13 @@ As an owner, I can see why my workspace is waiting to start.
 
 Governing contracts: [explicit consent owns each start](../software_architecture/lifecycle.g5.md#explicit-consent-owns-each-start).
 
-### Understand a blocked stop
-
-[I · g5.story.stop-blocked]
-
-As an owner, I can see why a failed flush prevented my workspace from stopping gracefully.
-
-Governing contracts: [stop fences future work before releasing custody](../software_architecture/lifecycle.g5.md#stop-fences-future-work-before-releasing-custody), [a durability failure preserves recoverable work](../software_architecture/durability.g5.md#a-durability-failure-preserves-recoverable-work).
-
-### Choose whether to accept work loss
+### Delete without waiting for a final save
 
 [I · g5.story.accept-work-loss]
 
-As an owner, I can explicitly force-delete a workspace when required persistence fails, accepting recorded work loss.
-
-Governing contracts: [stop fences future work before releasing custody](../software_architecture/lifecycle.g5.md#stop-fences-future-work-before-releasing-custody), [a durability failure preserves recoverable work](../software_architecture/durability.g5.md#a-durability-failure-preserves-recoverable-work).
+As an owner, I can force-delete a workspace without waiting for its final save, accepting recorded work loss.
 
-The [stop contract](../software_architecture/lifecycle.g5.md#stop-fences-future-work-before-releasing-custody) permits no force Stop.
+Governing contracts: [give each stop a bounded save window](../software_architecture/lifecycle.g5.md#give-each-stop-a-bounded-save-window), [report what a forced stop could not save](../software_architecture/lifecycle.g5.md#report-what-a-forced-stop-could-not-save).
 
 ### Resume after receiving ownership
 
@@ -576,6 +574,12 @@ The workspace switcher lists other review copies nowhere, but the copy currently
 - From [H003/H012/H013/H058/H066/H080](../../uplift/g5-lean/candidates/human-decisions.md), [v9](../../../docs/lean-backend-design-v9.md).
 - ~80% · workspace_vm_service.rs, workspace_flush_service.rs, dispatcher flush/restore; receipt persistence and explicit later Start exist; failed child cleanup can prematurely release custody, and full crash recovery is unexecuted.
 
+### g5.story.stop-always-completes
+
+- human-provided
+- The user: "the stop VM button should act as the nuclear weapon to kill all lifecycle issues and offer a fresh new restart when user hit the Start button after VM is stopped" ([request](../../chat_intent/nuclear-stop/request.md)).
+- Implemented at the API boundary · see `g5.lifecycle.stop.force` for the code, tests and the real-VM E2E that runs only in the Cloud Build e2e lane. A host that cannot be reached still keeps the stop `stopping` until the host returns or host-loss rules release the VM.
+
 ### g5.story.transfer-responsibility
 
 - human-verified
@@ -600,17 +604,11 @@ The workspace switcher lists other review copies nowhere, but the copy currently
 - From [v9](../../../docs/lean-backend-design-v9.md) §5.8.
 - ~75% · workspace_vm_service.rs, worker_host_service.rs, WorkspaceVmStatusStrip.tsx; start-intent and placement state exist; complete queue-rank, expiry and error presentation are unverified.
 
-### g5.story.stop-blocked
-
-- agent
-- From [v9](../../../docs/lean-backend-design-v9.md) §§5.4/5.8.
-- ~80% · workspace_vm_service.rs, WorkspaceHealthBanners.tsx; durable stop blockage exists; complete error rendering and crash recovery were not executed.
-
 ### g5.story.accept-work-loss
 
-- agent
-- From [v9](../../../docs/lean-backend-design-v9.md) §5.8.
-- ~75% · workspace_vm_service.rs and workspace deletion route; force Delete and loss records exist; failed root purge recovery and all retention/deletion paths remain unassessed.
+- agent, human-verified
+- From [v9](../../../docs/lean-backend-design-v9.md) §5.8. Narrowed to skipping the save window once ordinary Stop and Delete force through as well ([request](../../chat_intent/nuclear-stop/request.md)).
+- ~75% · workspace_vm_service.rs and workspace deletion route; force Delete (`StopReason::Delete` with an immediate deadline) calls the guest no more and kills the VM (`backend/g5-api/tests/api_workspace_vms.rs`: `delete_stop_settles_a_reserved_but_never_invoked_agent`, `::forced_workspace_delete_does_not_wait_out_a_new_nudge_after_flush_failure`); failed root purge recovery and all retention/deletion paths remain unassessed.
 
 ### g5.story.new-owner-resume
 
diff --git a/.g5/ontology/software_architecture/boundaries.g5.md b/.g5/ontology/software_architecture/boundaries.g5.md
index 74eef5f5..8334d593 100644
--- a/.g5/ontology/software_architecture/boundaries.g5.md
+++ b/.g5/ontology/software_architecture/boundaries.g5.md
@@ -83,7 +83,7 @@ policy or reversing dependency direction.
 
 - agent
 - [Architecture checks](../../../backend/g5d/tests/architecture/source_contracts.rs).
-- ~100% of inspected crate/import shape · `g5-api-routes` → `g5-api-core` → `g5-api-data`/`g5-api-contracts`/`g5-api-foundation`; route business orchestration and raw SQL still need semantic boundary review. Raw route SQL is limited to 23 named functions by `backend/g5-api/tests/route_sql_guard.rs`; bootstrap graph authorization and host-drain delivery moved from routes into JobService and WorkerHostService, and the flush receipt proof, VM-token checks, push permission and forced host removal into their owners. `backend/g5-api/tests/crate_private_fences.rs` also keeps host control's constructor, the ownerless stop and forced VM loss crate-private. Consolidation retains the former adapter-policy rule here; the shape estimate does not measure semantic compliance. The previously recorded owner-only formal-approval guard gap remains with `g5.governance.approval`.
+- ~100% of inspected crate/import shape · `g5-api-routes` → `g5-api-core` → `g5-api-data`/`g5-api-contracts`/`g5-api-foundation`; route business orchestration and raw SQL still need semantic boundary review. Raw route SQL is limited to 22 named functions by `backend/g5-api/tests/route_sql_guard.rs` (the operator flush-block listing was removed with the [workspace flush block](durability.g5.md#end-the-save-window-on-a-deterministic-capture-failure)); bootstrap graph authorization and host-drain delivery moved from routes into JobService and WorkerHostService, and the flush receipt proof, VM-token checks, push permission and forced host removal into their owners. `backend/g5-api/tests/crate_private_fences.rs` also keeps host control's constructor, the ownerless stop and forced VM loss crate-private. Consolidation retains the former adapter-policy rule here; the shape estimate does not measure semantic compliance. The previously recorded owner-only formal-approval guard gap remains with `g5.governance.approval`.
 
 ### g5.architecture.compatibility
 
diff --git a/.g5/ontology/software_architecture/durability.g5.md b/.g5/ontology/software_architecture/durability.g5.md
index 480b68cc..68c1f4af 100644
--- a/.g5/ontology/software_architecture/durability.g5.md
+++ b/.g5/ontology/software_architecture/durability.g5.md
@@ -83,23 +83,17 @@ The [V9 flush protocol](../../../docs/workspace-vm-api-v9.md#5-workspace-flush)
 
 Default capture limits are 100 MiB per file and 2 GiB per tree/pack, with oversize failing rather than silently excluding content.
 
-## A durability failure preserves recoverable work
+## A failed capture aborts a fork and is recorded by a stop
 
 [D · g5.git.flush-failure]
 
-Owner Stop, Delete, live fork and transfer abort when required persistence fails, except authorized deadline teardown or explicit force Delete with recorded loss.
+A live fork aborts when required persistence fails. A stop never aborts for it: [every stop completes](lifecycle.g5.md#stop-always-completes) and [records what it could not save](lifecycle.g5.md#report-what-a-forced-stop-could-not-save).
 
-## Persist deterministic durability blocks
+## End the save window on a deterministic capture failure
 
 [D · g5.git.flush-failure.block]
 
-A deterministic durability failure persists its fingerprint and one owner notice until repaired explicit retry.
-
-## Back off transient idle capture failure
-
-[D · g5.git.flush-failure.cooldown]
-
-A transient idle-flush failure has a five-minute cooldown before automatic retry.
+A deterministic capture failure, such as an oversized file or an invalid proof, ends a stop's [save window](lifecycle.g5.md#give-each-stop-a-bounded-save-window) at once instead of being retried. The failed flush keeps its error; the workspace keeps no block and no repair notice.
 
 ## Retain recovery sources while anyone relies on them
 
@@ -286,21 +280,15 @@ Native Git must exclude runtime/cache/session/layout and staging artifacts, whil
 
 ### g5.git.flush-failure
 
-- agent
-- From [C057–C058](../../uplift/g5-lean/candidates/contracts.md), [M018](../../uplift/g5-lean/candidates/scope-resolutions.md).
-- ~80% · stop/flush-block state machine; complete failure combinations untested.
+- agent, human-verified
+- From [C057–C058](../../uplift/g5-lean/candidates/contracts.md), [M018](../../uplift/g5-lean/candidates/scope-resolutions.md). The stop half (owner Stop, Delete and transfer aborting, with deadline and force-Delete exceptions) was replaced by the user's choice that every stop forces through with recorded loss ([request](../../chat_intent/nuclear-stop/request.md)); the fork half is unchanged.
+- Implemented for stops · no stop reopens admission (`backend/g5-api/tests/api_workspace_vms.rs` forced-stop tests). Fork abort not reassessed (last baseline ~80%, stop/flush-block state machine).
 
 ### g5.git.flush-failure.block
 
 - agent
-- From [C057–C058](../../uplift/g5-lean/candidates/contracts.md), [M018](../../uplift/g5-lean/candidates/scope-resolutions.md).
-- ~80% last whole-block baseline · stop/flush-block state machine; HTTP Stop with ignored native memory verifies persisted block/notification, reopened admission and no teardown. Other failure combinations remain untested.
-
-### g5.git.flush-failure.cooldown
-
-- agent
-- From [C057–C058](../../uplift/g5-lean/candidates/contracts.md), [M018](../../uplift/g5-lean/candidates/scope-resolutions.md).
-- ~80% · stop/flush-block state machine; complete failure combinations untested.
+- From [C057–C058](../../uplift/g5-lean/candidates/contracts.md), [M018](../../uplift/g5-lean/candidates/scope-resolutions.md). Before the [stop-always-completes request](../../chat_intent/nuclear-stop/request.md), a deterministic failure persisted a fingerprinted block, sent one owner notice and waited for a repaired explicit retry; the separate five-minute idle cooldown (`g5.git.flush-failure.cooldown`) was retired with it, because an idle stop no longer retries later.
+- Implemented · `workspace_vm_service.rs::give_up_saving` ends the window on `is_deterministic_flush_error`; `workspace_flush_service.rs::fail` keeps the failure on the flush row (a replay keeps its first time) and writes only `last_error.latest`; `idle_candidates` no longer skips a workspace holding a legacy block; the operator `GET /worker-hosts/flush-blocks` listing is removed. Tests: `backend/g5-api/tests/api_workspace_vms.rs`: `deterministic_flush_failure_gives_up_saving_and_kills_the_vm`, `::ignored_native_memory_gives_up_saving_and_kills_the_vm`, `::idle_stop_kills_a_workspace_whose_save_fails`, and `backend/g5-api/tests/api_workspace_flush_receipts.rs::too_large_flush_records_its_failure_on_the_flush_only`. Rows written by earlier versions keep an inert `last_error.flush_block`.
 
 ### g5.git.retention
 
diff --git a/.g5/ontology/software_architecture/lifecycle.g5.md b/.g5/ontology/software_architecture/lifecycle.g5.md
index 74c8d2b8..c7f5553f 100644
--- a/.g5/ontology/software_architecture/lifecycle.g5.md
+++ b/.g5/ontology/software_architecture/lifecycle.g5.md
@@ -45,21 +45,41 @@ Reject an unchanged invalid boot specification without repeatedly launching it.
 
 [D · g5.lifecycle.stop]
 
-Stop advances lifecycle generation, cancels old consent and closes admission before settling attempts and capturing required Git/session/journal state. This fencing also applies when compute is already stopped but Start remains queued.
+Stop advances lifecycle generation, cancels old consent and closes admission before settling attempts and capturing Git/session/journal state within its [save window](#give-each-stop-a-bounded-save-window). This fencing also applies when compute is already stopped but Start remains queued.
 
 [Reboot](#reboot-through-explicit-stop-then-start) composes this Stop with fresh Start consent.
 
+## Stop always completes
+
+[D · g5.lifecycle.stop.force]
+
+Every stop (owner, idle, transfer, Delete and deadline) ends with the VM stopped and released; Stop never reopens admission or returns the VM to running. When a capture step cannot finish inside the [save window](#give-each-stop-a-bounded-save-window), Stop makes no further guest calls, [records what it could not save](#report-what-a-forced-stop-could-not-save) and has the host kill the VM. Attempts still running end through ordinary VM-loss settlement, and the next Start restores a fresh incarnation from the last complete receipt.
+
+Reason: otherwise one broken guest or one journal event the API cannot store keeps the workspace in `stopping` and then returns it to `running`, with no way out for the owner.
+
+## Give each stop a bounded save window
+
+[D · g5.lifecycle.stop.window]
+
+Owner, idle, transfer and ordinary Delete stops get two minutes from admission to save. [Deadline stops](#deadline-shutdown-is-visible-and-bounded) keep their flush deadline, and force Delete gets no window. Failures are retried inside the window, except a [deterministic capture failure](durability.g5.md#end-the-save-window-on-a-deterministic-capture-failure), which ends it at once.
+
+## Report what a forced stop could not save
+
+[D · g5.lifecycle.deadline.loss]
+
+A stop that kills the VM records that state was lost instead of claiming capture succeeded. Showing or announcing the loss is not required.
+
 ## Release only the stopped incarnation
 
 [D · g5.lifecycle.stop.release]
 
-After required capture and [host custody proof](runtime.g5.md#release-capacity-only-with-exit-proof), stop/reap the VM and atomically settle Agents release without affecting a successor or automatically creating one.
+When capture completes or the save window ends, stop the VM through its host: a 30-second graceful shutdown after a complete save, an immediate kill otherwise. With [host custody proof](runtime.g5.md#release-capacity-only-with-exit-proof), reap it and atomically settle Agents release without affecting a successor or automatically creating one. A host that cannot be reached leaves the stop `stopping` until the host returns or the existing host-loss rules release the VM.
 
 ## Disclose an unrestorable provider bundle
 
 [D · g5.lifecycle.stop.bundle-exception]
 
-An invalid or oversized provider bundle may be marked unrestorable with owner notice and delivered-history fallback, without waiving required task-event persistence.
+An invalid or oversized provider bundle is marked unrestorable with owner notice and delivered-history fallback instead of failing the save; it does not excuse any other unsaved state.
 
 ## Idle reflects real owner activity
 
@@ -103,12 +123,6 @@ Attach session notices to later ordinary input without starting work or injectin
 
 Under the retained [MVP daily rotation](runtime.g5.md#keep-the-mvp-host-credential-and-rotation-model), cloud drain provisions replacement host capacity before stopping, flushing and releasing old VMs.
 
-## Report state lost at an authorized deadline
-
-[D · g5.lifecycle.deadline.loss]
-
-Authorized deadline teardown records lost state instead of claiming failed capture succeeded.
-
 ## Transfer changes authority only after clean release and purge
 
 [D · g5.lifecycle.transfer]
@@ -125,7 +139,7 @@ An organization owner or administrator may transfer a [base workspace](../featur
 
 [D · g5.lifecycle.transfer.failure]
 
-A failed transfer keeps the old owner and must not force loss or perform a hot handover.
+A failed transfer keeps the old owner and never performs a hot handover. Its stop [always completes](#stop-always-completes), so a failed save costs the recorded unsaved work but does not fail the transfer; the recipient starts from the last complete receipt.
 
 ## Block transfer while legacy execution remains active
 
@@ -217,18 +231,36 @@ When an owner's Start brings the workspace back to ready, should a job's agent a
 
 - agent
 - From [C065/C069–C070](../../uplift/g5-lean/candidates/contracts.md); [Experiment 7 audit](../../../docs/experiments/ontology_harness_tuning/ontology-experiment-7-history-audit.md) clarifies the queued-consent case.
-- Partial · `backend/g5-api-core/src/services/workspace_vm_service.rs::admit_stop` returns early for stopped/failed VMs without withdrawing queued consent or advancing generation. Source-inspected in this checkout; no runtime check in this update. Complete session/journal settlement remains unverified.
+- Partial · `backend/g5-api-core/src/services/workspace_vm_service.rs::admit_stop` returns early for stopped/failed VMs without withdrawing queued consent or advancing generation. Source-inspected in this checkout; no runtime check in this update. Complete session/journal settlement remains unverified. The save-window reference was added by the [stop-always-completes request](../../chat_intent/nuclear-stop/request.md); `g5.lifecycle.stop.window` records its implementation.
+
+### g5.lifecycle.stop.force
+
+- human-provided, human-verified
+- The user asked that Stop "act as the nuclear weapon to kill all lifecycle issues and offer a fresh new restart" and chose "Stop anyway, show loss" for every stop, idle and transfer included ([request](../../chat_intent/nuclear-stop/request.md)). The mechanism (no further guest calls, host kill, VM-loss settlement, receipt restore) is agent design accepted as part of that choice. Supersedes the v9 rule that user, transfer and idle stops abort ([VM API §5](../../../docs/workspace-vm-api-v9.md#lifecycle-admission-start-intent-and-stop-durability)) and human decision H072's transfer abort ([human decisions](../../uplift/g5-lean/candidates/human-decisions.md)).
+- Implemented · `workspace_vm_service.rs::stop_with_authority` sends every save failure through `give_up_saving`: inside the window a failure is retried by the reconciler, and past it (or for a deterministic capture failure) the stop records the loss, makes no further guest call and sends `vm.stop` in kill mode; `abort_stop` and the admission reopen are gone. Attempts end through the existing `agent_tasks.rs::apply_vm_loss`. API tests in `backend/g5-api/tests/api_workspace_vms.rs`, each failing on the previous code: `owner_stop_gives_up_on_a_task_event_it_cannot_store_and_start_boots_fresh` (an unstorable and an invalid journal event, then a new incarnation on Start), `deterministic_flush_failure_gives_up_saving_and_kills_the_vm`, `transient_flush_failure_is_retried_until_the_save_window_ends`, `user_stop_resumed_past_its_save_window_kills_without_calling_the_guest`, `idle_stop_kills_a_workspace_whose_save_fails`. Real-VM flows run only in the Cloud Build e2e lane: `frontend/e2e/flows/workspace-vm.spec.ts` ("stops a VM whose save cannot finish and starts it fresh", passed in build 8a42ccd3) and value-loop M11 `tests/e2e/src/scenarios/fork_recovery.rs::test_oversized_stop_kills_the_vm_and_start_restores_the_last_receipt` (a clean Stop's receipt survives a forced Stop and an API restart, and Start restores it without the unsaved files), which replaced the M11 scenario that asserted the old blocked Stop. An unreachable host still leaves the stop `stopping` (see `g5.lifecycle.stop.release`).
+
+### g5.lifecycle.stop.window
+
+- agent
+- Two minutes keeps v9's 120-second retry figure ([VM API §5](../../../docs/workspace-vm-api-v9.md#lifecycle-admission-start-intent-and-stop-durability)) and fits the user's accepted "about 1–2 minutes" ([request](../../chat_intent/nuclear-stop/request.md)). Deadline stops keep the [deadline flush window](#deadline-shutdown-is-visible-and-bounded).
+- Implemented · `StopReason::default_save_window` gives owner, idle and transfer stops `STOP_SAVE_WINDOW_SECS` (two minutes) and nudged deadline stops `DEADLINE_STOP_SAVE_WINDOW_SECS` (ten minutes); force Delete passes an immediate deadline. `backend/g5-api/tests/api_workspace_vms.rs`: `transient_flush_failure_is_retried_until_the_save_window_ends` checks the two minutes through the HTTP Stop route. Guest requests still carry the window end as their deadline, so no guest call outlives it. Giving up moves `stop_deadline_at` to the moment it gave up (`record_unflushed_loss`), so a retry after a busy or unanswered host command, or after an API restart, kills again instead of saving or shutting down gracefully (`::forced_stop_stays_a_kill_when_the_host_command_is_retried`, found by the independent review).
+
+### g5.lifecycle.deadline.loss
+
+- agent, human-provided
+- From [C068/C103](../../uplift/g5-lean/candidates/contracts.md); widened from deadline teardown to every forced stop by the [stop-always-completes request](../../chat_intent/nuclear-stop/request.md). The user declined a loss notice or display: "do NOT improve it, keep as-it, we do not need to show".
+- ~80% · `workspace_vm_service.rs::record_unflushed_loss` now runs for every forced stop, records nothing when the stop's flush already completed (`::stop_resumed_after_a_completed_save_records_no_loss`, found by CodeRabbit), and otherwise keeps its existing record (`unflushed_loss_at`, `last_error.reason = deadline_unflushed_loss` and the failure code as `detail`); tested by the forced-stop tests listed for `g5.lifecycle.stop.force`. The record does not name members, sessions or events, by the user's choice.
 
 ### g5.lifecycle.stop.release
 
 - agent
-- From [C065/C069–C070](../../uplift/g5-lean/candidates/contracts.md).
-- ~70% · stop/release and host custody; failed boot can drop unreaped custody and failed root deletion can lose pending purge state. Every VM release clears custody in `backend/g5-api-core/src/services/workspace_vm_release.rs`, and each one that ends a VM's lifecycle (all but a refused placement) calls `task_lifecycle.rs::settle_agents_after_release` in the same transaction. Settlement takes effect on every such release: completed stop, failed start, lease expiry and forced host removal run in an org-scoped transaction, and host-reported releases (inventory missing, exit, out of memory, stale kill, boot failure) run under the [host's organization](runtime.g5.md#host-facts-apply-under-the-hosts-organization). `backend/g5-api/tests/api_worker_hosts.rs` covers forced removal, a reported exit and a hello without the VM. The percentage was not reassessed.
+- From [C065/C069–C070](../../uplift/g5-lean/candidates/contracts.md). The two host shutdown modes and the unreachable-host boundary come from the [stop-always-completes request](../../chat_intent/nuclear-stop/request.md); nothing required remains in the guest after a complete save, and [retained roots are disposable caches](runtime.g5.md#treat-retained-roots-as-disposable-caches).
+- ~75% · `workspace_vm_service.rs::stop_with_authority` sends `vm.stop` graceful with `CLEAN_STOP_GRACE_SECS` (30 s) after a complete save and kill otherwise, each under its own idempotency key (`stop:<op>:host` and `stop:<op>:host-kill`) because the host binds a key to its payload; the host answers `Busy` while an earlier stop of the same VM is still running. `backend/g5-api/tests/api_workspace_vms.rs`: `workspace_vm_stop_then_resume_flushes_and_starts_a_new_incarnation` checks the graceful 30 s command, the forced-stop tests the kill. Only the API replica holding the host's tunnel can send the kill, so a replica without it yields the stop, also past the window, and leaves it due at once (`stop_with_authority`, `defer_stop_to_tunnel_owner`; `::expired_stop_on_a_replica_without_the_host_tunnel_still_reaches_the_owner`, found by the independent review). Failed boot can still drop unreaped custody and failed root deletion can lose pending purge state. Every VM release clears custody in `backend/g5-api-core/src/services/workspace_vm_release.rs`, and each one that ends a VM's lifecycle (all but a refused placement) calls `task_lifecycle.rs::settle_agents_after_release` in the same transaction. Settlement takes effect on every such release: completed stop, failed start, lease expiry and forced host removal run in an org-scoped transaction, and host-reported releases (inventory missing, exit, out of memory, stale kill, boot failure) run under the [host's organization](runtime.g5.md#host-facts-apply-under-the-hosts-organization). `backend/g5-api/tests/api_worker_hosts.rs` covers forced removal, a reported exit and a hello without the VM.
 
 ### g5.lifecycle.stop.bundle-exception
 
 - agent
-- From [C065/C069–C070](../../uplift/g5-lean/candidates/contracts.md).
+- From [C065/C069–C070](../../uplift/g5-lean/candidates/contracts.md). The task-event clause was narrowed when [every stop became forcible](#stop-always-completes): a forced stop may lose agent events and records them.
 - ~35% · session harvest and stop settlement; complete failed-harvest propagation and deterministic delivered-history fallback are missing.
 
 ### g5.lifecycle.idle
@@ -273,12 +305,6 @@ When an owner's Start brings the workspace back to ready, should a job's agent a
 - From [C068/C103](../../uplift/g5-lean/candidates/contracts.md).
 - ~75% · nudge/rotation; complete deadline/notice delivery unexecuted. Rotation's resumability sweep (`backend/g5-api-core/src/cloud/rotation.rs`) finishes only rotation's own drains, which carry no `drain_id`; an operator drain is left alone, and an operator cannot take over a rotation drain (`backend/g5-api/tests/api_worker_hosts_stale.rs::rotation_leaves_an_operator_drained_host_alone`, `::an_operator_drain_cannot_take_over_a_rotation_drain`).
 
-### g5.lifecycle.deadline.loss
-
-- agent
-- From [C068/C103](../../uplift/g5-lean/candidates/contracts.md).
-- ~75% · nudge/rotation; complete deadline/notice delivery unexecuted.
-
 ### g5.lifecycle.transfer
 
 - human-verified
@@ -293,9 +319,9 @@ When an owner's Start brings the workspace back to ready, should a job's agent a
 
 ### g5.lifecycle.transfer.failure
 
-- human-verified
-- From [C071–C073](../../uplift/g5-lean/candidates/contracts.md), [H080](../../uplift/g5-lean/candidates/human-decisions.md).
-- ~80% · workspace transfer operation; failed purge recovery unexecuted.
+- human-verified, human-provided
+- From [C071–C073](../../uplift/g5-lean/candidates/contracts.md), [H080](../../uplift/g5-lean/candidates/human-decisions.md). The forced-loss clause is replaced by the user's choice that transfer stops also "kill the VM and record the loss when saving fails" ([request](../../chat_intent/nuclear-stop/request.md)), amending H072's transfer abort on flush failure.
+- ~80% · workspace transfer operation; failed purge recovery unexecuted. A transfer's stop now forces through like any stop: `backend/g5-api/tests/api_workspace_vms.rs`: `transfer_completes_when_the_former_owners_save_fails` completes the transfer with the loss recorded and the VM killed.
 
 ### g5.lifecycle.transfer.legacy
 
```

</details>

### Relevant and governing ontology

| Block | Why it matters here | What the implementation must keep |
|---|---|---|
| [`g5.lifecycle.stop`](../../ontology/software_architecture/lifecycle.g5.md#stop-fences-future-work-before-releasing-custody) | Fencing comes before any saving | Admission still advances generation, withdraws consent and marks the stop before any guest call; a forced stop never reopens admission |
| [`g5.runtime.custody`](../../ontology/software_architecture/runtime.g5.md#release-capacity-only-with-exit-proof) | Release needs host exit proof | Forcing releases only on the host's `Stopped` or `NotFound`; an unreachable host keeps `stopping` |
| [`g5.git.receipt`](../../ontology/software_architecture/durability.g5.md#only-verified-complete-capture-advances-recovery) | Partial capture never advances recovery | A failed flush keeps its error and is never selected; Start restores the previous receipt |
| [`g5.git.restore`](../../ontology/software_architecture/durability.g5.md#warm-and-cold-restore-mean-the-same-thing) | "Fresh restart" | Start after a forced stop uses the ordinary restore |
| [Old VM work cannot change its replacement](../../ontology/concepts/system_invariants.g5.md#old-vm-work-cannot-change-its-replacement) | A killed VM's late writes | The epoch fence is unchanged |
| [Compute requires explicit Start consent](../../ontology/concepts/system_invariants.g5.md#compute-requires-explicit-start-consent) | "Fresh restart" must not mean auto-restart | A forced stop creates no successor |
| [`g5.agents.lifecycle.chat-held-turn`](../../ontology/features/agents.g5.md#a-new-message-ends-a-held-chat-turn) | A chat after a forced stop | Attempts end through the existing VM-loss settlement, whose held turns the next message ends |
| [`g5.lifecycle.deadline`](../../ontology/software_architecture/lifecycle.g5.md#deadline-shutdown-is-visible-and-bounded) | Deadline stops' nudge | The nudge is unchanged; its flush deadline is that stop's save window |
| [`g5.lifecycle.transfer`](../../ontology/software_architecture/lifecycle.g5.md#transfer-changes-authority-only-after-clean-release-and-purge) | Transfer waits for release and purge | Owner change still follows the (possibly forced) release and the credential-root purge |

## Design

```text
 Stop admitted (any reason): generation+1, consent withdrawn, stop_deadline_at = admission + window
   │
   ▼
 save phase: guest calls, each with the window end as its deadline
   close admission → settle agent attempts → harvest → flush Git
   │                                    │
   │ all saved                          │ step failed and (window over or failure is deterministic),
   │                                    │ or the stop resumed after its window
   ▼                                    ▼
 host vm.stop graceful, 30 s       record loss (unflushed_loss_at + reason, as before)
 key stop:<op>:host                     │
   │                                    ▼
   │                               host vm.stop kill, key stop:<op>:host-kill, no guest call
   └──────────────┬─────────────────────┘
                  ▼
   exit proof → release (stopped, custody cleared, Agents release settled)
                  ▼
   agent reconciler: attempts of the gone incarnation → apply_vm_loss
                  ▼
   owner's Start → new incarnation from the last complete receipt
```

`WorkspaceVmService` stays the single owner of the stop. Admission, fencing and the save order are unchanged; `stop_deadline_at` is the end of the save window (`StopReason::default_save_window`: two minutes, or ten minutes for a nudged deadline stop). Every save-step failure goes through one rule, `give_up_saving`: inside the window it is left for the reconciler to retry; past the window, or for a deterministic capture failure, the stop records the loss and kills. Giving up also ends the window in the database (`stop_deadline_at = LEAST(stop_deadline_at, now())`), so every later attempt of the same stop, after a busy host or an API restart, goes straight to the kill. A stop resumed past its window makes no guest call at all. Only the API replica holding the host's tunnel can send the kill, so a replica without it always yields the stop to that owner and leaves it due at once; before, past the deadline it drove the stop itself and pushed the shared retry time out, which could starve the owner. That replaces the per-reason split between `abort_stop` (removed with the admission reopen) and the deadline-loss path; `permits_deadline_loss` became `nudges_before_stopping`, which now only chooses whether a stop is nudged first.

No new state or mechanism. The forced branch reuses `record_unflushed_loss` and the host's existing kill mode. Agent attempts need no new settlement: once the incarnation is gone, `agent_tasks.rs::reconcile_attempt` already applies VM loss, which also ends the attempt stuck on an event it cannot store. The workspace flush block is removed rather than reused: `workspace_flush_service.rs::fail` keeps the failure on the flush row, idle stop no longer skips blocked workspaces, and the operator `GET /worker-hosts/flush-blocks` listing (only ever filled by blocks) is gone.

Force Delete now makes no guest call either. Its deadline is the admission time, and the dispatcher already refused requests whose deadline had passed, so in production its guest calls had always failed at once; only the in-process test peer accepted them.

## Acceptance outcomes

1. Owner Stop on a VM whose agent journal holds an event the API cannot store ends `stopped` once the window ends. The attempt is settled, and Start reaches a new incarnation.
2. A save failure a retry may fix is retried for two minutes, then the stop kills the VM without calling the guest again.
3. A deterministic capture failure (for example a file over 100 MiB) forces at once, with no block or notice left behind.
4. Idle and transfer stops with a failed save complete, and the transfer changes owner.
5. A stop resumed after its window makes no guest call.
6. With the host disconnected, the stop stays `stopping` until the host returns or host-loss rules release it.
7. A clean Stop still saves everything, uses a 30-second graceful shutdown and records no loss.
8. Stop never puts a VM back to `running`.

## Tests

API integration tests (`backend/g5-api/tests/`), on the real stop driver with the in-process host and dispatcher peer. Each one failed on `origin/main` source and passes here (run 2026-10-07, 10 failures in `api_workspace_vms`, 1 in `api_workspace_flush_receipts`):

- `api_workspace_vms::owner_stop_gives_up_on_a_task_event_it_cannot_store_and_start_boots_fresh` (outcome 1; an event with a sequence gap fails at the same commit as the NUL bytes did, and an event without a payload fails reading the replay)
- `::transient_flush_failure_is_retried_until_the_save_window_ends` (outcomes 2 and 5, and the two-minute window through the HTTP route)
- `::deterministic_flush_failure_gives_up_saving_and_kills_the_vm`, `::ignored_native_memory_gives_up_saving_and_kills_the_vm` (outcomes 3 and 8)
- `::idle_stop_kills_a_workspace_whose_save_fails`, `::transfer_completes_when_the_former_owners_save_fails` (outcome 4)
- `::user_stop_resumed_past_its_save_window_kills_without_calling_the_guest`, `::expired_drain_deadline_records_flush_loss_and_releases_the_vm`, `::delete_stop_settles_a_reserved_but_never_invoked_agent` (outcome 5 for owner, deadline and force-Delete stops)
- `::expired_stop_on_a_replica_without_the_host_tunnel_still_reaches_the_owner` (a tunnel-less replica's reconciler ticks no longer keep the tunnel owner from killing the VM; added after review round 2)
- `::forced_stop_stays_a_kill_when_the_host_command_is_retried` (a retry after `Busy` resends the same kill and does not try to save again; added after review round 1)
- `::stop_resumed_after_a_completed_save_records_no_loss` (a stop resumed after its window, whose flush completed before an API restart, kills without recording a loss; found by CodeRabbit)
- `::workspace_vm_stop_then_resume_flushes_and_starts_a_new_incarnation` (outcome 7)
- `api_workspace_flush_receipts::too_large_flush_records_its_failure_on_the_flush_only`

Outcome 6 relies on the existing `::stop_on_a_replica_without_the_host_tunnel_yields_to_the_tunnel_owner` and lease-expiry tests; it is not new behavior.

Real-VM E2E, value-loop M11: `tests/e2e/src/scenarios/fork_recovery.rs::test_oversized_stop_kills_the_vm_and_start_restores_the_last_receipt` replaces the M11 scenario that asserted the old blocked Stop (`test_incomplete_stop_flush_blocks_until_repaired_after_api_restart`, which failed in the first CI run of this branch). A clean Stop saves a two-member receipt; after Start, an unsaved file and a 101 MiB file make the next Stop give up at once; the receipt stays, the loss is recorded, an API restart retries nothing, and Start restores the saved files without the unsaved ones. The selector is renamed in `tests/e2e/value_loop_matrix.tsv`, `infra/ci/cloudbuild-ci-e2e.yaml` and `ci_e2e_lane_isolation.rs`.

Real-VM E2E, Playwright: `frontend/e2e/flows/workspace-vm.spec.ts` "stops a VM whose save cannot finish and starts it fresh" writes a 110 MiB file from the owner's terminal, stops (must reach `stopped` within 90 s with the loss recorded), starts, and checks the file is gone on the new incarnation. It runs in the Cloud Build `g5-ci-e2e` lane only.

Test changes: the in-process peer replays a configured journal after a cancel (from #337, which made the same change) and can script the host's `vm.stop` reply. Three tests that asserted the old abort were rewritten, and one flush-receipt test now asserts the flush-row record.

## Risks and assumptions

- **Automatic work loss is the point, and it has costs.** A healthy save slower than two minutes is cut off and recorded as lost. An idle workspace with an oversized file loses its unsaved work at every idle stop. The window is one constant.
- **Transfer** may hand the recipient an older state. This reverses H072.
- **Unreachable host:** Stop cannot finish without exit proof; host-loss and lease rules still release it.
- **Not covered:** a task that never settles still keeps idle stop from starting (`idle_candidates` skips workspaces with a running task), and a chat Stop on an unstorable event still cannot end that turn. #337 (merged) removes today's cause, NUL bytes; owner Stop is now the escape hatch for both.
- **Lost agent events include their usage reports**, so the provider usage of a killed turn's unsaved tail is not counted.
- **Old rows** may carry `last_error.flush_block`; nothing reads it any more.

## Review summary

Self-review by the implementing agent (not independent), covering the stop driver, flush service, idle selection, transfer and Delete callers, the removed operator route, and the changed tests. Commitments checked:

- `g5.lifecycle.stop.force` → `stop_with_authority` / `give_up_saving` → unstorable event, deterministic and transient flush failures, resumed past window → implemented, API-tested.
- `g5.lifecycle.stop.window` → `default_save_window`, route passes `None` → HTTP window check → implemented.
- `g5.lifecycle.stop.release` → host command mode and keys → graceful and kill assertions → implemented. The host returns `Busy` while an earlier graceful stop runs, so a later kill waits at most the 30-second grace.
- `g5.lifecycle.transfer.failure`, `g5.git.flush-failure(.block)` → transfer, idle and flush tests → implemented.
- `g5.runtime.custody` → release still requires `Stopped`/`NotFound` → unchanged.

Independent review (Codex CLI, gpt-6-astra, extra-high effort), round 1 at `db3de1f5`: one P2. After giving up, a stop whose host command came back `Busy` (or unanswered) was retried as a graceful shutdown, because the forced decision lived only in that one invocation. Fixed by ending the window durably when the loss is recorded; regression `::forced_stop_stays_a_kill_when_the_host_command_is_retried` fails without the line. The reviewer's independent probes of chat, job and cancel-requested attempts after a forced stop passed.

Round 2 at `25ee66ce`: the round-1 fix confirmed (its interruption and force-Delete probes passed), and one new P2. Past the window, an API replica without the host's tunnel drove the stop itself, failed to reach the host and pushed the shared retry time five seconds out on each of its ticks, so the tunnel owner could be starved while the host was reachable (reproduced with two service instances). Fixed by always yielding to the tunnel owner and leaving the stop due at once; regression `::expired_stop_on_a_replica_without_the_host_tunnel_still_reaches_the_owner`.

CodeRabbit, on the PR: one valid minor finding (a stop resuming past its window recorded a loss even when its flush had completed before an API restart; `record_unflushed_loss` now records nothing for a completed stop flush, regression above), one trivial metadata wording fix, and one declined (ordinary Delete uses `StopReason::User` and so the two-minute window; only force Delete sends `StopReason::Delete`, with an immediate deadline).

CI: the first e2e runs failed only on two stopped-workspace specs that still asserted a hidden terminal control after #333 kept it visible and disabled (#333's own last e2e failed the same way); this branch carries #340's first, test-only commit (`e333cad1`) for them. Value-loop M11 was rewritten for the new rule and passed on a real VM, as did the Playwright forced-stop test.

The full `cargo test -p g5-api --test '*'` run (115 files, four test threads) had one failure, `remote_repo_tracking_integration::concurrent_member_cycles_converge_without_lease_expiry`, a 45-second queue-drain deadline unrelated to stops; it passed 3 of 3 runs alone.

Remaining uncertainty:

- The Playwright real-VM test is unexecuted until the Cloud Build e2e lane runs; it assumes the guest reports `FLUSH_TOO_LARGE` for an untracked 110 MiB file.
- 30 seconds of graceful shutdown replaced 300; nothing required remains in the guest after a complete save, but a slow healthy guest is now killed at 30 s.
- A crash of the API mid-stop resumes after the existing 320-second reconciler delay, so the worst case after a crash is about five and a half minutes.
