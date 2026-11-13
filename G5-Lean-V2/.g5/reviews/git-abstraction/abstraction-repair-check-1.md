# Bounded abstraction repair recheck — reviewer 1

Read-only review of the uncommitted repair over `b310368d`, 2026-10-02.
Scope is consolidated AA1 (fork capture versus platform merge) and AA2 (retired
unchecked/dead APIs and their live CLI replacements). Read the current plan's
follow-up section and review's initial consolidation/runtime-reproduction update.
Original findings remain unchanged in `abstraction-audit-1.md`. No source edits,
Cargo builds, service changes or runtime tests were performed by this reviewer.

## Remaining repair issue

### RC1 — P2: local Push still refreshes its expectation after capturing the intended head

`backend/g5d-cli/src/push.rs:1013` captures `head_sha` with `current_head`, then
calls `align_local_branch`. The replacement at `:152–160` obtains a fresh target
OID and uses that as the expected old value. For the normal case where the target
branch is the current branch, this admits the stale-source interleaving:

1. Push captures current branch commit A.
2. Another checkout writer advances that same branch to B.
3. `align_local_branch` now reads B and successfully CASes B back to A.

The new primitive is checked, but its caller blesses intervening work as its
expected state. This is an inherited caller race left open by AA2's current
replacement, rather than a new regression caused by removing the old writer.
The previous delete/recreate fallback has correctly been removed.

**Required check/repair:** bind alignment's expectation to the initial capture;
in particular the current branch must still equal the captured `head_sha`.
Capture any separately aligned canonical/alias branch expectation before doing
dependent work, pass it to the operation, and propagate a stale-state refusal
without deleting the branch. Add a deterministic test capturing A, advancing the
current branch to B, then attempting alignment with A; B must survive. Also keep
the legitimate absent/canonical/remote-name alias cases. This follows
`g5.architecture.git_ref_updates` and AA2's explicit expected-state requirement.
Evidence here is the current source interleaving, not an executed CLI race.

## AA1 source assessment

- `workspace_fork_service.rs:576–585` acquires the existing
  `g5:workspace-commit:<source>` advisory transaction fence before source identity,
  membership, receipt eligibility and sequential Git-head capture. It remains
  held through receipt normalization and manifest construction. The transaction
  is rolled back at `:886`, before `lock_repo_rows` and before the later provider
  preflight or live VM operations. This closes the original successful-merge
  interleaving at the source owner.
- Reopening capture via `ScopedDbReopener::open` restores the scoped session; it
  does not retain the route's prior workspace row locks. The capture acquires only
  one workspace fence and performs ordinary source reads while holding it, then
  releases it before repository/name locking. I found no new fence/row-lock
  inversion on this changed path. Concurrent captures of the same source serialize;
  unrelated workspace fences are separate. Unrelated-workspace progress was not
  runtime-tested here.
- Actual merge landing takes the same fence (`branch_commit.rs:275–284`) before
  inventory/head validation and keeps it through Git writes and database commit
  (`:347–369`). Workspace clone merge also acquires that fence at
  `workspace_service.rs:7114`; multi-workspace JobMerge acquires fences in UUID
  order at `job_merge_service.rs:1645–1646`. The new reader does not add a second
  workspace-fence acquisition.
- The frozen fork manifest is intentionally persisted after releasing the source
  fence. Its immutable OIDs make a subsequent successful source advance harmless;
  the existing receipt-pinning/pruning gap remains separate and is not repaired
  by this lock.

## AA1 test assessment and current evidence limit

The two new tests in `backend/g5-api/tests/api_workspaces.rs:4043` onward exercise
the actual public `/clone` and `/merge` requests with real SQL and two local Git
repositories, including code/docs membership. The fixtures establish initial state;
the transitions under test go through production owners.

- The first pauses the real fork immediately after its first ref read, launches
  the real merge and distinguishes merge completion from a publication-fence
  waiter. It asserts persisted `repo_manifest`, both child branch refs and fork
  proof refs, complete child membership, final parent refs and same-key replay.
  Both `branch` and receipt-absent `durable` source modes run. The recorded
  `/private/tmp/g5-audit-concurrency-before.log` fails at the frozen manifest with
  the fence absent, so the principal assertion discriminates the original defect.
