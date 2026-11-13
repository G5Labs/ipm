# Independent commit review two — initial findings

Snapshot: `f2b2d9f578d62771e2ceea893dc1a805aa085864`, compared with parent `75b522e7`.
Mode: independent, full-scope, read-only discovery. No production, ontology, or shared test
files were edited. This report was saved before reading peer/prior review reports or
sharing findings. Investigative source, build output, and test output are isolated under
`/tmp/pr-commit-review-two/`.

## Inputs and scope

Read the actual `AGENTS.md`, ontology-review skill, design-review guide, the pinned
`ontology-plan.md` beginning with “Ontology changes and governance,” and `request.md`.
Compared the current ontology diff with the saved branch-wide diff: this commit adds
PR read/write/follow-push/target/title/JSON contracts, the shared-flow architecture, and
amends retained readiness. The saved diff also includes earlier authorship/account
connection work already present in the parent; that work is contextual here.

Read the affected commitments and footer entries in governance, Git/governance architecture,
security, runtime lease, system-design principles, and coding principles. Inspected the
actual service, public/internal route, authentication, credential, CLI, worker profile,
maintenance, migration, UI target/PR query, and WebSocket changes, plus material existing
preflight, publication, access, and merge consumers. Prior reports referenced by ontology
footers were not opened. Their claimed test results were not used as independent proof.

## Findings

### F1 — P1: Recheck target access after queued manual PR writes acquire their effect lock

**Introduced surface:** The new VM create/update route and explicit-title effect accept a
mutation after target access has been revoked. The earlier UI path also had broader
pre-existing admission-race exposure; this finding is specifically demonstrated through
the newly introduced VM route and title write.

**Evidence at this commit:**

- `backend/g5-api-core/src/services/workspace_pull_request_create.rs:111–113` locks the
  target workspace only for `Continuation`; manual user/VM phases do not protect its
  visibility/ownership.
- `workspace_pull_request_create.rs:143–152` performs an ordinary target-access check,
  then `281–286` enters local application without protecting the invitation or rechecking
  after the PR lock wait.
- `backend/g5-api-core/src/services/coordinated_repo_publication.rs:318–333` loads the
  existing PR and issues the newly added title UPDATE. The UPDATE can wait on another
  transaction's PR lock; after the wait it accepts the title using the earlier access
  decision. Evidence/description writes and source publication follow before the commit
  at `workspace_pull_request_create.rs:310`.
- The later provider admission at `workspace_pull_request_create.rs:314–319` correctly
  notices revocation, but the unauthorized local mutation is already committed. Provider
  synchronization has another precheck-then-wait boundary at
  `workspace_pull_request_provider_lifecycle.rs:555–558`.
- Automatic continuation already handles the equivalent race by locking/rechecking the
  invitation after its PR wait (`workspace_pull_request_create.rs:451–472`), making the
  missing manual-path protection concrete rather than an undefined policy question.

**Trigger and observed consequence:** An eligible owner uses a valid current VM token to
update an existing PR into an invite-only target. Hold that PR's row lock, wait until the
actual internal POST is blocked at its title UPDATE, revoke the owner's target invitation,
then release the lock. The request returns HTTP 409 with
`pull_request_provider_sync_incomplete`, `causeCode: FORBIDDEN`, and phase `admission`, but
the title has already changed from `feature.txt` to
`SHOULD NOT BE ACCEPTED AFTER REVOCATION` in PostgreSQL.

**Runtime evidence:** `/tmp/pr-commit-review-two/probe.rs` adds only the isolated test
`review_two_vm_target_revocation_while_waiting_for_pr_lock` to a copy of the current test
fixture. It uses the real internal HTTP route, a real PostgreSQL PR row lock observed via
`pg_blocking_pids`, and the existing scripted VM peer. It invokes the production service
libraries from the existing dependency artifacts; the production source remained frozen.
The preservation assertion fails with the persisted values above. Output is in
`/tmp/pr-commit-review-two/revocation.log`; compilation procedure/log are beside it.
Initial sandbox execution could not connect to local Postgres; the same isolated probe
then ran with approved local database access. No external provider is needed to exhibit
this defect.

