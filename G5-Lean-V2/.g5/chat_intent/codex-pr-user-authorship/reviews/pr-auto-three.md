# Automatic PR refresh — independent review three

Read-only discovery of the frozen implementation represented by
`/tmp/g5-pr-auto-review-snapshot.tgz`, 2026-09-30. This reviewer did not implement the
change, receive the author's solution narrative, or read peer findings. No production,
ontology or test files were edited. Findings were saved before sharing. Parent supplied
test-log locations and later baseline pass counts; these are evidence, not implementation
explanations. No additional agents were used.

Scope: the automatic-push amendment in `ontology-plan.md`, its request, actual changed
governance/architecture blocks, and material consumers of automatic evidence and provider
publication. Earlier CLI, authorship and title work is inspected only where reused by this
change. Evidence below is source inspection unless explicitly identified as test execution.

## Initial actionable findings

### F1 — P2: automatic continuation inherits a content-write refusal

`backend/g5-api-core/src/services/workspace_pull_request_create.rs:411` calls the same
`authorized_db` as explicit creation; lines 119–124 invoke `ensure_workspace_mutable`.
That helper always selects `DirectWorkspaceWrite`
(`workspace_mutation_policy.rs:246–260`), which rejects `read_only` and `freezing`.
The same policy explicitly permits Git operations, including receive-pack and publication,
in those states. Thus an existing PR created while writable never follows a successful
later push once its source is read-only. It stays pending forever despite current owner
and target authority. Because admission precedes the no-change check, a periodic scan also
introduces an error on an unchanged read-only PR.

This is a newly reused guard with the wrong policy variant for the committed-work
continuation (`g5.governance.pr.follow-push`, `g5.architecture.pr_flow`). Keep current owner,
transfer and target checks, but classify automatic publication as the appropriate Git
continuation instead of explicit content preparation. Preserve explicit create/update
restrictions. Regression: create PR A, put its source in each supported read-only/draining
state, successfully push B through receive-pack, and verify the existing PR reaches B with
no guest save; an unchanged scan should not manufacture a refresh failure.

### F2 — P2: explicit update can erase discovery of a push's unfinished provider work

Automatic admission at `workspace_pull_request_create.rs:412–417` considers only
`source_heads` and `pending_push_refresh`. Explicit create/update refreshes source evidence
and commits it at line 292 before its provider phase, but never sets the pending flag.
The new `finish_push_refresh` at lines 501–507 changes rows only when that flag is already
true.

Concrete sequence: existing PR at A, push B, explicit Update reaches B before the periodic
worker, then provider synchronization fails or the process stops after the evidence commit.
The PR now has `source_heads=B`, `pending_push_refresh=false`, and old/failed provider state.
Every automatic scan returns at line 417, including startup after a crash. The successful
push's provider continuation requires another manual Update despite the promised durable
discovery/retry (`g5.governance.pr.follow-push`, `g5.architecture.recovery`). This also occurs
when explicit update wins a race with automatic work before the automatic path creates its
pending flag.

Fix: make accepting refreshed evidence and recording unfinished provider continuation one
durable owner transition shared by both entry paths, or compare an authoritative provider
completion version during discovery. Settle only after the appropriate provider evidence.
Regression: public Update of an existing PR captures B, injected provider outage or the
post-evidence crash boundary occurs, and a subsequent no-event full scan completes provider
publication without manual action.

### F3 — P1: target permission can be revoked while refresh waits for the PR lock

At `workspace_pull_request_create.rs:429`, target access is checked before waiting on
`SELECT ... FOR UPDATE` at lines 432–436. After obtaining that lock, the code performs
provider observation (440–443), writes evidence (444–451), and admits provider effects
(452–455) without rechecking target access. `ensure_user_workspace_access` in `db/scoped.rs`
uses ordinary reads of target visibility/invitations (739–770); unlike the source owner
check, it does not lock the authority being checked. A title/update/merge holding the PR
row can keep this operation waiting while the target owner revokes the source owner's
invitation. Refresh then publishes using access that is no longer current. The preceding
provider observation adds another asynchronous gap before effects.

This violates the automatic amendment's current target authority at effect boundaries and
`g5.security.admission`/`g5.security.read`. Re-admit after blocking acquisition and immediately
before effect admission, using the project's authority synchronization approach so a
revocation cannot fall between the effective check and admission. Preserve the pending
obligation on refusal. Regression: hold the PR row lock, start automatic refresh as an
ordinary member invited to a private target, revoke that invitation, release the lock, and
assert no new evidence/provider effect; restore access and verify recovery.

### F4 — P2: an unresponsive Git remote can starve all PR refresh work

The new worker processes every PR serially at
`backend/g5-api-core/src/maintenance/workspace_pull_requests.rs:62–69`. Its automatic
provider path synchronously invokes `GitRepo::ls_remote_ref` on the Tokio worker thread
(`workspace_pull_request_provider_lifecycle.rs:991–1001`, and target observation afterward).
The helper uses libgit2 `connect_auth`/`list` without a deadline (`g5-scm/src/repo.rs:890–924`).
The checked-in dependency source available in the local registry declares both libgit2
socket/connect timeouts as zero, and no application timeout configuration was found.
Reqwest's separate 30-second timeout does not cover these Git calls.

