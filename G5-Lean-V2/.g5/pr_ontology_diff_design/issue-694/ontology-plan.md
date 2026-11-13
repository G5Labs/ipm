# Workspace deletion and policy transitions

Source: [issue #694](https://github.com/G5Labs/G5-Lean/issues/694).
The owner requested design followed by implementation. Main contains the
implementation originally developed on `FVB-i694-N2-workspace-deletion-and-policy`.
PR #715 was developed independently on the authorized branch
`issue-455-security-medium-committed-file-content-endpoints`. This document
reconciles their compatible intent and retains the original verification below.

## Governing behavior

[Deletion admission](../../ontology/features/workspaces.g5.md#g5.workspace.delete.admission)
must reject live children, active merges and unfinished forks before any stop,
including force. [Owner authority](../../ontology/features/workspaces.g5.md#g5.workspace.mutation-authority)
and [transfer exclusion](../../ontology/features/workspaces.g5.md#g5.workspace.transfer.formal)
remain valid through each policy write.
[Policy transitions](../../ontology/features/workspaces.g5.md#g5.workspace.content-policy.transition)
preserve admitted work and support explicit recovery after a lost worker.

## Owners and decisions

WorkspaceService coordinates deletion: lock the repository set and workspace,
check current owner, transfer and removal eligibility, and retain admission
before asking WorkspaceVmService to stop. Reuse `deletion_operation_id` on the
live workspace as the reservation and retain it on the tombstone. Competing
child/fork/merge admission, transfer and VM start must serialize with this
reservation and refuse once it exists. Database admission guards cover sibling
producers, including internal workers. VM stop continues to own flush, force,
loss accounting and recovery; it does not decide deletion eligibility.
The admission trigger runs under the existing restricted authz lookup owner:
organization bootstrap can insert Base mappings but cannot itself lock workspace
rows. The trigger gets only the existing lookup/row-lock authority, while its
ordinary callers retain their current table privileges and RLS restrictions.
The directly callable deletion check remains an invoker function.
Direct clean merges also check the reservation while holding their application
locks, since they may record a completed run without an intermediate active row.
The legacy clone-merge endpoint applies the same destination check: an explicit
merge target can differ from the clone parent and therefore have no live-child
blocker. Its already-captured source cut remains a read input; the destination
write is the operation fenced through landing.
Stop admission checks the reservation identity under its own workspace lock so
an older deletion cannot stop a workspace on behalf of a superseding request.

A retry joins the retained deletion; force may supersede its stop identity.
Final removal rechecks authority and the exact reservation. Failure after
admission retains the reservation for retry, including process loss between
admission, stop and cleanup. A completed stop is never undone. The existing
live-child refusal remains; implementing the broader cascade intention is
outside this fix.

A check followed by stop without retained admission cannot exclude races.
Holding SQL locks across the existing VM stop deadlocks against its own short
transactions and couples database capacity to external RPC latency. The
workspace's existing operation field supplies the missing durable exclusion;
no independent workflow or reconciliation owner is needed.

For content policy, persist a fresh transition UUID with `freezing`, and pass
it to synchronous completion, the background drain and timeout cleanup. Every
policy write locks the workspace first, then rechecks active current ownership,
Base/formal-review restrictions and pending transfer in fresh statements.
Completion and timeout compare the exact UUID; cancellation clears it. An old
worker that sees another identity exits without mutation or a policy event.
The UI keeps its existing `freezing` marker; the UUID is internal identity.
Transfer admission replaces a pending drain's identity, so a former owner's
worker stays invalid even if ownership returns to the same actor. The visible
marker remains recoverable through Restore editing.
Repeated requests join the existing transition. Legacy/stranded markers remain
recoverable by the current owner's restore-editing operation; automatic drain
resumption after restart is still not promised.

## Validation

Exercise real HTTP/service boundaries and PostgreSQL: normal and force deletion
refusal with a running VM and pending task; both sides of competing eligibility
admission; retained deletion retry and successful cleanup; policy ownership
change/pending transfer during drain; cancellation A, admission B, then stale A
completion and timeout; retry and stranded-transition recovery; valid current
owner completion. Check affected fork/merge/transfer/VM consumers and migration
compatibility. Run formatting, compilation, focused integration/system tests,
then publish and monitor the final head's CI and required review findings.

## Design review

The reservation closes admission before runtime effects and survives the stop's
separate transactions. Policy identity and fresh ownership checks address
independent races: neither substitutes for the other. Post-lock queries avoid
using a statement snapshot taken before waiting for the workspace lock. No
TLA/TLC work was requested. Implementation is authorized by the task request.

## Implementation evidence

Base integration (2026-10-22): issue #660's current-authority holds, viewer
ceiling and credential rules are compatible with this design. Preserve those
contracts alongside the workspace owner/membership checks and deletion guards.
Its published refresh-token migration owns version 105; renumber this branch's
unmerged admission and trigger-authority migrations to 106 and 107, preserving
their SQL bytes. This uses the feature-branch exception in
[migration immutability](../../ontology/concepts/system_invariants.g5.md#g5.invariant.migrations).
Validate the combined history and affected credential/owner consumers before
publishing the merge.
The subsequent issue #626 host-progress changes preserve custody until exit
proof and keep unrelated commands progressing. They remain owned by the host
and VM lifecycle components; workspace deletion still reserves eligibility
first and delegates stop/exit proof to those components. Retain both contracts
and validate the affected VM and stale-host consumers on the combined tree.

The implementation adds one nullable transition UUID, reuses the deletion
operation field, and centralizes deletion eligibility before runtime teardown.
Database guards serialize every child/fork/active-merge/transfer/repository
admission with deletion; the direct-clean merge and VM start owners also check
at their final application boundaries. Policy admission, restore, completion
and timeout hold current ownership and membership through their transactions.

Passed on this branch (2026-10-22):

- `cargo fmt --all -- --check` and `python3 scripts/check-migration-checksums.py`.
- `cargo clippy -p g5-api-core -p g5-api-routes -p g5-contracts --all-targets -- -D warnings`.
- `cargo test -p g5-api --test integration -- api_workspace_destructive_operations:: api_workspace_transition:: api_workspaces::test_delete api_workspace_vms::forced_workspace_delete api_workspaces::deletion_during_clone_merge api_workspaces::transfer_during_clone_merge api_workspaces::direct_clean_merge api_orgs::test_create_org api_orgs::new_org_tracks --test-threads=2`: 34 passed, including all ten new race/recovery regressions.
- Migration 107 (then numbered 106) ran under a non-superuser, non-bypass
  migrator with temporary SET membership and schema CREATE restored to their
  original revoked state.
- A second integration selection passed 22 tests: the formal fork-deadline
  race, organization test-token/bootstrap consumers, Base fork/name contention,
  workspace membership, transfer/start races and direct-merge ownership.
  Together the two final selections passed 56 tests, including all 15 tests
  that failed on the initially published CI head.

These tests use real HTTP, PostgreSQL, Git and background workers. VM RPC tests
use the existing scripted guest/host peers; they do not claim live MicroVM or
browser coverage. Those checks remain with the PR's CI lanes.

The legacy clone-merge race regression initially returned HTTP 200 and changed
the reserved destination branch. It requires the destination reservation check
at final application, alongside the existing ownership recheck.

Initial CI exposed the bootstrap role's inability to lock a workspace through
the new repository-mapping trigger. The existing organization-creation HTTP
test reproduced HTTP 500 and passes with migration 107 (then numbered 106).
The formal fork-deadline test now gates the detached resumer after admission
commits, then holds the source row through expiry; its original lock blocked admission itself after
the new exclusion guard. Both paths retain their original acceptance criteria.
The deletion and transfer races now observe the competing backend blocked by
the exact fixture transaction instead of relying on fixed sleeps.

After integrating main's credential and host-progress changes, 180 selected
integration tests passed with two test threads. This includes the complete
workspace VM, stale-host, credential-authority and organization-authority
suites, all previously selected deletion/policy/fork/transfer/bootstrap cases,
and the populated refresh-token upgrade and no-op replay. The historical
fixture now stages only versions through 104 before applying 105–107; later
migrations no longer leak into its starting schema. Formatting, migration
version uniqueness, catalog checks, all eight checksum-script tests, and
Clippy for the three affected API/contracts crates also passed on this tree.

## PR #715 reconciliation with main

The owner requested conflict resolution without a merge commit. Both branches
implement the same deletion reservation and policy-drain identity. Retain main's
current-authority checks, database admission triggers, exact reservation check
inside Stop admission, and force supersession. Keep PR #715's placement guard,
remote-tracking/binding guards, source-only rebase's optimistic target check,
transfer invalidation, and restart/retry regression coverage. Owner mutation
checks exclude deletion; the deletion coordinator uses its own current-owner
checks so an interrupted reservation can still be retried. Completed policy
writes report their known drained population without a fallible post-commit read.

Main's migrations 105–107 are already published and remain byte-for-byte intact.
Migration 106 already owns the policy UUID and constraint, so remove PR #715's
unmerged migrations rather than adding the same column twice or renumbering it.
This leaves no schema delta relative to main. Main's credential and host-progress
changes remain intact, with both implementations' regression suites retained.
The transfer regression now checks the invalidated identity remains untouched
by the old worker instead of assuming transfer preserves the old identity.

The earlier PR #715 evidence remains in its preceding commits: 258 distinct
local integration tests, Clippy, a populated migration rehearsal, and Rust/E2E
CI passed before main advanced. That evidence does not substitute for validation
of this reconciled tree. Verification results for this tree follow below.

Reconciliation validation on 2026-10-22, against main `3f2dafa25`:

- All 258 selected API integration tests passed. The initial run passed 23
  cases before it was interrupted to avoid repeated migration setup. All 235
  remaining cases, including the interrupted ones, passed using an isolated
  template migrated by the production runner; no test failure remains.
- Coverage includes both deletion/policy regression modules, VM lifecycle,
  fork admission and access, placement, workspace membership and transfer,
  PR/rebase consumers, the formal fork deadline, and the route authorization
  audit. The transfer test also changes ownership away and back before invoking
  the old worker's completion and timeout paths.
- `cargo clippy -p g5-api-core -p g5-api-routes --all-targets --offline -- -D warnings`,
  `cargo fmt --all -- --check`, and `git diff --check` passed.
- Migration checksums, version uniqueness and all eight checksum-script tests
  passed. The migration directory is identical to main.
- The proposed linear reconciliation commit has the verified tree, merges
  cleanly with main, and introduces no merge commit. CI and provider review
  must be checked again on the published final head.

Integration invocation: `cargo nextest run -p g5-api --test integration --offline
--no-fail-fast --test-threads=4 -E '<selection>'`. Selection:

```text
test(api_workspace_deletion_policy::) | test(api_workspace_destructive_operations::) | test(api_workspace_transition::) | test(api_workspaces::) | test(api_workspace_vms::) | test(api_workspace_fork_) | test(api_worker_hosts::placement_) | test(api_workspace_pull_requests::) | test(api_workspace_transfer_credentials::) | test(api_workspace_member_authz::) | test(route_auth_static::) | test(api_formal_agent_approvals::a_detached_fork_cannot_publish_after_its_capture_deadline)
```
