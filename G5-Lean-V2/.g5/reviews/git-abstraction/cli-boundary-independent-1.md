# CLI boundary independent review 1 — initial findings

Fresh independent, full-scope, read-only discovery of the frozen CLI/semantic increment on `design/git-abstraction`, base `8a856ad3`. Reviewed the supplied request, `.g5/ontology-plan-git-abstraction.md` beginning with **Ontology changes and governance**, the scoped actual diff in `/tmp/g5-cli-boundary-review/tracked.patch`, untracked `backend/g5-scm/src/native/checkout_queries.rs`, current governing ontology and affected metadata. Applied `ontology-review/SKILL.md` and its design review guide. No author explanation or peer report was supplied or read. No production/design/ontology/test edits were made. Probe resources are confined to `/tmp/g5-cli-review-1`; this report is the only workspace write.

This is an increment review, not completion approval for the entire Git abstraction plan. Remaining host, guest, and provider migrations and the explicitly parked object-confidentiality extension are outside this increment. No defects below require a new retry mechanism or persisted state.

## Initial actionable findings

### F1 — P2: Newly migrated Doctor, Spec and Job operations are outside signal cleanup

**Paths:** `backend/g5d-cli/src/doctor.rs:465`; `backend/g5d-cli/src/spec.rs:1245` and `:1902`; `backend/g5d-cli/src/job.rs:250` and `:320`; material consumer `backend/g5d-cli/src/process.rs:71–91`; `backend/g5-scm/src/native/process.rs:502`.

The migrated native operations run their children in separate process groups. The process entry point installs `native_git::with_interrupts` only for Clone, Add, Push and Workspace. Doctor, Spec, and Job now call this runner but are absent from that match. Ctrl-C/SIGTERM therefore kills the CLI without cancelling/reaping these children: unlike the prior ordinary child process, the native process no longer receives the terminal group's signal. For Job this includes a mirror push which may still change its destination after the caller exits.

**Evidence:** source trace from each changed call to the entry point and shared runner; a probe linked to the current SCM rlib used the actual `version_blocking` operation with a delayed fake Git executable. Sending SIGINT to the calling process group produced caller exit `-2`; the native child remained alive and wrote its completion marker after caller exit. `/tmp/g5-cli-review-1/{signal_probe.py,signal.log}`. This runtime probe exercises SCM process-group behavior, not the complete shipped Doctor/Job dispatch; dispatch omission is source evidence.

**Governing:** `g5.architecture.git_native_runner`, `g5.architecture.git_outcomes`, design “Remaining CLI mechanics and semantic identity” preserves existing cancellation, and cleanup ownership.

**Proposed repair/check:** include all new native-consuming command paths in process-level cancellation ownership (including Spec invocations reached through their command owner). Add process-entrypoint signal tests for at least Doctor/Spec and the Job mirror path; assert child/descendant exit before CLI exit and no delayed mutation. Preserve unrelated dispatcher/host signal owners.

### F2 — P2: Optional default-branch discovery rejects a previously valid native transport

**Paths:** `backend/g5-scm/src/native/checkout_queries.rs:29`; `backend/g5d-cli/src/add.rs:500–514`; reused validator `backend/g5-scm/src/native/mod.rs:919–980`; clone consumer `backend/g5d-cli/src/add.rs:119–140` / `g5-scm/src/repo.rs::clone_backend`.

`default_remote_branch` reuses a validator which permits HTTP(S), SSH and local files but rejects the native `git://` transport. Previously `infer_remote_default_branch` directly ran `git ls-remote --symref` and then allowed the libgit2 clone to be authoritative. Now `g5d add git://host/repo.git` without `--branch` returns `InvalidInput` before cloning because only `FailureKind::Exit` is treated as optional discovery failure. The same supported clone request with an explicit branch skips this new refusal. There is no contract in this increment restricting clone transport to HTTP(S)/SSH/file; the design explicitly says preserve native transport variants when moving probes.

