# Independent API Git observation review 3

Initial read-only discovery, saved before sharing findings. Reviewed against baseline
`f95fac40` and the frozen inventory in `/tmp/g5-api-observations-review`; all ten
snapshot files matched the working sources when checked. The new untracked
`backend/g5-scm/src/native/observations.rs` is included. No production or ontology
edits, delegation, main-review findings, or peer findings were used.

Inputs: `AGENTS.md`, ontology-review, authoring format and design-review guides,
the original saved request, the complete pinned Git-abstraction design beginning
with its governance section, its API Git proof observations increment, and actual
governing ontology bodies/footer entries. The design adds no ontology meaning in
this increment; its existing broad migration/evidence limits remain applicable.
Protocol process migration is subsequent scope. Changed observation interactions
with that existing protocol owner are included here.

## Initial findings

### R3-1 — P2: Repository discovery can return evidence from an ancestor repository

**Path:** `backend/g5-scm/src/native/observations.rs:25,55,91,123`, using
`native/process.rs:156–160`; affected consumer
`backend/g5-api-routes/src/routes/workspace_flush.rs:148–180`.

The migrated observations use `git -C <storage path>` without constraining discovery
to that repository. If the selected directory exists but has no valid Git metadata
and an ancestor is a repository, Git returns the ancestor's refs, objects and
ancestry. `RepoStorage::local_path` supplies the path without validating repository
presence. The old object/ancestry commands bound `--git-dir` to the selected storage
path and failed in this case. This is a new behavioral regression: an unavailable
selected repository can supply apparently valid proof from different storage.

**Evidence:** Executed the exact new command forms with cleared API environment in
an empty `repository/storage/repo-id` directory below a valid repository. Ref lookup
returned the ancestor's main OID, batch object inspection returned `commit`, and
ancestry returned success. The old explicit `--git-dir` object command returned 128,
`not a git repository`. See `/tmp/g5-observation-review-3/probe-results.json`.
This reproduces subprocess semantics, not a complete API/DB exploit.

**Obligations:** `g5.architecture.git_locations`, `git_outcomes`,
`g5.git.receipt`, and the increment's rule that repository failures are explicit
errors rather than usable proof.

**Repair/check:** Resolve a Git directory from the exact supplied repository root
inside SCM and reject an invalid root, while retaining bare, ordinary worktree and
linked-worktree support. Do not allow upward discovery to select another repository.
Add a native regression with a non-repository child of a valid repository; every
observation must fail. At the flush endpoint, invalid storage must return unavailable
without completing/selecting a receipt even if the ancestor contains matching proof.

### R3-2 — P2: `ref_tip` uses revision guessing where proofs require an exact ref

**Path:** `backend/g5-scm/src/native/observations.rs:24–39`; consumer
`backend/g5-api-routes/src/routes/workspace_flush.rs:151,169`.

`check-ref-format` establishes syntax, but `rev-parse --verify --quiet` still applies
Git's revision-name expansion. If the requested
`refs/g5/flush/workspace/flush/snapshot` is absent but
`refs/heads/refs/g5/flush/workspace/flush/snapshot` exists, `ref_tip` returns the latter
OID. The same holds for the branch leaf and missing mapped branches. The flush
verifier can therefore count different refs as its immutable receipt leaves.
This weakness was inherited from the old helper; it remains a concrete conformance
defect in the new public observation operation and its directly migrated proof path,
not a newly introduced ref-authority policy.

The same primitive also conflates a broken loose ref with absence: a ref containing
`garbage` produces exit 1 plus `warning: ignoring broken ref ...`, which line 30 maps
to `Ok(None)`. Thus the new explicit repository-failure distinction is incomplete.
The API corruption test only removes HEAD, a different failure that exits 128.

**Evidence:** Real Git probes created both shadow receipt leaves; exact `show-ref
--verify` found neither requested leaf, while the new `rev-parse` form successfully
returned the shadow OID. A corrupt loose ref returned exit 1. Results and a runnable
probe script are in `/tmp/g5-observation-review-3`.

**Obligations:** `g5.git.receipt`, `g5.git.receipt.write-admission`,
`g5.architecture.git_outcomes`, and the full-ref/error meaning in the pinned increment.

