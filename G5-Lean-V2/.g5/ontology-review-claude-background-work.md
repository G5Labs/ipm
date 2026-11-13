# Claude background work implementation review

## Scope and review mode

The pinned design is [ontology-plan-claude-background-work.md](ontology-plan-claude-background-work.md); this review starts from its `Ontology changes and governance` inventory and the saved [request](chat_intent/claude-background-work/request.md). The scope is the whole branch against `origin/main` at `967bf34c`: the design commit, the Claude Code 2.1.285 and Codex 0.159.3 upgrade commits, and the behavior commit (no default time limit after input acceptance, the Claude background window with report turns and events, the owner's End, Stop availability, the waiting panel and the continuation wording).

The implementing session first reviewed its own work. Three fresh, full-scope, read-only reviews then ran in parallel on the committed candidate, followed by the independent Codex CLI review required for shipping.

## Initial self-review findings

Found and fixed before the candidate was committed; each fix has a regression check that fails without it.

1. **End fails the attempt when Claude already left.** Writing the interrupt for an End to a provider that had just exited raised a broken-pipe error, and the supervisor turned it into `agent_turn_failed`; closing input had the same failure mode. Repair: a failed interrupt write closes input and ends the wait; closing input tolerates a closed pipe. Check: the `deaf` case of `agent_turn_owner_end_stops_claude_background_wait` (provider stops reading its input while background work is reported) settles `interrupted` with the repair and `agent_turn_failed` without it.
2. **A late interrupt reply fails the attempt.** Input closes 10 seconds after the End interrupt if Claude has not answered. A reply arriving after that no longer matched the waiting state and was classified as an unsupported control frame, failing the attempt. Repair: the supervisor remembers the interrupt it issued and accepts the reply in any later phase. Check: the `late` case of `agent_turn_owner_end_stops_claude_background_wait` (provider answers after 11 seconds) fails with the old guard (`failed`) and succeeds with the repair.

## Independent discovery, round 1

Three fresh, full-scope, read-only reviews (A, B, C) ran in parallel on candidate `5ef6291b`, each in its own detached worktree, and saved their reports before sharing (session scratch `review-{a,b,c}.md`). None found a blocker; each judged the core design faithfully implemented and every suite it ran passed. A and B ran the full macOS dispatcher process suite (49 tests); C reproduced three defects with the built supervisor and fake providers. Consolidated findings, before repair, with the reviewers that raised them:

1. **Post-exit output drain is unbounded (C major, B).** After the provider exits, the supervisor waits for stdout and stderr to close with no bound; a descendant that inherited the provider's stdio holds the attempt open. C reproduced a 30-second delay with a sleeper; a long-lived descendant never settles. The removed default deadline was the only bound. Violates `g5.conversation.providers.claude-background.untracked` and the bounded-cleanup rule.
2. **Unattended workflow and bootstrap agent tasks lost every time limit (A major, B, C).** `job_service` writes empty limits, which now mean no limit. The approved design says such tasks should request an explicit limit; nothing does.
3. **The default scope also drops the catalog's 24-hour bound (B major).** This contradicts the agent-authored `g5.agents.execution-limits` ("an absent request cannot relax a catalog or policy bound"), which the plan listed as preserved. The human-verified `.deadline` ("No platform time limit ends background waiting") and `owner-stop` ("self-hosted and local hosts have no such bound") support the implementation, so the agent block is the one out of line.
4. **End is deferred while any native request is pending and ignored after input closes (C, B, A).** A background sub-agent's pending request blocks End until it is answered or expires; End after a normal input close does nothing. C reproduced the second case.
5. **End's interrupt can race a report turn Claude just started (C, B).** The aborted turn's error result fails the attempt instead of the End outcome. The timing is a hypothesis; the error-result path is confirmed.
6. **A turn without a result can count as success (B).** Success ignores a follow-up turn that started and never produced a result before exit.
7. **Background sub-agent frames are projected as the main reply (C, A).** The mapper ignores `parent_tool_use_id`; the sub-agent's tool calls and text become the reply's rows and content, are persisted into the assistant message and replayed as history, and consume the timeline window. C reproduced it with the recorded interrupt session.
8. **The continuation wording adds a restart instruction (C, B).** "start them again if their results are still needed" goes beyond the required fact and can restart work the owner stopped.
9. **The waiting panel depends on the newest 200 timeline rows (A).** The set is emitted only on change, so after a reload during a long wait the panel, elapsed time and End can disappear.
10. **"Elapsed wait" is the oldest listed task's age (B, C).** It resets when the oldest task ends and can omit truncated tasks.
11. **The default setup clock no longer includes the dispatcher's Git refresh (A).** Up to about two extra minutes.
12. **Input acceptance is recorded after the `input.accepted` emit (B).** A negligible window in which an accepted input could still time out.
13. **Model-list evidence covers only subscription and ChatGPT sign-ins, and the list tests are not tied to the pins (A, B, C).**
14. **Saved selections of withdrawn Codex models fail on ChatGPT sign-ins (A, B).**
15. **`.deadline` does not mention a requested limit among the ways a wait ends (A).**
16. **Tests missing for accepted behavior (A, B, C):** the 3-second settle, a second-turn error, a Codex requested limit, a Claude wait outliving the default deadline, a requested limit ending a wait, Stop during a report turn and during its pending request, invalid-scope rejection, and the next queued input after End.
17. **A bounded End holds the task as `owner_stop` (A, B, C).** Consistent with `.end` ("a bounded stop settles as interrupted"), but the plan's acceptance sentence reads unconditionally.
18. **Pre-deploy chat tasks keep a one-hour limit (A, B, C).** The old default serialized `wallClockMs: 3600000`, which now reads as a request.
19. **Nits:** End is shown and deferred while Claude is mid-turn (A); a late End shows an error toast (B, C).
20. **Ontology footers are stale (A, B, C)**, and `g5.provider.catalog.pinned` names the frontend lists instead of the backend catalogs (B).

Noted as out of scope: the supervisor's control channel is reachable by same-uid tools (a pre-existing security hypothesis, C); long background sub-agents consume the attempt's output budget by design (C); info-level notices now render neutrally (C); Stop and Cancel discard the module outcome, so completed turns' usage is not billed (pre-existing, found by the author).

### Dispositions before repair

- Repair in code with regression checks: 1, 2 (workflow and bootstrap specs request the previous one-hour limit, as the design's step 2 and risk text direct), 4, 5 (an aborted result after the adapter's own interrupt is End's effect; a clean exit then succeeds with the notice), 6, 7 (drop frames of background sub-agents identified by `task_started` with `is_backgrounded`), 8, 9 (the session's live task carries the latest set), 10 (the adapter reports when the wait began), 11, 12, 13 (tests assert the pins they were checked against), 16.
- Reconcile ontology: 3 (amend `g5.agents.execution-limits` to the human-verified decisions), 15 (add the requested-limit reference to `.deadline`, from the approved design's step 2), 1 (state the output-closure rule after exit in `g5.agents.outcome`), 17 (state in `.end` that a bounded stop holds the task like Stop), 20.
- Evidence only, no change: 14 (Codex 0.153.3 already rejected all three withdrawn models for ChatGPT accounts, probed 2026-09-30, so un-offering them adds no failure; API-key selections still dispatch because launch does not enforce `offered`). For 13, every offered OpenRouter slug is on OpenRouter's public model list (2026-09-30); API-key accounts remain unprobed.
- Accepted with a record: 18 (transitional; only tasks not yet ended at deploy keep the hour), 19.

## Round 1 repair and recheck

Each repair was checked at its consumer. "RED" means the regression check failed against the pre-repair candidate `5ef6291b` (its supervisor swapped back in, or the repaired line reverted) and passes after the repair.

- **1. Bounded drain after exit.** After the provider exits, the supervisor waits at most five seconds for its output to close, then stops the provider's process group and settles from the evidence. `agent_turn_claude_window_edges` (`drain`: a sleeper holding the provider's output) settles in seconds and the sleeper is gone; RED: the `open_turn` case, whose leftover reader holds the output, never settled on `5ef6291b`, and reviewer C reproduced the `drain` delay there. `g5.agents.outcome` now states the rule.
- **2. Unattended limit.** `job_service` workflow and bootstrap agent specs request `wallClockMs: 3600000`. `profile_workflow_agent_task_requests_the_unattended_time_limit` drives the real `JobService` profile path; the request maps to scope `attempt` (`requested_wall_clock_limit_bounds_the_whole_attempt`). The bootstrap-graph path uses the same constant but has no separate test.
- **3. Catalog bound.** `g5.agents.execution-limits` amended; no code change. The approved design and the human-verified blocks already state that self-hosted and local hosts have no time bound.
- **4. End robustness.** End now waits only for Claude's own turn: a pending sub-agent request no longer blocks it and is invalidated (`background_wait_ended`) when input closes, and an End after input closed applies the bounded stop. `agent_turn_owner_end_edges`: `pending` succeeds with the invalidation (RED: `suspended` by the request timeout); `after_close` settles `interrupted/background_wait_ended` (reviewer C reproduced the ignored End on `5ef6291b`).
- **5. End-aborted turn.** An `error_during_execution` result after the adapter's own interrupt is End's effect. `agent_turn_owner_end_edges` (`race`) succeeds with the notice; by inspection the old code set `result_error` and failed.
- **6. Unfinished turn.** Success needs no turn left open at exit; otherwise `provider_turn_incomplete` and `provider_error`. `agent_turn_claude_window_edges` (`open_turn`); RED as above (on `5ef6291b` it never settled).
- **7. Background sub-agent frames.** The mapper drops frames whose `parent_tool_use_id` names a background launch (`task_started` with `is_backgrounded`). `exact_pin_interrupt_stops_background_agents_without_a_report_turn` asserts only the main agent's `Agent` tool on the recorded session; RED: `["Agent", "Bash"]` with the tracking disabled. Foreground sub-agents are unchanged.
- **8. Continuation wording.** Only the fact remains; `interrupted_task_continues_original_objective_without_resending_its_input` asserts the exact ending.
- **9. Panel after reload.** The session's live task carries the latest non-empty set (`TaskService::live_background_wait`); the panel and composer hint fall back to it when the loaded timeline lacks the report. `session_task_carries_the_live_background_set` (210 later rows, then an emptied set) and the ChatPanel test "keeps the panel from the session's live task once the report row has left the loaded timeline".
- **10. Elapsed wait.** The adapter reports `wait_started_at_unix_ms`; the panel counts from it (unit, projection and ChatPanel tests).
- **11. Setup clock.** Without a requested limit the dispatcher passes `G5D_SETUP_DEADLINE_UNIX_MS`, so its Git refresh stays on the default clock; `agent_wall_clock_scope_decides_the_settlement_deadline` also covers the invalid-scope refusal.
- **12. Acceptance timing.** The deadline lifts at the provider handoff (`input_handed`), before the acceptance is journaled.
- **13. Model lists.** The Codex and Claude list tests assert the pins they were checked against. Evidence: every offered OpenRouter slug is on OpenRouter's public model list; API-key accounts remain unprobed.
- **16. Added tests.** The 3-second settle (`settle`: the report starts one second after the set empties, and the fake skips it if input already closed), a failed report turn, a Codex requested limit (`codex-limited`), a four-second wait outliving a 1.5-second default, a requested limit ending a wait, Stop during a report turn and its pending request (`agent_turn_cancel_ends_a_background_wait`), and the invalid scope. The queued input running after a clean End is ordinary success settlement and has no separate test.
- **17, 15, 20.** `.end`, `.deadline` and `claude-followups` amended; footers updated.

Recheck on the repaired tree (macOS):
- Rust: `g5d-agent-turn` lib 56, `g5d-worker` lib 111, `g5d-dispatch` lib 116, `g5-contracts` lib 488, `g5-api-core` lib 1012; `dispatch_modules` 51/51.
- API (isolated Postgres): `api_native_requests` 5, `api_provider_accounts` 21, `api_task_lifecycle` 44, `api_workspace_vms` 38, `route_auth_static` 7.
- Frontend: 3607/3607 Vitest; `tsc` clean; ESLint 0 errors (warning count unchanged).
- `cargo fmt --check` clean; clippy `-D warnings` clean on every changed crate.

Remaining limits:
- Linux-only process checks (execution-owner reap, `agent_execution_tree_is_reaped_before_settlement`, the Linux pid assertions) run only in cloud CI.
- The QA live checks (a five-minute and a multi-hour background job, a Codex turn longer than an hour, one model per list) are unexecuted.
- API-key model acceptance is unprobed.
- `codex_auth::tests::cli_failures_and_timeout_end_the_wait_without_becoming_ready` failed twice in about 16 full-suite runs on the branch, at its lock-release check after the fake CLI is torn down. It passed 20/20 in isolation on the branch and 20/20 as part of the full suite at the base. The code under test is unchanged, so this is a pre-existing timing-sensitive test that the larger suite exposes. It is recorded, not fixed.

## Independent CLI review round 1

Codex CLI 0.158.0 with model `gpt-6-astra` at effort `xhigh` reviewed candidate `46554a63` read-only, in its own detached worktree; its report is in the session scratch (`codex-review-round1.md`). Verdict: changes required, for two major findings, one minor finding and one limitation that needed a design decision. Each finding was validated by tracing the code before repair.

1. **Major: VMs on the previous module catalog cannot run the new API's agent invocations.** Confirmed. The API always requested `agent.turn@1.0.2`, but a running VM booted with catalog `2026.09.29-agent-turn-recovery` serves only 1.0.1. The dispatcher's exact lookup refuses the invocation, maintenance keeps retrying it, and the chat stalls. Under the rollout rule ([design v9 §6.10](../docs/lean-backend-design-v9.md)) the control plane deploys first, so this would happen on every deploy.
   - Repair: the dispatch input targets the module contract of the VM's recorded catalog. A VM on the previous catalog gets 1.0.1 without `limits.wall_clock_scope`, and keeps the old one-turn behavior and attempt-wide default until a drain replaces it.
   - Check: `agent_dispatch_targets_the_module_contract_of_the_vm_catalog`: the previous catalog gets 1.0.1 without the scope, and the current catalog gets 1.0.2 with it.
   - Not run: an invocation against a live previous-catalog VM.
2. **Major: a delayed End can stop the next attempt.** Confirmed. The request named only the session, so TaskService picked whichever attempt was running when the request arrived.
   - Repair: End carries the `{taskId, attempt}` of the displayed set, taken from its journal row or from the live task's latest attempt. TaskService answers 409 when that attempt is no longer the running one. The UI offers End only for a set that names its attempt.
   - Checks: `owner_end_wait_reaches_only_the_running_attempt` (a stale attempt number and another task both get 409, and no dispatcher call is made), plus the ChatPanel and WorkspacePage End tests (request identity and body).
3. **Minor: End documentation promised the task is never held.** Confirmed. `useChat.ts`, the route and `task_lifecycle.rs` comments and Agent API §4.5 now separate the two outcomes: a clean exit succeeds and queued input runs next, while a bounded stop settles interrupted and holds the task for Continue.
4. **Limitation: End waits without bound for a running turn.** Confirmed: the reviewer's probe saw no interrupt for 27 seconds. Round 1 had accepted this as nit 19.
   - Decision, recorded as the new agent block `g5.conversation.providers.claude-background.end-bound`: End interrupts at once between turns, and after at most ten seconds during a turn. With no background work left, End does nothing more and the reply finishes normally.
   - The same rule applies after input has closed: there, End applies the bounded stop only while background work remains.
   - Checks in `agent_turn_owner_end_edges`:
     - `busy_turn`: a silent report turn is interrupted after its grace, and the attempt succeeds.
     - `drained`: End after the set emptied lets the report turn finish, with no End notice.
     - `late_report`: End during a turn that Claude runs after its input closed, with nothing in the background, lets the turn finish past the exit grace.
   - RED: with the turn grace removed, `busy_turn` never settled. With the earlier after-close rule, `late_report` settled `interrupted`.

The mid-turn interrupt was unrecorded at the pin. The author therefore ran four probes of Claude 2.1.285 on Linux and recorded `background-agent-interrupt-turn` (see Provider behavior evidence in the design). That evidence exposed two more defects:

5. **An End-aborted turn fails the attempt.** Claude exits 1 at EOF after an interrupt aborts a turn: all three mid-turn runs did, while idle interrupts exit 0. Round 1's `race` fake exited 0, so its earlier repair still settled `failed` with the real CLI.
   - Repair: exit status 1 after an End-aborted turn is End's effect. `CLAUDE_INTERRUPTED_TURN_EXIT_CODE` is asserted against the recording's exit code, and the `.end` body now says so.
   - Checks: `race` and `busy_turn` now exit 1 as the CLI does, and the `interrupt_turn` replay of the new recording succeeds with the End notice.
   - RED: without the rule, `race` settles `failed`/`provider_error`.
6. **Foreground commands were published as background tasks.** Claude 2.1.285 emits `task_started` and `task_notification` with `is_backgrounded: false` for long foreground commands, both its own and its sub-agents'. The mapper turned these into "Started background command" `subtask.*` events.
   - Repair: lifecycle events are emitted only for task ids that started backgrounded or appear in the background set.
   - Checks: `foreground_commands_claude_registers_as_tasks_are_not_background_work` (RED without the filter), `exact_pin_interrupt_during_a_turn_aborts_it_and_stops_background_agents`, and the `interrupt_turn` replay, which asserts that every `subtask.*` names the background agent.

Recheck on the repaired tree (macOS):
- Rust: `g5d-agent-turn` lib 58, `g5d-worker` lib 112, `g5d-dispatch` lib 116, `g5-contracts` lib 488, `g5-api-core` lib 1012; `dispatch_modules` 51/51.
- API (isolated Postgres): `api_native_requests` 5, `api_task_lifecycle` 45, `api_provider_accounts` 21, `api_workspace_vms` 38, `route_auth_static` 7.
  - The route-authorization audit flagged the new `taskId` in the End body. It is only compared with the running attempt that TaskService resolves from the owner-authorized session, so the audit now has a route-specific check for it (`service_level_background_end_guard_exists`), like the one for queued-input routes.
- Frontend: 3607/3607 Vitest; `tsc`, ESLint and Prettier clean.
- `cargo fmt --check` clean; clippy `-D warnings` clean on every changed crate.

Remaining limits, in addition to those of round 1:
- The rollout path is tested at the dispatch input only. No invocation against a live previous-catalog VM was run.
- End during a running turn is exercised with recorded and synthetic frames, not against a live attempt.

## Independent CLI review round 2

The same reviewer (Codex CLI 0.158.0, `gpt-6-astra`, effort `xhigh`) reviewed candidate `fa5f406d`. Its workspace sandbox now allowed local sockets, so it ran the dispatcher process suite itself: 20 `agent_turn_` tests passed. It also ran the PostgreSQL checks of the round-1 repairs, the frontend suites and nine fixture-integrity checks. Two sub-reviews ran in parallel. Verdict: changes required. All round-1 findings were confirmed resolved, but two major findings and one minor finding remained. The reviewer reproduced each, and each was validated against the code.

1. **Major: End could turn a genuine provider failure into success.** End's aborted-turn exception required only an issued interrupt and an `error_during_execution` result. So a refused interrupt followed by an unrelated error, with exit 1, settled `succeeded`.
   - Repair: a result counts as End's effect only when Claude did not refuse the interrupt and the result carries an `aborted_*` terminal reason. All recordings and probes at 2.1.285 show `aborted_tools` or `aborted_streaming`, and `exact_pin_interrupt_during_a_turn_aborts_it_and_stops_background_agents` asserts it. A refusal that arrives after an aborted result also voids the exception.
   - Check: `agent_turn_owner_end_edges` `refused`, which now settles `failed`/`provider_error`.
   - RED: with `fa5f406d`'s rule, it settled `succeeded`.
2. **Major: a blocked native reply defeated End's bound.** The supervisor awaited reply writes to Claude's stdin without a bound, inside the loop that also advances End and expires native requests. A provider that keeps stdin open without reading stalled everything until Stop or the API's 15-minute request expiry. The existing `deaf` case closes stdin, so it exercised a broken pipe rather than backpressure.
   - Repair: every write after the prompt, native replies and the End interrupt alike, must finish within 10 s (`PROVIDER_WRITE_TIMEOUT`). On failure or timeout the input is dropped, since a partial line may have been written, and the pending requests are invalidated with `provider_input_failed`. A `provider_input_failed` notice explains the lost reply.
   - When a native reply cannot be written, the provider then stops at once, as for an unanswered native request. The attempt settles `suspended` (`native_response_undeliverable`, held for Continue), or `interrupted`/`background_wait_ended` when End is in effect.
   - A first version left this case to the exit grace and settled it `failed`. That labelled a transport failure as a provider error and exposed a gap: End awaiting its interrupt reply never moved on once the input was gone. End now moves to its bounded stop whenever its input is dropped.
   - Check: `agent_turn_provider_that_stops_reading_cannot_hold_replies_or_end` reproduces the reviewer's case through the real dispatcher (three 60 KB permission requests, then three allow replies, with and without End first). Both modes settle within about 11 s with all three requests ended: `interrupted`/`background_wait_ended` with End, and `suspended`/`native_response_undeliverable` without.
   - RED: with `fa5f406d`'s supervisor, the End case never settled (60-second timeout).
3. **Minor: after a no-op End, the panel could stay at "Ending…" for a later wait.** Confirmed. When the old set had drained, End was dropped. If one refresh then brought the empty set and Claude's next batch together, the panel stayed mounted with End disabled.
   - Repair: the panel is keyed by its wait (task, attempt and the wait start the adapter reports), so a later wait gets a fresh panel and End.
   - Check: the ChatPanel test "offers End again for a later wait after a delivered End".
   - RED: without the key, End stays disabled.

Also changed: when End interrupts a running turn, its notice now says that the turn was stopped as well (asserted in `busy_turn`).

Not changed:
- Two `g5d-dispatch` watcher tests timed out in the reviewer's sandbox. Their files are unchanged, and both pass on the host, as does the full `g5d-dispatch` lib (116).
- Starvation of the control channel by continuous provider output, and unrecorded foreground-to-background transitions, remain hypotheses without a reproduction.
- The 2.1.285 `stop_task` control was probed: it stops one background task without aborting a turn, after which Claude reports in a new turn. It could become a gentler End mechanism later. The agreed design uses the interrupt.

Recheck on the repaired tree (macOS):
- Rust: `g5d-agent-turn` lib 58, `g5d-worker` lib 112, `g5d-dispatch` lib 116; `dispatch_modules` 52/52.
- Frontend: Vitest 3608/3608; `tsc`, ESLint and Prettier clean.
- `cargo fmt --check` and clippy `-D warnings` clean.
- No API code changed in this round; the API suites from round 1's recheck stand.

## Independent CLI review round 3

The same reviewer reviewed candidate `9d08a090` with three independent readers. Verdict: changes required. All round-2 findings were confirmed resolved: the refused-interrupt cases in every order of reply and result, the blocked-reply hang, and the stale "Ending…". Two new findings were reproduced against the built supervisor, both in round 2's write-failure handling, plus one documentation nit.

1. **Major: a failed reply discarded Claude's own completion.** Round 2's repair stopped the provider as soon as a reply could not be written. When Claude finished on its own while a reply was in flight, so the write met a closed pipe, the queued cancellations, report, result and usage were never read. The attempt settled `suspended`, or `interrupted` under End, and the task was held for Continue.
   - Repair: a failed reply still drops the input and invalidates the pending requests, but the supervisor keeps reading Claude's output. A clean finish then succeeds with its report and usage. A provider still running when the exit grace ends is stopped, settling `suspended` (`native_response_undeliverable`), or `interrupted` when End is in effect.
   - Check: `agent_turn_claude_finishing_while_a_reply_fails_keeps_its_result`. Claude opens three requests in a report turn, closes its input, then cancels them and finishes. It runs once with replies and once with End plus replies. Both succeed with the report and summed usage (12/23 tokens).
   - RED: with `9d08a090`, the replies case settled `suspended`, and the reviewer reproduced `interrupted` for End plus replies.
   - `agent_turn_provider_that_stops_reading_cannot_hold_replies_or_end` keeps the unresponsive case bounded: about 25 s per mode, through the write bound and the exit grace.
2. **Minor: a blocked reply could outlive its request's deadline.** A reply write that finished after the deadline was journaled as replied.
   - Repair: a reply write is also bounded by its request's deadline and any requested attempt limit. A reply cut off at the request deadline settles exactly as a native-response timeout (`suspended`, `native_response_timeout`).
   - Check: `agent_turn_native_reply_cannot_outlive_its_request_deadline` (two 60 KB requests, a two-second deadline, a provider that reads late).
   - RED: with `9d08a090` it settled `succeeded`.
3. **Nit: the documented invalidation reason.** A failed End interrupt write uses `background_wait_ended`, not `provider_input_failed`. Agent API §10.1 now distinguishes the cases.

The reviewer again saw the two `g5d-dispatch` watcher tests time out in its sandbox. They pass on the host, and their files are unchanged.

Before this round, the branch was also checked against the latest `main` (`35f9ad1c`) in a temporary merge. The merge had no conflicts, and `main`'s new ontology (host sizing, documents, deployment images) does not touch agent execution. On the merged tree the API suites passed (`api_task_lifecycle` 45, `api_native_requests` 5, `api_workspace_vms` 38, `route_auth_static` 7, `api_provider_accounts` 21), as did `g5-api-core` 1012, `dispatch_modules` 52/52, `tsc`, and 412 tests in the three affected frontend files.

Recheck on the repaired tree (macOS):
- `g5d-agent-turn` lib 58; `dispatch_modules` 54/54.
- `cargo fmt --check` and clippy `-D warnings` clean.
- The frontend and API are unchanged since round 3's recheck.

## Independent CLI review round 4

The same reviewer reviewed candidate `3ec3cc5b` with three independent readers. Verdict: changes required. The round-3 findings were confirmed resolved for their original scenarios: the completion modes kept their report and usage, and own-deadline expiry worked. One major finding, one minor finding and a documentation nit remained, all reproduced against the built supervisor. All three trace to one cause: the supervisor still awaited each reply write inside the loop, so while Claude was slow to read, nothing else was arbitrated.

1. **Major: a blocked reply write held cancellations and other deadlines.** Two cases share this cause.
   - Claude cancelled its answered requests and finished before their deadline, but the loop could not read that output. At the deadline, the cut-off write killed Claude and the report was lost.
   - An older, unanswered request expired while a newer reply's write was blocked, and its late cancellation then hid the expiry.
2. **Minor: deadline precedence after a delayed wakeup.** The write's error handler checked a later native deadline before an earlier attempt limit.
3. **Nit:** the docs omitted the attempt-limit case and said the exit grace runs only while Claude is idle.

Repair: Claude's input after the prompt is now a queue (`ProviderInput`). The loop writes it through a cancel-safe `select!` branch, so Claude's output, control lines and every clock run while a write is pending.
- A reply counts as delivered only once completely written. Until then its request stays pending, with its deadline, and Claude can cancel it; a reply not yet started for a cancelled request is dropped.
- The clock decides expiry in one place (`expired_clock`), with the attempt limit before native deadlines.
- Writing with no progress for 10 s, or into a closed pipe, drops the input. A lost reply then follows round 3's rule: Claude's own output settles the attempt, and the exit grace bounds a provider that keeps running. A lost End interrupt leads to End's bounded stop.
- The round-3 `NativeDeadline` path is gone, and §10.1 now describes the queue, both invalidation reasons, the attempt-limit precedence and when the exit grace runs.

Checks:
- `agent_turn_blocked_replies_leave_cancellation_and_deadlines_running` runs two cases:
  - `cancel`: two 60 KB requests with a two-second deadline; Claude cancels both after a second and finishes. The attempt succeeds with its usage, and the blocked reply never counts as delivered.
  - `expiry`: an unanswered request expires while two answered replies wait, and Claude retracts it before reading. The attempt settles `suspended`/`native_response_timeout`.
- `an_expired_attempt_limit_wins_over_a_native_deadline` (unit) pins the precedence.
- RED with `3ec3cc5b`'s supervisor: `cancel` delivered the cancelled reply, and `expiry` settled `succeeded`.
- The earlier write-path tests still pass under the queue: stopped reading (both modes), finishing while a reply fails (both modes), the native deadline case, all End edges, and the recorded replays.

Recheck on the repaired tree (macOS):
- `g5d-agent-turn` lib 59, `g5d-worker` lib 112, `g5d-dispatch` lib 116; `dispatch_modules` 55/55.
- `cargo fmt --check` and clippy `-D warnings` clean.
- The frontend and API are unchanged since round 3's recheck.

## Independent CLI review round 5

The same reviewer reviewed candidate `4a3aae78` with independent readers. Verdict: changes required. Both round-4 findings were confirmed resolved for their counterexamples, and Stop during a blocked write was bounded. Two major findings and a schema nit remained, each reproduced against the built supervisor with matched controls.

1. **Major: a queued End aborted the report after the work had finished.** End queued behind blocked replies became `Interrupting` at once, and the no-background check applied only to a requested End. So the unstarted interrupt was written after Claude had emptied its background set, and it aborted the report turn.
   - Repair: End is now `InterruptQueued` until the interrupt's first byte is written, and `interrupt_started` moves it on. Only then is the notice journaled, the reply wait started and an aborted turn attributed to End. An interrupt still unstarted is withdrawn when the background set empties.
   - Check: `agent_turn_owner_end_queued_behind_replies_yields_to_finished_work`. End waits behind three 60 KB replies while the agent's requests are withdrawn and Claude starts its report. The report finishes, End journals nothing, and no interrupt reaches Claude.
   - RED: with `4a3aae78`, the interrupt aborted the report.
2. **Major: late events could erase an expired request.** Expiry was checked only in the lowest-priority tick. After a paused supervisor resumed, the pending reply write completed after its deadline and cleared the request. With event output held back, a cancellation Claude sent after the deadline was read before the tick.
   - Repair: the earliest clock is its own `select!` branch, ahead of provider output, control lines and writes (`next_clock`), with the attempt limit first (`expired_clock`).
   - Check: `agent_turn_expired_request_is_settled_before_late_events` runs the supervisor binary directly, as the dispatcher does. One case stops it across a one-second deadline while Claude reads its input; the other holds back its event output behind a 500 KB unit while Claude retracts the expired request. Both settle `suspended`/`native_response_timeout`, and the late reply is not written.
   - RED: with `4a3aae78`, both settled `succeeded`.
3. **Nit:** the event schema's `provider_request.invalidated` reasons now include `background_wait_ended` and `provider_input_failed`. §10.1 also describes when End takes effect and the clock's priority.

Recheck on the repaired tree (macOS):
- `g5d-agent-turn` lib 59, `g5d-worker` lib 112; `dispatch_modules` 57/57.
- `g5d-dispatch` lib 116, except that `watcher::tests::excessive_native_depth_keeps_rescanning_until_watch_coverage_recovers` failed once under the full suite.
  - It passed 6/6 in isolation and 116/116 in the next two full runs.
  - Its file is unchanged on this branch, and it is the watcher test that also timed out in the reviewer's sandbox.
  - It is recorded as a pre-existing timing-sensitive test, like the `codex_auth` one.
- `cargo fmt --check` and clippy `-D warnings` clean. The frontend and API are unchanged since round 3's recheck.

## Independent CLI review round 6

The same reviewer reviewed candidate `ae2105b9`. Verdict: changes required. Round 5's queued-End fix and both of its original deadline counterexamples were confirmed, as was the schema nit. Two major findings and a test defect remained, each confirmed with reproductions and scratch mutations.

1. **Major: progressing replies could postpone End for minutes.** `InterruptQueued` had no bound of its own. Every byte of an earlier reply reset the stall timer, so with ten queued 60 KB replies read at 4 KiB/s, End had not taken effect after 36 seconds.
   - Repair: a queued interrupt that has not started after `END_INTERRUPT_REPLY` (10 s) makes End take effect anyway. The notice is journaled, the pending requests are invalidated (`background_wait_ended`), the input is dropped, and the bounded stop follows. Withdrawal when the work ends first still comes before this bound.
   - Check: `agent_turn_owner_end_is_bounded_behind_slowly_read_replies` (ten replies, Claude reading 4 KiB a second, native deadlines and the wall clock at 120 s). It settles `interrupted`/`background_wait_ended` within the bound.
   - RED: with `ae2105b9`, no settlement within 45 seconds.
2. **Major: End could still erase an expired request after slow event delivery.** A handler blocked on event delivery could span both a native deadline and End's reply wait. When it returned, `advance_claude_input` invalidated the expired request as `background_wait_ended` before the next clock check, and the attempt succeeded.
   - Repair: after every handler, Stop and the clocks are checked before End or the input window act.
   - Check: a third case in `agent_turn_expired_request_is_settled_before_late_events`. End is sent once Claude has reported its work and opened a two-second request; the interrupt goes unanswered while a 500 KB unit holds the event output for 13 seconds. The attempt settles `suspended`/`native_response_timeout`, and End's notice is still journaled.
   - RED: with `ae2105b9`, the attempt settled `succeeded`.
3. **Minor: the End test's background reader read `/dev/null`.** A non-interactive shell gives background lists `/dev/null` as stdin, so the reader never drained the replies, and the test could pass a faulty mutant.
   - Repair: the reader now takes Claude's stdin (`<&0`), and the test asserts that a reply was drained.
   - RED: the repaired test fails with `4a3aae78`.

While writing these tests, two setups were found that would not have exercised the defects, and both were corrected:
- End sent before Claude reports any background work is correctly a no-op.
- Native deadlines are capped by the input's wall clock.

Recheck on the repaired tree (macOS):
- `g5d-agent-turn` lib 59, `g5d-worker` lib 112, `g5d-dispatch` lib 116; `dispatch_modules` 58/58.
- `cargo fmt --check` and clippy `-D warnings` clean.
- The frontend and API are unchanged since round 3's recheck.

## Independent CLI review round 7

The same reviewer reviewed candidate `5376fcb9`. Verdict: changes required. All three round-6 findings were confirmed resolved for their scenarios, and no defect was found in the post-handler Stop check. Probes confirmed Stop precedence after a delayed handler, a bounded post-exit drain, Stop during the drain, and native timeout during the drain. One major finding and two minor ones remained.

1. **Major: the queued-End fallback published before it decided.** It awaited End's notice and only then invalidated the requests. When a slow journal held the notice past a native deadline, requests that had expired by then were invalidated as `background_wait_ended`.
   - Repair: the fallback now decides first. It ends the requests (`invalidate_all` takes them synchronously) and drops the input, then publishes the notice.
   - A failed reply does the same: it ends the requests before its notice.
   - The decision thus happens right after the clock check, with nothing awaited in between, so it cannot act on an expired request. The outcome no longer depends on how fast events drain: End decides at its due time, like the reviewer's fast-drain control.
2. **Minor, acceptance: End during a turn took about 20 seconds.** The ten-second turn grace and a second ten-second queue bound added up.
   - Repair: End is due ten seconds after it was received, in every phase. A queued interrupt that has not started writing one second (`END_INTERRUPT_WRITE`) after that makes End take effect through the fallback.
3. **Minor: a test rejected a legitimate End-first outcome.** In the stopped-reading test with End, End's reply wait can close the input before the write stall does. That settles correctly without the `provider_input_failed` notice, so the test now requires that notice only without End.

Checks: `agent_turn_owner_end_is_bounded_behind_slowly_read_replies` runs `idle` and `turn` modes.
- In each mode, End's notice arrives within 13 seconds of End, the attempt settles `interrupted`, and every `background_wait_ended` invalidation precedes the notice.
- RED with `5376fcb9`: `idle` failed the ordering assertion, and `turn` took effect only after the bound.

Recheck on the repaired tree (macOS):
- `g5d-agent-turn` lib 59, `g5d-worker` lib 112; `dispatch_modules` 58/58.
- `g5d-dispatch` lib 115/116: the macOS watcher test failed again under the full suite (see round 6).
- `cargo fmt --check` and clippy `-D warnings` clean.

## Independent CLI review round 8 and PR feedback

Round 8 reviewed candidate `a9783267` and returned **APPROVE**. All round-7 findings were resolved, and the reviewer's extra probes at the front-of-queue boundary behaved correctly. The only remaining item was a non-blocking nit: the fallback's comment said the input drops "before anything is published", but `invalidate_all` publishes the invalidations before the input closes. The PR ([#272](https://github.com/G5Labs/G5-Lean/pull/272)) was opened at that head. Then:

- **CodeRabbit (two Minor findings, both validated):**
  - End refused only snapshots that named a provider other than Claude. A missing snapshot or provider reached the dispatcher. End now requires an explicit `claude_code` provider. A new case in `owner_end_wait_reaches_only_the_running_attempt` covers an attempt whose snapshot is absent. RED: with the old check, End was delivered.
  - The CLI contract's per-turn result rule (§3.6) did not state End's exception; it now does.
- **A Linux-only test defect, found while investigating `g5-ci-rust`.** The Cloud Build log needed an interactive gcloud login, so the agent suites were run locally in the CI runner image. The only failure there was `agent_turn_owner_end_queued_behind_replies_yields_to_finished_work`. This was not the cause of the `g5-ci-rust` failure; see the next section.
  - Linux's `/bin/sh` is dash. Following POSIX, it gives a background list `/dev/null` as stdin before explicit redirections, so the fixture's `<&0` reader logged nothing.
  - The fixture now saves the input as fd 3 first, as the replay fixture does. `dispatch_modules` then passed 60/60 on Linux, including the Linux-only process checks; the supervisor, worker and dispatcher libs also passed there.
- **Comment nit:** the fallback's comment now says the requests end before anything is awaited, and the input closes and the notice follows.
- **Build cache:** the worktree's shared build directory had been polluted by the merge and base checks, because Cargo names workspace crates the same across checkouts of one repository. After cleaning every workspace member, the full validation was rerun from a clean build:
  - Rust: `g5d-agent-turn` 59, `g5d-worker` 112, `g5d-dispatch` 116, `g5-contracts` 488, `g5-api-core` 1012; `dispatch_modules` 58/58.
  - API: 5/45/21/38/7.
  - All passed.

## The `g5-ci-rust` failure: the End route's path parameter

`g5-ci-rust` failed on `a9783267` and again on `e6dc9c9b`. In both runs only the `rust-xfs-orchestrator` step failed. That step fails only when one of its hard-fail lanes fails: the unit tests, the API and fake-provider integration tests, or the isolation lanes. `dispatch_modules` runs in the g5d integration lane, which is not one of them. That lane's result is checked later, by the final CI status step. So the dash fixture above would have failed the check, but it did not cause the step failure.

- **Cause:** the new End handler extracted its session with `Path<Uuid>`, binding a variable named `session_id`. The route guard `routes::guards::tests::no_raw_uuid_for_typed_ids` (`g5-api-routes` unit tests) rejects a raw `Uuid` bound to a typed-id name.
- **Fix:** the handler uses `Path<SessionPathParams>`, like every other session route in `chat.rs`. RED: the guard fails with the old handler (241 of 242 pass) and passes with the fix (242/242).
- **Validation gap closed:** earlier validation ran the unit tests of the crates this change touches and five API integration binaries. It now runs what the hard-fail lanes run: the unit tests of all 24 packages, every API integration binary, `fake-provider-cli`'s tests, the g5d `dispatch` test and `consumer_boundaries`. On macOS, with the fix:
  - Unit tests: all 24 packages pass (3339 tests) except `g5d-foundation`'s `terminal::tests::interactive_session_streams_input_output_and_resizes`. On this macOS that test cannot confirm that the terminal session's processes were reaped. It fails the same way at the base commit, and this change does not touch that crate.
  - API integration: all 103 binaries pass, 949 tests.
  - `fake-provider-cli` 42 + 39, the g5d `dispatch` test 30, `consumer_boundaries` 8.

## The merge head's `g5-ci-rust` failure: a probe shared across tests

After `main` was merged (`c187395d`), `g5-ci-rust` failed in its unit lane on three tests that neither side changed. Two of them, in `git_transport`, had also failed on `a9783267` and `e6dc9c9b`, behind the route-guard failure. On `549dfedc` they passed.

- **Cause:** the `dispatch_modules` test `catalog_is_pinned_listed_described_and_probed` runs the `port-connectivity` diagnostic. That diagnostic opens and closes a connection, without sending a byte, to every port the dispatcher's scanner finds in `/proc/net/tcp`. On a shared test host those ports include other tests' listeners. The `git_transport` tests asserted that no connection at all reached their fake upstream, so they caught that connection. Inside a workspace VM the probe behaves as intended. This branch adds `dispatch_modules` tests; that probably shifts when the probe runs relative to the unit lane, but this was not measured.
- **Evidence:** the failure was reproduced on Linux in the CI runner image.
  - With only the diagnostics test running alongside, 12 of 12 `git_transport` iterations failed.
  - With every other g5d test running alongside, 0 of 11 failed.
  - A temporary probe of the stray connections found that they carried no bytes.
- **Fix:** the `git_transport` tests count an upstream connection only once it has sent a byte; they peek at it and do not consume it. This is the invariant they check: nothing is relayed upstream. All six upstream accepts use this. Under the same stress, 0 of 10 iterations fail.
- **The third test** was a separate race that already existed: `noninterrupt_fork_drains_admitted_code_and_docs_mutations_before_one_flush`. Admission counts a mutation before it reaches the test gate, so asserting the gate file as soon as four mutations were counted could fail. The test now waits for the gate file, as its sibling tests do; it passed 20 of 20 runs on Linux.
- **Also checked:** the full `g5d-dispatch` lib (120) and the g5d `dispatch` test (30) pass on Linux, and clippy is clean.
