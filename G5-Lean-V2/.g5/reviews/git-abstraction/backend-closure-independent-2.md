# Backend dependency closure — independent review 2

Initial discovery saved on 2026-10-02, before receiving author explanations or peer findings. This reviewer did not implement the change and made no production or ontology edits. Supplied context identified the retry constraint, parked branch-confidentiality extension, and remaining provider migration; these were scope inputs, not defect hints.

Scope: all 18 uncommitted files against `59c81a763d3c29760d8f4834e6cad73ce952d60b`, frozen in `/tmp/g5-backend-closure-review/{files.txt,tracked.patch,snapshot.tar}`. A byte comparison against the archive after discovery found all 18 files unchanged. The original request, the saved design beginning with **Ontology changes and governance**, the final backend-closure section, and the affected current ontology/footer entries were inputs. This report uses the assigned read-only discovery mode of `ontology-review` and its design-review guide. No delegation occurred.

## Findings

### F1 — P2: API lookup can select Git that the API identity cannot execute

**Changed path:** `backend/g5-api-core/src/services/mod.rs:850` now uses `g5_scm::native::locate_executable`; its reused implementation is `backend/g5-scm/src/native/process.rs:56–74`, particularly the `mode & 0o111` check at line 69.

The helper treats any execute bit as proof that this process can execute the candidate. Unix permission selection does not work that way: for a file owned by the API user, mode `0610` has a group execute bit but no owner execute bit, so execution is refused. An earlier such `git` on PATH masks a valid executable later on PATH. The previous `which git` lookup skips the unusable file. The API stores the new unusable path in `AppState.git_bin`, so smart-HTTP advertisement/fetch/receive, flush proof reads, and GC subsequently fail to spawn Git even though PATH contains a usable binary.

**Evidence:** an isolated runtime probe copied the exact current bodies of `locate_executable`, `resolve_host_binary`, and `executable_on_path`, with the existing native error types. It created two owner-controlled PATH entries: `first/git` at `0610`, `second/git` at `0700`. On this macOS host:

```text
new_lookup=/private/tmp/g5-backend-closure-review/reviewer2/lookup-fixture/first/git
previous_lookup=/tmp/g5-backend-closure-review/reviewer2/lookup-fixture/second/git
new_execution=Err(Os { code: 13, kind: PermissionDenied, message: "Permission denied" })
```

The probe's assertions passed after accounting for macOS `/tmp` canonicalization. Source/probe: `/tmp/g5-backend-closure-review/reviewer2/lookup_source_probe.rs`; executable: the same path without `.rs`. This is source-extracted resolver runtime evidence, not a full API startup/HTTP test. The existing current-source public-wrapper test `native::process::tests::executable_lookup_skips_non_executable_candidates` also passed, but covers `0600`, not execute bits belonging to an inapplicable identity class.

**Governing obligations:** final closure's preservation of API executable selection, `g5.architecture.git_native_runner`, `g5.architecture.compatibility`, and the review requirement to inspect reused operations' actual policy.

**Proposed repair:** check executable access for the launching identity, including applicable permissions/ACLs, and continue searching after unusable candidates, without spawning a lookup tool. Extend the public helper regression to an inapplicable execute bit followed by a usable candidate; preserve the existing override and fallback policy. The helper pre-existed, but this increment newly routes API discovery through its weaker predicate.

### F2 — P3: Expanded source guard rejects harmless local identifiers

**Changed path:** `backend/g5-api/tests/authoritative_workspace_reader_guard.rs:385–389` now applies to every scanned backend crate and rejects every AST identifier or type identifier whose text is `git2` or `libgit2_sys`.

The check is not limited to dependency references. For a newly covered crate, `fn summarize(git2: usize) -> usize { git2 }` yields two backend-access violations despite having no Git access. An innocent local rename can therefore fail the guard. This is a calibration issue rather than a production runtime defect; existing calibration only uses a distinct `git2_note` name or strings/comments.

**Evidence:** the exact current guard source was copied to `/tmp/g5-backend-closure-review/reviewer2/guard_probe.rs`, with one appended probe. Executing it produced:

```text
unrelated local identifier violations: ["g5-new/lib.rs: migrated owner accesses git2", "g5-new/lib.rs: migrated owner accesses git2"]
```

The same probe confirms actual `git2::Repository::open` and `libgit2_sys::git_libgit2_init` references in the new crate are rejected. All 22 existing source-guard tests pass.

**Governing obligations:** `g5.architecture.git_guard` and `g5.quality.review.guards`, which require harmless internal refactors to remain valid.

**Proposed repair:** identify dependency/import/path references rather than all matching local identifiers; retain the Cargo metadata check for renamed backend dependencies. Add the local-variable control alongside a real forbidden reference. No full Rust type resolution is necessary to distinguish this basic case.

## Retained boundary requiring explicit disposition

The new all-crate traversal does not make native process checking apply to all crates. Native detection still depends on the existing `mixed_tools` crate list or `native_owner` operation list. In the exact-source probe, the following in `g5-new/lib.rs` returns no violation:

