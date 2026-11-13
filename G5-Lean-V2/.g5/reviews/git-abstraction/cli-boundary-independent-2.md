# CLI Git-boundary increment — independent review 2

Initial read-only findings, saved before sharing with the implementation owner or reading any author explanation or peer report. No production, ontology, design, or shared test files were edited during discovery. The only repository write is this report. No delegation was used.

Scope: branch `design/git-abstraction`, implementation base `8a856ad3`, frozen tracked diff and `native/checkout_queries.rs` listed in `/tmp/g5-cli-boundary-review/files.txt`, with the supplied patch/archive. All changed owners and their material callers were inspected. The wider host/guest/provider migration and parked branch/object confidentiality extension are outside this increment. Changed interactions with existing runner, dispatcher, clone, configuration, and guard behavior are in scope.

Inputs read: the pinned `.g5/ontology-plan-git-abstraction.md` beginning with `Ontology changes and governance`, its recorded ontology diff, original request/clarifications, full design and increment sections, current Git/governance commitments and paired metadata, relevant runtime CLI, Git exclusion, boundary/design-principle commitments and metadata, review skill, authoring format, and design review guide. This increment adds a plan section but changes no governing ontology text. No supplied known-gap hints were used. The main agent supplied only artifact paths and build availability on request.

## Initial findings

### F1 · P2 · Newly migrated command families do not install native cancellation handling

`backend/g5d-cli/src/job.rs:250`, `doctor.rs:464`, and `spec.rs:1245,1902` now start SCM runner operations. However, `backend/g5d-cli/src/process.rs:71-90` installs `native_git::with_interrupts` only for Clone, Add, Push, and Workspace. Doctor, Job, and Spec still take the plain dispatch path. `backend/g5-scm/src/native/process.rs:502` puts every native child in a separate process group. A terminal SIGINT/SIGTERM therefore terminates these unwrapped CLI processes without cancelling or reaping their Git children; the child no longer receives the terminal group's signal. A job mirror push may continue after its CLI exits.

This violates `g5.architecture.git_native_runner` and the plan's CLI signal/cleanup contract. Source inspection follows the real dispatcher predicate to the newly changed consumers. An isolated compiled probe using the current public `NativeGit::version_blocking` and a fake Git child confirmed that terminating the parent process group with SIGTERM leaves the detached child alive: parent exit `-15`, and the child wrote a marker two seconds later. This is runner/runtime evidence plus real-dispatch source evidence, not a full actual-CLI signal test.

Proposed repair: include every CLI command family that now uses native operations in signal ownership (or otherwise centralize ownership without changing host/dispatcher lifecycle). Regression: invoke actual Doctor, Spec, and a Job mirror path with a blocking fake Git child, send both terminal SIGINT and SIGTERM, and prove child/descendant cleanup before CLI exit. Preserve an ordinary completed invocation as the control.

### F2 · P2 · Default-branch migration rejects a previously supported Git transport

`backend/g5-scm/src/native/checkout_queries.rs:29` reuses `validate_url`, whose whitelist in `native/mod.rs:917-975` rejects `git://` even with `GitAuth::None`. `backend/g5d-cli/src/add.rs:500-511` falls back to cloning only for `FailureKind::Exit`, so this new `InvalidInput` aborts `g5d add git://host/repo` before its existing clone operation. Previously the command ran native `git ls-remote --symref URL HEAD` and let Git handle the URL; libgit2 clone remains capable of this transport. Supplying an explicit branch bypasses the new probe, making behavior inconsistent.

The current public operation was exercised directly: `default_remote_branch("git://127.0.0.1:9418/repo", &GitAuth::None)` returned `InvalidInput` with the HTTP(S)/file whitelist error, before contacting any remote. A real local Git-daemon control was attempted, but binding a loopback listener was denied by the sandbox (`PermissionError: Operation not permitted`); native daemon/CLI end-to-end transport remains unexecuted.

