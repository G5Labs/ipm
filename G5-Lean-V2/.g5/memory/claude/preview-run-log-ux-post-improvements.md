---
name: preview-run-log-ux-post-improvements
description: "Workspace preview-ux-post-improvements (2026-10-27): three preview run-log UX fixes (superseded viewer, mounted panel while waiting, dialog width); design path, decisions, ship loop"
metadata:
  node_type: memory
  type: project
  originSessionId: 6e68d479-644d-4a8c-ab40-c97cde69b35d
  modified: 2026-10-27T06:46:44.195Z
---

Workspace `preview-ux-post-improvements` (branch of the same name, no linked issue; the
request lives in the project doc `request.md`, copied to
`.g5/chat_intent/preview-ux-post-improvements/request.md`). Design:
`.g5/pr_ontology_diff_design/preview-ux-post-improvements/ontology-plan.md`.

Decisions taken 2026-10-27 (agent, within the owner's proposal):
- P1: `TerminalClose::Superseded` sent by the hosted branch of `finish_pty`; browser stops
  reconnecting, header says "viewing in another window", Take over reattaches at the kept
  offset on the same xterm (`takeOver` in the socket hook). Multi-viewer rejected: it would
  reopen the owner's settled "one viewer at a time".
- P2: panel props `attach` + `pendingStateText`; first attach held until
  `previewRunStarted` (invocation.started / module.* / ready); no guest-side wait.
- P3: `logShown` drives Modal size; `logOpen` resets when a run ends with no notice.
- The socket hook resets published state at the start of each attachment lifetime with the
  render-time "seen inputs" pattern; `react-hooks/refs` lint flags `send` in that tuple, so
  it is excluded.

Ship loop requested by the owner: Codex review until converge → `g5d pr create` → resolve
CodeRabbit threads until converge → fix CI → email the owner when green.

**Why:** the owner's instruction set the whole loop in one message; a later session must
not re-ask or re-design.
**How to apply:** continue from the design doc; keep the three-commit series
(P3, P1, P2 order so each compiles); see [[g5d-publish-after-rebase]] and
[[e2e-testid-sweep-after-rebase]] before publishing.
