# API Git proof observations — independent discovery 2

Read-only independent review of the increment against `f95fac40`. This reviewer did
not implement the change, read the author's implementation explanation or consult
peer findings. The assigned input was the original request, pinned design and full
implementation scope, including untracked `native/observations.rs`; no known defect
hints were supplied. No production or ontology edits, delegation, Cargo builds or
database tests were performed. Investigation resources are isolated under
`/tmp/g5-observation-review-2`.

## Scope and governing input

Read `AGENTS.md`, ontology-review, the authoring format and design review guides,
the original Git-abstraction request and the saved plan beginning with `Ontology
changes and governance`. The plan's API Git proof observations section adds no
ontology meaning: it implements the existing execution, outcome, location, receipt
and network contracts. The baseline diff has no ontology-body changes; existing
footer assessments still need the main review's scoped update.

Reviewed all changed production and test files against the frozen inventory at
`/tmp/g5-api-observations-review`. A byte comparison of every archived file to the
working copy found no differences during this review. Extended inspection covered
receive-pack admission, post-acceptance publication/notification consumers,
WorkspaceFlushService verification/completion, the dispatcher HTTP error consumer,
RepoStorage path selection, native process supervision, and existing structural
guard mechanics. Protocol execution remains subsequent scope; the changed
observations' interaction with its accepted effects is in scope.

Current bodies and footer entries were read for `g5.architecture.git_owner`,
`git_execution`, `git_ref_updates`, `git_native_runner`, `git_outcomes`, `git_retry`,
`git_locations`, `git_guard`, `content_reader`, `flush_owner`, `g5.git.receipt`,
`receipt.write-admission`, `network`, `network.cas`, `network.ref-scope`, and the
relevant minimal API, enforcement, abstraction and information-hiding principles.

## Initial concrete findings

### R2-1 — P2: repository discovery may substitute the parent repository

**Path:** `backend/g5-scm/src/native/observations.rs:25` (also the other three
observation command constructors), using `native/process.rs`'s `-C` construction.

The new observation operations rely on ordinary upward Git discovery without
establishing that the supplied owner path is itself the repository. If an existing
directory has lost its `.git` metadata and is beneath another repository, all four
operations can observe that ancestor instead. `RepoStorage::local_path` supplies a
path without checking repository materialization. The previous API helpers supplied
an explicit `GIT_DIR`/`--git-dir`, so they did not substitute the parent. This is a
regression in repository identity and failure classification, even though the
shared runner's `-C` behavior predates this increment.