This violates the plan's instruction to preserve native transport variants and `g5.design.abstraction`/compatibility: a policy designed for credentialed writes is narrower than this unauthenticated observation. Proposed repair: preserve supported unauthenticated native protocols in this read operation while keeping credentialed destination restrictions. Regression: a local Git-daemon repository succeeds through default-branch inference and Add without an explicit branch, plus valid file/SSH/HTTP controls and forbidden credentialed destinations.

### F3 · P2 · Relative mirror destinations now resolve against the wrong directory

`backend/g5-scm/src/native/mod.rs:363-365` calls `destination.canonicalize()` against the process working directory. The previous `job.rs` command used `.current_dir(&plan.repo_dir)` before `git push <bare_path>`, so relative mirror URLs were interpreted relative to the source repository. `plan_repo_mirrors` and `extract_local_path` still admit and pass plain relative filesystem paths unchanged.

Runtime counterexample under `/tmp/g5-cli-review-2/relative-case`: source `source/` and sibling `mirror.git/`. The old-equivalent command, run in `source/`, `git push ../mirror.git main:main`, succeeded and created the branch. From the containing directory, the new public operation given the same source and `../mirror.git` returned `Local mirror must exist`. If a different repository exists at the process-relative location, it can receive the push instead. The new SCM operation changes the location meaning instead of encapsulating it.

This violates `g5.architecture.git_locations`, the already-recorded repository-relative native semantics, and the increment's promise to preserve dev-mirror layout/policy. Proposed repair: resolve a relative destination against the supplied source repository, with a consistent absolute destination for subsequent copy/use by the owner as applicable. Regression: distinct cwd/source directories, relative and absolute destinations, and a decoy process-relative repository; verify only the intended mirror receives the ref.

### F4 · P2 · The expanded production guard fails on an existing Unix-only test module

Adding CLI source scanning at `backend/g5-api/tests/authoritative_workspace_reader_guard.rs:317` brings `backend/g5d-cli/src/native_git.rs` into `typed_git_violations`. Its test module is `#[cfg(all(test, unix))]` at line 73, but the guard's test-only exclusion at lines 263-266 recognizes only exact `#[cfg(test)]`, `#[test]`, and Tokio test attributes. It treats the module's fixture process imports as production.

The actual compiled guard `/tmp/g5-cli-guard migrated_git_operations_keep_their_typed_owner_boundaries --exact` fails with `g5d-cli/native_git.rs: native execution import bypasses the shared runner`. An independently compiled exact-source copy produced the same result. This is an actual current suite failure, not just hypothetical missing calibration.

This violates `g5.architecture.git_guard`'s fixture exemption and harmless-variation obligation. Proposed repair: correctly classify cfg expressions that imply `test`, without exempting expressions that also enable production code; retain operation-presence checks. Regressions should accept `cfg(all(test, unix))` while rejecting `cfg(any(test, unix))` production bypasses.

### F5 · P2 · New CLI guard coverage misses configured or even locally named Git executables

At `backend/g5-api/tests/authoritative_workspace_reader_guard.rs:200-219`, the general CLI/semantic spawn check only rejects a call whose first argument is exactly the source literal `"git"`. The stronger process-path check applies only to push, workspace_manifest, and native_git, not the newly migrated add/doctor/job/spec owners. Thus the migrated `spec.rs` helper can reintroduce `let program = "git"; std::process::Command::new(program).output()` without any violation; `doctor.rs::check_git` can similarly use a configured `GIT_EXECUTABLE` value.

An isolated copy of the exact current guard plus these two counterexamples compiled and passed an assertion that both yield **no** violations. The existing literal/alias/Docker calibration test still passes. This establishes a discriminating hole in the newly added enforcement, not a claim of an existing production bypass or a requirement to solve arbitrary Rust dataflow.

