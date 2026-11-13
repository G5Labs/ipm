# Preview dialog controls and the preview app tab (issue #786)

Workspace `preview-ux-improvement`, owner conversation of 2026-10-26.

## Human instructions

1. "Can you check the issue and create the mockups based on the proposal?" — the
   proposal is [issue #786](https://github.com/G5Labs/G5-Lean/issues/786).
2. "can you remove the opening state?" — the mock's pending-Open state was dropped;
   the dialog's states are starting, serving, stopping and last failed.
3. "can you start implementing the button label version?" — of the two not-ready
   cues the mock compared, the owner chose **A**: the Open button itself says
   "Starting…" with a spinner while the application starts.
4. "can you also implement the tab change? ask me any open questions" — and the
   answers, each the recommended option:
   - After the preview stops (Stop in the dialog or the tab's bar, or the VM
     stops): the preview tab **stays and shows a stopped state** with a restart
     and a way back to the workspace.
   - Closing the preview tab with its ×: **only closes the tab**; the preview
     keeps serving and Open preview brings the tab back.
   - Persistence: the preview tab is **remembered across reloads like other
     tabs**, in the same per-user list; a reload re-mints the session or shows
     the stopped state.
   - Above the framed application: the **slim bar as mocked** (workspace,
     profile, port, state pill, Reload, Open in browser, Stop preview); Open in
     browser is the fallback when an application still refuses framing.

5. "ok i want to revert the tab change for now" — the app-tab implementation
   (commit 9f652a8b2) was reverted the same day; the dialog half stays. The design
   and the answers above are kept for when the tab change is picked up again.

6. "can you invoke codex astra to review, and also resolve the merge conflicts with
   693" — asked how, the owner chose: rebuild this branch as latest main, then PR #693's
   commits with their main conflicts resolved, then the dialog change with its #693
   conflicts resolved (one conflict-free branch holding both); and run the Codex
   `gpt-6-astra` review over the integrated branch. Done 2026-10-26; the review's one
   finding (the viewer ignored a reader's scroll while writes were in flight) was fixed.

## Agent interpretation

- The dialog half shipped first as `g5.preview.dialog-controls` (one Modal shell,
  Stop preview label, wrapping actions). The app-tab half is designed in
  [the plan](../../pr_ontology_diff_design/preview-ux-improvement/ontology-plan.md)
  and governed by `g5.preview.app-tab`.
- The app tab frames the preview's isolated origin. The proxy already admits the
  platform origin as a frame ancestor and the QA preview hosts are same-site with
  the app, so no backend change is needed; the plan records the assumption that a
  separate registrable domain, if ever adopted, would need a `SameSite=None;
  Partitioned` preview cookie.
- Outside the tab strip (layouts without it) Open preview keeps today's browser-tab
  path: there is no app tab to open there.
- Integrating #693 here means this branch carries Kaihui's eleven commits rebased onto
  main plus two reconciliation commits (the read-only viewer keeps none of main's input
  subscriptions; the hosted attach refusal carries main's `detail: None`). PR #693 can be
  updated from this branch or superseded by it; that is the owner's call.