- The second pauses real merge after accepted Git writes and before SQL commit,
  launches fork and requires no frozen fork operation before releasing merge.
  It then checks the complete post-merge cut through the same authoritative
  assertions. The rendezvous is behind `test-helpers`; it is one-shot and its RAII
  handle releases waiters on panic/drop. It does not alter production behavior.
- At recheck time, `/private/tmp/g5-audit-concurrency-repaired.log` contains
  **1 passed / 1 failed**: pre-merge capture passed, post-merge capture timed out
  waiting for the specifically named advisory-lock wait event. This does not
  establish a source-fence defect: a public request may first block at an earlier
  authorization/row boundary. It does mean post-merge runtime proof is still
  pending. Main owns diagnosis and rerun. Preserve the semantic assertion (fork
  cannot freeze until the actual merge settles); do not weaken it to elapsed time
  or mere request overlap.
- Follow-up source trace identifies the earlier blocking operation precisely:
  `PermissionService::require_user_scopes_on_conn` calls
  `authz.lock_user_scope_authority`; migration
  `backend/g5-api/src/db/migrations/042_scope_workspace_liveness_lock.sql:62–69`
  acquires `FOR SHARE OF workspace` even for read-scoped authority. The paused
  merge holds the target workspace `FOR UPDATE` from landing inventory. Therefore
  a new public `/clone` can correctly wait on the workspace row before it reaches
  the source advisory fence. The reverse-order test should observe this concrete
  lock contention too while preserving the no-frozen-operation assertion.
- Neither test seeds a valid retained flush receipt, a failed merge/SQL commit,
  live capture or an unrelated concurrently progressing workspace. Their result
  must be described as successful in-platform two-member merge versus branch/
  receipt-absent durable fork, not the exhaustive failure/receipt matrix.

One additional boundary warrants explicit qualification: after a *refused database
commit*, `branch_commit.rs:369–391` performs per-ref compensation via
`restore_applied_branch_refs` (`:1353`) after the transaction fence has ended.
That failure-time writer is not serialized with the new capture fence. No runtime
compensation/fork scenario was executed here. The current fix establishes the
successful landing property; an unrestricted claim covering settlement failures
would need to retain/reacquire common protection around compensation and test it,
while honoring the existing partial-write/no-cross-repository-atomicity contract.

## AA2 retirement and other replacement assessment

- `GitRepo::update_branch_ref`, `push_force`, the effectful
  `merge_branch_no_checkout`, dead `create_branch_ref_if_absent`, workspace-ops
  merge/pull/mirror/workspace merge exports and implementations are removed.
  Repository-wide source search found no remaining production consumer of those
  removed names. `write_baseline_ref` is now gated by `test`/`test-helpers` at
  `g5-scm/src/repo.rs:2523`.
- The dead structured-merge injection port, SCM resolver field/adapter, common
  compatibility re-export, semantic adapter and their sole-workflow tests are
  removed together. The canonical `local_grounding` resolver remains; the removed
  APIs had no current production caller in the inspected inventory. Removing the
  obsolete implementation's tests is appropriate and should not be presented as
  increasing current-path test coverage. SCM's now-unused YAML and semantic-test
  dependencies are removed too.
- `workspace_manifest.rs:1216,1232` uses the already-captured `refreshed_head` as
  the CAS expectation when retaining/replaying unpublished commits. This rejects
  an advance after refresh capture rather than reacquiring a newer expectation.
  No new defect found in those replacements. Current-branch/alias configuration
  assumptions and actual CLI regressions remain main's runtime-check scope.
- Native/libgit2 and local checkout versus authoritative landing remain legitimate
  variants. The API policy guard now rejects direct checked writers outside named
  policy owners, so merely using a checked SCM method does not authorize an API
  service to bypass canonical landing. Guard internals beyond that scoped check
  belong to the other repair reviewer.