A remote that accepts a connection and stops responding blocks the only refresh loop;
later local-only PRs and other organizations are never visited, and the outer cancellation
select cannot interrupt synchronous polling. Continuing after returned errors is insufficient
for `g5.architecture.recovery.progress` and the design's no-starvation obligation.

Fix: use a bounded Git observation operation off the async executor, with an actual
transport/child deadline and resource cleanup; ensure one waiting item cannot monopolize
the scan. Regression: first PR's Git transport stalls, a later eligible local-only PR still
refreshes within a bounded interval, and cancellation shuts down without an unbounded task.

### F5 — P2: existing provider members skipped as already included retain stale pins

Automatic synchronization reuses the `repo_needs_provider_pr` early return at
`workspace_pull_request_provider_lifecycle.rs:568–577`. With no pending title it never
loads an existing provider row for a member whose newly published source is identical to,
or already included in, the target. Its `source_head_sha` consequently remains A even
though automatic G5 evidence has advanced to B. If aggregate provider status was open,
`finish_push_refresh` clears pending and subsequent scans skip the PR. Merge consumes
every existing provider row and rejects A versus current B at lines 731–741, asking for an
Update PR. The title-only branch does not solve this either: `sync_recorded_title` does not
refresh its pinned source/target fields.

This matters for a multi-member PR when one repository's new source is already included in
the target but other members still need merging; the valid empty-member exception must not
discard refresh of an already recorded provider PR. It violates the same-record provider
refresh and merge-without-Update acceptance. Reconcile existing provider records separately
from the decision whether to create a new empty provider PR, and settle refresh only when
the authoritative consumer has compatible pins. Regression: keep the provider PR open,
advance a member's source to an already-included target commit, run automatic refresh, and
verify the coordinated merge does not reject an obsolete stored member pin. No new empty
provider PR should be created.

## Commitment and consumer coverage

| Commitment | Actual path / discriminating case | Assessment |
| --- | --- | --- |
| `g5.governance.pr.follow-push` | Branch-update EventBus hints and startup/15-second inventory; `refresh_after_push`; native push B after PR A | Basic path supported by supplied native test; F1/F2/F5 are exceptions. No new G5 PR insertion, target selection, autosave or compute entry is called by automatic work. |
| Published refs; no dirty snapshot | `PublishedEvidenceCapture` → `resolve_published_snapshot`; final vector equality check | Correct separation by inspection; supplied multi-member/dirty-flush API test verifies committed heads instead of retained dirty data. |
| Auto/manual external policy | `synchronize_after_push` observes remote source; `remote_integrations::enqueue_after_push` retains commit-trigger policy | Supplied provider test proves internal push alone does not push the external branch, then recovers after external publication. F4 affects liveness. |
| Closed/merged protection and unresolved merge pins | PR row lock shared with provider merge; pre-refresh provider terminal check; active merge guard on prepare and revalidate | Correct intent and ordinary paths by inspection. Supplied tests cover terminal G5 PR and merge source race, but no simultaneous provider acceptance/refresh test was executed in this review. |
| `g5.governance.approval.version` | No approval writes in continuation; approval reader still compares reviewed vector | Supplied API test records actual approval, pushes two members, verifies retained review version and stale approval. |
| `g5.security.owner/admission/tenant/read` | Maintenance scope discovers; continuation re-mints current owner scope; source row share lock; target access helper | Current owner/tenant separation is sound by inspection; target-access-before-entry test passes. F3 concerns revocation during admission waits. Owner transfer and membership-revocation race checks remain unexecuted here. |
| `g5.architecture.pr_flow` and minimal implementation | Shared evidence and provider owners; thin periodic worker and two pending/error columns | Mechanism is proportionate and avoids a second publication implementation. F1 identifies an overbroad reused guard, F2 an incomplete shared completion transition. |
| `g5.architecture.recovery/recovery.progress` | Open PR inventory plus source divergence/pending flag; per-PR error continuation | Event-loss/retry recovery covered in supplied tests, with F2 and F4 gaps. Errors stay separate from Git success. |
| Honest visibility and commit notification | Error overlays summary message; event emitted after DB commit; frontend invalidates source/target PR queries | Code and supplied WebSocket test support invalidation. Exact user-facing pending/error explanation and no-op read-only behavior require F1/F2 regression checks. |

## Verification and boundaries

Inspected `/tmp/g5-pr-auto-native.log` (1 native Git test passed),
`/tmp/g5-pr-auto-api-3.log` (3 automatic API tests passed), and
`/tmp/g5-pr-auto-ui.log` (4 WebSocket tests passed). Parent subsequently reported the
broader baseline: 36 PR API, 4 PR read API, and 5 native push API tests, typecheck and
WebSocketManager/QueryClient checks passed. Those results do not exercise the counterexamples
above. This reviewer did not rerun the suites or execute the proposed regressions; findings
are source-derived and should be confirmed while consolidating. No live GitHub/deployed
worker behavior was verified.

Read current block metadata for the changed PR flow/push commitments and governing approval,
merge, authority, recovery and design principles. Existing unassessed/deferred claims do not
establish correctness here. After repair, the main owner should update the changed block
footers with automatic implementation locations, actual regression results and remaining
live-provider/concurrency limits; no governing meaning amendment is required by these
proposed fixes. A review-workspace exclusion was checked against its actual service: current
Push review uses a distinct delivery flow, so no finding is made about nonexistent handback
PR creation.
