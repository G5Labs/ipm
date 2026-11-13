# Meeting decisions as tasks: three buttons, no votes, arrival cue, question index

Request: [chat intent](../../chat_intent/meeting-task-buttons/request.md) (2026-10-22).
Mocks: [meeting-task-buttons.html](mocks/meeting-task-buttons.html) (the card, the sidebar in rail and expanded modes, the spotlight, reduced motion, link failure) and [review-meeting-entry-options.html](mocks/review-meeting-entry-options.html) (ribbon entry variants 1A–1D, start-behaviour variants 2A–2D).

## Ontology changes

- `g5.workbench.meeting` (documents.g5.md) rewritten: the template's three options, one-press task creation with direct creation for a trusted destination and the shared dialog for an untrusted one, attach = quick-add's related link with the primary issue as parent, detach = no parent and no link, no second issue once the comment carries a reference, no vote controls on meeting cards, the earlier four-option set kept recognised.
- `g5.documents.question-index` (new): the Outline lists `[Q<N>.]` records with their state.
- `g5.documents.question-votes`: one clause pointing at the meeting exception.
- `g5.workbench.issues.arrival` (new, issues.g5.md): the data-driven spotlight and rail badge pulse for a link that arrives after first load.

## Governing ontology

- [g5.external-issues.creation](../../ontology/features/issues.g5.md#g5.external-issues.creation) and its admission and recovery rules: the person's own account, payload review for an untrusted destination, stateless creation with a per-submission key, exactly-once not promised.
- [g5.workbench.issues.create](../../ontology/features/issues.g5.md#g5.workbench.issues.create): quick-add's create-then-link, parent only when the primary issue is in the creation repository, keep the issue when linking fails.
- [g5.documents.question-decisions](../../ontology/features/documents.g5.md#g5.documents.question-decisions): the Answer is the owner's explicit save; revision checks; retry without answering twice.
- [g5.ui.unavailable-actions](../../ontology/features/ui.g5.md#g5.ui.unavailable-actions): disabled with the reason, never hidden.

## High-level design

### The three options

The extraction template (`workbench_builtins/actions.yaml`, all three phases) emits `attach` "Create a task and attach to the current workspace", `detach` "Create a detached task" and `reject` "Reject", with `allowFreeText` for the comment. Apply accepted decisions treats `attach` and `detach` (and the legacy `accept`) as accepted and leaves `reject` (and the legacy `delay`, `out-of-scope`) untouched; its reply lists the task references the comments carry. The e2e fake provider's meeting fixture emits the same three options.

`lib/meetingDecisions.ts` recognises two shapes of meeting card: the task set (three options) and the legacy set (four). The card's behaviour branches on the shape once; everything legacy keeps the merged code paths.

### One press on a task option

Owner of the sequence: `ReferencedQuestionCard`. Order of writes, chosen so each durable write happens before the next external one:

1. **Create.** Trusted destination: `useCreateIssue` directly with a fresh idempotency key per press, title = the decision, body = context, the comment and the meeting title, parent = the primary issue when it is in the creation repository (attach only). Untrusted destination: the shared `CreateIssueDialog` prefilled the same way; its own review and recovery apply, and `onCreated` continues the sequence. An unclear outcome from direct creation (`outcome_unknown`, `not_found`) shows a notice with Check again, which resends the same key with `checkOnly`, mirroring the dialog's unresolved state; nothing else is sent on G5's own.
2. **Record.** Answer with the option and the comment plus the issue reference, against the revision the press was made on (the existing moved-revision handling applies; a refused save keeps the reference in the comment, as today).
3. **Link** (attach only). `useLinkWorkspaceIssue` with `role: related`. A refused link keeps the issue and the recorded decision and shows the quick-add notice (link it from Issues & tasks).

When the recorded comment already carries an issue reference (created from this card earlier), a task press skips step 1: attach records and links that reference, detach records only. Reject records only.

The pressed option shows "Creating task…" while step 1 runs; the card's other inputs are disabled until the sequence settles. A task option is disabled with the reason when external issues are off, no account is connected, the target is unavailable, the viewer is not the attached owner, or (attach) the links have not loaded. A visually hidden `role="status"` line in the card announces the outcome ("Task KEY attached to this workspace", "Task KEY created, not linked to this workspace", or the failure).

### No votes on meeting cards

Meeting cards do not render the vote controls, the tally or the vote texts, and do not issue the votes query. The comment field's placeholder reads "Add a comment (optional)". Everything else about votes is unchanged.

### Arrival cue in Issues & tasks

`WorkspaceIssuesSection` remembers the link ids it has rendered after the first successful load; a row whose id is new gets the `g5-row-spotlight` class once (1200ms info tint fading, `--g5-ease-out`; under reduced motion a static tint for 1500ms). The rail's Issues & tasks icon (`TreeNode`) reads the same links query and shows the count as its badge; when the count rises after first load the badge gets a one-shot pulse (`g5-presence-once`, already defined for Home's presence dot; none under reduced motion). No whole-panel flash, nothing on first load, nothing on failure, nothing for a detached task (it never arrives in this list).

### Question index in the Outline

`EditorOutline` takes the document's Q records (`blocks.recordSpans` of kind `Q`) and the accents the editor already derives per record start. Entries merge with headings in block order; a record entry shows a state dot (open: warn, proposed: muted warn, decided: ok) and the record's title, indented under the nearest heading, and scrolls to the record's start block. I and D records are not listed, so the Core Ontology editor's outline does not change.

### Ribbon entry and start behaviour (decided: 1C)

The variants were presented as mocks; the recommendation was 1A + 2A, and the owner chose 1C. The ribbon's `review-meeting` action is now `kind: open, target: meeting`: one button opening the transcript dialog. The dialog offers two sources, a file to upload (stored under `meetings/`, then extracted) or a transcript already stored there (extracted as it is), and its primary action starts the extraction; that press is the confirmation, so question 2 needs nothing further. The dialog also lists the decisions documents already extracted (`meetings/*.decisions.md`), each opening in the centre, so a person returns to earlier meetings from the same entry. `apply-decisions` is removed (the owner's follow-up: every accepted decision already becomes a task at the press). `extract-decisions` stays in the config as a hidden action of the Analysis group (`hidden: true`, a new optional flag on the action DTO): the chat start route resolves the template by action id and config revision on the server, so the prompt cannot live only on the client, and the ribbon skips hidden actions. The "Open questions" button is renamed "Find gaps" (the owner's pick on 2026-10-22; the template surfaces open decisions and conflicting intents; alternatives offered: Find blind spots, Detect conflicts, Stress-test requirements, Surface risks).

## Decision notes

- **Direct creation from the press, not always the dialog.** The walkthrough's intent is one press per decision. The admission rule needs a payload review only for an untrusted destination, so the dialog appears exactly then. The direct path reuses the same `useCreateIssue` guards and key discipline.
- **Create before record, record before link.** The reference must be in the recorded decision before a second external write; a link failure then loses nothing. The alternative (link before record) could leave a linked issue the decision does not name.
- **No second issue on a changed mind.** A question decided with a task carries the reference in its comment; re-pressing a task option reuses it. The alternative, a per-question issue ledger, is a second decision store the ontology forbids.
- **Data-driven cue.** The spotlight keys on the link id arriving in the list, not on the press, so it also covers links made in another tab or by the agent, and never fires for a failed link.
- **Votes hidden on meeting cards only.** One press decides there; ordinary cards still collect votes before the owner answers. The vote store, page and API are untouched.
- **Legacy sets stay recognised.** Decision documents extracted before this change keep working; a re-extraction with the same set id edits the entries to the new options.

## Acceptance conditions

- A task press on a trusted destination creates, records with the reference, and (attach) links; the card shows the created key and the sidebar list gains the row with the spotlight class and the rail badge count rises. Verified by component tests (`ReferencedQuestionCard.test.tsx`, `WorkspaceIssuesSection.test.tsx`) and the e2e meeting flow (three options in the set; reject records; task options disabled with the reason without GitHub).
- An untrusted destination opens the dialog prefilled; its creation continues the sequence.
- A refused link keeps the issue and the decision and shows the notice; nothing is spotlighted.
- Meeting cards render no vote controls and request no votes; ordinary cards are unchanged.
- The Outline lists each Q record with its state and scrolls to it.
- Reduced motion: no scale, no pulse, a static tint that clears.

## Implementation evidence — 2026-10-22

Branch `feat/meeting-task-buttons` (worktree `/Users/davide/G5-Lean-wt/meeting-task-buttons`), uncommitted at the time of writing.

- Template and fixtures: `workbench_builtins/actions.yaml` (three options, Review meeting as an open action, hidden `extract-decisions`, `apply-decisions` removed, Find gaps), `workbenchBuiltins.generated.ts` regenerated, `fake-provider-cli` fixture and its CLI test, `workbench_config.rs` test, `g5-api` `tests/api_workbench.rs` id lists, `workbenchIcons.ts`. `cargo test -p g5-api-core --lib workbench_config`, `cargo test -p g5-api --test integration workbench` (19 tests) and `cargo test -p fake-provider-cli` pass (the two `steering.rs` failures also fail on main).
- Card: `ReferencedQuestionCard.tsx`, `lib/meetingDecisions.ts`; dialog: `CreateProjectDocModal.tsx`, `WorkspacePage.tsx`; cue: `WorkspaceIssuesSection.tsx`, `TreeNode.tsx` (`IssuesRailBadge`), `index.css`; index: `SpecEditor.tsx` (`EditorOutline`).
- Vitest: `meetingDecisions.test.ts`, `ReferencedQuestionCard.test.tsx` (legacy cards unchanged; task cards: labels and no votes, attach with parent and link and status, detach, refused link, untrusted dialog review, existing reference reuse, Reject, reader read-only), `WorkspaceIssuesSection.test.tsx` (arrival spotlight), `SpecEditor.outline.test.tsx`, `CreateProjectDocModal.test.tsx` (two sources, decisions list), `WorkspacePageAlignment.test.tsx`, `ActionRibbon.test.tsx`, `TreeNode.sectionOrder.test.tsx`, `journeySteps.test.ts`, `button-testid-coverage`: 283 tests pass (11 files). `tsc --noEmit` clean; eslint reports no errors.
- Gaps: the e2e meeting flow (`alignment-shell.spec.ts`) was updated for the dialog, the three options and Reject but not executed locally (the local e2e stack is unavailable on this machine); the rail badge pulse and the row spotlight are asserted by class and attribute, not visually; the real GitHub creation path has no automated coverage (no GitHub in the e2e stack), as before.