**Governance:** `g5.governance.pr.write`, `g5.governance.pr.target`,
`g5.architecture.pr_flow`, and `g5.security.admission/read` require current target access
at effect admission, including the newly introduced VM path.

**Repair:** Put shared source/target and invitation fencing/revalidation around local PR
acceptance and provider effect admission for all caller variants. Acquire locks in one
consistent order, recheck access after any PR wait, and retain the protection through the
corresponding commit/effects. Do not merely improve the later partial-error message.

**Regression:** Keep the actual VM POST/row-lock/revoke case and assert the old title,
description, evidence, pending title, and provider effects remain unchanged after refusal;
add the equivalent user route and provider-phase wait. Retain a valid invitation case
that completes successfully. Completed earlier source commits may remain, as designed.

### F2 — P2: Bound a waiting PR so the sole refresh worker can visit later PRs

**Introduced surface:** New automatic push-refresh worker.

**Evidence at this commit:**

- `backend/g5-api-core/src/maintenance/workspace_pull_requests.rs:62–68` sequentially
  awaits each `refresh_after_push` with no per-item deadline or nonblocking admission.
- `workspace_pull_request_create.rs:112–113` and `451–455` await workspace/PR row locks
  without a lock timeout. The error-recording UPDATE at `414–418` can also wait.
- The caller's `run_loop` (`maintenance/workspace_pull_requests.rs:33–38`) does not
  service another hint or interval while the pass waits; its only escape is whole-worker
  cancellation. `ScopedDb::from_capability` does not install a statement/lock timeout,
  and the timeout-setting operations elsewhere in this repository are operation-local.
- PR locks are retained across provider work. Explicit synchronization takes the PR
  lock at `workspace_pull_request_provider_lifecycle.rs:555–558` and can then run the
  existing unbounded Git transport path at `1072–1074`; a stalled such operation is a
  production contention source, not just an artificial migration lock.

**Trigger and consequence:** An earlier open PR has pending refresh while another
operation retains its workspace or PR lock. The sole background pass waits indefinitely
on that PR and never visits later eligible PRs or other organizations, even if those PRs
are local-only and unrelated. The HTTP client's 30-second timeout does not bound SQL
lock waits. Error handling that continues after a returned error does not address a
future that never returns.

**Evidence level:** Source/control-flow inspection, not an executed whole-worker
starvation test. The existing production-lock test and F1 probe establish that PR
operations really can wait on the lock; the unbounded duration and serialization follow
from the inspected code. This is a new recovery-worker defect, not a demand to redesign
all existing manual Git transport.

**Governance:** `g5.governance.pr.follow-push`, `g5.architecture.pr_flow`, and
`g5.architecture.recovery.progress` explicitly require waiting or failing entries not to
indefinitely prevent later eligible work from being revisited.

**Repair:** Give automatic work bounded/nonblocking admission and bounded per-item
execution, preserving the durable obligation when deferred. Ensure failure recording
cannot reintroduce an unbounded wait. A timeout/cancellation must not report uncertain
provider effects as complete. This can remain within the existing owner/worker design.

**Regression:** Hold an earlier PR's actual row lock, make a later independent PR need
refresh, run the maintenance worker, and assert the later PR reaches the new committed
head while the first lock is still held. Release the lock and assert the first PR then
converges. Include a provider-delay/uncertain-effect retry variation only if the selected
bounded-execution repair changes that boundary.

## Commitment ledger

