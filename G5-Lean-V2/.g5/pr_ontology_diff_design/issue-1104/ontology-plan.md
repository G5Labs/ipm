# Members may use the normal shipping controls (#1104)

Owner decisions and open points: [request](../../chat_intent/issue-1104/request.md).

## Problem

A member who owns a source workspace cannot **Ship to parent**, **Merge when
ready** or **Ship with agent** into the parent, because all three ask one
authority — "may the caller merge this pull request into its target" — that
admits only the target's owner (or a Base administrator). Owning the source as
an ordinary member is not enough. The blocked-ship note also appends "Ship with
agent resolves it," which is false for a permission denial.

## Decision

Add one branch to that authority: the **current owner of the pull request's
source workspace**, as an active member, may merge into the target when the
target has **at least one attached approval requirement**
(`g5.governance.approval.requirements`). Reason (owner): attaching a requirement
is the act that grants which children may merge in. Everything else about the
merge is unchanged — approvals and every other gate still apply, it is credited
to the person, and it is never a merge-with-override or GitHub override.

## Owners and the one authority question

The invariant is **who may merge a pull request into target T**. The target
workspace owns its branch state (`g5.workspace.git-authority`,
`g5.workspace.mutation-authority`); the merge coordinator advances it only for a
caller holding merge authority over T. That authority is asked in exactly these
places, all of which get the new branch:

- Immediate **Ship to parent** — `merge_workspace_pull_request` →
  `PullRequestMerger::Person::target_db`.
- The **background merger** rechecking the enabler each pass —
  `PullRequestMerger::WhenReady::target_db` (a `Forbidden` result already means
  "turned off: can no longer merge", `g5.git.pr-auto-merge`).

Both reuse one core predicate, `source_owner_may_merge` (new, in
`workspace_mutation_policy`): the actor currently owns the PR's source workspace
(row-locked, not deleted, no pending transfer) **and** the target has an active
attached requirement (`approval_configs`). Composed from two helpers —
`is_source_owner_of_pull_request` and `target_has_attached_requirement` — so the
enable path can reuse the first alone (below).

