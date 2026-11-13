# Start the workspace from the Preview dialog

Owner request (workspace chat, 2026-11-05): when no VM is attached, clicking Preview and
picking a profile should attach the VM and then start the preview, reusing what the chat
buttons already do ([intent](../../chat_intent/preview-wovm/request.md)). The owner
decided in chat the same day: the pending launch is a durable server-side record bound
to the Start consent, "like chat Start and send"; the picker lists the configuration
committed on the branch; a launch may wait under a Start already under way; the action
is called **Start workspace and preview** (question set `preview-without-vm` in the
Project Doc `preview-without-vm.md`; the agent's proposed answers mirror the chat).

## Governing commitments

- [g5.lifecycle.start](../../ontology/software_architecture/workspace_vm.g5.md#g5.lifecycle.start):
  compute attaches only on explicit Start consent; clearly labelled compound actions may
  carry it; queued work never authorizes waking.
- [g5.chat.start-and-send](../../ontology/features/chat.g5.md#g5.chat.start-and-send):
  the consent-bound queued input and its settlement. Today it lists preview among the
  controls that stay disabled; this plan amends that sentence.
- [g5.lifecycle.start.held-work](../../ontology/software_architecture/workspace_vm.g5.md#g5.lifecycle.start.held-work):
  work the owner queued runs once their Start brings the VM back.
- [g5.vm.preview-exclusive](../../ontology/software_architecture/workspace_vm.g5.md#g5.vm.preview-exclusive)
  and [g5.runtime.preview.admission](../../ontology/software_architecture/workspace_vm.g5.md#g5.runtime.preview.admission):
  one preview at a time, recorded under the workspace authority lock; replay must match.
- [g5.lifecycle.idle](../../ontology/software_architecture/workspace_vm.g5.md#g5.lifecycle.idle):
  a requested preview that has not reported ready prevents idle Stop.
- [g5.ui.unavailable-actions](../../ontology/features/ui.g5.md#g5.ui.unavailable-actions),
  [g5.ui.capacity-wait-notice](../../ontology/features/ui.g5.md#g5.ui.capacity-wait-notice),
  [g5.preview.dialog-controls](../../ontology/features/ui.g5.md#g5.preview.dialog-controls).

## What exists, and what of it transfers

Chat's Start and send (`routes/start_and_send.rs`) is one owner request: `begin` records
Start consent through ordinary Resume admission (`StartKind::CompoundStart`); the chat
request queues its input and `bind` ties the input to the consent under a share lock on
the VM row; the `workspace_vms` trigger from migration 110 settles bound inputs in the
same transition that ends the consent (consumed releases, cancelled/expired/failed/
superseded withdraws); `abandon` cancels a waiting consent if the rest of the request
fails. The dispatcher never picks a bound input.

A preview is already a queued task: `start_workspace_preview` inserts a `tasks` row with
`status = 'queued'` and records the preview intent on the VM row, and the shared command
dispatcher (`reserve_queued_command_dispatch`) admits queued preview tasks only while the
VM is `running` with a valid lease. So a preview task recorded before the VM exists waits
on its own, and dispatch after Start is the existing held-work path. What is missing is
exactly what chat added for inputs:

1. a way to record the task without a running VM (today the profile is resolved through
   the live file surface and the service refuses unless the VM runs), and
2. a binding to the consent, so a Start that ends without a VM cancels the task instead
   of leaving it queued for an unrelated later Start, and so a bound task is never
   dispatched by a VM its consent did not start.

Transfers as-is: `start_and_send::begin`/`abandon` and the route sequencing pattern
(`begin`, request transaction, `bind`, commit, `abandon` on failure); the trigger pattern
of migration 110; `StartComputeDTO` in the response; the frontend `StartAndSendNotice`
wording, the live gate, `GatedControl` for the non-startable kinds, and the dialog's
launch mutation, error display and exclusivity notice. `bind_input_to_start_consent` is
specific to `session_inputs`; the task binding is its sibling, not a reuse.

## Design

**Owners.** The VM service owns the Start consent and its transitions (unchanged). The
task service owns the preview task and its intent, and now also the task's binding to a
consent. The settlement trigger is the one place that coordinates the two, as it already
is for inputs: it runs in the VM row's transition, so no caller sequences the two owners.

**One owner request.** `POST /workspaces/:id/preview` accepts `startCompute: true`.
The route runs `begin` (consent recorded through Resume), opens the request transaction,
calls `start_workspace_preview` with the consent, binds the new task to it under the
same share lock on the VM row, commits, and returns the queued preview view with the
`startCompute` outcome. A failed request after `begin` runs `abandon`. If Resume reports
the VM already attached, the ordinary path runs with no binding.

**Recording without a VM.** When the consent is still waiting or starting,
`start_workspace_preview` resolves the configuration from the branch's published
snapshot (`preview_repo_read_view`, `.g5/workspace.yaml` or the named profile file) instead
of the live file surface; every other admission check stays (owner, not Base, not deleted,
no pending transfer, repository membership, exclusivity against queued/running/waiting
preview tasks, idempotency replay against the intent). Instead of requiring `running`
with a matching handle, it requires that the VM row carries this request's consent in
`pending`, `queued_for_capacity` or `starting`. The task row gets `start_consent_id`;
the intent status is `queued` as today.

**Port.** The spec's `port` comes from the committed file; the guest reads its own file at
launch and its `preview.ready` event already carries the port it listens on. The relay
target and the view take the ready event's port when present and the spec's otherwise,
so an uncommitted port change in the saved working tree does not strand the relay.

**Settlement.** Migration 152 adds `tasks.start_consent_id` with a partial index and
extends `g5_settle_start_bound_inputs` (one function, both tables): consent consumed
clears the binding, after which the dispatcher admits the task on the running VM; consent
cancelled, expired, failed, superseded or removed cancels a still-queued bound task with
outcome `cancelled` and reason `start_ended`, and settles its preview intent, so the dialog
and Activity show from the task outcome why it did not start. The dispatcher's
reservation adds `task.start_consent_id IS NULL`, so a bound task cannot be admitted by
any VM before its consent is settled. The owner's Close on a pending preview is the
existing queued-task cancel; it does not cancel the Start, as withdrawing a Start and send
message does not.

**Joining a Start under way (review finding, 2026-11-05).** Resume supersedes a pending
intent of the same owner, and the consent trigger withdraws the work bound to the
superseded one, so a second compound action during a pending Start (Start and send
after Start workspace and preview, or the reverse) would have cancelled the first one's
work. `start_and_send::begin` now joins the owner's Start under way instead of recording
another, for every compound action; `abandon` leaves a consent that other work has joined.

The join rule lives in Resume admission (`record_start_intent`), so the header's Start,
`g5d workspace start` and every compound action behave alike; `begin` learns the Start it joined
from admission. One predicate, `live_owner_start_intent` in the VM service, decides
which consent work may bind to or join: the owner's own Resume or compound consent,
`pending`, `queued_for_capacity` or `starting`, and within its bounded lifetime (the row
validator `g5_validate_runtime_intent_binding` refuses to carry an expired active intent,
so an expired one is dead whatever its status). `cancel_waiting_start` takes the VM row lock before checking for
bound work, so a binding committed under that lock is seen.

Codex review (round 1, 2026-11-06): `begin` reports the consent the VM row carries after
admission, since admission may have joined a Start another request recorded between the
read and the lock; replays accept the digest shape recorded before the port left it, so
previews recorded before deployment still replay.

Codex rounds 2 and 3: the preflight reads the workspace with an optional VM row, so a
never-started workspace's owner is admitted; the idempotency key is checked before any
Start; a request that recorded a Start and finds the VM running must find that consent
consumed before recording an unbound preview; a joined consent is marked `joined` on the
VM row (by Resume admission and by `begin`), and `cancel_waiting_start` refuses a joined
consent, so a Start and answer that re-queues its held turn without a bound column is
protected too.

Codex round 4: a request that recorded a Start binds only to that Start (the owner's
later Start joins rather than replaces it, so a different live consent means the recorded
one was cancelled); the join mark is a conditional update on the consent's liveness, so a
cancellation that landed since the read leaves nothing joined and a fresh Start is
recorded.

Codex round 6: admission returns the consent it selected under its lock
(`resume_for_binding`), so `begin` never reads the id from the VM row afterwards; an
owner's compound Start on compute already starting or running under their consent attaches
instead of conflicting.

Codex round 7: a recorded consent is checked by admission even when Resume reported the
VM attached (the row must still carry it, neither ended nor replaced); attaching to compute
that is starting marks the consent joined as well.

Codex round 9: the liveness predicate applies the bounded lifetime to every active
status, matching the row validator, and the join mark is written only to a live consent.

Review round 10 (fresh-context Claude reviewer; the Codex login did not survive the VM
loss): a compound Start refuses, with the reason, to attach to a boot under a Start the
owner cannot bind to (the platform's continuation, or their own past its lifetime) instead
of binding to a consent admission then reports as ended; the route reads the committed
configuration whenever the VM is not running, so admission's reasons reach the dialog;
waiting for capacity is read from the consent, not the VM status; a Start under way is one
on compute that is not running.

Review round 11 (fresh-context Claude reviewer): `begin` no longer joins through a shortcut
before Resume; admission's join branch already marks the consent joined and retries
placement for a consent still waiting, which the shortcut skipped. A missing committed
configuration while compute starts under any Start (the owner's or not) says the preview can
be launched once the workspace runs. The dialog's fallback gate takes the real ownership
flag.

Review round 12 (fresh-context Claude reviewer): a committed configuration the launch would
refuse is reported as that refusal rather than as missing; the placement activity tag names
the consent admission selected.

Review round 13 (fresh-context Claude reviewer): the statement no longer claims the owner's
own Resume may supersede a recorded Start (it joins since round 4; a recorded Start no longer
live is refused); a refused committed configuration is reported as a conflict naming the file
and the parser's message, which the dialog shows verbatim.

Review round 14 (fresh-context Claude reviewer, no actionable defect): the statement owns the
refusal while compute starts under a Start that is not joinable; the host boot command key
names the selected consent like the activity tag. Review converged: rounds 11 to 14 found no
blocking defect.

CodeRabbit (2026-11-06): the statement is split into a parent commitment and three children
(joining, committed configuration, Close), each with its own metadata and evidence links.

CodeRabbit (2026-11-06, second pass): the parent statement distinguishes refusals decided
before any Start (which record none) from a request refused after recording a Start, whose
consent is cancelled unless another action joined it; the race it named (two requests pass
preflight, the second joins and queues, the first is refused for the active preview) keeps
the consent for the accepted preview, as `abandon` does.

Known gaps: a preview cancelled by the consent trigger records no task event (Activity
refreshes on the VM transition, the dialog on its pending poll); the trigger's task
updates rely on consent-ending writers running on org-scoped sessions, which every current
writer does; a VM `starting` under the platform's automatic continuation is not joinable,
so Preview once running is refused with a reason until it runs.

**Already starting.** While the VM is `starting` or `queued` under an existing consent,
the dialog sends the same request without `startCompute`; the service binds the task to
the current consent so it settles the same way. Nothing is woken that was not already
consented. The agent route (`g5d workspace preview-start`) keeps its running-VM
requirement in this change; the service now supports the compound path, so adding the
flag there later is small.

**Frontend.** The Preview trigger is enabled while the owner's VM is stopped, failed,
starting or queued. The dialog lists the committed configuration while the VM is not
running, labelled as committed. The launch button reads **Start workspace and preview**
(or **Preview once running** during a Start) and sends the request; the response is the
queued preview, and the dialog shows "Starting the workspace… the preview launches once
it is running" with the capacity-wait reason when queued, then the ordinary
starting/serving states. Reload, another tab and Activity all read the same queued task
with a `pendingStart` flag on the view. The preview query polls while a pending preview
exists, not only while the VM runs. A cancelled Start shows the task's `start_ended`
ending with Start workspace and preview offered again.

### Why this is sufficient

The added state is one column and one trigger branch, with the same semantics as the
chat binding; the launch itself is the existing dispatcher path, so there is no
platform-run launch step outside the task lifecycle and no new failure reporting beyond
the task outcome the dialog already renders. The guarantees the owner asked for hold:
the launch survives the browser, exactly one preview is recorded, a Start that ends
without a VM leaves nothing queued, and a bound task never runs on a VM its consent did
not start. The browser-held alternative was rejected by the owner.

### Assumptions

- Merged previews (`mergeResult`) keep today's running-VM gating.
- The committed listing can differ from the VM's restored working tree. A profile edited
  but not committed launches in its saved form; one that exists only uncommitted is not
  listed until the VM runs; one deleted uncommitted fails at launch with the existing
  error, shown in the dialog and Activity.
- A pending preview counts as a requested preview for idle Stop, as the existing clause
  already covers queued preview tasks.

## Ontology amendment (applied with the implementation)

- `g5.chat.start-and-send`: remove "preview" from the controls that stay disabled and
  point to the new statement.
- New `g5.preview.start-and-preview` in workspace_vm.g5.md beside
  `g5.vm.preview-exclusive`: while the owner's VM is stopped, failed or starting, Start
  workspace and preview is one owner request that records Start consent and the preview
  task bound to it; the task is dispatched once that Start brings the VM to ready and is
  cancelled, with its reason shown, if the consent ends without one; the configuration
  offered and recorded comes from the branch while the VM is not running, and the relay
  follows the port the guest reports ready; Close cancels the preview, not the Start.
  Provenance: owner decision in chat 2026-11-05 and this plan.
- `g5.preview.dialog-controls`: one sentence for the pending-start state.

## Implementation outline

Backend (as implemented): migration 152; `tasks.start_consent_id IS NULL` in the
dispatcher reservation; `start_workspace_preview` takes the committed application and the
recorded intent id and binds the task itself under the VM row lock it already holds (no
separate bind step); `WorkspaceService::read_committed_preview_application` and
`list_committed_preview_applications` behind `GET …/preview-applications?source=committed`;
`preview_view`/`WorkspacePreviewView.pending_start`; relay target port from the ready
event's payload; route `startCompute` with `begin`/`abandon`, resolving the committed
application through `TaskService::workspace_vm_is_running` (route SQL guard).

Frontend: `PreviewControl.tsx` (trigger gate, committed listing, request flag, pending
state, labels, notice), `useTasks.ts` source parameter, `StartAndSendNotice` sentence
prop, `previewRunWords`/Activity wording for the pending and `start_ended` states, types.

## Verification

- Backend integration (sibling of `api_personal_start_and_send.rs`): compound start
  records the consent and a bound queued preview; cancelling the Start cancels the task
  with `start_ended` and settles the intent; consent reaching ready releases the task and
  the dispatcher admits it; expiry cancels it; a second start while pending is refused;
  replay with the same key returns the same task; non-owner refused; `Attached` outcome
  takes the ordinary path; a bound task is never reserved while the VM runs under a
  different consent.
- Backend unit: committed-source resolution (default, named, missing file,
  unmaterialized branch); relay port preference.
- Frontend (`PreviewControl.test.tsx`): stopped owner opens the dialog with the committed
  list and sends `startCompute`; pending state and notice; reload shows the pending
  preview; `start_ended` ending offers Start again; starting/queued sends without
  `startCompute`; reader, Base and merged mode unchanged. Existing tests asserting the
  disabled trigger while stopped are inverted.
- E2E (`workspace-preview.spec.ts`): after M12's Stop, one Start workspace and preview
  brings the workspace up and serves the workspace-default preview; "Resume requires an
  explicit relaunch" stands, since nothing relaunches without the owner's request.
