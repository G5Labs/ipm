# Follow-up messages queue during Codex device authorization

[Issue #1189](https://github.com/G5Labs/G5-Lean/issues/1189) records the owner's expectation that, while a ChatGPT subscription sign-in is pending, "all messages just keep queued". Until this change a new human message superseded the pending login: the original attempt was cancelled, its prompt withdrawn as failed history, and the new message ran instead, against [g5.chat.queue](../../ontology/features/chat.g5.md#g5.chat.queue).

## Governing commitments

- [g5.chat.login-before-prompt](../../ontology/features/chat.g5.md#g5.chat.login-before-prompt) (amended here): login completes before the original prompt is delivered; that prompt keeps its place, challenge and deadline, and later messages queue behind it without cancelling the login.
- [g5.chat.queue](../../ontology/features/chat.g5.md#g5.chat.queue): follow-ups queue without interrupting the current attempt and stay editable or withdrawable until reserved.
- [g5.chat.queued-launch](../../ontology/features/chat.g5.md#g5.chat.queued-launch): each queued input keeps its own launch choices.
- [g5.chat.failed-login](../../ontology/features/chat.g5.md#g5.chat.failed-login) and [its notice](../../ontology/features/chat.g5.md#g5.chat.failed-login.notice): a failed or interrupted login neither delivers the prompt nor retries it, and explains itself once per attempt.
- [g5.chat.device-challenge](../../ontology/features/chat.g5.md#g5.chat.device-challenge): the challenge stays private to the owner and attempt and expires promptly.
- [g5.conversation.delivery.steering](../../ontology/features/chat.g5.md#g5.conversation.delivery.steering): Codex attempts keep the queue path, so a queued follow-up cannot reach the provider before login.

## Design

Owners stay as they are. TaskService's login gate owns the attempt's auth phases; input admission owns ordering under the session lock; terminal settlement owns handing queued inputs to the next turn. The change removes the one cross-owner writer, `supersede_codex_auth`, and its call from input admission. A human message sent during `checking` or `waiting` then takes the path every follow-up on a running turn already takes: it joins the active task's queue with its own launch choices, editable and withdrawable until reserved. The gate never learns of it, so the challenge, its deadline and the original input's reservation are unchanged, and the readiness acknowledgment proceeds.

Readiness and admission no longer race for a decision. Ready locks the task and attempt rows; admission locks the session, then the task row. Either order serializes without a cycle and both succeed. Held-turn replacement does not apply: the task stays `running` while the auth phase is `waiting`, and Codex is not steerable, so nothing delivers a queued follow-up before login.

Failure paths are unchanged. The owner's Stop, the deadline, VM loss and ownership loss still end the login through their existing routes; the original prompt is withdrawn without replay, and settlement moves the queued follow-ups to the next turn, where each resolves its own route, including another sign-in if the cache was lost.

The `superseded` phase and its notice remain readable for historical rows and for attempts already superseded when this change deploys; nothing writes it for new attempts, and no withdrawn input is requeued. No new state, schema or VM protocol is needed; the VM module is untouched.

## Validation and evidence

API lifecycle tests replace the three supersession regressions: follow-ups queued during `checking` and `waiting` keep the task, reservation, challenge and deadline, raise no notice, stay editable and withdrawable, and after readiness the original prompt is accepted and the follow-ups dispatch in order, each with its own launch choices (a chat's permission mode is frozen per chat, so each follow-up carries a distinct effort and the dispatched effort is asserted per turn); readiness and admission succeed in both lock orders; Stop and timeout end only the original prompt while the queued follow-ups start the next turn; a historical `superseded` row still recovers its notice. The dispatcher test that killed login descendants on cancellation now represents the owner's Stop. The chat card's copy no longer promises cancellation; the card and countdown read the latest task's attempt, which the queued follow-ups share, so one card remains while they populate the queue.

The deployed-candidate journey (send A, queue B and C before authorizing, finish the same challenge, observe A, B, C in order) and the browser check across refresh are recorded in [docs/codex-device-auth.md](../../../docs/codex-device-auth.md) as release acceptance steps; they were not run from this workspace, which has no ChatGPT account. The 2026-09-23 live observation of cancellation there is historical evidence of the replaced behavior.
