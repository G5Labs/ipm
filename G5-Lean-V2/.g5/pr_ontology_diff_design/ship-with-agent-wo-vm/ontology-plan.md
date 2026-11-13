# Start and ship with agent on a stopped VM

Request and decisions: [chat intent](../../chat_intent/ship-with-agent-wo-vm/request.md);
audit, options and mock: Project Doc `ship-with-agent-without-vm.md` in the docs repository
(workspace 15159c69-2438-4c99-b200-53bbd2841ee0).

## Problem

Ship with agent ([g5.git.pr-auto-merge.with-agent](../../ontology/software_architecture/git.g5.md#g5.git.pr-auto-merge.with-agent))
queues the agent's chat in the source workspace with no Start consent. With the owner's VM
stopped, Merge when ready turns on, the chat opens with the message Queued, and the agent never
runs until the owner presses Start separately. It is the one owner control ending in a chat
request that [g5.chat.start-and-send](../../ontology/features/chat.g5.md#g5.chat.start-and-send)
does not yet cover; it was missed because the 2026-11-03 ribbon audit listed Prepare to Ship's
controls as durable.

## Decision

While the source workspace's VM is stopped or failed, the owner's click is **Start and ship with
agent**: one request records the Start consent, queues the agent's chat bound to it and turns
Merge when ready on. While a Start is under way the control stays Ship with agent and queues the
chat behind it; while running nothing changes. A Start that ends before compute is ready withdraws
the chat as Start and send does and leaves the merge-when-ready decision as made (card Q3,
proposed). The agent launch keeps coming from the dialog's Auto-resolution settings.

Owner's reason: Merge when ready and the agent are one coupled action; splitting them into a seed
and a send (the first answer) was rejected on 2026-11-05.

## Owners and sequence

Nothing new is owned or stored. The VM lifecycle owns the consent
([g5.lifecycle.start](../../ontology/software_architecture/workspace_vm.g5.md#g5.lifecycle.start),
Resume with the `compound_start` kind); agent dispatch owns the input-to-consent binding, settled
by the `workspace_vms` trigger in the transition that ends the consent; the auto-merge service
owns the decision on the pull request; the workspace messenger owns the queued chat. The route
orders them exactly as `POST /workspaces/:id/sessions/start` does through
`routes/start_and_send.rs`: `begin` for the source workspace before any transaction (Resume locks
the workspace row), the decision in the target transaction, `bind` after the messenger queues the
input inside the source transaction, commits in today's order, `abandon` on any later failure.
Admission is unchanged: `begin` runs the source owner's Resume admission, so a non-owner is
refused before anything is written, preserving "a refusal of the source-ownership check changes
nothing". A replayed request (same `requestKey`, chat already claimed) records no second consent,
by the check the start-session route makes before `begin`. Two requests with the same key that
both pass that check are excluded by the client (one in-flight submission per key, the control
disabled while busy). If they still interleave, the second records a fresh consent only while the
first still waits for capacity (a VM already starting refuses a second compound Start), and that
fresh consent supersedes the first and withdraws its chat: the shared rule of
[g5.chat.start-and-send](../../ontology/features/chat.g5.md#g5.chat.start-and-send), which the
owner reaffirmed for this control. The route then refuses the replay of the withdrawn chat and
abandons the consent it recorded, so the client sees a failure rather than a success without a
chat; the decision made by the first request stands and Ship with agent can be pressed again.
Serializing key check, consent and claim at the API boundary would have to cover every Start
and send route and is out of scope here (CodeRabbit review of PR #1128).

Why not bind the decision to the consent: the decision is the person's independent choice, which
[g5.git.pr-auto-merge](../../ontology/software_architecture/git.g5.md#g5.git.pr-auto-merge) keeps
once made; binding it would add a cross-owner effect to the VM trigger for no required outcome.
Why not a composer seed: rejected by the owner (coupled action); it would also have exposed a
system-written prompt to editing and left Merge when ready on with no agent when a draft is
dropped.

## Ontology changes

- `g5.chat.start-and-send`: add Prepare to Ship's Start and ship with agent to the compound
  actions.
- `g5.git.pr-auto-merge.with-agent`: the compound Start while the source VM is stopped or failed;
  the chat withdrawn and the decision kept when the Start ends early.
- `g5.workbench.ship-to-parent`: the label and note while stopped; the Start progress while
  starting.

## Implementation outline

Backend (`backend/g5-api-routes/src/routes/workspaces.rs`): `AutoMergeAgentDTO.start_compute`;
`enable_workspace_pull_request_auto_merge` resolves the source (`pull_request_source`), skips the
consent when the request key is already claimed, calls `begin`, and `start_merge_agent` calls
`bind` with the queued input id (check what `workspace_messenger.send` returns; extend its result
if the input id is not exposed). `EnableAutoMergeResponseDTO.start_compute: Option<StartComputeDTO>`.

Frontend: `PrepareToShipModal` reads `useWorkspaceLiveGate`; `startAndSend = gate.owner &&
liveGateCanStart(gate.kind)` drives the label "Start and ship with agent", the note, and
`startCompute: true` in `changeAutoMerge`; `StartAndSendNotice` shows progress while starting or
queued. `useSetAutoMerge` invalidates `workspace-vm` when the response carries `startCompute`.

Tests: integration cases in `backend/g5-api/tests/workspace_pull_request_auto_merge.rs` (consent
recorded and input bound on a stopped VM; replay records one consent; cancelled Start withdraws the
input and keeps `auto_merge_enabled_by`; non-owner leaves no consent and no decision); modal tests
per gate kind and for the request body; the e2e flow SH3 in
`e2e/flows/ship-to-parent.spec.ts` (label and note on a stopped child with an open pull request;
the compound request itself is covered by the integration tests).

## Decisions closed and evidence

- Card Q8 (compound start) confirmed in chat on 2026-11-05 ("1/ yes"); card Q3: the owner asked
  why the queued message is withdrawn, was shown the shared rule and the "stays queued"
  alternative, and chose "keep same as other behaviours": the Start-bound chat is withdrawn when
  the Start ends early and the merge-when-ready decision stays.
- Implemented: `begin_merge_agent_start`, `enable_auto_merge_with` and the `bind` in
  `start_merge_agent` (`routes/workspaces.rs`); `WorkspacePullRequestAutoMergeInput.agent.startCompute`
  and the VM-query invalidation (`hooks/useWorkspace.ts`); the label, note and progress in
  `PrepareToShipModal.tsx`. Tests: `workspace_pull_request_auto_merge.rs` (consent recorded and
  chat bound on a stopped VM, replay records one Start, Stop withdraws the chat and keeps the
  decision; running VM just queues; non-owner refused with nothing written) and
  `PrepareToShipModal.test.tsx` (stopped, failed, starting and running gates, non-owner; request
  body); e2e SH3 in `ship-to-parent.spec.ts`.

## Follow-up: Start and resolve with agent

Owner request (2026-11-06, workspace chat, with a screenshot of the Resolve Git Conflicts
dialog over Prepare to Ship): "resolve with agent button should also support non vm attach
version, basically if vm is not there, you should start a vm and send the message to resolve
the conflicts. add this fix to existing pr and mock ups too."

### Audit

The conflicts dialog (`DirectCleanMergeChoiceModal`, reached from Prepare to Ship's Get the
latest changes step and from the Rebase dialog) said "Start its workspace VM first" and its
Resolve with Agent posted `POST /workspaces/:id/rebase-merge`. On a stopped VM that route took
the central path, whose conflict branch `enqueue_source_resolution` admitted the task only under
a running owner attachment (`owner_handle` / `lock_owner_handle`), so the click failed with
`WORKSPACE_NOT_RUNNING` and nothing was queued; while a Start was under way,
`source_rebase_execution` refused with "starting or stopping". The resolution brief itself
needs no VM at enqueue: Git pins the inputs centrally and the VM fetches them at dispatch.

### Decision

Same compound design as Start and ship with agent, under the same owner decisions (Q8, Q3) and
main's joining rule (`g5.preview.start-and-preview.joining`, which names every compound action):
while the owner's VM is stopped or failed the choice reads **Start and resolve with agent** with
a note, and one request records the Start consent and queues the brief bound to it; while the
owner's Start is under way the choice stays Resolve with Agent, shows progress, and joins that
Start; a Start that ends early withdraws the brief. Refusals that start no VM: dirty or
unpublished saved work (the VM would restore it and the agent's brief requires a clean tree),
Git the agent cannot fix (`WorkflowRequired` outcomes), a non-owner. A clean rebase keeps landing
centrally; a VM found running when the Start is recorded takes the in-VM path. Owning statement:
`g5.git.stopped-rebase.resolve-with-agent`; `g5.chat.start-and-send` lists the action.

### Sequence and owners

`begin` runs after the central preflight has classified the rebase as a conflict (so a clean
rebase or an unfixable input never starts a VM) and before the queuing transaction (Resume locks
the workspace row). Under the workspace commit fence the route rechecks that nothing landed and
the VM still is not running (`source_rebase_state == Starting`), admits the task through
`lock_owner_pending_start` (the consent is the actor's, live and within its lifetime; the
workspace not Base, deleted or transferring) instead of the running attachment, queues the brief,
binds its input to the consent (`bind`, the shared share-lock rule) and commits; any failure after
`begin` abandons a consent this request recorded. `active_workspace_resolution` no longer counts a
task whose brief was withdrawn, so a fresh click after an early-ended Start admits a new
resolution while the sweep cancels the stale task. A second click while the Start is under way
joins it and returns the active task without a second binding.

### Evidence

Backend `api_workspace_pull_requests.rs`: `start_and_resolve_with_agent_binds_the_brief_to_the_owners_start`
(plain request still refused; consent recorded with kind `compound_start`; brief bound; second
click joins; Stop withdraws; fresh click admits a new task under a new Start) and
`start_and_resolve_with_agent_refuses_before_starting_when_it_could_not_run` (dirty receipt,
non-owner). Frontend `DirectCleanMergeChoiceModal.test.tsx` (stopped, failed, starting, queued,
running, non-owner; the `startCompute` option) and `WorkspacePageBasePull.test.tsx` (request body,
VM re-read, queued toast, chat opened). Mock and Project Doc section 6 in the docs repository.

