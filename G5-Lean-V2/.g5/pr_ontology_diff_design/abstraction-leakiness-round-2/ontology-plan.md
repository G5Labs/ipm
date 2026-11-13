# Abstraction leakiness, round 2: seven-PR plan

The round-2 review of 2026-10-03 (kept outside the repository, like round 1's audit) found that round 1 sealed table writes. What leaks now is owner logic copied between owners, authority checks made in routes, and nine behavior bugs. This plan fixes the bugs first, then the leaks that are critical and cheap. It fits round 1's budget, which the user confirmed: five PRs, each under 5,000 changed lines. The user added PRs 6 and 7 beyond it.

How to read it: [PR sequence](#pr-sequence) is the plan. Each PR names the bug or leak it closes and the test that must fail before the change. The user approved the plan and its three open points on 2026-10-03, and on 2026-10-04 added PR 6 for a race that PR 4's review found in paths PR 4 did not touch. PR 7 carries the owner's 2026-10-04 decision that Stop in a chat ends the turn; the owner approved its design the same day.

## Ontology changes and governance

**Comparison base:** `origin/main` at `f19cc664`.

**Ontology changes:** one new decision. Existing ontology covers every other PR: the code does not yet follow those rules.

- Added `g5.runtime.events.org-scope` in `software_architecture/runtime.g5.md`, approved by the user. Host control applies a host's facts in a transaction scoped to the host's organization, so release settlement works for host-reported releases. Its metadata records the gap.
- The `g5.lifecycle.stop.release` metadata links to it.

```diff
diff --git a/.g5/ontology/software_architecture/runtime.g5.md b/.g5/ontology/software_architecture/runtime.g5.md
@@ -88,6 +88,15 @@
 Journal host facts durably with sequence and atomically ingest deduplicated facts and applicable current-incarnation transitions before acknowledging a contiguous prefix.
 
+## Host facts apply under the host's organization
+
+[D · g5.runtime.events.org-scope]
+
+Host control applies a host's facts, and the owner effects they trigger, in a transaction scoped to
+the host's organization, so [release settlement](lifecycle.g5.md#release-only-the-stopped-incarnation)
+runs under the same row rules as any other caller. Widening the system role's table access was
+rejected: every system path would then see all organizations' tasks and transcripts.
+
@@ -290,6 +299,12 @@
+### g5.runtime.events.org-scope
+
+- human-verified
+- Proposed by the [round-2 leakiness plan](../../pr_ontology_diff_design/abstraction-leakiness-round-2/ontology-plan.md) for the [round-2 request](../../chat_intent/abstraction-leakiness/round-2.md), after the round-1 review found that host-reported releases settle no task or message row; approved by the user on 2026-10-03.
+- Unassessed · not implemented. Host control runs on the system pool (`backend/g5-api-core/src/services/mod.rs:491-494`), where row security hides `tasks` and `chat_messages`, so its releases settle only the preview intent on `workspace_vms`.
+
diff --git a/.g5/ontology/software_architecture/lifecycle.g5.md b/.g5/ontology/software_architecture/lifecycle.g5.md
@@ -217,7 +217,7 @@ (g5.lifecycle.stop.release metadata, last bullet; unchanged text elided)
-… The percentage was not reassessed.
+… The percentage was not reassessed. The proposed fix is [host facts under the host's organization](runtime.g5.md#host-facts-apply-under-the-hosts-organization).
```

**PR 7 (designed and approved 2026-10-04):** the owner decided that Stop in a chat ends the turn ([chat intent](../../chat_intent/abstraction-leakiness/round-2.md)). Two new decisions and three edits:
- `g5.agents.lifecycle.chat-stop` (new, the owner's decision): Stop in a chat cancels the turn it names; queued messages run next; a job's turn keeps today's hold.
- `g5.agents.lifecycle.chat-held-turn` (new, the agent's proposal, approved with the design): the next message ends a chat turn held for Continue, unless its input's delivery is unknown.
- `g5.agents.session-serial` gains the chat exception; Claude's End (`g5.conversation.providers.claude-background.end`) ends the turn like Stop; the open question `g5.lifecycle.start.held-work` narrows to job work; TaskService's surface lists chat turn ending.

```diff
diff --git a/.g5/ontology/features/agents.g5.md b/.g5/ontology/features/agents.g5.md
index a925251b..6f3072df 100644
--- a/.g5/ontology/features/agents.g5.md
+++ b/.g5/ontology/features/agents.g5.md
@@ -138,2 +138,16 @@ Because no default time limit ends provider execution, the owner's Stop or Cance
 
+### Stop in a chat ends the turn
+
+[D · g5.agents.lifecycle.chat-stop]
+
+Stop in a chat [cancels](#make-cancellation-terminal-for-the-logical-task) the turn it names, like most chat apps: the reply keeps its partial output, [marked stopped](#project-assistant-status-from-settled-outcome), and messages queued behind the turn stay queued and start the next turn. Stop on a message still waiting behind another turn withdraws just that message. The owner's next message also starts a new turn, whose prompt carries the stopped reply, so no Continue is needed. A Stop for a turn that already ended changes nothing and never reaches a later turn.
+
+A turn another owner follows is still held for [Continue](#continue-interrupted-work): a job's turn, because cancelling it would fail the job's dependent work, and a turn the platform started, such as a merge-conflict resolution, because its merge run follows that task. The task's own Stop and hold remains the way to pause any task for Continue.
+
+### A new message ends a held chat turn
+
+[D · g5.agents.lifecycle.chat-held-turn]
+
+When a chat's turn is held for Continue (after a workspace stop, idle stop, VM loss, transfer, a restart the host needed, or a question that went unanswered), the owner's next message ends that turn as [Stop](#stop-in-a-chat-ends-the-turn) does and runs next, also a message sent while the turn was still stopping, once the turn is held. A message of that turn the agent never received stays queued and runs first, because the owner did not stop it. A turn held because its input's delivery is unknown still waits for the owner's redelivery choice. A job's turn, and a turn the platform started, such as a merge-conflict resolution its merge run follows, still wait for Continue.
+
 ### Reserve once before invoking
@@ -294,3 +308,3 @@ Session tags alone must not select accounts or prove attribution; trusted host r
 
-A session has at most one nonterminal logical agent task and one live attempt, including held tasks without a process. Queued new input cannot bypass the hold. Distinct sessions may execute concurrently in the shared checkout.
+A session has at most one nonterminal logical agent task and one live attempt, including held tasks without a process. Queued new input cannot bypass the hold; in a chat it [ends the held turn](#a-new-message-ends-a-held-chat-turn) first. Distinct sessions may execute concurrently in the shared checkout.
 
@@ -453,3 +467,3 @@ consolidation. Copying the memory directory does not remove these limits.
 - From [C106–C110](../../uplift/g5-lean/candidates/contracts.md).
-- ~90% · task_lifecycle cancellation and focused ambiguous-write settlement pass; full cancellation/continue race matrix unexecuted.
+- ~90% · task_lifecycle cancellation and focused ambiguous-write settlement pass; full cancellation/continue race matrix unexecuted. Since round-2 PR 7 a requested cancellation is final even when the attempt reports an interruption or an unanswered question first (`outcome_decision`; `backend/g5-api/tests/api_chat_turn_stop.rs::stop_is_final_whatever_ends_the_attempt_first`).
 
@@ -461,2 +475,14 @@ consolidation. Copying the memory directory does not remove these limits.
 
+### g5.agents.lifecycle.chat-stop
+
+- human-provided, human-verified
+- The owner's decision of 2026-10-04 in the [round-2 chat intent](../../chat_intent/abstraction-leakiness/round-2.md), citing H023 ("cancel is required; pause is not needed", [human decisions](../../uplift/g5-lean/candidates/human-decisions.md)). The job exception is the agent's, from the [cancellation rule](#make-cancellation-terminal-for-the-logical-task)'s effect on dependents; the owner confirmed it with the design ("implement PR 7"). Context carries over through the [frozen chat history](conversations.g5.md#continuation-says-what-is-being-continued), which includes stopped replies; providers start fresh on every turn.
+- ~90% of these rules, by API tests · `backend/g5-api-core/src/services/task_lifecycle.rs::end_chat_turn`, called by the chat's Stop route, resolves the named input or reply to its turn under the session's turn allocation lock: a running turn's attempt is cancelled (`request_workspace_task_cancel`), a queued or held turn ends in `end_chat_turn_now`, a message waiting behind another turn is withdrawn, whether the owner typed it or the platform sent it (`withdraw_waiting_input_for_stop`), an ended turn is left alone, and a turn another owner follows, a job's or one the platform started (`turn_kept_for_its_owner`), is paused (`request_workspace_task_pause`). Settlement keeps the queued messages and `ensure_next_queued_agent_task` starts them. A turn's own message is the earliest one bound to its task that is not withdrawn. Tests: `backend/g5-api/tests/api_chat_turn_stop.rs` (running turn, a platform-started turn, an interruption or unanswered question racing the stop, the next message's prompt, waiting message, a waiting platform message, a waiting message in a job's chat, late Stop, a carried message, held turn, a queued turn, a turn held before its first attempt, job turn). Gaps: stopped turns record no usage ([owner stop](#owner-stop-reliably-ends-any-agent-attempt)); the browser's Stop was not run against it locally.
+
+### g5.agents.lifecycle.chat-held-turn
+
+- human-verified
+- Proposed in the round-2 plan (PR 7) from the owner's "No Continue is needed in the chat": a turn held for any other reason has no Continue in the chat either. The chat already offers Redeliver for an input whose delivery is unknown. The owner confirmed it with the design ("implement PR 7", [round-2 chat intent](../../chat_intent/abstraction-leakiness/round-2.md)). The platform-turn exception was added in the PR's review, because a merge run follows its resolution task; the owner confirmed it on 2026-10-04 ("the platform-turn exception looks right, keep it").
+- ~90% of this rule, by API tests · `backend/g5-api-core/src/services/agent_dispatch.rs::enqueue_agent_input` ends a typed chat's held turn (any hold but `delivery_unknown`, no job) with `task_lifecycle.rs::end_chat_turn_now` before it queues the owner's message, and moves the ended turn's queued messages into the new turn ahead of it; dispatch reserves a turn's earliest queued message, waiting while an edit holds it, so the carried message runs first. A turn whose own message the platform sent keeps its hold (`turn_kept_for_its_owner`). Settlement that holds a turn ends it at once when an owner's message is already queued behind it (`task_lifecycle.rs::hand_off_held_chat_turn`). Tests: `backend/g5-api/tests/api_chat_turn_stop.rs::a_new_message_ends_an_idle_held_turn_and_its_undelivered_message_runs_first`, `::a_new_message_ends_a_turn_held_after_an_unanswered_question`, `::a_turn_of_unknown_delivery_still_waits_for_redelivery`, `::a_job_turn_keeps_the_hold`, `::a_held_platform_turn_waits_for_continue`, `::a_message_sent_while_the_turn_stops_ends_it_once_held`, `::messages_run_in_order_while_the_earliest_is_being_edited`. Gap: holds after a workspace stop, VM loss, transfer or a needed restart take the same path but were tested through the idle-stop and unanswered-question holds only.
+
 ### g5.agents.invoke
@@ -585,3 +611,3 @@ consolidation. Copying the memory directory does not remove these limits.
 - From [C108](../../uplift/g5-lean/candidates/contracts.md); human-verified outcome, agent-derived enforcement: [H007/H018/H057/H072](../../uplift/g5-lean/candidates/human-decisions.md), [v9](../../../docs/lean-backend-design-v9.md).
-- ~90% · session/task constraints and dispatch hold logic. Agent dispatch omits the optional job cap.
+- ~90% · session/task constraints and dispatch hold logic. Agent dispatch applies the optional job cap (`agent_dispatch.rs::job_admits_task_sql!`; `backend/g5-api/tests/api_task_lifecycle.rs::job_agent_tasks_are_admitted_under_the_job_concurrency_cap`). The chat exception is `enqueue_agent_input`'s, under [the held-turn rule](#a-new-message-ends-a-held-chat-turn).
 
diff --git a/.g5/ontology/features/conversations.g5.md b/.g5/ontology/features/conversations.g5.md
index cb5dc204..59b89cf3 100644
--- a/.g5/ontology/features/conversations.g5.md
+++ b/.g5/ontology/features/conversations.g5.md
@@ -239,3 +239,3 @@ While an attempt waits on background work, the conversation shows Claude's repor
 
-The owner can end the wait without cancelling the logical task. The adapter stops Claude's background work through Claude's native controls where the pinned CLI supports them, otherwise through the bounded stop path. A visible notice records that the work was stopped. If Claude then exits cleanly with successful turn results, the attempt succeeds; a bounded stop settles as interrupted, which holds the task for Continue like Stop. A turn that the End interrupt itself aborts, and the exit status Claude reports for it, are the End's effect, not a failed turn result. [Cancel](agents.g5.md#make-cancellation-terminal-for-the-logical-task) and Stop keep their meanings.
+The owner can end the wait without cancelling the logical task. The adapter stops Claude's background work through Claude's native controls where the pinned CLI supports them, otherwise through the bounded stop path. A visible notice records that the work was stopped. If Claude then exits cleanly with successful turn results, the attempt succeeds; a bounded stop ends the turn as the chat's [Stop](agents.g5.md#stop-in-a-chat-ends-the-turn) does. A turn that the End interrupt itself aborts, and the exit status Claude reports for it, are the End's effect, not a failed turn result. [Cancel](agents.g5.md#make-cancellation-terminal-for-the-logical-task) and Stop keep their meanings.
 
@@ -413,3 +413,3 @@ Automatic first-request or best-effort summary naming uses the existing turn’s
 - From [H078](../../uplift/g5-lean/candidates/human-decisions.md), [v9](../../../docs/lean-backend-design-v9.md).
-- ~85% · routes/chat.rs::send_message, agent_dispatch.rs, session-input UI in ChatPanel.tsx; durable FIFO and one reserved input exist; complete continuation provenance and late acceptance handling remain incomplete.
+- ~85% · routes/chat.rs::send_message, agent_dispatch.rs, session-input UI in ChatPanel.tsx; durable FIFO and one reserved input exist; complete continuation provenance and late acceptance handling remain incomplete. Since round-2 PR 7, dispatch reserves a turn's earliest queued input; while another transaction (an edit) holds it, that turn waits and the pass tries the next one, where it used to skip ahead to a later input (`backend/g5-api/tests/api_chat_turn_stop.rs::messages_run_in_order_while_the_earliest_is_being_edited`, `::a_held_message_does_not_stall_other_chats`).
 
@@ -575,3 +575,3 @@ Automatic first-request or best-effort summary naming uses the existing turn’s
 - The owner [chose](../../chat_intent/claude-background-work/request.md#decisions) success with a notice when Claude exits cleanly after the wait ends; the control exists so an unwanted or never-ending background task cannot hold the session.
-- Implemented · `POST /api/v1/sessions/:id/background/end` (owner, Claude attempts) → `TaskService::end_session_background_wait` → private dispatcher `modules.background.end` → control line → `advance_claude_input`, which sends Claude's in-band `interrupt` (recorded at 2.1.285 between turns and during a turn: Claude answers at once and stops background sub-agents; background commands stop at input EOF), invalidates a sub-agent's pending request and closes input; after an earlier close it applies the bounded stop. Claude 2.1.285 exits 1 at EOF after an interrupt aborted a turn (`background-agent-interrupt-turn` recording); the outcome treats that status as End's effect (`CLAUDE_INTERRUPTED_TURN_EXIT_CODE`, pinned to the recording). A turn counts as aborted by End only when Claude did not refuse the interrupt and its result carries an `aborted_*` terminal reason; a refused interrupt followed by an unrelated error fails the attempt (`refused`). Process tests cover clean, late-reply, stubborn, deaf, pending-request, after-close, interrupted-report-turn, busy-turn and refused-interrupt providers and two recorded-session replays; without the exit-status rule the interrupted report turn settled `failed`. The API test covers owner-only, Codex refusal, an attempt whose snapshot names no provider, no running attempt and stale task or attempt identities ([review](../../ontology-review-claude-background-work.md)).
+- Implemented · `POST /api/v1/sessions/:id/background/end` (owner, Claude attempts) → `TaskService::end_session_background_wait` → private dispatcher `modules.background.end` → control line → `advance_claude_input`, which sends Claude's in-band `interrupt` (recorded at 2.1.285 between turns and during a turn: Claude answers at once and stops background sub-agents; background commands stop at input EOF), invalidates a sub-agent's pending request and closes input; after an earlier close it applies the bounded stop. Claude 2.1.285 exits 1 at EOF after an interrupt aborted a turn (`background-agent-interrupt-turn` recording); the outcome treats that status as End's effect (`CLAUDE_INTERRUPTED_TURN_EXIT_CODE`, pinned to the recording). A turn counts as aborted by End only when Claude did not refuse the interrupt and its result carries an `aborted_*` terminal reason; a refused interrupt followed by an unrelated error fails the attempt (`refused`). Process tests cover clean, late-reply, stubborn, deaf, pending-request, after-close, interrupted-report-turn, busy-turn and refused-interrupt providers and two recorded-session replays; without the exit-status rule the interrupted report turn settled `failed`. The API test covers owner-only, Codex refusal, an attempt whose snapshot names no provider, no running attempt and stale task or attempt identities ([review](../../ontology-review-claude-background-work.md)). Since round-2 PR 7, the outcome settlement ends a chat turn whose result is `background_wait_ended` instead of holding it, and holds a turn another owner follows, a job's or one the platform started (`task_lifecycle.rs`; `backend/g5-api/tests/api_chat_turn_stop.rs::claude_end_that_stops_claude_ends_a_chat_turn`, `::claude_end_that_stops_claude_holds_a_job_turn`, `::claude_end_that_stops_claude_holds_a_platform_turn`).
 
diff --git a/.g5/ontology/software_architecture/agents.g5.md b/.g5/ontology/software_architecture/agents.g5.md
index 630eea10..69243e0d 100644
--- a/.g5/ontology/software_architecture/agents.g5.md
+++ b/.g5/ontology/software_architecture/agents.g5.md
@@ -16,3 +16,3 @@ Its owners share task/session identities without making provider output the plat
 |---|---|---|
-| TaskService | Allocate, stop-and-hold, continue, cancel, native reply, `apply(event)` | [Logical-task owner](#one-logical-task-owner) |
+| TaskService | Allocate, end a chat turn, stop-and-hold, continue, cancel, native reply, `apply(event)` | [Logical-task owner](#one-logical-task-owner) |
 | Dispatch coordinator | `reserve_*`, `invoke_*`, result application | [Commit phases](#dispatch-transaction-phases) |
@@ -32,3 +32,3 @@ Its owners share task/session identities without making provider output the plat
 
-TaskService exposes allocation, stop-and-hold, continue, cancel, native reply and `apply(event)` operations for all execution kinds using scoped persistence and [Workspace admission](workspace.g5.md#one-live-runtime-facade).
+TaskService exposes allocation, chat turn ending, stop-and-hold, continue, cancel, native reply and `apply(event)` operations for all execution kinds using scoped persistence and [Workspace admission](workspace.g5.md#one-live-runtime-facade).
 
diff --git a/.g5/ontology/software_architecture/lifecycle.g5.md b/.g5/ontology/software_architecture/lifecycle.g5.md
index 72cffdea..74c8d2b8 100644
--- a/.g5/ontology/software_architecture/lifecycle.g5.md
+++ b/.g5/ontology/software_architecture/lifecycle.g5.md
@@ -175,3 +175,3 @@ Recover a broken VM or pick up a new image through Stop, then a fresh explicit S
 
-When an owner's Start brings the workspace back to ready, should agent and command work held by a stop return to the queue by itself, or wait for the owner to [continue each task](../features/agents.g5.md#continue-interrupted-work)? A turn the owner stopped from its chat carries the same hold as a workspace Stop, so releasing held work on Start would restart that turn too. Until answered, held work waits for Continue.
+When an owner's Start brings the workspace back to ready, should a job's agent and command work held by a stop return to the queue by itself, or wait for the owner to [continue each task](../features/agents.g5.md#continue-interrupted-work)? Chats need neither: [Stop in a chat](../features/agents.g5.md#stop-in-a-chat-ends-the-turn) ends the turn and the [next message ends a held one](../features/agents.g5.md#a-new-message-ends-a-held-chat-turn). A task's Stop and hold shares a workspace Stop's hold reason, so releasing held work on Start would also resume work the owner paused on purpose. Until answered, held work waits for Continue.
 
@@ -344,3 +344,3 @@ When an owner's Start brings the workspace back to ready, should agent and comma
 - agent, uncertain
-- Found while implementing [host facts under the host's organization](runtime.g5.md#host-facts-apply-under-the-hosts-organization) in round-2 PR 2: scoping Ready would make `backend/g5-api-core/src/services/task_lifecycle.rs::release_workspace_tasks_after_resume` take effect for the first time. That step and the [v9 hold table](../../../docs/agent-api-surface-v9.md#221-canonical-task-and-attempt-state) clear stop holds on an explicit Resume; the [continue story](../features/agents.g5.md#continue-interrupted-work) has the owner continue explicitly, and per-turn Stop (`request_workspace_task_pause`) sets the same `owner_stop` hold.
+- Found while implementing [host facts under the host's organization](runtime.g5.md#host-facts-apply-under-the-hosts-organization) in round-2 PR 2: scoping Ready would make `backend/g5-api-core/src/services/task_lifecycle.rs::release_workspace_tasks_after_resume` take effect for the first time. That step and the [v9 hold table](../../../docs/agent-api-surface-v9.md#221-canonical-task-and-attempt-state) clear stop holds on an explicit Resume; the [continue story](../features/agents.g5.md#continue-interrupted-work) has the owner continue explicitly, and per-turn Stop (`request_workspace_task_pause`) sets the same `owner_stop` hold. Narrowed in round-2 PR 7 after the owner decided that [Stop in a chat ends the turn](../features/agents.g5.md#stop-in-a-chat-ends-the-turn).
 - Unassessed · Ready is left unscoped (`host_control_state.rs::settles_agents_work`), so the release step matches no task row and held work waits for `continue_workspace_task`.
```

**Relevant and governing ontology**

| Block | Why relevant | Implementation must |
|---|---|---|
| [`g5.agents.dependencies`](../../ontology/features/agents.g5.md#dependencies-belong-to-logical-tasks) | Edges live in `task_dependencies`; readers use the empty legacy array | Read edges only from the edge table (PR 1) |
| [`g5.agents.admission`](../../ontology/features/agents.g5.md#one-admission-policy-covers-jobs-and-individual-work) | Agent reservation skips the job cap | Apply one predicate, with the cap, to agents and commands (PR 1) |
| [`g5.agents.allocation.specification`](../../ontology/features/agents.g5.md#validate-task-specifications-at-allocation) | JobService's specs are stored unchecked | Validate with dispatch's rules before any allocation write; a bad spec fails its own task, never the organization's dispatch pass (PR 1) |
| [`g5.lifecycle.deadline.rotation`](../../ontology/software_architecture/lifecycle.g5.md#replace-cloud-capacity-before-draining-it), [`g5.architecture.placement`](../../ontology/software_architecture/worker_host.g5.md#capacity-and-runtime-placement) | Rotation's sweep claims operator drains | Rotation acts only on its own drains (PR 1); forced host removal is one WorkerHostService operation (PR 4) |
| [`g5.runtime.events.org-scope`](../../ontology/software_architecture/runtime.g5.md#host-facts-apply-under-the-hosts-organization) (new), [`g5.lifecycle.stop.release`](../../ontology/software_architecture/lifecycle.g5.md#release-only-the-stopped-incarnation), [`g5.preview.service.replace`](../../ontology/features/preview_terminal.g5.md#stop-the-previous-preview-before-replacing-it) | Host-reported releases settle nothing, so a queued preview can restart by itself | Settle in the release transaction under the host's organization (PR 2) |
| [`g5.architecture.conversations`](../../ontology/software_architecture/agents.g5.md#shared-conversations-and-inbox), [`g5.architecture.task_owner`](../../ontology/software_architecture/agents.g5.md#one-logical-task-owner) | Each owner's data shapes are decoded by the other | One reader per shape, held by its owner; live chat events built by ChatService (PR 3) |
| [`g5.architecture.flush_owner`](../../ontology/software_architecture/workspace.g5.md#flush-owns-the-durable-snapshot), [`g5.git.receipt`](../../ontology/software_architecture/durability.g5.md#only-verified-complete-capture-advances-recovery) | The receipt's Git proof is in a route | The flush service proves the receipt itself (PR 4) |
| [`g5.security.admission`](../../ontology/concepts/security.g5.md#admit-actions-against-current-authority), [`g5.design.authority`](../../ontology/concepts/system_invariants.g5.md#each-fact-has-one-source-of-truth) | VM-token checks are copied 5 times; push permission is copied once | One admission reader in core; push permission from PermissionService (PR 4) |
| [`g5.architecture.api_layers`](../../ontology/software_architecture/boundaries.g5.md#api-adapters-depend-on-domain-operations), [`g5.architecture.commands.writers`](../../ontology/software_architecture/boundaries.g5.md#declare-table-writers-and-check-them) | Routes assemble lifecycle operations; `worker_hosts` is unguarded | Routes only translate; fence the inbound hatches; guard `worker_hosts` (PR 4) |
| [`g5.architecture.runtime_facade`](../../ontology/software_architecture/workspace.g5.md#one-live-runtime-facade), [`g5.preview.transport`](../../ontology/features/preview_terminal.g5.md#preview-transport-preserves-owner-authority), [`g5.terminal`](../../ontology/features/preview_terminal.g5.md#terminal-is-the-owner-escape-hatch) | Wire formats are written by hand on both ends; profile names disagree | Shared types and one profile rule; preview streams bound to the exact incarnation (PR 5) |
| [`g5.agents.lifecycle.cancel`](../../ontology/features/agents.g5.md#make-cancellation-terminal-for-the-logical-task), [`g5.agents.outcome.message-status`](../../ontology/features/agents.g5.md#project-assistant-status-from-settled-outcome), [`g5.agents.session-serial`](../../ontology/features/agents.g5.md#serialize-one-logical-conversation) | Chat Stop holds the turn for a Continue the chat never offers | End the named turn through cancellation's settlement, keeping queued messages for the next turn (PR 7) |
| [`g5.quality.delivery.promotion`](../../ontology/concepts/deployment.g5.md#promote-the-qualified-immutable-release) | PR 5 changes worker-image inputs | Keep the wire compatible with guests still on the old image (PR 5) |

## Outcome and scope

**Requested outcome:** re-evaluate leakiness, then plan the next round of refactor and fixes.

**Behavior bugs the review found** (none was run; each PR proves its own with a failing test first):

1. Host-reported VM releases (exit, out of memory, missing inventory, stale kill, boot failure) settle no task or message row, so a queued preview can start by itself later and notices stay undelivered.
2. Cloud rotation's recovery sweep treats an operator drain as its own leftover and can replace and tear down that host.
3. Job graphs show no dependency edges: every read model uses the empty `tasks.depends_on` array.
4. Agent dispatch ignores a job's concurrency cap, which matters since workflow agent tasks became dispatchable.
5. A malformed command spec makes reservation return an error, which ends the coordinator's pass for the whole organization.
6. Live chat chunks probably carry no `messageIndex`: the coordinator reads the message on the system pool.
7. A preview profile named with `_` or upper case is accepted by the API but refused by the guest.
8. An open job output view polls every 5 seconds forever once any task succeeds.
9. A session's cost lags by up to 5 minutes: only the repair pass sets it.

**How the five were chosen:** behavior bugs first, then leaks that are critical (wrong behavior or authority, or new leaks going unnoticed) and cheap (mostly moving code, checkable with existing tests plus one or two new ones).

| Measure, on `main` at `f19cc664` | Now | After the plan |
|---|---|---|
| Behavior bugs from the review | 9 | 1 (session cost lag, which needs a product decision) |
| Authority decisions made in routes | 6 (VM-token check ×3, flush proof, push permission, forced host removal) | 0 |
| Public inbound hatches in core | 3 (host-control backend, stop without an actor, `record_forced_vm_loss`) | 0, pinned by `crate_private_fences.rs` |
| Decoders of the attempt snapshot | 4 | 1, kept by a source guard |
| Wire formats written by hand on both ends | 2 (port prefix, terminal protocol) | 0 |
| Core tables under the writer guard | 12 | 13 (adds `worker_hosts`) |

## High-level design

```text
Problem                           Fix                                         PR
--------------------------------  ------------------------------------------  ---
Bugs inside existing rules        owner reads the authority, checks at entry  1
Host releases settle nothing      host control scoped to the host's org       2
Shapes decoded by two owners      one reader per shape, owner-held, guarded   3
Authority checks in routes        owner operations; routes translate          4
Hand-written wire formats         shared contract types, exact incarnation    5
Leases checked before a lock wait lease read once the rows are held        6
Chat turns held for a Continue    Stop ends the turn, the queue moves on     7
```

Every fix moves a decision to the owner the ontology already names, or makes the code follow a rule that already exists. PR 2 is the exception: it adds one rule, that host control acts within the host's organization, instead of widening the system role. Owner operations keep taking the caller's connection, so no commit point moves.

## PR sequence

One at a time, in this order.

| PR | Title | Closes | Size | Decision |
|---|---|---|---|---|
| 1 | Task and dispatch bugs | bugs 2, 3, 4, 5, 8 | ~800 | No |
| 2 | Host-reported releases settle | bug 1 | ~300 | Approved: `g5.runtime.events.org-scope` |
| 3 | One reader per shared shape | bug 6; 3 duplication leaks | ~600 | No |
| 4 | Authority checks in their owners | 6 route decisions, 3 hatches | ~1,500 | No; two owner choices, stated below |
| 5 | Preview and terminal contracts | bug 7; 2 wire formats | ~450 | Approved: kebab-case profile names |
| 6 | Lease checks after lock waits | 7 sibling paths of PR 4's lease race | ~700 | Requested by the user on 2026-10-04 |
| 7 | Chat Stop ends the turn | stranded chats after Stop or an interrupted turn | ~900 | The owner's decision of 2026-10-04; design approved the same day |

**Why this order:**

- PR 1 fixes the bugs that need no decision.
- PR 2 follows because PR 3's live-event fix runs background code under an organization scope too.
- PR 4 is the largest and touches push and token security, so it lands after the simpler ones.
- PR 5 is last of the original five because it rebuilds the worker image.
- PR 6 and PR 7 were added on 2026-10-04 and stack on PR 5 in that order.

### PR 1 — Task and dispatch bugs

- **Dependency edges:** the task read models (`task_service.rs:102-108, 324-330`, `job_service.rs:1457-1470`) and the progress graph read edges from `task_dependencies`.
  - Test: a submitted graph's task views and progress graph show its edges.
- **Job cap:** agent reservation (`agent_dispatch.rs:1160-1163`) applies the same cap check as commands (`:909-912`).
  - Test: more agent children than the cap, allocated through JobService in a workspace whose slots exceed the cap.
- **Spec checks:** allocation runs dispatch's validators before any write. JobService gets crate-visible builders instead of literal schema ids. Command reservation fails a bad task instead of returning an error that ends the coordinator's pass (`agent_dispatch.rs:938-939`, `agent_tasks.rs:168-174`).
  - Tests: a malformed spec writes no row; a bad queued command no longer blocks an agent task in the same organization.
- **Rotation drains:** the recovery sweep (`cloud/rotation.rs:334-342`) selects only rotation's own drains, and the stale comment goes.
  - Test: an operator-drained idle host survives `rotate_once`.
- **Job polling:** `useJobs.ts:584-589` compares task statuses with terminal task statuses.
  - Test: a frontend unit test.
- **As built:** agent reservation also requires the job to be running, as command reservation already did. The job-cap test inserts the job's agent tasks directly, like the existing command cap test. The allocation check is unit-tested, because the builders leave no API path that produces a bad spec. The [review record](review.md) has the details.

### PR 2 — Host-reported releases settle

- Host control applies each host's facts in a transaction scoped to the host's organization, using the mechanism org-scoped maintenance already uses (`scoped_db_for_maintenance_org`). The system role is not widened.
- Test: a host-reported VM exit with a queued preview and a queued notice. Expect the preview cancelled, the intent settled and the notice withdrawn. It fails today: the preview stays queued.
- Also run the existing host-control suites (`api_worker_hosts*`, `host_validator_rls_regression`) to show that every host fact still applies under the scope.
- **As built:** host control runs hello and the releasing VM facts in the same maintenance transaction (`maintenance_org_scope`), on the tenant pool. No migration was needed. Ready stays on the system pool: scoping it would, for the first time, let a Start continue every task a stop held, including a turn the owner stopped from its chat, and the plan did not cover that. It is recorded as an open question, [`g5.lifecycle.start.held-work`](../../ontology/software_architecture/lifecycle.g5.md#decide-whether-start-continues-held-work). Settling under the scope made releases lock tasks and notices, which exposed lock-order cycles with owner operations and settlement; every settling release now locks its workspace rows before its VM rows, the owner order, which also covers lease expiry and forced removal, where the cycles already existed. The [review record](review.md) has the details.

### PR 3 — One reader per shared shape

- **Attempt snapshot:** TaskService keeps the only decoder of `task_attempts.policy_snapshot_ref` and exposes the launch to ChatService. This replaces `chat_service.rs:2652-2687, 3033-3062` and the copies in `task_lifecycle.rs`, `agent_dispatch.rs` and `codex_auth.rs`.
- **Notices:** dispatch reads reserved notice contents through ChatService (`agent_dispatch.rs:2268-2279`).
- **Inbox rows:** TaskService passes the reserved inbox rows to `record_auth_failure_turn`, undoing the read that #301 moved into ChatService (`chat_service.rs:439-444`).
- **Live chat events:** ChatService builds them on the organization-scoped connection, replacing `agent_tasks.rs:589-692`.
  - Test, first: under the production role, `chat.chunk` carries `messageIndex`.
- **Guard:** a source guard keeps `policy_snapshot_ref` in TaskService files and `platform_notice` in `chat_service.rs`.
- **As built:**
  - The decoders of the snapshot's `agent_launch` member live in one TaskService module, `attempt_snapshot.rs`; ChatService no longer reads the snapshot and asks that module for a session's launch, through public launch readers that keep their signatures. Dispatch still reads the snapshot members it writes (grant, permission policy, recovery prompt), inside TaskService.
  - The decoders keep their two strictness levels. Dispatch and device login read only the resolved contract; chat reads the envelope, lifting a flat row.
  - The live-event fix also restores the live task timeline. The coordinator read `task_events` on the system pool too, so `task.timeline` was never published live.
  - Two metadata lines that PR 1 left stale (the job cap, the specification check) are corrected here.
  - The [review record](review.md) has the details.

### PR 4 — Authority checks in their owners

- **Flush proof:** `WorkspaceFlushService::complete` proves the receipt itself. The checks move from `routes/workspace_flush.rs:114-244`, keeping the git commands.
  - Test: a service-level call with a forged ref is refused.
- **Push permission:** `routes/git.rs:1493-1578` calls `PermissionService` instead of re-deriving roles and grants.
  - Tests: one case per grant source, plus a viewer.
- **VM-token admission:** one reader in core, owned by the workspace lifecycle, serves `routes/internal.rs`, both `routes/git.rs` copies, JobService and the flush service. Their route SQL exemptions are dropped.
  - Tests: one stale-epoch test per purpose.
- **Inbound hatches:**
  - `PostgresHostControlState::new` becomes crate-private, with a test-helpers constructor.
  - `stop` without an actor becomes a crate-private system entry.
  - Forced host removal becomes one WorkerHostService operation (custody check, drain, forced loss and deallocation in one transaction), so `record_forced_vm_loss` can be crate-private.
  - `crate_private_fences.rs` pins all three.
- **Guard:** `worker_hosts` joins the writer guard.
- **As built:**
  - The flush service now takes repository storage and the Git binary. `complete` reads the authority, proves the receipt with the route's Git commands, then records it; Git still runs between the two transactions. The test runtime peer creates its flush leaves before completing, as receive-pack does for a guest.
  - The VM-token reader is `workspace_incarnation.rs`, with one entry point for an organization-scoped connection and one for the system pool. The tenant role cannot call `authz.lookup_workspace_runtime_authority`, and the system role cannot read `workspaces`. Leases are compared with the database clock. JobService keeps its single locking statement for the lock order and then asks the reader. Push admission asks again under the VM row's lock.
  - Three route SQL exemptions are dropped, not two: push permission no longer runs SQL in `user_can_write_workspace`.
  - Forced host removal delivers the drain after the one transaction commits, so the host learns of it after termination. Delivery uses the live tunnel, which the route closes afterwards, as before.
  - `worker_hosts` was already in the writer guard (since round 1); the round-2 review was wrong, so nothing changed there.
  - The [review record](review.md) has the details.

### PR 5 — Preview and terminal contracts

- **Profile names:** API and guest share one rule (`validate_preview_profile_name`), and the profile list uses the typed parser.
  - Tests: a guest test for a disallowed name, and an API test for a profile with an unknown key.
- **Preview stream:** `workspace_preview_target` returns the VM id and epoch, and a facade operation opens the stream only on that exact incarnation. It writes the port through a codec in `g5-internal-api::stream_mux` that the guest's `serve_http` also uses.
  - Tests: a refused stale target and a codec round trip.
- **Terminal frames:** shared types in `g5-contracts::terminal`, used by the API and the guest, with one JSON compatibility test.
- **Rollout:** this changes worker-image inputs (`g5-contracts`, `g5-internal-api`, `g5d-dispatch`), so QA waits for a worker bake. The wire format does not change, so guests on the old image keep working. After deploy, check that the worker-image pin did not go backwards.
- **As built:**
  - The shared rule is now kebab-case, the one the guest already applied, so names the API used to accept and the guest refused (`_`, upper case) are refused at save, start and listing, and by `g5d preview-serve` run directly in a VM, which accepted them before.
  - The typed parser stays in TaskService (`task_lifecycle.rs::read_preview_application`); the committed profile list calls it. WorkspaceService's lenient parser is gone, along with `WorkspaceService::list_preview_applications`, which nothing called (the route lists the live checkout).
  - `open_preview_stream` takes the workspace, VM, epoch and port, refuses another incarnation with the readiness error the route already used (`PREVIEW_NOT_READY`), and writes the port header itself. `open_owner_stream` now opens terminal streams only.
  - The guest's terminal test hook rides on the shared `open` message behind a `g5-contracts` feature that only `g5d-dispatch`'s `test-support` enables, so production frames have no such field.
  - The [review record](review.md) has the details.

### PR 6 — Lease checks after lock waits

PR 4's review found that a VM lease checked in a statement that waits for a row lock can end during the wait and still pass: PostgreSQL rechecks a waited-for row only if the row changed, and `now()` is when the transaction began. PR 4 fixed its reader and the owner push path. The same check remains where these wait first:
- the runtime facade's call admission (`workspace_runtime.rs::call`, `FOR SHARE` on the VM row);
- admitting owner work (`workspace_runtime.rs::lock_owner_handle`, `FOR UPDATE OF w, vm`), used by PR creation and merges;
- preview start's authority read (`task_lifecycle.rs::start_workspace_preview`, `FOR UPDATE OF workspace, vm`);
- the device-login status read (`codex_auth.rs::session_codex_auth`), whose transaction can wait for the workspace row before its lease check;
- the runtime attachment (`workspace_runtime.rs::get`), whose authority lookup can wait for the workspace row; terminal streams and owner commands open on it without a runtime call;
- the preview target read (`task_lifecycle.rs::workspace_preview_target`), which mints preview tickets and whose transaction can wait in its authorization;
- the dispatch pass's agent reservation (`agent_dispatch.rs`), which runs in a transaction that may already have waited while settling an unrunnable command's dependents.

Each checks the lease once it holds the rows, or once its transaction has waited, against the database clock (`clock_timestamp()`). The command reservation uses the clock too, though nothing waits before it today. The Changes view's live check can wait in its own authority lookup; it uses the clock and only decides whether to ask the attachment, which checks again.
- Tests: one per path that can wait. A transaction holds a row without changing it while the lease ends, then releases it; the request must be refused.
- Not changed: the host's lease renewal. The host asked before its lease ended; refusing it because the API held the row would turn API lock contention into a lost VM. Lease expiry and renewal still serialize on the VM row. The expiry sweep's `<= now()` can only find a lease ended later, never sooner, so it stays.

### PR 7 — Chat Stop ends the turn

Chat Stop pauses the session's newest active task with an `owner_stop` hold (`task_lifecycle.rs::request_session_agent_task_pause`). The chat offers no Continue, and a typed chat's tasks have no job for the drawer to show, so every message after a Stop waits forever. A turn held for any other reason is stranded the same way: a workspace stop, idle stop, VM loss, transfer, a restart the host needed, or a question that went unanswered (its request is invalidated and the turn waits for Continue). Start releases none of them (PR 2's open question).

```text
Chat Stop names a turn (its reply's id, or the input id the send returned)
  message still waiting behind another turn → withdraw just that message (today's Withdraw)
  turn running        → cancel its attempt; the reply settles stopped with its partial text
  turn held or queued → end it now; its message is withdrawn if the agent never received it
  turn already ended  → no change
  a job's or platform turn → today's Stop and hold, for Continue
New message while the owner's chat turn is held (any hold but unknown delivery; not a job's or platform turn)
                      → end the held turn, keeping a message it never delivered, then queue
A message sent while the turn was still stopping → ends it once settlement holds it
Whenever a chat turn ends → the queued messages start the next turn, in order
```

Ending a turn reuses cancellation, which already keeps the queued messages when it cancels a running turn but withdraws every message bound to a held or queued one; ending keeps them in both cases. Stop already carries the id of the turn it means, but today it pauses the session's newest task whatever it names; resolving the id to its own turn means a late Stop cannot end a later turn. The next turn's prompt is the frozen chat history, which already includes stopped replies: every reservation freezes it, and providers start fresh on every turn because native resume is off. Each ending is one transaction, and a new message that ends a held turn is queued in the same transaction. For a running turn the stop request is committed first and the maintenance pass delivers it again if the runtime call fails, as for Cancel today. The task's Stop and hold, Continue and Cancel are unchanged, and a job's chat keeps them.

- Tests (API, with the test runtime peer):
  - Stop of a running turn keeps the partial reply as stopped, ends the task, and a message queued behind it runs next without Continue.
  - The next message after Stop starts a new turn whose prompt carries the stopped reply.
  - Stop of a queued turn withdraws only that message.
  - A late Stop naming an ended turn changes nothing and does not touch the next turn.
  - A workspace stop holds a turn; after Start the next message ends it, and a message the turn never delivered runs first, then the new one.
  - A question that went unanswered holds a turn; the next message ends it and runs.
  - A turn whose input's delivery is unknown still waits for the Redeliver choice.
  - A job's turn keeps the hold.
  - Claude's End that has to stop Claude ends the turn.
- Frontend: the Stop call and the queued-message tray stay; their tests follow the new statuses, the Stop and End comments in `useChat.ts` describe the new outcome, and the e2e helper that stops and then cancels a turn needs only Stop.

## Left out on purpose

- **Git:**
  - a guard against new raw `git2` and git subprocesses;
  - shared token-injection and GitHub header helpers;
  - `list_github_orgs` ignoring the configured URL, a one-line fix worth doing first in any later PR.
- **Chat:**
  - one module for a chat's saved settings;
  - follow-up turn rules owned outside the route (needs a decision: no block states the per-chat rule).
- **Tasks:**
  - one result-application path and status publisher;
  - adopting the status enums;
  - the stale statuses in `intent_log_service.rs`;
  - sealing incarnation claims (cross-area);
  - SpendService's counter rules, which also explain the session cost lag (needs a decision on session cost).
- **VM:** a VM status type, the copied guest protocol method names, building `WorkspaceVmService` from public config, and splitting `workspace_service.rs`.
- **Preview:** URL builders and prefix parsing, the three scope statements, and the terminal idle-activity rule.
- **Ownership questions:** `job_merge_runs` has no owner in the ontology, and the frontend's 22 direct API calls remain.

## Acceptance and risks

- Each PR starts with a test that fails on `main`. If a bug's test does not fail, that fix is dropped and the review is corrected.
- The writer guard and the fences only get stricter.
- No PR changes the HTTP API. PR 2 may need a forward migration if host control cannot open an organization scope with today's functions.
- **Risks:**
  - PR 1's job cap lowers concurrency for large bootstrap graphs, as `g5.agents.admission` intends.
  - PR 2 could hide a row host control needs; the host-control suites cover each fact kind.
  - PR 4 touches push and token security.
  - PR 5 depends on the worker-image pin, which went backwards on QA on 2026-09-30.
  - PR 7 changes what Stop does in every chat that is not a job's; the e2e cleanup helper relies on Stop.

## Review summary

- **Budget:** round 1's (five PRs, under 5,000 changed lines each), confirmed by the user; the user added PRs 6 and 7 on 2026-10-04.
- **Nothing was executed.** Bugs 1, 2, 5 and 6 are inferred from code and row policies; bugs 3, 4, 7 and 8 are read from code. Each PR proves its bug with a failing test first.
- **PR 2's mechanism:** it assumes the maintenance organization scope can be opened for host control, and that every statement host control runs is allowed under it. Verify this before coding.
- **Owner choices in PR 4, within existing ontology:**
  - the VM-token admission reader belongs to the workspace lifecycle, which owns custody;
  - forced host removal belongs to WorkerHostService, which owns drain and release, composing the lifecycle's release module.
- **PR 7 choices, approved by the owner with the design ("implement PR 7"):**
  - A job's chat keeps today's hold, because cancelling its turn would fail the job's dependent work.
  - The next message ends a chat turn held for Continue for any reason but an input of unknown delivery (`g5.agents.lifecycle.chat-held-turn`). The alternative is to let Start continue held chat turns, which covers only stop-related holds; the open question `g5.lifecycle.start.held-work` still covers jobs.
  - Claude's End that has to stop Claude ends the turn, as Stop now does; today it holds the task for Continue.
  - "The agent keeps its context" holds through the frozen chat history, not provider-session reuse: native resume is off.
  - Stopped turns still record no usage (a known gap of `g5.agents.lifecycle.owner-stop`).
- **Profile names in PR 5:** kebab case, the guest's rule (approved), so no profile that launches today becomes invalid. A saved profile with `_` or upper case would then need renaming; it cannot launch today anyway.
