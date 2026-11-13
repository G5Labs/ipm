# Native Git increment: independent review 2

## Review basis

Fresh independent read-only discovery review of the full assigned native increment; no author explanation, author history or peer findings read. No delegation. Read the original request, pinned `.g5/ontology-plan-git-abstraction.md` including its first ontology-diff section, actual current Git/governance blocks and metadata, and material runtime, security, durability, architecture and guard contracts. Reviewed every changed production/test file in the increment, its CLI consumers and the CLI process entrypoint. Prior resolution/diff work is outside this increment except for the shared feature gates/guard interactions.

Frozen patch: `/tmp/g5-native-review-snapshot/increment.patch`, SHA-256 `6cd1c7b023e60c79fd094fa951f36d2ad41317f183d23ca238f4ed41752fa3cd`. Current source inspected agrees on the paths cited below. No production or ontology edits were made. Investigation files are isolated under `/tmp/g5-native-review2`; no Cargo build or shared mutable test resources used.

## Initial findings (before consolidation or repair)

### R2-1 — P1: inherited Git parameters override the credential redirect fence

**Locations:** `backend/g5-scm/src/native/process.rs:60-64`, `:104-136` (`Invocation::new` / `authenticate`).

**Rule:** `g5.architecture.git_credentials`, `g5.architecture.git_native_runner`, and the saved native-migration design require destination-bound invocation credentials and prohibit forwarding them on redirects while preserving the declared runtime environment.

**Defect:** The runner removes trace variables but inherits `GIT_CONFIG_PARAMETERS`. It then places its credential helper/redirect/header fences in `GIT_CONFIG_COUNT` entries. Git applies inherited `GIT_CONFIG_PARAMETERS` afterward, so an inherited exact-URL `http.<url>.followRedirects=true` overrides the supposedly authoritative `false`. The unchanged actual remote passes `check_remote`; the authenticated invocation follows the redirect with its Authorization header. This is a real Git precedence issue, not a proposed future caller.

**Discriminating evidence:** An isolated loopback HTTP probe used the exact fetch arguments and six config entries assembled by the implementation, with a dummy Authorization value. Without the inherited parameter, only `/repo/info/refs` received the header and Git failed on HTTP 302. With `GIT_CONFIG_PARAMETERS='http.<url>.followRedirects=true'`, both `/repo/info/refs` and redirected `/other/info/refs` received `Basic review-fixture`. Same-origin path change suffices to cross the requested repository boundary. Probe: `/tmp/g5-native-review2/redirect_probe.py`. The initial sandbox blocked loopback binding; the approved isolated probe then executed successfully. No external requests or real credentials were used.

**Repair:** Define and enforce the CLI Git environment policy at invocation creation. Do not allow inherited config parameter channels to override operation-owned safety settings; preserve explicitly permitted runtime config separately. Audit related inherited Git environment channels for repository/destination or helper overrides rather than removing only trace output.

**Regression:** Exercise the public credentialed native fetch/push operation against two local repository paths with inherited `GIT_CONFIG_PARAMETERS`; assert only the intended path receives the secret and the redirect target is never contacted. Retain a valid inherited runtime configuration case.

### R2-2 — P2: Ctrl-C now leaves the native Git process group running

**Locations:** `backend/g5-scm/src/native/process.rs:202-208`; material consumers `backend/g5d-cli/src/process.rs:15-34,64-91`, `backend/g5-scm/src/native/mod.rs:334-358`.

**Rule:** `g5.architecture.git_native_runner` and the native-migration design require owned child/stream cleanup through cancellation. User-visible native CLI operations must not continue publishing after ordinary cancellation.

**Defect:** Every native process is placed in a new process group. The new group enables future-drop cleanup, but the CLI's push/materialization entry paths do not intercept SIGINT/SIGTERM and await that cleanup. Ctrl-C reaches the foreground CLI group, kills the supervisor process without running Rust destructors, and leaves the separate Git group alive. This changes the old native command's foreground-group behavior. `push_branch_blocking` also joins a private runtime synchronously, so simply dropping an outer async future does not cancel that active call.