```rust
fn read() { std::process::Command::new("git").arg("status").status(); }
```

This is a demonstrated guard coverage limit, not a new live production bypass found in this patch. It also predates the changed traversal's intent for libgit2, and current `g5.architecture.git_guard` metadata already says whole-platform native enforcement needs migration. Therefore it is recorded separately from F1/F2 rather than represented as a newly introduced production regression. Before claiming the full original abstraction design complete, reject straightforward Git spawns in newly added backend crates while keeping legitimate non-Git tools and test fixtures valid. The final closure sentence about guarding every backend crate should be read as the direct-backend dependency/reference coverage actually implemented, not as proof of universal native-spawn coverage.

## Commitment coverage and checks

| Commitment | Actual path and discriminating case | Result or limit |
|---|---|---|
| `g5.architecture.git_execution`, `g5.architecture.dependencies` | Contracts manifest/conversion removal, SCM feature wiring, common/API/g5d manifests; inspect all packages' Cargo dependency kinds, plus native-only SCM/contracts/host/dispatcher dependency trees | Only SCM has a normal direct `git2` dependency; API/core/workspace-ops/CLI/g5d fixture dependencies are dev-only. No `git2` or `libgit2-sys` in the evaluated native-only SCM, all-feature contracts, or host/dispatcher normal trees. |
| `g5.design.information-hiding`, `g5.design.minimal-api`, `g5.design.abstraction` | Private `backend_error`, existing owned `G5Error`, explicit `locate_executable(OsStr) -> Result<PathBuf, NativeGitError>`; existing public-surface guard | No backend handle or generic execution interface added. The shared contracts crate no longer exposes a backend conversion or feature. Current source surface tests pass. |
| `g5.architecture.compatibility`, `g5.architecture.git_outcomes` | Compare prior `From<git2::Error>` to the new SCM mapper and all affected call sites; inspect owned error consumers/status mapping | Both mappings construct exactly `G5Error::GitError { message: error.message().to_owned()/to_string() }`. After removing the new explicit maps and ignoring whitespace/trailing commas, all five affected implementation files (`repo`, `commit_preparation`, `resolution`, `safety`, `worktree`) match HEAD. No error-category/message or operation-order change found. |
| `g5.architecture.git_ref_updates`, authorized content-reader and projection ownership | Changed SCM operations and their existing public consumers; real repository/ref/read tests | 132 repository tests and 9 resolution tests pass, covering checked ref effects, immutable readers, invalid objects, failure uncertainty, and conflict refusal. The change only inserts the same conversion at prior implicit-conversion sites; no new caller or authority path found. API routes still consume the existing typed owners. |
| `g5.architecture.git_native_runner`, compatibility | API `which_git` to shared resolver, downstream API protocol/observation/maintenance consumers; prior-vs-current PATH candidate probe | F1. Override and fallback source policy remains; no runtime full API startup test was performed. |
| `g5.architecture.git_guard`, `g5.quality.review.guards`, `g5.quality.review.cases`, `g5.design.enforcement` | All-crate inventory; renamed/target-specific/normal/build dependency fixtures; test-only sources; public signatures; absent targets; actual new-crate forbidden references and local-name control | Existing 22 tests pass; F2 and the retained native coverage limit are independently reproduced. Metadata lookup fails closed for a missing package. Test fixtures remain exempt from production dependency checks. |
| `g5.architecture.git_retry` and original request constraints | Entire current patch and changed SCM paths | No new retry persistence, automatic replay, queue or retry mechanism. Provider migration remains outside this increment, and branch confidentiality remains parked. |

The broader ontology metadata remains explicitly partial. This closure should update relevant backend-conversion/dependency evidence during main-agent consolidation, while retaining provider/native guard and wider runtime limits. No ontology meaning amendment is necessary for the proposed repairs.

## Actual execution evidence and limits

- Recompiled the exact current `authoritative_workspace_reader_guard.rs` using `rustc --test` and existing Cargo dependency artifacts, with the correct `CARGO_MANIFEST_DIR`: **22 passed**. Probe uses the same compilation method and adds only the isolated fixture described above.
- Reused the supplied current SCM unit artifact `target/debug/deps/g5_scm-55905a683e8286c2`: **132 repository tests, 9 resolution tests, 3 safety tests, and 1 executable-lookup test passed**. The `commit_preparation::` test filter selected zero tests; it is not counted as runtime evidence.
- `cargo metadata --offline --locked --format-version=1 --no-deps` succeeded. Captured at `/tmp/g5-backend-closure-review/reviewer2-metadata.json`.
- `cargo tree --offline --locked -p g5-scm --no-default-features --features native -e normal`, `-p g5-contracts --all-features -e normal`, and `-p g5d-host -p g5d-dispatch -e normal` succeeded and contained neither backend package. Outputs are under `/tmp/g5-backend-closure-review/reviewer2/`.
- Repository test output: `/tmp/g5-backend-closure-review/reviewer2/repo-tests.txt`.
- The standard `cargo test -p g5-api --test authoritative_workspace_reader_guard --offline --locked` initially waited on a shared build lock, later began rebuilding, and was cancelled at main-agent coordination to avoid redundant shared-target work. Its compile progress is not a successful build result. Main-agent API/g5d compilation is separate evidence, not imported into this independent report.
- An initial attempt to link the new public resolver against an older existing SCM rlib correctly failed because that rlib lacked the new function. It was not used as current implementation evidence. The source-extracted resolver probe explicitly avoids that stale API problem; native-only existing error types supplied its types.
- No external provider calls, DB-backed API requests, Linux VM checks, or complete API/g5d builds were independently run in this discovery. They are not required to demonstrate the two reproduced issues, and no full-design completion claim is made.

