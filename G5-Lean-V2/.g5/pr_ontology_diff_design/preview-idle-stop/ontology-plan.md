# Preview survives the idle budget while it starts

## Ontology changes and governance

**Comparison base:** `main` at `a97f692a` (working tree, 2026-10-06).

**Ontology changes:** added `g5.lifecycle.idle.preview-startup` in `software_architecture/lifecycle.g5.md`; narrowed the service exemption sentence of `g5.preview.service` and added `g5.preview.service.stop-reason` in `features/preview_terminal.g5.md`, with metadata. Intent: [chat intent](../../chat_intent/preview-idle-stop/request.md).

```diff
diff --git a/.g5/ontology/features/preview_terminal.g5.md b/.g5/ontology/features/preview_terminal.g5.md
index 1e706ec6..e383f3c4 100644
--- a/.g5/ontology/features/preview_terminal.g5.md
+++ b/.g5/ontology/features/preview_terminal.g5.md
@@ -43,10 +43,16 @@ Governing contracts: [preview transport preserves owner authority](preview_termi
 
 [D · g5.preview.service]
 
-Preview is a visible cancellable service under live-access authority, exempt from bounded-work slots and wall-clock limits.
+Preview is a visible cancellable service under live-access authority, exempt from bounded-work slots and wall-clock limits. Until it reports ready it still counts for [idle selection](../software_architecture/lifecycle.g5.md#a-starting-preview-is-bounded-work).
 
 Use the [task lifecycle](agents.g5.md#separate-logical-outcome-from-execution-attempts) and [stream authority](../concepts/security.g5.md).
 
+### Say why a preview ended
+
+[D · g5.preview.service.stop-reason]
+
+When a workspace stop or loss ends a preview, the preview task's outcome records the reason (idle stop, owner stop, transfer, restart required, host drain, VM lost) and the activity view shows it; the module's bare "cancelled" alone does not explain to the owner what happened.
+
 ### Distinguish preview process from serving readiness
 
 [D · g5.preview.service.readiness]
@@ -137,7 +143,13 @@ Governing concepts: Live access follows [current-owner authority](../concepts/se
 
 - agent
 - From [C097–C098](../../uplift/g5-lean/candidates/contracts.md); outcome lineage: [H074/H104](../../uplift/g5-lean/candidates/human-decisions.md), [v9](../../../docs/lean-backend-design-v9.md).
-- ~85% · preview service/task kind; full lifecycle-limit matrix unexecuted.
+- ~85% · preview service/task kind; full lifecycle-limit matrix unexecuted. Idle selection treats a not-yet-ready preview as bounded work (`api_workspace_vms.rs::idle_reconciler_waits_for_a_starting_preview_but_not_a_ready_one`).
+
+### g5.preview.service.stop-reason
+
+- human-verified
+- [Preview idle-stop follow-up](../../chat_intent/preview-idle-stop/request.md), 2026-10-06.
+- `task_lifecycle.rs::apply_outcome` adds the mapped `lifecycle.interrupt_reason` to a preview's cancelled outcome (a null module reason counts as absent); `apply_start_disposition` does the same for a preview cancelled before its start was accepted, and `apply_vm_loss` for a preview lost with its VM (`vm_lost` when no stop was in progress). `workspace_activity.rs` carries it as the task entry's `reason`; `WorkspaceActivityPanel.tsx` renders it. Covered by `api_task_lifecycle.rs` (idle-interrupted, cancelled-before-start and lost previews) and the panel test.
 
 ### g5.preview.service.readiness
 
diff --git a/.g5/ontology/software_architecture/lifecycle.g5.md b/.g5/ontology/software_architecture/lifecycle.g5.md
index 74c8d2b8..68bef390 100644
--- a/.g5/ontology/software_architecture/lifecycle.g5.md
+++ b/.g5/ontology/software_architecture/lifecycle.g5.md
@@ -79,6 +79,12 @@ Use the organization’s 30-minute default idle timeout unless the workspace ove
 
 Count owner file/preview use as activity, debouncing preview updates by ten seconds.
 
+## A starting preview is bounded work
+
+[D · g5.lifecycle.idle.preview-startup]
+
+A requested preview that has not yet reported ready prevents idle stop like bounded work; once ready, its unused serving does not. A preview build can take most of the idle budget, and stopping it discards work the owner explicitly requested.
+
 ## Keep workspace lifetime independent of task completion
 
 [D · g5.lifecycle.idle.task-completion]
@@ -235,7 +241,7 @@ When an owner's Start brings the workspace back to ready, should a job's agent a
 
 - agent
 - Human [H005/H081](../../uplift/g5-lean/candidates/human-decisions.md), refined by [C067](../../uplift/g5-lean/candidates/contracts.md) and [M006](../../uplift/g5-lean/candidates/scope-resolutions.md).
-- ~70% · idle/activity tracking; a quiet connected owner terminal does not keep the workspace active.
+- ~70% · idle/activity tracking (`workspace_vm_service.rs::idle_candidates`, including the [starting-preview exclusion](#a-starting-preview-is-bounded-work)); a quiet connected owner terminal does not keep the workspace active.
 
 ### g5.lifecycle.idle.settings
 
@@ -249,6 +255,12 @@ When an owner's Start brings the workspace back to ready, should a job's agent a
 - Human [H005/H081](../../uplift/g5-lean/candidates/human-decisions.md), refined by [C067](../../uplift/g5-lean/candidates/contracts.md) and [M006](../../uplift/g5-lean/candidates/scope-resolutions.md).
 - ~90% · workspace activity/preview paths; complete keepalive-versus-interaction matrix unexecuted.
 
+### g5.lifecycle.idle.preview-startup
+
+- human-verified
+- [Preview idle-stop follow-up](../../chat_intent/preview-idle-stop/request.md), 2026-10-06; narrows the service exemption in [`g5.preview.service`](../features/preview_terminal.g5.md#preview-is-a-visible-service-with-explicit-lifetime).
+- `workspace_vm_service.rs::WORKSPACE_IDLE_PREDICATE` excludes a queued or running preview task without a `module.preview.ready` event; `idle_candidates` selects with it and `admit_stop` rechecks it under the workspace row lock for an idle stop, so a preview requested between selection and admission is not cancelled (CodeRabbit finding on PR #330). `api_workspace_vms.rs` covers a starting preview keeping the workspace, a ready one releasing it, and admission yielding to a preview queued after selection.
+
 ### g5.lifecycle.idle.task-completion
 
 - agent
```