The design explicitly requires configured-program detection and operation-level migration exceptions, and `g5.architecture.git_guard` requires rejecting actual bypasses while accepting unrelated tools. Proposed repair: guard the declared migrated Git-owning operations and their delegates against native process construction, permitting the independently owned Docker/toolchain paths by operation; add literal-variable/configured-path negative controls and valid unrelated-tool controls. Avoid broad whole-file process bans in mixed-tool owners.

## Other observed test defect

`native::tests::default_branch_and_local_mirror_use_explicit_scope_without_config_writes` failed with the inherited macOS temporary directory: expected `Destination`, observed `Rejected` at `native/tests.rs:2137`. The operation canonicalizes the mirror path, while the fixture installs an `insteadOf` entry for the original symlinked temporary path; therefore the installed rewrite does not match the actual pushed URL. The same test passed with `TMPDIR=/private/tmp/g5-cli-review-2`, an already canonical temporary root. This is a fixture portability problem, not evidence that a rewritten destination received data. Fix the fixture to target the actual canonical destination, and keep the wrong-destination absence assertion. The failure should not be treated as passing under the normal host environment.

## Commitment coverage ledger

| Commitment | Actual changed path and counterexample/check | Initial result |
|---|---|---|
| `g5.architecture.git_execution` | CLI direct backend sites now delegate to `GitRepo`; queries/mirror push use `NativeGit`; semantic constructor receives email. Production `git2` dependency moved to dev-only. Repository-wide constructor/call search found only test uses of the modified `FsQueryEngine::new`. | Source conformance for migrated mechanics; broader platform migration excluded. |
| `g5.architecture.git_ref_updates` | Mirror operation explicitly selects `refs/heads/<branch>`, FastForward, no follow-tags/submodule recursion; reuses checked destination and porcelain outcome implementation. Existing config and non-target refs are asserted by the new native fixture. | Ref-scope/fast-forward fixture passes with canonical temp root; destination-location meaning violated by F3. |
| `g5.architecture.git_native_runner` | New observations use bounded output, private runner invocation, per-invocation deadline, existing cancellation and child supervisor. No global safe-directory writes remain in CLI materialization. | Source conformance except F1. Real external/guest materialization, full CLI signal surface, and global-config byte comparison were not independently executed. |
| `g5.architecture.git_outcomes` | HEAD/file/ignore observations require well-formed successful output; corrupt/unavailable file history returns an error and spec policy uses conservative false. Mirror returns confirmed/refused/unknown branch outcomes; optional default discovery catches only Exit. | New history fixture passes for unborn, nested, tracked, literal glob path, detached, corrupt, outside-repo cases. F2 is an over-narrow reused-policy refusal. |
| `g5.architecture.git_retry` | Inspected complete diff and all new queries/mirror paths. | No new tracking, retry state, queues, workers, sleep/replay, or recovery authority. Optional read-to-clone continuation preserves owner policy and is not retry persistence. |
| `g5.architecture.git_credentials` | Newly exposed default-branch read uses existing destination validation and authentication injection; active Add caller uses None. Mirror uses local path/None. | No new direct injection or secret-bearing argv/config; reused authentication/cleanup inspected. Authenticated new default-branch read and SSH handshake not runtime tested. |
| `g5.architecture.git_locations` | SCM workdir discovery returns owned paths; typed remote configuration preserves existing push URL/user identity; mirror remains under job owner. | Workdir/config tests inspected; F3 violates relative location semantics. No API/dev/guest root collapse introduced. |
| `g5.architecture.git_guard` | Extended dependency/source scanning, existing public backend-surface and reader guards retained; tested literal backend/process aliases and Docker control. | F4 actual false positive; F5 configured-program false negative. No claim of whole-platform dynamic mediation. |
| `g5.runtime.workspace-cli` and `.context` | All changed native CLI consumers call the existing `native_git::runner`, consuming runtime config when the workspace socket is present and inheriting runtime PATH/loaders. | Source preserves composition; missing listener coverage is F1. Deployed guest/owner-UID materialization unexecuted. |
| Existing provenance identity / `g5.design.minimal-api`, abstraction, information hiding | New semantic constructor trims first line per instance and maps missing/empty to typed unknown; explicit actor/executor constructors and validators retained. No production caller of the removed implicit Git author probe found. New shared Git APIs return owned values and expose no raw argv/env/backend handle. | Targeted local-author, two-engine independence, and typed-unknown tests pass. No unjustified retry or identity layer found; F2/F3 erase valid input semantics. |
| Existing authorized reader/projection and Git authority | New CLI checkout observations serve the same local policy callers; no application content consumer was redirected away from the authorized reader; ref effects remain in SCM. | Source inspection found no new read-authority bypass. Existing full API content/source-vector suites not independently rerun. |

