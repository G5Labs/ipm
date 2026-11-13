# Agent time limits, Claude background work and CLI upgrades

## Ontology changes and governance

**Ontology changes.** Comparison base: `origin/main` at `967bf34c`, which includes the repository-root harness from #248. Modified, with their metadata:
- `g5.conversation.providers`: the Claude pin becomes 2.1.285, and one attempt can span Claude's own follow-up turns.
- `g5.conversation.providers.codex`: the Codex pin becomes 0.159.3.
- `g5.agents.execution-deadlines`: for Claude and Codex, the default deadline stops once the provider accepts its input.

Amended after the [implementation review](ontology-review-claude-background-work.md), each with its metadata:
- `g5.agents.execution-limits`: the agent wall clock bounds provider execution only when the task requests a limit. This reconciles the agent block, which still said an absent request cannot relax the catalog bound, with the human-verified decisions in `g5.conversation.providers.claude-background.deadline` and `g5.agents.lifecycle.owner-stop`.
- `g5.agents.outcome`: after the provider exits, its output closes at end of file or when a short bounded drain ends, so a descendant holding the provider's output cannot hold the attempt open.
- `g5.agents.execution-deadlines`: a requested limit also bounds a Claude background wait, and unattended workflow and bootstrap agent tasks request the previous one-hour limit (Design step 2).
- `g5.conversation.providers.claude-background.deadline`: a requested limit is listed among the ways a wait ends, citing the deadline rule.
- `g5.conversation.providers.claude-background.end`: a bounded stop holds the task for Continue like Stop, and a turn that End's own interrupt aborts is End's effect rather than a failed turn.
- `g5.conversation.providers.claude-followups`: every started turn needs a successful result, and background sub-agent frames are not units of the reply.

