# Preserve accepted chat input

The owner authorized implementation of the five fixes presented in this chat for
[issue #629](https://github.com/G5Labs/G5-Lean/issues/629), followed by verification,
PR publication and monitoring of checks, conflicts and required review findings.

## Ownership and behavior

TaskService owns each accepted input and its launch preferences. Store those choices
on the input and select them when that input starts a later turn; copying a previous
task is only the compatibility fallback for historical inputs with no saved choices.
An attempt retains its [frozen configuration](../../ontology/features/chat.g5.md#g5.chat.frozen-attempt),
including when another message is steered into it. Persist optional confirmed launch
evidence with the input and revalidate the effective provider, model, effort, permission
mode and account identity before its first ordinary attempt under the current owner's
credential authority. Unrelated settings or secret changes do not invalidate those
choices; Start retains its existing complete-resolution check. Changed choices fail
admission visibly with their old and current values and no automatic retry loop.
The owner's next Send clears stale confirmation on a failed, never-admitted input,
retains its preferences and runs it on current values ahead of the new message; Edit
and Withdraw remain available. Admitted retries retain their frozen choices. This
recovery follows the owner's [PR review](https://github.com/G5Labs/G5-Lean/pull/907#pullrequestreview-5676664038).
Successful reservation saves the admitted resolution with the input, so permitted
metadata changes cannot make a later retry use a different resolution.

Input-targeted Stop uses the same reservation exclusion as Withdraw. Keep the existing
session lock, revision and generation guards, and ordinary unreserved cancellation.

Steering owns independently observed write/flush evidence. Retain a positive RPC receipt
on the input independently of ordered transcript projection. The existing task settlement
path projects known receipts before releasing queued work; reservations without known
delivery return to the normal queue. Late completion may recover the original binding
only while its revision and generation remain current; an edit or subsequent reservation
fences it out. Reuse the existing journal and settlement owners rather than introducing
a steering retry reconciler or a new task lifecycle. Normal journal replay still owns
output segmentation and ordering; settlement uses the receipt if that journal is lost.

The committed Send result is an acceptance receipt. Steering completion records facts
under the service's tenant authority and survives cancellation of the HTTP request.
Its failure cannot turn the accepted Send into an error. Return only the identity and
outcome already authorized at acceptance, without waiting for steering completion and
with no post-transfer disclosure of chat state.
For steered Send, the response's `delivery` is always the committed `"queued"` snapshot;
the request no longer waits for the steering RPC. The frontend uses `inputId` and
subsequent input/journal updates rather than that response field to show delivery.

These implement [confirmed launch](../../ontology/features/chat.g5.md#g5.chat.confirmed-launch),
[input ownership](../../ontology/features/chat.g5.md#g5.chat.ownership), and
[steering recovery](../../ontology/features/chat.g5.md#g5.conversation.delivery.steering-recovery).
Ambiguous writes retain the existing accepted duplicate risk. A flush is not proof
that the provider consumed the message. Earlier #636 repairs remain governing.

## Verification

Add regressions at the API, database, dispatch and lifecycle boundaries for distinct
queued settings, changed inherited confirmations, Stop after reservation, positive RPC
evidence before VM loss, interrupted request completion, ownership changes and stale
receipts. Check ordinary successful dispatch/steering and explicit ambiguity fallback.
Run the affected Rust integration tests, formatting/lint and repository checks, then
the published PR checks. Record executed results and material gaps here.

Initial implementation locally verified against main `9b89761afbafc9230c87c84603ec862f95d133da`.

- Before implementation, the unchanged baseline failed both new regressions:
  input-targeted Stop returned 200 after a steering reservation (expected 409),
  and a known flushed input returned to `queued` after VM loss (expected `delivered`).
- The initial broad API run passed **176 tests**, including nine new acceptance tests.
  It covers distinct queued account/model/effort choices, immutable invoke retries,
  changed confirmation refusal, confirmed redelivery, ordinary Stop, steering Stop,
  frozen steering configuration, known flush before/after settlement, late withdrawal,
  newer revision/generation fencing, HTTP cancellation and changed ownership.
- Existing account, secret admission, continuation, preview, fork/start and transfer
  tests passed in that run. The generic runtime fixture now explicitly declines
  steering; only the dedicated live-steering fixture reports a positive flush.
- The frontend's chat hook and assistant-reply suites passed **109 tests**.
- Workspace Clippy with warnings denied, formatting, whitespace, migration checksum
  and version uniqueness, repository layout and local validation contract checks passed.

Commands (the container uses `RUSTUP_HOME=/usr/local/rustup` for Cargo):

```sh
cargo test -p g5-api --test integration -- api_task_lifecycle:: api_chat_ api_workspace_messages:: api_provider_accounts:: api_agent_execution:: api_runtime_secret_admission:: api_personal_start_and_send:: api_workspace_transfer_credentials:: --test-threads=4
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all --check
npm test -- src/hooks/useChat.test.ts src/components/chat/AssistantTurn.test.tsx # in frontend/
git diff --check
bash scripts/check-migration-version-uniqueness.sh
python3 scripts/check-migration-checksums.py
python3 scripts/check-layout.py
python3 scripts/check_local_validation_contracts.py
```

The API tests exercise real PostgreSQL, HTTP admission and runtime transport with a
scripted guest. No live Claude/Codex session was run locally, and database persistence
retries do not promise survival of a full API-process crash before a receipt commits.
Ambiguous delivery retains its existing recovery policy. Published CI and review
status are tracked on the PR; local results do not substitute for those gates.

### PR review follow-up

CodeRabbit's Major finding on PR #907 reproduced on `b89e2caa3`: receipt completion
waited for a workspace `FOR UPDATE` lock while holding session turn allocation,
against a concurrent Send's workspace `FOR SHARE` lock. The added regression failed
with PostgreSQL's lock timeout. Receipt recording now takes no exclusive workspace
lock: it records prior execution evidence under the session lock and exact historical
attempt/input identity, without performing a new owner-authorized workspace mutation.
The regression covers both a live attempt and projection after settlement while the
other transaction retains its workspace lock. Completion logging also retains the
persistence or spawned-task error without changing the accepted Send response.

After these changes, **46 steering and Stop tests passed**, including the tenth new
acceptance test; workspace Clippy with warnings denied and formatting passed again.

```sh
cargo test -p g5-api --test integration -- api_task_lifecycle::steering:: api_chat_turn_stop:: --test-threads=4
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all --check
```

The next review identified an unbounded wait for receipt persistence in the response
path. A regression on `16ee52a64` blocked persistence behind the session advisory lock
and failed because the committed Send still waited for completion. Completion and its
error observer now run independently of the response; the regression also checks that
the receipt persists after the lock releases and survives VM loss. The disconnected
client test accepts either cancellation or an already-returned receipt while verifying
that the still-blocked guest RPC completes independently.

The reported settlement lock inversion was checked against all settlement entry points
and is not present: `TaskService::apply` acquires session turn allocation before calling
any private lifecycle handler or taking the task/attempt locks. The later allocation
call reenters the same transaction's lock. The inline reply records this evidence.

Rebased onto main `cee24de3a698b59f4aa895b5966723e10c8b5b45`. The new broad API run
passed **177 tests**, with one old assertion requiring Send to wait for the 45-second
steering timeout. That assertion was updated to require prompt acceptance and then
independently await timeout recovery, retaining the one-RPC and queued-input checks.
The subsequent **47-test steering/Stop suite passed**, including all eleven new
acceptance tests. The blocked-persistence regression now passes while its database
lock stays held, and completion resumes after release. The frontend's **109 tests**,
workspace Clippy with warnings denied, formatting and repository checks passed again.

### Owner review: confirmation recovery and comparison scope

The owner's [review of `78be0abb`](https://github.com/G5Labs/G5-Lean/pull/907#pullrequestreview-5676664038)
identified two first-admission defects. The new regressions failed on the previous
implementation: a changed model produced only opaque resolution IDs in the refusal
notice, and an unrelated setting change refused admission. The next-Send recovery
path retained the stale confirmation and therefore repeated the same refusal.

Next Send now clears confirmation only when reattaching a failed, never-admitted
input, preserves its preferences, and dispatches it before the new message. The
notice identifies changed choices and their old/current values and explains Send
or Withdraw recovery. First admission compares the five effective choices; a
successful reservation saves the admitted resolution for frozen retries. Start's
full resolution-ID check is unchanged. All three settlement callers pass their
existing locked attempt, the duplicate no-requeue assignment is removed, and
confirmed-launch evidence remains `agent`.

Rebased onto main `823454a1c72c12d674c97075568ccda30be4e169`. The broad API command
above passed **180 tests**, including **49 steering/Stop tests** and **13 acceptance
tests**. The two new tests cover refusal followed by Send, Edit then Send, Withdraw,
unrelated setting and secret changes, unchanged Start rejection and exact admitted
resolution retention on retry. Workspace Clippy with warnings denied passed after
an iterator-style cleanup; formatting, whitespace, migration checksums/version
uniqueness, layout and local validation contract checks also passed.

The owner explicitly marked journaled-outcome projection ordering as not an action
item; no reachable failure was established, so that path remains unchanged. The
Send response contract is stated above and in the PR description. Published CI and
review checks still apply to the updated head.
