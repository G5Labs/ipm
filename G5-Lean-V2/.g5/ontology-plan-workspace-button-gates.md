# Workspace button gates: what each control does when the VM is running, stopped, or in a review copy

## Ontology changes and governance

**Comparison base:** `origin/main` at `5af3b761`. This design adds `g5.workspace.ui.gates`, `g5.workspace.ui.unavailable`, `g5.workspace.ui.vm-state`, `g5.workspace.ui.reader-publication` and `g5.workspace.ui.review-copy-row` in [workspaces](ontology/features/workspaces.g5.md); adds `g5.governance.review-workspace.exclusions` and amends `g5.governance.review-workspace.finish` in [governance](ontology/features/governance.g5.md); adds `g5.conversation.composer.stopped` in [conversations](ontology/features/conversations.g5.md). Lineage: the [chat intent record](chat_intent/workspace-button-gates/request.md), which carries the user's decision sheet. The actual diff of the contract bodies (metadata entries follow each file's footer):

```diff
--- a/.g5/ontology/features/conversations.g5.md
+++ b/.g5/ontology/features/conversations.g5.md
@@ -455,6 +455,12 @@
+### Compose only with a running workspace
+
+[I · g5.conversation.composer.stopped]
+
+While the owner's VM is not running, the composer is disabled with the state's reason and a Start action, and keeps the draft; nothing is queued from a stopped workspace, although [queued intent](#queued-intent-is-separate-from-delivered-history) would accept it. Controls that only prefill the composer (explain, review and discuss prompts, starter prompts) are disabled with the same reason. Stop, End wait and queued-input edits keep working, as do readers' views. The presentation follows [waiting actions](workspaces.g5.md#keep-a-waiting-action-visible-with-its-reason-and-start).
--- a/.g5/ontology/features/governance.g5.md
+++ b/.g5/ontology/features/governance.g5.md
@@ -123,11 +123,17 @@
+### A review copy publishes only through Push review
+
+[D · g5.governance.review-workspace.exclusions]
+
+A review copy is an ordinary owner workspace except that its work reaches anyone only as [suggestions](#a-review-returns-suggestions-to-the-source-owner). The server refuses on a copy: Fork, Rebase & Merge, Create pull request, Transfer, Publish branch, Make remote, Start review of the copy, and Changes-panel Commit (a copy is saved by Push review). The workspace page omits those controls and the Approvals tab on a copy, including Prepare to ship, and offers Make read-only, Edit description and issue links disabled with the reason that a copy follows its source: a read-only copy would refuse Push review, and a copy's description and issues are its source's. Everything else, including Start, Stop, chat, terminal, preview, comments and Push review, follows the copy's own [gates](workspaces.g5.md#offer-an-action-by-what-its-operation-needs).
+
 ### Finish review without discarding child work
 [D · g5.governance.review-workspace.finish]
-Explicit Finish stops the review child while dirty or unpushed work remains there, without introducing a sealed-result workflow.
+Explicit Finish stops the review copy's VM while dirty or unpushed work remains there, then offers to remove the copy; it introduces no sealed-result workflow and reuses the ordinary [stop](../software_architecture/lifecycle.g5.md#stop-fences-future-work-before-releasing-custody) and [removal](workspaces.g5.md#remove-a-workspace) of a workspace. Rounds already pushed stay on the source whether or not the copy is removed.
--- a/.g5/ontology/features/workspaces.g5.md
+++ b/.g5/ontology/features/workspaces.g5.md
@@ -494,6 +494,42 @@
+### Offer an action by what its operation needs
+
+[D · g5.workspace.ui.gates]
+
+The workspace page gates each control by the authority its server operation checks, through three gates and nothing else:
+
+- **Durable authority**: the viewer is the current owner and the workspace is not Base, read-only or freezing. It admits platform-record and Git actions: comments, review decisions other than applying a text suggestion, session management, queued input, task and job controls, remote fetch and push, pull-request creation and merging, lifecycle, sharing and policy.
+- **Live attachment**: durable authority and the owner's VM is running. It admits only what the [owner handle](../concepts/security.g5.md#one-eligible-owner-uses-live-compute) requires: saving a file, doc or ontology node, Changes-panel commit, push and update, terminal, preview, answering an agent's permission or question, applying a text suggestion, discarding from a review copy and resolving a divergence with an agent.
+- **VM state**: the reported lifecycle status, which decides messages and the one valid lifecycle button.
+
+A single read-only flag that also stands for "VM not running" hid durable actions while stopped and said "read-only" when the cause was the VM; the three gates keep the page truthful about the reason. Readers get read affordances, [replies](documents.g5.md#let-readers-reply-in-threads) and [file suggestions](governance.g5.md#suggest-file-changes-from-files) only.
+
+### Keep a waiting action visible with its reason and Start
+
+[I · g5.workspace.ui.unavailable]
+
+An action the owner could take once the VM runs stays visible, disabled, with the reason, and a Start action within reach: inside the control's own empty state where it has one (the chat composer, the terminal, FILES Edit, the Reviews panel, the status band) and otherwise the workspace banner directly above the content. Every such Start is the header's explicit [start](../software_architecture/lifecycle.g5.md#explicit-consent-owns-each-start); nothing starts compute as a side effect. An action the viewer can never take in this workspace, because they are a reader, the workspace is Base or read-only, or the workspace is a [review copy](governance.g5.md#a-review-copy-publishes-only-through-push-review), is omitted.
+
+### Name each VM state
+
+[I · g5.workspace.ui.vm-state]
+
+The banner and the VM strip name the reported state in its own words and offer only the valid lifecycle action: stopped and failed offer Start; starting and waiting for capacity say so, with the wait reason, and offer Stop; stopping offers nothing. Disabled controls take their reason from the same state. The owner never reads the reader's "Fork workspace to make edits" text, and a failed status read keeps the last known state under [page refresh](#keep-the-workspace-page-through-a-failed-refresh). [Base](#explain-base-as-read-only) keeps its own message.
+
+### Publication controls are the owner's
+
+[I · g5.workspace.ui.reader-publication]
+
+Create pull request, Rebase & Merge, Prepare to ship, Fetch and Push to remote, auto-pull and auto-push, pull-request refresh, close and merge, Publish branch and Make remote are offered only to the current owner. The server refuses them to anyone else, so a reader does not see them.
+
+### Show the current review copy in the switcher
+
+[I · g5.workspace.ui.review-copy-row]
+
+The workspace switcher lists other review copies nowhere, but the copy currently open appears as its own row under its source, highlighted, with the same Fork and Remove affordances the row's rules allow. A reviewer can otherwise remove their copy only from Home.
```

