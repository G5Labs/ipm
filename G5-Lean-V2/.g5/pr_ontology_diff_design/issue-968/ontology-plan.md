# Issue 968: keep a deferred question resumable only while its saved session awaits it

Work requested in [#968](https://github.com/G5Labs/G5-Lean/issues/968): an AskUserQuestion
answer was durably accepted while its native delivery stayed at **Sending** for five
minutes and then failed `wall_clock_exceeded`. The owner asked for design, implementation,
verification and a PR; implementation is authorized by that request.

## Findings at head

The adapter (`backend/g5d-agent/src/runtime.rs`) answers Claude's `PreToolUse` hook for
AskUserQuestion with `defer` whenever it is not restoring a question and Claude reports no
background work. It does not consider input it has already admitted to the same process.
A steered follow-up that Claude has buffered (or that the adapter still has queued behind
the hook reply) is consumed after the `tool_deferred` result: Claude starts another native
turn, synthesizes a missing tool result for the deferred question and saves that later turn.
The adapter still publishes `provider_request.opened` with `resume_session_id` and settles
`suspended / native_question_deferred`; TaskService keeps the request answerable; the
resume attempt sends only the initialize frame and waits the whole 300-second setup budget
for a question that the saved session no longer offers.

Pinned CLI 2.1.308 on Linux x86_64, driven with G5's exact arguments against a loopback
Messages API (`scripts/test-native-claude-question-overlap.py`, 2026-10-30):

| Sequence | Result |
|---|---|
| defer → close stdin → exit 0 → `--resume` with initialize only | hook callback reissued at once, then `can_use_tool` for the same question; answer accepted, `end_turn` |
| follow-up written before the hook reply, then defer | `tool_deferred`, then a second native turn (`system/init`, `end_turn` "Tool result missing") in the same process; transcript tail is the follow-up turn; resume reissues nothing in 20 s |
| follow-up written right after the defer reply | same as above: the race window after the reply is unsafe |
| follow-up before the hook, hook answered live | one native turn: the live `can_use_tool`, the answer and the follow-up all complete |
| same, with a second question after the first answer | both live questions answered in one turn |

These agree with the issue's macOS reproduction and establish the Linux behavior of the
pin. The historical QA transcript showed the same shape (a user message whose parent is the
`hook_deferred_tool` record).

## Owners and the missing guarantee

The Claude adapter owns steering admission, the question handshake, stdin lifetime and
native outcome classification. TaskService owns durable requests, accepted answers,
dispatch and the atomic answer/input settlement. The missing guarantee is the invariant
the issue names: **a deferred question is exposed as resumable only while the saved
native continuation can still restore that exact question.** Only the adapter can know
what it has written to the process, so the adapter decides deferral and confirms it;
TaskService keeps its existing settlement rules and gains accurate end reasons.

No new durable owner, queue or acknowledgement is added. The design reuses: the live
`can_use_tool` request path (no deadline for questions, stdin held open, steering refused
while a request is pending on both sides), the steering entry table in the adapter, the
existing `provider_request.invalidated` fact, the existing owner hold for a question that
can no longer be answered (`settle_task_row`, from #628), and the ordinary Send path for
recovery.

## Design

1. **Serialize the question hook with steering admission (adapter).** On the hook
   callback, defer only when the process holds nothing beyond the current turn: not
   restoring a question, no reported background work, no pending native request or queued
   reply, and no steering admitted to this process (no entry pending or shipped). The
   decision is taken before the hook reply is queued, and a deferring reply closes steering
   admission for the rest of the process: later `input.steer` lines get `input.steer_rejected`
   and the input stays in the ordinary queue with its revision and generation (existing
   API disposition). The probe shows the window between the reply and the `tool_deferred`
   result is unsafe, hence the gate closes at the reply, not at the result.
2. **Live questions after admitted steering (adapter, existing path).** When the gate
   finds admitted steering, the hook reply is `{}` and the question arrives as a live
   `can_use_tool` request of the running attempt: the attempt stays running without a
   deadline while the owner answers, steering is refused while it is pending, and every
   later question of that process is live too because the entries never disappear. This
   retains a provider process while the owner answers (the tradeoff the issue accepts);
   idle Stop is already prevented by running bounded work. Explicit Stop, Cancel, VM loss
   and transfer keep the live-request contract: the request is invalidated with the
   process and the turn holds or returns as those paths already decide; the question card
   shows that it was stopped, and the agent sees its unanswered question in history when
   the owner continues.
3. **`tool_deferred` is a candidate, confirmed at exit (adapter).** The durable question
   still opens on the `tool_deferred` result so the owner sees it at once and may answer
   while the process exits and capture runs. The adapter then confirms the candidate: if
   Claude starts another native turn after the deferred result, or the process ends with a
   provider error, or the session capture of a process that ended on its own fails, the
   adapter journals `provider_request.invalidated` for the question with the reason
   (`native_session_advanced`, `provider_error`, `session_not_saved`) and settles
   `suspended / native_question_lost` instead of `native_question_deferred`. TaskService
   maps that code to the existing owner hold (`waiting`, `continuation_reason=suspended`,
   `hold_reason=owner_stop`): the task holds for the owner, not for an answer, and
   Continue or the next message runs with history. Forced outcomes (owner Stop, VM loss)
   keep today's settlement: the question is preserved if its capture succeeded and is
   otherwise caught by item 4.
4. **Bound and explain restoration (adapter and TaskService).** While restoring a frozen
   answer, the matching question must arrive within `NATIVE_QUESTION_RESTORE_GRACE`
   (30 s) of Claude's initialize acknowledgement, and never later than the setup deadline.
   Expiry while still restoring settles `failed / native_question_restore_failed`
   (not retryable) with a notice that names the cause, instead of `wall_clock_exceeded`.
   TaskService invalidates the deferred request of any answer-restoring attempt that ends
   without transport-write evidence with `end_reason = native_question_restore_failed`
   (this also labels old guests' timeouts correctly), keeps the accepted response on the
   row, and answers an identical retry of an invalidated accepted answer with
   `undeliverable` rather than `queued`.
5. **Explicit recovery and accurate UI.** The chat timeline exposes the request's
   `nativeRequestEndReason` and the accepted `nativeRequestAnswer` beside the existing
   `nativeRequestStatus`. The question card shows *Answer saved · delivering* while the
   answer travels, *Answered* after delivery, and *Answer saved, not delivered* with
   **Send as a new message** when restoration failed or the session advanced; that button
   prefills the composer with the question and the saved answer and the owner sends it
   through the ordinary Send path, so nothing is injected into a different question and
   no history is truncated or replayed. The internal continuation that carries a saved
   answer is created with a visible label ("Delivering your saved answer to the agent's
   question") instead of empty content, so the queued-message tray and the delivered
   system row describe it; the label is never sent to the provider (the answer attempt
   writes only the initialize frame and the native `control_response`).
6. **Rollout.** The guest catalog advances to `2026.10.30-agent-turn-question-gate`
   (same `agent.turn` module versions; the hash and host pin follow). The API appends it to
   the shipped list but conditions no path on it: an older VM without the gate still gets
   bounded, correctly labeled restore failures from item 4 and the same recovery UI, and
   its historical contracts (`native_question_deferred`, `native_question_restore_failed`,
   frozen steering capability) are unchanged.

Rejected alternatives: withdrawing queued-but-unwritten steering frames at the hook so the
question can still defer (adds a second rejection path for an admitted write and a partial-
write case; the live path already handles them); delaying `provider_request.opened` until
after capture (hides the question for the capture window and changes the existing API
test contract without removing the need for exit-time confirmation); truncating the saved
transcript to the deferred checkpoint (rewrites real history, forbidden by the issue);
holding the task on `awaiting_answer` after a live question's process ends (nothing could
answer it natively, and the hold would never release by itself).

## Ontology

Amended in `.g5/ontology/features/chat.g5.md`: [g5.chat.native-question-recovery]
(deferral condition, confirmation, bounded restoration, recovery) and
[g5.conversation.delivery.steering] (a deferring hook closes admission). New
[g5.chat.native-question-delivery-state] [I] records only the presentation: acceptance
shown separately from delivery, the three card states following the state recovery
assigns, and the labeled continuation. Metadata links this plan and the probe.

## Acceptance coverage

- `g5d` dispatcher (`dispatch_modules.rs`): a follow-up steered before the hook keeps the
  question live and delivers both; steering after a deferring reply is rejected without a
  write and the deferral proceeds; a provider that runs another turn after `tool_deferred`
  (the 2.1.308 shape) settles `native_question_lost` with the question invalidated; a
  resume that acknowledges initialize but never reissues the question fails
  `native_question_restore_failed` within the bound; the existing deferral, resume,
  mismatch, redaction and round-trip cases stay green.
- API (`api_native_requests.rs`): `native_question_lost` holds for the owner with the
  request invalidated; a failed answer-restoring attempt labels the request
  `native_question_restore_failed`, keeps the response, exposes end reason and answer in
  the timeline and answers an identical retry `undeliverable`; the answer continuation row
  carries the label; existing deferred-answer, replay and pause cases stay green.
- UI (`AssistantTurn`/`ChatPanel` tests): delivering, delivered and not-delivered card
  states, the resend prefill, and the labeled tray row.
- Native pin: `scripts/test-native-claude-question-overlap.py` asserts the five sequences
  above against the pinned CLI on Linux; results recorded in this plan.
- Composed regressions: affected Rust integration suites, frontend unit tests, lint and
  format gates, then the published PR checks. Material gaps are recorded below.

## Implementation and verification — 2026-10-30

Adapter (`backend/g5d-agent/src/runtime.rs`, `runtime/steering.rs`): the question
hook defers only when the process holds nothing beyond the current turn, and a
deferring reply closes steering admission (`steering_closed`). A `tool_deferred`
result is a candidate (`deferred_question`); a later `TurnStarted` or non-deferral
`Result` sets `advanced_after_deferral`. Classification settles `native_question_lost`
when the session advanced or capture did not yield `Present`, invalidating the
request and emitting a cause notice before the outcome. Answer restoration is bounded
by `NATIVE_QUESTION_RESTORE_GRACE` armed at Claude's initialize reply; expiry while
restoring is `native_question_restore_failed`, not `wall_clock_exceeded`.

TaskService (`task_lifecycle.rs`): `native_question_lost` holds for the owner
(`owner_stop`); a terminal answer-restoring attempt labels its request
`native_question_restore_failed` and an identical retry claims `undeliverable`
(scoped to that end reason so ordinary settled replays stay `queued`). Dispatch
(`agent_dispatch.rs`) labels the answer continuation input; three timeline queries
expose `nativeRequestEndReason` and `nativeRequestAnswer`.

UI (`AssistantTurn.tsx`, `ChatPanel.tsx`): the question card shows saved-and-delivering,
answered, saved-but-not-delivered (with **Send as a new message** prefilling the
composer) and not-kept states; the answer continuation carries a visible label.

Rollout: guest catalog `2026.10.30-agent-turn-question-gate` with its recomputed hash
and host pin; appended to the API's shipped-catalog list, conditioning no path on it.

Executed (Linux, pinned CLI 2.1.308, Postgres 17):

- `scripts/test-native-claude-question-overlap.py` — PASS (A reissues; B does not; C/C2 deliver live).
- `g5d` dispatch: the clean deferral, the two lost-session cases, the admitted-steering
  live question, the restore-failure, and the existing resume/redaction/round-trip/steering
  tests — 8/8 pass.
- `g5-api` integration: full `api_native_requests` (15/15, incl. new lost-hold and
  restore-failed/undeliverable scenarios), the catalog frozen-module redispatch test,
  and `api_workspace_timeline` + `api_chat_activity` (21/21).
- Frontend: `ChatPanel`/`AssistantTurn`/`ChatMessage` suites (410) and
  `taskTimeline`/`useChat` (80); adapter clock unit test; dispatch catalog hash test.
- `cargo fmt` and `cargo clippy` clean on `g5d-agent` and `g5-api-core`; frontend
  `tsc` and `eslint` clean (pre-existing warnings only).

Gaps: the deployed-QA observation with delayed answering and a second question, and the
composed worker/API/DB/UI E2E lanes, run in CI and QA, not in this VM.

### Review round 1 (CodeRabbit on 97fc85bfc, 2026-10-30)

Validated and folded in:

- **Major — provider error after an advanced deferral.** The adapter invalidated the
  question only on the `native_question_lost` and unsaved-deferral codes, so a deferral
  followed by a second turn that ended in a provider error settled `provider_error` while
  TaskService's existing rule ("execution ended, not the question") kept the request
  answerable. The invalidation is now decided by the facts, not the code: an advanced
  session is withdrawn whatever ended the attempt (provider error or Stop), a clean exit
  without a captured session is withdrawn, and an unadvanced deferral ended by an error,
  Stop or limit keeps its question for restoration to decide. Dispatch scenario
  `advanced_error` covers it; the ontology sentence was corrected to say the same.
- **Major — early successful exit while restoring.** A resumed session that printed a
  result and exited 0 without reissuing the question classified as `Succeeded`. Exit
  classification now treats a successful exit with `restoring_question` still set as
  `native_question_restore_failed` (non-retryable), so the notice, the request label and
  the undeliverable retry all follow. Dispatch scenario `exits` covers it.
- **Minor — undeliverable retries.** An accepted answer invalidated with
  `native_session_advanced` or `session_not_saved` replayed as `queued`. Those reasons now
  join `native_question_restore_failed`; ordinary `execution_ended` settlement still queues
  (the #628 replay tests). API test `native_question_lost_outcome_holds_for_the_owner`
  answers before the loss and asserts the retry.
- **Minor — composer append.** The resend action appends the saved answer after a draft
  already typed instead of keeping only the draft; the client test types a draft first.
- **Trivial ×2 — probe hygiene.** The probe kills every CLI process group on any exit
  path before its scratch directory is removed, and scenarios C/C2 assert delivery from
  the recorded model requests: the pinned CLI folds a mid-turn message into the next tool
  result's content as a system reminder (not a separate text block), which is itself a
  useful protocol fact.

The `RUST_FMT` CI failure on that head was an unformatted unit test; `cargo fmt --all`.

### Review round 2 (CodeRabbit on 472e2426f)

- **Repeated resend.** After round 1 made the resend append to a typed draft, two clicks
  on an unedited composer appended the saved answer twice. The callback now remembers what
  it seeded and no-ops only for a repeat of that resend (see round 3 for the exact rule).
  The client test clicks twice unedited, then types a draft and clicks twice again.

### Review round 3 (CodeRabbit on fc3b26736)

- **A different question's resend was swallowed.** Round 2 compared only the composer
  value with the last seeded value, so with question A's answer still in the composer a
  click on question B's resend changed nothing and the owner could send A for B. The
  no-op now requires the same payload *and* a composer still exactly equal to the value
  that payload produced; another question's answer is appended. Nothing is matched by
  substring. Owner edits do not clear the memory: retyping that exact value by hand also
  no-ops, which loses nothing because the composer already holds the answer. The record
  above was corrected accordingly, and a two-question client test covers the append and
  the repeat.

### Review round 4 (owner, chat, 2026-10-31)

- **Duplicated rules.** The delivery-state block restated two rules the recovery decision
  owns: the undeliverable retry disposition and the withdrawn/restore-failed cases. Each
  rule now lives once: `g5.chat.native-question-recovery` keeps every lifecycle rule
  (deferral condition, withdrawal cases, bounded restoration, the kept answer, the
  undeliverable retry, the resend offer); `g5.chat.native-question-delivery-state` says
  only how those states are shown and links back. API comments that cited the
  delivery-state block for the retry and end-reason rules now cite recovery.

### Rebase onto main, 2026-11-02

`main` moved 135 commits (#966 live relay, #984 automatic continuation, notifications).
One textual conflict: `ChatPanel.tsx`, where `main` added `setSeedHeader` beside this
PR's `resendQuestionAnswer`; both kept. One semantic reconciliation Git did not flag:
`main`'s #966 live republish of a stored `provider_request.opened` row
(`attach_native_request_status`) carried only `nativeRequestStatus`, while this PR's
timeline reads also carry `nativeRequestEndReason` and `nativeRequestAnswer`; the republish
now carries all three, so the saved-but-not-delivered and not-kept card states render from
the live row as they do from history, as [live delivery](../../ontology/features/chat.g5.md#g5.chat.live-delivery)
intends ("carrying the request's status"). The ontology page auto-merged with `main`'s
new blocks; every other page is byte-identical to `main`.

`rust-deny` then failed on the rebased head with RUSTSEC-2026-0433 (`pulldown-cmark`
0.13.5, quadratic wikilink parsing; fixed in 0.13.6), an advisory published after
`main`'s last green run and not yet fixed on `main`. The lockfile moves only that crate
(`cargo update -p pulldown-cmark --precise 0.13.6`; `cargo metadata --locked` holds);
no workspace manifest changes. The flaw was reachable: the semantic Markdown parser and
review-edit offset mapping parse with `Options::all()`, which enables wikilinks, on
Project Doc and review text, so the bump is a real fix rather than gate hygiene.

`main` later took the same bump (01a9dfa70), so after the third rebase onto `main`
(19da3f5ea) this branch changes no lockfile line; the paragraph above records why the
branch carried it in the meantime.

That rebase replayed every commit without conflicts. `main` had added ontology changes
cards (`g5.chat.ontology-card`), which touch the same adapter run loop, outcome
application and assistant turn. They compose with this design and need no
reconciliation: a card is emitted and attached only for a succeeded attempt, so an
attempt that settles `native_question_lost` or `native_question_restore_failed`
carries none, and the card renders after the question card's states in the reply.
The guest catalog was untouched on `main`, so the `agent-turn-question-gate` version and
hash stand. Verified on the rebased tree: frontend typecheck, the chat component tests,
`cargo fmt --all --check`, and clippy for `g5d-agent` and `g5-api-core`.

### Review round 5 (owner, chat, 2026-11-03)

- **Too much on the card.** The owner does not want people to learn two new ideas or read
  cause explanations ([chat intent](../../chat_intent/issue-968/request.md)). The card now
  reads "Stopped" for a withdrawn question and for an undelivered saved answer, like any
  other ended request, and shows no explanation; the saved answer and **Send as a new
  message** remain as the recovery. The composer text no longer mentions the session. The
  adapter's two notices say only what happened and what to do; their `reason` field
  stays for diagnosis. `g5.chat.native-question-delivery-state` now records this as the
  owner's decision. Internals (end reasons, undeliverable retry, withdrawal) are unchanged.