| Commitment | Actual path / counterexample or check | Assessment |
|---|---|---|
| `g5.governance.pr.read` | CLI `pr.rs` → user/current-incarnation info routes → durable list or scoped `workspace_pull_request_read.rs`; attached-repository selection, canonical URL/number validation, bounded no-redirect reads, provenance | Inspected; no additional defect found. Live provider reads not executed here. |
| `g5.governance.pr.json` | `Selection::parse` before context/body/network; object/array projection, field/empty validation, node ID, null multi-provider aliases, comment demand; custom Clap scoping and process output | Inspected. Existing compiled `create_json_help_and_invalid_input_never_attempt_workspace_or_body_loading` independently passed (one targeted test). Other parser/read cases were inspected, not rerun. |
| `g5.governance.pr.write` | Thin public/internal routes → one shared owner; VM source/owner/incarnation pinning, save/push preparation, reuse, current phase admission, explicit partial errors | F1 violates target authority during queued effects. Other inspected refusal/preparation paths preserved. |
| `g5.governance.pr.target` | Shared `targets` filters current same-org live readable parent/Base, stable name/ID ordering; omitted resolution before preparation, pinned target thereafter; frontend consumes shared list | F1. Default/explicit selection and cache invalidation inspected without another finding. |
| `g5.governance.pr.title` | Trim/nonblank validation, atomic local pending intent, serialized latest-title consumption, existing/included provider-member PATCH, success-only clearing | F1 includes a newly accepted title after authority loss. Failure/retry and concurrent-title tests inspected; live GitHub unexecuted. |
| `g5.governance.pr.follow-push` | Events + startup/periodic inventory → current-owner continuation, published snapshots, source-head coalescing, existing PR identity, pending/error fields, external ref observation without push | F2 violates progress under lock contention. Other inspected behavior distinguishes source push from remote push and dirty snapshots; no new G5 PR or compute. |
| `g5.governance.publish/readiness` | Shared explicit preparation runs before fresh evidence; original VM/epoch carried into native operations; automatic path selects published refs | Inspected; active guest/native save/push was not independently executed. Existing tests use scripted peers; receive-pack tests were inspected. |
| `g5.governance.publish.github-author/actor` | Existing credential owner reused for new PR creation, current membership/repository/binding coverage, user grant for creation, operation-specific installation/personal policy retained | Changed credential-scope refactor and consumers inspected. Earlier grant/account flows are context, not new verification. Live provider authorship unexecuted. |
| `g5.security.owner/admission/tenant/tenant.write/read/read.durable/login.service-identities`, `g5.runtime.lease` | Typed adapters, route allowlist, scope minting, source owner/VM/epoch/lease admission, durable workspace reads, target access | F1. No separate cross-tenant or token-substitution bypass found on inspected changed paths. |
| `g5.security.credentials.publication` | Server-only provider credential resolution; provider read responses sanitize transport/provider errors; worker command carries workspace credentials only | Inspected; no new token disclosure found. |
| `g5.governance.approval.version/merge/merge.effects` | Published-evidence refresh never updates approval rows; active-merge rejection, shared PR lock, terminal-provider checks, revalidation before evidence acceptance and source check afterward | Inspected; existing approval and accepted-merge recovery tests checked in source. Full crash/merge operation semantics remain pre-existing incomplete scope. |
| `g5.architecture.pr_flow/governance_owner` | Entry adapters share complete explicit flow; automatic continuation reuses evidence/provider owners with a narrower mode; credentials retain their own policy | Sound ownership decomposition, with F1/F2 implementation gaps. No unjustified alternate PR algorithm found. |
| `g5.architecture.recovery/recovery.progress`, `g5.design.authority/commit-notify/durability` | Durable pending-title/push state; current-head discovery; PR hints emitted after automatic commit; visible pending/errors and explicit partial result | F2. Process restart and live ambiguous-provider recovery were not executed independently. |
| `g5.design.minimal-mechanism/change-locality/policy/transaction-composition` | Two narrow nullable/status schema additions, existing owner factoring, operation-specific automatic mode and in-scope credentials, UI delegates target choice | Inspected; mechanisms correspond to requested outcomes. F1 fix should remain shared; no policy amendment needed. |
| `g5.quality.migrations/migrations.verification` | Forward migrations 041/042 and appended checksums; populated/replay migration test source; no historical migration edits | Inspected; no migration test rerun. Earlier claimed populated-role result is not independent evidence here. |
| `g5.quality.review.cases/evidence/evidence.claims` | Permitted/refused/retry test sources, one independent compiled CLI check, isolated real HTTP/DB failing F1 probe, bounded scope versus pre-existing debt | Evidence distinguished below; no broad suite rerun. |