## Executed evidence and limits

All investigative code and mutable resources are in `/tmp/g5-cli-review-2`. `probe.rs` was linked directly against the current SCM rlib (`libg5_scm-750bbfac5cb00943.rlib`) and its matching Tokio dependency, avoiding a competing Cargo build. The fake-child test and relative-mirror test are in `probes.py`. `guard.rs` is an isolated exact-source guard copy plus the two bypass fixtures. These probes do not edit production/shared tests.

Executed against the supplied current binaries:

- `g5_scm-554aebe82f3c130a`: new checkout/history fixture passed.
- Same binary: new mirror/default fixture failed under inherited TMPDIR, then passed under canonical `/private/tmp/g5-cli-review-2` as documented above.
- `g5_semantic-1224c5f02275d82d`: local email normalization, two-engine author independence, and typed-unknown tests passed (3 tests).
- Actual `/tmp/g5-cli-guard` and independent exact-source copy: migrated-owner scan fails on the cfg(all(test, unix)) fixture.
- Independent exact-source guard: existing literal/backend-alias/Docker fixture passes; additional configured/literal-variable Git bypasses return no violation.
- Public SCM probes: relative-destination regression and detached-child survival reproduced; `git://` rejected before transport execution.

A real local Git-daemon control was blocked by sandbox loopback-bind permission. No escalation or alternative network route was attempted. Full CLI command signal lifecycle, real Docker mirror copying, guest materialization/identity, remote SSH/HTTP behavior, global-config preservation at the actual CLI public path, complete Cargo/platform suites, and alternate OS behavior were not independently executed. Passing helper tests are not claimed as proof of those outcomes.

Proposed metadata follow-up after coherent repair: update affected Git execution/runner/location/guard entries to reflect the completed CLI/semantic slice and preserve any remaining signal/materialization/transport evidence gaps. The existing metadata still explicitly describes wider native/provider migration; it should not be relabeled complete by this incremental review.

## Bounded repair recheck — C3/F3, C4/F4, C5/F5

This follow-up preserves all initial findings above. Scope was limited to the relative-mirror and guard repairs requested by the main agent after its saved consolidation. I inspected the actual repaired owners, tests, and the consolidation, then ran isolated probes without modifying production/shared tests or delegating. F1/F2 were not re-reviewed in this pass.

**C3/F3 is repaired on the evaluated paths.** `NativeGit::push_local_mirror_blocking` resolves a relative destination by joining it to the supplied source repository before canonicalizing. `job::plan_repo_mirrors` now resolves the source directory absolutely and forms a source-relative absolute mirror path that both push and Docker copy consume. The original exact public-operation counterexample now returns `Ok(Unchanged)` instead of `Local mirror must exist`, for both an absolute source path and `source` relative to the caller's directory. A newly compiled copy of the original public probe (`probe-recheck`) supplied this evidence; the original pre-repair probe remains intact.

The current SCM regression `native::tests::relative_mirror_destination_is_resolved_from_source_not_process_directory` passes and checks authoritative state: the source-relative bare mirror receives the commit and the process-relative decoy retains no refs. `job::tests::relative_mirror_plan_uses_the_checkout_for_push_and_copy` also passes. Source inspection confirms the same `bare_path` is passed to SCM and Docker copy. The existing absolute destination/fast-forward/config-preservation fixture also passes under the inherited macOS TMPDIR after its canonical rewrite repair. Real Docker transfer remains unexecuted in this recheck; it is not inferred from the plan-only fixture.

