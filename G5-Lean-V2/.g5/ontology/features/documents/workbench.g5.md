# Documents: workbench actions

Explainer templates, meeting-decision and mock dialogs, and the question outline. See [Documents](../documents.g5.md#g5.documents).

## Explainer templates

<a id="g5.documents.explainer-catalog"></a>
[D · g5.documents.explainer-catalog]

Offer reusable explainer prompts from built-ins plus valid saved templates in the repository’s committed configuration. Owner edits use the ordinary [live-file](../../software_architecture/workspace_vm.g5.md#g5.runtime.files.mutation) and [publication](../../software_architecture/git.g5.md#g5.git.save-publication) boundaries; saving a template does not itself publish it.

<a id="g5.documents.explainer"></a>
[D · g5.documents.explainer]

Explainers are reusable prompts, from built-ins and saved templates, that generate documents about the workspace.

<a id="g5.documents.explainer.templates"></a>
[D · g5.documents.explainer.templates]

Saving a user explainer template must not silently replace an existing template: replacement requires an explicit choice, and built-in template identities remain reserved.

## Decision follow-up

<a id="g5.workbench.meeting"></a>
[D · g5.workbench.meeting]

The ribbon's Review meeting is one dialog, offered while the workspace is writable and greyed with the reason otherwise. It takes a transcript to upload, stored as Markdown under `meetings/`, or one already stored there, lists earlier decisions documents to return to, and its primary action seeds the chat composer with the extraction prompt over the chosen transcript, as every [chat action](../ui.g5.md#g5.workbench.chat-actions) does, so the person sends it.

- Uploading a transcript needs the running VM, since it lands in the docs checkout, and the upload path says so with Start in reach; choosing a stored transcript only seeds the extraction chat, which [Start and send](../chat.g5.md#g5.chat.start-and-send) can carry, so it stays open while the VM is stopped.
- The extraction template is a hidden ribbon action the dialog seeds by id; there is no separate apply step, since each accepted decision becomes a task at the press.
- A meeting decision card is a `meeting-` set entry whose option press is the owner's Answer in one press, carrying the comment typed so far; other question cards keep select-then-Answer. It uses the [question decision owner](discussion.g5.md#g5.documents.question-decisions), with no second decision store.
- The three options are Create a task and attach to the current workspace, Create a detached task, and Reject. Reject records the decision and creates nothing.
- A task option creates a tracker issue through shared [issue creation](../issues.g5.md#g5.external-issues.creation) prefilled from the decision, then records the decision with the issue's reference appended; attach links the issue to the workspace as related with the primary issue as parent under the [quick-add rule](../issues.g5.md#g5.workbench.issues.create), and detach adds no parent or link.
- A trusted destination creates from the press; an untrusted one opens the shared dialog for payload review, and the press completes when the dialog creates.
- A task option is disabled with the reason while creation is impossible, or for attach while the links are unknown.
- A decision whose recorded comment already carries an issue reference creates no second issue: attach links that issue, detach only re-records. A refused save or link keeps the issue and says so.
- Meeting cards show no [vote controls](discussion.g5.md#g5.documents.question-votes) and read no votes, since one press decides.
- Cards still carrying the earlier Accept / Delay / Out of scope / Reject options keep their behaviour: Accept and Reject record only, while Delay and Out of scope offer Create issue prefilled with no parent or link.

<a id="g5.workbench.mock"></a>
[D · g5.workbench.mock]

Mocks are Project Docs: the clickable HTML screens live under `mocks/` in the workspace's Project Docs folder and open in the ordinary reader, with no preview profile, served copy or separate Preview control. The ribbon's Build mock is one dialog, like Review meeting: it lists this workspace's mocks newest first, and its primary action seeds the chat composer with the mock-building prompt, as every [chat action](../ui.g5.md#g5.workbench.chat-actions) does, so the person sends it.

- The dialog opens for anyone who can read the workspace; only its primary action carries the composer's own reason when the prompt cannot be seeded.
- The prompt is a hidden ribbon action the dialog seeds by id, and the Guided mock step counts documents under `mocks/` as its evidence.

<a id="g5.documents.question-index"></a>
[D · g5.documents.question-index]

The editor's Outline lists every `[Q<N>.]` record in document order beside the headings, with the record's title and its question state (open, proposed or decided), and scrolls to the record on press, so a decisions document with no headings is still navigable. Intent and decision records stay out of the Outline; the Core Ontology editor's own presentation is unchanged.

## Metadata

### g5.documents.explainer-catalog

- agent
- Expands: [Documents](../documents.g5.md#g5.documents).
- Source: [workbench design](../../../pr_ontology_diff_design/ux-v2-workbench/ontology-plan.md) and [quantitative refinement](https://github.com/G5Labs/G5-Lean/blob/345b546e9fed5477e6fdf3c77ad87002ed558dd6/.g5/TLA_verification/ben-quantitative/platform/README.md).
- ExplainerTemplates checked built-ins plus valid committed templates and live-only saving, with a failed-flush/retry witness. Ordinary publication, parsing and captured-view integrity are premises; route comments describing an immediate commit do not match the inspected live write.

### g5.documents.explainer

- agent
- Expands: [Documents](../documents.g5.md#g5.documents).
- Defines the `g5.documents.explainer` level for [ID levels](../ontology_and_harness.g5.md#g5.ontology.writing.id-levels).
- Children: [Templates](#g5.documents.explainer.templates).
- Source: [ontology writing principles](https://github.com/G5Labs/G5-Lean/issues/1119) clean-up, 2026-11-06.

### g5.documents.explainer.templates

- agent
- Expands: [Explainer](#g5.documents.explainer).
- Source: [former owner](https://github.com/G5Labs/G5-Lean/blob/3d17915df39bcf4cbe0cabc08500de63ff46fbb2/.g5/ontology/features/documents.g5.md) and [quantitative refinement](https://github.com/G5Labs/G5-Lean/blob/345b546e9fed5477e6fdf3c77ad87002ed558dd6/.g5/TLA_verification/ben-quantitative/platform/README.md).
- ExplainerTemplates checked explicit replacement consent and reserved built-in identities, and exposed an intervening native write lost between hash check and rename. Consent names the template, not a locked file version; the shared native-writer gap remains.

### g5.workbench.meeting

- agent
- Source: [human request and decisions](https://github.com/G5Labs/G5-Lean/blob/3d17915df39bcf4cbe0cabc08500de63ff46fbb2/.g5/chat_intent/issue-478-create-issue/request.md).
- One press per option: the owner's instruction of 2026-10-15 in the [tracker-writes chat intent](../../../chat_intent/issue-tracker-writes/request.md); details in its [design](../../../pr_ontology_diff_design/issue-tracker-writes/ontology-plan.md).
- Task options, no votes and the arrival cue: the owner's instructions of 2026-10-22 in the [meeting-task-buttons chat intent](../../../chat_intent/meeting-task-buttons/request.md), from the 2026-09-30 walkthrough where every non-rejected decision becomes a task; design and mocks in its [plan](../../../pr_ontology_diff_design/meeting-task-buttons/ontology-plan.md). The earlier four-option set stays recognised so existing decision documents keep working.
- Ribbon entry: the owner chose variant 1C of the [entry mocks](../../../pr_ontology_diff_design/meeting-task-buttons/mocks/review-meeting-entry-options.html) on 2026-10-22 ("let's do option 1c, then question 2 is moot"). Implemented in `CreateProjectDocModal.tsx` (transcript sources) and `WorkspacePage.tsx`; `extract-decisions` is a `hidden: true` action of the Analysis group in `workbench_builtins/actions.yaml` (the server resolves the template by id and config revision, so it cannot live only on the client); `apply-decisions` was removed on the owner's instruction of 2026-10-22.
- Implemented in `ReferencedQuestionCard.tsx` (`lib/meetingDecisions.ts`; direct creation over `useCreateIssue`, `CreateIssueDialog` for untrusted destinations, `useLinkWorkspaceIssue` for attach); tests in `ReferencedQuestionCard.test.tsx`, `meetingDecisions.test.ts` and `e2e/flows/alignment-shell.spec.ts`. The browser flow for creation is not exercised end to end (no GitHub in the e2e stack).

### g5.workbench.mock

- human-provided
- Source: the owner's instruction of 2026-10-30 ("change the Build mock button to also put the mocks in the project doc folder, so there isn't yet another special path … remove the preview mock button, instead when you press build mock open a little panel with the recent mocks, similar to review meeting"). Supersedes the Preview mock entry and the `.g5/preview_profiles/mock.yaml` step of the [workbench design](../../../pr_ontology_diff_design/ux-v2-workbench/ontology-plan.md).
- Implemented in `BuildMockModal.tsx` and `WorkspacePage.tsx`; `build-mock` is an `open` action with target `mock` and `generate-mock` a `hidden: true` chat action of the Mock group in `workbench_builtins/actions.yaml`. Tests: `BuildMockModal.test.tsx`, `WorkspacePageAlignment.test.tsx`, `workbench_config.rs`.

### g5.documents.question-index

- human-provided
- Expands: [Documents](../documents.g5.md#g5.documents).
- The owner's request of 2026-10-22 ("add an index on the side so we can quickly navigate across questions") in the [meeting-task-buttons chat intent](../../../chat_intent/meeting-task-buttons/request.md); read as the existing Outline rail, which listed headings only.
- Implemented in `DocumentPreview.tsx` and `DocumentOutline.tsx`, scoped to that preview’s root; tests in `DocumentOutline.test.tsx`.
