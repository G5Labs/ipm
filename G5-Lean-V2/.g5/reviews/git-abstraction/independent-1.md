# Independent review 1 — initial Git abstraction slice

Reviewed the uncommitted implementation against `3f7d2874`, including the untracked
`backend/g5-scm/src/resolution.rs`. Read-only discovery; no production or ontology
changes, no delegation, and no peer findings or implementation-session explanation
were consulted. The pinned design and its initial-slice amendment were inputs.
All eight files matched `/tmp/g5-git-abstraction-initial-review.sha256` at completion.

Read the saved request, the complete design beginning with **Ontology changes and
governance**, governing Git commitments and their metadata, authorized-reader,
complete-operation/dependency, error-evidence/recovery, redaction and review-guard
commitments. The design's embedded ontology changes are present in the current
ontology; this implementation snapshot changes no ontology content. Its metadata
still requires the main review's scoped implementation/evidence update.

## Findings saved before sharing

### F1 — P1: Create-only pins break the existing provider-resolution retry path

- Changed location: `backend/g5-scm/src/resolution.rs:111–129`, consumed at
  `backend/g5-api-core/src/services/git_conflict_resolution.rs:200`.
- The new method refuses every partial pair or differing existing pair. That is
  appropriate for create-or-confirm, but the provider-resolution caller supports
  a different, necessary operation: preparing the same run after a failed enqueue
  or SQL commit. `remote_integrations/resolution.rs:20–24,33–58` locks the existing
  `job_merge_runs` row, accepts `resolution_task_id = NULL`, and passes the same
  `input.run_id`. `remote_integrations/merge.rs:1975–1998,2117–2139` refreshes the
  provider/local inputs before that retry. A first attempt can leave Git pins while
  its SQL transaction rolls back. A partial pair or later head advance now makes
  every ordinary Start retry return Conflict instead of recovering.
- There is already a discriminating public-path regression:
  `backend/g5-api/tests/remote_repo_tracking_integration.rs:3373`,
  `resolution_retry_recovers_after_enqueue_or_commit_rollback_and_remote_advance`.
  It injects enqueue/commit rollback, establishes absent committed tasks and retained
  pins, also simulates one missing pin, advances both tips, and requires successful
  retry with exactly one task and the refreshed pin pair. Source inspection proves
  the new refusal branch contradicts those assertions; I did not run this DB test
  concurrently with the main agent's tests.
- The initial-slice design statement at `.g5/ontology-plan-git-abstraction.md:589–590`
  that the live caller only creates fresh IDs is false for this material caller.
- Governing: `g5.architecture.git_ref_updates` explicitly permits checked replacement;
  `g5.architecture.git_retry` retains existing recovery, and
  `g5.architecture.commands`/`g5.architecture.recovery` retain owner authority.
- Proposed repair: preserve create-or-confirm for genuinely fresh runs and provide
  the existing SQL resolution owner a typed, expected-state replacement/repair
  operation under its no-committed-task admission. Check the entire observed pair,
  including absent sides, under both ref locks before mutating. Do not restore an
  unchecked overwrite or add storage. Correct the saved design assumption and run
  the existing integration regression plus stale-expectation and committed-task
  preservation cases.

### F2 — P1: The uncertainty and retry guidance never reaches API users

- Changed location: `backend/g5-scm/src/resolution.rs:46–50,139–145`.
- `ResolutionPinError::Uncertain` contains a `G5Error::GitError`, and its automatic
  conversion drops the typed distinction. `pin_inputs` uses `?`, then its callers
  convert that `G5Error` into `AppError`. The real response path,
  `backend/g5-api-core/src/errors/mod.rs:128–131` through
  `backend/g5-contracts/src/error.rs:568–575`, intentionally serializes every
  `GitError` as code `INTERNAL_ERROR`, message `An internal error occurred`, and
  empty details. The operation, refs, uncertainty and instruction to inspect
  current state are all omitted. Non-conflict pre-write refusals are likewise
  mapped to this generic response. This prevents the agent/human from making the
  informed retry decision specifically required by the user.
- Runtime evidence: an isolated probe using the current `G5Error::to_error_response`
  with the new uncertainty message returned exactly
  `message: An internal error occurred; code: INTERNAL_ERROR; details: {}`.
  Probe source/results:
  `/var/folders/yf/bc92js5n0bjg59_gc7ztxqjr0000gn/T/g5-git-review-one-error-u0z4vnmf/`.
  This checks the real error mapper, not the full HTTP route. The initial standalone
  link lacked OpenSSL's library path; adding the existing Homebrew library path
  resolved it. No shared build outputs were written.
- Governing: `g5.architecture.git_outcomes`, `g5.architecture.git_retry`,
  `g5.data.outcome-evidence`, while preserving `g5.security.credentials.redaction`.
- Proposed repair: preserve an explicitly public, sanitized outcome through the
  existing API error envelope, including refused/unknown classification, authorized
  target and useful next action. Do not expose arbitrary internal Git errors or
  globally disable their sanitization. Add an assertion at the HTTP/error-envelope
  consumer; checking `error.to_string()` alone misses this failure.

