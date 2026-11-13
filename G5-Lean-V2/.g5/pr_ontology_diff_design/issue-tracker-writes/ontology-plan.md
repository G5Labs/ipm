# Done / Reopen from the workspace and issues from meeting decisions

Follows the [issue #478 design](../issue-478-create-issue/ontology-plan.md), which built the
shared guarded write path for comments and issue creation. Intent: the
[chat intent](../../chat_intent/issue-tracker-writes/request.md) of 2026-10-15.

## Ontology changes

| Block | Change | Summary |
|---|---|---|
| [`g5.external-issues.transition`](../../ontology/features/external_issues.g5.md#close-or-reopen-a-linked-issue-by-hand) | New | A person closes (completed) or reopens a linked issue from its workspace as their own GitHub account, through the comment guards; stateless, idempotent, no agent path. |
| [`g5.external-issues.completion`](../../ontology/features/external_issues.g5.md#derive-completion-from-the-work-actually-accepted) | Amended | Manual is a person's state change, in the tracker or from G5; the mapped derivation stays deferred. |
| [`g5.external-issues.broker`](../../ontology/features/external_issues.g5.md#keep-integration-writes-at-one-enforcing-boundary) | Amended | Hand transitions use the shared first step; mapped (automatic) transitions still wait for the enforcing broker. |
| [`g5.workbench.issues`](../../ontology/features/workbench.g5.md#the-rail-lists-the-workspaces-tracker-issues) | Amended | The rail's Done box and the issue page's Mark as done / Reopen are live, with their disabled reasons and refusal display. |
| [`g5.workbench.meeting`](../../ontology/features/workbench.g5.md#review-a-meeting-through-a-question-set) | Amended | Which cards offer Create issue, what pre-fills it, and that creating answers the question with the issue reference in the comment. |
| [`g5.workbench.deferred`](../../ontology/features/workbench.g5.md#defer-the-rest-of-the-mock-to-v2) | Amended | Done / Reopen leaves the deferral list. |

## Governing ontology

- [`g5.external-issues.creation.admission`](../../ontology/features/external_issues.g5.md#admit-creation-through-the-comment-guards): the guards a transition reuses (organization switch, domain "G5 comments", non-public at send time, the person's own grant).
- [`g5.external-issues.publication`](../../ontology/features/external_issues.g5.md#publish-to-an-explicit-authorized-audience) and [`.automation`](../../ontology/features/external_issues.g5.md#attribute-automated-external-updates-honestly): a hand transition is the person's own write; no bot identity.
- [`g5.external-issues.cli`](../../ontology/features/external_issues.g5.md#give-the-workspace-agent-the-persons-issue-operations): closing and reopening stay out of the CLI, so no internal route is added.
- [`g5.documents.question.answer-checkpoint`](../../ontology/features/documents.g5.md#answering-is-an-explicit-save-of-one-file) and [`.decision`](../../ontology/features/documents.g5.md#a-decision-is-a-persons-answer): the issue reference reaches the record only through the owner's Answer of that one set file.
- [`g5.workbench.issues.create`](../../ontology/features/workbench.g5.md#home-creates-issues): the dialog the cards reuse.

## High-level design

### Close or reopen by hand

```text
Rail Done box ──────┐
Issue page Action ▾ ┴─ POST /workspaces/:id/issue-links/:linkId/state {state}
                        comment_db → external issues on → link in workspace
                        → comment_domain → comments_enabled → audience now
                        → acting_user → PATCH /repositories/{id}/issues/{n}
```

**One admission, the comment guards.** `change_linked_issue_state` sits beside
`publish_primary_issue_comment` and `create_tracker_issue` in
`g5-api-routes/src/routes/external_issues.rs` and reuses `comment_db` (workspace read plus
the organization's `workspaces:write`), `ensure_external_issues_enabled`, `comment_domain`,
the domain's `comments_enabled`, `domain_repository_audience` and `acting_user`. The link
is found among the workspace's links by id (a link of another workspace is `not_found`).
Nothing is screened: the request carries no text. The transaction closes before GitHub is
called.

**Stateless and idempotent.** GitHub's `PATCH …/issues/{number}` with `state` is
idempotent, so no idempotency key, marker or publication record is needed: a retry sends
the same state again. Close sends `state_reason: completed` ("Mark as done"); reopen sends
`state: open` and GitHub records `reopened`. G5 does not read the issue first: the person's
intent is the target state, and a no-op PATCH is harmless. What this gives up: closing an
issue someone else already closed as "not planned" keeps that reason (the response shows
it); a reopen after another person's close reopens it, which is what the person asked.

**Outcomes.** `applied` returns the issue summary GitHub answered with, so the rail updates
without a second read. A refusal returns the typed error (`comments_disabled`,
`public_destination`, `visibility_unknown`, `github_connection_required`, or GitHub's own
refusal, for example no push access). A timeout, network failure or 5xx returns
`outcome: unknown`; the client re-reads the links, whose live summary shows the state. A lost
browser answer (no response, or a 5xx from G5) is treated the same way by the client.

**Provider.** `g5-scm` gains `update_issue_state` under `RepositoryCapability::IssueWrite`
(its doc widens from "create" to "create or transition"); `send_issue_write` takes the
method so PATCH shares the one write path and its classification (refused vs unknown,
422 as the request's fault).

**Frontend.** `useChangeIssueState(workspaceId)` posts and, on `applied`, installs the
returned summary into the links cache and invalidates the issue detail. The rail's Done
box is enabled when the feature is on, the link's issue is readable, the viewer is not on
a review copy and GitHub is connected; otherwise it is disabled with that reason. A refusal
or unknown outcome shows a dismissable notice under the row (the unknown case also
refetches the links). The issue page's Action ▾ gets the same rule for Mark as done /
Reopen; its refusal is announced through the workspace toast.

### Create issue from a meeting decision card

```text
ReferencedQuestionCard (set id `meeting-…`, selected option delay | out-of-scope)
  └─ Create issue… → CreateIssueDialog(initialTitle = decision, initialBody = context…)
       └─ onCreated(issue) → text += "owner/name#N <url>" → Answer (selected, text, rationale)
```

**Which cards.** The card recognises a meeting decision by the template's two fixed facts:
the set id starts with `meeting-` and the entry offers the `delay` and `out-of-scope`
options. The control appears when the current selection is one of those two, whether the
question is still open or already answered that way (the card's selection starts from the
answer). It is offered only on an answerable card (owner, live file) and is disabled with
the creation target's reason, as quick-add is; it is absent when external issues are off or
the route has no organization.

**Pre-fill.** Title: the decision (`entry.prompt`). Body: the context line (detail, decider,
target document), a "Decision: Delay | Out of scope" line with the owner's comment so far,
and the meeting's title. No parent: the issue is not a task of this workspace.

**One press decides.** On a meeting card an option press is the owner's Answer with the comment
typed so far (the mock's one-press buttons); the decided option is inert, another option
changes the answer, and the Answer / Clear controls stay for comment-only changes. Ordinary
cards keep select-then-Answer. The e2e meeting step presses Accept once.

**Record through the Answer.** When the dialog reports the issue, the card appends
`owner/name#N <url>` to the comment text and saves the answer with the selected option,
text and rationale: one explicit owner press (Create in the dialog) both creates the issue
and decides the question, under the existing answer path (revision rule, checkpoint,
applied-not-published retry). If that save is refused, the issue exists; the reference
stays in the comment field and the card's notice says so, so the owner presses Answer
again. The issue is not linked to the workspace.

## Decision notes

- **Manual, not derived.** The design doc's Manual policy is "a user publishes or changes
  state". The earlier reading that Done / Reopen wait for the completion mapping conflated
  the human change with the mapped derivation; only the latter stays deferred.
- **Who may close.** The same people who may comment from the workspace (organization
  `workspaces:write` and a readable workspace), under their own GitHub permissions. Linking
  stays owner-only; a transition changes no link.
- **Writes follow no redirect.** Review found that the shared transport followed same-origin
  redirects for writes too, so a transferred issue's PATCH could land in a repository the
  admission never checked. Writes now go through a client with no redirect policy; the 3xx
  is classified as a refusal (`g5.external-issues.broker`). Reads keep same-origin redirects.
- **Shared admission.** Comments, creation and transitions admit through one
  `admit_person_write` (switch, audience, grant), so the rules cannot drift.
- **Hook hygiene.** The state-change mutation binds the workspace at submission and cancels
  in-flight link reads around the write, so an older read cannot restore the previous state
  and a workspace switch cannot misroute the cache update.
- **No agent transition.** `g5.external-issues.cli` keeps close and reopen out of the CLI;
  the internal routes are untouched.
- **No pre-read, no record.** A transition carries no payload to confirm and is idempotent,
  so neither the comment record nor the creation marker applies. Rejected: a read-then-write
  with an expected state (a second GitHub call per press for a race the person resolves by
  looking at the row).
- **Generic card, template facts.** The card does not learn the meeting template; it keys
  on the set id prefix and option ids the template fixes, both already relied on by the
  Apply template and the e2e fixture.
- **One press on meeting cards only.** The human asked for the mock's one press per option
  (2026-10-15). Keyed on the same template facts as Create issue; other question sets were not
  discussed, so they keep the explicit Answer press.
- **Immediate save after creation.** Rejected: append the reference and wait for a separate
  Answer press (an issue created but never recorded on a navigation away), and a separate
  write path for the reference (a second writer of the set file).

## Acceptance conditions

- A person closes a linked issue as completed and reopens it under their own token; the
  response carries GitHub's summary. Each guard refuses with nothing sent: external issues
  off, comments off, public, visibility unknown, no grant, link of another workspace.
- GitHub's refusal is returned as its typed error; a 5xx reports `unknown`.
- The rail: Done box enabled only under the rule above, with each disabled reason; a press
  sends the state and updates the row (and every cached list linking that issue); a refusal
  shows the notice; unknown, or a lost answer, re-reads. A review copy is refused by the route.
- The dialog answers the revision it was opened for and stays open if another tab changes the
  decision meanwhile; a recorded reference is not restored
  after a later Clear; the saved option can be reselected after a refused change.
- The issue page: Mark as done / Reopen send the state; a refusal is announced.
- The card: on a meeting card an option press answers with the comment, the decided option
  is inert, ordinary cards keep two presses. Create issue appears only for `meeting-` sets
  with Delay or Out of scope chosen, disabled with the target's reason; the dialog opens
  pre-filled; after creation the answer is saved again with the reference appended; a
  refused save keeps the reference in the field with a notice.
- Backend scenarios in `api_external_issues.rs`; provider classification in `g5-scm`; the
  rail, page and card in their vitest suites. Live GitHub is not exercised.