**Evidence:** A real-Git fixture at `/tmp/g5-observation-review-2/repo` contains a
commit and an ordinary child directory `lost-store`. The exact new invocation
`git -C lost-store -c safe.directory=lost-store rev-parse --verify --quiet
refs/heads/main` returns status 0 and the parent's OID. The explicit `--git-dir
lost-store` invocation fails with status 128. Receipt verification can therefore
consult another repository's refs/objects instead of returning an operational
failure; mismatching parent state can also be reported as an invalid receipt.

**Rules:** `g5.architecture.git_locations`, `git_outcomes`, `g5.git.receipt`, and
the plan's requirement that repository failures not become proof observations.

**Repair:** Resolve/validate the exact supplied bare or worktree repository inside
SCM, preserving linked-worktree `.git` files, and prevent upward fallback for these
operations. Do not transfer repository-path reconstruction back to the API.

**Regression expectation:** Real native-operation tests must reject a non-repository
child of a valid repository even when the parent has the requested ref/OID; retain
passing bare, ordinary checkout and linked-worktree cases. An API flush test should
establish that such lost storage returns `FLUSH_UNAVAILABLE` and leaves both
completion and selected receipt unchanged.

### R2-2 — P2: broken refs are reported as ordinary absence

**Path:** `backend/g5-scm/src/native/observations.rs:30`.

`rev-parse --verify --quiet` exits 1 both for a missing ref and for certain repository
ref failures. The code maps every untruncated exit 1 to `Ok(None)` and ignores the
diagnostic. A loose ref containing `nonsense\n` produces `warning: ignoring broken
ref ...` and exit 1; a dangling symbolic ref similarly produces exit 1 with a
warning. Thus a broken mapped branch in flush verification becomes HTTP 409
`FLUSH_BRANCH_MISMATCH`, not an operational `FLUSH_UNAVAILABLE`. The dispatcher
preserves this deterministic proof code, which changes recovery interpretation.
The prior helper also conflated some lookup errors with absence; the explicit new
observation contract remains unimplemented for this case rather than this being
wholly new behavior.

**Evidence:** Real-Git exact-command probes distinguish absent, corrupt and dangling
refs in stderr but all return status 1. The actual source's branch returns `None`
for each. `symbolic_ref_target` happens to fail on the corrupt fixture, protecting
the push preflight, but flush `ref_tip` has no such check. A naive substitution with
`show-ref --verify --quiet` also returns 1 for these cases and is not a sufficient
repair.

**Rules:** `g5.architecture.git_outcomes` and the plan's explicit absence versus
repository-failure distinction; `g5.git.receipt` requires trustworthy proof.

**Repair:** Distinguish verified absent refs from invalid/unresolvable ref storage
with an appropriate typed/native check. Preserve ordinary absent refs and decide
dangling-symbolic handling explicitly; broken storage must be an error. Avoid
treating all stderr as failure because benign environment warnings are possible.

**Regression expectation:** Real corrupt-loose-ref and dangling-symbolic fixtures,
an ordinary missing-ref control, and an API completion check for operational error
without receipt advancement. Existing tests only remove `HEAD`, which gets status
128 and misses this counterexample.

### R2-3 — P2: a late observation error discards confirmed push effects

**Paths:** `backend/g5-api-routes/src/routes/git.rs:686`, `:83`, and the new
`current_ref_tip` adapter at `:1073`.

After native receive-pack has completed, `receive_pack` unconditionally awaits the
now deadline-bound ref observation with `?`, before consuming an `ok <ref>` report
already returned by Git. A timeout, malformed output or operational read failure
returns immediately: accepted refs remain changed, but the loop's confirmed
updates never reach queue insertion, branch events or reviewer notification. This
can also discard earlier accepted siblings accumulated in the loop. The public
error is rendered solely as the read operation's `Refused` result (`this invocation
made no Git changes`), without stating that the enclosing push has already changed
refs. The phrase is true of the read invocation but omits the established write
outcome needed by the caller.

This integration failure path partly existed with subprocess errors; introducing
bounded, typed observations adds deadline/malformed-output exits without adapting
the effect consumer. It is an affected observation integration defect, not a
request to migrate all protocol execution now.

**Evidence:** Source trace from successful `wait_with_output`, through parsed
report at line 676 and fallible read at 686, to side effects only after line 735.
No runtime fault-injection test was executed for this case.

**Rules:** `g5.architecture.git_outcomes`, `git_ref_updates`, the API observations
plan's retained accepted-push notifications, and `g5.architecture.pr_flow`'s accepted
ref signal consumed by publication.

**Repair:** Preserve and process already-confirmed report-status acceptance even
when a later observation fails; use ref inspection as fallback evidence only where
needed. Preserve admission/commit ordering and handle successful siblings. Any
returned late error must identify confirmed writes and unresolved observations,
with inspection guidance, rather than expose the read's effect category as the
whole push result. Add no retry persistence or automatic replay.

**Regression expectation:** A public receive-pack test should run a real successful
push while a selected post-receive observation fails (configured Git wrapper or
equivalent fault injection). Assert the actual ref, publication queue/event outcome,
and response evidence. Include a report-status success, a successful sibling of a
partial push, and a no-report-status unknown outcome; do not substitute fixture ref
mutation for the receive-pack owner transition.

## Commitment coverage and check results

| Commitment | Actual path and discriminating case | Result or gap |
|---|---|---|
| `git_execution`, `git_native_runner` | Four owned methods in `observations.rs`; API helpers delegate; stdin is private and concurrently joined with both output drains. | Source inspection supports consolidation and no public command/environment escape hatch. Existing runner cleanup ownership is retained. Full lifecycle tests were not executed by this reviewer. |
| `git_locations` | Storage-selected path passed into native operations; bare/worktree/linked fixtures inspected. | R2-1: unbounded upward discovery changes identity on missing metadata. |
| `git_outcomes` | Exact object-type record parsing, ref/ancestry results, `GitError::Native`, `FlushRouteError::Native`. | Object-type missing versus execution failure is well separated by source. R2-2 and R2-3 remain. Native negative-result branches also accept unexpected nonempty stdout; broader malformed-output cases are not covered by the new tests. |
| `git_retry` | Native operations and private supervisor; error propagation to existing flush lifecycle. | No new replay loop, queue, schema, worker or persistence in this increment. Existing dispatcher retry policy remains its own consumer. |
| `git_ref_updates`, `network`, `network.cas`, `network.ref-scope`, `receipt.write-admission` | Ref reads still occur after authorization/admission; symbolic refs rejected; exact old OID and immutable identical-replay branches preserved. | Source supports ordinary refusal semantics; new symbolic-ref API test reaches the real route and asserts unchanged target/no queue. Accepted-effect failure interaction is R2-3. Full authority/race matrices were not rerun. |
| `g5.git.receipt`, `flush_owner` | Route validates vector, exact immutable leaves, object types, ancestry before service completion/select. Service rechecks current incarnation/row and commits selection atomically. | New API test checks invalid cases, unchanged receipt selection, and subsequent valid completion. This is meaningful owner-path coverage, but was not executed here; corrupt-ref/location counterexamples absent. Existing service-alone and pin/prune limitations are unchanged scope. |
| `content_reader` | Only admission and flush proof consumers use the new methods; they return refs/types/ancestry, not application content. | No new authorized-content-reader bypass found. |
| `git_guard`, `minimal-api`, `enforcement`, `abstraction`, `information-hiding` | Existing syntax guard extended for flush and observation operations; required operation presence, helper rename, aliased/direct process bypass and test-only fixtures. | Source supports calibrated migrated-owner coverage. Remaining `git.rs` protocol owner is explicitly not fully guarded/migrated. New methods are sufficient narrow operations and private stdin support is justified by missing-object evidence; no unnecessary mechanism found. Guard binary was not run by this reviewer. |

Executed checks:

- `git diff --check f95fac40`: passed.
- Frozen snapshot comparison: all archived files exactly matched the reviewed copy.
- Real Git probes with the API-cleared environment reproduced R2-1 and R2-2;
  `/tmp/g5-observation-review-2/git-probes.json` retains outputs.
- Follow-up `show-ref --verify --quiet` probe rejected that simplistic repair for
  R2-2; outputs are in `show-ref-probes.json`.

Verification limits: no Cargo compilation/unit suite, database/public API test,
runtime deadline/drop cancellation test or source-guard executable was run by this
reviewer, avoiding concurrent build/database contention as assigned. Existing and
new test assertions were inspected, not counted as passing runtime evidence. The
parent's separately executed checks may supplement this report after initial
findings are saved; they do not erase these counterexamples.

## Proposed metadata reconciliation

After coherent repairs and rechecks, update the scoped current implementation
assessments for `git_execution`, `git_native_runner`, `git_outcomes`, `git_locations`,
`git_guard` and relevant receipt/network consumers. In particular,
`git_native_runner` still says API Git/flush migration is wholly outstanding and
should distinguish these migrated proof reads from subsequent protocol execution.
Retain the wider migration and receipt-service/pin-prune gaps. No governing meaning
amendment or design-scope expansion is needed to fix the three findings.