**Disposition:** AA1's original successful merge/fork race is repaired by source
inspection and its pre-merge counterexample has discriminating runtime evidence;
post-merge runtime evidence is pending in the inspected receipt. AA2 retirement is
coherent; RC1 remains for the live Push caller. Broader failure/receipt/live-VM
coverage and compilation/runtime receipts are explicitly unverified here.

## Incremental follow-up after RC1 and compensation repair

Rechecked the subsequent uncommitted source on 2026-10-02, without builds or
runtime mutation. Earlier findings and receipts above are retained chronologically.

- **RC1 is resolved by source inspection.** Push captures canonical and optional
  remote-name alias branch expectations before reading `current_head`, then passes
  those immutable expectations into `align_local_branch`. Alignment no longer
  reads a replacement expectation or deletes/recreates on failure. The original
  A-capture / B-advance / overwrite-B-with-A schedule now refuses. A concurrent
  advance that is itself the newly captured desired head produces the existing
  harmless already-applied outcome. The added local alignment test explicitly
  exercises an intervening advance, invalid OID refusal, creation and an expected
  update, asserting the branch and HEAD survive refusal. This reviewer did not run
  that CLI test.
- **Both success concurrency cases now have a passing receipt:** inspected
  `/private/tmp/g5-audit-concurrency-repaired-2.log`, **2 passed / 0 failed**,
  3.91 seconds. The reverse-order waiter recognizes either the workspace fence
  or the actual authority-row lock, requires a real blocker, and retains the
  no-frozen-fork-before-merge-settlement assertion. This supersedes the earlier
  failed reverse-order receipt without erasing it.
- **Compensation now cooperates with capture.** The landing preserves a scoped
  reopener, and a confirmed SQL commit refusal opens a fresh transaction and takes
  the same workspace fence before any reverse-order ref restoration. All per-ref
  compare-and-swap restores and incomplete-result collection happen under that
  fence; rollback releases it after completion. This closes the previously noted
  half-restored-vector capture window. Reopening/locking errors leave refs as they
  are and return an inspect-before-retry error; mismatches preserve another writer's
  ref and report incomplete compensation. There is no new lock namespace or retry
  persistence. I found no new lock-order cycle in this repair.
- **Outcome classification now has one owner.** The current source has replaced
  the old branch-commit-local predicate with `ScopedDbError::commit_was_refused`.
  Both SQL rollback actions and Git compensation call that method. It admits a
  `ClearFailed` result (COMMIT not sent), or a PostgreSQL `ERROR` excluding SQLSTATE
  class `08` and `40003`; FATAL/PANIC, transport loss and unknown completion do not
  authorize compensation. This preserves `ScopedDb::commit`'s stricter original
  policy and corrects the inherited broader branch-commit classification. Current
  unit additions inspect ClearFailed and pool-timeout commit errors; severities/
  SQLSTATE distinctions were source-checked here, not runtime-injected.
- **The new compensation test is discriminating by construction, but its runtime
  result is pending.** `concurrent_fork_cannot_capture_half_compensated_landing`
  drives the real prepared landing and its actual database COMMIT, using deferred
  uniqueness to force a confirmed refusal after accepted Git writes. It checks
  that exactly one member is restored at the rendezvous, starts the real public
  fork, and ultimately verifies the whole old source/child/manifest/proof vector.
  Without the compensation fence, a completed fork can expose the mixed vector and
  fail the final authoritative assertion. It does not invoke the public merge
  route for this fault injection; the two successful cases already cover that
  route, and this third test covers the shared landing/compensation owner itself.

No additional concrete defect was found in this bounded follow-up. Compilation,
CLI runtime and compensation-test execution remain main's work. The compensation
fence is reacquired after the refused transaction ends, so a capture that wins the
fence *before compensation starts* can legitimately observe the entire then-current
Git cut; the repair prevents capture during per-ref restoration and does not claim
cross-repository atomicity or that every concurrent fork must select the old cut.
Receipt-pruning, retained-receipt/live-capture, external writers and exhaustive
cancelled/failed-compensation scenarios remain outside the executed evidence.
