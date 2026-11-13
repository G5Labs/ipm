# Shell corner buttons

Source: [chat intent](../../chat_intent/shell-corner-buttons/request.md) (2026-10-15).

Two bounded UI changes in one PR: the floating bug-report trigger appears only while the
chrome trigger is absent or covered, and a mirrored bottom-right button opens Chat beside the
content on workspace routes.

## Ontology changes

| Block | Change | Why |
|---|---|---|
| `g5.quality.platform-reports.trigger` (new D, platform reports) | One controller; the chrome carries the steady trigger; the floating bottom-left trigger shows only on a page without a chrome trigger or while an overlay at the modal tier or above (not the report drawer) is open. | The user chose this among the alternatives; the duplicate existed only for the covered case. |
| `g5.workspace.ui.chat-corner-button` (new D, workspaces) | Bottom-right secondary Chat button on desktop workspace routes; hidden while its panel or a covering overlay is open; opens Chat on the right with the last session and composer focus; disabled with the reason on Base and read-only workspaces; open-only, no shortcut, no Escape. | User instructions, with focus/shortcut/Escape as agent picks. |
| `g5.workspace.base-ui.chat` (amended) | The disabled corner button is the one Chat affordance Base shows; it opens nothing. | "Base ... greyed-out button with a read only tooltip." |

## Governing ontology

After the ontology rewrite, the retained corner-control commitments live in [UI](../../ontology/features/ui.g5.md). Older governing references below name the accepted main baseline.

