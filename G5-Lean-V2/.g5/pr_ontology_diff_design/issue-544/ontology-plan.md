# Claude input during a running attempt

> Ontology rebase note: links to current owners were retargeted. Immutable main-commit links preserve the historical ontology/source context of this record; they are not additional current contracts.


Work requested in [#544](https://github.com/G5Labs/G5-Lean/issues/544), amended by
[the owner's PR comment](https://github.com/G5Labs/G5-Lean/pull/545#issuecomment-6184882074).
The authorized branch is `steer-claude`; append and push only that branch.

## Owners and smallest sufficient change

Reuse TaskService's session lock, inbox and immutable delivery generations;
WorkspaceRuntime's authenticated RPC; the dispatcher's control channel and journal;
and the worker's nonblocking stdin writer and existing multiple-turn settlement.
A complete write/flush to Claude stdin delivers the message. The worker journals
`input.shipped`; applying that event inserts the user row and starts the next
assistant segment on the same task and attempt. There is no provider replay,
consumption state, extra stdin hold, or change to attempt settlement.
The flush receipt is a delivery fact, not optional provider output. If it crosses
the provider recording budget, record it from the existing settlement reserve and
let the normal output-limit settlement stop the attempt; never discard the receipt
after writing the user's message.
Text blocks and tool calls already in progress retain their existing transcript
segment through completion; new units use the segment after the delivered message.
The flush can now occur inside a block, so moving its remaining deltas or tool result
to a different segment would duplicate text or leave an unfinished tool card.

The send endpoint persists the queued input and immutable steering binding before
one bounded RPC. Until the journaled flush, the input remains queued. The binding
prevents concurrent edits from changing the bytes being sent. Definite rejection
or a deadline without a definite answer clears the binding, leaving the ordinary
queue entry. A lost API request also leaves queued work, which the ordinary next-turn
reservation can claim; there is no steering retry loop, timer, recovery task or hold.
Normal queued-input reassignment and reservation clear the old steering binding.
After a fallback, stale flush evidence cannot deliver a newer reservation or edit.
The owner accepts a rare duplicate when an ambiguous write actually succeeded.

The dispatcher deduplicates each immutable input/generation and waits for a journaled
worker flush, not an acknowledgement of writing the module's own stdin. The worker
admits input and closes stdin in one event loop. Definite close/send rejection never
writes. After flush, queued Claude work may continue after stdin closes, using the
existing multiple-result settlement. A flushed message remains delivered after Stop,
process exit or VM loss even if Claude never reads it; the user can resend.

Only the current owner's ordinary send endpoint can steer. Require an active Claude
attempt with the frozen capability and initial acceptance, no older queued, reserved
or uncertain input, no hold, pending native request, background wait, cancellation or
pause. Attachments queue. Codex retains its existing queue. New Claude attempts enable
the capability everywhere; an existing process without it queues without restarting.
The UI shows queued input as Waiting and removes delivered input from the tray.
There are no steering Sending/Sent states or Redeliver/Cancel choices.

Governing: [inbox and delivery](https://github.com/G5Labs/G5-Lean/blob/3d17915df39bcf4cbe0cabc08500de63ff46fbb2/.g5/ontology/features/conversations.g5.md#queued-intent-is-separate-from-delivered-history),
[steering](../../ontology/features/chat.g5.md#g5.conversation.delivery.steering),
[steering fallback](../../ontology/features/chat.g5.md#g5.conversation.delivery.steering-recovery),
[transcript units](https://github.com/G5Labs/G5-Lean/blob/3d17915df39bcf4cbe0cabc08500de63ff46fbb2/.g5/ontology/features/conversations.g5.md#preserve-completed-assistant-units),
and [background input](../../ontology/features/chat.g5.md#g5.conversation.delivery.steering).

## Verification

Retain the pinned 2.1.285 recordings as protocol evidence: during a tool the same reply
can address new input, and during final text a second native turn uses the same process.
These scripted local-model recordings are not live-model QA or a release prerequisite.
Trim fake-provider, dispatcher-process, API, UI and public API/workspace E2E coverage
to the flush lifecycle. Prove exact text, FIFO, one task/attempt, immutable RPC retry,
close/send fallback, no replay argv or dependency, Stop/VM-loss preservation, ambiguous
RPC queue fallback, and the eligibility exclusions. Preserve initial-delivery recovery
and existing native requests, background settlement and Codex behavior.
Queued shared-VM and restart journeys explicitly select Codex through the atomic
session-start endpoint; the legacy empty-session endpoint always creates Claude.
The single-host CI lane runs the VM scenarios serially, while each shared-VM
scenario still holds concurrent chat turns. The new
Claude journey verifies delivery during a held provider step without any opt-in.
Fake Codex reuses the bounded workspace write/release directives for those queue
journeys, with a process test for writes before waiting and release after stdin EOF.
All deterministic CI hosts enable both fake providers so the queue journeys never
fall through to a real Codex process; the host configuration guard checks both.

Validation passed locally: two pinned-CLI flush-only recordings (tool and final text,
no replay flag or echoes, stdin EOF and normal exit); the real dispatcher/worker process
test covering flush before provider read, immutable retry, close rejection and Stop;
the output-boundary process case leaves 64 bytes in the recording budget, retains
one flush receipt, settles normally as limit-exceeded, and fits the existing reserve
(reproduced failing before the fix and passing afterward; runaway-output cases pass);
209 worker and 96 fake-provider tests; 107 API lifecycle, Stop and native-request tests
(including ten steering cases, rerun against the final migration); 297 chat/timeline/
WebSocket tests; TypeScript, frontend lint, Clippy, formatting, a fresh migration and E2E
compilation. Route SQL, service layering and shared-shape guards passed. The API and
UI cases also verify that an in-flight text block or tool call keeps its original
segment across a flush. The populated migration fixture uses the non-superuser,
non-BYPASSRLS deployment role: 084 adds the CHECK without validating old rows,
085 validates after 084 commits while a concurrent write remains open, and replay
is a no-op. The core-table writer guard removes the retired steering-recovery writer.
Full-stack native steering passed in Cloud Build `b1a445c4-02f0-44fd-be3f-2c2057ce3044`;
final published Rust/E2E checks and review findings are tracked on
[PR 545](https://github.com/G5Labs/G5-Lean/pull/545).
