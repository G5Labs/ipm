# Preview runs in workspace Activity — request and interpretation

## User instructions (2026-10-29, workspace `preview-status-in-activity`)

- "can you check how preview task show in the activity" — an investigation of the
  Activity page's presentation of preview tasks.
- "what is current badge?" and "what is these status related to preview run log status?
  is there a mapping" — the owner compared the Activity row's state with the preview
  dialog's run log stages and asked for the relation between them.
- "can you give me a complete map: Mapping to the Activity row" — a full table of every
  preview task state, what the run log and preview indicator show for it, and what the
  Activity row shows.
- "can you make a proposal with a current and proposal state? so we have better
  observation in the activity page." — a design, not an implementation.

## Findings presented in chat (the current state)

- Every preview run is one `task` row in Activity, named by the constant task name
  `Preview`, with the raw task status appended once ended (`Preview succeeded`,
  `Preview cancelled`, `Preview failed`). Ongoing runs are Current rows badged
  `Current · Queued` or `Current · Running`.
- Readiness and closing are tracked (the `module.preview.ready` event, the task spec's
  cancel request) and shown by the run log as Starting, Building, Serving on port N and
  Closing, but the task status does not change at those points, so Activity shows one
  `Running` from dispatch to the end.
- The row's only explanation is the outcome's top-level `reason`, which the stop path
  sets (idle stop, owner close, transfer, host drain, restart required, workspace stop,
  VM lost). An application crash, a module failure, a timeout, an interrupted attempt and
  a failed or killed cleanup set no such reason, so those rows read as a bare
  `Preview failed` or an ordinary `Preview cancelled`, while the run log shows the
  application exit, its stderr tail and the cleanup notice.
- Two rows pair contradictory words (`Preview failed` with body `Cancelled by you` when
  the VM was lost during a Close) and one prints a raw reason (`Ended: dispatch
  admission failed`).

## Agent interpretation

- "Better observation in the activity page" means that an owner reading Activity, without
  opening the preview dialog, can tell which run a row is, what stage an ongoing run is
  in, and how an ended run ended, in the same words the run log uses. It does not mean
  moving the application's output into Activity: the output stays in the hosted terminal
  session and its tail in the outcome (`g5.preview.service.exit-diagnostics`,
  `g5.preview.run-log`).
- The proposal keeps every row a projection of records G5 already holds
  (`g5.workbench.landing`): stages are derived from the task's events and spec, endings
  from the task's outcome. No new persisted record or event is proposed.
- The design is in
  [ontology-plan.md](../../pr_ontology_diff_design/preview-status-in-activity/ontology-plan.md);
  the decisions it leaves to the owner are the question set referenced from the Project
  Doc `preview-activity-proposal.md`.

## Owner decision (2026-10-29, after the mock)

- Asked what `web` in the mock was; told it stood for the repository the preview was
  started from, the owner said "i think we can remove repo name". The run is therefore
  named by its profile only (`Preview · full-stack`, or `default profile`); the
  repository is not part of the name or the row. The design, mock and the `run-identity`
  question were amended; the question's remaining choice is where the profile name is
  set (at task creation, or composed by Activity).

## Owner answers (2026-10-29)

"I answered all the questions, can you help me regenerate mock": the set `preview-activity`
holds the owner's decisions for the stage badge (Starting, Building, Serving on port N,
Closing), ended wording (ending verb plus a cause sentence), failure detail (one-line cause
only) and the link (ongoing and failed rows open the preview dialog's log). Run identity
shows the agent's proposal (profile in the task name at creation), not yet confirmed as a
decision. The mock was regenerated as today-versus-decided with each row's destination.

## Owner instruction (2026-10-29, on the regenerated mock)

"do not show the failures message in red": a failed run's cause sentence uses the ordinary
body colour like every other row body. The mock, screenshots and the plan's limits reflect it.

## Owner findings on navigation (2026-10-29)

- "now when i click preview in the activity, it goes to change page with empty content":
  every task row links to the task route, which opens the Workspace changes tab for any
  task kind (`taskOutputTabForTask`); a preview changes no files, so the tab is empty.
- "also when i click workspace rail, it should direct correctly": the rail's Current task
  row only opens the Activity page today (`onOpenActivity`).
- Agreed direction: no preview row ever opens Workspace changes. Rows with a log (ongoing,
  failed, a close whose cleanup failed or was killed) open the preview dialog with its log;
  every other preview row opens the preview dialog's launcher. The rail's Current preview
  row opens the same destination as the Activity row; command rows keep opening Activity.
- The owner confirmed this direction in chat ("yes, agreed.", 2026-10-29).

## Implementation (2026-10-29)

All five answers carry the owner's decision. Implemented on branch
`preview-status-in-activity` per the plan: profile-named preview tasks, the shared
preview-run projection, the `preview` object on Activity task entries, one wording module
for Activity, the rail and the run log, and preview rows opening the preview dialog (with
its log when the run has one) from the page and the rail. The owner also asked, after
implementation, for Codex review rounds until they converge, a PR, CodeRabbit comments
addressed, green CI, and an email summary.
