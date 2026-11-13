# Merge when ready

Request: [merge-when-ready](../../chat_intent/merge-when-ready/request.md).
Governing blocks: [`g5.git.pr-auto-merge`](../../ontology/software_architecture/git.g5.md#g5.git.pr-auto-merge)
and its [readiness](../../ontology/software_architecture/git.g5.md#g5.git.pr-auto-merge.readiness)
child, the story [`g5.story.merge-when-ready`](../../ontology/features/workspaces.g5.md#g5.story.merge-when-ready),
and the amended [human decisions](../../ontology/concepts/security.g5.md#g5.security.agent-human-exceptions).

## Why not "the agent merges itself into its parent"

Letting `PullRequestMerger::WorkspaceAgent` merge into the parent, with guidance to
do so only on explicit clearance, was rejected:

- The approvals gate does not guarantee a human. A target with no attached
  requirements admits any merge (`evaluate_merge_admission` over zero
  definitions); an agent-only definition or an enabled self-approval can satisfy
  requirements with no human either.
- "The user" is not identifiable from inside the VM: a child's chats are written by
  its parent agent, and the fix-CI loop reads CI logs, bot comments and issue text
  that can look like clearance.
- It would hand the VM credential the owner's merge authority over the parent,
  and over Base for an organization administrator, where provider-first merge is a
  real GitHub merge.

Merge when ready keeps the human decision on the server, per pull request, and
leaves the agent's authority unchanged.

## Who performs the merge

G5 merges, as GitHub's auto-merge does, rather than the agent calling merge once
armed. The agent-executed variant needs the agent's session alive at the moment
the PR turns green (background watches die with the session), and still widens
`WorkspaceAgent`'s target rule. A server pass needs no new agent authority and
works when nobody is watching.

The pass runs the UI's merge body, `merge_pull_request_as`, with a third merger,
`PullRequestMerger::WhenReady { user_id, org_id, pull_request_id }`, that acts as
the person who turned it on:

- `target_db` re-admits that person at every step exactly as the UI does for a
  session: owner of the target (`workspaces:write` + `ensure_workspace_owner`),
  otherwise organization administrator of a Base target (`base_admin_subject_org`
  + `require_org_admin_on_conn`, the token-scope test replaced by the minted
  `org:admin` scope). A refusal means the decision no longer has an author with
  authority, so G5 turns merge when ready off and records why.
- `source_read_db` gives that person `workspaces:read` on the source, as
  for a person session.
- `ensure_external_checks_merge_policy` treats this merger as if the organization
  required external checks.

Merge admission, recovery, provider-first merge, landing and the `merged` event
are therefore unchanged, and the merge is attributed to that person. The
provider merge already pins the admitted head (`merge_pull_request` sends `sha`).

## State

Nullable columns on `workspace_pull_requests` (migration 120), so RLS, grants
and cascade come with the row:

- `auto_merge_enabled_by`, `auto_merge_enabled_at`: the live decision; both
  null when off. Kept after the merge as the record of who decided.
- `auto_merge_reason`, `auto_merge_checked_at`: the latest reason it is still
  waiting (or why G5 turned it off) and when the pass last looked. This is an
  observation that cannot be derived later: the refusal of a merge attempt is
  only known by attempting it.
- `auto_merge_observed_heads`, `auto_merge_observed_since`: the provider head of
  each repository as last read from GitHub, and since when (the settle window).
- `auto_merge_attempt_key`, `auto_merge_attempted_at`: the heads a merge attempt
  that did not land ran against, for the retry rule.

Turning it on or off appends a pull-request event, `auto_merge_enabled` or
`auto_merge_disabled` (message: the reason for an automatic or agent turn-off),
shown in Activity. A separate table was rejected: its lifecycle would duplicate
the PR row's (org, RLS, cascade) with no independent identity.

## Writers

| Who | Route | Rule |
|---|---|---|
| Person with merge authority over the target | `POST /api/v1/workspaces/:target/pull-requests/:pr/auto-merge` | Open PR only. Records the person, clears the reason and progress columns, appends `auto_merge_enabled`. Turning on again by the same person is a no-op; by another person it records the new author. |
| That authority or the source workspace's owner | `DELETE` on the same path | Clears the decision, appends `auto_merge_disabled`. Already off is a no-op. |
| The source workspace's agent | `POST /api/internal/workspace-agent/:self/pull-requests/:pr/auto-merge/disable` | The PR's source must be the caller's own workspace. Acts as the owner; audited by the agent audit layer. There is no agent route that turns it on, and VM tokens are refused on `/api/v1`. |
| The pass | — | Writes only the progress columns, or turns it off when the author lost authority. |

The pass and the routes both write under `WHERE status = 'open'`; a PR closed
or merged meanwhile is simply skipped.

## Readiness

Each pass (30 s, one scan owner across replicas via an advisory lock, as the PR
refresh scan) visits open PRs with merge when ready on, in order:

1. Admit the author as the merger (turn off on refusal).
2. If the PR has provider repositories, read checks with the author's read
   scope. Failing or pending waits with that reason. Passing records the
   `headRefOid` vector; a vector different from the observed one restarts the
   two-minute settle window. A repository with no reported checks counts as
   passing in the shared verdict, which is why the window exists.
3. Evaluate merge admission lock-free against the source's committed cut
   (`read_committed_cut` + `evaluate_merge_admission`); not admitted waits with
   its reason. This makes newly granted approvals take effect on the next pass.
4. Compute the attempt key from the source cut, the target cut and the observed
   provider heads. If the last attempt that did not land used the same key less
   than ten minutes ago, keep waiting with its reason. A blocked preflight
   writes a `merge_blocked` event, so this bounds Activity noise to six entries
   an hour for an unchanged blocked PR.
5. Run `merge_pull_request_as`. A refusal records its message and the attempt
   key; a merge clears nothing (the PR is no longer open).

Capacity: each armed PR costs one checks read per repository per pass, using the
GitHub App installation token; tens of armed PRs per deployment stay well inside
the App's rate limit. The checks observation can still change between the read
and provider acceptance, the same limit as the organization policy; GitHub
branch protection independently governs acceptance.

## Surfaces

- PR summary DTO: `autoMergeRequest: {enabledBy, enabledByName, enabledAt,
  mergeMethod: "REBASE", reason, checkedAt} | null` (null while off), named after
  gh's field.
- `g5d pr merge [PR] --auto | --disable-auto`; PR defaults to this workspace's
  latest outgoing G5 PR as in `view`. A person login calls the `/api/v1`
  routes. Inside a VM, `--auto` is refused locally with the reason (ask the
  owner) and `--disable-auto` calls the agent route. A bare `g5d pr merge [PR]`
  merges now through the existing person or agent merge route, so the agent form
  still only merges pull requests into its own workspace. `--match-head-commit`,
  `--admin` and the merge-method flags are not offered.
- Prepare to Ship: next to Merge, "Merge when ready" when the PR is open and
  merge when ready is off; when on, who turned it on, the latest waiting reason
  and "Turn off".
- Agent guidance in `.agents/AGENTS.md` and `AGENTS.md`: never merge into the
  parent yourself; ask the owner to turn on merge when ready, then keep CI green
  and the branch rebased; `g5d pr view --json autoMergeRequest` shows why it
  waits; turn it off if the change should not land.

## Merge with agent

Governing block: [`g5.git.pr-auto-merge.with-agent`](../../ontology/software_architecture/git.g5.md#g5.git.pr-auto-merge.with-agent).

The person route that turns merge when ready on takes an optional
`{"agent": {"agentLaunch", "requestKey"}}`. With it, one request:

1. admits the person on the target (as without an agent) and turns merge when
   ready on in that transaction, uncommitted;
2. admits them on the source as the chat start does (`workspaces:write`,
   mutable, and the source's owner, since only the owner's agent runs its
   chats), and queues a new chat through `WorkspaceMessenger::send` with the
   server's prompt, the person's launch choice and request key; commits it;
3. commits the decision.

A refusal in 1 or 2 leaves both unchanged. Should the last commit fail after the
chat was queued, the agent's first read shows merge when ready off and it stops,
which is the loop's own end condition. The prompt is server text, like
base-conflict resolution's, so `g5d` and the UI agree and a test can read it. It
names the pull request and target and describes the loop: read
`g5d pr view <id> --json state,localMergeStatus,blockingMessage,autoMergeRequest,targetHeads`;
stop when merged or turned off; when not rebase-mergeable, rebase onto the
`targetHeads` commits and publish the rewrite with `git push --force-with-lease`
against the head fetched before rebasing (origin is G5, whose pull-request
refresh forwards a rewrite to the provider with its own lease; `g5d pr update`
refuses rewritten history); when checks fail, fix, commit and `g5d pr update`;
otherwise wait with bounded `g5d pr checks --watch`. The owner replaced the first
prompt, which paired rebasing with `g5d pr update` (2026-10-25). It never
merges and never turns merge when ready on.

Prepare to Ship ([`g5.workbench.ship-to-parent`](../../ontology/features/ui.g5.md#g5.workbench.ship-to-parent),
2026-10-26) has three controls. **Ship to parent** merges at once and is disabled,
naming the blocker, while the dialog's own gate (admission refused, branch not
ready, required checks failing or pending) or a refusal from the merge itself holds
it. **Ship with agent** is always beside it once a pull request is open, using the
dialog's agent choice and opening the started chat; it is secondary on the all-green
path and primary when blocked. The **merge when ready** switch turns the decision on
or off without an agent. Starting another agent while merge when ready is on goes
through Ship with agent and keeps the existing decision.

## Announcements

Governing block: [`g5.git.pr-merge-announcement`](../../ontology/software_architecture/git.g5.md#g5.git.pr-merge-announcement).

`merge_pull_request_as` announces every merge, clicked or by the pass, after it
lands; the pass announces its own turn-off and the person/agent routes theirs.
One helper, under the organization's maintenance scope in its own transaction:

- publishes `workspace.pull_request_updated` to both workspaces (the PR list,
  Prepare to Ship and, newly, Activity refetch on it);
- notifies, category `pull_request`, the person merge when ready merged as and
  the source owner, minus the person who acted (a clicked merge or turn-off);
  the UI keeps these until read, like review notifications;
- appends a platform notice (`platform_notice.kind = pull_request`, keyed by
  the pull request and outcome so it is added once) to the source
  workspace's chats with queued, running or waiting agent tasks, through the
  same allocation as stop notices.

An announcement is best effort after the merge committed: a failure is logged
and never reported as a failed merge. The agent's own loop reads remain the
authority for it.

## Implementation

- State and writes: migration `120_pull_request_auto_merge.sql`;
  `services/git/workspace_pull_request_auto_merge.rs` (enable, disable by side,
  the pass's progress and turn-off, all under `status = 'open'` with the row
  locked); the summary's `autoMergeRequest` column in
  `coordinated_repo_publication.rs`; Activity labels for the two new events.
- Merger and routes: `PullRequestMerger::WhenReady` and the shared
  `admit_base_admin` in `routes/workspaces.rs`, with the person routes beside
  merge/close; the agent's `disable_auto_merge` in `routes/workspace_agent.rs`,
  allowlisted for incarnation tokens in `auth_middleware.rs` (the turn-on path
  is not, so a VM token cannot reach it even by mistake).
- Pass: `g5-api-routes/src/auto_merge.rs`, spawned in `g5-api` `main.rs`. It
  lives beside the routes because it runs the route-level merge body.
- `g5d pr merge` in `g5d/src/cli/pr.rs`; `autoMergeRequest` in `pr_json.rs`.
- UI: `MergeWhenReady.tsx` in Prepare to Ship's Ship section.
- Guidance: `.agents/AGENTS.md` and `AGENTS.md`.
- Merge with agent: `EnableAutoMergeRequestDTO` and `start_merge_agent` beside
  the person routes; the prompt is `auto_merge::merge_with_agent_prompt`;
  `MergeWhenReady.tsx` takes the dialog's agent choice and opens the chat.
- Announcements: `auto_merge::announce`, called from `merge_pull_request_as`,
  the pass's turn-off and the person's turn-off; notices through
  `chat_service::append_platform_notice`, which stop notices now share;
  `pull_request` notifications persist in `useNotifications.ts`; Activity
  refetches on `workspace.pull_request_updated` (`ws.ts`).

## Evidence

- `backend/g5-api/tests/workspace_pull_request_auto_merge.rs` (live Postgres,
  real routes): waits for a required approval then merges as its author;
  refuses a non-owner member and the agent's credential (internal path and
  person route), lets the agent turn it off with an attributed event; turns
  itself off when the author loses target ownership; a conflicting attempt
  writes one `merge_blocked` event and is retried only after the interval; a
  ready local-only pull request merges on the next pass and its summary keeps
  `autoMergeRequest`; against a GitHub fake, failing checks wait without an
  attempt, passing checks wait out the settle window, a new provider head
  restarts it, and a settled head admits the attempt.
- `route_auth_static` pins the new handlers' guards, `admit_base_admin` and
  the `WhenReady` re-admission; `workspace_agent_parity` still passes.
- `PrepareToShipModal.test.tsx`: turning it on and off while Ship is blocked;
  no control without an open pull request.

- Announcements and Merge with agent (same file): a merge when ready merge
  notifies its author and leaves one `merged` notice in a chat with waiting
  agent work but none in an idle chat; a person's own turn-off and clicked
  merge notify nobody but still reach the working agent; G5's turn-off is a
  `warning` notification; Merge with agent turns it on and queues one loop
  chat in the source (a retry with the key returns it); a non-owner of the
  source changes nothing; starting an agent keeps a Base admin's decision.
  Stop-notice suites (`api_task_lifecycle`, `api_workspace_vms`) cover the
  shared notice insert. UI: `PrepareToShipModal.test.tsx` (agent choice sent,
  chat opened; a reader gets only the no-agent choice) and
  `ws.pullRequests.test.ts` (Activity refetch).

Gaps: no test lands a provider-first merge under `WhenReady` (the fake
refuses the provider merge); that path is the UI's merge body unchanged.
`--match-head-commit` is deferred by the human.
