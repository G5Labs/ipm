---
name: issue-786-preview-mocks
description: Issue #786 (preview dialog controls + preview in a G5 app tab) — mocks in docs/mocks/preview-dialog-and-tab; dialog half (cue A) shipped 2026-10-26 on preview-ux-improvement (PR #846); app-tab half designed with the owner's four answers, implemented and reverted the same day at the owner's request; pick up from the saved design, do not re-ask.
metadata:
  type: project
---

Workspace `preview-ux-improvement` is linked to G5Labs/G5-Lean#786. On 2026-10-26 the owner asked
for mockups of the issue's proposal; they live in `docs/mocks/preview-dialog-and-tab/` (README,
index.html, fragment, 13 PNGs). The issue says implementation remains deferred and @rahul-g5 asked
@kaihuiz-g5 to take it with PR #693 (which is CONFLICTING against main as of 2026-10-26).

**Why:** The mock proposed decisions that are not in the issue; the owner settled them on 2026-10-26 (answers in `.g5/chat_intent/preview-ux-improvement/request.md`):
not-ready cue variant A (button label "Starting…") vs B (header pill); the tab is a route
`/:org/:workspace/preview` that frames the isolated preview origin in an iframe (third-party
handoff cookie, frame-ancestors, WebSocket upgrades must be verified); closing the tab never stops
the preview; the tab stays after Stop with a restart; tab label "Preview · <workspace>".

On 2026-10-26 the owner chose cue A ("Starting…" on the Open button) and asked to start implementing; the dialog half shipped on the branch as `g5.preview.dialog-controls` (one Modal shell, Stop preview label, wrapping actions; labels changed in PreviewControl.test.tsx and workspace-preview.spec.ts). The branch is PR #846; PR #693's commits were integrated into it (below).

**How to apply:** The app-tab half is settled, not open: the owner approved the design
(`.g5/pr_ontology_diff_design/preview-ux-improvement/ontology-plan.md`, governing
`g5.preview.app-tab`) and answered its questions (the tab stays with a stopped state after
Stop; × only closes the tab; remembered across reloads like other tabs; the slim bar as
mocked), the implementation (commit 9f652a8b2) was reverted the same day at the owner's
request "for now". When the owner picks it up again, implement from that saved design and
those answers; do not re-run design or re-ask them. Reuse the run log unchanged and treat the
issue's screenshots as unavailable from the VM (GitHub user attachments need a session).
See [[ui-mocks-convention-and-snapshots]] for rendering PNGs.

**2026-10-26 integration:** at the owner's choice the branch was rebuilt as latest main +
PR #693's 11 commits (rebased, conflicts resolved in g5-contracts terminal.rs,
WorkspaceTerminalPanel.tsx, useWorkspaceTerminalSocket.ts, two e2e specs, MEMORY.md) +
the dialog commits (PreviewControl merged with #693's Log toggle/run log; ui.g5.md keeps
run-log and dialog-controls blocks) + reconciliation commits (viewer without input
subscriptions; `detail: None` on the hosted attach refusal; Codex-review scroll fix). Full
vitest (5118) and the #693 Rust suites passed; Playwright not run. Codex review:
`codex review --base main -c 'model="gpt-6-astra"'`, one P2 finding, fixed. Force-pushed
with lease over a9a341d77. PR #693 itself was not touched (not my branch).
