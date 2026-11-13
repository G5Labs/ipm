# Chat bubble stack

Source: [chat intent](../../chat_intent/chat-bubble-stack/request.md) (2026-10-29) and the
reviewed [mock](../../../docs/mocks/chat-bubble-stack/README.md). Implementation authorized
2026-10-29 after the mock's questions were answered.

Every chat of a workspace that needs the person gets a round bubble above the bottom-right
Chat corner button. The face shows what the chat is doing (working, waiting for your answer,
idle with a new result); pressing it opens that chat in the floating panel. Two per-browser
settings choose the style (Off / Still / Animated / 3D) and the stack layout (Column /
Collapsed).

## Ontology changes

| Block | Change | Why |
|---|---|---|
| [g5.workspace.ui.chat-bubble-stack](../../ontology/features/ui.g5.md#g5.workspace.ui.chat-bubble-stack) (new D, UI) | The stack: which chats get a bubble, faces and marks, order and cap, open behaviour, following the button, the two settings with their values, defaults and reduced-motion fallback. | Owner request; the values, cap, defaults and layout setting are the owner's answers to the mock's questions. |
| [g5.chat.activity.awaiting-input](../../ontology/features/chat.g5.md#g5.chat.activity.awaiting-input) (new D, chat) | A chat's activity also says whether its turn waits for the person (pending question or permission request on the running attempt, or a deferred question holding the turn), and the list is hinted when a request opens or settles. | The bubble needs the waiting state for every chat; today only the open chat's timeline carries it. |
| [g5.workspace.ui.chat-corner-button](../../ontology/features/ui.g5.md#g5.workspace.ui.chat-corner-button) (amended) | One sentence: the stack sits above the button and moves with it. | Keeps the button's commitment the single place that says what the corner shows. |

## Governing ontology

- [g5.chat.activity.turns](../../ontology/features/chat.g5.md#g5.chat.activity.turns): the
  active turn and finished results are unchanged; waiting is an extra fact about the live
  task, derived from the same records. A turn held for any reason other than a deferred
  question is still "not active" with a finished result, so it reads as idle.
- [g5.chat.inspection](../../ontology/features/chat.g5.md#g5.chat.inspection): the idle
  bubble is the uninspected result; it leaves when the person's acknowledgement lands,
  which the opened chat already records by display. A bubble press acknowledges nothing by
  itself.
- [g5.chat.activity.refresh](../../ontology/features/chat.g5.md#g5.chat.activity.refresh):
  the list stays authoritative; the new hint on request open/settle is advisory, and the
  15-second refresh while a turn is active covers a lost hint.
- [g5.workspace.ui.chat-corner-button](../../ontology/features/ui.g5.md#g5.workspace.ui.chat-corner-button):
  visibility, gate, drag and the right-edge open are reused as they are; a bubble is a
  second way to trigger the same open with a chosen session.
- [g5.workbench.header-styles](../../ontology/features/ui.g5.md#g5.workbench.header-styles):
  the settings follow the same per-browser store, normalization and settings-page form.

## High-level design

### Server: `awaitingInput` on session activity

- `SessionActivity` (`backend/g5-api-core/src/services/agents/chat_activity.rs`) gains
  `awaiting_input: bool`, one more select expression of `ACTIVITY_COLUMNS`:
  the session has an agent task that is either `running` with a `provider_requests` row in
  status `pending` (question or permission), or `waiting` with `hold_reason =
  'awaiting_answer'`. A session has at most one live agent task, so an `EXISTS` over the
  session's tasks names it without another lateral join. `reply_pending` is not waiting:
  the person has answered and delivery is in flight. A timed-out request is `invalidated`
  and the attempt settles right after, so it reads as a finished result.
- Derived, not stored: the facts are `tasks` and `provider_requests`, both already owned by
  the task lifecycle; nothing new is written.
- Hint: `TaskService::apply` announces chat activity after a journal event of kind
  `provider_request.*` even though the task stays `running`. Owner replies already announce
  when handled locally; a deferred answer re-queues the task, which enqueue announces.
- The list and detail DTOs serialize the new field through the existing `activity` object;
  the frontend type marks it optional for older servers.

### Frontend

- **Derivation.** `lib/chatSessionActivity.ts` gains `chatBubbleState(activity)`:
  `waiting` when `awaitingInput`, else `working` while a turn is active, else `idle` while
  the result is unseen, else `null` (no bubble). The picker's spinner/dot indicator keeps
  `chatActivityState` unchanged.
- **Settings.** `UserSettings.chatBubbleStyle` (`off | still | animated | 3d`, default
  `still`) and `UserSettings.chatBubbleStack` (`column | collapsed`, default `column`), with
  option lists and guards in `lib/types.ts`, defaults in `lib/defaults.ts`, normalization in
  `hooks/useUserSettings.ts`, and two selects under Preferences · Layout in
  `components/settings/WorkspaceSettingsContent.tsx`.
- **Corner.** `components/ui/CornerButton.tsx` renders its portal as a fixed column
  (`stack` above the button) positioned by the same remembered offset, so the stack follows a
  drag; the button keeps its own classes, handlers and tests. Nothing else of the primitive
  changes.
- **Stack.** New `components/chat/ChatBubbleStack.tsx`: takes the visible sessions, the two
  settings and `onOpen(sessionId)`; sorts waiting, working, idle (stable within a group,
  so the server's activity order holds), caps at five with a "+N" control that calls
  `onOpen(null)`; renders one `button` per chat with the face, ring, dot, tooltip and
  accessible name. Faces: Still and Animated are `<img>` layers from
  `src/assets/chat-bubbles/` (96/192 px, 1x/2x); Animated alternates the second frame with a
  stepped CSS animation that `prefers-reduced-motion` disables. 3D lazy-loads the
  `robot-heads` package (MIT, React ≥18, canvas only) and renders `RobotHead` with
  `working` / `happy` / `sleeping`, `paused` under reduced motion; the chunk is loaded only
  when the style is `3d`. Collapsed shows the first bubble with a count badge and unfolds
  on hover or focus within.
- **Page.** `WorkspacePage.tsx` passes the stack into the corner button whenever the button
  renders and the style is not `off`; `onOpen(id)` selects the session without revealing
  the center (`handleViewSession(id, undefined, false, false)`) and then runs the corner
  open, so the right-edge panel shows that chat with the composer focused. The disabled
  button (Base, readers, read-only) renders no stack: those workspaces have no live chats
  for this person to answer, and the gate's reason stays the only affordance.

### Capacity and limits

- One `EXISTS` per listed session over indexed task rows; the list query stays one statement.
- The stack reads the page the picker already loads (the 50 most active chats, active turns
  first). A chat waiting on a running attempt is active and therefore listed; a held deferred
  question or an old uninspected result beyond that page has no bubble. Accepted as the
  picker's existing limit rather than adding a server-side attention query; revisit if
  workspaces with more than fifty chats need every waiting chat surfaced (CodeRabbit, PR #953).
- Images: five PNG pairs, about 200 kB total, hashed assets in the bundle. `robot-heads` is
  a separate lazy chunk (~90 kB) fetched only for the 3D style.

## Settled questions

- Ongoing = active turn, waiting for input, or uninspected result (agent interpretation,
  mocked and not contradicted by the owner's "waiting on a command" reading).
- Waiting includes permission prompts (owner, 2026-10-29).
- Names Off / Still / Animated / 3D; cap 5; layout setting with Column default; per-browser
  store (owner, 2026-10-29).
- Default style `3d` (owner, 2026-10-29 second round; `still` was the agent's initial
  pick).

## Follow-up (2026-10-29): Minimize and Close on the floating panel

Owner request after PR #953 was published. The floating Chat panel gains a Minimize
control beside Close. Minimize **pins** the active chat: it keeps a bubble even while it
needs nobody (idle face, no dot). Close **dismisses** it: no bubble until its activity
changes, which also drops a pin. The memory is per browser and per workspace
(`g5:chat-bubbles:<workspaceId>`): a pinned-id list and a dismissed map from session id to
the activity signature at dismissal (active turn start, latest result task and attempt,
awaiting input). A bubble shows when the chat needs the person and its signature differs
from the dismissed one, or when it is pinned. `lib/chatBubbleMemory.ts` owns the pure rules
and storage, `hooks/useChatBubbleMemory.ts` the page's state; `chatBubbles()` takes the
memory, so order and cap are unchanged. The generic `FloatingPanel` only gains an optional
`onMinimize`; the chat panel is its one consumer. Tabs do not sync this memory: a pin or
dismissal in one tab shows in another after its next load, accepted as the same scope the
other per-browser appearance state has.

## Follow-up (2026-10-29): stack in the center case, hover label

Owner request after the second round. While Chat is the center surface the corner button
has no job, but the stack stays: `CornerStack` (in `CornerButton.tsx`) portals the same
column at the button's remembered spot, reading the offset the movable button stored, and
`WorkspacePage` renders it under the button's mobile, overlay, floating-panel and gate
conditions with `centerChatActive` inverted. A press there selects the chat for the center
(`handleViewSession`), the overflow count only reveals Chat. The stack sits where the button
would be rather than one button-height above it, so the corner has no empty gap (agent
choice). The hover label replaces the native tooltip: a chip beside the bubble with the
title and state, shown on hover or keyboard focus through CSS, while the accessible name
keeps both for assistive technology.

## Follow-up (2026-11-03): drag from any bubble, a bubble for every chat

Owner report: in the full-page (center) chat the stack could not be moved, and only the
button was a drag handle. `useCornerDrag` (in `CornerButton.tsx`) now owns the remembered
offset and the pointer handlers; `CornerButton` applies them to the button and to the stack
column, `CornerStack` to the lone column, so a press on any bubble drags the pair to one
shared spot. The drag's own click is stopped in the capture phase on the handle so it never
reaches the pressed bubble's open or the button's open. Pointer capture is taken at the
pointerdown, on the pressed element itself (`event.target`) rather than the stack: a browser
retargets the pointerup and the click to the capturing element, so capturing the stack kept
a short press's click from the bubble and no chat opened (owner report, 2026-11-09); and
capturing only after the drag threshold, tried first, let a brisk drag leave the 40px handle
before any move reached it, which the alignment e2e flow caught (the button no longer moved).
jsdom has no pointer capture, so the unit test stubs `setPointerCapture` to pin which element
holds it; both behaviors were confirmed in headless Chromium against the real component.

Owner request in the same PR: "always have one bubble for each chat (sorted by most
recent), even when idle/closed", and remove the Minimize control of the second-round
follow-up "since there will always be avatars for every single chat". `chatBubbleState` now
returns idle instead of nothing; `chatBubbles` maps the list in its order, which the server
already sorts by most recent activity (`g5.chat.activity.order`), so no client sort remains
and the waiting/working/idle grouping is gone. The Minimize control, the pin/dismiss memory
(`lib/chatBubbleMemory.ts`, `hooks/useChatBubbleMemory.ts`, the `g5:chat-bubbles:<ws>` key)
and the `memory` prop were removed; Close only closes the panel. Kept as agent choices:
the cap of five with the count (a page of 50 chats cannot stack), the idle face with the
dot for an uninspected result, and the collapsed layout unchanged. A browser that still
holds an old `g5:chat-bubbles:<ws>` entry simply ignores it.

## Follow-up (2026-11-09): a count setting instead of the layout, an unfolding count

Owner request in the same PR: remove the "Chat bubble stack" (column/collapsed) setting,
add a setting for how many avatars show (default 5), and make the `+N` count clickable so
the column expands to every chat. `UserSettings.chatBubbleLimit` replaces `chatBubbleStack`:
a whole number from 1 to 50 (the list's page size; `isChatBubbleLimit`), default 5, with
the settings page offering a number input that commits only in-range whole numbers. A stored
`chatBubbleStack` is ignored like any unknown key. `ChatBubbleStack` takes `limit`; the
count past the farthest bubble toggles a local `expanded` state: unfolded, every chat shows
and the same control (a chevron, "Show the N most recent chats only") folds them back. The
state is per mount, so the stack starts folded again after the floating panel closes
(agent choice: the fold is a glance, not a preference). An unfolded column of many chats
may reach past the viewport top; accepted for now since the person unfolds it on purpose
and can fold it back, and a scrolling column would clip the hover labels beside it. The
collapsed layout's CSS-only unfold, count badge and `data-layout` attribute are gone, and
the count no longer opens Chat, so the page's open handlers take a session id only.

## Implementation evidence (2026-10-29)

- Server: `SessionActivity.awaiting_input` and the `EXISTS` expression in
  `backend/g5-api-core/src/services/agents/chat_activity.rs`; the hint after
  `provider_request.*` journal events in `TaskService::apply` and on every claimed reply in
  `reply_to_native_request` (`task_lifecycle.rs`). `api_chat_activity.rs::
  a_pending_request_marks_the_turn_awaiting_input` covers a pending permission on a running
  attempt (true, with the hint), the owner's reply (false, with the hint) and a deferred
  question holding the turn (true with no active turn). The whole `api_chat_activity` module
  passed locally against a pgvector Postgres.
- Client: `components/chat/ChatBubbleStack.tsx` with `ChatBubbleStack.test.tsx` (order and
  faces, open, off/no-chat, cap and overflow, animated frames and reduced motion, collapsed,
  3D states and pose); `lib/chatSessionActivity.ts::chatBubbleState` with
  `chatSessionActivity.test.ts`; settings in `lib/types.ts`, `lib/defaults.ts`,
  `hooks/useUserSettings.ts` (`useUserSettings.test.ts`) and
  `components/settings/WorkspaceSettingsContent.tsx` (`WorkspaceSettingsContent.test.tsx`);
  `components/ui/CornerButton.tsx` `stack` prop (`CornerButton.test.tsx`, "carries a stack
  above the button that follows a drag"); `pages/WorkspacePage.tsx` wiring
  (`WorkspacePageV2.test.tsx`, "chat bubble stack": stacked above the button and opened at
  the right edge, nothing while off, nothing above a disabled button). The full frontend
  unit suite passed; `vite build` emits the twelve face assets and `robot-heads` as its own
  chunk.
- Not exercised: a browser flow with a live agent asking a question (the e2e stack); the
  3D canvas renders only in a real browser.
