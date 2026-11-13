# Live chat delivery, five-second history batches and Stop without a lock upgrade

Work: [#966](https://github.com/G5Labs/G5-Lean/issues/966). Request and the owner's
decisions: [chat intent](../../chat_intent/issue-966/request.md). Authorized branch:
`i966-chat-decouple-live-streaming`. Baseline: `origin/main` at
`0e2f79b9bf25f4dd9804120ec46ce2560d8e4fb9`.

## What is wrong today

Every chat text delta is journaled durably in the guest, then found by the API's
reconciler poll (every two seconds), then applied in its own database transaction,
committed, acknowledged to the guest through a second admission transaction and a
journal rewrite with several fsyncs, and only then published to the browser
([agent_tasks.rs](../../../backend/g5-api-core/src/services/agents/agent_tasks.rs),
[journal.rs](../../../backend/g5d-dispatch/src/journal.rs)). The guest's `module.event`
broadcast reaches only the host's control connection, which drops it, so nothing
pushes. Live text therefore waits on history writes, and a backlog of one attempt
delays the terminal outcome and unrelated work. The browser appends `chat.chunk` text
blindly by message id, so a history refresh can overwrite live text and a redelivered
chunk duplicates it.

Separately, every agent-task control (Stop, Pause, Cancel, Continue, cancel of an
unknown input, native reply claim) holds the workspace row `FOR SHARE` from admission
and then upgrades it to `FOR UPDATE` after taking the session advisory lock. Two
controls, or a control and a Send, deadlock. None of those operations writes the
workspace row; they read only `owner_user_id`, which the share lock already protects.

## Owners and contract

Owners are unchanged: the guest dispatcher journal owns durable, sequenced, redacted
events and the acknowledgement watermark; TaskService owns history projection, task and
attempt transitions and settlement; the API event bus owns browser fan-out under the
existing membership-activity check; the browser's shared turn projection owns
rendering. One new process-local coordinator is added in the API, the **attempt
follower**, and one new durable fact, the guest-confirmed acknowledgement watermark.
No second durable queue, no browser-held recovery state.

### Attempt follower (API, process-local)

For each live attempt the reconciler claims, one follower task runs until the attempt's
terminal event is persisted and acknowledged, or its incarnation is replaced. It:

1. **Fetches** journal pages from the guest after its fetched cursor, woken by the
   host's module-event hint or by a poll (one second while a page was non-empty, two
   seconds idle). A fetch binds every event to the claimed task, attempt, VM and epoch;
   payloads cannot choose routing.
2. **Publishes live** immediately after binding, before any database write: the
   timeline entry each event projects to, with its execution identity
   (`taskId`, `attempt`, `seq`) and its owning message id, on the task and chat
   channels. Message ids are derived without the database: the attempt's canonical
   assistant message and, for a flushed steering input, the deterministic steered-reply
   id carried by `input.shipped`; a text, thought or tool unit stays with the segment
   where it first appeared. The follower's segment map is seeded once from persisted
   events and then advanced from the stream, so a stalled history transaction cannot
   stall live text.
3. **Buffers** the unpersisted tail (bounded: 4,096 events or 8 MiB per attempt). The
   same buffer serves the tail read for refresh and reconnect. On overflow the buffer
   is dropped and later rebuilt from the guest journal; live publication continues and
   the tail read reports catch-up rather than a partial range.
4. **Flushes history** in one transaction per batch: routine stream events
   (`message.*`, `thought.*`, `tool.*`, notices) wait up to **five seconds** from the
   first unflushed event, or until the batch reaches 256 events or 512 KiB; any other
   event kind (confirmed input delivery, provider requests, provider session evidence,
   title, preview readiness, the terminal outcome) flushes the batch at once. A page
   that still has more behind it flushes at once, so a ready backlog drains in bounded
   batches with no five-second wait between them. The flush runs concurrently with
   fetching and publishing, and waits a bounded two seconds for its locks (session
   advisory, VM, task and attempt rows): past that it yields and retries from the
   buffer, so a stalled write never holds a Stop or a send. A batch that is refused as
   inconsistent with the persisted prefix (typed, not matched on text) rebuilds the
   tail; any other failure retries the same batch. After the commit the stored rows of
   the batch's boundary kinds are published again under their ids: a provider request's
   live row is provisional and the stored one, with the request's status, is what the
   browser can act on.
