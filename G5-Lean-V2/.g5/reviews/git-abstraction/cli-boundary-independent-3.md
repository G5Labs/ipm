# CLI boundary independent review 3

Initial independent, full-scope read-only discovery against HEAD `8a856ad3b78bb3bb26ea72585f5a38b8d9f96856` plus the frozen patch and `native/checkout_queries.rs` in `/tmp/g5-cli-boundary-review/`. No implementation explanation or peer findings were supplied. Production code, ontology, design and shared tests were not changed. Investigative files are in `/tmp/g5-cli-review-3`; existing test binaries were reused without running Cargo or contending on its build lock.

Scope: every changed owner in `files.txt`, material CLI composition/callers, the actual native runner's context/transport/process supervision, SCM remote configuration, semantic engine construction/persistence, and the changed source guard. The pinned plan was read starting with its complete ontology-change inventory/diff and governance, then its design and implementation sections. Current Git execution/ref/outcome/retry/credential/location/guard commitments and affected metadata, CLI runtime/context/drain, owner/dependency/compatibility principles, and source-guard calibration govern this review. The branch's earlier ontology changes remain governing; this increment changes the plan but has no ontology-body diff. Host/guest/provider work and the parked branch/object confidentiality extension were not treated as findings in this increment.

## Initial findings

### F1 — P1: newly migrated Job/Spec/Doctor native children bypass CLI interruption ownership

Changed sites include `backend/g5d-cli/src/job.rs:250`, `job.rs:320`, `spec.rs:1245`, `spec.rs:1901` and `doctor.rs:464`. They now use the shared native runner, whose `native/process.rs:502` creates an independent process group. The actual process entry owner (`backend/g5d-cli/src/process.rs:71-77`) installs `native_git::with_interrupts` only for Clone, Add, Push and Workspace. Job, Spec and Doctor never cancel the shared `NativeCancellation` and wait for cleanup when the CLI receives SIGINT/SIGTERM.

Before migration the raw Git child was in the CLI terminal process group. After migration a terminal group signal can terminate the CLI while the runner-owned Git process remains in its separate group. In Job this includes the mirror push; it can continue changing the mirror after the CLI has exited, and child/stream cleanup is not established. This violates `g5.architecture.git_native_runner` and the plan's explicit existing-cancellation preservation. It is an interaction defect introduced by moving these calls, although the missing selector lines are unchanged.

Proposed repair: extend the process-level native command classification to every now-migrated command that can reach the shared runner, while preserving unrelated lifecycle owners. Add command-entry tests for at least Job's mutating path and Spec/Doctor's reads that deliver terminal SIGINT/SIGTERM, verify the intended exit status, and prove the Git process group/descendants are gone before exit. Source tracing establishes the missing dispatch branch; runtime evidence and its precise limits are recorded below.

### F2 — P2: optional default-branch discovery now rejects the existing `git://` transport before clone

`backend/g5-scm/src/native/checkout_queries.rs:29` reuses `validate_url`, whose current allowlist excludes the native unauthenticated `git://` protocol (`native/mod.rs:912-964`). `backend/g5d-cli/src/add.rs:499-513` only treats `FailureKind::Exit` as an optional discovery failure; InvalidInput is propagated at `add.rs:121` before the existing libgit2 clone path. Thus an otherwise valid `g5d add git://host/repo.git` without `--branch` is refused, while providing an explicit branch skips the new probe and retains the old clone path.

The direct current SCM public-operation probe returned `InvalidInput` and the HTTP(S)/file-only guidance for `git://127.0.0.1:9/repo.git`, before any transport attempt. Its SSH control reached the transport and returned `Exit`, confirming that this is protocol admission, not general network failure. Full `g5d add` against a live git-daemon was not run. The old probe passed the URL directly to native `git ls-remote`; the supported clone owner was not intentionally narrowed by this increment.

Proposed repair: allow supported unauthenticated native transport variants in this observation, without widening token-bearing grants; include default-branch discovery and CLI add regressions using `git://`, with the explicit-branch path as a control. This follows the plan's “Preserve native transport variants when moving probes” and `g5.design.abstraction`/`g5.architecture.policy`.

### F3 — P2: new CLI/semantic spawn guard does not reject configured Git paths or even raw string literals

`backend/g5-api/tests/authoritative_workspace_reader_guard.rs:200-218` checks exact source bytes against `"git"`. Because only three CLI files receive the stronger `native_owner` path/import check, the migrated `spec.rs`, `job.rs`, `doctor.rs`, `add.rs` and semantic source accept bypasses such as:

```rust
fn probe() {
    let git = std::path::Path::new("/usr/bin/git");
    let _ = std::process::Command::new(git);
}
fn probe() { let _ = std::process::Command::new(r"git"); }
```

A standalone copy of the exact guard function returned no violations for both examples under `g5d-cli/spec.rs`. It correctly rejected the ordinary literal `"git"` and accepted the unrelated `"docker"` control. The implementation therefore does not meet the explicitly required configured-program-path case in the saved design / `g5.architecture.git_guard`; this is a demonstrated guard defect, not a claim that current production code already uses the bypass. It does not require arbitrary Rust type or macro expansion to reproduce.