**Discriminating evidence:** Copied the already-built native test binary to an isolated path (no build). Launched the existing `deadline_includes_pipes_held_by_descendants_after_leader_exit` fixture in a new foreground-equivalent group, waited for its real native child PID, sent SIGINT to the parent group, and observed the fixture's delayed `escaped` side-effect file after the parent exited with `-2`. The normal deadline/future-abort tests pass, demonstrating that the missing case is process-level CLI cancellation. Probe: `/tmp/g5-native-review2/sigint_probe.py`.

**Repair:** Wire CLI process cancellation into native-operation cancellation and await process-group termination before exiting, including synchronous push consumers; or provide another reliable process-custody mechanism that handles parent death without violating dependency direction. Preserve the distinction between possible already accepted effects and stopping subsequent work.

**Regression:** Run a real CLI/native-operation subprocess, send terminal-style SIGINT and SIGTERM while Git/its child holds the operation open, and assert no surviving owned process or delayed effect. Keep async future-drop and timeout cases.

### R2-3 — P2: target evidence is discarded before the CLI renders native errors

**Locations:** `backend/g5-scm/src/native/mod.rs:43-56,70-74`; `backend/g5-scm/src/native/tests.rs:333`.

**Rule:** `g5.architecture.git_outcomes` and caller-decided retry require errors to identify the operation and target so the human/agent can inspect the affected state.

**Defect:** `at_target` stores a redacted target, but `Display` never renders it. Converting the typed native error to `G5Error::GitError` keeps only that display string, permanently discarding the repository/ref/lease evidence. Deadline, destination-mismatch and lost-response errors therefore say to inspect current state without identifying it. This is especially material for a multi-member workspace push, where previously published members may be named but the failed member is not.

**Evidence:** The source is direct; the increment's own `push_outcomes_preserve_confirmed_refs_and_separate_stale_proxy_from_auth_failure` test fails at line 333: `error.to_string().contains("refs/heads/main")` is false. This failure was reproduced twice with the copied current native-enabled test binary.

**Repair:** Preserve and expose the sanitized target through the public CLI error conversion, retaining effect and useful action. A structured error is preferable if callers need machine-readable evidence, but at minimum the ordinary rendered error must contain the target.

**Regression:** Assert target/ref/effect survives actual `NativeGitError -> G5Error -> CLI text/JSON` conversion for deadline, destination refusal and partial publication, with no credential leakage.

### R2-4 — P3: the new source guard rejects harmless identifiers rather than native execution ownership

**Location:** `backend/g5-api/tests/authoritative_workspace_reader_guard.rs:85-94`.

**Rule:** `g5.architecture.git_guard`, `g5.quality.review.guards` require rejection of actual bypasses while allowing harmless internal refactors.

**Defect:** For either migrated CLI file, any identifier named `process`, `Command` or `base64` is rejected, regardless of whether it names a process API. For example, renaming an unrelated local binding to `process`, or using the already-existing CLI `Command` enum, makes the guard fail without introducing a Git spawn or credential operation. Conversely, it does not establish ownership for an imported generic spawn helper. The present harmless-refactor fixture only uses a `runner` name and misses this case.

**Evidence:** Direct AST predicate inspection. No guard runtime was executed in this review.

**Repair/regression:** Resolve the imports/call forms relevant to subprocess construction and credential assembly, retain missing-target checks, and add a harmless binding/helper rename plus unrelated CLI enum case. Report its structural limits rather than treating identifier spelling as mediation.

## Commitment coverage and available results