### Relevant and governing ontology

| Block | Why relevant | Implementation must |
|---|---|---|
| [g5.lifecycle.start](ontology/software_architecture/lifecycle.g5.md#explicit-consent-owns-each-start) | Every Start the page offers is explicit consent | Route every inline Start to the header's resume mutation; never start compute from a save, send or seed |
| [g5.security.owner](ontology/concepts/system_invariants.g5.md#one-eligible-owner-uses-live-compute) | Live gate boundary | Use live attachment only for the owner-handle operations named in `g5.workspace.ui.gates` |
| [g5.security.read.durable](ontology/concepts/security.g5.md#serve-nonowners-from-durable-state) | Readers' affordances | Readers keep reads, replies and file suggestions; publication controls go to the owner only |
| [g5.documents.files-view.owner-edit](ontology/features/documents.g5.md#start-compute-only-for-an-owners-edit) | FILES Edit already follows the pattern | Keep "Start workspace to edit"; make its label follow the VM state |
| [g5.documents.comments.authority](ontology/features/documents.g5.md#the-current-owner-writes-comments) and [.replies](ontology/features/documents.g5.md#let-readers-reply-in-threads) | Durable actions without a VM | Comments stay on the durable gate |
| [g5.governance.review-suggestions](ontology/features/governance.g5.md#a-review-returns-suggestions-to-the-source-owner) | Push review from a stopped copy publishes what it saved | Push review stays enabled while the copy is stopped |
| [g5.governance.accept.checkpoint](ontology/features/governance.g5.md#publish-accepted-review-files) | Applying a text suggestion is a save | Accept stays live-gated with reason and Start; comment and artifact kinds stay durable |
| [g5.workspace.base-ui.message](ontology/features/workspaces.g5.md#explain-base-as-read-only) | Base keeps its exact text | The per-state banner never replaces the Base message |
| [g5.workspace.page.refresh](ontology/features/workspaces.g5.md#keep-the-workspace-page-through-a-failed-refresh) | A failed status read is not a stop | Gate on the last known VM state; "unknown" only before the first read |
| [g5.governance.review-workspace](ontology/features/governance.g5.md#review-in-a-workspace-you-own) | A copy is an ordinary owner workspace | Everything not in the exclusion list follows the copy's own gates |
| [g5.story.remove-a-workspace](ontology/features/workspaces.g5.md#remove-a-workspace), [g5.lifecycle.stop](ontology/software_architecture/lifecycle.g5.md#stop-fences-future-work-before-releasing-custody) | Finish review reuses them | Finish = existing stop mutation, then the existing remove dialog; no new route |

## Requested outcome

Every workspace control behaves by what its operation needs: durable actions work with the VM stopped; VM-bound actions stay visible, disabled, with the reason and a Start in reach; each VM state is named; review copies omit exactly what the server refuses; readers never see publication controls; the composer keeps its draft while stopped; Finish review stops the copy and offers to remove it; the open copy appears in the switcher.

## Design

```
WorkspacePage ──── computes ────► durable gate   = owner ∧ ¬Base ∧ ¬readOnly ∧ ¬freezing
      │                           live gate      = durable ∧ vm.status == running
      │                           vm kind        = stopped | failed | starting | queued | stopping | running | unknown
      │                           isReviewCopy   = reviewOrigin ≠ null ∨ list.isReview
      ├─ WorkspaceLiveGateProvider {kind, canWrite, live, canStart, start}
      │        └─ useWorkspaceLiveGate() in ChatPanel, editors, FileTree/TreeNode, OntologyTree,
      │           PreviewControl, WorkspaceTerminalPanel, StatusBand hints, ReviewChangesModal,
      │           RemoteConflictAgentLauncher, PullRequestList, ReviewWorkspaceBanner
      │              └─ <StartWorkspaceHint verb> = liveGateReason(kind, verb) + Start (→ header resume)
      ├─ banner: per-state text + Start/none; Base and reader texts unchanged
      ├─ ReposBar / Prepare to ship / sessions / review gaps / Reviews: durable gate, ¬isReviewCopy where excluded
      ├─ sidebar create/delete, uploads, explainer, generate: passed on durable gate; disabled-with-hint on live gate
      └─ Finish review: stop mutation → toast → remove dialog (copy)
```

One React context carries the page's gate to every leaf so no prop threads through the 11k-line page or its tab machinery; a pure `liveGateReason(kind, verb)` turns the state into one sentence per control. Components outside a provider (tests, other pages) read `null` and behave as today. Review identity on a copy joins the workspace list's `isReview` with the review context so excluded controls never flicker in. Nothing changes server-side: the gates mirror `owner_handle` and the `ensure_not_review_workspace` callers, and Finish review composes `POST /stop` with `POST /delete` through the existing hooks and dialog.

**Acceptance outcomes.** With the owner's VM stopped: Create PR, Rebase & Merge, Prepare to ship, rename and new session, review-gap actions, comments, decisions other than Accept, task and job controls and remote sync work; the composer, agent answers, Upload docs, file and doc creation, editor Save, Generate, explainer, preview, terminal and Resolve with agent are disabled with a Start in reach; the banner names the state. Starting, waiting, stopping and failed each read differently. A review copy shows no Fork current, Rebase & Merge, Create PR, Prepare to ship, Transfer, Publish branch, Make remote or Approvals tab, and its Details actions are disabled with a reason; its Update, Discard and Run agent review say "Start your review workspace…" while stopped; Push review works stopped; Finish review stops it and offers Remove; the switcher lists it under its source. Readers see no publication controls. Known server gap: the review-gap actions (Answer, Fix, Implement) are offered on durable authority as picked, but the server's review-action write (`PUT /grounding/review-actions`, `compare_and_set_project_doc_intent_ledger_draft`) is a stub from the Lean MVP cutover that refuses every changed selection whether or not the VM runs; the pick is exercised end to end only once that durable ledger write exists, which is separate backend work.

**Risks.** The page's `workspaceReadOnly` feeds editors' `editable`; it stays as the live gate for content editing so a stopped owner cannot type into a stale buffer. Components that read the gate through context need the provider in page tests; those outside keep today's behavior. The FILES "Starting…" label now follows the VM state and returns to "Start" after a failed resume by design.

**Open assumptions.** The Details actions pick on a copy named a Start action; implemented as disabled with a reason only (recorded in the chat intent). "Pure suggestion" in the Accept note is read as comment and artifact kinds, which already need no VM.

## Review summary

Self-reviewed after implementation, then reviewed independently by Codex CLI (`gpt-6-astra`, `xhigh`) and CodeRabbit ([review artifact](ontology-review-workspace-button-gates.md)). Round 1 added these decisions: PR list actions follow ownership alone, so Base's owner keeps them; Finish review waits while the state is unknown or the copy is stopping and offers removal only once a Stop has completed; the controls a copy excludes are withheld until the page knows whether it is a copy; the Changes view and the code editor keep their VM-bound controls disabled with the reason rather than hiding them; the open copy's ancestors open once in the switcher. Round 2 added: with a page gate, the Changes view's controls follow durable authority for visibility and live attachment for execution rather than a possibly lagging attachment read; Rebase & Merge needs a writable source; a control a reader or read-only workspace can never use is omitted even where it only prefills the composer; Link and Unlink on a copy stay visible but disabled; the state banner with its one lifecycle action renders on phones too; and PR-target actions and Base's fetch controls follow ownership alone, now stated in `g5.workspace.ui.reader-publication`. Round 3 added: the review identity counts as known only when the review context has answered or the list names the workspace, and every consumer of that identity (Details, the Changes view, the switcher's active row, the Approvals hint) waits rather than assumes an ordinary workspace; the review banner and My review follow the page's VM state when present; and the mobile layout keeps the owner's lifecycle strip through every state. Round 4 added: a `?view` hint the page has not consumed is never mirrored over by the active tab; confirmation buttons and their handlers (Discard, Commit, Switch back) apply the gate at the moment of submission, keeping typed drafts. Round 5 added: mirroring waits for the hint's own tab to be selected; Preview needs the running VM but no writable policy, as the server checks; the Reviews panel's Commit takes the page's VM reason. Round 6 added: synchronizing an existing remote binding (Fetch, Push, the auto toggles, Retry sync, Approve rebased publication, Mark resolved) follows ownership alone, so a read-only owner keeps it and a reader sees none of it; only publishing a new branch keeps the writable-source gate. Round 7 added: a dialog kept open through a gate change (Make remote, Mark resolved) applies the gate at submission and keeps its draft. Round 8 extended that recheck to every VM-bound dialog (Create explainer, Create project doc, Start preview), so a stopped workspace never queues an explainer chat, and gave Base's provider-history acceptance the same ownership gate as its other sync controls; the live-attachment gate is now defined by the running VM, with durable authority named per operation. Round 9 added: the Delete project doc confirmation and Generate recheck the gate at submission, and the gate carries a policy-free VM kind so the terminal and Preview offer a read-only owner the VM reason and Start rather than a policy change. Rounds 10 and 11 added: the review-gap actions' server write is a pre-existing stub, recorded as a backend gap; every publication dialog (Create PR, Rebase & Merge, Prepare to ship, Make remote) rechecks one shared publication predicate at submission. After the merge the user asked for two changes, shipped with the e2e repair: the reason and Start no longer sit beside every disabled control; one `GatedControl` wrapper reveals them only when the control is tried (click, Enter or Space), with the reason still on hover, while the terminal panel keeps its inline empty state and the banner keeps its Start; and the status band's Plan card, which fell through to a developer placeholder on Base once Generate was omitted there, now states the gate's reason. Consequential decisions made while coding: the gate carries `owner` so Base offers its pull controls to its owner only; a read-only policy and readers keep the M25 "Fork workspace" banner text, since the picks covered VM states only; the page wraps its providers in one same-depth component so the diff stays reviewable; Finish review is offered only when the copy is removable by the switcher's rule. Verified by typecheck, lint, and the full frontend unit suite (266 files, 4335 tests). Remaining uncertainty: no page test renders a review copy from the list flag and asserts every omitted control together; the Finish review handler and the per-surface placement were inspected, not executed live. Reuse verified during coding: `requestWorkspaceStart` reaches the page's listener only for the owner and the hint offers Start only when `canStart`; `useStopWorkspaceVm` is skipped for a copy that is already stopped; the list's `isReview` comes from the fork record while the single-workspace DTO hardcodes `false`.