Proposed repair: enforce operation-specific ownership of native process construction for these migrated owners while retaining declared non-Git tools, and calibrate with configured paths, raw strings, aliases and harmless unrelated processes. Avoid whole-file exemptions that permit new Git execution.

### F4 — P2: the new mirror rewrite regression fails under the normal macOS temporary-directory spelling

`backend/g5-scm/src/native/tests.rs:2123-2137` installs `insteadOf` against the uncanonicalized tempfile path; `push_local_mirror_blocking` canonicalizes the destination first (`native/mod.rs:361-368`). On macOS's normal `/var/...` → `/private/var/...` alias, the configured prefix cannot match. Consequently the test performs an ordinary non-fast-forward push and returns `Rejected`, failing its expected `Destination` assertion.

Executed current SCM test `native::tests::default_branch_and_local_mirror_use_explicit_scope_without_config_writes`: failed with left `Rejected`, right `Destination`. The same unmodified test passed with `TMPDIR=/private/tmp/g5-cli-review-3`, which supplies a canonical path. This confirms a path-alias-sensitive test defect, not a destination escape demonstrated in production. Canonicalize the fixture's mirror path before creating its rewrite and assert the forbidden destination remains unchanged; retain the normal macOS environment in regression coverage.

## Commitment and owner coverage

| Commitment / obligation | Actual owner and evidence | Result or limit |
|---|---|---|
| `git_execution`, minimal API, information hiding, dependency direction | New observations live in private native module with owned paths/results; `GitRepo::{discover_workdir,configure_remote}` hides backend handles; CLI direct git2 moved to dev-only; semantic raw author probe removed | Source-preserved; no generic command/env API introduced. Whole-platform backend closure is outside increment. |
| `git_ref_updates`, local mirror layout and materialization ownership | Job still owns dev-mirror destination, first-repo scalar head and best-effort copy policy. New branch operation reuses validated explicit `refs/heads/<branch>`, fast-forward push, `--no-follow-tags`, bounded porcelain effect parsing | Real native mirror test passes with canonical fixture; divergent push retains destination head. Cancellation interaction is F1. No mapped-branch authority is inferred from mirror writes. |
| `git_native_runner`, CLI context, cleanup | New methods use same context and invocation supervisor; binary resolution and repository-control clearing inspected. `run_blocking` routes through private runtime and joins it | Per-invocation supervision reused; process entry does not wire cancellation for all new consumers (F1). Guest runtime and actual bundled Linux guest not executed. |
| No global/system writes; identity/exclusions stay owned | Removed workspace-ops global safe-directory function and both manifest calls. Remaining remote updates affect repository configuration only. `GitRepo::configure_remote` preserves unrelated push URL/config via existing libgit2 mechanics | Source-preserved. Actual clone/materialization/global config comparison was not run in this reviewer; existing lower-level test only asserts source config. |
| `git_outcomes`, conservative spec observations | `require_success`/`negative_observation` reject diagnosed corruption; new-file operation resolves valid checkout, distinguishes missing symbolic head from broken ref/object, and uses literal file history | Current checkout-query test passes for unborn/nested/tracked/wildcard-looking/detached/broken/outside cases. CLI missing-source and gitignored-source tests pass. Overall command cancellation remains F1. |
| `git_retry`, explicit agent/human recovery | No changed schema, new state, queues, retry loops or workers. Mirror failure remains warning/continue; reads retain conservative fallback | Preserved by source inspection. No automatic replay introduced. |
| `git_credentials`, transport compatibility | Default probe consumes `GitAuth`, runner injection/redaction reused; current CLI passes None. Mirror uses None and canonical local destination, refusing configured redirection | Existing credential scope not expanded, but valid unauthenticated protocol is narrowed (F2). Live remote authentication/handshake not exercised. |
| `git_locations` | Mirror layout stays in Job; supplied checkout location remains in SCM discovery; native history probes use caller's directory | No new location scheme. Canonical path handling is correct production intent; test fixture does not account for aliases (F4). |
| Semantic local provenance, per-engine identity | `FsQueryEngine::new(root,email)` trims first line, maps missing/empty to typed unknown and calls unchanged validated actor/executor constructor; complete Rust caller search finds only test consumers of `new`, hosted explicit constructors unchanged | No live caller loses implicit identity in this checkout. Per-instance capture retained. New semantic tests were inspected, not executed by this reviewer. |
| `git_guard`, structural calibration, authorized reader | Existing reader/projection guard logic retained, manifest normal/build backend checks cover CLI/semantic, explicit literal/alias controls added | Demonstrated configured/raw-string bypass F3. No application-reader authority was moved into the CLI raw-probe path. Full guard suite not run by this reviewer. |
| Reused remote configuration policy | `configure_origin` formerly create-on-any-find-error now uses SCM create only on NotFound; manifest existing-origin paths reuse pre-existing SCM setter | Valid states preserve effects; corrupt lookup now returns error instead of speculative create. No finding. |

