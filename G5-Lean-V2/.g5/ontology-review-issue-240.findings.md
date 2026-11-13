# Issue 240 — independent read-only discovery review

Independent, read-only discovery review by a fresh reviewer (one of the discovery reviewers, not the consolidator). No production, test or ontology files were edited. Inputs: [request](chat_intent/issue-240/request.md), [pinned design](ontology-plan-issue-240.md), `git diff` and the untracked files on `ben/issue-240-remote-branch-ui`, the review skill and the authoring references. G5 `5e8c886149..b203ed5a73` was used only for comparison.

**Recorded ontology diff:** byte-identical to `git diff -- .g5/ontology`, checked with `diff`. Five blocks were added and one index line was added. No existing commitment changed.

## Commitment ledger

| Commitment | Actual path | Counterexample / check | Result or gap |
|---|---|---|---|
| `g5.story.see-remote-tracking-state` | `WorkspaceDetailsPanel.tsx:619-629` → `BranchTrackingInline` / `RemoteStatusPill` | Each card shows its state in words and expands in place. Nothing is portaled. | Conforms, with gaps F3 and F4. |
| `g5.workspace.remote-ui.status` | `remoteTrackingStatus.ts:140-166`, `RemoteStatusPill.tsx`, local pill at `WorkspaceDetailsPanel.tsx:625-628` | Priority is review > blocked > behind > ahead > healthy. No counts. The label is always present, so color is never the only signal. Tokens are used. Checked in `remoteTrackingStatus.test.ts` and the Inline `it.each`. | Conforms. Gap F3: a tracked card with no feed row gets no pill. Theme note: F8. |
| `g5.workspace.remote-ui.detail` | `BranchTrackingInline.tsx:115-135`; popover `BODY_CLASS` | The detail test checks the portal (`container` contains the body), `role=dialog`, shadow/fixed classes, and that Escape and outside clicks do nothing. This test would fail on the old portal. | Conforms. Stacked multi-repo rendering is untested (F9). |
| `g5.workspace.remote-ui.unknown` | `BranchTrackingInline.tsx:61-96` | Sync-runs loading and error are tested. Error shows Retry and hides the expander. | Partial evidence. The merge-runs gate is untested (F2). The null case is F3. A stale refetch collapses the detail (F12). |
| `g5.workspace.remote-ui.actions` | Head is `BranchTrackingInline.tsx:109-130`; mutations sit in the popover body | Collapsed head has exactly one button (would fail if Push/Pull were added to the head). The review state makes no generic push call (`BranchTrackingPopover.test.tsx:850`). | Conforms. "Once" inside the expanded body is not asserted (F9). The unavailable-state Retry is a query refetch, which `.unknown` allows. |
| `g5.security.owner`, `g5.security.read.durable` | Body gating unchanged. No new query or mutation family. | Does the inline body expose a mutation the old popover hid from nonowners? | Preserved against baseline. The client never hid tracking mutations from nonowners (pre-existing). Server enforcement was not reviewed. |
| `g5.workspace.identity.base`, `g5.workspace.base-ui.*` | `RemoteTrackingActions` `isBase` branch unchanged | Is there a new Base control? | Preserved (none added). |
| `g5.design.reuse`, `g5.architecture.policy`, `g5.design.minimal-mechanism` | One derivation used by both the pill and the body. Duplicate `isBehind`/`isAhead`/`describePauseReason`/`isResetBelowBaselineReview` removed. No new hooks or API. | Are parallel rules left behind, or mechanisms nobody asked for? | Conforms. Nits: F7 (dead branches and exports), F8 (hand-rolled buttons instead of `Button`). |
| Retained remote integration (pause reasons, Resolve with agent, coordinated reset approval set) | Popover logic unchanged apart from removing `onActionCompleted` | Resolve with agent, the two-repo approval set and refreshed-checkpoint tests all pass. | Preserved. |
| A1 state mapping | `remoteTrackingStatus.ts` | Is any Lean pause reason dropped? | Holds. Every reason gets a label, and unknown reasons fall back to `Paused — <reason>`. |
| A2 action visibility | Popover and `RemoteTrackingActions` | Did controls or gating change? | Holds. Only tokens changed. |
| A3 single consumer | `grep` | Does anything else use the icon, or does ReposBar render tracking? | Holds. The only consumer is `WorkspaceDetailsPanel`, and ReposBar has no tracking. |
| A4 rewrite copy | Popover copy | Is a provider rewrite attributed to "this workspace"? | Not applicable. The only "this workspace" (`:391`) refers to the resolution agent. |
| A5 tokens | Pill, inline, popover, actions | Do raw palette classes remain? | Holds, apart from `text-white` on filled buttons, which matches `UI.tsx:419`. `CoordinatedRepoSyncModal` still uses raw classes (F11). |