## Bounded repair follow-up — B1/B2/B3

Follow-up requested after the initial consolidation was saved in `.g5/ontology-review-git-abstraction.md`. This section preserves the initial report above. Scope is only the executable-selection repair, its API selection helper/test, the all-backend guard/calibration changes, and the newly discovered test-fixture error conversion. No production or ontology edits or additional delegation occurred.

### Disposition and evidence

- **B1 / F1 resolved for the reproduced case.** The shared lookup now calls `faccessat(AT_FDCWD, path, X_OK, AT_EACCESS)` on Unix, retaining the ordinary-file check and skipping inaccessible candidates. The public-helper regression covers modes `0610` and `0641`, accounts for root's different permission semantics, and executes the selected program. The API helper keeps explicit override precedence and the existing fallback. Re-running the initial source-extracted resolver probe now selects `second/git`, agrees with `which`, and successfully executes it. A second isolated probe includes the exact current API `select_git_executable` body (with its reference routed to the same extracted lookup): usable PATH, explicit override, missing PATH entry and absent PATH assertions pass. Files: `/tmp/g5-backend-closure-review/reviewer2/{lookup_repaired_probe,api_selection_repaired_probe}.rs`. This remains source-extracted runtime evidence; the main agent's actual SCM/API unit-test builds are separate evidence. No Linux/root/ACL fixture was independently run here.
- **B2 / F2 resolved for the reported controls.** Backend identifiers are now checked in import/path contexts, including `use_wildcard`. The local parameter/value controls pass while actual backend paths, aliases and wildcard imports reject. The earlier shared log contained a failing wildcard-import calibration; the current source includes the repair, and my own newly compiled exact-source guard runs **24/24 passing**. Output: `/tmp/g5-backend-closure-review/reviewer2/guard-followup.log`; binary: `guard_followup` in the same directory.
- **B3 native coverage repaired, with one new gating-calibration issue below.** Native process checking now applies across non-SCM backend crates. Current calibration rejects both literal Git and configured aliased constructors in a newly named crate. I inspected the new exceptions against actual call sites: selected agent-provider programs/auth commands, `passt`, `mkfs.ext4`, the `g5d vm-exec` launcher, and admitted process-isolation user commands. They preserve existing distinct operation ownership and are restricted to the named owner/function/expression; tests reject a new sibling function or a literal Git substitution. The user-command execution exception is consistent with the distinction between platform Git operations and user tools.
- **Fixture compatibility repair is appropriate.** `backend/g5-api-core/src/services/remote_integrations/worker.rs:3001` now explicitly constructs the same owned `GitError` message in a test fixture instead of calling the removed `From<git2::Error>`. Its value is equivalent to the removed conversion, and production classification logic is unchanged. The main agent reported finding it while compiling core's unit tests; I inspected the bounded diff but did not independently run that build.

### Follow-up finding F3 — P3: dispatch fixture gating check is textual and uncalibrated

The new `dispatch_git_fixture_stays_test_feature_gated` test in `backend/g5-api/tests/authoritative_workspace_reader_guard.rs:1942` uses whitespace removal and `contains` to verify the feature gate. It accepts a commented-out old declaration plus an unconditional real module declaration, and even accepts a file containing only the commented-out declaration. Conversely, a harmless comment between the actual attribute and module makes it fail. The production declaration is currently correctly gated; this is a new structural-guard calibration defect rather than an observed production exposure.

The exact predicate yields:

```text
guarded: true
unguarded_with_old_declaration_commented: true
guarded_with_comment: false
missing_module_with_old_declaration_commented: true
```

Example false pass:

```rust
/* #[cfg(feature="test-helpers")] pub mod dispatch_test_support; */
pub mod dispatch_test_support;
```

**Proposed repair:** inspect the actual `mod_item` and its attached `cfg` attribute, reusing/generalizing the adjacent existing AST-based host-fixture check. Add controls for a missing/commented module or gate, an unconditional module, and harmless comments/formatting. This follows B3's requirement to preserve fixture exceptions only when their actual entry module is test-feature gated, plus `g5.quality.review.guards` and `g5.architecture.git_guard`. The fixture bypass exception itself need not be broadened or removed.

No broader review was started. B1 and B2 are closed on the evidence above; B3's new-crate process boundary is closed for the reviewed direct/aliased cases, with this gating-calibration repair remaining. Existing limits on arbitrary macro/type expansion remain unchanged.