| Governing block | Why relevant | Implementation must |
|---|---|---|
| [`g5.lifecycle.idle`](../../ontology/software_architecture/lifecycle.g5.md#idle-reflects-real-owner-activity) | Defines what blocks idle stop. | Keep service output and keepalives out of activity; the new rule adds a bounded-work case, not an activity source. |
| [`g5.lifecycle.idle.preview-startup`](../../ontology/software_architecture/lifecycle.g5.md#a-starting-preview-is-bounded-work) | New rule. | Idle selection skips a workspace whose current preview task is queued or running and has not reported ready; a ready preview does not block. |
| [`g5.preview.service`](../../ontology/features/preview_terminal.g5.md#preview-is-a-visible-service-with-explicit-lifetime) | Preview is exempt from bounded-work slots and wall-clock limits. | Do not add a slot or time limit; only idle selection changes. |
| [`g5.preview.service.stop-reason`](../../ontology/features/preview_terminal.g5.md#say-why-a-preview-ended) | New rule. | The interrupted preview's outcome carries a reason; the activity entry exposes and renders it. |
| [`g5.preview.service.readiness`](../../ontology/features/preview_terminal.g5.md#distinguish-preview-process-from-serving-readiness) | Readiness is the `module.preview.ready` event. | Reuse that event as the "ready" signal in idle selection. |

## Requested outcome and design

On QA, a 20-minute preview build was stopped at the 30-minute idle mark because idle selection counted only agent and command tasks as work, and the activity view then showed only "cancelled".

```
idle_candidates ──skip if── preview task queued/running AND no module.preview.ready
workspace stop ──interrupt_reason──▶ apply_outcome(preview) ──▶ tasks.outcome.reason
                                                                   └─▶ activity entry.reason ─▶ panel text
```

Idle selection gains one more exclusion in the same `NOT EXISTS` subquery: a preview task that is queued or running without a readiness event. The predicate is one constant, and an idle stop rechecks it under the workspace row lock at admission, so work requested between selection and admission is not interrupted (added after the CodeRabbit review). No new state: readiness already lives in `task_events`, and the preview intent is untouched. When a running preview ends after a stop interrupt, whether its module reports a cancelled outcome, the start is cancelled before acceptance, or the VM is lost, the settlement writes the task outcome with a `reason` mapped from `lifecycle.interrupt_reason` (`idle` → `idle_stop`, `user`/`delete` → `owner_stop`, `transfer`, `credential`/`capacity` → `start_required`, `drain`/`rollout` → `host_drain`; an owner cancel keeps `owner_cancel`). The attempt row keeps the module's raw payload. The activity DTO's task entry gains an optional `reason`, read from `outcome ->> 'reason'`, and the panel shows a short phrase under the title.

**Acceptance.** A workspace idle for longer than its timeout with a starting preview is not an idle candidate; the same workspace with a ready preview is. An idle-interrupted running preview ends `cancelled` with `reason = idle_stop`. The activity view says why.

**Risks.** A preview whose build never reaches ready keeps the VM up until the owner cancels it or rotation stops the VM, the same as a forgotten agent attempt; preview has no wall-clock limit by design. The reason text is only as good as the mapping; unknown reasons fall back to the raw word.

## Review summary

Self-review after implementation (not independent); findings and checks are in [review.md](review.md). Reviewed: the idle-selection exclusion and its only caller `reconcile_idle`; `apply_outcome` for a module-reported preview outcome and `apply_vm_loss` for a preview lost with its VM, both writing the reason (the loss path was added after the Codex review); the activity DTO's new `reason` field through `workspace_entries` to the panel. Runtime evidence: `api_workspace_vms.rs` (starting preview blocks idle selection, ready preview does not; the existing idle flush/stop test still passes), `api_task_lifecycle.rs` (an idle-interrupted running preview settles cancelled with `reason = idle_stop`, the attempt keeps the raw payload, the preview intent settles, and the activity route returns the reason; a preview lost with its VM records `vm_lost` or the in-progress stop's reason), `api_external_issues.rs` activity tests, and the panel's Vitest suite. Not executed: a browser E2E of the idle stop, which needs a timed idle reaper against a live VM; the QA incident path itself (a 20-minute build under a 30-minute timeout) is reproduced only by the integration tests above. Remaining uncertainty: whether a preview that never reports ready should eventually stop; it now keeps the VM up like a forgotten agent attempt, by design.