**C4/F4 is repaired on the evaluated paths.** The guard evaluates whether a cfg predicate can enable production when `test` is false, instead of recognizing only exact `cfg(test)`. The former actual failure (`migrated_git_operations_keep_their_typed_owner_boundaries`) now passes. The configured-program/cfg calibration test, declared CLI-owner inventory test, and declared native-observation-owner inventory test also pass against an independently compiled exact-source guard.

Additional independent controls in `/tmp/g5-cli-review-2/guard-recheck.rs` pass: `all(test,unix)` and nested predicates implying test exempt fixture imports; `any(test,unix)` and a nested expression permitting a Linux production path retain rejection. Hiding a required operation in a test-only item still yields a missing-target violation. These checks support the intended fixture distinction; no full Rust cfg/macro semantic proof is claimed.

**C5/F5's original counterexamples are repaired, but the broader alias/harmless-refactor obligation remains incomplete.** The original locally named `"git"` and configured `GIT_EXECUTABLE` examples now yield violations. Direct import aliases, raw-string/absolute Git executable cases, and ordinary literal Docker controls in the new calibration test pass. An independent control also permits a renamed Docker helper using `use std::process::Command as Process`.

Two further simple calibration cases discovered during this recheck remain:

1. **Alias chains and local type aliases still bypass rejection.** The exact repaired guard returns `[]` for both valid source snippets below in `g5d-cli/spec.rs`:
   ```rust
   use std::process as p;
   use p::Command as C;
   fn read() { C::new("git"); }
   ```
   ```rust
   type Process = std::process::Command;
   fn read() { Process::new("git"); }
   ```
   The implementation resolves imported names only one step and does not resolve local type aliases in process construction. These are direct alias variations of the forbidden path, not arbitrary dynamic/macro cases. Extend alias resolution for these concrete forms and preserve a real non-process type with a `new` method as a harmless control.
2. **Harmless raw-string tool spelling is falsely rejected.** In `g5d-cli/job.rs`, both `std::process::Command::new(r"docker")` and `std::process::Command::new(r#"docker"#)` yield `unowned/configured process construction bypasses the Git boundary`, although the allowed program is the identical literal Docker tool. `permitted_cli_tool` decodes the source expression with `serde_json::from_str`, which is not a Rust string-literal parser. Accept supported Rust literal spellings by their actual value while continuing to reject the equivalent raw-string `git` cases.

The two later discoveries above are distinct from the initial saved findings and remain proposed repairs, with exact probe output in `/tmp/g5-cli-review-2/guard-recheck-extra-evidence.txt`. They are calibration defects under the existing `git_guard` obligation; no production bypass currently using them was found. All original-scope runtime/source limits remain unless expressly narrowed here. No claim is made that C5 is fully closed merely because its first examples now fail.

## Final narrow guard recheck

Rechecked only the two later alias/literal calibration findings. Both are repaired on the reported forms. The guard now records local type aliases and repeatedly resolves imported/type-alias paths with a cycle guard before classifying process construction. The exact chained-import and local-type-alias Git examples are rejected, including a mixed import/type chain. The literal reader recognizes ordinary and raw strings, and the exact `r"docker"` / `r#"docker"#` harmless controls are accepted while raw-string Git remains rejected.

Executed the current `/tmp/g5-cli-guard`: `cli_guard_covers_configured_programs_and_test_only_cfg`, `cli_backend_guard_allows_other_tools_and_rejects_git_and_backend_aliases`, and `migrated_git_operations_keep_their_typed_owner_boundaries`; all three passed. Source inspection confirms the added exact examples are in the first calibration test. No remaining concrete counterexample was found within this narrow recheck. This closes the two later observations and the evaluated C5/F5 cases; it does not claim full Rust type, lexical-scope, macro, or dynamic mediation. Only this report was edited.
