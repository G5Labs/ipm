# History coverage for chat turns

Issue: [G5Labs/G5-Lean#847](https://github.com/G5Labs/G5-Lean/issues/847).
Authorized branch: `issue-847-chat-history-loses-tool-calls-command-history`.

Governing blocks: [history window](../../ontology/features/chat.g5.md#g5.chat.history-window),
[unloaded reply text](../../ontology/features/chat.g5.md#g5.chat.history-window.unloaded-text) (owner chose withholding),
[history refresh](../../ontology/features/chat.g5.md#g5.chat.history-refresh). They specialize
[presentation](../../ontology/features/chat.g5.md#g5.chat.presentation) and
[replay](../../ontology/features/chat.g5.md#g5.chat.replay), and keep the
[presentation owner](../../ontology/features/chat.g5.md#g5.chat.presentation-owner) from #817.

## Problem

Messages load 500 at a time; timeline activity loads 200 raw `task_events` rows at a
time, newest first. Nothing relates the two windows, so every turn older than the
activity floor renders as bare prose. #819 then fills the unloaded head of a
partially loaded turn with saved prose, which makes that turn look complete. Measured
on QA (2026-10-27): session `0420d525…` has 1,257 rows, of which 894 are
`message.delta`; session `67f8acdb…` has one turn of 3,467 rows (561 tools, 1,579
deltas), and its whole history is 21 pages and about 8 MB. No page size makes large
turns fit, so the UI has to know and show what is loaded.

Two refresh defects compound it. `useSessionTimeline` is a TanStack infinite query, and
v5 refetches every held page in order on each invalidation (the 3 s streaming poll and
every WS chat event), committing only after all of them. More loaded history therefore
means more requests per poll, and a slow refetch is cancelled by the next one. The
page-1 merge also drops held rows when a refreshed window is more than five pages
away from them (the old "D42 exception").

## Design

**Coverage (derived, no new state).** The hook's loaded rows are contiguous from the
floor (the oldest held row) to the newest. A turn's activity is *complete* when no older
activity exists, or when the floor is strictly older than the turn's assistant message
(created atomically with the accepted input, before any of its events). The comparison
is made at millisecond precision, and an equal timestamp counts as not covered, so the
error is always toward one extra load. Otherwise the turn is *partial* when some of its
events are loaded, or *unloaded*. ChatPanel derives this per assistant turn from the
timeline floor and `hasOlder`. While the first timeline read is pending, no turn counts
as complete: each shows its marker in a loading state rather than its saved prose.

**Rendering (AssistantTurn stays the owner).** A partial or unloaded turn renders a
not-loaded marker ("Earlier activity in this reply isn't loaded" with a load action)
where its loaded activity begins. `buildAssistantTurnBlocks` receives the coverage, and
for a non-complete turn it skips the #819 recovery of saved prose *before* the first
loaded block; recovery of interior and trailing gaps is unchanged, because the loaded
region is contiguous. Complete turns keep the full #819 behavior, which still serves
old sparse rows. An unloaded turn shows the marker under its prompt and keeps its
terminal state (Stopped or an error) visible. Its saved reply text, artifacts, saved tool
list and citations stay hidden until its activity loads.

**Loading.** The marker loads older activity when clicked, and automatically while any
marker is within a viewport of the visible area, one page at a time through the
existing single-flight `loadOlder`, until no marker is in range or the floor passes it.
The panel's retention keeps the bottom edge of the row at the viewport top fixed, so
loading does not move the visible rows, whether the new activity lands above, inside or
below that row. A
failed load turns the marker into a retry and stops automatic loading for that marker
until it is retried. The top "Load earlier history" control remains for messages older
than the message window.

**Refresh (reuse the messages lifecycle).** Replace the infinite query with the
`useChatMessages` shape: one query entry holding the merged ascending rows, the floor
cursor and `hasOlder`. A refresh is one newest-window read merged into the held rows,
with fetched rows winning for the range they cover. `loadOlder` reads below the held
floor cursor, cancels an in-flight refresh on that key, and prepends through
`setQueryData`, so an ordinary refresh does not re-read held older pages; the two
exceptions are reading back across a gap and reaching an open request, both below. When a refreshed window shares
no row with the held rows, the read continues backwards until it does or history ends.
Progress is kept in a session-scoped ref, so a read cancelled by the next poll resumes
instead of restarting; a later window that overlaps the saved progress continues from
its cursor. The fixed five-page cap and its drop are removed. Held rows are dropped only
when history ends without meeting them, because they no longer exist server-side. The
bridge cost equals the rows appended since the last successful read, which contiguous
history needs anyway. Held rows are otherwise never re-read, because journaled events
are immutable and later changes arrive as new rows (tool completions, request
replies and invalidations). The exception is a native request's `reply_pending` state,
which the server joins onto the opening row at read time: while a held request is
open, each refresh reads back to its row. A reply or invalidation row, or its
attempt's `invocation.outcome`, ends this, so a stale record on a finished attempt
cannot make every poll read deep history. The consumer surface keeps `data`, `hasOlder`,
`loadOlder` and `appendError`, replaces `isFetchingNextPage` with `isLoadingOlder`, and
adds `coverage`. WorkspacePage, the workbench controller and the personal chat panel read
the renamed flag and pass the coverage inputs to ChatPanel.

## Choices and reasons

- *Frontend coverage rather than server turn paging.* A turn can exceed any bounded
  page (3,467 rows), so turn paging still needs an incomplete state, and it would
  change a contract that `g5d workspace timeline` also reads. Coverage is derived
  from rows the client already holds and adds no durable record.
- *No server delta compaction now.* Dropping superseded `message.delta` rows would
  roughly halve the pages needed, but it has to match the client's snapshot-anchor
  rules (`projectNativeSnapshots`) at page boundaries, and it would put a second
  implementation of that logic on the server. Coverage and viewport loading meet the
  issue's guarantees without it. It remains a separate optimization if load latency
  needs it.
- *Withholding #819 head prose instead of reverting #819.* Interior and trailing
  recovery still repairs complete turns whose rows predate per-block output.
- *Resumable bridge rather than a gap representation.* A gap would need
  multi-segment coverage everywhere. Resuming keeps one contiguous region and still
  converges under the 3 s cancelling poll, provided fewer than one page (200 rows)
  arrives between polls.

## Assumptions

- `task_events` are not deleted while their session exists (no `DELETE FROM
  task_events` path found), so "history ended without the held rows" is exceptional.
- Turn events are created no earlier than their assistant message. Pre-acceptance
  checkpoints that lack a message identity are not rendered as turn content.
- The panel's top-edge retention covers growth at a marker inside the list, not only
  at the top. If it does not, anchoring on the first loaded row below the marker is
  part of this change.

## Verification plan

- Hook tests (`useChat.test.ts`): tail merge with overlap; a disjoint window bridged
  across more than five pages; a bridge cancelled mid-way that resumes; history end
  dropping vanished rows; `loadOlder` concurrent with a refresh (no lost prepend); a
  failed and retried older read; a refresh never re-reading older pages; session switch.
- Turn tests (`AssistantTurn`/`ChatPanel`): complete, partial and unloaded turns at the
  real message/timeline boundary, including a turn larger than one page; no head
  prose for a partial turn; no duplicated or reordered blocks after loading older pages
  with overlap; an active turn appending while older pages load; marker retry; auto-load
  only while a marker is near the viewport.
- Update the D42 cap/drop tests to the new refresh rule.
- Browser: real ChatPanel in the Vite harness with a fixture shaped like the QA sessions
  (a fresh load, then scrolling up). Live QA rechecks of the issue's passages need a QA
  deployment; they are reported separately and not claimed by local tests.

## Validation

Implemented on the authorized branch as designed. Two refinements came up during
implementation. Option A also keeps saved artifacts, saved tool lists and citations out of
an unloaded turn, which shows its marker and terminal state only. And the retention
anchor follows the bottom edge of the row at the viewport top: growth happens only at a
turn's head, so it can land below the viewport, where the old shift by total growth
would move the visible rows.

- Vitest: the full frontend suite passed before the open-request read-back (345 files,
  5,239 tests) and again after it, on each rebased head (5,242 tests; then 342 files and
  5,239 tests at `a4ef973` after `main` retired some files). This includes the new hook,
  coverage, AssistantTurn and ChatPanel boundary tests and the three open-request tests. The
  first of those fails with the read-back disabled. TypeScript, ESLint (no new warnings) and
  Prettier pass.
- CI on PR #897 passed g5-ci-e2e and g5-ci-rust at `a4ef973`. Earlier heads failed only
  on unrelated flakes: a g5d reply-after-End race and a retry-passed preview run-log spec.
- Playwright (`chat-timeline-history-pagination.spec.ts`, mocked HTTP): the unloaded
  oldest turn shows its marker and no saved reply. Nothing older is requested until it
  is scrolled into view; then exactly one older page loads, after which narration and
  tool evidence render together and are not re-read on later refreshes.
  `chat-timeline-recovery.spec.ts` passes unchanged.
- Chromium harness (real ChatPanel, 20 turns of 66 rows, 60-row pages, 250 ms reads):
  a fresh load stopped after two pages with 18 turns marked. With a prompt pinned at the
  viewport top, loads of two and five pages moved it by at most 1 px. Scrolling to the
  top loaded to completion with every prose step present once and in order. A failed
  read stopped automatic loading until Retry, which resumed it.
- Not run locally: the live-backend E2E specs (no auth/backend stack in this VM) and
  the issue's QA passages, which need a deployment.