**Evidence:** current SCM public-path probe `/tmp/g5-cli-review-1/probe default git://localhost/repository.git` returns `InvalidInput` with “must use HTTP(S), or an unauthenticated local file”; saved in `transport.log`. Source inspection establishes the new hard failure at Add and unchanged clone implementation. A live `git daemon` clone was not executed.

**Governing:** `g5.architecture.git_execution`, `g5.design.abstraction`, the design's mechanism-preservation and native-transport requirements. Credential prohibition does not require refusing credential-free `git://` discovery.

**Proposed repair/check:** give this observation an operation-appropriate supported transport policy, retaining credential and destination checks. Exercise Add against a local Git daemon with default branch discovery, plus HTTP/SSH/file variants and forbidden credential-bearing destinations; do not repair this by making every genuine malformed/destination error invisible.

### F3 — P2: CLI/semantic native guard permits configured Git executable bypasses

**Paths:** `backend/g5-api/tests/authoritative_workspace_reader_guard.rs:175–185,200–219,1288–1302`.

For the newly covered CLI files such as Spec, Doctor, Job, Add, and for semantic code, `native_owner` is false. The new special check only rejects a constructor's argument when its exact source is `"git"`. `Command::new(git_path)`, `Command::new(GIT_PROGRAM)` with `const GIT_PROGRAM: &str = "git"`, and equivalent configured native execution therefore pass. The guard's stronger process-path/import logic only covers the earlier three CLI files. This leaves exactly the configured-program bypass the pinned design requires guards to reject. It also provides whole-file freedom for new configured Git spawns in newly migrated owners, rather than operation-specific non-Git exceptions.

**Evidence:** extracted the actual current `native_process_path`, `imported_paths`, and `typed_git_violations` implementations unmodified into `/tmp/g5-cli-review-1/guard.rs`. Compiled and ran four calibration cases. Literal Git was rejected; configured path and constant Git were accepted with `[]`; Docker remained accepted. Exact output is `guard.log`. This is a runtime check of the checker, not a production bypass observed in current code.

**Governing:** `g5.architecture.git_guard`, `g5.quality.review.guards`, design “Enforce the intended boundaries” item 2 and operation-specific migration exceptions.

**Proposed repair/check:** guard construction ownership for all migrated owners while declaring narrow actual non-Git tool operations; cover configured paths/constants/aliases, valid Docker and other non-Git construction, helper renames, and missing intended owner targets. Static checks need not prove arbitrary Rust execution but should cover the explicitly promised configured-program case.

### F4 — P3: New mirror rewrite test assumes the temporary root has no symlink components

**Path:** `backend/g5-scm/src/native/tests.rs:2121–2137` and canonicalization at `native/mod.rs:363–371`.

With `TMPDIR=/tmp/g5-cli-review-1/tests` on macOS, the new mirror test fails: expected `Destination`, observed `Rejected`. `push_local_mirror_blocking` canonicalizes `/tmp/...` to `/private/tmp/...`, while the test installs `insteadOf` against the original `/tmp/...` spelling. That rewrite does not apply to the canonical destination, so the runner correctly reaches the existing divergent mirror and refuses its push. Running the identical test with `TMPDIR=/private/tmp/g5-cli-review-1/tests` passes.

This is a fixture portability defect, not evidence that the production destination was redirected or that its branch changed. Canonicalize the configured match prefix in the test, and retain a symlink-root fixture so the forbidden rewrite is actually exercised.

## Commitment and coverage ledger

