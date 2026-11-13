# Runtime secret admission and terminal memory — issue #695

Request: [issue #695](https://github.com/G5Labs/G5-Lean/issues/695) (audited base
`df1d0999`). The issue text is the intent source; the owner's chat message only set the
workflow (design, then implementation, then a PR). No separate chat intent is recorded.

## Ontology changes

| ID | Change |
|---|---|
| [`g5.chat.secret-admission`](../../ontology/features/chat.g5.md#g5.chat.secret-admission) | New (agent, I). One exportable-secret policy shared by Runtime Config writes, agent/command dispatch and terminal admission; dispatch admits before reserving; an invalid stored configuration settles its task visibly and the pass continues. |
| [`g5.chat.secret-admission.kept-input`](../../ontology/features/chat.g5.md#g5.chat.secret-admission.kept-input) | New (agent, D). The refused turn keeps its queued input; the owner's next message runs it. |
| [`g5.chat.secret-admission.reconcile`](../../ontology/features/chat.g5.md#g5.chat.secret-admission.reconcile) | New (agent, D). An unconfirmed attempt whose set turned invalid re-invokes with withheld entries and a preflight error; the guest decides. |
| [`g5.terminal.secret-admission`](../../ontology/features/terminal.g5.md#g5.terminal.secret-admission) | New (agent, I). The API and the guest refuse an invalid terminal secret set with a structured, browser-visible reason; valid stored configurations open. |
| [`g5.terminal.secret-admission.loader-vars`](../../ontology/features/terminal.g5.md#g5.terminal.secret-admission.loader-vars) | New (human-verified, D). Loader variables stay unreserved (#535/#576). |
| [`g5.terminal.output-retention`](../../ontology/features/terminal.g5.md#g5.terminal.output-retention) | New (agent). Retained terminal output is two bounded windows; streaming never accumulates; a polling reader is told when earlier output was not retained. |

Governing and unchanged: [`g5.chat.dispatch`](../../ontology/features/chat.g5.md#g5.chat.dispatch)
(reserve without network calls, commit, invoke, apply),
[`g5.chat.dispatch-secrets`](../../ontology/features/chat.g5.md#g5.chat.dispatch-secrets)
(validated secrets injected into agent.turn),
[`g5.chat.dispatch-eligibility`](../../ontology/features/chat.g5.md#g5.chat.dispatch-eligibility)
(a waiting input must not block eligible independent sessions — the progress contract the
issue cites), [`g5.chat.credentials`](../../ontology/features/chat.g5.md#g5.chat.credentials)
(credentials resolve for the current owner at dispatch, so secrets are re-read when an
unconfirmed attempt is rebuilt), [`g5.terminal.credentials`](../../ontology/features/terminal.g5.md#g5.terminal.credentials)
(platform-owned environment keys cannot be overridden — the new terminal block names them),
[`g5.runtime.terminal.attachment`](../../ontology/features/terminal.g5.md#g5.runtime.terminal.attachment)
(replay window and explicit reset, which the polling window now mirrors),
[`g5.security.guest-authority`](../../ontology/concepts/security.g5.md#g5.security.guest-authority)
(owner-supplied loader variables are not a boundary against guest root; untouched, per the
issue's explicit exclusion of #535/#576).

## Design

### A. One exportable-secret policy (owner: `g5-contracts`)

Today three rule sets disagree: the API dispatch builder (uppercase name ≤128 bytes, value
≤1 MiB, no count limit), the guest module parser (same, count ≤128), and the guest terminal
opener (mixed-case `is_exportable_task_env_key`, reserved prefixes/keys, key ≤256, value
≤64 KiB, no NUL, ≤128 entries, and the whole `open` line ≤64 KiB). The config writer enforces
none of them. A stored key that one consumer accepts and another refuses is the root cause of
both the dispatch stall and the silent terminal failure.

| Option | Verdict |
|---|---|
| **A1. One policy in `g5-contracts`, used by the writer and every consumer** | **Chosen.** Key-local rules (name, reserved, value size, NUL) are the same everywhere; aggregate rules (count, serialized size) are bounded so the terminal `open` line and the module RPC line both fit. |
| A2. Loosen the terminal guest to the dispatch limits | Rejected: the `open` line would need to carry up to 128 MiB; the mux bounds buffered lines. |
| A3. Keep per-consumer rules and only make refusals visible | Rejected: "valid configurations work" requires that a configuration accepted at write time opens a terminal and dispatches. |

The policy (`g5_contracts::types::task_env`):

- name: `[A-Z_][A-Z0-9_]*`, 1–128 bytes, not a reserved key. Reserved: the `G5_`/`G5D_`
  prefixes and the existing list, extended with the platform-owned entries the dispatcher
  installs for every owner process (`GIT_AUTHOR_NAME/EMAIL`, `GIT_COMMITTER_NAME/EMAIL`,
  `GIT_CONFIG_GLOBAL`, `GIT_CONFIG_NOSYSTEM`, `GIT_TERMINAL_PROMPT`, `CODEX_HOME`,
  `CLAUDE_CONFIG_DIR`). This is the optional item: refusing these keys everywhere keeps the
  terminal's Git identity, Git configuration and provider homes platform-owned without a new
  ordering rule, and without touching loader variables.
- value: ≤64 KiB, no NUL (an environment value cannot contain NUL; the guest terminal already
  refused it, the module path would fail at spawn).
- count ≤128 entries; serialized JSON object ≤256 KiB. The terminal `open` line limit becomes
  this budget plus a small fixed headroom (one constant in `g5_contracts::terminal`), so a
  policy-valid set always fits the frame. The module RPC line limit (tens of MiB) is far above
  the budget plus the prompt limit.
- The retired `CODEX_AUTH_JSON` key is still removed before any consumer, as today.
- Errors are a small enum with a stable code and a human sentence naming the key and the
  limit; the sentence is what the owner sees in the config editor, the chat notice and the
  terminal status.

**Capacity and the scope merge.** Effective secrets are the union of org, workspace and user
scopes. Key-local rules hold for any union, so they are final at write time. Count and size
are checked for the written scope alone at write time (a single scope can never exceed the
budget) and again on the merged set at admission, where they are final: three full scopes can
still exceed the count, and that case is refused visibly rather than guessed at write time
(an org write would otherwise have to re-validate every workspace and user combination).

### B. Configuration writes validate (owner: `ConfigService`)

`merge_secret_maps` already refuses the retired key. It now validates each `set_secrets`
entry with the policy and the resulting scope map's count and serialized size, before
encryption. Removals never fail. The route maps the error to a validation error whose message
is the policy sentence; the Runtime Config editor already displays the server message.

### C. Dispatch admits before it reserves (owner: `TaskService` dispatch, `agent_dispatch.rs`)

Today `build_agent_dispatch` and `build_command_dispatch` resolve the effective config and
validate secrets *after* the reservation is written; the error propagates out of
`reserve_*_dispatch`, the tenant loop logs and `break`s, and every later pass picks the same
oldest candidate again. The attachment preflight and the admission failures that already
settle through `settle_dispatch_admission_failure` show the intended shape.

| Option | Verdict |
|---|---|
| **C1. Validate the effective secrets inside the queued admission block (agent and command), before the reservation write; settle a violation with `settle_dispatch_admission_failure`** | **Chosen.** No reservation, attempt row or reserved input is created; the loop continues to the next candidate; the task row records `dispatch_admission_failed` with the policy sentence. |
| C2. Carry a `secret_preflight_error` to the guest like attachments | Rejected: the guest would launch an agent without the secrets it was configured to have, spending a slot to fail. |
| C3. Drop the invalid entries and launch with the rest | Rejected: a turn running without a configured credential is the silent failure the issue forbids; and it is product policy. |

**Visibility.** A settled admission failure that needs the owner's correction
(`requeue_after_admission_failure == false`: unsupported permission policy today, invalid
secrets now) also records an assistant `error` message in the chat with the policy sentence,
through the existing failure-turn recorder used by Codex authentication failures (no user
rows: the input stays `queued`). The queued input stays attached to the failed task, like
the permission-policy precedent; automatic requeue would fail again every pass and litter the
session with failed tasks. Enqueue completes the correction path: when the owner's next message
creates a fresh turn, the queued messages of the session's failed turns rebind to it and run
ahead of the new message (today only a turn held for Continue rebinds, so the kept input would
otherwise stay orphaned until withdrawn). Transient admission failures keep their current rebinding and
record nothing.

**Unconfirmed attempts.** `load_unconfirmed_*_dispatch` rebuilds the request for an attempt
whose invoke was never confirmed, re-resolving secrets (they rotate at dispatch). If the
configuration became invalid between passes (a bypassed writer, or a policy that tightened
across a deploy while the attempt stayed unconfirmed), the build must still reconcile an invoke
that may already have reached the guest, and reservation makes no network calls, so it cannot
ask the guest first.

| Option | Verdict |
|---|---|
| **C4. Rebuild with the offending entries withheld and a `secret_preflight_error`, exactly like the attachment preflight** | **Chosen.** The guest answers from its own state: an execution it already runs confirms (`already_running`/`already_finished`) and ignores the retried params; one it never saw starts and fails the turn before the provider runs, with the sentence as its outcome code, and the loop records the sentence in the chat only on that `started` disposition. |
| C5. Settle the attempt with the existing `cancelled` start disposition | Rejected: a lost invoke response does not mean nothing runs; cancelling at the API would orphan a turn the guest is executing. |
| C6. Probe `modules.status` inside the reservation | Rejected: violates reserve-without-network (`g5.chat.dispatch`). |

For the whole-set violations (count, size) nothing is carried; otherwise each entry that passes
on its own is. Commands have no preflight channel, so the withheld entries are logged. The
queued input of such an attempt follows the failed turn's ordinary settlement.

A settled candidate is finished work, not the end of the tenant's pass: the reservation tries
the next queued candidate in the same transaction (bounded to eight settlements per call), so
the later independent session dispatches in the same pass rather than one tick later.

The admission notice carries no task attempt (chat messages bind to attempts in pairs, and a
task refused at admission has none); its id is the canonical one for the task's missing first
attempt, so a repeated settlement cannot duplicate it.

The agent and command paths share one helper that resolves the effective secrets and validates
them, so they cannot drift.

### D. Terminal admission answers structurally (owners: API `terminal_ws_adapter`, guest `open_pty`, browser hook)

- **API producer.** After resolving the owner's effective secrets it validates them with the
  policy. On a violation it still upgrades the WebSocket, sends the guest-shaped control line
  `{"type":"rejected","reason":"secrets_invalid","detail":"<sentence>"}` and closes; the guest's
  PTY stream, opened by the authorized owner-stream step the route-auth audit requires before the
  terminal id is read, closes without an open frame. It also refuses an `open` frame above the frame limit the same
  way (defensive; cannot happen for a policy-valid set).
- **Guest consumer.** `open_pty` replaces its ad-hoc checks with the same validation and reads
  the `open` line with the shared frame limit. A violation answers `rejected` with
  `secrets_invalid` and the sentence before closing, instead of closing silently. The reserved
  workspace-token test that today expects a silent close expects the rejection line.
- **Browser.** The hook already stops reconnecting on `rejected`. It now reads the reason:
  `terminal_unavailable` (or none, older guests) keeps "Session ended"; `secrets_invalid`
  shows "Runtime Config secrets refused" with the detail sentence and offers Retry, so the
  owner can fix the configuration and reopen without a reload.
- `TerminalServerMessage::Rejected` gains an optional `detail`; `TerminalRejection` gains
  `SecretsInvalid`. Older browsers ignore the new field; older guests never emit it.

### E. Bounded terminal output retention (owner: `TerminalSession`)

The session keeps three stores: the 4 KiB replay ring, the 64 KiB polling ring, and an
unbounded `drain_buffer` string that every path appends to and only `drain_output` empties.
The browser streaming path never drains it, so a verbose shell grows dispatcher memory
without bound. The polling ring is written but never read.

| Option | Verdict |
|---|---|
| **E1. Remove `drain_buffer`; `drain_output` reads the polling ring from a per-session drained cursor (`replay_from`, as reattachment does)** | **Chosen.** One bounded store serves the polling reader; the streaming path's bookkeeping is unchanged; memory is bounded by the two configured windows. |
| E2. Cap `drain_buffer` and drop the oldest | Rejected: a third copy of the same bytes for nothing. |

A polling read returns the retained bytes since the cursor. When the cursor fell out of the
window, the result is the retained tail prefixed by one notice naming how many bytes were not
retained, mirroring the reattachment reset. Shutdown clears the rings and advances the cursor
so a later read is empty. `get_output` on the manager keeps its signature.

### Interaction sketch

1. Owner saves a secret → policy → refused with the sentence, or stored.
2. Dispatch pass, org X: candidate T1 (stored-invalid workspace config) → admission validates
   the effective map → `settle_dispatch_admission_failure` + chat notice → `Ok(None)`,
   commit → loop continues → candidate T2 (other workspace, valid) → reserved, invoked.
3. Owner opens a terminal → API validates the effective map → `rejected{secrets_invalid}` to
   the browser, which shows the sentence and Retry; otherwise the guest validates the same set,
   attaches, and answers `opened`.
4. A long `yes` in the terminal → streaming reads push each chunk through the broadcast and
   both rings; nothing else retains it.

## Assumptions and acceptance conditions

- The 64 KiB value and 256 KiB serialized budgets are judgment calls bounded by the terminal
  frame; they are well above any real credential (keys, PEMs, kubeconfigs, service-account
  JSON). A stored value above 64 KiB that dispatched before now fails visibly at admission and
  in the editor on rewrite; the issue asks for compatible limits end to end.
- Guest skew: a guest older than this change still closes silently on an invalid set it
  receives, but the API now never sends one; an older guest's 64 KiB `open` line can still cut
  a policy-valid set between 64 KiB and the new budget until the guest is updated.
- Direct RPC injection into the guest's module path (used by dispatcher tests for `HOME` and
  `G5_TEST_*` names) is unchanged: the guest module parser stays compatible with, and looser
  than, the producer. The module runner's secrets-after-platform ordering is therefore reached
  only through the validated API path.
- The chat notice is written for correction-required admission failures only; the UI shows
  it on its next session read (no new live event).

## Tests

- `g5-contracts`: policy boundaries (name 128/129, count 128/129, value 64 KiB ±1, serialized
  budget ±1, NUL, reserved keys including the new platform-owned ones, lowercase).
- `g5-api-core` unit: writer boundaries through `merge_secret_maps` (set, update, remove
  crossing the count boundary); dispatch validation helper.
- `g5-api` integration: config routes refuse with the sentence; a stored-invalid workspace
  config (seeded through the encryption manager, bypassing the writer) makes the oldest
  task settle `dispatch_admission_failed` with a chat notice while `run_once` dispatches a
  later valid session's turn to the runtime peer; an unconfirmed attempt whose config turned
  invalid is released and then settled; terminal WebSocket with a stored-invalid config
  receives the `rejected` control line and opens no PTY stream; a boundary-valid set reaches
  the peer's `open` frame.
- `g5d` dispatcher: `open` with an invalid set receives `rejected{secrets_invalid}`; a
  128-entry boundary-valid set at the value limit opens; the reserved-token case expects the
  line.
- `g5d-foundation` terminal: sustained output far beyond both windows through `read_raw`
  leaves retained bytes ≤ the two windows (fails before the fix on `drain_buffer`); polling
  after overflow returns the tail with the notice; polling without overflow is lossless.
- Frontend: hook test for `rejected` with `secrets_invalid` and detail; panel shows Retry.

## Implementation record (2026-10-22)

Implemented as designed, with two discoveries folded into the design above during
implementation: the attempt-free chat notice (chat messages bind attempts in pairs through a
foreign key, so a notice for a task refused at admission must carry neither) and the
settled-candidate loop (a candidate settled at admission previously ended the tenant's pass
for one tick; the reservation now tries the next candidate in the same transaction). The
unconfirmed-attempt reconciliation uses the attachment-preflight pattern rather than the
cancelled disposition first sketched, because a lost invoke response does not mean nothing runs.

Code: `g5-contracts/src/types/task_env.rs` (policy, new), `task_context.rs` (reserved keys),
`terminal.rs` (frame limit, `SecretsInvalid`, `detail`); `config_service.rs`
(`merge_secret_maps`); `agent_dispatch.rs` (`validate_dispatch_secrets`, `dispatch_secret_set`,
`reconciled_dispatch_secrets`, `QueuedCandidate`, `record_admission_refusal_notice`,
`record_secret_preflight_notice`, enqueue rebinding); `agent_tasks.rs` (notice after a fresh
start); `errors/mod.rs` (editor-visible sentence); `routes/mod.rs` (terminal admission);
`g5d-dispatch/src/server.rs` (`open_pty`); `g5d-agent/src/types.rs` (`secret_preflight_error`);
`g5d-foundation/src/terminal/session.rs` (`PollingBuffer`); `useWorkspaceTerminalSocket.ts`,
`WorkspaceTerminalPanel.tsx`.

Evidence, all on this branch:

- `cargo test -p g5-contracts --lib -- task_env terminal`: 10 passed (policy boundaries and the
  protocol shape). `cargo test -p g5-api-core --lib`: the 6 writer/dispatch tests pass
  (`secret_writes_follow_the_shared_policy_and_removals_always_succeed`,
  `dispatch_secret_keys_are_environment_names`, `a_formal_review_admits_only_the_credentials_it_carries`,
  `a_reserved_attempt_reconciles_with_the_entries_that_still_pass`, and the two retired-key tests).
- `cargo test -p g5-api --test integration -- api_runtime_secret_admission::`: 4/4 against a local
  Postgres 17 (`config_writes_refuse_policy_violations_with_the_reason`,
  `a_stored_invalid_secret_settles_its_turn_visibly_and_later_sessions_dispatch`,
  `an_unconfirmed_attempt_whose_secrets_turned_invalid_reconciles_with_a_preflight`,
  `the_terminal_refuses_a_stored_invalid_set_with_its_reason_and_opens_a_valid_one`). Related
  suites `api_task_lifecycle::`, `api_chat_turn_stop::`, `api_terminal_activity::`,
  `api_config_secret_hints::`, `api_member_activity::`, `api_workspace_messages::`,
  `api_workbench_chat::`: 146 passed, plus the 31 of the turn-stop/admission rerun after the
  passed-over-turn test was updated (that turn now starts in the same pass; the scripted peer
  reports any started execution as unknown, so a second pass would have reconciled it as lost).
- `cargo test -p g5d --test dispatch -- terminal`: 9/9 including
  `terminal_open_admits_secrets_under_the_shared_policy` (128 entries, one at the value limit, an
  open line past 64 KiB opens and the shell sees the values; eight violations answer
  `rejected`/`secrets_invalid` with the sentence) and the reserved-token case now expecting the
  line. `cargo test -p g5d --test terminal_pty`: 19/19.
- `cargo test -p g5d-foundation --lib -- terminal::session::tests`: 23/23 including
  `streaming_output_is_retained_only_within_the_bounded_windows` (1 MiB through `read_raw`, retained
  bytes within 4 KiB + 16 KiB, the poll reports the loss, a later small poll is lossless).
- `cargo clippy --tests -- -D warnings` on the eight changed crates and `cargo fmt --all --check`:
  clean. Frontend: `vitest run src/components/execution/` 26/26 (two new cases), `tsc --noEmit`
  and `eslint` on the changed files clean.

Gaps: the CI Docker/VM E2E lane and `g5-terraform-plan-qa` did not run here; the full `vitest`
suite was not run (the three execution-panel files were, 26 tests). No live guest ran the
reduced re-invoke end to end (the API side is covered; the agent runtime's refusal is covered by
its `validate` unit test, the same path as the attachment preflight). The retention regression
cannot compile against the old session (the accessor is new), so its discriminating power rests
on the bound it asserts: an unbounded buffer counted by the accessor would fail it.