### F3 — P2: The production guard treats test fixtures as production and as required operations

- Changed location:
  `backend/g5-api/tests/authoritative_workspace_reader_guard.rs:64–76,98–103`.
- The AST walk enters `#[cfg(test)]` modules and `#[test]` functions unconditionally.
  A legitimate test fixture that opens a Git repository inside any migrated owner
  fails the new production-dependency check. Conversely, a test-only function named
  `ref_oid`/another required operation satisfies the required-production-target
  check after the real operation disappears.
- Isolated runtime probe extracted the exact checked function from the snapshot.
  `fn read() {} #[cfg(test)] mod tests { fn fixture() { let repo =
  git2::Repository::init_bare(path); } }` returned a migrated-owner Git violation.
  `#[cfg(test)] mod tests { fn read() {} }` with required `read` returned no violations.
- Governing: `g5.architecture.git_guard` explicitly exempts tests/fixtures from
  production dependency checks and requires missing targets to fail;
  `g5.quality.review.guards` requires valid variations to remain allowed.
- Proposed repair: scope dependency checks and required operation discovery to
  production syntax; add both test-fixture and test-only-namesake calibration cases.

### F4 — P2: A normal function-value call bypasses the new maintenance-reader guard

- Changed location:
  `backend/g5-api/tests/authoritative_workspace_reader_guard.rs:78–97`.
- The check looks only at the immediate function child of a `call_expression`.
  Outside the maintenance owner,
  `fn read() { let reader = GitRepo::head_ref_commits; reader(&repo); }`
  produces no violation, even though the banned operation is explicit in the source.
  This is an ordinary Rust refactor of the UFCS call that the calibration test
  claims to reject; no dynamic dispatch or macro expansion is needed.
- Isolated runtime probe of the exact guard returned `[]` for that fixture.
- Governing: `g5.architecture.git_guard`, `g5.design.enforcement` and
  `g5.quality.review.guards`.
- Proposed repair: cover references to these typed-reader operations as values as
  well as direct calls, with a matching negative fixture. Preserve legitimate
  maintenance-owner use. Source guards need not solve arbitrary dynamic mediation
  to catch this explicit operation reference.

Guard probe source/results for F3/F4:
`/var/folders/yf/bc92js5n0bjg59_gc7ztxqjr0000gn/T/g5-git-review-one-ehevew1_/`.
The probe used read-only existing parser libraries, with its binary and files in an
isolated temporary directory. Compile and all three counterexample assertions passed.

## Commitment coverage and checks

| Commitment | Actual path and case | Result / evidence limit |
| --- | --- | --- |
| `g5.architecture.git_execution`, `g5.architecture.commands` | Three migrated owners now use backend-independent diff, ref-read and resolution operations; replay arguments remain with the task policy owner. | Preserved in inspected slice. Full platform migration is explicitly pending. |
| `g5.architecture.git_ref_updates`, `g5.architecture.git_owner` | Resolution pair checks both refs under locks; collision, partial pair, symbolic refs, invalid commits, lock contention and concurrent writers have focused tests. | Fresh-run safety is sound on inspection. Existing provider owner requires the missing checked repair/replacement path, F1. Runtime suite left to main. |
| `g5.architecture.git_outcomes`, `g5.data.outcome-evidence` | Resolution transaction commit failures conservatively report unknown effects, not rollback; earlier failures are marked refused. | Library distinction inspected; public evidence is lost, F2. Library tests check Display only. |
| `g5.architecture.git_retry`, `g5.architecture.recovery` | No new tracking tables, columns, workers or automatic replay appear in the diff. Existing provider retry owner and regression inspected. | No new retry persistence/replay. Existing recovery regresses, F1; informed retry blocked by F2. |
| `g5.architecture.projection_owner`, `g5.architecture.content_reader` | `projection_convergence::{ref_oids,ref_oid}` call shared peeled readers. `run_repo` and `apply_projection` retain authority/checkpoint logic. | Semantically equivalent read migration inspected: sorted heads only; absent ref distinct from missing mirror, dangling symref and noncommit target. Focused tests cover these cases. No content-reader bypass introduced. |
| `g5.design.abstraction`, `g5.design.minimal-api`, `g5.architecture.dependencies` | New public enums and operations hide git2; private repo access is `pub(super)`; no upward dependency added. | Sufficient small API for diff/reads/fresh pins. Replacement omitted on an incorrect caller assumption, F1. |
| `g5.architecture.git_locations` | Both API users still get paths from `RepoStorage`; bare/worktree opening stays in `GitRepo`. | Preserved in this slice; other location kinds remain migration work. |
| `g5.security.credentials.redaction` | Resolution failure formatting calls the existing scrubber; inputs otherwise contain UUID refs and commit OIDs, with no new credential channel. | No new credential exposure identified. Fixing F2 must retain public redaction. Exhaustive configured-secret behavior not exercised. |
| `g5.architecture.git_guard`, `g5.design.enforcement`, `g5.quality.review.guards`, `g5.quality.review.cases` | Existing reader guard retained; new AST guard checks migrated owner access, direct maintenance-reader use, missing targets and helper renames. | Existing intended fixtures inspected. Three isolated counterexamples prove F3/F4; source checks do not establish runtime authority. |
| Full-context semantic diff | `semantic_record_diff_from_text_with_budget` passes supplied bytes with `DiffContext::Full`; parsing and output budget remain unchanged. Ordinary helper still uses default options. | Preserved on inspection; focused new test discriminates omitted full context, trailing unterminated line and unchanged files. No repository reads added. |