## Findings

1. **Defect, minor (user-visible copy).** Governing rule: `g5.workspace.remote-ui.status` (the glyph is retired and state is read in words).
   - Where: `RemoteTrackingActions.tsx:113`. The queued-push notice still says "the tracking icon turns green once it lands", and the comment at `:64-65` also mentions "green tracking icon". The diff updated the comments at `:10` and `:330` but missed these two. "Watch Sync activity below" also doesn't match the "Recent pull/push activity" heading (pre-existing).
   - Consequence: users are told to watch for an icon that no longer exists.
   - Fix: reword to something like "…watch Recent pull/push activity below; the status pill reads Tracking healthy once it lands."
   - Regression check: a `RemoteTrackingActions` test that asserts the queued notice text and that it doesn't contain "icon".

2. **Missing evidence.** Governing rule: `g5.workspace.remote-ui.unknown` ("never guesses an action from missing … data").
   - Where: `BranchTrackingInline.tsx:61,69` gates on `mergeState` loading or error, and Retry (`:86-88`) invalidates `branchMergeRunsQueryKey`. None of these paths is tested; the tests only fail or delay sync-runs.
   - Consequence: deleting the merge-runs gate would still pass every test. That would bring back the old icon's behavior of showing a `watermark_violation` + reset run as "blocked" (a guess) while merge runs are missing.
   - Fix: add Inline tests for three cases:
     - merge-runs 500 → `unavailable` pill with no expander, then Retry recovers.
     - merge-runs delayed with a `watermark_violation` pause → `loading` pill (not `blocked`), then `review` once the runs arrive.
   - Regression check: both tests fail when `mergeState` is removed from the gates.

3. **Design question.** Governing rules: `g5.workspace.remote-ui.status` ("each remote-tracked repository card shows an always-visible status pill") and `.unknown`.
   - Where: `WorkspaceDetailsPanel.tsx:619` picks the inline view from workspace-detail `trackingMode`. `BranchTrackingInline.tsx:105` returns `null` when the sync-runs feed has no `remote_tracking` row for that repo. The card then shows neither a tracking pill nor the local pill.
   - Consequence: the state is hidden rather than labelled unknown. This happens transiently right after Publish or Make remote, when the detail flips before the sync-runs refetch, and whenever the two feeds disagree. The old icon (and G5) behaved the same, and `BranchTrackingInline.test.tsx:235` codifies it.
   - Proposed fix: when the parent says `remote_tracking` but the feed has no row, render an explicit pill (for example `loading`/"Tracking status pending", or `unavailable`) with no expander.
   - Regression check: an Inline test where the feed returns `trackingMode: "tracking"` and an explicit pill still appears.

4. **Defect, minor (accessibility regression).** Governing rule: the "feedback" clause of `.actions` (also general a11y).
   - What changed: before, a completed action closed the popover and returned focus to the trigger. The old approval test asserted `branch-tracking-icon` `toHaveFocus`, and that assertion was removed without a replacement.
   - What happens now: the detail stays open, but controls that unmount after success drop keyboard focus to `<body>`. That covers Confirm resolved, Resolve with agent (replaced by "Merge in progress"), and approved publish when the card leaves.
   - Where: `BranchTrackingPopover.tsx` `MergeRunCard` handlers and `PausedDivergenceResolution`.
   - Fix: after success, move focus to the detail's status line (`tabIndex=-1`) or to the expander.
   - Regression check: after Confirm resolved succeeds, `document.activeElement` is not `document.body`.

