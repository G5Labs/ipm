# Three preview run-log UX problems observed on QA (2026-10-27)

Workspace `preview-ux-post-improvements`, owner conversation of 2026-10-27.

## Human instructions

1. The owner's project document "Request: fix three preview run-log UX problems
   (observed on QA, 2026-10-27)", reproduced below as supplied. It records the
   symptoms, the load-balancer and API evidence, the root causes and a proposal
   (P1 superseded viewer, P2 no panel rebuild while waiting, P3 dialog width),
   the order P3 → P2 → P1, the ontology blocks to amend and the acceptance.
2. Chat: "implement the request, after that, invoke codex to review the code until
   converge and then create a pr, resolve coderabbit comments until converge and
   fix CI failures if any. When everything is green, send me an email."

## Agent interpretation

- The proposal's minimal P1 is taken; the multi-viewer alternative is recorded as
  rejected because it would change the settled "one viewer at a time" decision of
  [g5.preview.output.live-tail](../../ontology/software_architecture/workspace_vm.g5.md#g5.preview.output.live-tail),
  which the owner did not re-decide.
- P2's optional guest-side wait in `open_hosted_pty` is not taken: holding the first
  attach until the run is dispatched plus the existing bounded retry already meets
  "at most one attach before `invocation.started`".
- The design is [ontology-plan.md](../../pr_ontology_diff_design/preview-ux-post-improvements/ontology-plan.md).

## The owner's document, as supplied

# Request: fix three preview run-log UX problems (observed on QA, 2026-10-27)

## Context

The preview run log (`g5.preview.run-log`) streams the application's output through a guest-hosted
terminal session `preview:<task id>` (`g5.preview.output.live-tail`, commit 5a70955b0). On QA
(workspace `preview-ux-improvement`, VM on image `d8411c47`, which includes that feature) three UX
problems reproduce. Evidence comes from the GCP load-balancer logs for
`GET /api/v1/workspaces/<id>/terminal`, the g5-api logs, QA DB task/VM rows and the code on `main`.

Out of scope: VMs booted from an image older than the feature (their dispatcher refuses every
`preview:` attach and the log retries 24 times, ~70 s of flicker). That is fixed by restarting the
workspace; do not design around it beyond what problem 2 already covers.

## Problem 1 — two browser tabs viewing the same preview log kick each other out

**Symptom.** Open the Log in one tab, then open the same workspace in a second tab and open the Log
there. Both headers cycle `reconnecting at offset N`; each connection lives 1–6 s; after ~35 s one tab
stops at `connection failed`. The workspace shell terminal (`?view=terminal`) does not have this
problem because its terminal id lives in per-tab `sessionStorage`, so each tab gets its own shell.

**Evidence (LB log, same client IP).** Consecutive `/terminal` connects alternate between two offsets
(`offset=9686`, `offset=9812`, …), i.e. two viewers with independent progress take turns; a fresh
attach after the loser gave up stayed connected 67 s.

**Root cause.**

**Root cause.**
1. `TerminalManager::attach_hosted` (`backend/g5d-foundation/src/terminal/mod.rs`) bumps
 `attachment_generation` and cancels the previous viewer; its relay exits `PtyExit::Superseded`.
2. The hosted branch of `finish_pty` (`backend/g5d-dispatch/src/server.rs`) sends no `closed` control
 line on `Superseded`; it just closes the stream. `TerminalClose` has only `SessionEnded`.
3. `useWorkspaceTerminalSocket.ts` sees a plain `onclose`, reconnects with exponential backoff
 (250 ms → 5 s) at its offset, and supersedes the other tab in turn.
4. The attempt counter resets only after 30 s stable (`STABLE_CONNECTION_MS`), never reached while
 flapping, so each side stops after `MAX_RECONNECT_ATTEMPTS = 8`. That is why it "settles".

**Governing decision.** `g5.preview.output.live-tail` records "one viewer at a time" (owner, 2026-10-20).
Single viewer is settled; what the superseded side does is an implementation detail, not a decision.

## Problem 2 — the log panel flickers 2–3 times right after Start preview

**Symptom.** Click Start preview, open Log immediately: for 5–8 s the panel alternates between an empty
terminal and the line `Waiting for the application to start…`, then follows normally.

**Timeline (task 737d2ed3).** 05:55:54.9 task created → 05:55:57 attach refused → 05:55:59.8 task
dispatched to the VM → 05:56:00 attach refused → 05:56:02.8 module started (`invocation.started`,
session hosted here by `modules/runner.rs`) → 05:56:03 attach succeeded.

**Root cause.**
1. `PreviewRunLog.tsx` retries a refused live-run attach every `ATTACH_RETRY_DELAY_MS = 2500`, up to
 `ATTACH_RETRY_LIMIT = 24` (intended by `g5.preview.run-log.attach`).
2. Each retry remounts the whole `WorkspaceTerminalPanel` via `key={refusedHere.attempts}` (xterm,
 FitAddon and socket rebuilt), and between attempts `outputAvailable` is false so the panel is
 replaced by one line of text and the dialog height changes. That remount is the visible flicker.
3. Guest side, `open_hosted_pty` answers `rejected` immediately for a session that does not exist yet.

## Problem 3 — after Stop preview the dialog keeps the wide (log) width

**Symptom.** Log open → dialog grows from `sm` (`max-w-md`) to `xl` (`max-w-5xl`). Stop preview →
dialog shows the launcher (profile dropdown + Start preview) but stays `xl`. Expected: same width as
before the log was opened.

**Root cause.** `PreviewControl.tsx` drives both "render the log" and "shell width" from one
`logOpen` state with different lifetimes:
1. `<Modal size={logOpen ? "xl" : "sm"}>`.
2. `logOpen` is reset only when the dialog closes (`useEffect`: `if (!open) setLogOpen(false)`).
3. After Stop, `active` is false and the launcher branch renders `PreviewRunLog` only for
 `lastFailed` / `lastCloseFailed`; a clean cancel has neither, so no log but an `xl` shell.
 Side effect: a later Start preview in the same dialog auto-expands the log.

## Proposal

### P1 — superseded viewer: tell the browser, stop fighting (fixes problem 1)

Minimal, keeps "one viewer at a time":
- `backend/g5-contracts/src/terminal.rs`: add `TerminalClose::Superseded`.
- `backend/g5d-dispatch/src/server.rs`: in the hosted branch of `finish_pty` (or where
`PtyExit::Superseded` is produced for a hosted viewer), send
`TerminalServerMessage::Closed { reason: Superseded }` as the stream's last frame before closing.
Send it only to the viewer being replaced; the new viewer's `opened` is unchanged.
- `frontend/src/components/execution/useWorkspaceTerminalSocket.ts`: on `closed` with
`superseded`, stop reconnecting (`terminalRejected = true`), keep the output on screen, publish
an attachment state; `terminalViewer.ts` renders it as `viewing in another window` (or similar) and
`WorkspaceTerminalPanel` offers a `Take over` button that re-attaches at the viewer's offset.
- Alternative (larger, changes the settled decision): per-viewer cursors so several tabs follow
the same session. The ring already serves by offset; the limiter is the single
`attachment_cancel`/generation and `resize_hosted`. Do not pick this without the owner re-deciding
`g5.preview.output.live-tail`; note it as the rejected alternative in the design.

Tests: dispatcher test in `dispatch_modules` (second attach → first stream ends with
`closed/superseded`, second keeps streaming, detach of the loser does not end the session);
`useWorkspaceTerminalSocket` unit test (no reconnect after `superseded`, Take over re-attaches at
offset); Playwright `workspace-preview.spec.ts` with two pages on one preview log: no flapping, loser
shows the notice, Take over swaps roles.

### P2 — do not rebuild the panel while waiting for the session (fixes problem 2)

Pick the first; add the second if the first alone still shows a blink:
- `PreviewRunLog.tsx`: keep `WorkspaceTerminalPanel` mounted across refused attempts. Drop
`key={attempts}`; instead expose a `retryKey`/`restart` on the panel (the socket hook already has
`restart`) and call it on each retry. Show `Waiting for the application to start…` as an overlay or a
header status inside the fixed-height (`h-72`) container, not as a replacement element, so the
dialog's height and the terminal's DOM stay stable.
- Delay the first attach until the run is known to be dispatched: the run log already polls the
task timeline (`entries`); do not attach before `invocation.started` is present (or the preview view
reports a stage past Queued). This removes the two always-refused attempts.
- Optional guest-side: `open_hosted_pty` may wait a bounded few seconds for a `preview:` session that
does not exist yet before answering `rejected`, so the first attach usually succeeds. Only if the
two frontend changes are not enough.

Tests: `PreviewRunLog.test.tsx` — a refused-then-hosted sequence renders one terminal instance
(no unmount; assert the xterm host element identity is stable) and the waiting text appears inside
the panel; no attach request before the timeline shows the run started. Playwright run-log case:
start preview, open Log at once, assert the terminal element is not re-created.

### P3 — derive the dialog width from whether a log is rendered (fixes problem 3)

- `PreviewControl.tsx`: compute `logShown` = a `PreviewRunLog` is actually rendered in the current
branch (`active` branch: `logOpen`; launcher branch: `logOpen && (lastFailed || lastCloseFailed)`;
"Stop and start"/merge branches: false) and use `size={logShown ? "xl" : "sm"}`.
- Also reset `logOpen` when a run ends cleanly (active → not active with neither failure notice), so a
later Start preview does not auto-expand the log. Keep `logOpen` across active → failed so the
failure notice's log stays open (existing behavior/tests).

Tests: `PreviewControl.test.tsx` — Log open on a running preview → dialog has the `xl` class; Stop
preview (clean cancel) → dialog back to `sm` and the Log toggle collapsed; failed run with log open →
stays `xl` (regression guard for the existing "drops the terminal once the VM stops" case).

### Order and size

1. P3 first (frontend only, ~30 LOC + tests).
2. P2 frontend changes (~80 LOC + tests).
3. P1 (contract + dispatcher + frontend, ~150 LOC + tests; needs the ontology amendment below).

### Ontology

- `g5.preview.output.live-tail.viewer`: add how a superseded viewer is told (`closed/superseded`)
and that it stops reconnecting; record the rejected multi-viewer alternative.
- `g5.preview.run-log.attach`: state that retries keep the terminal mounted and start only once the
run is dispatched.
- `g5.preview.dialog-controls` (ui.g5.md): the shell is wide only while a log is rendered.
Follow the G5 workflow (ontology-planning → implement with E2E coverage → ontology-review).

## Acceptance

- Two tabs on one preview log: no `reconnecting at offset` loop; the second tab follows, the first
shows a notice with Take over; LB shows no `/terminal` connects shorter than the session itself.
- Start preview + Log: no visible remount; at most one attach before `invocation.started`.
- Log open → Stop preview: dialog width equals the pre-log width; Start again does not auto-open Log.
- `just ci-frontend` (vitest + Playwright preview cases) and the dispatcher/foundation Rust tests
green.