**Repair/check:** Use an exact reference lookup with a reliable absent/corrupt
distinction, preserving supported symbolic-ref resolution. Add native shadow-ref and
corrupt-ref cases, plus an API proof case where both legitimate receipt leaves are
missing and only shadow branches exist. Completion must refuse and leave receipt
selection unchanged. Real absence remains an ordinary `None`; broken storage is an
operational error. Revisit existing inherited cases rather than weakening the design.

### R3-3 — P2: New post-push observation failures discard already confirmed effects

**Path:** `backend/g5-api-routes/src/routes/git.rs:675–686,735–767`, changed
`current_ref_tip` at 1073–1082, and new `GitError::Native` at 86–89.

After receive-pack has completed, the loop unconditionally performs the newly bounded
ref read before consulting its already captured `report-status` result. A deadline,
invalid observation or repository-read failure returns immediately through `?`.
Even when Git explicitly reported `ok <ref>`, the route drops accepted-update
collection, publication enqueue, branch events and reviewer notifications. The
returned native observation error describes only a read that made no changes; it
does not preserve the successful push or identify the missing refresh/recovery.
In a multi-ref push, a read failure for a later ref also discards earlier confirmed
updates still buffered in the vector.

The fallible caller pattern predates this increment, but the migrated helper adds
deadline/malformed-output failure paths and a new effect-bearing error surface.
This is a material changed interaction, not a request to migrate protocol streaming.

**Evidence:** Source trace from the new helper/error mapping through the unchanged
post-receive consumer. Runtime post-acceptance fault injection was not executed;
the existing native deadline test covers only a standalone read, and the new API
test covers pre-receive symbolic-ref refusal.

**Obligations:** `g5.architecture.git_outcomes`, `git_native_runner`,
`git_ref_updates`, `g5.governance.pr.follow-push`, and the increment's explicit
preservation of accepted-push notifications. The follow-push contract permits manual
recovery but requires identifying incomplete work and directing Update PR.

**Repair/check:** Retain and process confirmed protocol acceptance independently of
optional later ref observations; use reads where acceptance genuinely needs further
evidence. Report unknown/partial push outcomes and the remaining action when a needed
read fails, without overwriting established success. Inject a failure only into the
post-receive observation through the public push endpoint. Assert actual branch OID,
publication/PR continuation and response guidance; include a later-ref failure after
an earlier confirmed acceptance. This can be repaired within existing route ownership.

### R3-4 — P2: The full-OID guard accepts SHA-256 abbreviations

**Path:** `backend/g5-scm/src/native/observations.rs:90–95,121–126,158–165`;
affected flush fields at `routes/workspace_flush.rs:132–154,174–178`.

Accepting either 40 or 64 hex characters independently of repository object format
does not ensure a full OID. In a SHA-256 repository, Git treats a 40-character prefix
as an abbreviation, and both object inspection and ancestry accept it. For flush
fields without a direct ref-string comparison (`branch_oid`, and detached/foreign
`local_head_oid`), this can admit an abbreviated persisted identity despite the new
API/design promising full object IDs. The prior route length check already had this
weakness; the new supposedly validated SCM boundary carries it forward.

**Evidence:** In a real SHA-256 repository, submitting the first 40 characters of the
64-character commit OID to the exact new batch-check returned `commit`. The checked-in
SHA-256 test uses full IDs and therefore does not discriminate this case. No complete
SHA-256 flush/restore API path was executed.

**Repair/check:** Verify input length against the selected repository's object format,
or include the resolved object identity in the private response and require exact
normalized equality. Preserve full SHA-1 and SHA-256 cases and reject abbreviated
SHA-256 object/ancestry inputs. At the receipt boundary, abbreviated identity must
not complete/select a receipt. If cross-format identity conversion is intended, it
needs an explicit contract; current implicit prefix lookup is not such a conversion.

## Checks and coverage ledger

Executed without Cargo builds or DB tests, using existing compiled artifacts and
`TMPDIR=/tmp/g5-observation-review-3`:

- `target/debug/deps/g5_scm-08d6ca130345ed39 native::tests::api_observ --test-threads=1`:
  3 passed, covering bare/worktree/linked-worktree SHA-1/SHA-256 reads, ordinary
  absent/symbolic refs, four object kinds, ancestry, invalid input/output, and
  deadline/cancel/drop cleanup with no retry.
