# Frontend queue feedback for follow-ups

[Issue #1069](https://github.com/G5Labs/G5-Lean/issues/1069), narrowed by the
[owner's scope correction](../../chat_intent/issue-1069/frontend-scope.md), fixes
the speculative transcript bubble and the gap before queue reads complete.
Reuse the existing queue, labels, composer recovery and API. No backend change,
new recovery tray or draft-lifetime change belongs in this PR.

The governing commitments are [queue](../../ontology/features/chat.g5.md#g5.chat.queue),
[delivery](../../ontology/features/chat.g5.md#g5.chat.delivery),
[uncertain delivery](../../ontology/features/chat.g5.md#g5.chat.uncertain-delivery),
[replay](../../ontology/features/chat.g5.md#g5.chat.replay) and
[authority](../../ontology/features/chat.g5.md#g5.chat.authority).
TaskService still owns admission and delivery; the existing per-session frontend
query cache owns temporary presentation only. Pending feedback has no mutation
permissions. The POST response associates it with the server input ID; authoritative
queue/history data then replaces it without overwriting newer state. An input omitted
from a queue read stays visible without actions until history confirms delivery.
A history refresh used for this handoff must preserve live output and cached output
on read failure. Rejection removes only the affected temporary entry and uses the
existing composer recovery path.

The existing API reveals identity only in the POST response. If authoritative queue
or history data arrives first, a temporary duplicate can remain until that response;
repeated text and other writers prevent reliable content matching. If the response
is lost, frontend-only correlation cannot determine whether the submission was
accepted. These limit the original issue's exact-once pre-response acceptance
criterion; this PR does not claim to satisfy that criterion in full. No automatic
resend is added. Existing composer recovery can replace newer text on a same-chat
failure, and does not preserve failed snapshots across panel remounts. Those are
pre-existing limitations, not new recovery guarantees of this flicker fix.

Verification uses real frontend hooks and the shared ChatPanel with controlled
responses: slow POST/GET, queue/history handoff, repeated text and early delivery
before identity is known, chat switching, refusal, stale reads, permissions and
continued streaming. It covers Enter/Send, idle/busy chats, both provider selections
and Home. These are frontend regressions against the unchanged API, not live-provider
assurance. Previous backend and recovery-tray evidence no longer describes this PR.
