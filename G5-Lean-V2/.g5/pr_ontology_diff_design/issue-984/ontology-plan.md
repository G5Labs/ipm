# Automatic continuation of chat turns interrupted by host operations (#984)

Request: [G5-Lean#984](https://github.com/G5Labs/G5-Lean/issues/984) asks whether to build
chat auto resume when the host a workspace runs on is rotated, and to design it if yes.
Decision status: the recommendation below is **yes, narrowly**. The owner first asked
in chat (2026-10-30) for the resume work only, then decided the Project Doc question set
`chat-auto-resume` (docs repository, page `chat-auto-resume-decision.md`) on 2026-10-31:
the scope, the no-replay continuation with the restore sentence, the immediate start with
an hour's capacity wait, the chat notice with a live notification (Q5), the loss cap of
two, the ordinary idle timeout after continued work (Q7, so no shorter bound) and an
organization setting on by default (Q8). All of it is built on this branch.

## Governing blocks (in this diff)

- New [`g5.lifecycle.start.automatic-continuation`](../../ontology/software_architecture/workspace_vm.g5.md#g5.lifecycle.start.automatic-continuation):
  the scoped, system-recorded consent that restarts a workspace whose compute the
  platform ended while a chat turn was active, with its trigger, conditions, cap,
  capacity bound, notices, post-work idle bound and organization setting.
- Amended [`g5.lifecycle.start`](../../ontology/software_architecture/workspace_vm.g5.md#g5.lifecycle.start)
  (the exception is now named) and
  [`g5.lifecycle.start.held-work`](../../ontology/software_architecture/workspace_vm.g5.md#g5.lifecycle.start.held-work)
  (an automatic start releases the same held work).
- Amended [`g5.chat.activity.turns`](../../ontology/features/chat.g5.md#g5.chat.activity.turns)
  (a pending automatic start keeps the queued turn active) and
  [`g5.runtime.host.rotation`](../../ontology/software_architecture/host_control_plane.g5.md#g5.runtime.host.rotation)
  (cross-link; host mechanics unchanged).

## What happens today (verified in code)

Rotation of an idle host never interrupts a chat: the old host is retired only after a
fresh idle-custody check finds it empty (`g5.runtime.host.rotation`). A busy host is
force-drained only at its credential cliff (host credential lifetime minus two hours, once
the replacement is ready, or at lifetime minus fifteen minutes regardless), so a
workspace that stays running long enough does get interrupted. Interruptions come from
the forced lanes:

| Cause | Owner-side effect on a running chat turn | Comes back when |
| --- | --- | --- |
| Credential-cliff forced drain, operator drain, unreachable-host drain | Owner is warned ("Workspace shutdown scheduled", default 30 min), workspace is saved and stopped gracefully. `apply_interruption_policy` re-queues the turn (`queued`, `continuation_reason=interrupted`, no hold). | Owner presses Start (dispatch needs `vm.status='running'`). |
| Host failure, lease expiry, missing inventory, forced host removal | VM released as `failed` with the cause on `workspace_vms.last_error`; the 2 s reconciler settles the attempt `vm_lost`; agent task becomes `waiting`, `hold_reason=vm_lost`. Unsaved guest changes may be lost (`unflushed_loss_at`). | Owner presses Start: `release_tasks_after_resume` re-queues `vm_lost` holds (`g5.lifecycle.start.held-work`). |

The continuation attempt already exists and is the path an owner Start takes: it carries
`continuation_reason`, injects "Continue the same task after {reason}. Resume from durable
conversation history and do not replay previously delivered input…", resumes the native
session when a harvested bundle verifies (`claude --resume`, Codex `thread/resume`) and
otherwise reconstructs from the transcript with a `conversation_reconstructed` notice
(`g5.chat.recovery`). The original input is never re-sent
(`deadline_and_vm_loss_cannot_resume_the_original_prompt`). Delivery-unknown inputs and
pending clarifications stay held for the owner. `StartKind::RolloutContinuation` is
parsed but never constructed, and `g5.lifecycle.start` already carved out "an
already-authorized rollout continuation" as the one consent exception. The chat UI has
no Resume or Continue button; the turn shows "Stopped" and Activity says "Stopped while
its host was drained" or "Ended because the workspace VM was lost".

So the missing piece is not resumption, it is the **start**: nobody authorizes compute
after a platform-caused stop, so a turn the owner left running stalls until they notice.

## Recommendation and reasons

**Yes, build it, scoped to platform-caused interruptions of active turns.**

- Owners leave agents working unattended for hours; a stall caused by host maintenance
  is invisible until they return and is a pure loss of the time they bought.
- Nearly all of the mechanism exists (interruption policy, `vm_lost` hold and release,
  continuation attempts, native resume, the unused start kind). The added surface is one
  consent record and its guardrails.
- The consent boundary of `g5.lifecycle.start` stays intact: the automatic start is a
  named, scoped exception that only ever re-attaches compute the owner already had
  attached and that the platform, not the owner, took away.

Not recommended: restarting every workspace the host operation stopped (compute spent
with no work to do, and an eight-hour default idle timeout to burn through), and
replaying the interrupted input (forbidden today for good reason: tool calls before the
interruption already took effect).

## Owners and decisions

- **Workspace VM lifecycle (`workspace_vm_service.rs`) owns the consent.** The
  automatic start is a start intent of kind `automatic_continuation` (the never
  constructed `rollout_continuation` kind, renamed) recorded on `workspace_vms.start_intent`
  with the current owner, lifecycle generation and a one-hour expiry instead of the
  owner-start fifteen minutes. The pending-intent reconciler already admits intents only
  for the current owner and generation, with `authz.workspace_execution_allowed`, no
  pending transfer and the workspace undeleted, and already expires them; no new
  admission rule is needed. Placement excludes draining hosts already.
- **Who records it, and when (as built).** A pass of the workspace lifecycle
  reconciler, `reconcile_automatic_continuations`, runs every two seconds with the other
  VM intents. It reads stopped or failed VM rows with agent work, no live start intent
  and an undecided last incarnation (`epoch`), and decides each once. The decision is one
  JSON record on `workspace_vms.automatic_continuation` (migration 140): the decided
  `epoch`, an `outcome`, a `cause`, the loss `count` since the owner's last Start and,
  for a start, the `intent_id`. Agents gives the verdict on what the interruption left
  (`WorkspaceAgentWork::interrupted_work`), because every release clears `stop_reason`
  with the rest of custody and only the task rows carry the cause.
- **The verdict.** Unsettled while an attempt of the ended incarnation still runs.
  Otherwise, in order: turns held `vm_lost` whose settled attempt belongs to this
  incarnation and whose lifecycle marker is not an owner-decided stop; turns still queued
  that the loss release stamped `vm_lost` for this incarnation, or whose latest attempt
  settled `vm_lost` under a drain policy; turns a drain or rollout stop left queued with
  their interrupt reason; else none. A turn with an unanswered clarification or in a
  session with an input of unknown delivery never qualifies. The loss stamp
  (`mark_queued_agent_turns_lost`, in the release transaction) applies only when the
  released VM had reached Ready and ended `failed` or `lease_expired_during_stop`, so a
  clean stop's exit and a failed initial boot stamp nothing.
- **Decisions other than a start** (`none`, `cap_reached`, `ended`, `refused:<reason>`)
  are written by the pass with a fenced update: the row must still be that incarnation,
  stopped or failed, undecided and without live consent. **A start** writes its
  `recorded` decision inside `record_start_intent`, under the same row lock and in the
  same transaction as the consent, with the same fence; a replay of the consent by the
  start queue must own the recorded intent. A definite refusal of the start (owner
  authority, workspace state, limits, configuration) is recorded, over the consent's own
  record when placement ended that consent in place (the owner VM limit fails it);
  any other failure is retried on a later pass. The owner reads refusals as phrases.
- **Consent ends with the turns it was recorded for.** Every path that ends a queued or
  held agent turn (owner Cancel, chat Stop, a new message taking over a held turn, a
  withdrawal that leaves a turn without input) calls
  `release_automatic_consent_without_work`, which locks the VM row while an automatic
  consent waits and cancels it when no qualifying turn remains
  (`continuable_work_remains`: the verdict's turns without the incarnation binding, since
  a refused placement advances the epoch while the consent waits). `record_start_intent`
  makes the same check before recording or replaying automatic consent.
- **Stops decide.** Every stop admission other than a drain or rollout writes a
  `stopped` decision for the current incarnation: when the stop is admitted on a running
  VM, when a deadline stop is announced, and when Stop is pressed on a workspace already
  stopped. A drain or rollout re-admitted after the VM ended neither decides nor cancels
  the automatic consent it led to. A lapsed consent, the owner's or the platform's, that
  never ran its VM (`expired`, `cancelled` or `failed` without `consumed_vm_id`) ends the
  continuation (`ended`), so refused placements never renew the hour and a lapsed owner
  Start after a loss is not taken over.
- **Owner Start resets the lineage.** Recording new owner consent clears the record and
  with it the loss count; an exact replay or a Resume that only attaches to running
  compute records nothing and keeps any decision made since.
- **Cap and rollout.** Lost VMs increment the count and are refused at two; drain and
  rollout continuations keep it. Migration 140 also backfills a `none` decision for
  incarnations already ended, and for running VMs already under a non-drain stop, so no
  historical interruption is started again after deployment.
- **Dispatch (`agent_dispatch.rs::continuation_instruction`) owns the continuation
  text.** The instruction already names the cause ("Continue the same task after
  vm_lost"); after a lost VM it adds "The workspace was restored from its last save;
  changes made after it may be lost, so re-check them before relying on them." Drains
  flush before stopping, so their continuation keeps the existing text.
- **Notices (Q5, built).** After each decision the owner should hear about, the pass
  appends one platform notice to every chat with interrupted work
  (`notify_working_agents`, so the agent reads it on its next turn too) and stores a
  `workspace_lifecycle` notification for the owner, pushed live to their user channel and
  their connections on the workspace channel through the WebSocket hub. The texts:
  "Interrupted: the workspace's host was drained / the workspace VM was lost. G5 started
  the workspace again and the interrupted chat continues.", or "... G5 did not start the
  workspace again: <why>. Press Start to continue." for the cap, a refusal, an hour
  without capacity or a failed boot. Decisions the owner made themselves (their Stop, a
  lapsed owner Start) get no notice. The Activity row keeps its interruption reason. The
  first design's notice wording follows; the mechanism above replaces it.
  (`g5.chat.error-notice` presentation, not an error): "Interrupted: the workspace's host
  was drained / the workspace VM was lost. G5 restarted the workspace and continued this
  turn." or, when no continuation follows, the reason ("automatic continuation is off for
  this organization", "no capacity within an hour", "this workspace lost its VM twice
  since your last Start") with "Press Start to continue". The Activity Current row keeps
  its existing reasons (`host_drain`, `vm_lost`); the queued turn spins while the
  automatic intent is pending, exactly as for an owner's pending start. No email.
- **Notification channels today (verified 2026-10-30).** The platform has a persistent
  `notifications` table, a per-workspace list route, a bell and panel in the workspace
  top bar, and a live `notification` WebSocket event that `NotificationService::
  create_notification` publishes after commit. Only two lifecycle events use the table,
  the deadline-stop nudge ("Workspace shutdown scheduled") and ownership transfer, and
  both insert with raw SQL, so no live event is published: the bell catches up only when
  the workspace page reloads. The nudge also appends a chat notice to every chat with an
  active turn (`append_lifecycle_notice`), which is the path this design reuses. VM loss
  produces no notification and no chat notice, only the "Stopped" message status and the
  Activity reason; a Start, manual or otherwise, notifies nobody; the stop toast is
  client-side and shows only while the page is open; the bell is per workspace, so an
  owner away from that page sees nothing until they return. Implementation therefore
  routes the interruption/continuation notification through `NotificationService` so it
  is pushed live, and the owner's answer to Q5 decides whether an owner who is away is
  also emailed when no continuation is possible.
- **Post-work idle (Q7, decided against).** The owner chose the normal idle timeout; nothing
  was built. The first design's proposal follows for the record. The automatically started incarnation carries its start kind;
  the idle admission uses a fifteen-minute inactivity bound for it until the owner's
  first recorded G5 UI activity on that incarnation, after which the normal idle timeout
  applies. This reuses `g5.lifecycle.idle` admission (running bounded work still prevents
  the stop) and adds one condition, not a new lifecycle.
- **Organization setting (Q8, built).** `automatic_continuation_enabled` (boolean, absent
  = true) is the eleventh registered org key: migration 141 extends the config validator,
  `org_admin_settings.rs` reads and writes it, the Admin page shows it as "Continue
  interrupted chats automatically", and the reconciler's scan reads it per workspace
  organization; while it is off nothing is decided, so turning it on continues turns
  interrupted meanwhile. The first design named it `auto_continue_interrupted_turns`
  under the existing typed org settings owner and the Admin page
  (`g5.data.configuration.org-admin-settings`).

## Interaction sketch

1. Forced drain at 14:00: owner warned; workspace saved and stopped at 14:30; the
   running turn re-queued `interrupted` with the drain marker. On its next pass the
   lifecycle reconciler finds the stopped row undecided, asks Agents, gets `host_stop`,
   and records the automatic start intent (expires 15:30) with the decision.
2. The pending-intent reconciler places the workspace on a ready host; Ready releases
   held work (`release_tasks_after_resume`) and the queued turn dispatches a continuation
   attempt that resumes the native session.
3. The turn finishes at 14:50; the owner has not touched the workspace. At 15:05 idle
   admission stops it (fifteen-minute bound for an automatically started incarnation).
4. Variant, host failure: lease expiry at 14:00 releases the VM as failed with
   `unflushed_loss_at`; the reconciler settles the attempt `vm_lost` and, cap permitting,
   records the intent and increments the counter; the continuation text carries the
   restore warning. A second loss records a second continuation; a third holds the turn
   with the cap notice until the owner's Start, which resets the counter.
5. Variant, no capacity: the intent waits as `queued_for_capacity`; at 15:00 it expires,
   the notice is updated to "no capacity within an hour; press Start to continue", and
   the turn stays queued behind stopped compute (not spinning, per
   `g5.chat.activity.turns`).

## Preserved guarantees and risks

- Consent and authority: the automatic intent is admitted by the same predicates as an
  owner intent and belongs to the current owner and generation; a transfer, deactivation,
  read-only formal review workspace or Delete invalidates it like any pending intent.
- Atomicity: the start consent and the decision are one transaction under the VM row
  lock, fenced on the incarnation being undecided and without live consent, so neither a
  crash nor a second replica can start a workspace twice or leave a start undecided.
- Side effects of an interrupted tool call: unchanged from today's owner-Start
  continuation; the added restore sentence tells the agent what is uncertain. Drains are
  lossless (save precedes stop).
- Terminals, previews and `g5d` sessions of the old incarnation do not come back; this
  design is about chat turns only, as the issue asks.
- Cost: bounded by the fifteen-minute post-work idle bound, the loss cap and the
  organization setting.
- Open assumption: the one-hour capacity wait and the cap of two are agent choices put
  to the owner (Q4, Q6).

## If the owner decides otherwise

- **No**: revert the four ontology amendments; optionally record in
  `g5.lifecycle.start.held-work` that platform-interrupted turns wait for the owner's
  Start by design, and add a chat notice naming that.
- **Drains only**: drop the loss cap and the restore sentence; the release path records
  nothing.
- **Hold for the owner's Continue**: no automatic start; add a Continue control to the
  chat for a held platform-interrupted turn (the route exists) and a notice. This is the
  "ask the user" safety option.
- **Every stopped workspace restarts**: the recording predicate drops the active-turn
  condition; the post-work idle bound then matters more. Not recommended.
- **Normal idle timeout after continuation**: remove the fifteen-minute bound.
- **No organization setting**: drop the setting; keep the cap.

## Implementation and testing

Built (2026-10-30): migration `140_workspace_vm_automatic_continuation.sql` (the record
column; `g5_start_intent_is_valid` accepts the `automatic_continuation` kind),
`StartKind::AutomaticContinuation` with its hour-long consent and the owner-start reset,
`WorkspaceVmService::reconcile_automatic_continuations` and its record helpers,
`WorkspaceAgentWork::interrupted_work` implemented by `task_lifecycle.rs::interrupted_work`,
the lifecycle pass in `workspace_lifecycle.rs`, and `continuation_instruction`.
Tests: `backend/g5-api/tests/api_automatic_continuation.rs` (twenty-seven real-Postgres
cases over the rules above: lost VM, drain, cap, lapsed consent, failed initial boot,
successive drains, owner Stop in its three forms, replays, every turn-ending path,
clarification and unknown-delivery exclusions, incarnation binding) and two unit tests
(the continuation instruction, the refusal classification). Eleven Codex review rounds
(gpt-6-astra, high effort) shaped the fences above; the re-audit then folded the decision
write into the consent transaction. Also tested: the notice and notification after a start and after the cap, and the
organization setting turning the pass off and on; `api_org_admin_settings.rs` and the
Admin card test carry the new key. Not built: an e2e run on a real host; the existing
`host_loss_recovery.rs` scenario still covers restore and preview fencing only.

### Original plan

- Backend: intent recording in `workspace_vm_service.rs` (stop release) and
  `task_lifecycle.rs::apply_vm_loss`; counter column and setting key migration; longer
  expiry for the automatic kind; idle admission condition; continuation text; chat
  notice journal event; Activity/notice wording in `frontend`.
- Tests (real Postgres, `api_task_lifecycle.rs` / `api_worker_hosts.rs` style): drain
  stop records the intent and the Ready releases and continues the turn without an owner
  Start; lease expiry does the same with the restore sentence; cap refuses the third loss
  and the owner Start resets it; delivery-unknown-only and clarification-held turns do
  not restart; setting off records the notice only; intent expiry on capacity leaves the
  turn queued and not spinning; idle bound stops the unused incarnation after its work
  settles and not while a turn runs. Frontend: notice rendering and Activity wording.
  E2E: extend `host_loss_recovery.rs` with a running chat turn and assert the
  continuation on the new host.
