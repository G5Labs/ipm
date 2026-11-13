# Preview run log: superseded viewers, a stable panel while waiting, dialog width

Request: [chat intent](../../chat_intent/preview-ux-post-improvements/request.md)
(the owner's QA finding of 2026-10-27 with evidence, root causes and a proposal).
Prior design on the same path: [preview-status-observation](../preview-status-observation/ontology-plan.md)
(the hosted session and its viewer) and [preview-ux-improvement](../preview-ux-improvement/ontology-plan.md)
(the dialog shell).

## Governing intent

- [`g5.preview.output.live-tail`](../../ontology/software_architecture/workspace_vm.g5.md#g5.preview.output.live-tail):
  the output is dispatcher memory on a hosted session; settled with the owner:
  **one viewer at a time**. Not reopened here.
- [`g5.preview.output.live-tail.viewer`](../../ontology/software_architecture/workspace_vm.g5.md#g5.preview.output.live-tail.viewer):
  the viewer rides the unchanged terminal transport and
  [attachment rules](../../ontology/features/terminal.g5.md#g5.runtime.terminal.attachment)
  (a reattachment invalidates the previous generation). **Amended:** how the invalidated
  viewer is told, and what it does.
- [`g5.preview.run-log.attach`](../../ontology/features/ui.g5.md#g5.preview.run-log.attach):
  a refused attach on a live run is retried, bounded. **Amended:** the retry keeps the
  terminal mounted and the first attach waits for the run to be dispatched.
- [`g5.preview.dialog-controls`](../../ontology/features/ui.g5.md#g5.preview.dialog-controls):
  one shell, one dismiss. **Amended:** the shell is wide only while a log is rendered.

## Problem 1 — two tabs on one log fight over the single viewer slot

**Facts and owners.** The terminal manager (`g5d-foundation`) owns the viewer
generation: `attach_hosted` cancels the previous viewer, whose relay exits
`PtyExit::Superseded`. The dispatcher's hosted `finish_pty` owns what the superseded
stream is told; today nothing, so the browser's socket sees a plain close and applies
its reconnect policy, superseding the other tab in turn. The flapping is two correct
local policies with no shared fact: neither side knows it was *replaced* rather than
*disconnected*.

**Change.** Add the missing fact to the wire, at the one place that knows it:

- `g5-contracts` `TerminalClose::Superseded` beside `SessionEnded`.
- Hosted `finish_pty`: on `PtyExit::Superseded` send `closed`/`superseded` as the
  stream's last frame, then close. Only the replaced viewer receives it; the new
  viewer's `opened` is unchanged; shells are unchanged (their stream still closes
  silently and the browser learns on the next `open`, per the contract comment).
- Browser (`useWorkspaceTerminalSocket`, hosted mode): `closed`/`superseded` is final
  like `session_ended`: no reconnect, the output stays on screen, the attachment
  publishes `superseded`. The viewer header says *viewing in another window* and
  offers **Take over**, which reattaches on the same surface at the viewer's own
  offset (the hook keeps the last offset); the ring serves it unchanged, so a viewer
  within the retained window resumes without a redraw and one beyond it gets the
  normal `reset` with the "earlier output" line. The other tab then receives
  `superseded` and shows the same notice: control changes hands only on a human click.

**Why not multiple viewers.** Per-viewer cursors are already how the ring is read; the
limiters are the single attachment generation (what `resize_hosted` applies to and what
fences a stale viewer) and the settled owner decision. Lifting it would need a resize
policy among viewers and a new owner decision; the request forbids designing it without
one. Rejected, recorded in the viewer block's metadata.

**Capacity.** Unchanged: the 4 MiB ring bounds how far a superseded viewer can fall
behind and still resume without a redraw.

## Problem 2 — the panel is rebuilt while the session does not exist yet

**Facts and owners.** `PreviewRunLog` owns the run-level knowledge (is the run live,
how many refusals, the fallback notice); the panel and its socket hook own one
attachment lifetime. Today a retry is expressed as a *new panel* (`key={attempts}`) and
the wait as a *different element*, so the dialog's height and the terminal's DOM change
on every attempt; the first two attempts are always refused because the guest hosts the
session only once the module starts.

**Change.** Separate "the surface exists" from "an attachment is open":

- `WorkspaceTerminalPanel` gains `attach` (default true): false keeps the xterm surface
  mounted and the socket closed; flipping it true opens an attachment on the same
  surface. The hook's effect already models one attachment lifetime on its `enabled`
  input; it now resets its published status and attachment at the start of each
  lifetime so stale words never leak into the next one. No `key`, no remount.
- `WorkspaceTerminalPanel` gains `pendingStateText`: the header's state words while the
  parent is still arranging the attachment. `PreviewRunLog` passes *waiting for the
  application to start…* for a live run until its own attach has connected once; the
  fixed-height container and the terminal element are stable throughout.
- `PreviewRunLog` holds the first attach of a live run until the run is dispatched:
  `previewRunStarted` (an `invocation.started` or `module.*` timeline entry, or the view
  reporting ready). An ended run attaches at once. The timeline poll (2 s) bounds the
  delay; the existing bounded retry covers the race between the entry and the guest
  opening the PTY, so at most one attach precedes `invocation.started`.
- Not taken: a guest-side wait in `open_hosted_pty`. The two browser changes meet the
  acceptance; a wait would hold a mux stream open for a session that may never come.

## Problem 3 — the dialog keeps the log's width after a clean stop

**Facts.** `PreviewControl` renders a `PreviewRunLog` only in two branches (active run;
launcher with a failure notice) but sizes the shell from `logOpen`, whose lifetime is
the dialog's. After a clean stop the launcher renders no log and the `xl` shell stays.

**Change.** The width follows the rendered log: `logShown` is true exactly when a
`PreviewRunLog` is in the rendered branch, and `size={logShown ? "xl" : "sm"}`.
`logOpen` resets when the run ends with no notice (clean close, or no preview), so the
next Start does not auto-expand the log; it is kept across active → failed and
active → cleanup-failed so the notice's log stays open (existing tests).

## Order

P3 (frontend), P2 (frontend), P1 (contract, dispatcher, frontend). Each lands with its
tests; the Playwright preview flow gains the two-tab, no-remount and width assertions.

## Verification

- Rust: `g5-contracts` line test for `closed`/`superseded`; `dispatch_modules`
  hosted-terminal test asserts the superseded stream's last frame is
  `closed`/`superseded`, the new viewer keeps streaming and the session survives.
- Vitest: socket hook (superseded is final; Take over reattaches at the offset on the
  same surface; a held attach opens on flip), panel viewer (notice and Take over;
  pending words), `PreviewRunLog` (one panel instance across a refused-then-hosted
  sequence; no attach before the run started; waiting words in the header),
  `PreviewControl` (xl while a log is rendered, sm after a clean stop, xl kept for a
  failed run's log, Log collapsed on the next start).
- Playwright `workspace-preview.spec.ts`: two pages on one log (second follows, first
  shows the notice, Take over swaps roles, no reconnect loop); Log opened right after
  Start keeps one viewer element; dialog returns to `max-w-md` after Stop.