5. **Acknowledges** the committed prefix cumulatively to the guest after the commit, on
   its own task so the next batch never waits on the guest, and records the guest's
   confirmation in `task_attempts.journal_acked_seq`. One acknowledgement task runs
   per follower and always carries the latest prefix. The reconciler's retry scan for
   unconfirmed prefixes runs off the tick, one scan at a time. Every runtime call
   holds a pooled connection as its incarnation fence for the whole round trip
   (`WorkspaceRuntime::call_once`), so the follower's calls beside the fetch —
   acknowledgements (followers' and the scan's), the idle status probe and control
   redeliveries — share one bound of a quarter of the runtime pool (at least one),
   while fetches are bounded to half the pool and at most 16 (`FollowerRegistry::for_pool`):
   a host whose guests accept calls but never answer pins at most three quarters of
   the pool (10 + 5 of the default 20; 16 + 13 of the deployed 54), never all of it. A probe runs on its
   own task, is re-armed from its return rather than its start, and backs off up to
   a minute after a failure or timeout.
6. **Reconciles controls**: the committed cancel or pause intent is re-read when the
   route pokes the follower, every ten seconds regardless, and again after a read that
   failed; it is redelivered every two seconds while it stands, one call in flight
   under the shared side-call bound, beside the reader rather than inside it.

Fairness: followers are independent tasks, so one stalled runtime or one large backlog
cannot hold the reconciler tick, queued dispatch or native-request work. Guest fetches
share a per-process limit so a burst cannot monopolize the host tunnel. The tick keeps
its organization rotation for claims and excludes attempts already followed.

Capacity limits that make this sufficient: the event bus is process-local and every
environment runs one API replica (helm `api.replicaCount: 1`), which is the envelope
the current `chat.chunk` fan-out already has; a multi-replica deployment would need a
cross-replica bus and is out of scope. The guest reply line is capped at 4 MiB by both
the host relay and the API; pages stay under 2 MiB, one event is at most 1 MiB.

### History batch (TaskService)

`apply_journal_batch` applies a contiguous run of events for one attempt under the
locks `apply` takes today, once: session turn-allocation advisory lock, task and
attempt rows. It drops events at or below the persisted watermark, refuses a gap,
inserts all event rows in one statement with each event's receipt time as
`created_at` (so timeline order and cursors are unchanged), applies per-kind effects in
event order, refreshes each touched assistant segment once, settles on the terminal
event, and advances the watermark once. Row shape and identities are unchanged
(`g5.chat.journal`); the row metadata gains `seq` so saved and live rows share a key.
`apply` for one event remains the batch of one, used by control-plane facts.

The VM stop drain and Changes housekeeping, the other journal consumers, use the same
paged replay and the batch apply, and acknowledge their required prefix before
retirement.

### Tail read

`GET /api/v1/sessions/:id/live-tail` returns, for the session's live attempt, the
persisted watermark, the fetched-but-unpersisted events with their owning message ids
and a state: `live` (the tail is complete from the watermark), `catching_up` (no
follower in this process yet, or the buffer is being rebuilt) or `none` (no live
attempt). It needs session read authority, the same as the transcript; events are
already redacted by the guest. The browser reads the saved transcript and timeline,
then the tail, merges by execution identity and sequence, and re-reads when the
persisted prefix moved past what it holds. The tail is read only after a completed
saved-history read, when the turn becomes active over a held window, and once per
sequence gap; a cache write never triggers one. Live rows are placed once per animation
frame. A hole in an attempt's sequence that persistence already covers is filled by
reading saved history backward from the row after it (`timelineHoles.ts`), bounded per
signal; the newest-window refresh reads back until it meets a held saved row. A live row is dropped only when its own saved copy is held,
never on a timer: a saved row beyond its sequence proves it saved, not read.

### Guest journal

Replay accepts `limit` and `max_bytes` and answers `has_more`/`next_seq`; it reads from
a per-execution sequence-to-offset index instead of parsing the whole file. An
acknowledgement always writes the durable watermark but compacts the file only when the
acknowledged prefix is large (at least 1 MiB and a quarter of the file), when nothing
unacknowledged remains (the file is removed) or when the budget sweep needs it. Unacknowledged
events are never dropped. Callers that omit paging get a bounded page too, which older
APIs already tolerate because they re-replay above their watermark.