Amended after the [independent CLI review](ontology-review-claude-background-work.md#independent-cli-review-round-1):
- `g5.conversation.providers.claude-background.end`: the exit status Claude reports for a turn that End's own interrupt aborted is End's effect too. At 2.1.285 Claude exits 1 after such a turn.
- Added `g5.conversation.providers.claude-background.end-bound` (agent): End interrupts a running turn after at most ten seconds, names the attempt whose wait the owner saw, and does nothing more when no background work remains.
- Metadata only: `g5.conversation.providers`, `g5.conversation.providers.claude-background.visibility`, `g5.agents.outcome`, `g5.agents.execution-deadlines` and `g5.architecture.provider_adapter.claude-background`.

Added, each with a metadata entry:
- In [conversations](ontology/features/conversations.g5.md): `g5.conversation.providers.claude-background`, `g5.conversation.providers.claude-background.untracked`, `g5.conversation.providers.claude-followups`, `g5.conversation.providers.claude-background.visibility`, `g5.conversation.providers.claude-background.end`, `g5.conversation.providers.claude-background.input` and `g5.conversation.providers.claude-background.deadline`.
- In [task execution](ontology/features/agents.g5.md): `g5.agents.lifecycle.owner-stop`.
- In [Agents architecture](ontology/software_architecture/agents.g5.md): `g5.architecture.provider_adapter.claude-background`.
- In [provider accounts](ontology/features/provider_accounts.g5.md): `g5.provider.catalog.pinned`.

The request and the owner's decisions are recorded in [the background-work intent](chat_intent/claude-background-work/request.md). Actual ontology and metadata diff:

```diff
diff --git a/.g5/ontology/features/agents.g5.md b/.g5/ontology/features/agents.g5.md
index 41f87c4d..883d0221 100644
--- a/.g5/ontology/features/agents.g5.md
+++ b/.g5/ontology/features/agents.g5.md
@@ -146,0 +147,6 @@ Preserve the [distinct identities of logical progress and attempts](../concepts/
+### Owner Stop reliably ends any agent attempt
+
+[D · g5.agents.lifecycle.owner-stop]
+
+Because no default time limit ends provider execution, the owner's Stop or Cancel of agent work must end a Claude or Codex attempt in every state: an active, silent or looping turn and, for Claude, also a [background wait](conversations.g5.md#keep-a-claude-attempt-live-while-claude-owns-background-work), a pending native request or a follow-up turn. It uses bounded cooperative interruption followed by forced reap under the [execution tree owner](../software_architecture/agents.g5.md#execution-tree-ownership), and the stopped state is visible. On cloud hosts, [daily rotation](../software_architecture/runtime.g5.md#keep-the-mvp-host-credential-and-rotation-model) drains even busy hosts before their 24-hour credential expires, which bounds the worst case; self-hosted and local hosts have no such bound.
+
@@ -225 +231 @@ Bootstrap authority does not permit arbitrary workspace creation.
-Declare execution success only after terminal provider outcome, output closure and process exit.
+Declare execution success only after terminal provider outcome, output closure and process exit. After the provider process exits, its output closes when its pipes reach end of file or a short bounded drain ends; a descendant that inherited the pipes cannot hold the attempt open.
@@ -234 +240,4 @@ delivery; every accepted limit must govern the actual provider execution. An abs
-request cannot relax a catalog or policy bound. The catalog's serialized module-output
+request cannot relax a catalog or policy bound, except the agent wall clock: without a
+requested limit it bounds only setup and input delivery under the [attempt deadline
+rule](#distinguish-attempt-and-provider-deadlines), so neither the default nor the
+catalog's wall clock ends provider execution. The catalog's serialized module-output
@@ -250,6 +259,13 @@ accepted partial output and settle the attempt as limited.
-The attempt-wide deadline starts before authentication and restore and covers initial
-input delivery, provider execution and harvesting. A terminal result or blocked write
-does not reset it. At expiry, execution cannot succeed; cleanup and any remaining
-harvest have bounded grace before settlement. V1 accepts no separate provider-only
-timeout; any future provider budget starts at launch, covers input, output and exit,
-and cannot exceed the remaining attempt budget.
+The default attempt deadline starts before authentication and restore, covers initial
+input delivery, and stops once the provider [accepts the input](conversations.g5.md#delivery-means-actual-transport-handoff).
+No default time limit applies to Claude or Codex turns or to [Claude background
+waiting](conversations.g5.md#do-not-time-limit-claude-background-waiting). A wall-clock
+limit the task explicitly requests bounds the whole attempt, including provider
+execution and any Claude background wait; unattended workflow and bootstrap agent
+tasks request the previous one-hour limit. A terminal result or blocked write does not reset a running deadline. At
+expiry, execution cannot succeed; cleanup and any remaining harvest have bounded grace
+before settlement, and harvest after a normal exit has its own bound. Without
+a requested limit, a hung or runaway turn ends only through the [owner's Stop](#owner-stop-reliably-ends-any-agent-attempt),
+the [output bound](#bound-output-before-framing-and-parsing) or a VM stop. V1 accepts
+no separate provider-only timeout; any future provider budget starts at launch, covers
+input, output and exit, and cannot exceed the remaining attempt budget.
@@ -427,0 +444,6 @@ Dependency state follows the [logical task lifecycle](agents.g5.md#separate-logi
+### g5.agents.lifecycle.owner-stop
+
+- human-verified
+- The owner [made](../../chat_intent/claude-background-work/request.md#decisions) a reliable way to end the turn the condition for dropping automatic time limits, noting that the worst case is at most 24 hours, and extended both rules to Codex.
+- Partial · Stop and cancel use the [issue 229](https://github.com/G5Labs/G5-Lean/issues/229) bounded interruption and forced reap for both providers, independent of the supervisor's state; `agent_turn_cancel_ends_a_background_wait` covers a Claude wait, a report turn and a report turn's pending request with a SIGINT-ignoring provider (process death asserted on Linux only, so in cloud CI); the frontend keeps Stop for the owner of a mutable workspace regardless of VM status reads. `g5-api-core/src/cloud/rotation.rs` force-rotates busy cloud hosts at the host-credential cliff (TTL minus two hours). Pre-existing gap: Stop discards the module outcome, so completed turns' usage is not billed.
+
@@ -498 +520 @@ Dependency state follows the [logical task lifecycle](agents.g5.md#separate-logi
-- Partial · `g5d-agent-turn/src/runtime.rs` requires a terminal result, output closure and successful process exit; active dispatcher regressions cover early exit, terminal-before-EOF timeout and harvest, malformed frames and Claude's max-turn result. The full late-output/process-exit matrix and live provider/VM path remain unexecuted.
+- Partial · `g5d-agent-turn/src/runtime.rs` requires a terminal result for every started turn, output closure and successful process exit, except that Claude's recorded exit status 1 after the owner's End interrupt aborted a turn is [End's effect](conversations.g5.md#let-the-owner-end-background-waiting); after exit it waits at most five seconds (`PROVIDER_DRAIN_GRACE`) for the output to close, then stops the provider group and settles from the evidence. Active dispatcher regressions cover early exit, terminal-before-EOF timeout and harvest, malformed frames, Claude's max-turn result, an unfinished follow-up turn and a descendant holding the output. The full late-output/process-exit matrix and live provider/VM path remain unexecuted.
@@ -504 +526 @@ Dependency state follows the [logical task lifecycle](agents.g5.md#separate-logi
-- Partial · `agent_dispatch.rs` rejects resolved Codex max-turn requests; `g5d-agent-turn` rejects Codex policy caps, applies a policy-only Claude turn cap and shares the raw stdout budget with the optional title call; `g5d-dispatch` applies the request/policy/catalog attempt deadline. Focused synthetic dispatcher tests pass. Live provider/VM behavior is unexecuted.
+- Partial · `agent_dispatch.rs` rejects resolved Codex max-turn requests; `g5d-agent-turn` rejects Codex policy caps, applies a policy-only Claude turn cap and shares the raw stdout budget with the optional title call; `g5d-dispatch` applies the request/policy/catalog attempt deadline only to a requested limit and otherwise only the setup bound (amended after review to match the human-verified [deadline](conversations.g5.md#do-not-time-limit-claude-background-waiting) and [owner-stop](#owner-stop-reliably-ends-any-agent-attempt) decisions). Focused synthetic dispatcher tests pass. Live provider/VM behavior is unexecuted.
@@ -515,2 +537,2 @@ Dependency state follows the [logical task lifecycle](agents.g5.md#separate-logi
-- [Issue 233](https://github.com/G5Labs/G5-Lean/issues/233) asks to define attempt-wide and provider-only budgets across setup and cleanup.
-- Partial · `g5d-dispatch::modules::runner` starts the effective clock before Git refresh and passes its deadline into `g5d-agent-turn::runtime`; the supervisor holds a fixed success deadline while bounded cleanup/harvest has grace. Active blocked-input and terminal-before-EOF regressions reach timeout and harvest; the API settlement test distinguishes unknown and proven pre-write delivery. Native auth and stalled harvest paths remain unexecuted; Linux cleanup proof awaits Cloud CI.
+- [Issue 233](https://github.com/G5Labs/G5-Lean/issues/233) asks to define attempt-wide and provider-only budgets across setup and cleanup. The [background-work decisions](../../chat_intent/claude-background-work/request.md#decisions) remove the default limit on Claude and Codex execution after input acceptance, while setup, harvest and explicitly requested limits stay bounded; the [approved design](../../ontology-plan-claude-background-work.md#design) has unattended workflows request a limit.
+- Implemented · `limits.wall_clock_scope` separates a requested limit (`attempt`) from the default (`input_delivery`); `g5d-agent-turn::runtime::Deadlines` lifts the default once the provider takes the input, `g5d-dispatch::modules::runner::effective_run_limits` sets no settlement deadline without a requested limit and passes its setup deadline, normal harvest has a five-minute bound, and `job_service` workflow and bootstrap agent specs request one hour. Tests: `agent_turn_default_limit_stops_at_input_acceptance` (Claude and Codex turns outlive the default; a Codex request still ends; blocked input still times out), `agent_wall_clock_scope_decides_the_settlement_deadline`, `requested_wall_clock_limit_bounds_the_whole_attempt`, `profile_workflow_agent_task_requests_the_unattended_time_limit`. Chat tasks created before this change keep their serialized one-hour value as a request until they end. During the control-plane-first rollout, a VM still on the previous module catalog receives `agent.turn` 1.0.1 without `wall_clock_scope` and keeps the old attempt-wide default until it is replaced (`agent_dispatch_targets_the_module_contract_of_the_vm_catalog`). Native auth and stalled harvest remain unexecuted; the Linux cleanup proof runs in cloud CI.
diff --git a/.g5/ontology/features/conversations.g5.md b/.g5/ontology/features/conversations.g5.md
index ff705152..a1f976ed 100644
--- a/.g5/ontology/features/conversations.g5.md
+++ b/.g5/ontology/features/conversations.g5.md
@@ -212 +212 @@ Continuation retains [logical task identity](agents.g5.md#separate-logical-outco
-Claude uses CLI 2.1.270 until a scoped upgrade and runs one direct turn with native bidirectional questions and permissions.
+Claude uses CLI 2.1.285 until a scoped upgrade and runs one direct attempt with native bidirectional questions and permissions. That attempt can span Claude's own follow-up turns while [Claude-owned background work](#keep-a-claude-attempt-live-while-claude-owns-background-work) runs.
@@ -218 +218 @@ Claude uses CLI 2.1.270 until a scoped upgrade and runs one direct turn with nat
-Codex uses CLI 0.153.3 exec JSONL until a scoped upgrade, receiving one prompt then EOF without actionable interactive cards or a custom ask fallback.
+Codex uses CLI 0.159.3 exec JSONL until a scoped upgrade, receiving one prompt then EOF without actionable interactive cards or a custom ask fallback.
@@ -225,0 +226,50 @@ Unexpected Codex interactive protocol fails explicitly while ordinary prose ques
+### Keep a Claude attempt live while Claude owns background work
+
+[D · g5.conversation.providers.claude-background]
+
+After a run-ending result, the Claude adapter keeps the attempt and Claude's input open while Claude reports background tasks or background sub-agents that it owns. It closes the input only when Claude is between turns, reports no remaining background work and has no pending native request; exit, harvest and settlement then proceed normally. The agent, not the platform, decides whether that work continues or stops. One provider process still serves one attempt, so this is not [resident interaction](#defer-resident-provider-interaction).
+
+Preserves [provider-process tracking](agents.g5.md#track-the-provider-process-as-execution) and [outcome evidence](agents.g5.md#provider-output-is-not-the-final-execution-decision).
+
+### Exclude untracked processes from background work
+
+[D · g5.conversation.providers.claude-background.untracked]
+
+Only work that Claude itself reports keeps an attempt live. A process the agent detaches outside Claude's background tracking, for example with `nohup` or a new session, does not extend the attempt and is stopped by [execution tree cleanup](../software_architecture/agents.g5.md#execution-tree-ownership) when the attempt ends.
+
+### Claude follow-up turns belong to the same attempt
+
+[D · g5.conversation.providers.claude-followups]
+
+When Claude starts its own turn to report finished background work, that turn belongs to the same attempt and assistant reply. Its completed units follow earlier ones in provider order under [unit identity](#preserve-completed-assistant-units); each turn result is a turn boundary rather than the attempt outcome, and usage accumulates across turns. The attempt succeeds only if every turn that started produced a successful result and the [outcome evidence](agents.g5.md#provider-output-is-not-the-final-execution-decision) holds. Frames from Claude's background sub-agents are not units of the reply; that work is shown through the [background set](#show-claude-background-waiting) and Claude's own report turns.
+
+### Show Claude background waiting
+
+[D · g5.conversation.providers.claude-background.visibility]
+
+While an attempt waits on background work, the conversation shows Claude's reported background set and the elapsed wait as bounded, sanitized descriptions rather than an unexplained in-progress reply. Readers see the same projection; only the owner can [end the wait](#let-the-owner-end-background-waiting).
+
+### Let the owner end background waiting
+
+[D · g5.conversation.providers.claude-background.end]
+
+The owner can end the wait without cancelling the logical task. The adapter stops Claude's background work through Claude's native controls where the pinned CLI supports them, otherwise through the bounded stop path. A visible notice records that the work was stopped. If Claude then exits cleanly with successful turn results, the attempt succeeds; a bounded stop settles as interrupted, which holds the task for Continue like Stop. A turn that the End interrupt itself aborts, and the exit status Claude reports for it, are the End's effect, not a failed turn result. [Cancel](agents.g5.md#make-cancellation-terminal-for-the-logical-task) and Stop keep their meanings.
+
+### End takes effect within a bounded time
+
+[D · g5.conversation.providers.claude-background.end-bound]
+
+The owner's [End](#let-the-owner-end-background-waiting) does not wait indefinitely for a running Claude turn. Between turns it interrupts Claude at once; during a turn it lets the turn run for up to ten seconds, then interrupts it. End names the attempt whose wait the owner saw, so a delayed or retried End never affects a later attempt ([operation authority](../concepts/data_model.g5.md#keep-operation-authority-distinct-from-resource-state)). When End would take effect and Claude reports no remaining background work, End does nothing more, and the reply finishes normally, including the turn that reports that work.
+
+### Queue owner input behind the waiting attempt
+
+[D · g5.conversation.providers.claude-background.input]
+
+New owner input queues behind a waiting attempt under [session serialization](agents.g5.md#serialize-one-logical-conversation) and is delivered by a later attempt. The adapter never writes it into the waiting Claude process; live delivery remains [resident interaction](#defer-resident-provider-interaction).
+
+### Do not time-limit Claude background waiting
+
+[D · g5.conversation.providers.claude-background.deadline]
+
+No platform time limit ends background waiting. The wait ends when Claude's background work ends, when the owner [ends it](#let-the-owner-end-background-waiting), when a wall-clock limit the task explicitly requested expires under the [attempt deadline rule](agents.g5.md#distinguish-attempt-and-provider-deadlines), or when Stop, Cancel, VM stop (including daily host rotation) or VM loss ends the attempt. Claude's turns have no default time limit either, under the [attempt deadline rule](agents.g5.md#distinguish-attempt-and-provider-deadlines). Background work does not survive Stop, VM loss or continuation, and a later [continuation input](#continuation-says-what-is-being-continued) states that the interrupted attempt's background work stopped.
+
@@ -498 +548 @@ Automatic first-request or best-effort summary naming uses the existing turn’s
-- ~75% · continuation/notices; frozen provenance and retry-exact rendering incomplete.
+- ~75% · continuation/notices; frozen provenance and retry-exact rendering incomplete. The continuation input states that the earlier attempt's background tasks and sub-agents are no longer running (`agent_dispatch.rs`; `interrupted_task_continues_original_objective_without_resending_its_input`).
@@ -503,2 +553,2 @@ Automatic first-request or best-effort summary naming uses the existing turn’s
-- From [C122–C123](../../uplift/g5-lean/candidates/contracts.md), [H079](../../uplift/g5-lean/candidates/human-decisions.md); chosen CLI pins remain in repository version files and [C3](../../uplift/g5-lean/investigations/agents.md).
-- ~90% · Claude adapter and version pin; authenticated native interaction scenarios unexecuted.
+- From [C122–C123](../../uplift/g5-lean/candidates/contracts.md), [H079](../../uplift/g5-lean/candidates/human-decisions.md); the [background-work request](../../chat_intent/claude-background-work/request.md) asks for the scoped Claude upgrade and multi-turn attempts. Chosen CLI pins remain in repository version files and [C3](../../uplift/g5-lean/investigations/agents.md).
+- Implemented · `scripts/claude-code-cli-version` pins 2.1.285, which passes `scripts/check-claude-cli-flags.sh` on Linux; the exact-pin fixtures in `tests/fixtures/agent-turn-native/claude/` were re-recorded at 2.1.285 with `record.py` (native requests, cancel, result-before-EOF and four background sessions, one interrupted during a turn) and `g5d-agent-turn::protocol` tests replay them. One process now spans Claude's own follow-up turns ([claude-followups](#claude-follow-up-turns-belong-to-the-same-attempt)). Authenticated native interaction through the product and the QA live checks remain unexecuted.
@@ -509,2 +559,2 @@ Automatic first-request or best-effort summary naming uses the existing turn’s
-- From [C122–C123](../../uplift/g5-lean/candidates/contracts.md), [H079](../../uplift/g5-lean/candidates/human-decisions.md); chosen CLI pins remain in repository version files and [C3](../../uplift/g5-lean/investigations/agents.md).
-- ~90% · Codex exec adapter and CLI pin; live provider turn unexecuted.
+- From [C122–C123](../../uplift/g5-lean/candidates/contracts.md), [H079](../../uplift/g5-lean/candidates/human-decisions.md); the [background-work request](../../chat_intent/claude-background-work/request.md#decisions) asks for the scoped Codex upgrade. Chosen CLI pins remain in repository version files and [C3](../../uplift/g5-lean/investigations/agents.md).
+- Implemented · `scripts/codex-cli-version` pins 0.159.3; `g5d-worker::runner::codex` replays a recorded 0.159.3 exec stream and `g5d-agent-turn::codex_auth` parses its device-login layout; each offered model completed a ChatGPT-sign-in exec turn on 0.159.3 (2026-09-30 probe). A live product turn, including one longer than an hour, is unexecuted.
@@ -517,0 +568,48 @@ Automatic first-request or best-effort summary naming uses the existing turn’s
+### g5.conversation.providers.claude-background
+
+- human-verified
+- The [background-work request](../../chat_intent/claude-background-work/request.md) states that background work should survive and that the agent decides whether to stop it. [Agent API §10.1](../../../docs/agent-api-surface-v9.md#101-transport-and-launch-target) already requires closing Claude's input only after a run-ending result with no remaining provider work; [probes](../../ontology-plan-claude-background-work.md#provider-behavior-evidence) show Claude kills background tasks after input EOF but waits for background sub-agents.
+- Implemented · `g5d-agent-turn/src/runtime.rs` (`advance_claude_input`) keeps Claude's input open while the reported set is non-empty, closes it between turns with an empty set and no pending request (3-second settle once background work was seen), and bounds the drain after exit. Dispatcher process tests (`agent_turn_claude_background_report_turn_stays_in_the_attempt`, `agent_turn_claude_window_edges`, the recorded-session replay) pass and fail with the old close-at-first-result rule ([review](../../ontology-review-claude-background-work.md)). The QA live checks are unexecuted.
+
+### g5.conversation.providers.claude-background.untracked
+
+- agent
+- Keeps [issue 229](https://github.com/G5Labs/G5-Lean/issues/229) containment for work the provider does not own; [probes](../../ontology-plan-claude-background-work.md#provider-behavior-evidence) show self-detached processes outlive Claude's exit unless execution-tree cleanup reaps them.
+- Implemented · `g5d-foundation::process_termination::execution_owner` reaps remaining descendants at every supervisor exit on Linux; after a provider exits, the supervisor stops its process group once the bounded output drain ends, so a descendant holding the provider's output no longer holds the attempt open (`agent_turn_claude_window_edges`, `drain` case; [review](../../ontology-review-claude-background-work.md) finding 1). Nothing tells agents to prefer Claude's background tasks over detaching; the [provided harness](ontology_and_harness.g5.md#provide-the-harness-in-every-workspace) deliberately carries no such runtime guidance. The Linux reap of an escaped descendant after a successful attempt runs only in cloud CI.
+
+### g5.conversation.providers.claude-followups
+
+- agent
+- [Probes](../../ontology-plan-claude-background-work.md#provider-behavior-evidence) show each self-started report turn begins with `system/init` and ends with its own `result`; [issue 232](../../chat_intent/issue-232/request.md) owns unit identity.
+- Implemented · `g5d-worker::runner::claude` maps a later `system/init` to `TurnStarted` and drops frames of background sub-agents (by the `task_started` launch id); `g5d-agent-turn::runtime::project_event` gives result-only text of turn N≥2 the block `g5:terminal-result:N`, sums tokens and keeps the latest session cost (`accumulate_usage`), fails the attempt on any turn's error result or an unfinished turn. Process tests cover summed usage, the report text in the reply, a failed report turn and an unfinished one; the protocol test replays the recorded interrupt session without sub-agent rows.
+
+### g5.conversation.providers.claude-background.visibility
+
+- agent
+- Design recommendation so a long wait is explainable; `background_tasks.replaced` already exists in the [Agent API event union](../../../docs/agent-api-surface-v9.md) without an emitter.
+- Implemented · the adapter emits bounded, redacted `background_tasks.replaced` (with when the wait began) and `subtask.*` only for tasks Claude runs in the background; Claude 2.1.285 also registers long foreground commands as tasks, and the mapper ignores those (`foreground_commands_claude_registers_as_tasks_are_not_background_work`). The session's live task carries the latest non-empty set (`TaskService::live_background_wait`); `frontend/src/lib/taskTimeline.ts` and `ChatPanel`'s `BackgroundWaitPanel` show the set and elapsed wait to every reader, End only to the owner and only for a set that names its attempt. Unit, API (`session_task_carries_the_live_background_set`) and jsdom tests only; not exercised in a browser against a live attempt.
+
+### g5.conversation.providers.claude-background.end
+
+- human-verified
+- The owner [chose](../../chat_intent/claude-background-work/request.md#decisions) success with a notice when Claude exits cleanly after the wait ends; the control exists so an unwanted or never-ending background task cannot hold the session.
+- Implemented · `POST /api/v1/sessions/:id/background/end` (owner, Claude attempts) → `TaskService::end_session_background_wait` → private dispatcher `modules.background.end` → control line → `advance_claude_input`, which sends Claude's in-band `interrupt` (recorded at 2.1.285 between turns and during a turn: Claude answers at once and stops background sub-agents; background commands stop at input EOF), invalidates a sub-agent's pending request and closes input; after an earlier close it applies the bounded stop. Claude 2.1.285 exits 1 at EOF after an interrupt aborted a turn (`background-agent-interrupt-turn` recording); the outcome treats that status as End's effect (`CLAUDE_INTERRUPTED_TURN_EXIT_CODE`, pinned to the recording). A turn counts as aborted by End only when Claude did not refuse the interrupt and its result carries an `aborted_*` terminal reason; a refused interrupt followed by an unrelated error fails the attempt (`refused`). Process tests cover clean, late-reply, stubborn, deaf, pending-request, after-close, interrupted-report-turn, busy-turn and refused-interrupt providers and two recorded-session replays; without the exit-status rule the interrupted report turn settled `failed`. The API test covers owner-only, Codex refusal, an attempt whose snapshot names no provider, no running attempt and stale task or attempt identities ([review](../../ontology-review-claude-background-work.md)).
+
+### g5.conversation.providers.claude-background.end-bound
+
+- agent
+- The [independent CLI review](../../ontology-review-claude-background-work.md#independent-cli-review-round-1) found End deferred indefinitely while a turn ran, with the control stuck at "Ending…", and that a delayed End named only the session; the human-verified [End](#let-the-owner-end-background-waiting) exists so background work cannot hold the session.
+- Implemented · `advance_claude_input` interrupts after `END_TURN_GRACE` (10 s) during a turn and drops End when the set is empty, and Claude's input is written through a non-blocking queue that counts as broken after 10 s without progress, so a provider that stops reading cannot hold End: its input is dropped and End's bounded stop settles `interrupted`, while a provider that finishes cleanly meanwhile still succeeds. End takes effect only when its interrupt's first byte is written; an interrupt still queued is withdrawn when the background work ends first (`agent_turn_owner_end_queued_behind_replies_yields_to_finished_work`), and End is due ten seconds after it was received, between turns or during one: if its interrupt has not started a second later, End ends the requests and drops the input before publishing its notice, and the bounded stop follows (`agent_turn_owner_end_is_bounded_behind_slowly_read_replies`). The End request carries `{taskId, attempt}` from the displayed set, and `end_session_background_wait` answers 409 when that attempt is no longer the running one. The waiting panel is keyed by its wait start, so a pending End never carries over to a later wait. Tests: `agent_turn_owner_end_edges` (`busy_turn`, `drained`, `late_report`, `refused`), `agent_turn_provider_that_stops_reading_cannot_hold_replies_or_end`, `agent_turn_claude_finishing_while_a_reply_fails_keeps_its_result`, the `interrupt_turn` recorded replay, `owner_end_wait_reaches_only_the_running_attempt`, and the ChatPanel (including "offers End again for a later wait after a delivered End") and WorkspacePage End tests. A live End against a running turn is unexecuted (QA pending).
+
+### g5.conversation.providers.claude-background.input
+
+- human-verified
+- The owner [chose](../../chat_intent/claude-background-work/request.md#decisions) to queue new input until the background work ends, keeping the [resident-interaction deferral](../../uplift/g5-lean/candidates/human-decisions.md) from H069/H079.
+- Implemented by existing session serialization; the adapter writes only the prompt, native replies and the End interrupt into Claude's input (`agent_turn_claude_background_report_turn_stays_in_the_attempt` asserts a single input line).
+
+### g5.conversation.providers.claude-background.deadline
+
+- human-verified
+- The owner [chose](../../chat_intent/claude-background-work/request.md#decisions) no time limit for background waiting because `docker-ci` can run for hours, and no automatic guard on Claude turns; daily host rotation and the 24-hour [guest token](../software_architecture/runtime.g5.md#short-server-lease-and-long-credential-have-separate-purposes) still end it.
+- Implemented · `WallClockScope::InputDelivery` lifts the default deadline once the provider takes the input, the dispatcher sets no settlement deadline without a requested limit, and continuation input states that background work stopped. Process tests show a four-second wait outliving a 1.5-second default and a requested limit ending a wait. Chat tasks created before this change carry the serialized one-hour default as a request until they end. Multi-hour live waits are unexecuted (QA pending).
+
@@ -570 +668 @@ Automatic first-request or best-effort summary naming uses the existing turn’s
-- ~85% · invalidation/deadline/runtime recheck; full uncertain-write and race matrix unexecuted.
+- ~85% · invalidation/deadline/runtime recheck; full uncertain-write and race matrix unexecuted. Writes to Claude's input after the prompt are queued and written without blocking the supervisor loop (`ProviderInput`), so request deadlines, cancellations and Claude's output keep being handled while Claude reads slowly. A reply counts as delivered only once completely written, and its request keeps its deadline until then (`agent_turn_native_reply_cannot_outlive_its_request_deadline`, `agent_turn_blocked_replies_leave_cancellation_and_deadlines_running`). The earliest clock is armed ahead of provider output, control lines and writes, with an expired attempt limit deciding before a native deadline (`next_clock`, `expired_clock`), so after a delayed wakeup or slow event delivery a late reply, a buffered cancellation or End cannot erase an expired request; Stop and the clocks are checked again after every handler (`agent_turn_expired_request_is_settled_before_late_events`). Writing with no progress for 10 s, or into a closed pipe, drops the input; if a reply was lost, the pending requests end as `provider_input_failed`, Claude's own output still settles the attempt, and a provider still running after the exit grace is stopped as `suspended` (`native_response_undeliverable`) (`agent_turn_provider_that_stops_reading_cannot_hold_replies_or_end`, `agent_turn_claude_finishing_while_a_reply_fails_keeps_its_result`).
diff --git a/.g5/ontology/features/provider_accounts.g5.md b/.g5/ontology/features/provider_accounts.g5.md
index aa84813e..5ab5b454 100644
--- a/.g5/ontology/features/provider_accounts.g5.md
+++ b/.g5/ontology/features/provider_accounts.g5.md
@@ -68,0 +69,8 @@ Offer the retained Codex model catalog under account and model policy without th
+### Keep offered models usable with the pinned CLIs
+
+[D · g5.provider.catalog.pinned]
+
+The offered Claude and Codex model lists contain only models that the pinned CLI accepts for the account types they are offered to. A CLI pin upgrade re-checks both lists, removing models the new pin or an account type no longer accepts and adding models it newly supports.
+
+Applies the [provider pins](conversations.g5.md#use-each-native-provider-within-its-chosen-transport) and the [Codex transport](conversations.g5.md#run-codex-through-its-chosen-noninteractive-transport).
+
@@ -244,0 +253,6 @@ This developer-desktop trust model provides neither a credential broker nor isol
+### g5.provider.catalog.pinned
+
+- human-verified
+- The owner [asked](../../chat_intent/claude-background-work/request.md#decisions) to update the model lists with the CLI upgrades in case some are no longer supported. A probe showed Codex 0.153.3 rejecting `gpt-6-luna` for a ChatGPT login while 0.158.0 accepted it.
+- Implemented for this upgrade · `backend/g5-contracts/src/{claude,codex}_models.rs` (generated into `frontend/src/lib/harnessCatalog.gen.ts`) offer models that completed a turn on the new pins with subscription and ChatGPT sign-ins, and every offered OpenRouter slug is on OpenRouter's model list (2026-09-30); API-key accounts are unprobed. `offered_models_match_the_pinned_cli_model_list` and `offered_variants_match_the_current_picker_lineup` assert the pin they were checked against, so a pin change fails them until the lists are re-checked. The withdrawn Codex models already failed for ChatGPT sign-ins at 0.153.3; saved selections still dispatch because launch does not enforce `offered`.
+
diff --git a/.g5/ontology/software_architecture/agents.g5.md b/.g5/ontology/software_architecture/agents.g5.md
index 10185707..9c0b7ed5 100644
--- a/.g5/ontology/software_architecture/agents.g5.md
+++ b/.g5/ontology/software_architecture/agents.g5.md
@@ -80,0 +81,8 @@ The dispatcher owns admission/cancellation; TaskService decides logical completi
+### Interpret Claude background work inside the adapter
+
+[D · g5.architecture.provider_adapter.claude-background]
+
+The Claude adapter alone interprets Claude's turn, background-task and sub-agent frames to decide when to close input. It reports the background set as `background_tasks.replaced` and sub-task events through the [shared event contract](../../../docs/agent-api-surface-v9.md). TaskService and the dispatcher receive ordinary events and one outcome; they do not schedule provider work.
+
+Preserves [provider-process tracking](../features/agents.g5.md#track-the-provider-process-as-execution), the [background window](../features/conversations.g5.md#keep-a-claude-attempt-live-while-claude-owns-background-work) and the [execution tree owner](#execution-tree-ownership).
+
@@ -170,0 +179,6 @@ workspace owner using ProviderAccountService's eligibility decision.
+### g5.architecture.provider_adapter.claude-background
+
+- agent
+- Keeps provider frame semantics in the adapter, since the [background-work request](../../chat_intent/claude-background-work/request.md) is Claude-specific and the platform must not gain a [second scheduler](../features/agents.g5.md#track-the-provider-process-as-execution).
+- Implemented · `g5d-worker::runner::claude` maps turn and background frames to `TurnStarted`, `BackgroundTasks` and `TaskLifecycle`, drops background sub-agent frames and ignores the task frames Claude 2.1.285 emits for long foreground commands; `g5d-agent-turn::runtime` alone keeps the turn and background state and emits ordinary journal events and one outcome; TaskService only normalizes the event kinds and exposes the latest set on the session's live task.
+
```

**Relevant and governing ontology.**

| Commitment | Obligation, owner, allowed case and counterexample | Conclusion |
|---|---|---|
| [`g5.conversation.providers.claude-background`](ontology/features/conversations.g5.md#keep-a-claude-attempt-live-while-claude-owns-background-work) | The Claude adapter keeps input open after a result while Claude reports owned background work, and closes it only between turns with an empty background set and no pending native request. Allowed: a background `docker-ci` keeps the attempt live and Claude reports when it finishes. Counterexample: closing input at the first result, as today, which kills the job about 5 s later. | New; it is the rule [Agent API §10.1](../docs/agent-api-surface-v9.md#101-transport-and-launch-target) already states. |
| [`g5.conversation.providers.claude-background.untracked`](ontology/features/conversations.g5.md#exclude-untracked-processes-from-background-work) | Only Claude-reported work extends an attempt. Allowed: a `nohup` job is reaped when the attempt ends. Counterexample: a detached process outliving its attempt with no owner. | New; reuses the issue 229 cleanup unchanged. |
| [`g5.conversation.providers.claude-followups`](ontology/features/conversations.g5.md#claude-follow-up-turns-belong-to-the-same-attempt) | Claude's self-started report turns stay in the same attempt and assistant reply; per-turn results are boundaries; usage sums; success needs every turn result to succeed. Counterexample: treating the first `result` as the outcome, or dropping the report text. | New. |
| [`g5.conversation.providers.claude-background.visibility`](ontology/features/conversations.g5.md#show-claude-background-waiting) | The conversation shows the reported background set and elapsed wait; readers see it, only the owner controls it. Counterexample: an unexplained spinner for hours. | New. |
| [`g5.conversation.providers.claude-background.end`](ontology/features/conversations.g5.md#let-the-owner-end-background-waiting) | The owner ends the wait without cancelling the task: native stop where supported, else the bounded stop path, with a visible notice. A clean exit after the stop is success; a bounded stop settles as interrupted. | New; owner decision; the native stop control is an open assumption. |
| [`g5.conversation.providers.claude-background.input`](ontology/features/conversations.g5.md#queue-owner-input-behind-the-waiting-attempt) | Queued owner input waits for a later attempt and is never written into the waiting process. Counterexample: writing it as a second user frame, which is resident interaction. | New; owner decision; preserves `g5.conversation.providers.later` and `g5.scope.later-work`. |
| [`g5.conversation.providers.claude-background.deadline`](ontology/features/conversations.g5.md#do-not-time-limit-claude-background-waiting) | No platform time limit ends the wait. It ends when the work ends, when the owner ends it, or when Stop, Cancel, VM stop (including daily rotation) or VM loss ends the attempt. Allowed: a four-hour `docker-ci` finishes and Claude reports. Counterexample: the one-hour attempt clock ending the wait. | New; owner decision; a later continuation input states that the background work stopped. |
| [`g5.agents.lifecycle.owner-stop`](ontology/features/agents.g5.md#owner-stop-reliably-ends-any-agent-attempt) | The owner's Stop or Cancel ends a Claude or Codex attempt in every state (an active, silent or looping turn; for Claude also a background wait, a pending native request and a follow-up turn) through bounded interruption and forced reap, with a visible stopped state. Allowed: Stop ends a silent turn whose provider ignores SIGINT. Counterexample: a Stop that waits for the provider to cooperate. | New; owner condition for dropping automatic limits. |
| [`g5.agents.execution-deadlines`](ontology/features/agents.g5.md#distinguish-attempt-and-provider-deadlines) | The default deadline covers setup and input delivery and stops once the provider accepts the input. No default limit applies to Claude or Codex turns or to Claude waits. An explicitly requested limit bounds the whole attempt, and harvest keeps its grace. Allowed: a three-hour refactor turn completes. Counterexample: a requested two-hour workflow limit being ignored. | Modified. |
| [`g5.conversation.delivery`](ontology/features/conversations.g5.md#delivery-means-actual-transport-handoff) | Claude accepts input at the native user-frame write and flush; Codex at prompt write and EOF. | Preserved; marks where the default deadline stops. |
| [`g5.architecture.provider_adapter.claude-background`](ontology/software_architecture/agents.g5.md#interpret-claude-background-work-inside-the-adapter) | Only the Claude adapter interprets turn and background frames and emits `background_tasks.replaced` and sub-task events; TaskService and the dispatcher see ordinary events and one outcome. | New ownership. |
| [`g5.conversation.providers`](ontology/features/conversations.g5.md#use-each-native-provider-within-its-chosen-transport) | Claude 2.1.285; one attempt may span Claude follow-up turns. | Modified; exact-pin fixtures must be re-recorded. |
| [`g5.conversation.providers.codex`](ontology/features/conversations.g5.md#run-codex-through-its-chosen-noninteractive-transport) | Codex 0.159.3 exec JSONL, one prompt then EOF, no interactive cards. | Modified; the exec event mapping must be re-verified at the new pin. |
| [`g5.provider.catalog.pinned`](ontology/features/provider_accounts.g5.md#keep-offered-models-usable-with-the-pinned-clis) | Offered Claude and Codex models must work with the pinned CLI for their account types; an upgrade re-checks both lists. Allowed: dropping a model the new pin rejects for ChatGPT logins. Counterexample: offering a model whose first turn fails with "model not supported". | New; owner decision; refines the human-verified [`g5.provider.catalog`](ontology/features/provider_accounts.g5.md#keep-standard-codex-model-availability). |
| [`g5.conversation.continue`](ontology/features/conversations.g5.md#continuation-says-what-is-being-continued) | Continuation creates one system-origin input naming the prior execution and stop reason. | Preserved; its text also says the interrupted attempt's background work stopped. |
| [`g5.harness.provided`](ontology/features/ontology_and_harness.g5.md#provide-the-harness-in-every-workspace), [`g5.workflow.basic-chat`](ontology/features/ontology_and_harness.g5.md#basic-chat-workflow-for-simple-work) | Every workspace gets the same root instructions and four maintained skills; basic chat behaves close to chat without a harness. | Preserved: no runtime guidance is added to the harness. |
| [`g5.agents.model.provider-process`](ontology/features/agents.g5.md#track-the-provider-process-as-execution) | One provider process is the execution; its subagents get no second platform scheduler. | Preserved: the platform reports Claude's set and never schedules it. |
| [`g5.agents.outcome`](ontology/features/agents.g5.md#provider-output-is-not-the-final-execution-decision) | Success needs the terminal result, output closure and exit. | Preserved; for Claude, "terminal" is the last turn's result, after which input closed. |
| [`g5.agents.execution-limits`](ontology/features/agents.g5.md#freeze-and-enforce-effective-execution-limits), [`g5.agents.output-bounds`](ontology/features/agents.g5.md#bound-output-before-framing-and-parsing) | Frozen limits and raw-output budgets cover every turn of the attempt. | Preserved; follow-up turns consume the same budgets. |
| [`g5.agents.session-serial`](ontology/features/agents.g5.md#serialize-one-logical-conversation), [`g5.story.queue-a-follow-up`](ontology/features/conversations.g5.md#queue-a-follow-up) | One live attempt per session; follow-ups queue without interrupting. | Preserved; waits can be longer. |
| [`g5.agents.admission.occupancy`](ontology/features/agents.g5.md#keep-slot-accounting-faithful-to-work), [`g5.lifecycle.idle`](ontology/software_architecture/lifecycle.g5.md#idle-reflects-real-owner-activity) | A running or waiting attempt counts as bounded work: it holds its slot and prevents idle stop. | Preserved and intended; a forgotten attempt keeps the VM up until someone ends it or rotation stops the VM. |
| [`g5.lifecycle.deadline`](ontology/software_architecture/lifecycle.g5.md#deadline-shutdown-is-visible-and-bounded), [`g5.lifecycle.deadline.rotation`](ontology/software_architecture/lifecycle.g5.md#replace-cloud-capacity-before-draining-it), [`g5.runtime.lease`](ontology/software_architecture/runtime.g5.md#short-server-lease-and-long-credential-have-separate-purposes) | A planned shutdown or the daily rotation drain interrupts a running attempt after its 30-minute notice; the guest token lasts 24 hours. | Preserved; these are the effective ceilings on cloud hosts. |
| [`g5.conversation.request-lifetime`](ontology/features/conversations.g5.md#native-requests-expire-with-their-execution), [`g5.conversation.request-lifetime.deadline`](ontology/features/conversations.g5.md#bound-native-request-waiting) | Native requests from follow-up turns belong to the same attempt and keep their 15-minute bound. | Preserved; a pending request blocks input close. |
| [`g5.conversation.transcript.units`](ontology/features/conversations.g5.md#preserve-completed-assistant-units), [`g5.conversation.transcript.terminal`](ontology/features/conversations.g5.md#resolve-terminal-answer-text), [`g5.conversation.transcript.presentation`](ontology/features/conversations.g5.md#keep-streaming-and-history-in-agreement) | Ordered units, deduplicated terminal text, one reduction for live and history. | Apply per follow-up turn. |
| [`g5.agents.outcome.message-status`](ontology/features/agents.g5.md#project-assistant-status-from-settled-outcome) | Assistant status comes from settlement, so the reply stays in progress during the wait. | Preserved; the visibility block explains that state. |
| [`g5.conversation.bundle.harvest`](ontology/features/conversations.g5.md#fence-and-bound-session-harvesting), [`g5.conversation.recovery`](ontology/features/conversations.g5.md#recover-a-conversation-from-known-delivered-history) | Harvest at exit; later attempts reconstruct delivered history. | Preserved; the report text becomes delivered history for later attempts. |
| [`g5.architecture.agent_execution_tree`](ontology/software_architecture/agents.g5.md#execution-tree-ownership), [`g5.design.cleanup-proof`](ontology/concepts/system_design_principles.g5.md#prove-cleanup-before-releasing-resources) | Reap and prove at supervisor exit. | Preserved; Stop relies on it. |
| [`g5.conversation.providers.later`](ontology/features/conversations.g5.md#defer-resident-provider-interaction), [`g5.scope.later-work`](ontology/scope.g5.md#preserve-explicit-later-work) | Resident providers and live steering stay deferred; Codex has no background window. | Preserved; Codex follows the provider-neutral limit and Stop rules. |

## Requested outcome and scope

- **Claude background work:** a Claude attempt stays live while background tasks or background sub-agents that Claude owns are still running. The agent decides whether that work continues or stops, and Claude reports the results in its own follow-up turns.
- **CLI upgrades:** move Claude Code from 2.1.270 to 2.1.285 (the npm `stable` tag at planning time; `latest` was 2.1.286) and Codex from 0.153.3 to 0.159.3 (the npm `latest`). Re-check both providers' offered model lists against the new pins (owner decision).
- **Claude and Codex:** no default time limit on provider execution after input acceptance; an explicitly requested limit still applies; the owner's Stop reliably ends an attempt in every state.
- **Delivery:** one PR (owner decision).

Out of scope:
- A Codex background window, because `codex exec` ends its own background work when its turn completes.
- Resident processes across attempts.
- Live delivery of new owner input.
- Platform-owned long-running commands.
- Automatic hang, cost or turn-count guards (owner decision).

## Provider behavior evidence

Method: the same headless transport as the runtime (`claude -p --input-format stream-json --output-format stream-json`, and `codex exec --json` with the prompt then EOF). Each run started a 35–45 s job that writes a marker file when done, and told the model not to wait for it. Linux runs used the local G5-Lean host image (aarch64, uid 999) with the QA pins; 2.1.285 was installed on top of the same image. Claude 2.1.285 and Codex 0.158.0 on macOS gave the same pattern.

| Work still running when the turn ends | Claude 2.1.270 | Claude 2.1.285 | Codex 0.153.3 |
|---|---|---|---|
| Background Bash task, input closed at the result | Killed 5.2 s after EOF; exit at 10.0 s | Killed 5.2 s after EOF; exit at 10.5 s | Exec session killed; exit 0.6 s after the turn |
| Background sub-agent, input closed at the result | Waited about 43 s, ran a report turn, second `result`, then exited | Same, about 45 s | Spawned sub-agent never finished; exit 0.8 s after the turn |
| Background Bash task, input kept open | Woke on completion and reported in a new turn; stayed until input closed | Same | Not applicable: exec needs EOF |
| Process detached with `nohup` | Survived the CLI exit and finished | Same | Killed |

Each self-started report turn began with `system/init` and ended with its own `result`. Background state arrived as `background_tasks_changed`, `task_started`, `task_progress`, `task_updated` and `task_notification` system frames.

**In-band interrupt at 2.1.285** (Linux, 2026-09-30): the `background-agent-interrupt` and `background-agent-interrupt-turn` recordings, plus four probes:
- Claude answers the interrupt at once with a `control_response` success.
- Between turns it stops background sub-agents, starts no report turn and exits 0 after EOF.
- During a turn it also stops the turn's foreground command and ends the turn with `error_during_execution`. It then exits 1 after EOF, in all three runs.
- Background Bash jobs survive the interrupt in both cases and stop at EOF.
- Claude also emits `task_started` and `task_notification` frames with `is_backgrounded: false` for long foreground commands, both its own and its sub-agents'.

Limits of this evidence:
- The runs used Docker on aarch64, not the libkrun guest on x86_64.
- Codex's Linux sandbox (`bwrap`) cannot start in Docker, so the Codex rows used `danger-full-access`.
- The QA pins in production were not exercised end to end.

## Design

1. **Upgrade both CLI pins and their model lists.**
   - Bump `scripts/claude-code-cli-version` to 2.1.285 and `scripts/codex-cli-version` to 0.159.3, rebuild the images, and pass the CLI flag check.
   - Re-record the Claude exact-pin fixtures in `tests/fixtures/agent-turn-native/claude/`, adding background-task, background-sub-agent, report-turn and EOF-while-busy recordings.
   - Re-verify Codex exec JSONL handling at 0.159.3 in the mapper tests. There are no recorded Codex native fixtures.
   - Re-check the model catalogs in `backend/g5-contracts/src/claude_models.rs` and `codex_models.rs` (generated into `frontend/src/lib/harnessCatalog.gen.ts`) against what each new pin accepts for API-key and subscription accounts. Remove models that are no longer accepted, and add newly supported ones.
   - Update the exact-pin assertions and the pin notes in [Agent API v9](../docs/agent-api-surface-v9.md) and [design v9](../docs/lean-backend-design-v9.md).
   - On their own, these upgrades still run one turn per attempt.
2. **Remove default time limits from agent execution.** This applies to both Claude and Codex.
   - The supervisor's default deadline covers setup and input delivery and stops once the provider accepts the input: the native user-frame write for Claude, prompt write and EOF for Codex. Harvest after exit keeps its bounded grace.
   - A wall-clock limit that a task explicitly requests, for example by an unattended workflow, still bounds the whole attempt. This needs the request to distinguish an absent limit from the default.
   - The dispatcher's module clock must not end an agent attempt by default. Besides the work itself, only Stop, Cancel, the Claude End action, the output cap, VM stop (including daily rotation) and VM loss end it.
   - The control plane deploys first (design v9 §6.10). A VM that still runs the previous module catalog gets `agent.turn` 1.0.1 without the new scope field, and keeps the old attempt-wide default until it is replaced. This was amended after the independent CLI review.
3. **Make Stop reliable in every state.** Because nothing else ends a hung or runaway turn, the owner's Stop must reach the live execution and end it in every state:
   - an active, silent or looping Claude or Codex turn;
   - for Claude also a background wait, a pending native request or a follow-up turn.

   It reuses the issue 229 bounded interruption and forced reap. Stop stays visible and enabled throughout.
4. **Track Claude's state inside the adapter.** Three facts drive the close decision:
   - Whether Claude is in a turn: a turn starts at the input write or at `system/init`, and ends at its `result`.
   - The background set, replaced whole by each `background_tasks_changed` frame.
   - Any pending native request.

   Close input when Claude is between turns, the set is empty and no request is pending. With nothing running, that happens right after the first result, exactly as today. The 15-second exit grace starts at input close, not at the first result.
5. **Fold report turns into the attempt.**
   - Each follow-up turn's completed units append to the same assistant reply, with terminal-text deduplication per turn.
   - Usage sums across turns.
   - Any error result fails the attempt.
   - One outcome is emitted after exit.
   - A repeated `system/init` must not reset session identity or the delivered-input bookkeeping.
6. **Make the wait visible.**
   - The adapter emits `background_tasks.replaced` with bounded, sanitized entries (id, kind, description, start time), plus `subtask.*` events for background tasks only. Claude's task frames for long foreground commands are not background work.
   - The API projects them as non-error status facts.
   - The conversation shows a waiting panel on the live reply, with elapsed time and the owner's End action.
   - The composer explains that a queued message will be delivered after the wait.
7. **Let the owner end the wait.** The operation is a TaskService command authorized like Stop, delivered to the live adapter on the same control channel as native replies.
   - The adapter stops Claude's background work through a native control if the pin supports one; otherwise it uses the bounded stop path (interrupt, then reap).
   - It posts a notice ("background work stopped by the owner"), then settles.
   - A clean exit after the stop settles as success with that notice. A bounded stop settles as interrupted.
   - Amended after the independent CLI review:
     - End names the task and attempt whose wait the owner saw. TaskService refuses it once that attempt is no longer the running one.
     - Between turns the adapter interrupts at once. During a turn it first lets the turn run for up to ten seconds, so End never waits indefinitely.
     - If no background work remains, End does nothing more.
     - The aborted turn, and Claude's exit status 1 after it, are End's effect. Such a turn is recognized by an `aborted_*` terminal reason, and only when Claude did not refuse the interrupt.
     - Claude's input after the prompt is written through a non-blocking queue, so a provider that stops reading cannot hold End, its native requests or the attempt's clocks. A reply counts as delivered only once completely written, and a failed reply never overrides Claude's own clean finish.
8. **No harness change.** After #248 every workspace receives the repository-root harness ([`g5.harness.provided`](ontology/features/ontology_and_harness.g5.md#provide-the-harness-in-every-workspace)), and basic chat should behave close to chat without a harness, so this change adds no agent guidance there.
   - Claude already backgrounds long commands on its own.
   - A process the agent detaches itself is reaped at attempt end, as specified.
   - The one misleading case, history claiming work is still running after an interruption, is handled by the continuation input. It states that the interrupted attempt's background work stopped.

**Delivery: one PR** (owner decision), with commits in this order so each builds and passes its own tests:
1. This design: plan, request and ontology changes.
2. Claude Code 2.1.285, its re-recorded exact-pin fixtures and the Claude model list.
3. Codex 0.159.3, its verification tests and the Codex model list.
4. The behavior change: no default time limit and reliable Stop for both providers, the Claude background window with report turns, events and API projection, the owner's End, the waiting panel, Stop availability and the continuation wording.

Repairs from the reviews follow as their own commits. Amended during implementation: the planned split of the behavior change into three commits was merged into commit 4. The limit, background-window and End changes share the supervisor's attempt loop and one `agent.turn` wire version (module 1.0.2), so separate commits would need intermediate wire versions without separate rollback value. Each CLI upgrade stays its own commit, which keeps the upgrades bisectable and revertible on their own.

## Observable acceptance

Through the real dispatcher with fake providers that replay recorded frames:
- **Claude background task:** the attempt stays running after the first result with input open. The report turn's text appears in the same reply. The attempt succeeds with summed usage and a proven reap.
- **Claude background sub-agent:** the same.
- **Nothing in the background:** input closes at the first result, which is today's behavior.
- **Native request in a report turn:** it blocks input close and keeps its deadline.
- **Owner ends the wait:** the work stops, a notice is visible, and when Claude exits cleanly the task is neither cancelled nor held, so the next queued input runs. If Claude has to be stopped, the attempt settles interrupted and the task is held for Continue, like Stop.
  - End during a running turn takes effect within about ten seconds.
  - An End that names an earlier attempt is refused.
- **Queued owner input during the wait:** it is never written into the process. The next attempt delivers it, and its reconstructed history includes the report text.
- **Detached `nohup` process:** reaped at attempt end.
- **No automatic limit:** with an injected clock, a Claude turn, a Claude wait and a Codex turn each longer than the old one-hour deadline continue.
- **Explicit limit:** a task that requests a wall-clock limit still ends at it, for both providers.
- **Stop in every state:** Stop ends the attempt with reap proof during an active turn and a silent turn whose provider ignores SIGINT, for both providers. For Claude it also does so during a background wait, a pending native request and a follow-up turn.
- **Idle stop:** never fires during a running or waiting attempt. A planned shutdown or rotation interrupts it after its 30-minute notice.

Live check: on QA at the new pin:
- Ask Claude to run a five-minute background job, and confirm the report arrives without another user message.
- Repeat the original `docker-ci` scenario, which may run for hours.
- Confirm a Codex turn longer than one hour is not cut.

Gates:
- The CLI flag check and Claude recordings at 2.1.285.
- Codex mapper tests at 0.159.3.
- Model-list tests showing that every offered model is accepted by its pin.
- The existing suites.
- Cloud Rust and E2E on the PR head.

The live check also confirms that one model from each list completes a turn on QA.

## Risks and open assumptions

Source inspection during coding must settle:
- How the Claude mapper and runtime treat repeated `system/init` and `result` frames, and everything that currently keys off the first result.
- Whether each `result` frame carries per-turn or cumulative usage and cost, to avoid double counting.
- Whether an existing control path can carry the owner's End action to the live adapter.
- How the dispatcher's module clock and the supervisor's attempt clock bound an agent turn today for each provider, so that neither ends an attempt by default.
- Whether a task request can distinguish an absent wall-clock limit from the default; today the default fills an absent value.
- How the fake providers and recordings support multi-turn Claude attempts.
- Which Codex exec JSONL events changed between 0.153.3 and 0.159.3.
- How each pin reports the models it supports for API-key and ChatGPT-subscription accounts, so the model-list re-check is evidence rather than guesswork.

Settled during implementation (see Provider behavior evidence): at the new pin, Claude's in-band interrupt stops background sub-agents, and input EOF ends background Bash tasks. End therefore uses the interrupt and then EOF, with the bounded stop as a fallback.

**No automatic guard, for either provider.** A hung turn, a runaway loop or a Claude background task that never ends keeps the VM running, its slot occupied and the chat blocked until the owner ends it.
- A runaway loop also keeps spending model tokens, since the workspace spend cap only blocks new work.
- On cloud hosts, forced rotation at the host-credential cliff (24-hour TTL minus two hours) bounds this to under 24 hours.
- Self-hosted and local hosts are not force-rotated, so there only the owner's Stop bounds it.
- Unattended workflows should request an explicit limit.

**Daily rotation.** Multi-hour jobs are exposed to daily host rotation, which stops the VM after a 30-minute notice. Past the 24-hour guest token, in-guest platform calls fail, and the provider credential must still be valid when Claude wakes for its report turn.

**Continuation after loss.** After VM loss and continuation, reconstructed history still says work is running in the background when that work is gone. The continuation input states that it stopped, because the provided harness carries no runtime guidance.

**One PR.** The review is larger, and neither CLI upgrade can be reverted on its own; the ordered commits mitigate both. Both upgrades land together with the behavior changes, so a regression on QA needs bisecting by commit.

## Owner decisions

1. New owner messages during a wait queue until the wait ends; they are never written into the waiting process ([`g5.conversation.providers.claude-background.input`](ontology/features/conversations.g5.md#queue-owner-input-behind-the-waiting-attempt)).
2. When the owner ends the wait and Claude exits cleanly, the attempt succeeds with a notice; a bounded stop settles as interrupted ([`g5.conversation.providers.claude-background.end`](ontology/features/conversations.g5.md#let-the-owner-end-background-waiting)).
3. Background waiting has no time limit, because `docker-ci` can run for hours ([`g5.conversation.providers.claude-background.deadline`](ontology/features/conversations.g5.md#do-not-time-limit-claude-background-waiting)).
4. Agent turns have no automatic guard either, on the condition that the owner has a reliable way to end the turn; the worst case is under 24 hours on cloud hosts ([`g5.agents.lifecycle.owner-stop`](ontology/features/agents.g5.md#owner-stop-reliably-ends-any-agent-attempt), [`g5.agents.execution-deadlines`](ontology/features/agents.g5.md#distinguish-attempt-and-provider-deadlines)).
5. Rules 3 and 4 apply to Codex as well as Claude.
6. Ship everything in one PR.
7. Upgrade Codex to 0.159.3 in the same PR, and re-check both providers' model lists against the new pins ([`g5.provider.catalog.pinned`](ontology/features/provider_accounts.g5.md#keep-offered-models-usable-with-the-pinned-clis)).

## Unresolved questions

No open owner questions. A reminder for very long waits was not requested; if it is added later, it notifies and never stops the work.

## Design review

The alternate route went through lifecycle idle and deadline shutdown, admission occupancy, native request lifetime, transcript units, outcome evidence, the execution tree and the scope deferrals. It found no conflicting authority:
- **Provider-process tracking:** holds, because the adapter reports Claude's background set and never schedules it.
- **One input per attempt and the resident-interaction deferral:** both hold, because new owner input still queues.
- **Attempt deadline:** amended so provider execution has no default limit after input acceptance, while setup, harvest and explicitly requested limits stay bounded. Reliable Stop and forced cloud rotation replace the automatic guard.
- **Codex:** the deadline rule and the Stop requirement are provider-neutral. The Codex transport, its lack of a background window and its non-interactive policy are unchanged.
- **Idle stop:** a running or waiting attempt counts as bounded work, which keeps the VM up as intended.
- **Workflow tasks:** they use the same attempts; unattended ones should request explicit limits.
- **Harness:** #248's repository-root harness is shipped unchanged; the change adds no runtime guidance to it, and the continuation input carries the one needed fact.
- **Credentials:** descendants keep the attempt's existing credentials only for the attempt's lifetime; no new authority is introduced.

The remaining assumptions are implementation facts, to be settled by source inspection before dependent code.