Additional checks: `git diff --check` passed; all snapshot hashes passed. I did not
run Cargo or shared DB tests while the main agent owned those resources. No browser,
provider, guest or whole-platform outcome is claimed. Native runner, provider/transport,
credentials migration, native-only dependency closure and remaining backend consumers
are planned work rather than defects in this initial slice.

Implementation status: initial slice has the four findings above. Verification status:
source/caller review and isolated error/guard probes complete; the existing provider
rollback/retry integration case remains the key unexecuted public-path check for main.

## Bounded repair recheck

Rechecked the four consolidated findings, the new public error contract and provider
replacement composition after the main agent's coherent repair. The original findings
above remain unchanged. This pass read the consolidated report and current source;
production/tests/ontology were not edited and no shared builds or database tests ran.

| Finding | Repair and recheck | Disposition |
| --- | --- | --- |
| F1 / R1 | `ResolutionPinState` captures both optional direct targets; replacement checks exact equality under both locks before the desired-pair no-op or mutation. Provider preparation reads that state only after the existing row lock has established no committed task. Workspace resolution explicitly uses no replacement state. New unit cases cover complete/partial preparation, stale desired-pair replay and an intervening change on either side. | Source repair is correct. The existing public rollback/advanced-head regression is still pending in the main agent at this recheck; do not claim end-to-end recovery until it passes. |
| F2 / R2 | The lossy `From<ResolutionPinError>` conversion is removed. `resolution_pin_error` now emits safe `GitOperationFailed` details with repository, refs and refused/unknown effect. Real `AppError` serialization tests check those details, omission of backend paths/tokens and unchanged generic GitError sanitization. | Original total loss at the API envelope is repaired. A narrower human-facing/cause gap remains below. |
| F3 / R3 | Guard traversal skips cfg(test), test and Tokio test items; skipped functions cannot satisfy required operations. Attribute state resets before later production items. | Closed for the reported counterexamples by source inspection and an isolated exact-source probe. |
| F4 / R4 | Guard inspects field/scoped member references, including function-value references. | Closed for the reported counterexample by an isolated exact-source probe. |

The isolated guard recheck compiled the current function with read-only cached parser
libraries, putting all outputs in
`/var/folders/yf/bc92js5n0bjg59_gc7ztxqjr0000gn/T/g5-git-review-one-recheck-owsul77k/`.
Four assertions passed: allowed cfg(test) fixture, rejected test-only namesake,
rejected function-value reader bypass, and allowed Tokio test-only reader fixture.
`git diff --check` passed. Main reports 165 SCM tests, both conflict-resolution
tests, 23 change-graph tests and four reader guards passing; those are supplied
main-agent results, not independently executed here.

### Remaining R2 gap — P2: distinguish uncertainty in the displayed message and retain a safe failure cause

`backend/g5-api-core/src/services/git_conflict_resolution.rs:237–248` now preserves
the effect but discards every cause. `backend/g5-contracts/src/error.rs:268` gives
refusal and uncertainty the identical message, “Git operation pin_resolution_inputs
did not complete; inspect the requested commits and refs before deciding how to
retry.” The cause/stage is also absent from `error_details` at lines 493–504.

This matters at the actual human consumer: `frontend/src/lib/api.ts:114–136` retains
the server message as `Error.message`, and both resolution error displays in
`frontend/src/components/workspace/BranchTrackingPopover.tsx:521,610` render only
`start.error.message`. The human therefore cannot distinguish “this invocation
changed no refs” from “one or both refs may already have changed.” Nor can an API
agent distinguish a stale expected pair from an invalid/missing input object, a
ref lock, or a storage failure: they all produce the same refused envelope. The
new test verifies `details.effect` and the word “inspect,” so it does not catch
either omission.

`g5.architecture.git_outcomes` requires errors to identify what failed/remains
unknown and a useful corrective action; the user's constraint requires clear
errors for agent/human retry decisions. Preserve that distinction in the public
message itself and carry a safe typed failure category or stage selected by the
operation. Expected/observed OIDs for a stale pair can be safe public evidence;
raw backend diagnostics and filesystem paths should remain internal. Verify the
serialized message for both effects and at least two refusal causes, or verify
the actual displayed result. This is a bounded completion of R2, not a request to
build a general error framework or change generic GitError sanitization.

No other defect was found in the repaired replacement composition or added public
variant. No new schema/retry storage or automatic replay was introduced. Projection
read and diff behavior remain unchanged by the repair.