5. **Nit (accessibility).**
   - Where: `BranchTrackingInline.tsx:115-129`. Every expander has the same accessible name ("Details & actions"), so screen-reader users can't tell repositories apart in a multi-repo workspace.
   - Also: every pill, including the static "Local only" one, has `role="status"` (a live region), so a background poll that changes a state is announced.
   - Fix: pass the repo label into `BranchTrackingInline` and set `aria-label` (for example "Details & actions for acme/api"), or point `aria-describedby` at the pill. Consider dropping `role="status"` from the static local pill.
   - Check: `getByRole("button", { name: /acme\/api/ })`.

6. **Nit (consistency).** Governing text: design step 1 says the pill and any banner showing the same priority both consume the derivation.
   - The state is shared, but the expanded header words it differently (`BranchTrackingPopover.tsx:727-733`):
     - Pill "Remote moved — your branch is behind" vs header "Remote changes available".
     - Pill "Review required — approve rebased publication" vs header "Paused — provider history was rewritten by a rebase — publication approval required". The header calls a review state "Paused".
   - Both phrasings are visible together once the card is expanded.
   - Fix: either lead the header with `status.label` and keep the longer explanation as the sub-line, or align the wording.

7. **Nit (dead code / minimal implementation).**
   - `BranchTrackingPopover.tsx:633-655` (its own loading and error bodies) and the `?? "healthy"` fallback at `:674` can't be reached in production, because the only consumer gates on the same queries and returns `null` first.
   - `isBehind`, `isAhead`, `isResetBelowBaselineReview` and the `RemoteTrackingStatus` interface are exported but only used inside `remoteTrackingStatus.ts`. `remoteTrackingLabel` is exported only for its test.
   - Stale names and comments:
     - The test title "…the icon+tooltip indicate" (`BranchTrackingPopover.test.tsx:399`).
     - `useRemoteTracking.ts:58,87` ("tooltip", "icon").
     - The component and test are still called "Popover" although it is now an inline body.
   - Fix: drop or un-export what's unused and update the comments. A rename is optional.

8. **Design question (theme legibility).** Relevant text: the acceptance outcome "legible in light and dark themes", and `g5.design.reuse`.
   - Where: `RemoteTrackingActions.tsx:343,372,408`, `BranchAutoPullControl.tsx:137`, `BranchTrackingPopover.tsx:428,503`, `RemoteConflictAgentLauncher.tsx:106`. The filled buttons moved from `bg-slate-800` / `bg-rose-700` to `bg-[var(--g5-info|blocked)] text-white`.
   - Contrast (computed): white on `--g5-info` is about 2.5:1 in editor dark and about 3.7:1 in editor light. White on `--g5-blocked` is about 2.8:1 in editor dark. All are below AA 4.5:1 for `text-xs`.
   - Hue collisions: in `signal-studio`, `--g5-info` is red-orange, so the "ahead" pill and the Pull/Push buttons share the blocked hue. In `obsidian-gold`, info and warn are both gold. The labels still carry the meaning.
   - This matches the existing `Button kind="primary"` convention (`UI.tsx:419`), so it isn't unique to this PR.
   - Proposed fix: reuse `Button` instead of hand-rolled classes, and raise filled-button contrast in the design system as a follow-up.

9. **Missing evidence (test coverage).**
   - (a) There's no assertion that each action appears exactly once in the expanded body. For example, in the ahead state `getAllByTestId("push-to-remote-button")` should have length 1.
   - (b) Stacked multi-repo cards and independent per-card expansion aren't tested in `WorkspaceDetailsPanel.test.tsx`.
   - (c) The "stays open" assertion after approval (`BranchTrackingPopover.test.tsx:851`) runs as soon as the request is seen, before the mutation settles, so it can't catch a close. (No close path remains, so the risk is low.)
   - Fix: add (a) and (b). For (c), wait for the button to return to idle before asserting.

