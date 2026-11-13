# Workspace agents: child workspaces and stop by user

[G5-Lean#710](https://github.com/G5Labs/G5-Lean/issues/710) asks for UI parity for the
agent in a workspace VM, child-workspace coordination, owner scope for personal
workspaces, and **stop workspaces by user** in place of the earlier subtree stop. Its
comments hold the audit, a capability matrix and proposed ontology blocks.

This design covers the issue's first step: stop by user, plus the complete
parent/child loop (fork with a prompt, probe, chat, merge), Start/Stop and child
deletion. Each is reached with the VM's own credential and the same domain operations
the UI uses. [Human intent and interpretation](../../chat_intent/issue-710-workspace-agent-parity/request.md).

Prepared against `origin/main` at `a6e8aa7e` on 2026-10-22. The owner asked for implementation the same day; [implementation notes](#implementation) record where the code departs from or refines this design.

## Scope

**In scope**

- Stop by user, for the user, any of their workspace agents and an organization
  administrator.
- The agent scope rules:
  - operations: own workspace plus direct same-owner children, widened in
    [increment 8](#increment-8-owner-scope) to every workspace of the same owner;
  - reads: any workspace the owner can read in the UI (owner decision, 2026-10-22).
- From the VM:
  - fork a child with a prompt and launch it;
  - list and probe children;
  - list readable workspaces and read their chats: messages and timelines, including
    tool calls and output;
  - start a chat or send a follow-up in a child;
  - Start or Stop self or a child;
  - merge a child into the agent's own workspace through the ordinary pull-request path;
  - delete a child.
- `g5d` commands and agent guidance (`.agents/AGENTS.md`) for these operations.

**Deferred, in the issue's order**

1. **Personal owner scope.** Done in [increment 8](#increment-8-owner-scope), for every
   caller. Deferred until [PR #645](https://github.com/G5Labs/G5-Lean/pull/645)
   landed (it has since merged). The owner's decisions on deletion, fork sources and
   credential exposure are recorded (see [owner decisions](#owner-decisions)). The scope check below is written so owner
   scope becomes a third accepted relation, with no new mechanism.
2. **Other reads and controls.** Previews, comments, approvals, activity, and PR close
   and refresh.
3. **Git and review workflows.** Provider sync, review copies and rebase-merge launch.
4. **Workspace configuration.** Description and settings.
5. **Capability catalog and parity tests** for the whole matrix.

## Governing contracts

| Contract | How it constrains this design |
| --- | --- |
| [`g5.ui.api-parity`](../../ontology/features/ui.g5.md#g5.ui.api-parity) | Agents reach UI actions through the same backend operations; `g5d` adds commands. |
| [`g5.workspace.mutation-authority`](../../ontology/features/workspaces.g5.md#g5.workspace.mutation-authority), [`g5.security.owner`](../../ontology/software_architecture/workspace_vm.g5.md#g5.security.owner), [`g5.security.current-authority`](../../ontology/concepts/security.g5.md#g5.security.current-authority) | The agent acts for the target's current owner. Every request rechecks current authority. |
| [`g5.security.credential-classes`](../../ontology/concepts/security.g5.md#g5.security.credential-classes) | A VM identity never becomes a person session. Person-account operations stay out of reach. |
| [`g5.lifecycle.start`](../../ontology/software_architecture/workspace_vm.g5.md#g5.lifecycle.start), [`g5.lifecycle.stop`](../../ontology/software_architecture/workspace_vm.g5.md#g5.lifecycle.stop), [`g5.lifecycle.stop.claim`](../../ontology/software_architecture/workspace_vm.g5.md#g5.lifecycle.stop.claim) | Fork-and-launch is a labelled compound Start. Queued input never starts a VM. Retries cannot revive cancelled consent. |
| [`g5.lifecycle.member-deactivation`](../../ontology/software_architecture/workspace_vm.g5.md#g5.lifecycle.member-deactivation) and [`.recovery`](../../ontology/software_architecture/workspace_vm.g5.md#g5.lifecycle.member-deactivation.recovery) | The mechanism stop by user generalizes. |
| [`g5.workspace.fork-state.formal`](../../ontology/features/workspaces.g5.md#g5.workspace.fork-state.formal), [`g5.chat.creation.idempotency`](../../ontology/features/chat.g5.md#g5.chat.creation.idempotency) | Fork and chat replay keep their request identity. |
| [`g5.chat.authority`](../../ontology/features/chat.g5.md#g5.chat.authority), [`g5.chat.ownership`](../../ontology/features/chat.g5.md#g5.chat.ownership), [`g5.chat.queue`](../../ontology/features/chat.g5.md#g5.chat.queue) | `ChatService` and `TaskService` keep chat and queue ownership. The agent acts for the owner. |
| [`g5.workspace.merge`](../../ontology/features/workspaces.g5.md#g5.workspace.merge), [`g5.git.merge-recovery`](../../ontology/software_architecture/git.g5.md#g5.git.merge-recovery), [`g5.workspace.approval-human-authority`](../../ontology/features/workspaces.g5.md#g5.workspace.approval-human-authority) | Merge keeps approvals, pinned versions and recovery. An agent request is not an approval. |
| [`g5.workspace.delete`](../../ontology/features/workspaces.g5.md#g5.workspace.delete) | Deletion checks are unchanged. Code gap `WORKSPACE-CASCADE` means a child with live children is refused. |

## Ontology changes

All changes are in this branch's diff.

**New blocks**

- `features/workspaces.g5.md`, new section "Agent operation":
  - stories `g5.story.agent-fork-child`, `.agent-probe-child`, `.agent-chat-child` and `.agent-merge-child`;
  - `g5.security.agent-workspace-delete`;
  - story `g5.story.stop-workspaces-by-user`.
- `concepts/security.g5.md`, "Workspace execution":
  - `g5.security.agent-workspace-scope` (D);
  - `g5.security.agent-read-scope` (D, human-provided by the owner's 2026-10-22 choice);
  - `g5.security.agent-admitted-work` (D);
  - `g5.security.agent-human-exceptions` (D).
- `features/chat.g5.md`: `g5.security.agent-chat-control` (D).
- `software_architecture/workspace_vm.g5.md`: `g5.lifecycle.stop.by-user` (D) and
  `g5.lifecycle.stop.by-user.release` (I).

**Amended blocks**

- `g5.lifecycle.member-deactivation.recovery`: deactivation now commits the shared
  execution hold.
- `g5.security.owner`: "no unfinished deactivation cleanup" becomes "no execution hold".
- `g5.lifecycle.stop.window`: stop-by-user stops get the two-minute save window.
- `g5.chat.authority`: the owner's workspace agent acts for them within
  [agent scope](../../ontology/concepts/security.g5.md#g5.security.agent-workspace-scope).
- Metadata of `g5.ui.api-parity`: links this design and records the remaining gaps.

**Not added**

- A separate personal owner scope block. [Increment 8](#increment-8-owner-scope) widened
  `g5.security.agent-workspace-scope` itself instead.
- The issue draft's "transfer to the held user is refused". See
  [stop by user](#why-transfer-needs-no-new-refusal).

## Stop workspaces by user

### State and ownership

The membership's existing `deactivation_cleanup_id` becomes the **execution hold**. It
is a constant-size, independently retained identifier. It already refuses new
execution and runtime admission for every workspace the user owns in the
organization, through these existing checks:

- `PermissionService::require_active_member_on_conn(execution = true)`;
- `authz.workspace_execution_allowed`;
- `authz.lock_workspace_member_activity`;
- the `workspace_vm_member_activity` trigger.

`WorkspaceVmService::reconcile_member_deactivation` already discovers the user's
workspaces in bounded batches and drives ordinary `admit_stop` with a durable retry
schedule. `authz.finish_member_cleanup` already clears the identifier only once no
owned workspace is starting, running or stopping, holds a VM, has an active Start
intent, or has a queued or running task.

Stop by user is that cleanup without `deactivated_at`. Organization access is
unchanged, and the hold clears on settlement.

- `OrgService` owns the request: it sets the hold.
- `WorkspaceVmService` remains the only shutdown owner.
- `TaskService` keeps holding queued work (`waiting/start_required`).

| Membership | Hold | Access | Execution |
| --- | --- | --- | --- |
| Active | none | normal | normal |
| Active | set (stop by user) | normal | refused; stop drives every owned workspace |
| Inactive | set or none | denied | refused |

### Operation

`OrgService::stop_member_workspaces(actor, org, user)`:

1. Locks the target membership, which must be active.
2. Checks the actor: the user themself or an active administrator with `org:admin`.
   A workspace-agent caller is the VM credential's current owner (see
   [scope](#agent-scope)).
3. Sets `deactivation_cleanup_id = COALESCE(deactivation_cleanup_id, new id)` and
   commits. A repeated or concurrent request joins the pending hold, so the operation
   is idempotent without a client key.
4. Returns the hold and the existing per-workspace progress
   (`OrgService::member_shutdown_progress`, extended with each VM's `last_error`
   reason so an unreachable host or failed save is visible as a blocker).

Nothing else changes inside the transaction. Start consent is invalidated by the
existing path:

- From the moment the hold commits, `require_active_member_on_conn` refuses Start and
  `workspace_execution_allowed` keeps the start queue from re-driving queued intents.
- Cleanup's `admit_stop` then cancels the intent and advances `lifecycle_generation`.
- Ordinary Stop drains each workspace, with its stages, save window, claim and loss
  record.

### Code changes

- **Reconciler stop reason.** The reconciler passes `StopReason::MemberDeactivated`
  when the membership is inactive, and a new `StopReason::ExecutionHold`
  (`execution_hold`) otherwise.
- **Places the new reason must be added:**
  - the `workspace_vms` stop-reason CHECK constraints;
  - the supersession list in `g5_workspace_vm_identity_is_fenced`;
  - the task hold mapping (`waiting/start_required`).
  - It uses the two-minute save window and no nudge.
- **Owner constraint.** Relax `org_owner_always_active` to
  `role <> 'owner' OR deactivated_at IS NULL`, so the organization owner can be held.
  The service still refuses to deactivate the owner.
- **Column name.** Keep `deactivation_cleanup_id`. Renaming it would mean recreating
  seven `authz` functions, a constraint and the Rust queries for no behavioural gain.
  The new migration comments the column, and the ontology names the concept.

### Callers and surfaces

| Caller | Entry point |
| --- | --- |
| The user | `POST /api/v1/orgs/:orgId/members/:userId/stop-workspaces` where `userId` is the caller. The Members page offers it on the user's own row. |
| An administrator | Same route, `org:admin`. The Members page offers it on every row and reuses the shutdown-progress view. The existing `g5_audit` user-action event records it. |
| Any running workspace agent of the user | `POST /api/internal/workspaces/:id/stop-owner-workspaces` (`g5d workspace stop-all`). The target is always the token's own `owner_user_id` and `org_id`. |

`GET /:orgId/members/:userId/shutdown` is widened from administrators-only to the user
themself. An internal `GET` returns the same progress to the agent while its VM still
runs.

A caller is not exempt. An agent that stops its owner's workspaces stops its own VM
too, after the save window, and the human starts what they want again.

### Interactions

- **Fork during the pass.** Fork publication needs only active membership, so it
  completes and creates a stopped child owned by the user. The fork's initial chat is
  refused by `enqueue_agent_input` and recorded as `start_chat_error`. The route's
  launch Start is refused too, so the child never attaches compute. Another owner's
  fork of one of the user's workspaces is owned by that other person, outside the hold.
- **Deactivation during the pass.** Deactivation keeps the existing identifier
  (`COALESCE`), so the cleanup continues with `MemberDeactivated` for the stops
  admitted after it. Reactivation never clears the hold.

<a id="why-transfer-needs-no-new-refusal"></a>

- **Transfer to a held user** is not refused. Transfer terminates the VM before it
  commits, so the workspace joins the held set already stopped and cannot start until
  the hold clears. Settlement counts it like any other owned workspace. The issue
  draft's refusal adds nothing the hold does not already guarantee.
- **Stop reason.** The reconciler's `admit_stop` reads `deactivated_at` under the same
  membership lock that fences the cleanup identifier, and records `member_deactivated`
  or `execution_hold` accordingly, so a deactivation during the pass is reflected in
  the stops admitted after it.
- **Release.** `settled_member_cleanups` and `finish_member_cleanup` clear the hold on
  the next lifecycle tick after everything has settled. Nothing restarts. Held tasks
  stay `waiting/start_required`, and old Start intents are cancelled, so new compute
  needs an explicit Start. There is no unfreeze step.

### Why not something smaller or different

| Candidate | Problem |
| --- | --- |
| Subtree stop with a fork freeze (earlier design) | A personal agent's work has no common ancestor. The freeze blocked other owners' reader forks. |
| Loop over ordinary Stop from the caller | A fork that finishes after the loop starts, or a queued Start, escapes. Losing the caller abandons the pass. |
| A separate hold column beside the cleanup identifier | Two markers with identical admission and settlement semantics. Deactivation would then need both, so every check would need both. |

## Agent scope

The VM credential (`WorkspaceIncarnationClaims`) proves one live incarnation of one
workspace. A new module, `db::workspace_agent_scope` (beside `db::scoped`, the other
request-scoping helpers), decides a target from that credential:

```text
agent_target(db, claims, target_id, need) -> SelfWorkspace | Other
```

**Checks, in order**

1. The caller incarnation is current (`workspace_incarnation::current_incarnation`,
   `Share`).
2. The target is live, in `claims.org_id` and not a formal review workspace.
3. `target.owner_user_id = claims.owner_user_id`.
4. Not Base, and a target other than the caller is not a review copy. (Until
   [increment 8](#increment-8-owner-scope): `target = claims.workspace_id` or
   `target.cloned_from_id = claims.workspace_id`.)

`need` is `SelfOrOther`, or `OtherOnly` for chat mutations and delete. A refusal is
`Forbidden` with no detail that distinguishes "not in scope" from "does not exist".

**Why this is sufficient**

- `cloned_from_id` is written only at fork completion and never updated.
- The remaining facts are rechecked under each reused service's own row locks:
  - the target's current owner (each service requires `owner_user_id = actor`);
  - deletion;
  - a pending transfer.
- The caller's own incarnation can go stale between the check and the effect. That
  is the same window the existing VM-token PR create accepts. The effect still runs
  with the owner's current authority, so no new authority leaks.

**Database scope.** The route opens the owner-actor scope that VM PR create already
uses: `scoped_db_for_user_actor_in_org(owner, org, scopes)`. RLS, active membership
and the owner checks therefore apply exactly as for the owner's browser.

- The VM token never becomes an `AuthContext::User`.
- Person-account operations remain unreachable.

**Attribution.** Records keep the owner as actor, as VM PR create does. Each
mutating request emits one `g5_audit` event, `workspace_agent.action`, whatever its
outcome ([Increment 4](#increment-4-audit-and-files)), carrying:

- caller workspace, VM and epoch, and the owner;
- route, path, target workspace, status and outcome class;
- the handler's operation and result, and a file's repository and paths.

Persisting an agent-origin marker on records is left to owner scope, where the
issue's safety review asks to show delegated actions in the target's activity.

**Reads** use a separate relation. A read target is any workspace the owner can
currently read: the route applies `scoped::ensure_user_workspace_access` to the
owner, the same check `scoped_db_for_chat_session_view` applies to a browser. That
includes other people's organization-visible workspaces. For an organization owner or
administrator it includes every live workspace (`g5.workspace.audience`), while
formal review workspaces stay excluded as in the UI. The caller incarnation is checked
first, as for every agent route. Reads touch only the database, never a VM.

**Owner scope** ([increment 8](#increment-8-owner-scope)) replaced check 4: any live
workspace of the same organization and current owner, Base and review copies
excluded, for every caller. Only the scope function and the fork source changed.

## Child operations

Each is a thin internal route plus a `g5d` command. The routes are in
`routes/workspace_agent.rs`, under `/api/internal/workspace-agent/:id/…` with `:id` the
token's workspace, and are allowlisted in `is_workspace_incarnation_internal_route`.

| Operation | Reused owner and change | Replay identity |
| --- | --- | --- |
| **Fork with a prompt** | `WorkspaceForkService::fork_workspace`, requested by the owner, source = the caller's own workspace, `ForkSource::Durable`. The fork-then-launch Start moves from the `clone_workspace` route into one `WorkspaceForkService`-level composition shared by the UI route and the agent route (see [fork and launch](#fork-and-launch)). | Fork key `agent:<key>` in the owner's namespace. |
| **List children** | One read: direct live non-review children with the same owner. Each row has name, branch, VM status, start-intent status, active task count and pending forks from this workspace. | — |
| **Probe a child** | Composes existing reads: workspace detail, `WorkspaceVmService::view` (status, start intent, stop reason, `last_error`), `TaskService::list_workspace_tasks` (active and recent, with outcome), `ChatService::list_sessions`. No VM call, so a probe never starts or prolongs compute. | — |
| **List readable workspaces** | `WorkspaceService::list_workspaces` for the owner, the Home list. | — |
| **Read chats in a readable workspace** | `ChatService::list_sessions`, `get_messages` and `get_session_timeline` (tool calls and output from `task_events`), and `TaskService::list_workspace_tasks`. The read relation applies, so the caller's own workspace and other people's visible workspaces are included. | — |
| **New chat in a child** | `WorkspaceMessenger::send`, `MessageTarget::NewSession`, actor = owner. | `agent:<key>` request key. |
| **Follow-up in a child chat** | The follow-up launch inheritance moves out of the `send_message` handler into one `queue_follow_up` function that the UI route and the agent share: fixed provider, frozen permission mode, model/effort/account fallback. `WorkspaceMessenger` gains `MessageTarget::ExistingSession` and `record_claim`, so the follow-up's key is claimed and recorded in `workspace_message_requests` like a new chat's. The input is queued. Steering into a running turn stays UI-only for now (recorded gap). | `agent:<key>`, so a lost reply does not duplicate input. |
| **Start a child** | `WorkspaceVmService::resume(child, owner, intent, Resume)`. A chat command's `--start` makes the same call after the input commits, as a labelled compound, and only when that request first accepted the input: a replayed chat request starts nothing, so it cannot revive consent a later Stop cancelled. | Intent id is a UUIDv5 of the agent key. |
| **Stop self or a child** | `WorkspaceVmService::stop(target, owner, op, User)`. The admitted stop outlives the caller ([`stop.claim`](../../ontology/software_architecture/workspace_vm.g5.md#g5.lifecycle.stop.claim)). | Operation id is a UUIDv5 of the agent key. |
| **Merge a child into self** | The UI path. `workspace_pull_request_create::create_or_update` takes a new `PullRequestCaller::WorkspaceChild`: the caller's own incarnation is checked and the source must be an authorized child, while the child's running VM, if any, saves through its owner handle as for the owner's browser (a stopped child with unsaved work is refused, as in the UI). The UI's whole PR merge handler body becomes `merge_pull_request_as`, shared by a `PullRequestMerger::Person` and a `PullRequestMerger::WorkspaceAgent`, which may merge any open PR whose target is the caller's own workspace: external-check policy, approvals and provider-first merge included. | PR identity. Merge reconciles provider acceptance before re-admission. |
| **Delete a child** | The same two operations as the UI's `delete_workspace_action`: `WorkspaceVmService::stop`, then `WorkspaceService::delete_workspace`, each of which rechecks the owner and the stopped VM itself. `ChildOnly`: the caller's own workspace is refused before any effect. A child with live children is refused by the existing check, so an agent delete never reaches grandchildren. | Operation id is a UUIDv5 of the agent key. |
| **Stop by user** | [Above](#stop-workspaces-by-user). | Joins the pending hold. |

### Fork and launch

Today `clone_workspace` starts the forked child with a fresh intent id on every call.
A replay of a completed fork key therefore creates new Start consent, even after the
child was stopped, which violates `g5.lifecycle.start`.

The shared composition fixes this for both callers:

- **Intent id.** It derives the intent id from the fork operation id.
- **When it may Start.** Only while the child's `start_intent` is absent or already
  carries that id.
- **Replays.** A replay re-observes the original Start; it never adds a new one once
  the child has other lifecycle history.
- **Result.** It returns separate fork, chat (`start_chat_error`) and Start outcomes,
  and the child stays discoverable when the launch fails or waits for capacity.

A durable fork copies the caller's last complete flush, or its committed branch heads.
A live fork from inside the parent would interrupt the parent's own running turn
(`WORKSPACE_BUSY` or interrupt), so the agent path does not offer it.

Agent guidance therefore says: commit and push your work before forking, because the
child starts from what the workspace branch holds.

### Merge into a running parent

A PR merge advances the target branch centrally, exactly as when the owner merges in
the UI while the parent VM runs. No barrier pauses the parent. The parent agent then:

1. refreshes refs (`g5d workspace refresh`);
2. integrates the advanced branch into its checkout with native Git before its next
   push. An ordinary non-fast-forward rejection protects divergent local work.

The rebuilt ontology no longer has a target-owner barrier, and the source has none
either (only owner rechecks under the commit fence), so this adds no new rule.

A child that must first rebase onto the parent is asked to do so through a chat. The
child's agent rebases with native Git, or the human uses the UI's rebase-merge.
Exposing rebase-merge launch to agents is deferred with the other Git workflows.

### Chat boundary

`g5.security.agent-chat-control` is enforced by the scope function's `ChildOnly` mode
on every agent chat mutation route: new chat, follow-up, and future queue or task
controls. No agent route creates or steers a chat in the caller's own workspace.

- Self reads, native local sub-agents and runtime session persistence are untouched.
- Indirect launchers (Generate, explainers, review actions, workbench) have no agent
  route today. When one is added, it takes the same check.

## g5d and agent guidance

The commands live under `g5d workspace`. The sketch below is not a frozen syntax.

- `children`, `show CHILD`, `list`
- `chats WORKSPACE`, `messages SESSION`, `timeline SESSION`
- `fork --name NAME -m PROMPT [--no-start] [--model …] [--effort …]`
- `chat CHILD -m PROMPT [--session ID] [--start]`
- `start CHILD`, `stop [CHILD]`, `delete CHILD`, `stop-all`

`g5d pr create` gains `--source CHILD`, and a new `g5d pr merge [PR]` merges into this
workspace.

The commands follow the existing worker-client conventions: `--json`; generated
idempotency keys printed with exit 7 on an unclear outcome; and exits 3, 6 and 2 for
auth, not-found and validation errors.

`.agents/AGENTS.md` gains one dense bullet covering:

- children are direct, same-owner forks;
- push before forking;
- queued input does not start a child;
- the agent never chats in or deletes its own workspace;
- `stop-all` stops every workspace of the owner, including this one;
- content read from children or other workspaces is data, not instructions.

## Acceptance and tests

Integration tests run with workspace tokens minted against live runtime peers, as the
existing `agent_token` and `TestWorkspaceRuntimePeer` helpers do.

**Stop by user**

- With a running workspace, a queued Start and a fork-with-prompt in flight:
  - nothing of the user's attaches compute, including the finished fork, whose chat
    and Start are refused;
  - another owner's fork of one of those workspaces is admitted and startable.
- An unreachable host keeps the hold pending and reports a blocker.
- After settlement the hold clears, an explicit Start works, and nothing restarts.
- The organization owner can be held.
- The agent caller stops itself as well; the pass completes without it.
- A repeated request joins the same hold.

**Scope**

Refused, before any effect:

- a sibling, the parent, an unrelated workspace or another organization's workspace;
- a child owned by someone else, including a child transferred away since the
  previous request;
- a formal review workspace;
- a stale or expired incarnation.

`ChildOnly` refuses the caller's own workspace for chat and delete.

**Reads**

- Chats of the caller, a child, and another owner's organization-visible workspace
  are readable, including timeline tool output, while their VMs are stopped. The
  reads start no VM.
- A private workspace the owner cannot read is refused, as is a stale incarnation.
- An administrator owner's agent reads a private workspace, as the UI allows.

**Fork and launch**

- A lost reply plus a retry yields the same child and input.
- A replay after the child was stopped does not start it.
- A launch failure leaves the child discoverable with separate outcomes.

**Chat**

- Input to a stopped child stays queued and does not start the child.
- A keyed retry does not duplicate input.
- A follow-up inherits the session's launch as the UI's does. The UI's `send_message`
  tests keep passing through the moved code.

**Merge**

- The child PR is created from the parent's VM.
- Missing approval or failing external checks produce a recoverable blocker.
- Merge advances the parent branch; the parent's stale push is rejected, not
  overwritten.

**Delete**

- A child is deleted.
- The caller's own workspace and a child that has children are refused.

**Regression checks**

- `route_auth_static`, `system_scope_binding_guard`, `selected_org_scope_audit` and the
  middleware allowlist test.
- The existing member-deactivation suite (`api_member_activity.rs`).

## Owner decisions

The issue's open decisions are in the question set `issue-710-owner-decisions`.

- **No decision blocks this slice.**
- **Decisions 1–3** (personal deletion, fork sources, credential exposure) gate
  personal owner scope.
- **Decision 4** (agent-requested formal agent reviews) gates approval requests.
- **Decision 5** (issue close and reopen) keeps the current exclusion unless changed.
- **Decision 6** (agent access to a child's preview) chose a task-bound, non-browser
  preview credential; [Increment 7](#increment-7-agent-preview-credential).

## Assumptions and risks

- **Credential exposure.** Any guest process can read `G5D_WORKSPACE_TOKEN_FILE`. With
  child scope it can now fork this workspace, and start, chat in, merge from or delete
  its same-owner children. The owner already accepted this
  ([guest authority](../../ontology/concepts/security.g5.md#g5.security.guest-authority)):
  the same process can already rewrite the workspace branch, and a child is reachable
  only from its parent. Owner scope widens the reach and is gated on decision 3.
- **Read exposure.** The same token can read every conversation the owner can read,
  including tool output. For an administrator owner that is the whole organization.
  The owner chose this read parity on 2026-10-22. Redaction
  ([`g5.security.redaction`](../../ontology/concepts/security.g5.md#g5.security.redaction))
  still applies to stored events. PR #645 deferred delegated reads for personal
  workspaces pending a credential-exposure decision; on rebasing onto it
  (2026-10-26) `g5.security.credentials.delegated-read` now defers only a separate
  credential, and the personal agent reads under
  [`g5.security.agent-read-scope`](../../ontology/concepts/security.g5.md#g5.security.agent-read-scope)
  with the existing VM credential (decision 3).
- **Machine-speed creation.** Stop by user is the containment. The per-owner limits
  ([design](../owner-resource-limits/ontology-plan.md)) also bound an owner's
  workspaces, pending forks included, and running or waiting VMs, for UI and VM
  callers alike; queued input stays unbounded.
- **Migration numbering.** The migration (relaxed owner constraint, new stop reason)
  is `117_execution_hold.sql`, after main's `116_chat_activity_inspection.sql`; it
  carries forward the `owner_removed` stop reason PR #645 added in `110` and `111`.

## Increment 2: controls, approvals, PRs and issue state

The owner answered the five open decisions on 2026-10-22
([record](../../chat_intent/issue-710-workspace-agent-parity/request.md#decisions-in-this-workspace)).
Decisions 1–3 belong to personal owner scope, deferred until PR #645. Decisions 4 and 5 enter this
increment, which continues the issue's step 3 for the agent's own workspace and its
children. Every route reuses the UI's service operation as the owner, with the target
admitted by `agent_target` (`ChildOnly` for chat controls, `SelfOrChild` otherwise).

| Operation | Reused operation | Target |
| --- | --- | --- |
| Stop a chat turn | `TaskService::end_chat_turn`, then the post-commit `deliver_workspace_task_cancel` | child |
| End a background wait | `end_session_background_wait` (needs the child's running VM) | child |
| List queued inputs | `list_session_inputs` (read relation) | readable |
| Edit, withdraw, redeliver queued input | `edit_queued_session_input`, `withdraw_queued_session_input`, `authorize_unknown_input_redelivery`, with their revision guards | child |
| Rename a chat | `ChatService::update_session_title` after `ensure_workspace_mutable` | child |
| Continue or cancel a task | `continue_workspace_task`, `request_workspace_task_cancel` + delivery | child for agent (chat) tasks; self or child for other tasks, such as a preview |
| Read approval state | `approval_config_service::workspace_state` | self or child |
| Request human approval, cancel own request | `create_human_requests`, `cancel_human_request` | self or child |
| Request a formal agent review (decision 4) | `ApprovalReviewService::admit`, with the UI handler's commit-acknowledgement recovery | self or child |
| List, close, refresh G5 PRs | `list_workspace_pull_requests`, `close_workspace_pull_request`, `prepare_…/apply_prepared_workspace_pull_request_refresh` | list: readable; close and refresh: self or child |
| Close or reopen a linked issue (decision 5) | the UI's transition: `admit_person_write(PersonWrite::Transition)` + `update_issue_state` under the owner's connected GitHub account | own workspace's links |

**Still not reached:**

- recording an approval decision (`g5.security.agent-human-exceptions`);
- forking a chat into a child (the non-persona session fork has no request key, and its copy keeps the source's creator);
- previews, comments and settings, which come in a later increment.

**Ontology amended:** `g5.external-issues.transition` and `g5.external-issues.cli` now
admit the agent's transition, replacing the exclusion the owner lifted.

## Implementation

Implemented on this branch on 2026-10-22. The design above is current; these notes
record placement and the checks run.

- **Stop by user.** `OrgService::stop_member_workspaces`, migration
  `117_execution_hold.sql`, `POST /orgs/:orgId/members/:userId/stop-workspaces`
  (`WorkspacesWrite`, service rechecks self or admin), the self-readable shutdown
  progress with `blocker`, the agent's `stop-owner-workspaces` route on a system-pool
  owner scope (`agent_owner_system_db`), and the Members page action.
- **Scope.** `db::workspace_agent_scope`: `agent_owner_db`, `agent_target`,
  `agent_read_target`, `list_agent_children`.
- **Fork launch.** `WorkspaceVmService::launch_forked_workspace` with a `FirstOnly`
  start admission; the UI's `clone_workspace` uses it too.
- **Commands.** `g5d workspace list|children|show|chats|messages|timeline|fork|chat|
  start|stop|delete|merge|stop-all`; agent guidance in `.agents/AGENTS.md`.
- **Static checks.** The route-auth, selected-org, system-scope binding, route SQL and
  layering audits pass with explicit contracts for the new routes.

### Increment 2

Implemented on this branch on 2026-10-22, after the owner's decisions.

- **Routes.** New `routes/workspace_agent.rs` routes cover child chat controls (stop,
  background end, rename, queued-input edit/withdraw/redeliver), task
  continue/cancel, approvals, PR list/close/refresh and `issue-state`. `issue-state`
  takes the issue reference that `g5d issue unlink` takes, the primary by default,
  instead of a link id.
- **Shared handler bodies.** The UI handlers `get_approvals_state`,
  `create_human_requests`, `create_review` (its commit-acknowledgement recovery
  included), `cancel_human_request`, `refresh_workspace_pull_request`,
  `close_workspace_pull_request` and `change_linked_issue_state` now delegate to
  `*_with` / `*_as` functions that the agent routes call. `PullRequestMerger` names
  the caller for merge, close and refresh.
- **Commands.** `g5d workspace inputs|input|stop-turn|end-wait|rename|task|approvals|
  request-approval|request-review|cancel-approval|prs|pr-close|pr-refresh` and
  `g5d issue close|reopen`.
- **Tests.** In `workspace_agent_parity.rs`:
  - `an_agent_controls_its_childs_chats_but_never_its_own`;
  - `approvals_are_read_for_self_and_children_only`;
  - `issue_state_needs_a_link_of_the_agents_own_workspace`;
  - PR list, refresh and close checks inside the merge test.
- **Not exercised.** A requested human approval and a formal agent review are not
  driven end to end (they need approval definitions and a review PR), and nor is a
  live GitHub issue transition. The shared UI suites cover those bodies.

## Increment 3: managed preview

The owner asked to "implement preview parity" on 2026-10-22. The UI's managed
preview runs only for the workspace's owner on a running VM. The agent gets the same
operations for its own workspace or a direct child. Preview is an application
service, so `SelfOrChild` applies, not the child-only chat rule
([`g5.preview.agent-control`](../../ontology/software_architecture/workspace_vm.g5.md#g5.preview.agent-control)).

| UI action | Agent route (`/api/internal/workspace-agent/:id/…`) | Reused operation |
| --- | --- | --- |
| Discover applications | `GET workspaces/:t/repos/:r/preview-applications` | `TaskService::list_live_preview_applications` (live checkout) |
| Read the current preview | `GET workspaces/:t/preview` | `current_workspace_preview` |
| Start a preview | `POST workspaces/:t/preview` (`agent:` key) | `start_workspace_preview` (exclusivity, digest-matched replay) |
| Close the preview | `POST workspaces/:t/preview/cancel` | `cancel_preview_with`, shared with the UI route |
| List and save profiles | `GET` / `POST workspaces/:t/repos/:r/preview-profiles` | `list_preview_profiles`, `create_preview_profile_with`, shared with the UI route |
| Open the preview in a browser | none | the handoff is a person's browser ticket |

**Browser access.** The preview URL is a single-use browser handoff that only a
person session may mint (`g5.security.credential-classes`,
`g5.workspace.preview-origin-security`).

- **Own preview:** the agent reaches it on `127.0.0.1:<port>` in its own guest.
- **A child's preview:** its loopback is private to the child's VM, so the only way
  in is the relay. The owner chose a non-browser, task-bound preview credential
  (decision 6); see [Increment 7](#increment-7-agent-preview-credential).

**Seen but not changed here.** `preview_session_mint` accepts any `AuthContext::User`
with `TasksRead`. That includes an organization API key, although
`g5.security.credential-classes` reserves minting browser tickets for a person
session. This is a pre-existing gap outside this increment.

**Implemented.**

- **Routes and commands:** the routes above, and `g5d workspace
  preview|preview-apps|preview-start|preview-close|preview-profiles|preview-profile-save`.
  `preview-apps` without `--repo` asks every repository of the workspace.
- **Test:** `an_agent_runs_the_managed_preview_of_its_workspace_and_children` covers:
  - discovery, start, keyed replay and the second-start refusal;
  - state, then close (the task is cancelled);
  - a stopped child's refusal for its VM, not for scope;
  - another owner's workspace refused.

## Increment 4: audit and files

On 2026-10-23 the owner asked to audit refused and failed agent attempts as well as
successes, to add the missing PR-refresh event, and to give the agent the UI's file
routes, audited ([request](../../chat_intent/issue-710-workspace-agent-parity/request.md#decisions-in-this-workspace)).

**Audit** ([`g5.security.agent-audit`](../../ontology/concepts/security.g5.md#g5.security.agent-audit)).
This amends the attribution rule in [Agent scope](#agent-scope): a route layer on the
workspace-agent router emits one `workspace_agent.action` event per mutating request
after the handler returns, with the route template, request path, HTTP status and
outcome class (`success`, `client_error`, `server_error`) the person's
`user_action.http` event uses. Inside a routed request, `audit` records the
operation, target and domain result in a task-local note for that event, and `note_file`
records a file's repository and paths before the operation runs, so a refused
attempt names the file. A request refused before its handler (malformed body) still
carries the route and the target from the path. Reads stay unaudited, as for people.
The stored agent-origin marker stays deferred to owner scope.

**Files.** Same operations as the editor and Files panel; the UI handlers now delegate
to `*_as` bodies that take an admitted scope and the actor.

| UI action | Agent route (`/api/internal/workspace-agent/:id/…`) | Scope | Reused operation |
| --- | --- | --- | --- |
| List files | `GET workspaces/:t/files?repoId&path&depth` | read | `files::read_file_tree_as` |
| Open a file | `GET workspaces/:t/files/content?repoId&path` | read | `read_workspace_file_as` |
| Save (optionally commit and push) | `POST workspaces/:t/files/write` | self or child | `write_workspace_file_and_maybe_push_as` |
| Delete | `POST workspaces/:t/files/delete` | self or child | `delete_workspace_file_as` |
| Rename or move | `POST workspaces/:t/files/rename` | self or child | `rename_workspace_file_as` |

Reads follow [`g5.security.agent-read-scope`](../../ontology/concepts/security.g5.md#g5.security.agent-read-scope):
the owner's running checkout live, anything else from the committed cut, as a
browser would get it. Writes need the target running under the owner, as in the
editor; the file-write policy (owner, mutable, not Base, repository member) and the
guest's hash checks are unchanged. Writing into its own workspace is allowed by the
general scope rule, though the agent normally edits its checkout directly. File
paths travel in the query or body, not a wildcard segment, so the middleware's
path allowlist stays exact. `g5d workspace files|file|file-write|file-delete|file-rename`
send each write once: a repeat is refused (the file exists or its hash moved),
never applied twice.

**Implemented.** Tests `every_mutating_agent_request_is_audited_whatever_its_outcome`
and `an_agent_edits_files_in_its_running_child_and_reads_what_its_owner_can`, plus
refresh and refused-close audit checks in the pull-request test. The runtime test peer
has no `fs.rename`, so a successful rename is covered only by the shared UI body.

## Increment 5: runnable launches

On 2026-10-24 a preview run showed a parent agent forking a child, choosing
`--permission-mode acceptEdits` for it, and every child turn failing in under a second.
The API accepts any catalog mode when a message is queued, but `agent.turn` admission
(`permission_policy`) runs only `default` and `bypassPermissions` for Claude and the
`never/…` modes for Codex. The agent saw only `failed`, because no read exposed a
task's outcome. The owner asked for three repairs.

- **Refuse before queuing.** `agent_dispatch::agent_turn_permission_modes` names the
  runnable modes; a unit test holds it equal to what admission accepts. The fork
  route (when it carries a prompt) and the new-chat route resolve the launch the chat
  would get, through `ConfigService::resolve_launch` on the target (a fork's child
  copies the source's configuration), and refuse with 422
  `unsupported_agent_turn_permission_mode` before anything is created. A launch that
  does not resolve is left to the shared path, which reports it as before. A
  follow-up checks the chat's fixed mode, or the one it names when the chat has none,
  and names a new chat as the remedy. The check is on the agent routes only:
  the UI composer offers `acceptEdits` and `plan`, and Codex's default chat mode is
  not runnable either, so refusing them in the shared queue path would change the
  UI's behaviour; that stays a recorded gap.
- **Say which modes run.** `--permission-mode` help lists them and notes that
  `default` waits for a person; `.agents/AGENTS.md` tells agents to give children
  `bypassPermissions` (or a `never/…` Codex mode) unless a runnable default is set.
- **Show why a task failed.** `TaskListEntry` gains `failure {reason, detail?}` from
  a failed task's outcome (text detail only). `g5d workspace show` prints it under
  the task, and the UI's task row shows it on hover. This is what the probe row
  above already promised ("with outcome").

Tests: `launches_no_agent_turn_can_run_are_refused_before_anything_is_queued`,
`agent_turn_permission_modes_are_exactly_the_modes_admission_accepts`,
`show_prints_why_a_task_failed`, and the `TaskRow` failure test.

## Increment 6: fork with an issue and member lookup

On 2026-10-26 the owner asked for two more agent gaps to be closed
([request](../../chat_intent/issue-710-workspace-agent-parity/request.md#decisions-in-this-workspace)):
forking a workspace with a primary issue from the command line, and looking up
users for assigning issues.

**Fork with an issue.** The owner offered `g5d workspace fork` or a `g5d issue link`
that reaches other workspaces. The fork is chosen, because the UI's fork already owns
this:

- `WorkspaceForkService` resolves the issue when the fork is admitted.
- It checks primary availability under the admission lock and freezes the issue on
  the fork row.
- `finalize_operation` links it in the transaction that publishes the child. A
  primary claimed meanwhile becomes related, and the outcome reports both roles.

The agent route passes `issue {ref, role}` into that same request and returns
`issueLink`. It adds no state and no second write. An issue that does not resolve, or
a primary that is already taken, refuses the fork before any child exists. A fork
that loses the primary or the feature while it runs completes as the outcome
reports, and a replay returns the same result.

A cross-workspace `issue link` would instead be a second operation after the fork.
It would need its own scope rule and retry story, and Create & Fork would not be one
step.

- **Command.** `g5d workspace fork … --issue REF [--issue-role primary|related]`. The
  role defaults to primary, as in the dialog. (`issue link` defaults to related
  because it adds to an existing workspace.)
- **Bare numbers.** A bare number names an issue in the creation domain, as `issue
  link` does. The shared `issue_creations::creation_reference` now serves both
  routes.
- **Not added.** Linking, unlinking or transitioning issues on an existing child
  stays self-only. Fork-time linking covers the request.

**Member lookup**
([`g5.organization.agent-member-directory`](../../ontology/features/organizations.g5.md#g5.organization.agent-member-directory)).

- **Route.** `GET /api/internal/workspace-agent/:id/users?q&user&includeDeactivated&limit`.
  - It is admitted like other reads: the caller workspace's own token, with
    `agent_owner_db` rechecking the current incarnation and the owner's active
    membership.
  - It composes the Members page's `OrgService::list_members` with
    `github_user_grants::connected_logins`, which owns the connected accounts. The
    composition sits in the route because identity services must not depend on git
    services (`service_context_layering.rs`).
- **Words.** `q` words must each match a name, email or login (case-insensitive
  substring).
- **Exact lookup.** `user` is exact: `@me` (the owner), a user id, an email or a login.
- **Deactivated members.** They are hidden unless asked for. Their history (issues,
  comments) remains, so naming them is still meaningful.
- **Accounts.** Each row carries `accounts: [{provider, login}]` rather than a GitHub
  field, as the owner asked. Today the only source is the member's GitHub App user
  grant, which is account-wide, not per organization.
- **Command.** `g5d user search [WORDS…] [--include-deactivated] [-L N]` and `g5d user
  view USER`. The help says which value each issue domain takes as an assignee: the
  email for G5 issues, the GitHub login for GitHub.
- **JSON** (owner request, 2026-10-26). `--json [FIELDS]` follows gh, like `g5d issue`:
  - `view` prints the member as an object; `search` prints an array, and says on
    stderr when more matched.
  - `--json` alone prints G5's full record, which gains fields as G5 learns more about
    people. `--json a,b` keeps those fields, and an unknown field is refused with the
    list of available ones.
  - A word clap reads as the value of `--json` is put back as the member or a search
    word.
- **Audit.** Reads stay unaudited, like other agent reads.
- **Exposure.** The roster is what any member reads on the Members page. The
  connected login is new to other members; the owner accepted that. No UI change.

**Implemented.**

- **Fork route.** `fork_with_prompt` takes `issue` and answers `issueLink`.
- **Lookup.** `lookup_users` and `select_users` in `routes/workspace_agent.rs`, plus
  `github_user_grants::connected_logins`.
- **Middleware.** `GET …/users` is on the incarnation allowlist.
- **Commands.** `g5d workspace fork --issue/--issue-role` and `g5d user search|view`
  (`cli/user.rs`). The guidance is in `.agents/AGENTS.md`.
- **Tests.**
  - `a_fork_links_its_issue_inside_the_fork_as_the_ui_fork_does`: bare number, primary
    default, replay, a taken primary refused before any child, related, an unknown
    issue, a bad role.
  - `an_agent_looks_up_its_organizations_members_with_their_connected_accounts`:
    roster, accounts, `me`, deactivated, words, exact names, another organization,
    limits, the own-path and stale-credential refusals.
  - The `g5d` plan and render unit tests.
- **Not covered.**
  - A fork whose primary is lost while the fork runs. The UI fork tests cover that
    shared finalization.
  - A GitHub-domain fork. It resolves through the same `resolve_reference`.

## Increment 7: agent preview credential

On 2026-10-26 the owner answered `agent-child-preview-access` with "Add a task-bound,
non-browser preview credential" and asked for it to be implemented
([`g5.preview.agent-access`](../../ontology/software_architecture/workspace_vm.g5.md#g5.preview.agent-access),
amending [`g5.security.credential-classes`](../../ontology/concepts/security.g5.md#g5.security.credential-classes)
and [`g5.workspace.preview-origin-security`](../../ontology/software_architecture/workspace_vm.g5.md#g5.workspace.preview-origin-security)).

**Owners.** No new state. The credential is a signed claim set. Every fact it relies on
is read again at each request from its existing owner:

- the caller incarnation (`current_incarnation`);
- the relation, owner and setting (`agent_target`);
- the current ready preview task (`TaskService::workspace_preview_target`, which the
  person's relay already uses).

The relay stays the only data path. Host translation, the isolated origin, response
sanitization, activity and the stream to the child's VM are unchanged.

**Mint.** `POST /api/internal/workspace-agent/:id/workspaces/:t/preview/credential`:

- Admission is `admit_self_or_child` with `workspaces:write`, so a child needs the
  setting, as for starting or closing its preview.
- It reads the target's current preview task from `workspace_vms.preview_intent`
  and requires it ready, like the person's browser mint. A missing or unready preview
  answers `PREVIEW_NOT_READY`; a deployment without a preview suffix answers
  `PREVIEW_ORIGIN_UNCONFIGURED`.
- `JwtMinter::mint_preview_agent` signs `PreviewAgentClaims`: the caller incarnation
  (org, workspace, owner, VM, epoch), the target workspace, the task, a
  `preview-agent:<task>` audience, `token_use: "preview_agent"` and ten minutes.
- The answer is `{taskId, previewUrl, header, credential, expiresAt}`.
- The route layer audits it as a mutating agent request (`g5.security.agent-audit`).

**Use.** The agent sends `X-G5-Preview-Credential: <credential>` to
`https://<task>.<suffix>/…`. Auth middleware recognizes the header only on a
host-translated preview path for the same task:

- More than one value, or any `Sec-Fetch-*` header, is refused with 401. Browsers
  always send fetch metadata, and page scripts cannot remove it, so a browser that
  somehow holds the credential cannot use it.
- An invalid credential is refused without falling back to the cookie or a bearer
  token, so the outcome never depends on which credential happened to win.
- A valid credential becomes a request-local `AuthContext::User` for the owner. It
  has the person ticket's read scopes, `token_use: "preview_agent"` and the
  `preview-agent:<task>` audience, so route scopes and owner RLS apply unchanged. The
  credential's claims travel beside it as `PreviewAgentContext`.

The relay (`routes/preview.rs::proxy`) then:

- refuses an agent request that has no context, arrived off the preview origin or is
  a WebSocket upgrade;
- rechecks the scope through `agent_owner_db`, `agent_target(SelfOrChild)` and
  `workspace_preview_target(task, ready)` in one owner transaction, and requires the
  task's workspace to be the credential's target;
- drops the header upstream, with the other platform credentials.

The person-only paths (sliding renewal and the `Origin: null` embed drop) key on
`token_use: "preview"`, so they never apply. The audit layer labels the request's
`auth_kind` `preview_agent`, not `jwt`. `validate` and the other preview validators
reject the new `token_use`, and it is listed as reserved, so an agent's credential
pasted into an app `Authorization` header is classified as a platform credential and
stripped.

**Command.** `g5d workspace preview-fetch [PATH] [--workspace WS] [-X METHOD] [-H
'Name: value']… [-d DATA | -d @FILE] [-i]`:

- It mints a credential for the target's current preview (this workspace by default),
  sends one request and prints the response body (`-i` adds the status line and
  headers).
- It does not follow redirects. An HTTP status of 400 or more exits 1 after printing
  the body.
- The credential is never printed: agent tool output is stored unredacted in the
  transcript, which other readers of the workspace can see, and the scrubber does
  not recognize JWTs.

**Choices** (agent; the owner chose only the credential class):

- **Self or child.** The agent's own preview is also reachable on loopback. Accepting
  self through the relay uses the ordinary scope, and tests the real origin.
- **Ten minutes, no renewal.** Each command mints its own, so no client has to
  refresh it; the per-request rechecks make expiry a backstop, not the revocation path.
- **No WebSocket.** A long-lived stream would outlive the per-request recheck of the
  parent's incarnation and setting. HTTP covers testing a page or an API.
- **Header only.** A cookie or URL parameter would let browsers carry it and leak it
  into history or logs.

**Acceptance.**

- A parent agent fetches a child's ready preview through the relay; the child's
  application does not receive the header.
- The following are refused: browser fetch metadata, a credential for another task,
  WebSocket upgrades, a closed preview, the setting off, an ended parent incarnation,
  and a target that is not self or a child.
- The mint is audited.
- g5d plans the two requests without printing the credential.

**Risk.** Any process in the parent's guest can already read the VM credential and mint
one (guest authority, decision 3). The credential adds no reach beyond the
self-or-child scope it rechecks.

## Increment 8: owner scope

On 2026-10-26 the owner instructed: "target workspace owner is the same as current
workspace owner", because a personal workspace has no children and would otherwise be
unable to manage the platform for its user
([record](../../chat_intent/issue-710-workspace-agent-parity/request.md#decisions-in-this-workspace)).
Decisions 1–3 already allowed deletion of any owned workspace, Base plus owned
workspaces as fork sources, and the existing VM credential.

**Rule** (`g5.security.agent-workspace-scope`). `agent_relation` admits a target that is
live, in the caller's organization, has the caller's current owner, and is not Base, a
formal review workspace or a review copy. The parent check is gone, so the relation is
`SelfWorkspace | Other`, and the needs are `SelfOrOther` and `OtherOnly`, the latter in
place of `ChildOnly` for chat mutations and delete. The setting check in `agent_target`
is unchanged: every non-self target needs it.

**Fork source.** The fork request takes an optional `sourceWorkspaceId`
(`g5d workspace fork --from ID`), defaulting to the caller. After the setting check:

- the caller itself, as before;
- the organization's Base, when the owner can read it (`agent_read_target`);
- otherwise `agent_target(source, SelfOrOther)`.

The fork service then refuses a personal source (`g5.personal.refusals`) and a review
workspace as for the UI. Launch validation reads the source's configuration, as the
fork copies it.

**Choices** (agent):

- **Every caller, not only personal ones.** The instruction names the current workspace
  without a kind, and one rule keeps the scope function simple. A fork's agent can
  therefore operate on its parent, siblings and the owner's personal workspace.
- **Base excluded as a target.** Base belongs to the organization: its administration
  is the [Base admin](../../ontology/features/workspaces.g5.md#g5.workspace.mutation-authority)
  path, stop by user already skips it, and an org owner who created it would otherwise
  give their agents Base operations. It stays a fork source (decision 2).
- **Review copies stay excluded**, as before, for any non-self target.
- **The personal workspace is a target** like any owned workspace; its own refusals
  (no deletion, fork, publication) still apply in the reused operations.
- **Probe relation.** `self`, `child` (a direct fork of the caller) or `owned` when the
  agent may operate on the workspace, else `other`. `children` still lists direct
  same-owner children; `list` shows every readable workspace.

**Why this is sufficient.** The parent relation was never a lock-held fact: each reused
operation rechecks the current owner under its own row locks, so dropping the parent
check removes no recheck. Base is identified by `is_base`, which never changes.

**Risk.** Any process in the guest that reads the VM credential can now fork, chat in,
start, stop, delete and write files in every workspace of the owner, not only
children. Containment is the owner's setting (on by default), the per-request audit,
owner limits and stop by user. Decision 3 accepted the existing credential for owner
scope.

**Gaps.** Pull-request publish, merge, close and refresh still act on pull requests
into or out of the caller (publishing a child's into it); publishing or merging between
the owner's other workspaces needs a person or the source workspace's own agent.

**Tests.** `operations_reach_this_workspace_and_its_owners_other_workspaces` (unrelated
same-owner workspace and grandchild admitted; another owner's workspace, Base and
review workspaces refused); `a_personal_agent_forks_base_and_manages_its_owners_workspaces`
(fork of Base and of an owned workspace, a personal source refused, chat and stop in
an owned workspace, the setting off refusing a Base fork); CLI request planning for
`fork --from`.
