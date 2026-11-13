# Preview runs in workspace Activity: stage, ending and identity

Source: [chat intent](../../chat_intent/preview-status-in-activity/request.md) (2026-10-29).
Decisions to confirm: the question set `preview-activity` referenced from the Project Doc
`preview-activity-proposal.md` in the docs repository. Status (2026-10-29): the owner answered
every question with the recommended option; four carry a decision (stage badge, ending wording,
headline-only failure detail, rows open the preview log) and run identity (profile at task
creation) was confirmed later the same day. Implemented 2026-10-29; evidence is listed under
`g5.ui.activity.preview-row` in `ui.g5.md`.
Mock: [mocks/preview-activity-states.html](mocks/preview-activity-states.html) renders every
preview task state as an Activity row, today beside the decided rendering with each row's
link destination, plus the rail's Current rows (`?view=current|proposed`, `?mode=light`);
rendered in `screenshots/` (2026-10-29).

One bounded change to the Activity projection and its row: a preview run's row names the
run, carries the run's stage while it is ongoing, and states how it ended in the same words
as the preview dialog's run log. Everything shown is derived from records G5 already holds.

## Current state

The Activity record projects every non-agent task as one `task` row
(`workspace_activity.rs::task_entry`); ongoing ones are Current rows
(`current_entries`). For a preview the row is:

| Task status | Facts held but not read by Activity | Run log shows | Activity shows |
|---|---|---|---|
| queued | — | Queued, Starting | `Preview` · Current · Queued |
| running | no module event | Starting | `Preview` · Current · Running |
| running | module started (`invocation.started` / `module.*` event) | Building | `Preview` · Current · Running |
| running | `module.preview.ready` event, port in spec | Serving on port N | `Preview` · Current · Running |
| running | spec `lifecycle.cancel_requested` | Closing | `Preview` · Current · Running |
| succeeded | — | Closed | `Preview succeeded` |
| cancelled | outcome `reason` (owner_cancel, idle_stop, owner_stop, workspace_stop, transfer, start_required, host_drain) | Closed | `Preview cancelled` + stop sentence |
| cancelled | outcome `detail.close` failed or killed | Closed; cleanup failed + notice | `Preview cancelled` + stop sentence |
| failed | outcome `result.status` app_exited, exit code or signal, stderr tail | Failed while building + "Application exited with status N" + tail | `Preview failed` |
| failed | module failure with `result.message` or `detail.reason` | "Preview failed: message" | `Preview failed` |
| failed | module reported timed_out or interrupted | Failed (no headline) | `Preview failed` |
| failed | reason vm_lost | Failed | `Preview failed` + "Ended because the workspace VM was lost" |
| failed | VM lost during a Close, reason owner_cancel | Failed | `Preview failed` + "Cancelled by you" |
| failed | reason dispatch_admission_failed, `detail` text | Failed | `Preview failed` + "Ended: dispatch admission failed" |

Three gaps follow. The five live stages collapse to one `Running`. Every ending that the
stop path did not name reads as a bare `Preview failed` or an ordinary `Preview cancelled`,
although the outcome holds the cause (`g5.preview.service.exit-diagnostics`). Every row is
named `Preview`, so two runs with different profiles are indistinguishable. Two smaller
defects: the title and body can contradict each other (failed / "Cancelled by you"), and an
admission failure prints its raw reason. The run log's own failure reader also misses
timed-out and interrupted outcomes (it matches only an outcome event whose status is
literally `failed`), so aligning the two surfaces on one wording helper fixes that too.