- [g5.quality.platform-reports](https://github.com/G5Labs/G5-Lean/blob/970b28f6d0f4ac3bd745cd190f94d85b14a57ce5/.g5/ontology/features/platform_reports.g5.md): the report
  flow, context and intake are unchanged; only trigger visibility moves.
- [g5.workbench.shell-chrome](https://github.com/G5Labs/G5-Lean/blob/970b28f6d0f4ac3bd745cd190f94d85b14a57ce5/.g5/ontology/features/workbench.g5.md#the-alignment-shell-has-no-title-bar):
  the tab strip hosts Report a problem and, while the center strip is hidden, the ribbon
  hosts the Chat toggle; both stay. The corner button is additive.
- [g5.workspace.ui.center-tabs](https://github.com/G5Labs/G5-Lean/blob/970b28f6d0f4ac3bd745cd190f94d85b14a57ce5/.g5/ontology/features/workspaces.g5.md#center-tabs-are-hidden-until-enabled):
  the title-bar toggle and the center Chat tab remain the center entry points; the corner
  button is the third, and the only one that leaves the center to the content.
- [g5.workspace.base-ui.chat](../../ontology/features/ui.g5.md#g5.workspace.base-ui.chat)
  and [g5.workspace.ui.unavailable](https://github.com/G5Labs/G5-Lean/blob/970b28f6d0f4ac3bd745cd190f94d85b14a57ce5/.g5/ontology/features/workspaces.g5.md#keep-a-waiting-action-visible-with-its-reason-and-start):
  Base shows no Chat surface; a never-available action is normally omitted. The user asked
  for a visible disabled button instead, recorded as the exception on the Base block.
- [g5.workspace.ui.gates](https://github.com/G5Labs/G5-Lean/blob/970b28f6d0f4ac3bd745cd190f94d85b14a57ce5/.g5/ontology/features/workspaces.g5.md#offer-an-action-by-what-its-operation-needs):
  "read-only" for the disabled state is the durable gate (`workspaceWritable`), so the
  tooltip reuses the gate's reason text.

## High-level design

### Report trigger visibility

- **Dialog-stack signal.** `useDialogFocus.ts` already keeps the registry of mounted
  overlays with their z-tier priority. It gains a subscription (`useSyncExternalStore`)
  that answers "is an overlay with priority in [modal tier 50, report-drawer tier 130)
  mounted". The settings flyout (40) does not count; the report drawer (130) does not count,
  so the floating trigger stays mounted while the drawer sits over a modal and focus
  restores to it on close.
- **Chrome registration.** The `topbar` placement of `PlatformIssueReportTrigger` registers
  with the controller on mount and unregisters on unmount. The provider renders the floating
  `shell` placement only while `enabled && showShellTrigger && (chromeTriggers === 0 ||
  overlayCovering)`. This keeps the floating trigger on settings, account and CLI pages,
  which mount no chrome trigger, without a route list.
- **Unchanged:** shortcut, screenshot exclusion, context, drawer, the `topbar` placements in
  `TopBar`, `OrganizationHomeTopBar` and `ShellControls`, and `App.tsx`'s props.

### Chat corner button

- **Primitive.** `components/ui/CornerButton.tsx`: a body-portaled
  `fixed bottom-4 {left|right}-4 z-[120]` round 40px secondary button with the report
  trigger's tokens, `data-screenshot-exclude`, an accessible name, and a disabled form
  (`aria-disabled`, reduced opacity, reason in `title`, no click). The report trigger's
  `shell` placement moves onto it so the two corners stay identical.
- **Owner.** `WorkspacePage.tsx` owns the chat surface state and renders the button in the
  desktop branch once the workspace detail is loaded. Shown when: desktop breakpoint, Chat is
  offered for this workspace (same gate as the title-bar toggle), the panel it opens is not
  open, and no covering overlay is open (the same dialog-stack signal). Disabled when the
  workspace is Base or `workspaceWritable` is false; the tooltip is the durable gate's reason
  (`liveGateReason`) so wording matches the other disabled controls.
- **Open.** Click sets the floating chat panel open
  (`rightPanelDetached`), which already removes Chat from the center and shows the selected
  content there. `FloatingPanel` gains `initialPlacement: "right-edge"`: x at the right
  edge, y below the shell chrome, height to the bottom, default width 380, with its existing
  drag, resize, Dock and Close. The session is the page's `activeChatSessionId` (last
  active; the no-session landing otherwise). `ChatPanel` focuses the composer on this open
  when the composer is enabled. Dock returns Chat to the center; Close closes it; the button
  reappears after either, derived from state.
- **Layouts.** The same under `simple`, `advanced`, `both` and `alignment`; the
  alignment ribbon's Chat toggle and the center tab are untouched. Mobile renders nothing.

## Settled questions

- **Surface**: no docked right chat column has existed since `07a865f24` (2026-09-22); Chat is
  in the center or in the Pop out floating panel. Options were (a) that floating panel
  anchored to the right edge, reusing Pop out, or (b) a restored docked, resizable right
  column with its own width persistence and tab-strip interaction. The user chose (a) on
  2026-10-15 after the [mock](https://claude.ai/artifact/8TByJV2WDYMvTMq8RMckUP); it reuses
  one surface and adds no layout state.
- **Read-only**: keep the greyed-out button in read-only workspaces even though the title bar
  still opens Chat for reading there (2026-10-15).

## Decision notes

- The floating report trigger sits at z-120, under the full-screen artifact viewer (9999);
  there only the shortcut reaches the report. Pre-existing; recorded on the block.
- No keyboard shortcut for the corner Chat button: unlike the report trigger it is never
  hidden while it could act. No Escape: the floating chat panel is not a dialog today.
- Floating-panel size is not persisted across opens for Pop out. The right-edge Chat
  remembers its width per browser since the resizing follow-up below.
- Stacked on PR #559 (tab return): that PR persists whether Chat was in the center per
  tab. A right-edge Chat is not in the center, so returning to the tab shows the content
  with Chat closed and the corner button back; the floating placement is not remembered.
  Accepted; nothing in #559 needed changing.
- The disabled button reads the durable gate only: a stopped VM still opens Chat, whose
  composer carries the Start hint, so the button stays enabled there.

- Codex review (2026-10-15): two full-screen dialogs bypassed the shared dialog lifecycle, so
  the stack would not have seen them: the chat screenshot lightbox and the spec record delete
  dialog. Both now use `useDialogFocus` at their z tier, which also gives them Escape, the focus
  trap and focus restore.
- Codex conformance review (2026-10-15): under the alignment layout the panel measured only
  the tab bar and covered the ribbon's controls; the boundary now includes the workbench
  header and the workspace toolbar (a second pass: the `simple` layout hides the ReposBar but
  keeps the toolbar's owner and VM controls). The report drawer (z-130) sat under floating panels (z-200+); it
  now renders at z-1000 while keeping keyboard tier 130. The e2e flow asserted the chrome
  report trigger unconditionally although the e2e stack does not enable reporting; it now
  asserts by the advertised feature.

## Resizing follow-up (2026-10-15)

The merged panel resized only from its bottom-right corner, which for the right-edge Chat
sits in the viewport corner and can only grow the panel off-screen. `FloatingPanel` now
carries a handle on every edge and corner; a drag moves only the dragged edges, within the
existing width bounds (280 to 800) and the viewport, so the right-edge Chat grows from its left
edge with the right edge anchored and from its top edge with the bottom anchored. The
bottom-right handle keeps the visible grip. The owner seeds the right-edge panel with the
width it last had in this browser (`g5:chat-corner-width`, written when a resize drag ends)
and leaves Pop out at the primitive's default, so the two placements stay distinct. Height is
not remembered: the right-edge panel derives it from the chrome bottom on each open. Drag and
top-edge resize both keep the title bar inside the viewport so Dock and Close stay reachable.
The window-resize case is unchanged: an open panel does not follow a viewport change.

## Movable button follow-up (2026-10-20)

The merged button sits at a fixed bottom-right spot and can cover whatever the content
places there, and it also showed while Chat already held the center, where it had no job
(the center chat's own Pop out exists for that direction). Three changes:

- `CornerButton` takes a `movable` option with a storage key. A press that travels under
  4px is still a click; beyond it the press becomes a drag that moves the button, swallows
  the click the browser dispatches afterwards, and stores the offset from the home corner
  (`g5:chat-corner-button-offset`, `{x, y}` in CSS pixels). Offsets rather than absolute
  coordinates keep the button against the same edges across viewport sizes; the stored or
  dragged offset is clamped to the viewport on read, on drag and on window resize. A
  disabled button moves too, since it hides content all the same. The report trigger on
  the left keeps its fixed corner; it only shows while the chrome is absent or covered.
- The owner hides the button while `centerChatActive`, in addition to the floating panel,
  covering overlay and mobile cases. Under the hidden tab strip the title-bar Chat toggle
  leaves the center; under the strip a content tab does. The center no-session landing had
  no Pop out (only the session header and the panel-layout header carry one), which would
  have left that landing two steps from the floating panel; it now carries the same Pop out
  control in its top-right corner.
- Audit of the floating ↔ center switch: Dock (floating title bar) and Pop out (center
  chat header) already move the same session both ways, and a running agent keeps
  streaming because messages and session state live in the page. The unsent composer text
  did not survive: `ChatPanel` is mounted in a different parent on each side, so the
  switch remounts it and its local `inputValue` reset. The page now keeps one
  `ChatComposerDraft` object per workspace and hands it to the panel, which reads the text
  on mount and writes it on each change; the applied composer-seed token travels with it
  so a remount does not re-apply a seed over later typing. Pending references and
  attachments are deliberately not carried: the panel clears them on every session-change
  effect run, which includes mount, and carrying them would need that effect to tell a
  remount from a session change. Pop out from the center still opens centered, not at the
  right edge; the two placements stay distinct as before. The draft also happens to
  survive a switch between workspace tabs, since the page instance keeps one draft per
  workspace id; that is untested and not promised (leaving to Home drops it). The
  button moves by pointer only; there is no keyboard move and no reset short of clearing
  the stored offset, accepted for this scope.

## Implementation status (2026-10-15)

Implemented as designed on top of PR #559. Unit, page and App suites pass locally; the
ChatPanel account-field case is a known flake unrelated to this change. The e2e flow is
written and runs only in CI. Evidence is recorded on the two ontology blocks.

## Testing

- `useDialogFocus`: subscription reports a modal, ignores the settings flyout tier and the
  report-drawer tier, clears on unmount.
- `PlatformIssueReportButton.test.tsx`: floating trigger absent while a chrome trigger is
  mounted; present while a `Modal` is open and gone when it closes; present on a page with no
  chrome trigger; drawer still opens from the floating trigger over a modal and restores
  focus to it.
- `App.test.tsx`: the workspace-route case becomes "no floating trigger until a modal
  opens"; settings route keeps the floating trigger; the feature-off case is unchanged.
- `WorkspacePageV2.test.tsx` and `WorkspacePageAlignment.test.tsx`: button visible on
  desktop; hidden on mobile; hidden while the floating chat is open and back after Dock and
  Close; hidden while a modal is open; disabled with the Base reason on Base; disabled with
  the read-only reason for a reader and a read-only policy; click opens the floating chat
  at the right edge with the last active session and focus in the composer; center shows
  the content tab meanwhile; alignment layout behaves the same.
- `frontend/e2e/flows/alignment-shell.spec.ts`: one flow for the corner button (open, dock),
  runs in CI only (no local e2e stack).
- 2026-10-20: `CornerButton.test.tsx` covers the fixed corner when not movable, the home
  offset, a click without movement, a drag that moves, stores and swallows its click, a
  press inside the threshold, viewport clamping on drag, on a stored spot and on resize, an
  unreadable stored spot, moving while disabled and a cancelled press. Page tests add:
  absent while Chat is the center surface and back once the content takes it (both
  layouts), the remembered spot, and the draft surviving Pop out, Dock and the corner open;
  existing cases leave the center Chat first, and the no-session center landing pops out.
  The e2e flow drags the button before opening and expects it gone after Dock.
