# Deactivate an organization member

An admin can disable another person's access to one organization while retaining
their membership, role, workspace ownership and history. Access closes when the
membership change commits. Workspace shutdown continues independently and cannot
re-enable access on failure. Reactivation is explicit and never starts compute.

Request: [human intent and interpretations](../../chat_intent/member-removal/request.md).
Design prepared against `52de4512` on the authorized `member-removal` branch.
Implementation and integration with main `4839e606` completed on 2026-10-17.
Published as [G5-Lean #622](https://github.com/G5Labs/G5-Lean/pull/622).

## Governing contracts

- [Deactivation](../../ontology/features/organizations.g5.md#g5.organization.deactivation),
  [retention](../../ontology/features/organizations.g5.md#g5.organization.deactivation.retention)
  and [reactivation](../../ontology/features/organizations.g5.md#g5.organization.reactivation)
  own the person-facing behavior.
- [Current authority](../../ontology/concepts/security.g5.md#g5.security.current-authority),
  [active membership](../../ontology/concepts/security.g5.md#g5.security.active-membership)
  and [existing streams](../../ontology/concepts/security.g5.md#g5.security.membership-streams)
  govern admission with every credential class.
- [Deactivation shutdown](../../ontology/software_architecture/workspace_vm.g5.md#g5.lifecycle.member-deactivation)
  and [its recovery](../../ontology/software_architecture/workspace_vm.g5.md#g5.lifecycle.member-deactivation.recovery)
  specialize the existing Stop and explicit Start contracts.
- [Transfer](../../ontology/features/workspaces.g5.md#g5.workspace.transfer)
  remains a separate operation with
  [stop-before-transfer and eligible-target guards](../../ontology/features/workspaces.g5.md#g5.workspace.transfer.formal).

## State and ownership

`OrgService` owns changes to membership activity. Add `deactivated_at` (null means
active), a monotonic `activity_revision`, and nullable `deactivation_cleanup_id` to
`org_memberships`. Existing rows start active. The cleanup identifier both records
unfinished shutdown and fences workers from an older cleanup cycle; it is not another
membership status. The revision prevents a delayed admin request from undoing a newer
decision. Keep the existing role and role-derived `owner_eligible_user_id` unchanged:
adding activity to that generated key would reproduce the foreign-key blocker.
Protect the `owner` role from deactivation in both the service and schema.

| Membership | Cleanup | Organization access | Workspace execution |
| --- | --- | --- | --- |
| Active | None | Existing role and resource rules | Existing explicit Start and owner rules |
| Inactive | Pending or complete | Denied | No admission; outstanding shutdown retries |
| Reactivated | Pending | Restored within existing permissions | Blocked until cleanup completes |
| Reactivated | Complete | Restored within existing permissions | Stopped; fresh deliberate Start required |

Deactivation locks and rechecks the actor and target in one transaction, requires a
current active owner/admin with `org:admin`, refuses self and protected-owner targets,
then sets the inactive timestamp and creates a cleanup ID if none is pending. A repeat
does not replace pending cleanup. Reactivation clears only the inactive timestamp;
it cannot clear cleanup, create Start consent, restore a cancelled Start, or dispatch
work. Each actual activity transition increments the revision. Opposing or stale
requests are serialized using the observed revision; missing targets and cross-tenant
requests retain existing refusal behavior. Removal must not delete a membership with
unfinished cleanup, even after its last eligible workspace has been transferred.
Preserve the existing audit stream with
actor, organization, target and outcome.

`PermissionService`, scoped database admission and `AuthzService` own the shared
active-membership predicate. `WorkspaceVmService` remains the sole shutdown owner;
`TaskService` retains task interruption/hold/settlement. Extend the existing lifecycle
reconciliation loop to discover pending cleanup, including on reactivated memberships.
Do not add a second job executor or use the unrelated platform-issue outbox.

## Why a durable cleanup marker is needed

| Candidate | Guarantee or limit |
| --- | --- |
| Delete membership, demote the role, or disable the global user | Violates retained references, role or organization isolation. |
| Inactive flag plus a best-effort stop loop | A process crash can lose shutdown; reactivation can hide work from an inactive-member sweep. |
| Atomically stage every owned workspace's full stop | Can work with a bounded ownership inventory, but couples access revocation to all workspace/task locks. No owned-workspace limit is established here, and the current stop operation opens and commits its own transaction. |
| Membership cleanup ID plus existing per-workspace stops | Chosen: constant-size deactivation write, durable discovery after crashes, no lost cleanup on reactivation, and existing stop claims/receipts do the execution. |

The extra marker supplies the missing guarantee that pending revocation cleanup
survives reactivation without an unbounded atomic fan-out. Scanning uses indexed,
bounded pages and excludes already processed stopped rows; an unreachable workspace
must not monopolize the first page or starve another membership. Use the existing
16-item lifecycle batch as a starting bound, with bounded stop admission and the
existing independently claimed stop executors. The current two-second loop interval
is not a shutdown SLA: work volume, save time and host reachability affect completion.
Admission denial never waits for that scan.

## Authorization through every path

Filter by active membership in capability minting and organization enumeration, then
recheck within the transaction that consumes authority. Extend
`authz.lock_user_scope_authority` and the runtime authority lookup, preserving their
restricted definer boundaries and existing lock guarantees. A cached RLS capability,
`ScopedDbReopener`, JWT role, scope grant or previously resolved runtime handle is not
fresh authority. Organization tenancy through RLS alone does not establish activity.

Apply this to normal HTTP and service entry points; current-owner and admin helpers;
API-key use and issuance; OAuth organization selection; refresh/switch; CLI approval
and exchange; Git transport and provider credential resolution; private and Base
workspace access; browser tickets; and user-attributed background publication. Grant
and membership lookups used only to display historical people must still return inactive
identities. Invitations, role edits and login cannot reactivate an existing membership.

Account sign-in retains the existing membership-based allowlist exception, including
retained inactive memberships; this admits only the global account and does not grant
organization access. Return only active organizations as usable destinations. An
implicit person-session refresh can select another active organization, or keep an
account-only session when none remain. An explicit request for the inactive
organization is refused. Keep global
identity, refresh credentials and access to other organizations intact. An organization
API key cannot switch organizations. Do not delete or permanently revoke keys as part
of deactivation: otherwise-valid credentials may be used after explicit reactivation,
subject to their original scope, expiry and independent revocation.

Existing streams need independent enforcement beyond login and subscribe-time checks.
The implementation rechecks membership at delivery, including the resource organization
for account-wide notifications.
Reauthorize each protected inbound operation and bounded outbound delivery batch,
including terminal traffic, preview HTTP bodies/upgrades, logs, filesystem watches
and event subscriptions. Carry the resource's organization through user-channel
notifications too, so account-wide feeds cannot disclose the disabled organization's
data. Drop buffered protected data on denial and close the affected stream; retain
other organizations' independent access. Capture the activity revision for subscriptions
so a deactivate/reactivate cycle cannot silently validate the old subscription again.

Use current database authority at delivery admission, with bounded batches/timeouts
and no positive time-based activity cache. Never hold a transaction for the entire
connection. An invalidation event can accelerate closing but cannot be the only
enforcement across API replicas. Database unavailability stops protected forwarding.
This adds database work proportional to active delivery batches; batching ready data
is permitted, granting a future delivery lease is not. Data already delivered and finite
effects admitted before the deactivation commit cannot be recalled.

## Shutdown and concurrency

Runtime admission requires both an active owner membership and no pending cleanup
for that owner in the workspace's organization. Enforce it in Start/Resume and compound
starts, capacity/start reconciliation and rollout continuation, task reservation and
final invocation, new execution-producing submissions, steering/native-answer
continuation, runtime RPC and live relay.
Creation/fork publication and transfer acquisition require active, role-eligible
membership at their commit. A scheduler's system database scope is not owner authority.
Database admission guards backstop workspace ownership acquisition and VM transitions into starting/running, including late ready/registration callbacks. Service checks retain useful refusal responses; the database guards cover competing writers through commit.
An already-sent ambiguous launch belongs to the shutdown obligation and is never replayed
as permission to start new work.

For each cleanup ID, the lifecycle owner visits that member's owned workspaces and
performs its own transaction-bound stop admission. Under the existing workspace lock
order, recheck the same organization, current owner and cleanup ID before changing
anything. These checks serialize with membership and ownership changes through commit;
do not perform a separate unlocked check followed by an unconditional stop.

- Cancel prior Start intents even for stopped/failed VMs waiting for capacity, hold
  queued work, and fence old lifecycle consent. Already stopped or never-started
  workspaces need no VM launch. Base has no VM; do not attempt its prohibited Stop.
  Include unresolved custody under deletion and join its existing teardown rather
  than treating deleted metadata as proof that execution ended.
- For starting/running compute, use a distinct `member_deactivated` stop reason with
  immediate admission closure, existing interruption and a two-minute save window;
  there is no advance activity grace period. Save failure cannot reopen access.
- Join a stop already in progress instead of replacing its operation or extending
  its save deadline. Expedite a scheduled-but-not-yet-admitted stop into the same
  immediate closure. Per-workspace stop state remains the owner of retry/claim/receipt
  handling, including request cancellation and API restart.
- Only trusted, incarnation-bound cleanup may cancel, flush, settle outcomes and
  request host termination while the member is inactive. This authority cannot be
  used for owner commands, arbitrary guest work, provider publication or new execution.
  Audit cleanup consumers such as internal Git flush callbacks separately from normal
  user Git access so denying the owner does not prevent bounded saving.
- Clear the cleanup ID only after a fresh, transaction-protected check finds no
  outstanding runtime/launch, executable old consent or unprocessed queued-work hold
  for the affected owner. Unconfirmed termination remains pending. Compare the same
  cleanup ID at completion; old workers and late VM results cannot affect a later
  cleanup cycle or replacement VM.

Do not acquire all workspace locks while holding the membership mutation lock. Normal
workspace admission retains its workspace/commit-fence ordering and holds current
membership authority through the relevant database or bounded dispatch admission.
Cleanup completion locks the membership and checks remaining obligations without
taking workspace locks; competing admissions must lock and recheck that membership.
Failed admission records a durable workspace retry time in a separate transaction,
fenced by the current owner and membership cleanup ID. Deferred work leaves the
bounded admission page; unattempted work precedes due retries so a failing batch
cannot starve later workspaces or members. Successful admission clears its retry
state in the same transaction. A new cleanup cycle ignores old retry state.
A concurrent uncommitted stop therefore leaves completion pending, while no fresh
execution can appear between the final check and clearing the marker. Mutations of
multiple memberships lock them in a deterministic order; retain bounded deadlock/
serialization retry where existing surrounding operations require it.

Host connectivity and exit confirmation are necessary for physical shutdown. Logical
denial remains effective during a partition; do not report a VM stopped just because
its lease expired. This preserves the existing
[physical termination question](../../ontology/software_architecture/workspace_vm.g5.md#g5.vm.physical-uniqueness-question)
and makes no new promise to kill an unreachable guest instantly.

## Transfer and presentation

Reuse the existing separate transfer endpoint and lifecycle. Active owner/admins may
transfer an eligible ordinary, nondeleted, non-review, non-Base workspace from an
inactive owner to another active, owner-eligible organization member. The protected
organization-owner role is not transferred. Preserve stop/purge before ownership
commit, held work and explicit Start for the recipient. Recheck and hold both the
initiator's authority and the recipient's activity/role at final ownership commit,
including a resumed transfer. The retained foreign key checks role eligibility only;
it cannot prevent transfer to an inactive recipient. A transfer already completed
must not cause a later old-owner cleanup worker to stop the recipient's VM.

Add explicit Deactivate and Reactivate actions to the members page, with an Active /
Inactive status separate from role. Keep inactive people visible in the administrative
roster and historical attribution. Deactivate explains the organization scope and that
owned workspaces will stop; Reactivate explains that compute stays stopped. Display
shutdown pending separately from access disabled, with affected workspace progress
available to admins. Keep Transfer separate and filter its recipients to active eligible
members. Existing removal and viewer-demotion blockers remain.

Use explicit `POST /orgs/:orgId/members/:userId/deactivate` and `/reactivate` operations
with an observed `activityRevision`, returning current member status and whether
shutdown remains pending. A repeated request for the current state is a no-op; an
outdated request that would change a newer state returns a conflict for refresh.
Successful deactivation acknowledges committed access denial, not completed physical
shutdown. Update typed responses, frontend schemas/hooks and organization selection.
On access denial the client clears that organization's live views and offers another
active organization or the account-only view. Do not log the person out globally.

## Implementation and release checks

Add a new migration; preserve existing migration checksums and all ownership/history
references. Extend activity-change invalidation through the existing access-generation
trigger, but do not treat that counter alone as enforcement: no runtime consumer was
found. Update role-only queries according to their authorization versus historical
purpose, rather than indiscriminately hiding inactive rows.

Release schema and all authorization/runtime/stream changes before enabling the new
actions. Old API or scheduler replicas that treat row existence as active must be
drained before deactivation can be used; mixed-version authorization is unsafe. A
rollback after deactivation requires retaining these guards or disabling protected
traffic, never silently clearing inactivity.

Before implementation is complete, exercise these behaviors through real PostgreSQL,
HTTP/stream interfaces and lifecycle workers, with the existing scripted host/guest
doubles where necessary:

- Admin targets admin/member/viewer; self, protected owner, inactive actor, wrong
  organization and insufficient role/scope are refused. Retained membership/role,
  workspace ownership, reader grants and historical references are unchanged.
- Tokens issued before deactivation, API keys, CLI approval/exchange, refresh/switch,
  direct service calls, reused database capabilities and captured runtime handles
  cannot access the organization. Another organization works throughout; invitation
  acceptance and role changes cannot reactivate the member.
- An already-open event/terminal/preview stream, including one on another API replica
  and a user-channel organization event, cannot admit protected traffic after
  deactivation. Loss of an invalidation event and database failure cannot bypass it.
- Running, starting, capacity-queued, stopped-with-queued-work and never-started
  workspaces settle correctly. API death immediately after membership commit, stopped
  reconciler/restart, failed flush, lost host reply and an unreachable host leave
  access disabled and cleanup recoverable. Cleanup still saves through its narrow
  service path. More than one batch progresses without starvation.
- Race deactivation with dispatch, explicit Start, fork publication, transfer, another
  administrator action and stream delivery using controlled barriers. Reactivate
  before cleanup runs and during stop retries: no old compute or consent returns.
  Run another deactivate/reactivate cycle and deliver stale worker/callback results.
  No workspace starts until cleanup settles and a new explicit Start is admitted.
- Transfer from an inactive owner succeeds only to an active eligible recipient;
  deactivate the initiator or recipient during drain and verify final commit refuses.
  Existing ownership purge, owner-only VM access, Base/review exclusions and ordinary
  Stop/Resume regressions remain covered. Exported service APIs cannot construct a
  lifecycle bypass for a user command.
- Browser coverage drives both actions, pending shutdown, stale-state conflict,
  disabled organization navigation and separate Transfer. Run affected Rust and
  frontend build/lint/tests and migration/route-authority regressions.

## Integration with current main

Integration retains main's owner-only workspace configuration, direct provider
transport and independent task model. Member inactivity remains an additional authorization and
execution-admission condition; removed Job APIs and records stay retired.

Event sockets retain main's 30-second read-authorization deadline, periodic renewal,
post-commit eviction and independent close signal. Membership activity is still checked
at every delivery with the captured revision; the general read-authorization lease
cannot cache activity or revive a stream after deactivate/reactivate. Protected data
waits must remain interruptible by closure and the authorization deadline.

Event delivery reuses the subscription's resolved organization under that read lease;
account-wide notifications still resolve their resource per event. An organization
first becoming available after connection setup captures its current active revision
on first delivery; an existing revision binding is never replaced. The selected
organization's activity check is reused only within the same delivery. Runtime streams
check membership revision and incarnation in one database statement. Git and archive
reads use buffers bounded at 256 KiB. None of these paths grants future activity
authority. Database forks use the same active-session admission as reopened scopes.

Browser access-loss recovery bypasses a stored JWT for the denied context and requests
a fresh server-selected organization or account-only token. Transient refresh failures
retry with exponential backoff from one second to thirty seconds; context changes,
sign-out and unmount cancel retries and fence late answers.
Late HTTP membership denials only start recovery while their original user and
organization remain selected, including a renewal of that same token context.
WebSocket access-loss notifications carry the user/organization captured before
ticket acquisition. Recovery ignores notifications for a different selected context;
ticket retries cannot silently switch that binding during an authentication refresh.

Approval definitions and new human approval requests require active recipients in
the same organization, held through assignment commit. Inactive identities remain
visible on existing definitions, requests and decisions; the editor excludes them
from new versions, and individual/bulk request controls cannot select them. Activity
is exposed separately from membership so deactivation does not label someone as
having left the organization or remove historical attribution.

Main owns migrations through 098. The unmerged membership migration is 099;
main's catalog remains unchanged. Migration 099 updates existing tenant policies
to evaluate active organization membership through statement-local InitPlans,
preserving policy roles, commands and isolation. Each subsequent statement still
rechecks membership, including scopes opened before deactivation.

## Implementation evidence

Implemented in migration 099, identity authorization and scoped database context,
workspace runtime/lifecycle, transfers, live relays, typed API contracts and member
controls. SQLx metadata tracks the changed queries. Historical roles, ownership keys,
workspace records and credentials remain retained.

Validation covers:

- PostgreSQL/HTTP integration regressions, including all 23
  [deactivation tests](../../../backend/g5-api/tests/api_member_activity.rs), existing
  organization authorization, OAuth callbacks, Stop/Resume, transfers, archive downloads,
  route authority, task isolation and bounded event revocation. Coverage includes old
  credentials and database scopes, real event/terminal/preview traffic, cross-organization
  isolation, newly joined memberships, unavailable authority, early reactivation, queued
  dispatch, stale Start consent, cleanup retries and transfer races. Fault injection
  reproduces admission starvation, then verifies fair progress across service recreation,
  early reactivation, stale retry generations and a failed stop handoff. Deleted
  workspaces remain in shutdown progress only while obligations remain.
- Fresh installation and the restricted 098-to-099 upgrade/replay in
  `membership_activity_upgrade_retains_owners_and_restores_definer_privileges` preserve
  grants, workspace data and RLS policy metadata. EXPLAIN ANALYZE verifies statement-local
  InitPlans; database tests retain cross-organization read/write isolation and next-statement
  revocation in an already-open scope. Historical migration tests use their deployed
  SQL session boundary and function ownership rather than current runtime admission.
- All 78 WebSocket unit tests and the full frontend suite. Focused browser regressions
  cover activity actions, shutdown progress, stale revisions, eligible transfer recipients,
  stored-token recovery, transient refresh failure, context-change cancellation and late
  HTTP denial.
- Rust offline compilation, affected-library Clippy, formatting, full TypeScript checking,
  Prettier, ESLint and the production frontend build. ESLint retains six existing warnings.
  Migration checksums and Git whitespace checks pass; SQLx query metadata is current.

Host/guest integration uses the existing scripted runtime peer over the real mux and
API transports; it does not qualify physical termination on production infrastructure.
No formal modeling or deep verification campaign was requested or executed. Release
requires draining old API/scheduler replicas as specified above.