## Consumer and evidence limits

Frontend source/target PR query invalidation, automatic-update WebSocket hint, pending
status in all three changed PR components, title/body compatibility, shared target choices,
and bundled worker guidance were inspected. The UI still retains existing merge recovery
controls; no separate frontend bug was established. UI tests and TypeScript checks were
not rerun during this independent discovery.

No live GitHub, deployed VM, browser, process-restart, or complete merge/approval runtime
claim is made. The original provider-first merge protocol, nonowner formal-approval gap,
and full P6 readiness are explicitly documented pre-existing work and are not added as
findings. The isolated F1 probe provides actual persisted-state evidence; F2 remains
source-established and should get its specified discriminating regression during repair.

Proposed metadata update after consolidation: record the queued manual admission gap in
`g5.governance.pr.write/target`, `g5.architecture.pr_flow` and the recovery-progress gap in
`g5.governance.pr.follow-push`/`g5.architecture.recovery.progress` until repairs and checks
close them. Governing meaning and saved design need no product-policy amendment.

## Incremental review of the consolidated repair delta

Reviewed the working-tree production delta from `f2b2d9f5` after C1–C4 were saved in
`review.md`. Read-only bounded follow-up; no production edits and no duplicate execution
of the API/native/read/UI suites already running in the main task. The earlier failed
runtime probe remains the pre-repair evidence, not a post-repair result.

### C3 / initial F1: implementation repair is sound on inspected paths

`source_db_for_target` now captures the VM runtime handle before taking tenant workspace
locks, avoiding a self-wait against the runtime lookup's separate system connection.
It sorts/deduplicates workspace IDs and takes the VM source UPDATE strength at its first
acquisition, rather than later upgrading a source SHARE lock while already holding the
target. `lock_owner_handle` then revalidates the captured owner/VM/epoch/lease. User and
automatic callers retain the SHARE strengths they require.

The local apply path calls `lock_target_access` after local row-lock waits but before
publication/commit; a refusal rolls back title, description, evidence and pending intent.
The provider phase explicitly acquires the PR lock before the same invitation lock/access
recheck, and preserves the distinct partial-error semantics when the earlier phase was
validly committed. Invitation protection is retained through each effect transaction;
visibility/ownership are protected by the workspace locks. The automatic path uses the
same helper. No new target-access or reversed workspace-pair ordering defect was found.

The added real VM/user row-lock revocation cases compare the complete persisted PR row,
check no provider rows appear, and restore access for a successful retry. The provider-phase
case observes the actual wait after local commit and verifies refusal leaves that committed
state unchanged. These are discriminating cases for the original counterexample. Their
runtime results are owned by the main task and were not independently rerun here.

### C4 / initial F2: bounded contention and recovery are implemented

Automatic scopes now set a five-second PostgreSQL lock timeout, including the separate
failure-recording scope. The worker wraps the complete per-PR attempt, including error
recording, in a 120-second timeout and then continues to later entries. The existing durable
open-PR inventory plus source-head/pending comparison survives cancellation; the code does
not clear work or claim publication success on timeout. Provider observation remains the
bounded cancellable HTTP path. The new test keeps an earlier real PR lock held until later
organization work advances, then releases it and verifies the earlier PR converges. No
remaining instance of F2 was found in this bounded delta review. Exact runtime completion
and cancelled-provider behavior remain subject to the main task's test evidence.