- The same binary's exact `native_input_and_output_are_drained_concurrently` test:
  1 passed; a 256 KiB stdin payload and large stdout/stderr do not deadlock.
- `/tmp/g5-observation-review-3/probes.py`: real Git 2.50.1 edge probes above and a
  frozen-source comparison with no mismatches. macOS Git emits an unrelated cleared-
  environment temporary-directory warning; findings depend on status/OID output,
  not treating every stderr warning as fatal.
- Existing `authoritative_workspace_reader_guard` binaries contained only two older
  tests. The requested new guard matched zero tests, so no guard runtime pass is
  claimed. Its new source calibration was inspected.

| Commitment | Actual path and discriminating case | Result / remaining limit |
|---|---|---|
| `git_execution`, `g5.design.minimal-api`, `abstraction`, `information-hiding`, `g5.architecture.dependencies` | Four named operations, owned results, private stdin/argv/environment; no new dependency edge or raw process API | Preserved by inspection; sufficient narrow API, no unjustified new subsystem |
| `git_locations` | Storage-owned path, bare/worktree/linked worktree plus invalid nested path | Valid layouts pass; R3-1 violates exact repository selection |
| `git_outcomes` | Missing object versus process error; exact/missing ref; post-write read failure | Ordinary and malformed-output tests pass; R3-2/R3-3 remain |
| `git_native_runner`, working-state preservation | Private supervised stdin, concurrent output, deadline/cancel/drop; before/after refs/index/HEAD/config/working files | Four native tests pass; cancellation at HTTP caller/admission lifetime not runtime-tested |
| `git_retry` | No retry loop/state/schema added; lifecycle retries remain with existing owners | Preserved by source and native one-attempt assertion |
| `g5.git.receipt`, `receipt.write-admission` | Route vector/ref/type/ancestry checks before service atomically completes/selects; invalid cases plus corrected retry | New API test is discriminating by inspection; unexecuted here; R3-1/R3-2/R3-4 affect proof input meaning |
| `g5.git.network`, `network.cas`, `network.ref-scope`, `git_ref_updates` | Authorization/admission precede observations; missing mapped ref still cannot create; symbolic refs refused; native CAS retained | Ordering preserved by inspection; public push/flush regression suites not run by this reviewer; R3-3 affects accepted outcomes |
| `g5.governance.pr.follow-push` | Confirmed receive result → accepted vector → enqueue/events/reviewer updates | Normal path retained by inspection; R3-3 on read failure |
| `git_guard`, `g5.quality.review.guards` | New flush/observation owner existence, renamed private helpers, aliased/direct process bypasses, test fixtures | Calibrated source cases present; runtime guard unavailable in existing binaries; protocol file deliberately not claimed closed |
| `g5.architecture.content_reader`, `projection_owner`, `flush_owner`, `commands` | Only proof owners call new observations; reader source selection/pinning unchanged; service retains final receipt write | No new content-reader bypass; pre-existing route verification/service boundary remains explicitly documented |
| `g5.security.credentials.redaction` | API environment cleared; errors use shared scrubber and typed operation/target; no credential input added | Preserved by inspection; no new secret-specific API diagnostic injection test run |

## Proposed reconciliation and limits

Fix the applicable defects, then update existing footer assessments for
`git_execution`, `git_native_runner`, `git_outcomes`, `git_locations`, `git_guard`,
`git_retry` and the affected receipt/push owners to include the observed increment
without claiming full protocol or platform migration. Record unresolved failure cases
directly in their applicable entries. The actual footer still describes API Git/flush
as remaining migration work and has not yet incorporated this increment's evidence;
that is expected pending main consolidation, not a request to change valid intent.

The design already states the needed exact repository/proof/error obligations; no
contract weakening or new retry storage is justified. Preserve the broad design's
scope distinctions and existing accepted MVP object-visibility limitation.

This review did not run full Cargo builds, DB-backed API suites, HTTP cancellation,
post-acceptance failure injection, Linux checks or deployed Git. Main was assigned
the build/DB work. Runtime evidence above proves native subprocess cases, not complete
receipt persistence or successful-push continuation under failure. Review findings
and unexecuted evidence remain separate.