## Executed checks and limits

- Reused current native-only test binary `target/debug/deps/g5_scm-554aebe82f3c130a`, containing both newly added native tests. `checkout_queries_preserve_literal_paths_and_conservative_errors` passed.
- Mirror/default-branch test failed in normal macOS environment and passed in the canonical isolated temporary root, as detailed in F4.
- Reused current CLI test binary `target/debug/deps/g5d_cli-4b8cae290564ff67`: `validate_specs_missing_source_warns_for_uncommitted_spec_then_fails_after_commit` and `validate_specs_rejects_gitignored_source_file_even_when_present` passed.
- Current SCM public-operation probe built with rustc against `libg5_scm-750bbfac5cb00943.rlib`; no Cargo build invoked. It established the direct typed admission behavior in F2. A live network transport was not needed for its InvalidInput refusal; localhost SSH control could not connect under this sandbox and only establishes it passed admission.
- Standalone exact guard-function probe compiled and executed; configured Git and raw-string bypasses accepted, ordinary literal rejected, Docker accepted. This is evidence about the checked guard itself, not a model of another guard.
- An initial doctor signal probe linked an older `g5d-cli` rlib rather than current changed CLI sources. It observed child termination, but is **not current-checkout evidence** and is excluded from the findings' support. Current dispatch/native source is the basis of F1; any later source-included probe is recorded separately.
- No full Cargo suite, full CLI materialization, global/system config snapshot, real guest launcher execution, production git-daemon/SSH authentication, or live API/provider workflow was executed. Existing source and supplied metadata were not promoted to new runtime evidence. No numeric conformance percentage is claimed.

The four findings above are preserved as the initial report. Main-agent consolidation should evaluate and repair them coherently, then update relevant metadata without presenting this limited review as whole-design completion.

## Corroboration after the initial report was saved

F1 now has a discriminating runtime probe in `/tmp/g5-cli-review-3/{signal_current.rs,signal_probe.py,signal-current-results.json}`. The harness includes the **current source files** `g5d-cli/src/process.rs` and `native_git.rs` directly, links the current SCM rlib, parses actual CLI command arguments, and dispatches the same `runner()?.version_blocking()` operation used by current Doctor. The unused PR branch is stubbed because it is inaccessible across the test harness's crate boundary; no production source is modified. This exercises the real process selector and native runner, not the complete Doctor checks or Job mirror caller.

With a fake Git executable that holds its pipes, records its PID and writes a delayed marker, terminal process-group SIGTERM and SIGINT both terminate the Doctor-classified CLI while the native Git child remains alive and writes the marker **after CLI exit**. The same invocation classified as Push uses the actual interrupt owner, exits 143/130, and leaves neither child nor marker. Thus the selected owner transition discriminates the missing protection. All probe children were cleaned up.

| Parsed command | Signal | CLI exit | Git alive after exit | Delayed effect after exit |
|---|---:|---:|---|---|
| doctor | TERM | -15 | yes | yes |
| doctor | INT | -2 | yes | yes |
| push (control) | TERM | 143 | no | no |
| push (control) | INT | 130 | no | no |

## Later discovery before consolidation

### F5 — P1: relative mirror destinations resolve against the wrong directory and can write to another repository

`backend/g5-scm/src/native/mod.rs:361-364` canonicalizes `destination` directly, relative to the CLI process cwd. The existing Job caller retains plain filesystem URLs (`job.rs:334-340`) and passes their relative path unchanged (`job.rs:252-255`). Before migration its `Command::current_dir(&plan.repo_dir)` made Git resolve this same path relative to the source checkout. The change therefore alters the effective repository before the runner's destination checks; those checks validate the newly selected wrong path and cannot catch the substitution.

A direct current-public-operation probe created `relative-case/workspace/source`, its intended `relative-case/workspace/mirror.git`, and another valid `relative-case/other/mirror.git`, then invoked the operation with destination `../mirror.git` while cwd was `relative-case/other/cwd`. The prior command (`git -C <source> push ../mirror.git main:main`) successfully wrote the intended mirror. After deleting that fixture ref, the new `push_local_mirror_blocking(<source>, ../mirror.git, main)` returned `Ok(Updated)` while the intended mirror stayed empty and the other cwd-relative mirror received the commit. Both destinations and all mutations were isolated under `/private/tmp/g5-cli-review-3/relative-case`.

This violates the typed operation's repository identity and `g5.architecture.git_locations`/`git_ref_updates`, and the saved plan's requirement to preserve mirror destination/layout and existing native transport semantics. Proposed repair: resolve a relative local destination against the source repository's effective working directory before canonicalization (or make the Job owner explicitly supply that resolved destination), preserving absolute paths. Add a public-operation regression with distinct process cwd, source directory, intended relative mirror and a decoy mirror, asserting only the intended mirror changes. The existing Job copy stage's handling of relative paths predates this increment and was not assessed as a new repair obligation; the newly changed push target is independently demonstrated.
