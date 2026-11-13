# Owner setting for agents to modify other workspaces

Request: [agent-other-workspaces](../../chat_intent/agent-other-workspaces/request.md).
Governing block: [`g5.security.agent-other-workspaces`](../../ontology/concepts/security.g5.md#g5.security.agent-other-workspaces),
refining [agent scope](../../ontology/concepts/security.g5.md#g5.security.agent-workspace-scope).

## Owner and state

The setting is one more typed `workspace_settings` column,
`agent_may_modify_other_workspaces boolean NOT NULL DEFAULT true` (migration 119).
Reusing the settings row gives the needed lifecycle for free: the owner-only
settings route ([configuration authority](../../ontology/features/workspaces.g5.md#g5.workspace.configuration-authority),
`ensure_workspace_owner` holds the workspace row `FOR SHARE`, so a change
serializes with transfer and is refused while one is pending), the fork copy and
RLS. A separate table or a grant record (for example "enabled by user X",
effective only while X owns the workspace) was rejected: the explicit clear in
transfer already ties the value to the owner, and a derived rule would leave a
stale "on" in the row for the UI to explain.

The owner first asked for an off default, then for personal workspaces to
default on, and finally (2026-10-26, before the branch shipped) for every
workspace to default on. Migration 119 was edited in place to `DEFAULT true`,
which also fills every existing row, and the separate personal-workspace
upgrade (former 121) was removed; no writer sets the value at creation any more.
Lookups that find no settings row treat it as on, matching the column.

## Writers

- **Owner** through `PUT /api/v1/workspaces/:id/settings`
  (`agentMayModifyOtherWorkspaces`) and the workspace settings page. Workspace
  incarnation tokens are only accepted on the `/api/internal/workspace-agent`
  allowlist, so the agent cannot reach this route with its VM credential.
- **Fork** (`workspace_fork_service` finalize): copies the source value when the
  fork's owner is the source's current owner; a fork for anyone else (a member
  forking Base or a colleague's workspace, a reader's review copy) takes the
  default, on; a formal review workspace gets `false`. Taking the default rather
  than `false` is what makes "on for all" hold for ordinary new workspaces,
  which are forks of a Base the member usually does not own.
- **Transfer** (`WorkspaceVmService::transfer` final transaction): sets it
  `false` in the same commit that changes `owner_user_id`, as originally
  instructed; with the on default this makes a transferred workspace the one
  ordinary case that starts off for its new owner. That statement is the
  only owner change, so no trigger is needed.

## Enforcement

One check at the existing chokepoint. `agent_target` (every agent operation's
target admission) refuses any target other than the caller's own workspace
unless the caller's setting is on, before it looks the target up, so the
refusal says nothing about the target. This covers child start, stop, delete,
chats and their controls, tasks, approvals, previews, child pull-request
publication (`PullRequestCaller::WorkspaceChild`) and any later child operation
that admits through `agent_target`. The fork route refuses before the fork
service runs, because its target admission is the caller itself.

The probe classified its relation with `agent_target`; it now uses the ungated
`agent_relation`, so reads are unaffected. Merging into the agent's own
workspace (`PullRequestMerger::WorkspaceAgent`), closing or refreshing its pull
requests, issue state and stop-all never admit another target and stay
unaffected. Stop-all is kept deliberately (see the request's interpretation).

The refusal is `G5Error::AgentOtherWorkspacesDisabled` → 403
`AGENT_OTHER_WORKSPACES_DISABLED`, with a message telling the agent the setting is
off and its owner must turn it on, so `g5d` prints an actionable error without a CLI
change. The setting is read in the request's admission transaction with the
caller incarnation already checked current; an owner turning it off affects new
requests only ([admitted work](../../ontology/concepts/security.g5.md#g5.security.agent-admitted-work)).

## Evidence

`backend/g5-api/tests/workspace_agent_parity.rs`: the default is on; refusals
with the setting off (fork, child operations, publish) while self operations,
reads and self-merge admission continue; turning it on admits them; same-owner
forks copy it (off included), other-owner forks take the default; transfer
clears it; the agent's credential cannot update settings.
`api_personal_workspaces.rs` checks a new personal workspace starts on.

Gap: under today's [agent scope](../../ontology/concepts/security.g5.md#g5.security.agent-workspace-scope)
a personal workspace can neither fork nor have children, so its "on" admits
nothing yet. It takes effect with the personal owner scope deferred in the
[G5-Lean#710 design](../issue-710-workspace-agent-parity/ontology-plan.md).