## Disclosed repair source recheck

This is a bounded follow-up after reading main's O1–O6 consolidation, not a fresh
independent discovery. Raw findings above remain unchanged. I inspected the repaired
observation implementation, receive-pack consumer, new native/API regressions and
flush error mapping. No build, DB test or new runtime check was performed in this
recheck; main owns current execution evidence.

| Consolidated finding | Recheck assessment |
|---|---|
| O1 / R3-1 | Addressed in source. Every observation now supplies an explicit `--git-dir` relative to the selected `-C` path, preventing parent discovery. The added nested invalid-storage native cases cover all four operations; the public flush fixture relocates matching proof to the ancestor and checks unavailable/no receipt advancement. Existing bare/worktree/linked-worktree cases remain. |
| O2 / part of R3-2 | Addressed for the established counterexamples. Native evidence now rejects storage-error diagnostics, truncated/malformed negative responses, and unresolved symbolic refs. Native and public flush cases cover broken refs and compressed objects; benign Apple environment warnings retain a positive control. This is an explicit diagnostic interpretation, not a proof against all future Git diagnostic formats. |
| O3 / part of R3-2 | Addressed in source. `for-each-ref` records are matched by exact ref-name equality; descendants cannot satisfy the requested identity, and revision-name expansion is no longer used. Native shadow-ref and public missing-receipt-leaf controls discriminate the original issue. |
| O4 / R3-3 | Addressed in the scoped read-failure path. Captured `ok <ref>` evidence bypasses the optional read, fallback failures are retained while later refs are processed, admission commits before confirmed continuation, and the final error reports Partial/Unknown with current-ref inspection and Update PR guidance. The real receive-pack test exercises confirmed, accepted-sibling-plus-read-failure and no-report unknown outcomes, asserting branch OID, queue/event effects and one failed observation. This source review does not establish executed PR worker/provider completion or other pre-existing enqueue/DB-failure semantics. |
| O5 / R3-4 | The native acceptance defect is addressed: object and ancestry operations check the selected repository's object format before lookup, so SHA-256 prefixes cannot reach Git's abbreviation resolution. Both valid object formats and forbidden prefixes are in the native test. One new HTTP error-classification defect remains below. |

### R3-R1 — P2: Wrong-width receipt OIDs are reported as temporary Git unavailability

The O5 repair introduces a caller-invalid input outcome at
`native/observations.rs::check_object_format`: a 64-digit value in a SHA-1 repository
or 40-digit prefix in a SHA-256 repository returns `FailureKind::InvalidInput`.
The route's `valid_oid` deliberately accepts both widths, and `require_commit`
propagates that new error through the blanket `From<NativeGitError>`. Consequently
`workspace_flush.rs:219–227` returns HTTP 503 `FLUSH_UNAVAILABLE` for a permanent
invalid proof. For a SHA-1 receipt with a 64-hex `branch_oid`, the previous
`cat-file -t` check produced the ordinary proof refusal `FLUSH_NOT_COMMIT` / 409.
The repository is available; reissuing the same receipt cannot fix the input.

This is a concrete source-traced error mapping regression introduced by the repair,
not a remaining acceptance bypass: the receipt still does not complete. It conflicts
with the increment's separation of invalid proof and operational failure and its
preservation of flush proof responses. The dispatcher preserves the HTTP status and
only selected error codes from the response, so callers receive the wrong recovery
category. I did not execute this HTTP case or claim a specific infinite retry loop.

**Repair/check:** At the caller-owned OID proof check, translate the SCM
`InvalidInput` result to the appropriate existing proof refusal rather than the
unavailability branch; retain 503 for repository/process failures. Avoid broadly
reclassifying invalid internal ref/setup state as caller error. Add a public flush
case using a 64-hex `branch_oid` against its ordinary SHA-1 fixture: expect a 409
proof code and unchanged receipt selection, then prove the corrected receipt still
completes. The existing full-format native controls must stay green.

Apart from R3-R1, this bounded source recheck found no further concrete defect in
O1–O5's repaired paths. Closure still requires main's current native/API execution
results; previous passing binaries predate these repairs and are not reused as proof.