### Module-event hint (host and guest control stream)

The host's control connection already receives every `module.event`. It now coalesces
them into a content-free `vm.module_events {vm_id, exec_ids}` notification to the API,
sent only when the API advertised `module_event_hints_enabled` in `host.accept`, which
the API sets only for hosts advertising the `module-event-hints:v1` capability. Older
peers on either side see nothing new, and polling remains the fallback. A hint carries
no data, so loss or coalescing is harmless; the journal stays the single source and
ordering authority.

### Agent-task controls

`end_chat_turn`, `request_workspace_task_cancel`, `request_workspace_task_pause`,
`continue_workspace_task`, `claim_native_reply` and the unknown-input cancel keep
`FOR SHARE OF workspace` through commit, taken in the operation itself so direct service
callers get the same guarantee, and lock only the task, attempt and request rows they
transition. The exclusive workspace lock and `lock_task_workspace` are removed. Preview
start, cancel and the projection of preview readiness and outcomes, which write VM
state, lock the VM row exclusively (before the task row, as a release does) with the
workspace shared.
Transfer, delete, stop and placement still take the workspace exclusively, so a held
share continues to exclude them.

Repeated Stop requests for one named turn converge: a running turn keeps its one
cancellation intent and one queue transition; a queued or held turn ends once and later
requests report `already_ended`; a Stop that names a turn which already ended never
reaches its successor. Stop returns the dispatcher's disposition (`signalling`,
`already_finished`, `cancelled`) or `undelivered` when the intent committed but the
runtime call failed, in which case the follower redelivers it; only an uncommitted
request is an HTTP error. The browser shows `Stopping…` as soon as a Stop is sent,
suppresses another request for that turn while one is in flight, shows
`Finishing…` for `already_finished`, and offers Retry on failure, in the workbench
and the Home chat alike.

## Choices considered

- **Push payloads through the host control stream** instead of a hint: rejected. It
  would be a second data path with its own ordering and gap protocol beside the
  journal, and high-volume text on the stream that carries lease renewals.
- **Poll faster without a hint**: sufficient for latency only at a cost of one guest
  round trip per attempt per interval; kept as the fallback for older hosts.
- **Deterministic ids without a follower map**: message ids are already deterministic,
  so the map only caches which segment owns each unit; it is rebuildable from the
  journal and persisted rows, not a new durable record.
- **Retry acknowledgements in memory only**: an API restart would leave retained guest
  journal data until the VM is retired; the confirmed watermark column makes the
  obligation durable and bounded.
- **Lock the workspace exclusively at admission** (`scoped_db_for_workspace_update`):
  rejected; it inverts against journal projection, which holds the session advisory lock
  and then needs a key-share on the workspace to insert the successor task.

## Assumptions and acceptance conditions

- One API replica per environment; the live relay is process-local like the existing
  fan-out. Recorded in `g5.chat.live-delivery`.
- An older guest without paging may still answer one oversized reply for a backlog over
  4 MiB; this is the pre-existing limit and is logged, not silently skipped.
- Live text may precede saved history by up to the flush interval; refresh recovers it
  through the tail read, not from browser memory.
- The membership-activity check stays per delivered WebSocket message; batching it is a
  separate optimization. The per-connection writer queue grows from 256 to 2,048 events
  so one published page (at most 1,024 rows) does not overflow a healthy connection; a
  slow connection still drops events and is closed through its authority signal (a close
  frame queued behind the full queue would be dropped like the events), and the browser closes a
  sequence gap with a tail read rather than rendering past it.
- Latency, batch size, transaction count, acknowledgement age and Stop disposition are
  recorded as structured tracing fields (no metrics library exists).

Verification maps the issue's criteria onto real boundaries: g5-api integration tests
with PostgreSQL and the scripted runtime peer for batching cadence, the incident shape,
database write stalls, tail recovery, the deterministic Stop race (advisory lock held,
two and twenty authenticated Stops, with a reconciliation waiter, and with a foreign
share holder), control siblings and crash/ack recovery; g5d-dispatch journal tests for
paging, amortized compaction, concurrent append during compaction and watermark
retention; host control tests for the negotiated hint; frontend unit tests for live-row
identity, duplicate and reorder handling, refresh merge and the Stopping state; and the
existing Playwright flows. Remaining gaps are reported with the PR.
