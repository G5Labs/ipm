# Issue writes from the workspace: Done / Reopen and meeting-decision issues

Session of 2026-10-15 (Claude Code, Davide). Design: [ontology-plan](../../pr_ontology_diff_design/issue-tracker-writes/ontology-plan.md).

## Human instruction

> Ben merged some PRs this week so now we can write issues from a workspace with
> external-issue tracker write path (the broker). Couple of features were pending on it:
> creating issues from the sidebar; creating issues from meeting reviews. Let's implement those:
>
> - `g5.workbench.issues` — Mark done / Reopen / quick-add task are rendered but disabled
>   "until the broker exists".
> - `g5.workbench.meeting` — issue creation for delayed and out-of-scope decisions "waits
>   for an external publication path; until then the decision document records the
>   intended destination".
> - `g5.workbench.deferred` — issue quick-add and Done / Reopen from G5 explicitly deferred
>   pending "the tracker write broker and completion mapping".

Later the same day, on hearing that a meeting card took two presses (an option, then Answer
inside the card): "yes, collapse it to one press per option".

## Agent interpretation

- The quoted wording predates PR #478's merge (2026-10-13): quick-add already creates and
  links a related issue (`g5.workbench.issues`, amended 2026-10-13). The remaining work is
  Done / Reopen from the rail and the issue page, and Create issue from delayed and
  out-of-scope meeting decision cards.
- "The broker" is the guarded publication path comments and issue creation share
  (`g5.external-issues.broker`, first step). No outbox or retry worker exists; a hand
  transition does not need one.
- One press per option applies to meeting decision cards only (a `meeting-` set entry with
  the template's four options); ordinary question cards keep select-then-Answer, since the
  mock's one-press buttons were the meeting's, and the question store's other consumers were
  not discussed.
- Done / Reopen is read as the Manual policy's human state change under the person's own
  GitHub account (`g5.external-issues.completion`, design doc "Manual: a user publishes or
  changes state"), not as the mapped completion derived from G5 operations, which stays
  deferred. The instruction names the controls, so the completion mapping is not pulled in.
