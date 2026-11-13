# Issue 240 review: inline remote-tracking status

**Mode:** one independent read-only discovery review by a fresh subagent that had not seen the implementation session, followed by consolidation and one repair by the implementing agent. This was judged a localized change (one UI surface, no shared contract, authority, lifecycle or persisted-state change), so the single-review form was used rather than three parallel reviews. The raw findings are preserved unchanged in [ontology-review-issue-240.findings.md](ontology-review-issue-240.findings.md).

**Inputs:** [request](chat_intent/issue-240/request.md), [design](ontology-plan-issue-240.md), the working-tree diff on `ben/issue-240-remote-branch-ui`.

## Initial result

The reviewer found no blocking defect. The recorded ontology diff matched the actual diff byte for byte. All five changed commitments and the governing ones conformed, subject to the findings below. Open assumptions A1–A3 and A5 held; A4 did not apply, because no copy attributes a provider rewrite to "this workspace".

Checks the reviewer ran before any repair:
- `tsc`: clean.
- `eslint`: 0 errors; one warning (unused `BranchSummary`) was pre-existing.
- `vitest`: 106 tests across 8 affected suites passed.

## Findings and dispositions

| # | Finding | Kind | Disposition |
|---|---|---|---|
| 1 | The queued-push notice and a comment still pointed users to a "tracking icon" | Defect (copy) | **Fixed.** The notice now points to "Recent pull/push activity" and the "Tracking healthy" status. Stale hook comments were also updated. |
| 2 | The merge-runs loading/error gate and its retry had no tests | Missing evidence | **Fixed.** Added tests for merge runs delayed during a watermark pause (loading, then review) and merge runs failing (unavailable, then Retry recovers). Removing the merge-runs gate makes the delayed test fail. |
| 3 | A repo the workspace marks remote-tracked, but with no sync-feed row, showed no pill | Design gap vs `g5.workspace.remote-ui.status` / `.unknown` | **Fixed and amended.** The card shows a "Tracking status pending" pill with no expander. `.unknown` now names the not-yet-reported case. |
| 4 | Focus fell to `<body>` when an acted-on control unmounted, now that the detail stays open | Defect (a11y regression) | **Fixed and amended.** `BranchTrackingInline` returns focus to the expander when the focused element inside the detail is removed. `.actions` records the stay-open and focus-return decision. Disabling the restore makes the regression test fail. |
| 5 | Every expander had the same accessible name; every pill was a live region | Nit (a11y) | **Fixed.** The expander is named "Details & actions for `<repo>`". `role="status"` was removed from the pill. |
| 6 | The pill and the detail header worded the same state differently ("Review required" vs "Paused …") | Nit | **Fixed.** The header shows the shared label, with the pause reason on its own line below. |
| 7 | Dead code: unreachable loading/error bodies in the popover, unused exports, stale titles | Nit | **Fixed.** Those branches are now a single guard, helper exports were narrowed, and the stale test title was renamed. The component keeps its `BranchTrackingPopover` name so test ids and history stay stable. |
| 8 | Filled buttons converted to `--g5-info` / `--g5-blocked` with white text fall below AA contrast in dark themes | Design question | **Fixed within scope.** Converted buttons now use `--g5-text0` fill with `--g5-bg0` text. The token contrast guard already requires that pair to reach ≥ 4.5:1 in every palette, and it is the neutral equivalent of the original slate-800/white. The pre-existing info-filled approve button and `Button kind="primary"` are unchanged; raising their contrast belongs in the design system. |
| 9 | "Once" was not asserted in the expanded body; stacked cards were untested; one stay-open assertion ran before the action settled | Missing evidence | **Fixed.** Added assertions that Push and Pull each appear once, a stacked two-repo card test, and a wait for the approval to settle. |
| 10 | Footer metadata still said "not yet implemented"; the design described an inline variant that was not built | Metadata | **Fixed.** Footers were reassessed and the design records that the floating variant was removed. |
| 11 | `CoordinatedRepoSyncModal` still uses raw light-theme classes | Out of scope | **Deferred.** Offered as a follow-up task. |
| 12 | A single failed background poll over cached data switches the card to unavailable and unmounts an open detail | Design question | **Open.** This matches the previous popover's error behavior and the current `.unknown` rule. It is recorded as an unresolved question in the design for a human decision. |

## Commitment ledger after repair

- `g5.story.see-remote-tracking-state` → `WorkspaceDetailsPanel.tsx` → the panel tests check the local-only pill, a tracked pill outside the chip, and stacked cards → **implemented; unit-tested.**
- `g5.workspace.remote-ui.status` → `remoteTrackingStatus.ts`, `RemoteStatusPill.tsx` → derivation and label tables, rendered per-state pills, and header/pill text equality → **implemented; unit-tested.**
- `g5.workspace.remote-ui.detail` → `BranchTrackingInline.tsx` plus the popover body → no-portal, collapsed-by-default, Escape and outside click ignored → **implemented; unit-tested; page scroll not browser-verified.**
- `g5.workspace.remote-ui.unknown` → `BranchTrackingInline.tsx` → loading, unavailable with retry (both feeds), merge-runs delay, and pending → **implemented; unit-tested; finding 12 open.**
- `g5.workspace.remote-ui.actions` → head holds pill and expander only; body renders actions once; focus returns to the expander → **implemented; unit-tested.**
- `g5.security.owner`, `g5.security.read.durable` → body gating unchanged → **preserved.** The client never hid these controls from nonowners; server enforcement was not reviewed.
- `g5.workspace.identity.base`, `g5.workspace.base-ui.*` → no new Base control → **preserved.**
- `g5.design.reuse`, `g5.architecture.policy`, `g5.design.minimal-mechanism` → one shared derivation, duplicates removed, no new API or hooks → **conforms.**
- Retained remote integration → Resolve with agent, the coordinated approval set and refreshed checkpoints still pass their tests → **preserved.**

## Rechecks after repair

- `tsc` (project and build configs): clean.
- `eslint` on the changed files: 0 errors; the same pre-existing warning.
- Full frontend `vitest`: 235 files, 3598 tests, all passing.
- Mutation checks: disabling focus restore fails the Mark-resolved focus test; removing the merge-runs loading gate fails the merge-runs delay test.
- Browser (local dev stack, Vite from this worktree): the Base workspace's Repos & branches card renders a "Local only — not published" pill under each chip with no horizontal overflow. No workspace on that stack has a tracked repo, so tracked states were not seen in a browser.

## Unverified

- Tracked-state rendering, in-flow scrolling and pill contrast in a browser.
- The E2E `data-wiring` and a11y-scan specs (not run).
- Server-side owner enforcement for the tracking mutation endpoints.
