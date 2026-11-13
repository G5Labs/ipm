# Workspace button gates implementation review

Agent review comments, attribution and round tools described in this historical record
were retired by the [October 24 removal](chat_intent/workspace-colab-harden/remove-agent-review-comments.md). They are not supported interfaces.

Self-review of the `worktree-workspace-button-gates` candidate against the saved [design](ontology-plan-workspace-button-gates.md) and [chat intent](chat_intent/workspace-button-gates/request.md). The same agent implemented the change, so this is not an independent review.

## Initial findings and evidence

| Commitment | Actual path → counterexample/check | Result |
|---|---|---|
| `g5.workspace.ui.gates` | `WorkspacePage.tsx` computes `workspaceWritable` (owner ∧ ¬Base ∧ ¬readOnly ∧ ¬freezing) and `liveEditable` (∧ running); `lib/workspaceLiveGate.tsx::liveGateKind` orders refusals Base → reader → policy → VM. Counterexample: a stopped owner's Create PR, Prepare to ship, rename/new session, review-gap actions and queued-input controls must stay; the composer, saves, terminal, preview, agent answers and Accept must not. | `WorkspacePageBasePull.test.tsx` (stopped owner keeps Create PR enabled; no VM start), `WorkspacePageV2.test.tsx` (stopped owner keeps New session and queued-input controls; composer off with Start), gate unit tests. Passing. |
| `g5.workspace.ui.unavailable` | `StartWorkspaceHint` renders the reason and dispatches `WORKSPACE_START_EVENT`, which the page handles only for the owner; `canStart` is true only for stopped/failed. Counterexample: a reader or a starting VM must get no Start; an approval's buttons must be disabled, not an error toast on click. | Gate tests (hint hidden when live, no Start while starting); terminal panel test (hint + Start fires the event); status band test (both Requirements buttons disabled with the reason and a Start); `ChatPanel.tsx` approval/question buttons disabled with `title` and a hint. Passing. |
| `g5.workspace.ui.vm-state` | `liveGateBanner` names stopped, failed, starting, queued (with the capacity reason), stopping and unknown; the strip keeps Start on stopped/failed and Stop on starting/queued. Counterexample: an owner whose VM read is still pending must not read "stopped". | Gate unit test per state; page test asserts the "not known yet" composer reason while the VM read is pending. Passing. |
| `g5.workspace.base-ui.message` | Base path returns the exact M25 text before the state banner. Read-only policy and readers keep the same text (decision recorded in the gate module). | `WorkspacePageV2.test.tsx` Base and read-only banner assertions pass unchanged. |
| `g5.workspace.ui.reader-publication` | ReposBar merge/PR, Prepare to ship and the PR-destination retry take owner gates; `BranchActionsMenu`, `MakeRemoteMenu`, `RemoteTrackingActions`, `BranchAutoPullControl` and `PullRequestList` return nothing for a reader through the gate; Base keeps its pull-only controls for its owner only (defect 1 below). | `PullRequestList.test.tsx` reader case passes; the others are inspected (no gate → prior behavior, so existing tests still cover the owner path). A reader-case test for the Details chips remains unwritten. |
| `g5.workspace.ui.review-copy-row` | `buildWorkspaceHierarchy` keeps the current workspace when it is a review copy; other copies stay out. | `WorkspaceSwitcher.test.tsx` new case passes; the prior "keeps review copies out" case still passes. |
| `g5.governance.review-workspace.exclusions` | `isReviewCopy` joins the list's `isReview` with the review context; Fork current, Rebase & Merge, Create PR, Prepare to ship, Transfer, Approvals tab and PR-destination retry are omitted; branch chips return nothing on a copy; Details actions disabled with `REVIEW_COPY_DETAILS_REASON`. Counterexample: the single-workspace DTO hardcodes `is_review: false`, so the detail alone cannot identify a copy. | `WorkspaceDetailsPanel.test.tsx` copy case passes. The page-level omission is inspected: every excluded control reads `isReviewCopy`. No page test renders a copy with the list flag yet (gap). |
| `g5.governance.review-workspace.finish` | `handleFinishReview` stops through `useStopWorkspaceVm` when running/starting/queued, toasts, then opens the existing remove dialog; the banner offers it only when the copy is removable. | Banner test covers the button and its absence; the page handler is inspected, not executed (gap). |
| `g5.conversation.composer.stopped` | `composerDisabledReason` comes from the gate; `ComposerDisabledNotice` adds Start when `canStart`; starter prompts, Explain in chat, Discuss, Review with my agent and Review Workspace disable with the reason; Chat about this stays enabled per the pick. | Page test (stopped owner: Start beside the reason; read-only: Review Workspace disabled, no seed). Passing. |
| `g5.lifecycle.start` | Every Start reaches `POST /resume` through the header's handler; no gate change starts compute. | `WorkspacePageBasePull.test.tsx` asserts zero VM starts through PR retry. |
| `g5.design.minimal-mechanism` | One context and one pure reason function replace per-prop threading; `WorkspacePageProviders` keeps the page's JSX depth so the diff stays reviewable. No new server state. | Inspected. |