10. **Missing evidence (ontology metadata / design record)**, for the consolidator to apply.
    - All five footer entries in `workspaces.g5.md` (`:482`, `:644`, …) still say "Unassessed · not yet implemented; …BranchTrackingIcon…", which is now false. Proposed text:
      - `g5.story.see-remote-tracking-state`: "Partial · WorkspaceDetailsPanel.tsx renders BranchTrackingInline / RemoteStatusPill; gap: a tracked repo with no sync-feed row shows no pill (F3). Unit-tested; not browser-verified."
      - `.status`: "Implemented · remoteTrackingStatus.ts, RemoteStatusPill.tsx; unit-tested."
      - `.detail`: "Implemented · BranchTrackingInline.tsx mounts BranchTrackingPopover in flow; not browser-verified for scrolling."
      - `.unknown`: "Partial · BranchTrackingInline.tsx gates both feeds; merge-runs path untested (F2); null-row gap (F3)."
      - `.actions`: "Implemented · head holds pill + expander; post-action focus gap (F4)."
    - The plan's Design step 3 describes a "new `inline` variant". The implementation instead deleted the floating variant and the `onActionCompleted` prop. That is justified by A3, but the plan should record it. `onActionCompleted` legitimately stays on `BranchAutoPullControl`, which `BranchActionsMenu` uses to close its own popover.

11. **Nit (out of acceptance scope).** `CoordinatedRepoSyncModal.tsx:23-72`, opened from `RemoteTrackingActions`, still hard-codes `bg-white!`, rose and slate classes, so it renders light in dark themes. Consider a follow-up.

12. **Design question (flapping).** Governing rule: `.unknown`.
    - Behavior: in React Query v5, a failed 15 s background poll of either feed sets `isError` even though cached data exists. `BranchTrackingInline` then swaps to the "unavailable" pill and unmounts the expanded body. That throws away local state: the Mark-resolved confirm step, the push notice and its settle timers, and the chosen agent. `expanded` is kept, so the body comes back on recovery.
    - Comparison: the old icon collapsed the same way on a sync-runs error. The new code also collapses on a merge-runs error.
    - This conforms to "never guesses … from stale data". The question for a human is whether a transient refetch failure over known data should instead keep the body and only disable mutations.

## Checks run

- `git diff -- .g5/ontology` vs the plan's recorded diff: identical.
- `npx tsc --noEmit -p tsconfig.json`: exit 0.
- `npx eslint` on all changed and new files plus `e2e/flows/data-wiring.spec.ts`: 0 errors. One warning (`BranchSummary` unused, `WorkspaceDetailsPanel.tsx:52`) is pre-existing.
- `npx vitest run` on remoteTrackingStatus, BranchTrackingInline, BranchTrackingPopover, WorkspaceDetailsPanel, RemoteTrackingActions, BranchAutoPullControl, BranchActionsMenu and MakeRemoteDialog: 8 files, 106 tests, all pass.
- `grep` for leftovers:
  - `branch-tracking-icon` / `BranchTrackingIcon`: none in `frontend/`; only historical docs and the stale ontology footers.
  - Popover `onActionCompleted`: removed. Remaining uses are on `BranchAutoPullControl`, which is valid.
  - No E2E spec references the tracking test ids.
- Discrimination was assessed by inspection only. No mutation testing was run (read-only).

## Unverified areas

- Browser rendering: in-flow expansion and page scroll at the viewport bottom, the pill in every theme and mode, and E2E `data-wiring` (not run).
- Server-side owner authority for the tracking mutation endpoints (the client doesn't gate them for nonowners, as before).
- The a11y scan (`e2e/flows/a11y-scan.spec.ts`) against Workspace Details with a tracked repo, and color-contrast of the new pills and buttons at runtime.
