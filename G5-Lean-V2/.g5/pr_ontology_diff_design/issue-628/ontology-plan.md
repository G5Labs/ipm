# Issue 628: keep task journals and settlement progressing through rejected provider events and races

Audit findings H2, D6, S23, S28/S29 and S9/S10/S15/S27 from the issue were re-verified
against this branch head (`49d0187d`). Line references below are to that head.

## Findings at head and scope

- **H2 (confirmed, both sides).** The runtime removes settled/cancelled provider request
  ids from its `pending` map (`g5d-agent/src/runtime.rs` `frame_written`,
  `ClaudeControl::Cancelled`), so Claude can reopen the same id in one attempt. The API's
  `journal_projection_error` (`task_lifecycle.rs:2315`) checks shape only; the reopened
  insert then violates `provider_requests UNIQUE (task_id, attempt, provider_request_id)`,
  the whole `apply` transaction fails, the sequence is never acknowledged, and every later
  fact — including the terminal outcome — is unreachable. The reconciler retries the same
  frame forever; a workspace Stop's `quiesce_agent_tasks → settle_stopped_execution →
  apply_replay_event` keeps failing until `give_up_saving`/save-window expiry force a kill
  with unflushed loss. The DDL also enforces `btrim` and char-length bounds the shape check
  does not mirror, and jsonb `::text` size checks can exceed the compact-JSON bound the
  code checks, so sibling deterministic rejections exist beyond the duplicate key.
- **D6 (confirmed).** `claim_native_reply` (`task_lifecycle.rs:2369`) gates non-deferred
  requests on `task.status = 'running'` and rejects `responded`/`invalidated` outright, so
  an identical retry after request settlement, invalidation at attempt end
  (`invalidate_attempt_requests` with `execution_ended`) or task completion gets a conflict
  instead of the frozen response. Deferred (`resume_session_id`) requests already replay
  `responded` answers; that support stays.
- **S28 (confirmed).** Unsupported `control_request` frames and in-attempt id reuse set
  `provider_error` and emit a notice but never answer Claude's control request
  (`runtime.rs:1759-1833`), leaving Claude blocked mid-turn with nothing bounding the wait.
- **S29 (confirmed).** Owner pause/chat-Stop invalidates a deferred question
  (`invalidate_attempt_requests`, reasons `owner_pause`/`chat_stop`); the already-journaled
  suspension (`native_question_deferred`) then still applies `hold_reason='awaiting_answer'`
  (`outcome_decision:4521`, kept by `settle_task_row:4319-4337`) although no live question
  remains, presenting an answer wait nothing can release.
- **S9 (confirmed).** `agent_tasks::load_active_attempts` tops up after the fair-share pass
  with a second claim loop; the first pass already committed `last_reconciled_at`, so the
  same attempts can be claimed twice in one pass and reconciled twice.
- **S15/S27 (confirmed, narrowed).** `claim_native_reply` locks `FOR UPDATE OF request,
  task, workspace` in one joined statement after the workspace lock; the plan may lock the
  request row before the task row while `apply` holds the task/attempt lock
  (`lock_attempt`) and then writes `provider_requests` rows — a task↔request order
  inversion that can deadlock. The reconciler claim queries use `SKIP LOCKED` and cannot
  participate in a cycle. Implicit FK locks (`provider_requests → task_attempts` KEY SHARE)
  are taken under an attempt row the same transaction already holds `FOR UPDATE`, so they
  add no new ordering edge.
- **S23 (moot at head).** Execution Jobs were removed entirely in #618 (`4839e606`,
  migration `088_remove_jobs.sql` drops the tables); `rollup_job_if_terminal` and the racy
  rollup no longer exist, and no analogous aggregate settlement remains. No repair pass is
  needed because the data was dropped with the feature. Recorded here; no code change.

## Design

Ownership is unchanged: the g5d-agent runtime owns the provider boundary, `TaskService`
(`task_lifecycle.rs`) owns journal projection and attempt settlement,
`WorkspaceVmService` owns stop/capture and is untouched. The missing guarantee is a
progress guarantee at two boundaries: the producer must not emit facts the persistence
contract must reject, and the API must convert the deterministic rejections that still
arrive into consumable, durable evidence instead of an eternal retry.

1. **Producer identity contract (g5d-agent).** Track every provider request id used in
   the attempt (opened, restored, cancelled or settled — a `HashSet` bounded by the
   recording budget, since each accepted open records a journal event). A `can_use_tool`
   control request whose id is reused anywhere in the attempt, or whose identity violates
   the persistence contract (empty/overlong after trim, or carrying U+0000 that storage
   sanitization would re-map onto another id), is rejected to Claude with its native
   mechanism — a `control_response` of subtype `error` — plus the existing bounded notice
   and `provider_error`, so the turn settles as a defined failure instead of leaving
   Claude blocked on an unanswered control request. Unsupported `control_request`
   subtypes get the same error response (S28); other unsupported control frames keep the
   notice-plus-`provider_error` path since nothing waits on them. The native deferral
   path validates the same identity rules for the tool-use id and resume session id.
2. **API boundary (task_lifecycle).** Two targeted mechanisms, no broad SQL swallowing:
   - `journal_projection_error` becomes stateful for `provider_request.opened`: in
     addition to the existing shape checks it mirrors the persistence contract
     (trimmed, 1..=512, valid resume shape) and, under the already-held attempt lock,
     rejects an id that already has a `provider_requests` row for this attempt. The
     rejection is the existing consumable projection-error evidence (bounded `error`
     timeline row; sequence consumed; later facts apply).
   - The three `provider_request.*` appliers run inside a savepoint. A deterministic
     failure — an `AppError` conflict from the applier, or a database error in SQLSTATE
     classes 22/23 (data exception, integrity violation: stable for the same event under
     the attempt lock) — rolls back to the savepoint, rewrites the event's timeline row
     into the same bounded projection-error evidence, and consumes the sequence. All
     other errors (connection loss, serialization/deadlock 40xxx, unexpected classes)
     propagate unconsumed, so transient storage failures keep their retry guarantee.
   Terminal `invocation.outcome` facts are deliberately not consumed-as-error: they are
   the settlement itself, and their applier takes no rejectable provider identity.
   With this, Stop settlement replays past a rejected frame to the terminal outcome and
   capture proceeds; quiescence, cancellation and save-receipt rules are untouched (3).
3. **Replay of accepted answers (D6, item 5).** Restructure `claim_native_reply` around
   request status after owner authorization and response validation:
   - `pending`: first acceptance; existing admission gates (running current attempt, or
     ready compute for deferred) and deadline check are unchanged.
   - any status with a stored response whose `(request_id, body, digest)` match the
     retry: return the frozen outcome without touching the VM or reopening the request —
     `responded` → `already_delivered`; `reply_pending` on the live running attempt →
     existing redelivery; every other matching state (execution ended, request
     invalidated after acceptance) → local `queued` (both values already in the client
     contract). Mismatching body/key/digest keeps the existing conflicts; an
     unanswered-then-invalidated request stays unanswerable.
4. **Hold correction (S29, item 6).** In `settle_task_row`, when a non-terminal decision
   holds `awaiting_answer` but no deferred request is `pending`/`reply_pending` any more,
   hold for the owner instead: `hold_reason = 'owner_stop'` with the suspension's
   continuation reason, so Continue and resume release it through their existing paths.
5. **Reconciler dedupe (S9, item 7).** `load_active_attempts` skips attempts already
   claimed earlier in the same pass.
6. **Lock order (S15/S27, item 7).** `claim_native_reply` takes its row locks in the
   canonical order the owner paths already document (workspace → task → request) by
   splitting the joined `FOR UPDATE` into workspace (held), then task, then request.
   The canonical order is recorded at `lock_task_workspace`.

PR review findings folded in (zcl-g5/ChatGPT on PR #635): (c) `parse_claude_control`
trimmed request identities before the new validator saw them, so a padded id was
silently normalized open and answered with an id the provider never sent —
identities now parse raw, the validator rejects untrimmed ids, and every
rejection echoes the exact raw id; (d) the reply route's authorization holds the
workspace row `FOR SHARE` (`require_current_workspace_owner`), and claiming in
that same transaction upgraded it to `FOR UPDATE`, deadlocking two concurrent
owner replies — `reply_to_native_request` now ends the authorization transaction
and claims in a fresh one that takes the exclusive locks directly, repeating
the full admission (current owner, undeleted workspace, no pending transfer)
under them, so nothing a window-admitted transfer or deletion refuses can
still freeze an answer. The same share-then-upgrade pattern pre-exists in the
other owner task actions (cancel/pause/continue/chat stop) and is reported as a
follow-up (issue #652) rather than expanded here. (e) The savepoint's
consumed-conflict path excludes the nested restored-answer input acceptance:
a conflict from the durable reservation — or acceptance evidence missing
entirely despite transport-write delivery — is state inconsistency, not a
rejectable provider fact, so it escalates and stalls visibly — like a
standalone `input.accepted` conflict — rather than recording delivered input
as rejected (coderabbit Major on PR #635).

Review findings folded in (codex, gpt-6 astra, xhigh): (a) rejected facts must
present as error evidence everywhere — the browser projection
(`taskTimeline.ts`) now maps any entry with `projection_error` to an error
status event instead of interpreting its original kind (a demoted `opened` or
`replied` could otherwise render a phantom approval card or mark a live
request resolved), and the chat history query no longer attaches
`nativeRequestStatus` to rejected rows; (b) a control-request rejection echoes
the provider's own request id, so each queued rejection envelope is charged to
the turn's unrecorded output — a stream flooding malformed or reused control
requests ends `limit_exceeded` through the ordinary budget instead of
accumulating queued envelopes without bound.

Rejected alternatives: pre-checking every DDL constraint in Rust without the savepoint
(fragile mirror of jsonb byte accounting; drifts), `ON CONFLICT DO NOTHING` on the insert
(silently merges distinct requests — #336 forbids normalizing identities), and
consuming all applier conflicts for every event kind (would let a corrupted
`input.accepted` or terminal outcome disappear instead of stalling visibly).

## Ontology

One new block `g5.chat.native-request-boundary` [D] in
`.g5/ontology/features/chat.g5.md` (next to `g5.chat.native-question-recovery`, which it
links) records the producer identity contract, the consumable-rejection rule with its
transient/deterministic split, the replay-after-settlement rule and the owner-hold rule.
No human-provided block changes.

## Acceptance coverage (tests)

Integration (`g5-api --test integration`, style of `api_native_requests.rs` /
`api_workspace_vms.rs` / `api_task_lifecycle.rs`):
- open → reply/settle → reopen same id → terminal outcome: duplicate becomes error
  evidence, watermark advances, attempt settles, reconciliation terminal (H2).
- sibling identities: whitespace-padded and overlong ids are consumed as evidence; a
  NUL-bearing id followed by its sanitized twin converges without merging identities;
  a valid open on the same attempt still succeeds after a rejected one.
- workspace Stop with a scripted journal carrying the duplicate frame before the
  terminal outcome: settlement completes, flush receipt recorded, no unflushed loss
  (`TestWorkspaceRuntimePeer::attach_with_journal_events`).
- claim replay matrix: identical retry after `responded`, after `execution_ended`
  invalidation, after task completion; changed body/key rejected; non-owner forbidden;
  deferred `responded` replay preserved (D6).
- pause/stop-invalidated deferred question + journaled `native_question_deferred`
  suspension → task waits with `owner_stop`, Continue succeeds (S29).
- owner reply racing journal settlement on separate connections (spawned tasks, two
  scoped transactions) converges without deadlock; reconciler pass claims each attempt
  once (S9, item 7 tests).

Runtime unit tests (`g5d-agent`): reused id and unsupported control request produce an
error `control_response` frame for Claude plus the notice; identity validation rejects
NUL/overlong ids at the producer.

Checked consumers: `reply_to_native_request` route (`routes/tasks.rs:198`) and the chat
UI consume `delivery_disposition` values already in `frontend/src/lib/types.ts:2991`
("delivered" | "already_delivered" | "queued"); no contract change. `run_admitted_stop`
and `reconcile_stopping` are exercised, not modified.