| Commitment | Actual paths / discriminating cases | Result and limit |
|---|---|---|
| `git_execution`, `commands`, `minimal-api`, `abstraction`, `information-hiding`, `dependencies`, compatibility | All 18 frozen files inspected; CLI direct git2 moved into `GitRepo::{configure_remote,set_remote_url,discover_workdir}`; native probes use named operations; raw helper removed from workspace-ops; Cargo git2 moved to dev dependencies | No raw backend/argument/environment escape added in these production operations. Reused transport semantics have F2; configured spawn enforcement has F3. No new layer beyond existing SCM owner. |
| `git_native_runner`, runtime CLI/context, cleanup | CLI composition, signal admission, `Invocation`, blocking thread/runtime wrapper, native destination/config handling | Deadline and inherited-control behavior reused; new read-only queries classify effect as Refused. F1 at newly reached entrypoints. Deployed guest setup and full cancellation matrix unexecuted. |
| `git_ref_updates`, `git_locations`, `git_outcomes` | Job planning retains dev-container destination; new local mirror operation declares one source/destination branch and FastForward; reused push handles confirmation/rejection/partial evidence and disables following tags/submodule pushes | Canonical-root existing test passes: only main published, source config unchanged, divergent update refused, destination unchanged, rewrite refused. F4 qualifies the noncanonical fixture. Full Docker-copy workflow and concurrent local-mirror updates not executed. |
| `git_retry`, recovery | Full diff and new operations; existing Add optional observation policy and Job best-effort warnings | No new persistence, queues, retry loops, automatic replay or retry owner. Mirror warnings now retain detailed outcomes. |
| `git_credentials`, security redaction, runtime credential authority | `default_remote_branch`/mirror reuse validator, expansion checks, authenticate, target redaction; CLI selects runtime config from existing owner | No new token ownership/config persistence; no credentials in new command arguments. Global safe-directory write is removed without replacement. Live SSH/guest/external account authentication not executed. |
| Conservative spec semantics and location discovery | `file_is_new_blocking`, `path_is_ignored_blocking`, `discover_workdir`; Spec call sites retain false fallback on operational error | New test passes for unborn checkout, committed nested file, literal wildcard, ignored/ordinary path, detached HEAD, corrupt HEAD, and outside repository. Full CLI validation suite and all unusual filesystem encodings not independently run. |
| Existing provenance identity; semantic dependency direction | Entire changed FsQueryEngine constructor and test updates; `with_actor_and_executor`; `ProvenanceActor` constructors/validation; repository-wide caller search | No production FsQueryEngine consumer found. Explicit local email retains first-line trimming, per-instance identity and typed unknown; hosted actor/executor validation unchanged. Semantic package suite not independently run. |
| `git_guard`, quality calibration, authorized reader/projection owner | Full guard diff, existing reader and dependency guards; newly scanned crates and backend dependency metadata check | Existing content-reader/projection checks retained; no application content read moved outside them. F3. Existing library tests and guard suite beyond the custom probe not independently run. |

## Check receipt and limits

- Reused current SCM build artifacts whose dependency file includes `native/checkout_queries.rs`; test executable `target/debug/deps/g5_scm-554aebe82f3c130a` timestamp 2026-10-02 18:05:28. Avoided shared Cargo build contention.
- `native::tests::checkout_queries_preserve_literal_paths_and_conservative_errors`: **1 passed** with private TMPDIR.
- `native::tests::default_branch_and_local_mirror_use_explicit_scope_without_config_writes`: **failed** under `/tmp` symlink spelling as F4; **1 passed** under `/private/tmp` canonical spelling (`mirror-canonical.log`).
- Attempted `repo::tests::remote_configuration_and_checkout_discovery_preserve_unrelated_state` in that executable: **0 tests matched**, because this executable is the native-only variant. This is not a pass (`remote.log`). The test's assertions and implementation were inspected.
- Current SCM public-operation transport and signal probes and actual extracted guard probe are described above; compiled with existing dependency artifacts, no shared test mutation. The initial probe link needed the local OpenSSL library search path; corrected without rebuild.
- No full shipped CLI process, live Git daemon/SSH/HTTP server, deployed guest, or Docker workflow execution was claimed. No provider/host/guest migration completion claim. Current ontology implementation footers still describe the prior scoped evidence; after repair the main reviewer should update the affected execution/runner/guard/outcome metadata with exact new coverage and these remaining limits rather than treating this report as full-plan proof.

Implementation review is complete for this frozen increment; F1–F3 remain production/contract repair obligations, and F4 is a fixture repair. Verification remains qualified as above. No fixes were applied.