The route helper `scoped_db_for_source_owner_merge` tries the existing
owner/Base-admin path first (`scoped_db_for_owner_or_base_admin`); on its
`Forbidden` it resolves the target's organization through a `workspaces:read`
scope and opens the member's write capability with
`scoped_db_for_user_actor_in_org` (a `workspaces:write` scope admits only the
target's owner; see [Evidence](#evidence)), admitting them iff
`source_owner_may_merge` holds, otherwise returning the original owner refusal.
Only the three merge surfaces call it; refresh/close keep the narrower
owner/Base-admin authority (the source side is only granted *merge*).

## Merge-time, not click-time, for the auto-merge decision (decision 3)

Turning **Merge when ready** on no longer asks merge authority; it asks only
that the caller is the source owner or already a target merger (a weaker gate
that still stops arbitrary members) — `scoped_db_for_source_owner_merge` without
the requirement check, which reuses `is_source_owner_of_pull_request` without the requirement check. The real
authority (including the requirement) is rechecked by the background merger
every pass; if it has lapsed — requirement retired, ownership lost, membership
gone, transfer pending — the merger turns the decision off, saying why, under
the existing `Outcome::Revoked` path. This is why the requirement is *not*
checked at enable: a decision taken before a requirement exists is legitimate
and simply waits (or is turned off) rather than being refused up front.

**Ship with agent** keeps its separate source-ownership check at click
(`start_merge_agent` → `ensure_workspace_owner` on the source), because the
agent runs only in its owner's workspace; the chat is queued before the decision
commits, so a refusal of either leaves neither a decision nor a chat
(`g5.git.pr-auto-merge.with-agent`). Agents still never turn it on.

## The misleading message (decision 4)

A permission denial returns the ordinary `Forbidden` ("Insufficient
permissions", `g5.ui.permission-denied-reason`/#1102 — the server does not
encode a specific 403 reason here). The frontend stops appending "Ship with
agent resolves it" whenever the merge error is a 403, and — using the approvals
section it already renders — explains the likely cause when the target has no
attached requirement ("members can ship only once the parent has an approval
requirement"). For a source owner a 403 on Ship to parent means exactly that
(with a requirement present they hold authority and the merge proceeds to report
real blockers instead), so the approvals-derived explanation is accurate without
a new backend error.

## Reuse of existing guarantees

- The merge body (`coordinated_repo_publication::merge_workspace_pull_request`
  and the apply/land path) re-opens its transaction only through `target_db`, so
  admitting the source owner there carries the whole merge; no inner call
  re-checks target ownership.
- RLS on `workspaces`/`approval_configs` is org isolation; application-level
  `ensure_workspace_owner` is the single-writer gate we are extending, not RLS,
  so a `workspaces:write` scope for a member admits the write once the new
  authority holds.
- Target liveness: the merge uses the target's committed/published cut and
  refuses on head drift exactly as a target owner's own merge does; the
  dirty-head gate is a pre-existing no-op stub, so source-owner merge is neither
  safer nor less safe than today's non-Base merge (open point 3).

## Assumptions kept explicit (raise with owner, do not decide silently)

- Administrators alone attach requirements, so an admin can enable child→parent
  merge on a workspace without its owner's agreement
  (`g5.workspace.approval-administration` unchanged).
- Any attached requirement counts — agent-only or self-approval-allowing
  definitions included — so a member could land work with nobody else involved.
- The dirty-target barrier is a no-op stub; a live target owner's unsaved work
  is protected only by head-drift refusal, as it is for the target owner's own
  merges.

## Tests

Backend route-auth and integration: member immediate-merge admitted with a
requirement and refused without; auto-merge enable admitted for the source owner
without a requirement; background merge completes for the authorized member and
revokes when the requirement is retired / ownership lost / transfer pending;
existing owner, Base-admin and agent cases still hold; Ship with agent still
needs source ownership. Frontend: the 403 permission path shows the accurate
message and never "Ship with agent resolves it", plus the three controls for a
source-owner member.

## Evidence

Backend (`g5-api` integration, live Postgres): `workspace_pull_request_auto_merge::{a_member_owning_the_source_merges_when_ready_once_its_requirement_is_met, merge_when_ready_turns_off_when_the_members_authority_lapses, a_member_ships_to_parent_only_with_a_requirement}` cover the member's auto-merge enable, background merge, revocation on requirement-retire, and immediate-merge authority with/without a requirement; `only_people_who_may_merge_turn_it_on_and_the_agent_only_turns_it_off` still refuses an unrelated member and the agent. Static route-auth (`route_auth_static::{route_secondary_identifiers_have_auth_manifest_entries, owner_or_base_admin_admission_keeps_owner_and_base_admin_checks}`) pin the new `scoped_db_for_source_owner_merge` wiring. Owner/Base-admin regression: `api_workspace_pull_requests::{org_admin_refreshes_and_merges_a_pull_request_into_base_they_do_not_own, pull_requests_into_base_refuse_callers_without_organization_administration, org_admin_cannot_act_on_pull_requests_into_a_workspace_that_is_not_base, local_ship_to_base_fast_forwards_and_retry_cannot_duplicate_completion}` and the full `workspace_merge_override` suite pass unchanged (33 auto-merge/override/static tests green).

Frontend: `PrepareToShipModal.test.tsx` — "shows a permission denial accurately and never claims Ship with agent resolves it (#1104)" exercises the 403 Ship-to-parent path; all 45 modal tests pass.

Key implementation fact: a member cannot open a `workspaces:write` scoped db on a workspace they do not own (the single-writer fence in `scoped_db_for_workspace`), so `scoped_db_for_source_owner_merge` resolves the target org through a read scope and opens the member's write capability with `scoped_db_for_user_actor_in_org` — the same org-scoped write path the Base administrator and the background merger already use.
