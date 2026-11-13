# Implementation review: workspace rebase consistency

Self-review of PR [G5Labs/G5-Lean#334](https://github.com/G5Labs/G5-Lean/pull/334) by the implementing session, so it is not independent. The design is [ontology-plan.md](ontology-plan.md), whose `Ontology changes and governance` section was the starting inventory. Scope: the branch diff against `origin/main`, including the dev-stack follow-up fix to Ship to Base.

## Initial findings

These were recorded before the review's repairs. Defects found and fixed earlier in the implementation session are listed separately below.

| # | Kind | Governing ontology | Finding | Consequence |
| --- | --- | --- | --- | --- |
| R1 | Missing evidence | g5.governance.remote.workspace-rewrite | `require_publishable_tracked_source` (Create/Update PR) had no test. | The rule against overwriting provider commits on Create/Update PR was unverified, although this path previously force-pushed over them. |
| R2 | Unmet design obligation | g5.governance.remote.workspace-rewrite; plan §4 | Manual mode's pending rewrite showed as "Ahead — local commits to push", the same label as ordinary new commits. | The owner can't tell that Push will replace provider history rather than extend it. |
| R3 | Defect (outcome reporting) | g5.architecture.git_outcomes | After G5 accepted the guest's leased push, a failure updating the local tracking ref reported the member as not rebased. | An effect that already happened was reported as a failure. A retry would recover. |
| R4 | Stale artifact | Authoring format, governing contract | The plan's recorded ontology diff predates the tightened blocks and the new metadata. | The handoff no longer matched the ontology in scope. |

Earlier, in-session defects that are already fixed and covered by tests:
- The split checkout left by the old central rebase was refused rather than reconciled (Ben's case). Reconciliation was added.
- The Ship to Base import raced the target's auto-pull. Ship now recaptures once.

## Repairs and rechecks

| # | Repair | Recheck (runtime unless noted) |
| --- | --- | --- |
| R1 | Added `api_workspace_pull_requests.rs::update_pr_publishes_tracked_sources_only_over_unchanged_provider_history`. | Passes. With the guard removed it fails: Update PR returns 200 and would replace the provider commits. |
| R2 | The tracking status reports `rewritePending` when the watermark is not an ancestor of the G5 head (`branch_sync_runs_service.rs`, DTO). The pill reads "Ahead — rewritten history to push". | The worker test asserts the flag is true before the explicit Push and false after it, through `GET .../sync_runs`. Vitest covers the label. |
| R3 | The tracking-ref update after a confirmed leased push is best-effort (`member/rebase.rs`). | Guest dispatcher test passes. Inspection: the restore guard is still cleared before this step. |
| R4 | The plan's diff block was replaced with the actual current diff. The original handoff diff is preserved below. | Inspection. |

## Later discoveries (CodeRabbit review)

These were found by an external reviewer after the self-review repairs and fixed in the same review artifact.

| # | Kind | Governing ontology | Finding | Repair and recheck |
| --- | --- | --- | --- | --- |
| C1 | Defect (defense in depth) | Plan §2: Base is never a rebase source | `rebase_source_in_vm` skipped `reject_base_source_workspace`, which the central path applies. | The VM path now applies the same check before preflight. Inspection only: Base has no VM, so `source_rebase_execution` never routes it there. |
| C2 | Defect | g5.architecture.workspace_rebase | The in-VM rebase inherited the 30 s Changes RPC budget, so a large fetch, replay and push could be cancelled mid-operation. | `CHANGES_REBASE_BUDGET_MS` (240 s) stays inside the module's 300 s wall clock. Inspection; existing API tests pass. |
| C3 | Defect, contradicting the plan | g5.governance.remote.workspace-rewrite (missing synchronization proof grants no overwrite) | Create/Update PR skipped classification when the tracked source had no watermark, then force-pushed over a moved provider tip. | Classification always runs; without a watermark only a fast-forward publishes. The PR test adds a no-watermark case, which fails with a 200 when the old skip is restored. |

## Commitment accounting

| Commitment | Actual path | Check | Result |
| --- | --- | --- | --- |
| A running VM executes every rebase, even when Git could rebase centrally | `routes/workspaces.rs::start_workspace_merge` → `source_rebase_execution` → `rebase_source_in_vm` | API: `running_workspace_rebases_in_its_vm_even_when_central_git_could` (VM invoked for both members, no central run). Dev stack: `start_workspace_merge (in VM)` succeeded for `test-rebase-2` in a booted VM on catalog `2026.10.06`, with no following commit-triggered integration run. | Verified with the scripted peer and once in a booted VM. The booted-VM run looked like a validated no-op; no guest push through the receiver was observed. |
| Transitions refuse with no fallback | `source_rebase_execution` start-intent and status predicate | API: `starting_workspace_refuses_rebase_without_central_fallback` | Start intent verified. `stopping` is inspection only. |
| Older guest asks for a restart | `workspace_runtime.rs` `OPERATION_NOT_FOUND` mapping | Inspection | Unverified at runtime. |
| Stopped central path only with a clean receipt, rechecked under the fence | Route recheck after the commit fence | API: `stopped_workspace_rebases_centrally_only_with_a_clean_published_receipt` | Verified. A Start racing that recheck was not exercised. |
| The VM takes G5 work, replays local commits without duplicating patches, publishes under the captured lease | `MemberGit::rebase_onto` | Guest: `workspace_changes_rebase_replays_in_the_checkout_under_the_captured_lease` (10 cases, real authenticated origin) | Verified at the guest boundary. |
| Refusals leave the member unchanged; conflicts hand off to resolution | `restore_checkout`; `rebase_source_in_vm` conflict branch | Guest conflict and divergence cases; API `get_latest_from_base_intermediate_replay_conflict_starts_an_agent` | Verified. |
| Completion means G5 accepted the result; partial members are reported | Leased push; `source_rebase_refusal` | API asserts G5 heads | Partial multi-member refusal is inspection only. |
| Workspace rewrite over an unchanged provider publishes by push mode with a lease | `classify_history` → `classify_and_act` → `push_existing_commit(replace)` | Worker: auto under the mocked App, manual explicit push, rewind; SCM lease tests | Verified, without a live provider. |
| A provider advance or rewrite is never overwritten, even without a watermark | Classifier `Diverged` / `WatermarkViolation`; `require_publishable_tracked_source` | Worker `workspace_rewrite_never_overwrites_independent_provider_changes`; R1 test for PR, including the no-watermark case (C3) | Verified. |
| False checkpoints are rechecked; genuine ones stay | `is_unapproved_rewrite_checkpoint`, `obsolete_false_rewrite_checkpoint`, scheduler / retry / push | Worker sweep test; genuine-rewrite recheck | Verified. Retry and explicit-push entry are inspection only. |
| Base never publishes | `publication_allowed` arm | Unit decision table | Verified (unit). |
| Automatic publication requires an org App | Credential path unchanged | Worker App test, plus the existing no-App refusal test | Verified. |
| A known applied merge is not a false failure | Ship recapture | `provider_first_merge_completes_when_target_auto_pull_lands_it_first` (fails without the fix) | Verified. |

## Remaining gaps

| Governing ontology | Gap | Proposed action | Uncertainty / alternatives / blockers |
| --- | --- | --- | --- |
| g5.architecture.workspace_rebase | No booted-VM rebase that publishes (guest push through the production Git receiver) followed by Create PR | On the dev stack, rebase a running workspace whose base has moved, then Create PR, then Ship | Needs a rebase that changes heads; the observed booted-VM run was a no-op |
| g5.architecture.workspace_rebase | Stop, transfer or replacement during replay; Start racing the stopped-path recheck | Fault-injection test with the runtime peer and lifecycle transitions | Moderate fixture work |
| g5.architecture.workspace_rebase | Older guest `OPERATION_NOT_FOUND` and `stopping` admission are inspection only | Add a runtime-peer option and a VM-row fixture that satisfies the incarnation shape | Low risk; shared predicate |
| g5.governance.remote.workspace-rewrite | No live GitHub provider | Exercise on a disposable connected branch | Needs a test GitHub repo and App install |

## Original handoff ontology diff

This is the diff recorded in the design before implementation, preserved as evidence. The plan now carries the actual diff.

<!-- ONTOLOGY_DIFF_START -->

```diff
diff --git a/.g5/ontology/features/governance.g5.md b/.g5/ontology/features/governance.g5.md
index 76ecb6b2..edd153f3 100644
--- a/.g5/ontology/features/governance.g5.md
+++ b/.g5/ontology/features/governance.g5.md
@@ -483,5 +483,13 @@ Accepted branch updates schedule forwarding in auto mode, the default for new wo
 [D · g5.governance.remote.destination]
 
-Synchronization updates an existing destination only after ancestry checks and an exact destination lease. A moved or removed branch is not silently recreated. Initial branch creation is a separate explicit operation under the [publication proof rule](#prove-initial-external-publication).
+Synchronization updates an existing destination only after ancestry classification and an exact destination lease; the [accepted workspace rewrite policy](#distinguish-workspace-rewrites-from-provider-rewrites) governs non-fast-forward publication. A moved or removed branch is not silently recreated. Initial branch creation is a separate explicit operation under the [publication proof rule](#prove-initial-external-publication).
+
+### Distinguish workspace rewrites from provider rewrites
+
+[D · g5.governance.remote.workspace-rewrite]
+
+Remote integration distinguishes changes to the accepted G5 branch from changes to the provider since the Git synchronization watermark. When the provider still equals that watermark, an accepted workspace rewrite is eligible for publication under the existing binding, push mode and authority, using that exact provider OID as its lease. It needs no separate provider-rewrite approval. Manual mode retains the pending rewrite without advancing the watermark or importing the old provider tip over it. This is a scoped publication policy, not permission for unconditional force or for rewriting an independently changed provider.
+
+A provider rewrite retains explicit approval while the histories differ; equal accepted workspace and provider tips can settle synchronization without overwriting either side. Concurrent provider advances require incorporation or reconciliation. Missing destinations and missing synchronization proof do not grant overwrite authority. Existing falsely classified checkpoints are re-evaluated against fresh Git inputs and current authority before resuming; genuine unresolved provider-rewrite checkpoints remain. Git owns heads and synchronization evidence, while existing integration operations own scheduling, fenced outcomes and recovery. No new rewrite flag or head registry is needed to distinguish the two sides.
 
 ### Keep queued publication consent with its requester
@@ -803,6 +811,12 @@ Automatic divergence pauses synchronization without launching resolution. An exp
 
 - agent
-- [Retained integration design](../../../docs/data-model-cleanup-plan.md#provider-transport-and-publication).
-- Source-inspected · worker uses `push_existing_commit`; deletion-between-fetch-and-push and complete publication recovery were not executed.
+- [Retained integration design](../../../docs/data-model-cleanup-plan.md#provider-transport-and-publication); [workspace rebase amendment](../../chat_intent/improve-workspace-rebase/request.md) scopes non-fast-forward publication.
+- Source-inspected · worker uses `push_existing_commit`; deletion-between-fetch-and-push and complete publication recovery were not executed. Workspace rewrite publication is a new design obligation, not implemented or verified by this assessment.
+
+### g5.governance.remote.workspace-rewrite
+
+- agent
+- [Workspace rebase request and interpretation](../../chat_intent/improve-workspace-rebase/request.md); narrows the previous blanket provider-rewrite approval rule without changing provider-change protection, Base's no-push rule or publication credentials.
+- Design only · `branch_integration_service.rs` collapses either side's loss of baseline ancestry into `WatermarkViolation`; worker and coordinated PR publication retain that refusal. Classification, leased rewrite transport, existing-checkpoint recovery and status consumers require the [plan's verification](../../pr_ontology_diff_design/improve-workspace-rebase/ontology-plan.md#acceptance-and-verification).
 
 ### g5.governance.remote.consent
diff --git a/.g5/ontology/features/workspaces.g5.md b/.g5/ontology/features/workspaces.g5.md
index 86042c45..09133f9e 100644
--- a/.g5/ontology/features/workspaces.g5.md
+++ b/.g5/ontology/features/workspaces.g5.md
@@ -151,4 +151,12 @@ Governing concepts: Owner pushes use [current-owner authority](../concepts/secur
 Governing contracts: [complete a proven divergence resolution](#complete-a-proven-divergence-resolution), [publish a resolved divergence by push mode](#publish-a-resolved-divergence-by-push-mode).
 
+### Rebase without system divergence
+
+[I · g5.story.rebase-consistency]
+
+Rebasing a workspace branch via the UI or in a workspace VM by the agent should not result in system divergence. If a VM is running, any workspace rebase executes there, even when central Git could perform it cleanly. Accepted force-pushes from the VM remain intelligible to remote integration and follow the workspace's publication policy.
+
+Governing contracts: [workspace rebase ownership](../software_architecture/workspace.g5.md#execute-workspace-rebases-in-the-running-vm), [workspace rewrite publication](governance.g5.md#distinguish-workspace-rewrites-from-provider-rewrites).
+
 ### Follow the provider in Base
 
@@ -593,4 +601,10 @@ This is the only reconciliation Base offers, because Base has no VM. It replaces
 - Implemented · `remote_integrations/worker.rs::auto_complete_proven_divergence` completes a proven run before the cycle claim; the activity list labels the `resolved` audit row. Real Postgres/Git worker tests cover an agent rebase pushed through the production Git receiver, externally resolved runs in auto and manual push modes, unproven and merge-commit heads, and provider-rewrite checkpoints. Not exercised in a browser or against a live provider.
 
+### g5.story.rebase-consistency
+
+- human-provided
+- [Workspace rebase request](../../chat_intent/improve-workspace-rebase/request.md#human-instructions).
+- Design only · [Plan](../../pr_ontology_diff_design/improve-workspace-rebase/ontology-plan.md). QA exposed central/live-checkout divergence and a workspace rewrite classified as a provider-rewrite checkpoint; implementation and E2E verification remain outstanding.
+
 ### g5.story.base-follows-provider
 
diff --git a/.g5/ontology/software_architecture/workspace.g5.md b/.g5/ontology/software_architecture/workspace.g5.md
index 3298cf5f..769e5ca7 100644
--- a/.g5/ontology/software_architecture/workspace.g5.md
+++ b/.g5/ontology/software_architecture/workspace.g5.md
@@ -18,4 +18,5 @@ and [preview/terminal](../features/preview_terminal.g5.md).
 | Workspace lifecycle | `view`, `resume`, `stop`, `transfer` | [Attachment and release](#lifecycle-coordinates-attachment-and-release) |
 | WorkspaceRuntime | `RuntimeRequest`; `file_*`, `workspace_changes*`, `modules_*`, `open_owner_stream` | [Authorized live access](#one-live-runtime-facade) |
+| Workspace rebase owner | Prepare, execute and publish source rebase through runtime or stopped landing | [Rebase execution](#execute-workspace-rebases-in-the-running-vm) |
 | Workspace reader | File/tree/diff/history over bound read views | [Durable reading](#durable-repository-reads) |
 | WorkspaceForkService | `fork_workspace` | [Complete fork](#checked-fork) |
@@ -60,4 +61,12 @@ operations with an authorized actor, exact incarnation, deadline and request ide
 [Lifecycle](lifecycle.g5.md#explicit-consent-owns-each-start) alone owns Start.
 
+### Execute workspace rebases in the running VM
+
+[D · g5.architecture.workspace_rebase]
+
+The workspace rebase owner selects execution under current lifecycle authority. A running workspace executes every source rebase in its current owner VM through WorkspaceRuntime, including clean replay, fast-forward and no-op validation; it never lands a central result behind that VM. An unavailable VM or a lifecycle transition refuses or defers the operation rather than authorizing fallback. A stopped workspace permits central execution only when its complete recovery cut is clean, fully published and on mapped branches without a sequencer, and Start cannot race final admission. This keeps [restore](durability.g5.md#advance-a-wholly-clean-stopped-workspace-after-landing) from resurrecting the previous cut.
+
+The complete operation captures exact inputs, preserves local work, executes in the selected owner and publishes with [old-OID CAS](durability.g5.md#update-mapped-branches-through-exact-old-oid-cas). Completion requires G5 acceptance of the VM result; conflicts, unpublished results and partial member success remain explicit and recoverable through the existing operation and Git state. Retries inspect those outcomes before further effects. Native agent rebases join this flow at their accepted leased push. No server fallback, blind reset or refreshed lease may erase intervening work. [Provider publication](../features/governance.g5.md#distinguish-workspace-rewrites-from-provider-rewrites) remains a separate outcome.
+
 ### Durable repository reads
 
@@ -142,4 +151,10 @@ execution evidence under the [event contract](runtime.g5.md#event-facts-survive-
 - ~75% · `backend/g5-api-core/src/services/workspace_runtime.rs`; fresh private health is not checked at admission and editor Save lacks the observed VM precondition. `call`, lifecycle requests and raw VM streams are crate-private (`backend/g5-api/tests/crate_private_fences.rs`), so code outside `g5-api-core` builds only owner requests; the 7 `call` consumers outside the facade are the fork and stop lifecycle owners. Owner streams are terminal streams; a preview stream opens through `open_preview_stream`, only on the incarnation its preview task runs on. The percentage was not reassessed.
 
+### g5.architecture.workspace_rebase
+
+- agent
+- [Workspace rebase request and interpretation](../../chat_intent/improve-workspace-rebase/request.md); implements [rebase consistency](../features/workspaces.g5.md#rebase-without-system-divergence).
+- Design only · `job_merge_service.rs::prepare_source_workspace_merge_with_mode` currently selects central execution by mergeability; `WorkspaceRuntime::prepare_pull_request_source` saves and pushes without checkout reconciliation. Runtime execution, lifecycle admission and recovery acceptance cases in the [plan](../../pr_ontology_diff_design/improve-workspace-rebase/ontology-plan.md) remain unimplemented and untested.
+
 ### g5.architecture.content_reader
 
```

<!-- ONTOLOGY_DIFF_END -->