| Commitment | Actual path | Discriminating case/check | Result or gap |
|---|---|---|---|
| `git_execution`, `commands`, `minimal-api`, `abstraction`, `dependencies` | Optional crate features, native public operations, private `Invocation`, migrated CLI call sites | Public signatures contain no argv/env passthrough or native handles; dependencies stay below CLI/runtime orchestration | Source-inspected sound for this increment. Other native/backend/provider migration remains explicitly outside it. No fresh Cargo feature build performed here. |
| `git_ref_updates`, `git_owner`, `g5.git.network.cas`, `g5.git.network.ref-scope` | `BranchPush`, exact leases, explicit source/destination, CLI fast-forward admission and managed candidates | Real local Git lease, create-if-absent, stale rejection, no tag following and tracking fetch tests | Existing native local-Git test passed from copied binary. Managed server admission and live HTTP publication not re-executed. Configured fetch refspecs retain existing semantics rather than claiming mapped-ref authority. |
| `git_native_runner`, `g5.runtime.workspace-cli`, `.context`, `.bundle` | PATH resolution, inherited loader/context, private supervisor and blocking wrapper | Child/drop/deadline tests; process-level SIGINT probe | Async abort/deadline tests passed; R2-2 identifies cancellation gap. Only CLI context introduced; host/dispatcher materialization isolation and complete native-only consumer closure remain migration gaps. |
| `git_credentials`, security platform-token/publication/redaction | URL validation, destination checks, invocation-only scoped headers, redaction | Multiple push URLs/rewrites refused; credential argv/output bounds tested; inherited parameter redirect counterexample | Existing destination/redaction tests passed. R2-1 violates credential destination fence. No provider credentials added to guest. |
| `git_outcomes`, `git_retry`, recovery principle | Native effect enums, push porcelain, CLI conversion, per-member publication accumulation | Rejection/partial/lost reply classifications source-inspected; no persistence/replay introduced | No new retries/tracking tables. R2-3 drops target. Outcome test fails before reaching all its cases; remaining table cases were not fully runtime established by that failing test. |
| `git_locations` | CLI supplies checkout path and captured URL | No API/dev-mirror path reconstruction introduced | Source-inspected preserved; typed location kinds remain wider design work. |
| Authorized reader/projection and `git_guard` | Existing guard plus native CLI guard branches | Earlier reader logic retained; migrated targets required | No new content read path added. R2-4 guard calibration issue. Whole-platform native/provider guard closure is not claimed. |
| PR authorship/checks/follow-push and selected-file checkpoint/flush owners | Existing callers still own policy; changed native push lower layer | Trace CLI managed candidate flow and shared entrypoint; no PR credential selection or persistence changes | No source-level policy relocation found. Live server follow-push and checkpoint suites were not run here. |
| `scm_provider_client`, credential scopes, GitHub transport | No provider HTTP changes in this increment | Feature gate touches existing provider value types only | Not implemented by this increment; retain existing design gaps rather than declaring conformance. |

## Verification receipt and limits

- Copied `target/debug/deps/g5_scm-08d6ca130345ed39` to `/tmp/g5-native-review2/scm-tests` before running. It lists the current native test names and reproduces the line-exact source failure. Because this reviewer did not rebuild, this is existing-artifact execution evidence, not an independently reproduced build.
- Ran seven native tests with the HTTP listener test excluded from the sandboxed batch: six passed, one failed (R2-3). Passing tests cover real local clone/fetch/push/leases, destination rewrite refusal, credential argument/error redaction/output cap, async cancellation, pipe-held deadline and truncated-secret redaction.
- The separate loopback redirect probe required approved sandbox escalation for listening and used only dummy credentials. It executes the implementation's exact native Git command/config pattern, not the Rust public method; a public-path regression is still required.
- Process SIGINT probe executes the compiled production supervisor through the existing native fixture. Full shipped CLI process integration and Linux guest behavior remain unexecuted.
- No Cargo builds, provider/API/PostgreSQL tests, deployed guest tests, or peer reports were used. Review/repair token or elapsed costs were not measured. Main owns consolidation, repairs, final native/CLI verification and corresponding metadata updates.
