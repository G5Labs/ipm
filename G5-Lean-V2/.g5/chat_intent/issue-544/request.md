# Native Claude steering delivery

The owner requested [issue 544](https://github.com/G5Labs/G5-Lean/issues/544),
verification, a PR, green checks, no merge conflict, and inline replies to verified
in-scope high-priority review findings.

The [owner's PR direction](https://github.com/G5Labs/G5-Lean/pull/545#issuecomment-6184882074)
on 2026-10-15 supersedes the earlier replay-confirmation interpretation: a complete
write and flush to Claude stdin means delivered, just as for the initial input.
There is no consumption state, replay requirement or steering recovery lifecycle.
After flush, Stop or VM loss may prevent Claude from reading the message; the owner
accepts that loss. Keep the message in history and let the user resend if needed.
If the single send RPC has no definite answer by its deadline, keep the input queued
for the next turn; the owner accepts the rare duplicate if the write actually happened.
Definite pre-write rejection also queues. Same-key retries still write at most once.

Shipping enables native steering everywhere it is eligible, without an organization
switch or live-Claude QA gate. A frozen per-attempt capability remains appropriate
for processes launched before this protocol. Ordinary owner text, exact bytes and
FIFO are preserved; Codex, attachments, background waits, native requests, holds,
cancellation and pause retain the ordinary queue fallback. Never kill or restart
Claude to deliver a message.
