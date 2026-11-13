# Chat activity, inspection, header rename and per-chat recall

Issue: [G5Labs/G5-Lean#823](https://github.com/G5Labs/G5-Lean/issues/823).
Authorized branch: `issue-823-improve-workspace-chat-management-activity`.

Governing blocks, all in [chat](../../ontology/features/chat.g5.md):
[header title](../../ontology/features/chat.g5.md#g5.chat.ui.header-title),
[picker](../../ontology/features/chat.g5.md#g5.chat.ui.picker),
[activity](../../ontology/features/chat.g5.md#g5.chat.activity),
[turns](../../ontology/features/chat.g5.md#g5.chat.activity.turns),
[order](../../ontology/features/chat.g5.md#g5.chat.activity.order),
[inspection](../../ontology/features/chat.g5.md#g5.chat.inspection),
[acknowledgement](../../ontology/features/chat.g5.md#g5.chat.inspection.acknowledge),
[refresh](../../ontology/features/chat.g5.md#g5.chat.activity.refresh),
[prompt history](../../ontology/features/chat.g5.md#g5.chat.ui.prompt-history) and its
[store](../../ontology/features/chat.g5.md#g5.chat.ui.prompt-history.store).
The scope amendment adds
[code-block Copy](../../ontology/features/documents.g5.md#g5.documents.markdown.code-copy). They specialize
[ChatService/TaskService ownership](../../ontology/features/chat.g5.md#g5.chat.ownership),
[settlement](../../ontology/features/chat.g5.md#g5.chat.settlement) and
[read authority](../../ontology/features/chat.g5.md#g5.chat.authority).

## Owners and interactions

**Activity is derived, not stored.** TaskService already owns the facts: `tasks`
(status, `created_at`, one live agent task per session by
`tasks_one_active_agent_per_session_idx`) and `task_attempts` (`ended_at` set only at
settlement). ChatService's list query projects them per session:

- `activeTurnStartedAt`: `created_at` of the session's running agent task, or its
  queued one while the workspace's `workspace_vms` row is starting or running or
  carries a pending start intent.
- `lastFinishedAt` and `latestResult {taskId, attempt, hasOutput}`: the latest settled
  attempt, found on the newest task that has a result. A terminal task with no attempt
  (admission refusal, cancel before dispatch) contributes its `ended_at` with a null
  `attempt`. `hasOutput` is whether that attempt has an assistant message; it is
  always false for a null attempt.
- `unseenResult`: `latestResult` ended after the caller's watermark and the chat's floor.

The same projection is added to `GET /sessions/:id`. `ORDER BY` follows
[order](../../ontology/features/chat.g5.md#g5.chat.activity.order):
active first, `activeTurnStartedAt DESC`, then `COALESCE(lastFinishedAt, created_at) DESC`,
then `id DESC`, then the existing `LIMIT/OFFSET`. Session `updated_at` stays as it is
for its other readers, and nothing orders by it any more.

Rejected alternatives, and why:
- Ordering by `updated_at`: renames, settings and provider bookkeeping bump it.
- A denormalized `last_turn_finished_at` column: it adds a second writer to every
  settlement, hold and cancel path for no outcome derivation lacks.
- Treating attempts as turns: retries and continuations would show as separate turns.

The sequential-task invariant in [turns](../../ontology/features/chat.g5.md#g5.chat.activity.turns)
keeps each chat's lookup to a few index probes, so ordering a whole workspace is linear
in its chat count. Migration indexes: `chat_sessions (workspace_id)`, which was lost
with the `status` column in 008, and `tasks (chat_session_id, created_at DESC) WHERE
kind = 'agent'`. Expected envelope: workspaces with up to a few thousand chats. Beyond
that, a rebuildable projection would be the next step; it is not needed now.

**Inspection is the only new durable state.** It lives in the new table
`chat_session_inspections (org_id, workspace_id, session_id, user_id,
inspected_through, updated_at)`:
- primary key `(session_id, user_id)`;
- FKs to the chat session and the user, both `ON DELETE CASCADE`;
- RLS: `tenant_isolation`, plus restrictive `formal_review_private_*` policies keyed by
  `workspace_id`, as in 108.

ChatService owns the table. `chat_sessions.inspection_floor_at timestamptz NOT NULL DEFAULT now()`
is added as DDL. Postgres evaluates `now()` once for existing rows, which gives them
the rollout time; each new row gets its creation time. With the floor, no per-user
backfill is needed, and existing chats do not all turn unread at rollout.

Operation: `PUT /api/v1/sessions/:id/inspection {taskId, attempt?}`.
1. Authorize with session read scope (`scoped_db_for_chat_session_view`), so readers
   can record their own state. This does not use `workspaces:write`, which would
   restrict it to the owner, as notification mark-read is.
2. Resolve the result. It must be a settled attempt, or the end of an attempt-less
   terminal task, of an agent task in this session; otherwise return 409.
3. Upsert with `inspected_through = GREATEST(existing, result end time)`.
4. Return the session's activity.
5. Publish `chat.activity` to the caller's own connections on the workspace channel
   (`publish_to_user`).

The client names the result it saw, and the server derives the time. That keeps
navigation timing out of the record, and a concurrent newer result can never be
swallowed by it.

**Acknowledgement trigger (ChatPanel).** ChatPanel acknowledges the active session's
`latestResult` when `unseenResult` is set and all of these hold:
- `document.visibilityState === "visible"` and `document.hasFocus()`;
- the owner passes a new surface-visible prop. The hidden center tab keeps ChatPanel
  mounted, so mounting alone proves nothing;
- the transcript is at the bottom;
- either `hasOutput` is false, or the loaded messages hold that `(taskId, attempt)`'s
  assistant message in a terminal status.

The trigger re-evaluates on data change, scroll, focus and `visibilitychange`, and
sends at most one acknowledgement per result per mount. To support the last
condition, `ChatMessageDTO` gains the existing `task_id`/`attempt` columns. When the
list reports a result the cached transcript lacks, the transcript is invalidated. A
stale cache therefore never satisfies the condition, and never blocks it forever.

**Live refresh.** Today `task.status` reaches only `task:` channels, and
`publish_task_status` has no callers. Background chats therefore get no start or
completion event. The fix is one helper that publishes `chat.activity
{workspaceId, sessionId?}` after commit. It is called from the agent-task transitions
that change activity:
- enqueue that creates a task;
- journal settlement application (success, failure, hold);
- cancel, stop, pause and continue routes;
- native-answer release;
- host control's VM ready (which releases turns held for a start) and VM
  exit/failure events, after their commit, with no `sessionId`.

The ownership-transfer hold publishes nothing: that commit can revoke the previous
owner's read access, and `api_ws_revocation` requires a revoked socket to receive
no application data. Those turns were active, so readers' lists re-read on the
15 s refresh.

These all run in the HTTP-serving API process. That process also runs the dispatcher
and journal reconciler, and production has one API replica, so in-process fan-out
reaches browsers. Hints carry no state; the browser refetches
`["chat-sessions", workspaceId]` (debounced) and the active `["chat-session", id]`.

The browser fallback rule is to refetch on reconnect, on focus/visibility regained, and every
15 s while a listed chat is active. A missed call site or dropped hint therefore costs
latency, not correctness. The existing `task.status` handler's blanket chat-cache
invalidation is unchanged for subscribed task channels.

**Header and picker (ChatPanel `sessionHistoryPicker`).** The active-session title
trigger becomes `min-w-0 flex-1`, with a truncating label and a `title` tooltip,
followed by a pencil button. The pencil appears only with `onRenameSession`, which is
owner-only as today. The header editor:
- Enter saves and Escape cancels;
- the input is disabled while saving;
- a failure leaves the editor open with an inline `role="alert"` message, fixing
  today's unhandled rejection.

The no-session "History" and "View all" triggers keep their own widths. Picker rows
lose the pencil and the inline editor, and gain the indicator and the time line from
[picker](../../ontology/features/chat.g5.md#g5.chat.ui.picker). The center no-chat
view's "Recent conversations" strip takes the first three chats in server order instead
of re-sorting by `updatedAt`, and shows the picker's activity time (`Active now` or
`Last active …`) instead of `updatedAt`, so a rename alone neither moves it nor changes
its time ([owner's choice](../../chat_intent/issue-823/recent-conversations.md)). Its
layout is unchanged.

**Per-chat prompt history (frontend only).**
- Key: `g5:chat-prompt-history:<user>:<workspace>:<session>`, computed in ChatPanel
  from the owner-supplied person/workspace scope and the active session. The existing
  cursor reset on key change then ends browsing on a chat switch.
- If the composer still holds the cursor's unedited recalled prompt at the switch,
  it is replaced by the cursor's saved (empty) draft.
- The owner's send handler resolves with the accepting session id. For a first send,
  that is the session it created, even when the person has already switched away.
  ChatPanel records against that id, or against the session captured at send time.
  The existing ungated record-after-accept keeps delayed sends on their originating
  chat.
- The value becomes `{usedAt, entries}`. Recording enforces the browser-wide
  200-chat and 1,000,000-character budget by evicting the least recently recorded
  buffers.
- Legacy two-segment workspace keys are removed on first use after load.
- If `localStorage` throws, a module-level map serves the page.
- Sign-out clearing keeps the shared prefix.

Same-browser reload and VM restart durability come from `localStorage` itself; no VM
mechanism is added.

## Code block Copy

The owner's 2026-10-26 issue comment extends the scope with
[code-block Copy](../../ontology/features/documents.g5.md#g5.documents.markdown.code-copy).
This is separate from the #810 device-code copy, which stays out of scope.

- **One control, one clipboard helper.** `shared/CodeCopyButton` takes the exact text
  and owns the feedback. `CodexDeviceAuth`'s `writeClipboard` moves to `lib/clipboard`
  and both use it. It resolves false, and never throws, when the Clipboard API is
  missing or the write is refused.
- **Feedback.** A live region (`role="status"`) is always present, so outcomes are
  announced. It shows "Copied" for 1.5 s, or "Couldn't copy" for 5 s, with a manual-copy
  hint as its tooltip. When presses overlap, only the latest press's outcome shows. The
  button keeps the accessible name "Copy code".
- **Placement.** `MarkdownRenderer`'s `CodeBlock` always renders its header now: the
  language label, which truncates, and Copy, which does not shrink. Every
  `MarkdownRenderer` surface gets it, including restricted content, which already
  renders every fence through `CodeBlock`. The FILES Markdown preview is not built on
  `MarkdownRenderer` (it renders the GitHub dialect through react-markdown). It
  overlays the same control at a code block's top right, as GitHub does. SpecEditor's
  read-only code blocks swap their silent copy icon for the control. The structured
  code editor's copy of the live draft is an editing tool and is left unchanged.
- **Text.** `MarkdownRenderer` copies the parsed fence body. The preview copies the
  `pre` text minus react-markdown's trailing newline. Neither copies the highlighted
  HTML.

## Assumptions to keep true

- Only agent tasks with a `chat_session_id` form chat turns. Command tasks and
  admission notices with NULL task/attempt are not results.
- Implementation found that a VM stop or release leaves queued agent tasks queued
  (`settle_agents_after_release` cancels only previews). Queued therefore counts as
  active only while the workspace's compute is starting, running or has a pending
  start, so a turn queued behind stopped compute does not spin
  ([turns](../../ontology/features/chat.g5.md#g5.chat.activity.turns), amended).
- In-attempt native questions and permission requests show the spinner, because
  the turn is still running. The request card in the transcript remains the
  attention cue.
- Attempt `ended_at` is strictly increasing within a session, because tasks are
  sequential. The watermark compares timestamps; equal end times within one chat
  are not expected.

## Verification plan

Backend integration tests (`g5-api` `integration` target), using real Postgres:
- ordering across active, inactive and no-turn chats, with ties;
- rename, settings and inspection leaving order unchanged;
- the 50-row limit applying after ordering;
- turn-state mapping for queued, running, waiting holds, continue, and
  failed/cancelled/stopped settlement;
- acknowledgements: reader-recorded, owner-isolated, monotonic, a stale attempt not
  clearing a newer one, unsettled or foreign attempts refused;
- the rollout floor;
- RLS and formal-review policy inventory;
- `chat.activity` publication on the main transitions.

Frontend unit tests:
- `chatPromptHistory`: per-chat isolation, legacy discard, budget eviction, storage
  failure;
- ChatPanel: chat A/B recall including a switch mid-traversal, first-send and
  delayed-send recording, the existing prompt-history suite;
- header rename: save, cancel, error;
- picker rows and indicators;
- acknowledgement conditions: hidden surface, unfocused page, scrolled up, stale
  transcript, newer concurrent result.

Code-block Copy:
- component tests: exact text with and without a language, restricted fences, no
  control on diagram, widget or question fences, an unavailable or refused clipboard,
  the keyboard path, feedback expiry, overlapping presses, and the FILES preview;
- a real-browser check of the clipboard contents in Chromium, in chat for Claude and
  Codex replies, and at narrow chat and detached-panel widths.

Playwright, against the stack with fake Claude and Codex:
- a background-chat completion shows a solid circle, which clears on viewing;
- two running chats are ordered by start;
- the header rename;
- A/B prompt recall surviving a reload;
- a VM stop/start or restart where the environment supports it.

Any lane left unexecuted is reported, not assumed.

## Validation

Chat activity, inspection, header rename and per-chat recall, at `331a472e7`:

- Backend (`g5-api` integration target, real Postgres): `api_chat_activity` (10 tests),
  `migration_116_chat_activity_inspection`, `formal_review_policy_inventory`,
  `route_auth_static`, and the affected chat, task-lifecycle, native-request,
  workspace-message, WebSocket and personal start-and-send suites (196 tests) pass.
  Clippy passes with `-D warnings` on `g5-api-core`, `g5-api-routes` and `g5-api`.
- Frontend: the full Vitest suite (331 files, 5,165 tests), the production typecheck and
  ESLint (no errors) pass. The browser lane passes 39 tests, including the new
  `chat-header.spec.ts` at 1100, 420 and 320 px in center and panel layouts.
- Full stack (`just dev-up` with a process-isolation host and the fake Claude and Codex
  CLIs): `e2e/flows/chat-activity.spec.ts` passes for both providers. It covers a
  background completion, an unfocused page, second-tab convergence, ordering, header
  rename, A/B recall across reload, and a VM stop/start (Claude). `chat.spec`,
  `personal-workspace`, `codex-settings`, `assistant-reply-wait`, `chat-composer-paste`,
  `chat-timeline-recovery` and `chat-timeline-history-pagination` pass. The dev stack
  needed CI's placeholder org Anthropic key (`seed-ci-org.sh`).

Not executed:
- `chat-restart-continuity` (needs CI's API container name);
- `workspace-preview` (needs a preview environment);
- the microVM host (this VM has no KVM);
- live Claude and Codex providers.

Separate devices converge through the same server state and user-scoped hint as the
tested second tab.