### Defects found and repaired before commit

1. `RemoteTrackingActions` and `BranchAutoPullControl` offered Base's pull controls to every non-reader kind, but Base's kind is `base` for readers too; the gate now carries `owner` and Base offers them to its owner only.
2. The PR-destination retry still keyed off the review context alone; it now uses `isReviewCopy` and `workspaceWritable` like the controls it serves.
3. The strip's terminal title printed the raw loading status; it now says only why.
4. Adding a provider level let the formatter re-indent the whole page; a same-depth `WorkspacePageProviders` wrapper reverted that.
5. `handleFinishReview` referenced the remove handler before its declaration; the handler moved above it.

### Expectation changes

Four page and strip tests asserted the old wording or hidden controls; each was compared with the governing block before changing: the terminal control now stays disabled (`g5.workspace.ui.unavailable`), the composer says "Start the workspace…" or "not known yet" (`g5.workspace.ui.vm-state`), New session stays while stopped (`g5.workspace.ui.gates`), and a read-only workspace's Review Workspace button is disabled rather than seeding (`g5.conversation.composer.stopped`). The stopped-owner PR fixture no longer marks the workspace read-only, since the VM endpoint models the stop.

## Rechecks

- `tsc --project tsconfig.json --noEmit`: clean.
- `eslint src/`: 0 errors (warnings pre-existing).
- `vitest run` (full): 264 files, 4300 tests passing after the test updates; the final run after defects 1–3 is recorded in the design's review summary.
- Prettier: every changed file formatted; three pre-existing unformatted files untouched.

## Independent review round 1 (Codex CLI 0.155.1, `gpt-6-astra`, effort `xhigh`, read-only sandbox)

Reviewed candidate `11597913`. Verdict: changes required, with two P1 and six P2 findings. Each was traced in the code before repair; all eight were valid. CodeRabbit's seven inline comments on PR #333 arrived in the same window; two duplicated Codex findings and the other five were valid too. Dispositions:

| Finding | Governing rule | Repair |
|---|---|---|
| P1 Base's owner lost PR refresh/close/merge (`canWrite` is false on Base) | `g5.workspace.ui.reader-publication` | `PullRequestList` gates on `owner`, which the server checks; Base-owner test added. |
| P1 Finish could report "stopped" on an unknown status and ignored an accepted-but-still-releasing Stop | `g5.governance.review-workspace.finish`, `g5.lifecycle.stop` | `finishReviewStep` and `finishReviewStopped` (unit-tested) decide: unknown and stopping wait with a toast; a `stopping` answer is reported as in progress and removal is offered only once stopped. |
| P2 Mobile Finish had no Remove dialog | `.finish` | The dialog renders in the mobile layout too. |
| P2 (and CodeRabbit) the listed copy row offered Fork | `g5.governance.review-workspace.exclusions` | Row Fork excludes `isReview`; the switcher test now supplies both row callbacks and asserts Remove present, Fork absent; metadata corrected. |
| P2 (and CodeRabbit) sidebar context menus and file drops bypassed the gate | `g5.workspace.ui.unavailable` | Section menu items disable with the reason; drop targets are withheld while blocked. |
| P2 (and CodeRabbit) Reviews Start and the terminal title during starting/queued/stopping/unknown | `g5.workspace.ui.vm-state` | Both use `liveGateReason`; Start renders only when `liveGateCanStart`; strip test asserts each state's reason; Reviews test covers `starting`. |
| P2 CodeEditor Save and the Changes view's Update/Push/Commit vanished when stopped | `.unavailable` | Save stays, disabled with the reason, while the owner waits on the VM; the Changes view keeps the three controls disabled with the reason and a Start hint. |
| P2 Review identity defaulted to ordinary while both reads were pending | `.exclusions` | `reviewIdentityKnown` (list or review context answered) withholds every excluded control until known; the gate carries it for the branch chips. |
| CodeRabbit: `useAnswerBlockedReason` ignored a missing resolver when a gate is present | `g5.conversation.composer.stopped` | Falls through to the resolver check. |
| CodeRabbit: `MakeRemoteMenu` unmounted an open dialog when the gate flipped | `.reader-publication` | The gate only hides the trigger, like `available`. |
| CodeRabbit: the copy's row sat under a collapsed source | `g5.workspace.ui.review-copy-row` | The copy's ancestors open once when the switcher learns it; manual collapse is kept. |

Codex could not run Vitest in its sandbox (temporary-directory writes denied); its validation was TypeScript and ESLint plus source tracing. The repairs' checks are recorded below.

## Independent review round 2 (same reviewer, candidate `46b7dd4b`)

Verdict: changes required, with five P2 and one P3; every round-1 repair was confirmed. CodeRabbit added four comments on the same commit, three overlapping. All were valid on inspection. Dispositions:

| Finding | Governing rule | Repair |
|---|---|---|
| P2 Publish branch and Make remote ignored `reviewIdentityKnown`; the switcher's detail-built fallback row lacked `isReview` | `g5.governance.review-workspace.exclusions` | Both chips wait for a known identity; the page marks its fallback summary as a copy when the review context knows it, and the switcher also hides Fork on the active row when the page says it is a copy. |
| P2 (and CodeRabbit) Changes controls followed a possibly lagging attachment read instead of the gate, and a reader of an attached workspace was offered them | `g5.workspace.ui.gates`, `.unavailable` | With a gate, durable authority shows Update, Push and Commit and live attachment enables them; the reason reaches the Commit button; a reader sees none. |
| P2 Mobile had no lifecycle action after Start | `g5.workspace.ui.vm-state` | The state banner renders in both layouts and carries Start or Stop per state. |
| P2 Rebase & Merge was offered to the owner of a read-only or freezing source; the Review Workspace button was disabled for permanent refusals instead of omitted | `g5.workspace.ui.gates`, `.unavailable` | Merge takes the durable gate (Base's owner keeps the explained disabled control); the button is omitted for a reader or read-only workspace and the page test asserts omission. |
| P2 Issue Link and Unlink vanished on a copy | `.exclusions` and the Details pick | Both stay visible, disabled with the reason; `WorkspaceIssuesPanel.test.tsx` covers them. |
| P3 (and CodeRabbit) the sidebar's section memo depended on the ungated drop callback | `.unavailable` | The memo depends on the gated one. |
| CodeRabbit: `?view=approvals` and the stale-tab cleanup read only the review context | `.exclusions` | Both use the combined identity; the hint waits until it is known and runs again then. |
| CodeRabbit: the Base-owner exception was missing from the ontology | `g5.workspace.ui.reader-publication` | The block now states that PR-target actions and Base's fetch controls check ownership alone, while source-side publication needs a writable source. |

## Independent review round 3 (same reviewer, candidate `8c172c69`)

Verdict: changes required, with six P2; every round-2 repair was confirmed. All six were valid on inspection. Dispositions:

| Finding | Governing rule | Repair |
|---|---|---|
| P2 The `?view=approvals` hint was marked consumed before the identity check, so a delayed identity never opened the tab | `.exclusions` | The readiness check runs before consumption and the reverse effect keeps the pending hint. |
| P2 Mobile lost every lifecycle control once the VM ran | `g5.workspace.ui.vm-state`, `g5.lifecycle.stop` | The mobile layout renders the owner's VM strip (without Terminal) under the banners. |
| P2 A cached list without the current row counted as known identity; the active fallback row offered Fork while unknown | `.exclusions` | `reviewIdentityKnownFrom` (unit-tested) requires the review context or a list that names the workspace; the switcher withholds Fork on the active row until known. |
| P2 Details acted on an unknown identity and the open policy dialog could submit after it resolved | `.exclusions` | Details treats unknown identity as waiting, with its own reason; the transition rechecks at submit. |
| P2 The Changes view trusted its own review-context read, so a failed read re-offered Commit on a copy | `.exclusions` | With a page gate the combined identity decides; test added. |
| P2 The review banner's buttons used the review context's VM status while their hint used the page gate | `g5.workspace.ui.vm-state` | Banner and My review discard follow the page gate when present; test covers the disagreement. |

## Independent review round 4 (same reviewer, candidate `b5805147`)

Verdict: changes required, with three P2 and nothing higher; every round-3 repair was confirmed. All three were valid. Dispositions:

| Finding | Governing rule | Repair |
|---|---|---|
| P2 A restored center tab could mirror its own `?view` over a still-pending Approvals hint | navigation (round-3 follow-up) | The URL-mirroring effect never overwrites a hint the URL→tab effect has not consumed, in either branch. |
| P2 The Discard confirmations and the Commit dialog's submit checked only mutation progress | `g5.workspace.ui.gates`, `.unavailable` | Confirmation buttons disable with the current reason and their handlers refuse; the commit dialog shows the reason and keeps the typed message; tests cover a stop while the dialog is open. |
| P2 Switch back ignored the gate | `.gates`, `.unavailable` | Switch back follows the same visibility and execution gates, with its reason; test covers a cached mismatch report under a stopped gate. |
| CodeRabbit: the mobile strip offers Start to the owner of a read-only or freezing workspace | `g5.workspace.ui.vm-state` | Kept, deliberately: the desktop strip does the same, the server admits Start regardless of policy, and a running read-only workspace keeps its preview and terminal. The gate's read-only kind refuses editing and its Start hint, not the lifecycle control. |

## Independent review round 5 (same reviewer, candidate `4673b734`)

Verdict: changes required, with three P2 and nothing higher; every round-4 repair was confirmed. All three were valid. Dispositions:

| Finding | Governing rule | Repair |
|---|---|---|
| P2 One render after the Approvals hint is consumed, the previously selected tab could still mirror itself over it | navigation | The URL→tab effect records the tab the hint asked for; mirroring waits until that tab is selected (or the URL changes again). |
| P2 Preview was refused to a running read-only or freezing owner, which the server allows | `g5.workspace.ui.gates`, `g5.preview.service` | Policy kinds no longer block Preview; the gates block now states the exception; test covers the running read-only owner. |
| P2 The Reviews panel's Commit never received the gate's reason, so a stale attached report kept it enabled after a stop | `.gates`, `.unavailable` | The panel passes its VM-state reason to the Commit button; the starting-VM test asserts it. |

## Independent review round 6 (same reviewer, candidate `b56635b2`)

Verdict: changes required, with two P2 and nothing higher; every earlier repair was confirmed. Both were valid. Dispositions:

| Finding | Governing rule | Repair |
|---|---|---|
| P2 Fetch, Push, auto-pull and auto-push disappeared for the owner of a read-only or freezing workspace, which the server allows | `g5.workspace.ui.gates`, `.reader-publication` | Synchronizing an existing binding follows ownership alone; publishing a new branch keeps the writable-source gate; both ontology blocks say so. |
| P2 Approve rebased publication and Retry sync still showed for readers | `.reader-publication` | Both follow the owner gate; the popover tests cover reader and owner. |

## Independent review round 7 (same reviewer, candidate `283fe5ab`)

Verdict: changes required, with two P2 and nothing higher; every earlier repair was confirmed. Both were valid, and both are the same shape: a dialog kept open through a gate change could still submit. Dispositions:

| Finding | Governing rule | Repair |
|---|---|---|
| P2 An open Make remote dialog ignored the publication gate | `g5.workspace.ui.reader-publication` | The dialog body reads the gate: its submit disables and refuses, with a note, while the draft stays; test flips ownership with the dialog open. |
| P2 An open Mark resolved confirmation ignored ownership | `.reader-publication` | The confirmation closes and its handler refuses once ownership moves; test covers the transfer with the confirmation open. |

## Independent review round 8 (same reviewer, candidate `0e0309f3`)

Verdict: changes required, with two P2 and nothing higher; the round-7 repairs and every earlier one were confirmed. Both were valid. Dispositions:

| Finding | Governing rule | Repair |
|---|---|---|
| P2 Open VM-bound dialogs (Create explainer, Create project doc, Preview's Start preview) still submitted after the workspace stopped; the explainer reached the durable enqueue path, which queues while stopped | `g5.workspace.ui.unavailable`, `g5.conversation.composer.stopped` | Each dialog reads the page gate: its submit (and the project doc Retry failed) disables with the state's reason, shows the Start hint, and its handler refuses; drafts, the upload queue and the selected application stay. Tests open each dialog live, stop the workspace, and assert no request. |
| P2 Base's Accept provider history and its confirmation were offered to readers | `g5.workspace.ui.gates`, `g5.workspace.ui.reader-publication` | The acceptance section takes the popover's ownership gate: readers keep the paused status with an owner-only note and lose the controls; an open reviewed reset closes and its handlers refuse once ownership moves. Tests cover a reader on Base and ownership loss after the preview. |

CodeRabbit, same period: the live-attachment bullet said "durable authority and VM running" while exempting preview and the terminal from policy. The bullet now defines live attachment by the running VM, names which operations also need durable authority, and records the submission-time recheck.

## Independent review round 9 (same reviewer, candidate `f1d350db`)

Verdict: changes required, with one P2 and one P3; the round-8 repairs and every earlier one were confirmed. Both were valid. Dispositions:

| Finding | Governing rule | Repair |
|---|---|---|
| P2 Two dialogs still submitted after a stop: the Delete project doc confirmation (its delete needs the owner handle) and Generate (which reached the page's generic read-only error with no reason or Start in the dialog) | `g5.workspace.ui.gates`, `g5.workspace.ui.unavailable` | The confirmation is its own component reading the gate: Delete disables with the reason and Start hint and refuses, the confirmation staying open; the page handler also refuses without live attachment. Generate's Start takes the gate reason and hint, keeping the selections. Tests open each while live, stop, and assert nothing is called. |
| P3 The terminal's recovery hint for a read-only or freezing owner named the policy, not the stopped VM, and offered no Start, though the terminal needs no writable policy | `g5.workspace.ui.gates`, `g5.workspace.ui.vm-state` | The gate now carries `vmKind`, the kind from ownership and VM state alone; the terminal and Preview hints read it, so a read-only owner whose VM stopped sees the VM reason and Start, and intermediate states name themselves without Start. Preview's earlier policy-only exception is replaced by the same kind. Tests cover stopped and starting under a read-only policy. |

## Independent review round 10 (same reviewer, candidate `f662c04b`)

Verdict: changes required, with one P2 and one P3; the round-9 repairs and every earlier one were confirmed. Dispositions:

| Finding | Governing rule | Disposition |
|---|---|---|
| P2 The review-gap actions (Answer, Fix, Implement), now offered on durable authority, call a server write that refuses every changed selection: `compare_and_set_project_doc_intent_ledger_draft` returns a conflict unconditionally | `g5.workspace.ui.gates` | Confirmed, and pre-existing: the stub arrived with the Lean MVP cutover (`09f11e34`) and refuses with the VM running too, so these actions never persisted on `main` either. The UI gate matches the route's intended contract (a durable docs-branch draft write), so it stays; the durable ledger write is separate backend work, recorded below and in the plan's acceptance outcomes. Not repaired here. |
| P3 Preview's open-dialog hint omitted the policy-free kind, so a read-only owner whose VM stopped was told to change the policy | `g5.workspace.ui.gates`, `g5.workspace.ui.vm-state` | The dialog hint reads the VM kind like the trigger; test covers a read-only owner with the dialog open. |

## Independent review round 11 (same reviewer, candidate `831fe53d`)

Verdict: changes required, with one P2 and one P3; the round-10 repair was confirmed and the review-action stub accepted as a documented gap. Dispositions:

| Finding | Governing rule | Repair |
|---|---|---|
| P2 Open publication dialogs (Create PR, Rebase & Merge confirm, Prepare to ship) kept submitting after publication authority closed (ownership moved, policy became read-only, identity resolved to a copy) | `g5.workspace.ui.gates`, `g5.workspace.ui.reader-publication` | One `publicationBlockedReason` helper names why publication is not the viewer's; each dialog disables its submit with that reason and refuses in its handler, keeping the target, description and selections; the page's Create PR and merge handlers refuse too. Tests cover ownership loss and a read-only policy with each dialog open, and a stopped owner who keeps them. |
| P3 The VM-state block said the owner never reads the reader's "Fork workspace" text, while the banner keeps it for a read-only or freezing policy by design | `g5.workspace.ui.vm-state` | The sentence now scopes the rule to VM states and names the policy exception. |

The user asked, after this round, to merge once nothing blocks; the loop stops here with the repairs above applied and tested.

## Follow-up after merge (PR #340)

PR #333 merged with the e2e check pending, and that check then failed: two stopped-workspace specs asserted the terminal control absent and the old composer wording. The specs now assert the kept, disabled control and the new wording. In the same PR the user asked for two changes after seeing the stopped status band:

| Request | Governing rule | Change |
|---|---|---|
| "Start the workspace to …" plus Start beside every greyed-out button is too much; show it only when the user tries the button | `g5.workspace.ui.unavailable` (amended) | `GatedControl` wraps each gated control. Nothing shows until the control is tried; a click, Enter or Space reveals the reason and Start in a popover anchored to that control; Escape or a click elsewhere closes it; the reason stays on hover through `title`. The wrapper keeps the same DOM node across gate changes. The terminal panel keeps its inline empty state; the banner keeps its Start. Every hint test now tries the control first. |
| Base's Plan card reads "<not implemented yet>" | `g5.workspace.ui.gates` | Generate is omitted on Base and Base has no chat for View changes, so the card fell through to a developer placeholder. The placeholder is gone; a Plan card with no action the viewer can take states the gate's reason. Tests cover Base and the no-gate case. |

## Remaining gaps

| Governing ontology | Gap | Proposed action | Uncertainty |
|---|---|---|---|
| `g5.workspace.ui.gates` | Review-gap Answer, Fix and Implement reach a server stub that refuses every changed selection, VM or not. | Implement the durable docs-branch ledger write behind `PUT /grounding/review-actions` with an API test asserting a persisted selection without a VM. | None about the gap; its fix is backend scope outside this change. |
| `g5.governance.review-workspace.exclusions` | No page-level test renders a review copy from the list flag and asserts the omitted controls together. | Add a `WorkspacePageReviewTab` case with `isReview: true` in the list fixture. | Low; each gate is a one-line condition inspected here. |
| `g5.governance.review-workspace.finish` | Stop-then-remove is not executed in a test or live. | Add a page test with a running copy: Finish stops, toasts and opens the remove dialog. | Low; both operations are existing hooks. |
| `g5.workspace.ui.unavailable` | No live QA walk of every surface. | Walk a stopped workspace and a stopped copy on QA after merge. | Visual placement only. |