Evidence: `workspace_activity.rs` (task query, `task_entry`, `current_entries`),
`task_lifecycle.rs` (`preview_outcome_with_reason`, `preview_stop_reason`,
`workspace_preview_view`'s ready/closing/close derivation, the `Preview` name at insert),
`preview_serve.rs::app_failure_outcome`, `WorkspaceActivityPanel.tsx` (`task` case,
`TASK_END_REASONS`, `currentTaskStatus`), `previewRunLogModel.ts` (`previewStages`,
`previewFailure`), `previewIndicator.ts`.

## Proposed state

| Task status | Facts | Activity title | Badge / body |
|---|---|---|---|
| queued | — | `Preview · full-stack` | Current · Queued |
| running | no module event | same | Current · Starting |
| running | module started | same | Current · Building |
| running | ready | same | Current · Serving on port N |
| running | cancel requested | same | Current · Closing |
| succeeded | — | `Preview ended · full-stack` | "The application exited on its own" |
| cancelled | owner_cancel, close clean | `Preview closed · …` | "Closed by you" |
| cancelled | owner_cancel, close failed / killed | `Preview closed · …` | "Closed by you, but the cleanup command failed" / "… did not finish in time and was killed" |
| cancelled | platform stop reason | `Preview stopped · …` | the existing stop sentence (idle, workspace stop, transfer, restart required, host drain) |
| failed | app_exited | `Preview failed · …` | "Application exited with status N" (or "signal N") |
| failed | module message / detail reason | `Preview failed · …` | "Preview failed: message" |
| failed | timed_out, limit_exceeded | `Preview failed · …` | "Exceeded its time limit" |
| failed | interrupted | `Preview failed · …` | "Interrupted: supervision of the run was lost" |
| failed | vm_lost | `Preview failed · …` | "Lost with the workspace VM" (+ the stop sentence when a stop interrupt was under way) |
| cancelled | owner_cancel, close lost (VM lost during the Close; recorded by this change) | `Preview closed · …` | "Closed by you, but the workspace VM was lost before cleanup finished" |
| failed | dispatch_admission_failed | `Preview did not start · …` | "Could not be started: " + detail |

The row's name part (`full-stack`) is the profile, or `default profile` when none was
named; the repository is not shown (owner decision, 2026-10-29). The row still shows once, moves from Current to the
record at its end time, and keeps its Eye icon, Tasks filter group and rail presence while
Current. The rail's Current row shows the same stage word in place of Running.

## Ontology changes

| Block | Change | Why |
|---|---|---|
| `g5.ui.activity.preview-row` (new D, `ui.g5.md`, beside `g5.ui.activity-current`) | A preview run is one Activity row named by its profile. While ongoing it carries its stage: queued, starting, building, serving on its port, closing. Ended, it states how it ended in the words of the run log: ended on its own, closed by the owner (naming a failed or killed cleanup), stopped by the platform with the retained stop reason, failed with the application's exit or the module's cause, or did not start. Stages and endings are derived from the task's events, spec and outcome; nothing is persisted and no extra rows mark readiness or closing. The application's output and stderr tail stay in the run log. A preview row, on the page and in the rail, opens the preview dialog: with its log when the run has one, else its launcher; never the task's Workspace changes. | Owner request for better observation of preview runs in Activity (2026-10-29). |
| `g5.ui.activity-current` (amended) | "each row carrying its state (running, waiting for input, queued)" gains: a preview row carries its run stage instead of running, per `g5.ui.activity.preview-row`. | Same request. |
| `g5.preview.run-log` (amended, one clause) | The stage strip and the failure notice share their wording with the Activity row, so timed-out and interrupted runs get a notice too. | One vocabulary for both surfaces; fixes the run log's missed outcomes. |
| `g5.preview.service.stop-reason`, `g5.preview.service.exit-diagnostics` | Unchanged: both already require the reason and the diagnostics to be retained and shown; this design adds Activity as a second reader. | — |

The owner decided run identity: the preview task's name at creation is `Preview · <profile>`
(or `Preview · default profile`), recorded in the new D block and implemented in
`start_workspace_preview` (the name is the run's durable identity in every task list, not
only Activity).

## Governing ontology

- [g5.workbench.landing](../../ontology/features/ui.g5.md#g5.workbench.landing): every row
  is a projection of a durable record and the feed derives no time or event the database
  does not hold. Stages come from `task_events` rows and the task spec; endings from
  `tasks.outcome`.
- [g5.ui.activity-current](../../ontology/features/ui.g5.md#g5.ui.activity-current): one
  row per ongoing item, shown once, outside the filters, refreshed while shown. The stage
  rides on the existing 5 s refresh of the Current read.
- [g5.preview.run-log](../../ontology/features/ui.g5.md#g5.preview.run-log) and
  [g5.preview.run-log.ended](../../ontology/features/ui.g5.md#g5.preview.run-log.ended):
  the dialog remains the place for the application's output; a cleanly closed run keeps
  no log link, so Activity never promises one.
- [g5.preview.service.stop-reason](../../ontology/software_architecture/workspace_vm.g5.md#g5.preview.service.stop-reason)
  and [g5.preview.service.exit-diagnostics](../../ontology/software_architecture/workspace_vm.g5.md#g5.preview.service.exit-diagnostics):
  the reason and the exit diagnostics are already in the outcome; Activity reads, never
  rewrites, them.
- [g5.vm.preview-stop](../../ontology/software_architecture/workspace_vm.g5.md#g5.vm.preview-stop):
  Close is recorded on the task (the spec's cancel request), which is why "closing" is a
  derived stage and not an event.

## Design

### Owner and derivation

`workspace_activity.rs` (the activity projection) owns what a preview row says. It does
not grow a second lifecycle: it reads the same facts `task_lifecycle.rs` reads for the
preview view, through one pure projection owned by the Workspace context
(`services/workspace/preview_run.rs`: name at creation, stage from started, ready and
closing, ending from the outcome), which the Agents' task lifecycle imports; the service
layering contract (`service_context_layering.rs`) forbids the reverse dependency. So the
dialog and Activity cannot drift. The facts and their sources:

- started: a `task_events` row for the task with kind `invocation.started` or `module.*`.
- ready: a `task_events` row with kind `module.preview.ready` (the view's existing query).
- closing: task status running and spec `lifecycle.cancel_requested = true`.
- port: spec `preview.port`.
- close: outcome `detail.close` when cancelled (clean, failed, killed).
- failure: outcome `status`, `reason`, `result.status`, `result.app_exit_code`,
  `result.app_signal`, `result.message`, `detail.reason`, `detail` (text form).
- identity: spec `preview.profile` (no repository join; the owner dropped the repository name).

The live set is bounded by execution slots (one preview per workspace), so the two
EXISTS subqueries per live preview are negligible. The record page reads the same row shape; ended rows carry no stage, but the started
fact still tells an unstarted stop from a stopped run.

### Task entry DTO

`ActivityEntryBody::Task` gains an optional `preview` object, present only for preview
tasks: `{ profile, port, stage, ending }` where `stage` is one of queued,
starting, building, serving, closing (absent once ended) and `ending` is a small
structured value the client turns into words: `{ kind, cause, reason, close, exitCode,
signal, message }` with `kind` in ended, closed, stopped, failed, did_not_start and `cause`
naming a failure's origin (app_exited, module, timed_out, interrupted, vm_lost) or an
unstarted stop. The server decides `kind`, `cause` and `stage` (one place, tested in Rust);
the client owns the sentences. Non-preview
tasks are unchanged.

### Wording in one place

The preview view (`WorkspacePreviewView`) carries the same structured `ending`, so the
dialog's launcher words a failed run with the row's sentence and offers its log only when the
run kept one (review round 5).

A new `previewRunWords.ts` (frontend `lib/`) turns `stage` into the badge word and
`ending` into the title verb and body sentence. `previewRunLogModel.ts` uses it for the
stage strip's labels and for the failure headline (and so gains the timed-out and
interrupted notices), `WorkspaceActivityPanel.tsx` for the row and badge,
`ActivityRailSection.tsx` for the Current state word. `TASK_END_REASONS` stays for
command tasks; the helper carries its own stop-sentence table, and the server normalizes the
reasons other writers record on queued previews (host loss, ownership transfer) into the
same vocabulary.

### Title, link and identity

- Title: `Preview` + ending verb (none while ongoing) + ` · <profile>`.
- Link (decided, refined 2026-10-29 after the owner hit the empty page): no preview row
  opens the task route, whose Workspace changes tab is always empty for a preview
  (`taskOutputTabForTask` returns it for every kind). Rows with a log (ongoing, failed, a
  close whose cleanup failed or was killed) open the preview dialog with its log, the same
  view as the dialog's View log; every other preview row opens the dialog's launcher, since
  a cleanly closed run keeps no log link (`g5.preview.run-log.ended`). The rail's Current
  preview row opens the same destination; command rows keep opening Activity. This needs a
  page-level "open preview" signal (with or without log) from `WorkspacePage.tsx` to
  `PreviewControl.tsx`, whose dialog and log are local state today; the DTO's `ending.kind`
  and `stage` decide the destination on the client, so Activity holds no second rule.
- Identity: recommended at the source, the task name at insert in
  `start_workspace_preview` (`Preview · full-stack`), which improves the Jobs tree
  and `g5d workspace show` for free and needs no join in the projection; the DTO still
  carries `profile` so the client never parses the name. The owner dropped the repository
  name and confirmed naming at creation (2026-10-29); implemented and asserted by the start
  API tests.

### Alternatives considered

- Separate Activity rows for "ready" and "closed": rejected. They would be derived events
  the database does not hold as rows, would break "one row per run, shown once", and the
  run's end already marks closing.
- Changing the task status at ready (a new status): rejected. Task status is shared by
  every task kind and by dispatch, cancellation and idle reclamation; a preview-only state
  there would leak into those owners for a display need.
- Reading `workspace_vms.preview_intent.status` for readiness: rejected. It covers only the
  workspace's current preview and is rewritten by the next start; `task_events` is per
  run and already what the preview view reads.
- Writing the ending sentence server-side: rejected. The client already owns the run log's
  wording; a structured `ending` keeps one vocabulary without duplicating prose in Rust.

### Limits and assumptions

- Stage freshness is the Current read's 5 s poll; a run that starts and ends between polls
  shows only in the record, as today.
- "Serving on port N" names the guest port from the spec, as the run log does; the
  isolated origin is not shown in Activity.
- Failure sentences use the ordinary body colour, not red (owner decision, 2026-10-29).
- The stderr tail is not shown in Activity; its one-line headline is. The tail remains in
  the run log while the VM runs (owner decision recorded on `g5.preview.run-log.ended`).
- Existing ended rows change wording on deploy (same records, new projection); no
  migration.

### Tests

- `api_workspace_timeline.rs`: a preview task with a started event, a ready event and a
  cancel request yields stage starting → building → serving → closing on the current read;
  ended previews (killed close, idle stop, app exit, admission failure, exited on its own)
  yield the expected `ending`; command tasks carry no `preview`. The remaining outcome
  shapes (each stop reason, signal exit, module message, timed out, output limit,
  interrupted, VM or host lost before and after start, a Close cut short) are covered by
  the `task_lifecycle.rs` unit test of the projection.
- `workspace_activity.rs` unit tests: serialization of the `preview` object and its absence.
- `previewRunWords.test.ts`: every stage and ending to its words.
- `WorkspaceActivityPanel.test.tsx` and `ActivityRailSection.test.tsx`: title, badge word
  and body for the representative rows; no contradictory pairs; a preview row (page and
  rail) opens the preview dialog with or without its log and never the changes tab.
- `previewRunLogModel.test.ts`: the strip's labels unchanged for existing cases, plus
  notices for timed-out and interrupted outcomes.
- Task naming: the start API tests in `api_workspace_vms.rs` assert the inserted name
  (`Preview · default profile`, `Preview · full-stack`); the unit test
  `preview_tasks_are_named_by_their_profile` covers the helper.