### C2: guarded title acceptance is implemented

Both existing-PR reuse branches now call one title writer whose UPDATE includes
`status='open'` and requires exactly one affected row. A concurrent terminal transition
therefore produces Conflict and transaction rollback instead of modifying terminal title
or pending intent. The added test holds a real close operation across title acceptance and
compares the complete terminal row. This addresses the title-specific finding without
changing publication policy or adding a second flow.

### C1: one remaining identity edge in the UI repair (P2)

The same-PR success→failure and pending→recovery transitions are repaired: both dialogs
prefer query props, and `onSuccess` cancels older reads and replaces matching cached rows
before exposing mutation success. However, the precedence now applies even when the
query prop and mutation result identify **different PRs**:

- `frontend/src/components/workspace/CreatePullRequestDialog.tsx:176` unconditionally
  prefers `existingPullRequest`.
- `frontend/src/components/workspace/PrepareToShipModal.tsx:266–271` does the same, including
  the provider notice and subsequent actions.
- `frontend/src/hooks/useWorkspace.ts:527–533` replaces only an already cached matching ID;
  it does not replace/remove a stale older open PR for the same source/target when the
  response is a newly created PR.

Concrete trigger: the source query still contains open PR A; A is closed elsewhere without
this client receiving/refetching that change; the user invokes the shared create/update
flow, which legitimately creates PR B for the same source/target. The successful mutation
returns B, but the cache still contains A until the newly invalidated list request finishes.
Both dialogs therefore display A's provider outcome/links, and Prepare to Ship chooses A
for actions, despite possessing B's authoritative success result. A slow or stalled refetch
extends that stale interval. This is introduced by reversing the original mutation-first
precedence; it is within C1's stated “same-PR query data” handoff requirement.

Repair the identity handoff: prefer live data over a retained mutation result only when
it is for the same accepted PR, or update the relevant open-list cache to the authoritative
source/target result so stale A cannot win. Preserve the existing same-ID failure/recovery
behavior and filtered-list semantics. Add a mounted A-closed/B-created case with the list
refetch held pending, asserting B's links, status and action ID immediately after success.
The current tests cover same-ID transitions and matching-ID cache replacement only.
This residual is source-established; no new UI runtime test was run in this follow-up.

### Incremental limits and disposition

C2–C4 have coherent inspected repairs and meaningful added tests; their test results are
not claimed by this reviewer. C1 has the remaining different-identity handoff case above.
No broader architecture change or ontology-policy amendment is needed. The main task
should record final runtime results and the disposition of this minority edge before
closing the corresponding consolidated finding.

## Final bounded C1 recheck

The residual identity handoff is repaired. `currentWorkspacePullRequest` in
`frontend/src/lib/types.ts` selects live data only when it matches the retained mutation
result's PR ID; absent a mutation it uses live data. Both dialogs now use that helper,
and Prepare to Ship preserves its explicit merged-result precedence. The same-ID live
failure/recovery behavior remains intact, while a stale predecessor A can no longer
replace newly accepted B. The resulting provider presentation and action inputs continue
to derive from that selected PR. No additional material defect was found in this final
bounded recheck.

The added mounted regressions cover the A/B distinction in both dialogs and would fail
under the prior unconditional live-data preference. I inspected the main task's completed
logs: `/tmp/g5-pr-commit-ui3.log` records 60 passing UI tests;
`/tmp/g5-pr-commit-api2.log` records 46 passing PR API, five native-push and four read-API
tests, including the user/VM invitation, provider-wait, close-race and contended-worker
regressions. These are inspected main-task execution results, not duplicate runs by this
reviewer. Typecheck/lint were reported in progress and are not claimed as completed here.

All findings raised by this reviewer now have inspected repairs and the relevant recorded
regression evidence. Original findings, the failing pre-repair probe, and intermediate
assessment remain preserved above. Live GitHub/deployed VM/process-restart limits remain.
