# Review — preview survives the idle budget while it starts

Self-review of the implementation against [ontology-plan.md](ontology-plan.md), 2026-10-06. Base: `main` at `a97f692a`, working tree.

## Commitments and checks

| Commitment | Actual path | Counterexample / check | Result |
|---|---|---|---|
| `g5.lifecycle.idle.preview-startup` — a not-yet-ready preview blocks idle stop; a ready one does not | `workspace_vm_service.rs::idle_candidates` `NOT EXISTS` subquery; sole caller `reconcile_idle` | Aged workspace with a running preview and no `module.preview.ready` event must not be a candidate; inserting the event must make it one | Runtime: `api_workspace_vms.rs::idle_reconciler_waits_for_a_starting_preview_but_not_a_ready_one` passes; `idle_reconciler_flushes_and_stops_an_inactive_workspace` still passes |
| `g5.lifecycle.idle` — service output and keepalives are not activity | `workspace_runtime.rs::record_activity` untouched | The change adds a bounded-work exclusion, not an activity write; no new writer of `last_activity_at` | Inspection: no new writer |
| `g5.preview.service` — no slot or wall-clock limit for previews | admission/slot accounting untouched | Only idle selection reads preview state | Inspection: diff touches no slot or deadline path |
| `g5.preview.service.stop-reason` — a stop-ended preview records why | `task_lifecycle.rs::apply_outcome` → `preview_outcome_with_reason` → `settle_task_row` outcome; `workspace_activity.rs` task entry `reason`; `WorkspaceActivityPanel.tsx::taskEndReason` | Running preview with `lifecycle.interrupt_reason = idle` receiving a bare `cancelled` outcome must end with `outcome.reason = idle_stop`, the attempt must keep the raw payload, and the activity route must expose it | Runtime: `api_task_lifecycle.rs::idle_interrupted_preview_records_why_it_ended` passes; panel Vitest asserts "Stopped because the workspace went idle" |
| `g5.preview.service.readiness` — readiness is the `module.preview.ready` event | reused as the ready signal in idle selection, same check as `workspace_preview_target` | — | Inspection |
| `g5.preview.service.replace` — replacement/loss settle the intent | `settle_preview_intent(..., "settled")` in `apply_outcome` unchanged | The new test asserts the intent is `settled` after the idle-interrupted outcome | Runtime |

## Findings

1. Test seed violated `tasks_time_order` (started before created) — test defect, fixed before the run passed. Not a product defect.
2. `apply_vm_loss` wrote no task outcome for a lost preview (`settle_task_row(..., None)`), so a VM-lost preview showed a bare status. Initially recorded as outside the request; the Codex review (gpt-6-astra, high, 2026-10-06) pointed out that `g5.preview.service.stop-reason` covers losses too. Fixed: a lost preview's task outcome now carries `vm_lost`, or the in-progress stop's mapped reason; regression test `api_task_lifecycle.rs::preview_lost_with_its_vm_records_why_it_ended`.

## External review

- Codex `gpt-6-astra` (high), run locally with `codex review --base origin/main`: one P2 finding (the VM-loss path above), addressed. It could not run the backend integration tests (database permissions in its sandbox) or the frontend typecheck (dependencies); both were run here.
- CodeRabbit on PR #330, first pass (3 comments), all addressed:
  1. Minor: `apply_start_disposition` settled a preview cancelled before its start with the raw disposition, bypassing the reason mapper. Fixed; test `preview_cancelled_before_its_start_records_why_it_ended`.
  2. Minor: a `"reason": null` in the module outcome counted as present. Fixed (`is_string()` check); the idle-interrupted test now sends a null reason.
  3. Major: a preview requested between `idle_candidates` and stop admission could be cancelled by the idle stop. Fixed: the idle predicate is one constant, and `admit_stop` rechecks it under the workspace row lock for `StopReason::Idle`, yielding (`StopAdmission::NotIdle`, outcome `running`) when work appeared; `reconcile_idle` does not count a yield as admitted. Test `idle_stop_admission_yields_to_a_preview_that_started_after_selection`.
- CI (`g5-ci-rust`) failed on the first push with clippy `type_complexity` for the activity task row tuple; fixed with a `TaskRow` alias. Local clippy on the changed crates is clean; a pre-existing `g5d-dispatch` warning on main is unrelated.

## Not executed

- Browser E2E of an idle stop interrupting a preview: needs a timed idle reaper against a live VM; not feasible in the local stack (see team memory on the local microVM).
- Pre-existing frontend `tsc` errors in `MarkdownPreview.tsx` are unrelated to this change; eslint and Vitest on the changed files pass.

## Rebase onto main (2026-10-07)

Rebased over #339 (Stop always completes) and #341. One conflict, in `workspace_vm_service.rs::idle_candidates`: main removed the `flush_block` exclusion because an idle stop now kills a workspace whose save fails (`g5.lifecycle.stop.force`, test `idle_stop_kills_a_workspace_whose_save_fails`), and this branch had moved the idle-timeout clause into `WORKSPACE_IDLE_PREDICATE`. Resolved by keeping main's rule and the shared predicate, so the query filters on `running`, `idle_retry_after` and the predicate only. The idle-startup and stop-reason blocks do not contradict the stop rewrite: admission is rechecked before the save window opens, and a preview ended by a forced stop records the stop's reason through `apply_vm_loss`. After the rebase: `api_workspace_vms` (63) and `api_task_lifecycle` (57) pass locally, as do the activity panel tests and `tsc`.
