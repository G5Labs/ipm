# Backend dependency closure: independent review 3

## Review basis

Fresh independent, full-scope, read-only discovery review against `59c81a763d3c29760d8f4834e6cad73ce952d60b`. Reviewed all 18 paths in `/tmp/g5-backend-closure-review/files.txt`; every path still matched `snapshot.tar` when checked. Inputs were the saved original request, the pinned design beginning with `Ontology changes and governance`, current governing ontology and relevant footer entries, the frozen diff/source, and actual affected consumers. No author explanation, prior review report, or peer finding was used. The parent supplied current build-artifact locations only. No delegation or production/ontology edits occurred.

This increment closes backend dependencies and error conversion, broadens the backend guards, and moves API executable lookup into SCM. Provider consolidation remains explicitly open. The branch-confidentiality extension remains parked, and no retry persistence or automatic replay is authorized or introduced here. The working ontology has no uncommitted changes in this increment; the design adds only its final-backend-closure section.

## Initial finding

### R3-1 — P2: API discovery can select a Git file that the API process cannot execute

Affected changed call: `backend/g5-api-core/src/services/mod.rs:850`, through the new `g5-scm::native::locate_executable` (`backend/g5-scm/src/native/mod.rs:219–225`). The reused resolver's acceptance predicate is `backend/g5-scm/src/native/process.rs:56–74`.

The old API lookup used `which git`, which skips a candidate that is not executable by the calling user. The shared resolver accepts a file whenever **any** Unix execute bit is set. These are different policies: a current-user-owned file with mode `0641` is not executable by that owner, even though the other-execute bit is set. With that file first on PATH and a usable Git second, the new API startup records the first path successfully, so its fallback does not apply. `NativeGit::api` canonicalizes this existing path and later Git operations fail with permission denied instead of using the valid later executable. This newly affects smart-HTTP advertisement/fetch/push, flush proof observations, and API GC, all of which consume `AppState.git_bin` or its maintenance copy.

Evidence executed as UID 501 on this macOS host:

- Isolated fixture: first `git` mode `0641`, second `git` mode `0700`; both regular files with valid shell content.
- `os.access(first, os.X_OK)` returned false; the second returned true.
- `/usr/bin/which git` under that PATH returned the second file.
- A Rust probe compiling the **unchanged exact source** of `resolve_host_binary` and `executable_on_path` returned the first file. Probe source/executable: `/tmp/backend-closure-r3-lookup.rs` and `/tmp/backend-closure-r3-lookup`.
- Executing that selected first file raised `PermissionError [Errno 13]`.
- Fixture root: `/var/folders/yf/bc92js5n0bjg59_gc7ztxqjr0000gn/T/g5-r3-lookup-hn6xsv5o`.
- The current public locator unit test passes, but only distinguishes mode `0600` from `0700`; it does not exercise effective execution permission.

Governing obligation: the design's final closure preserves API executable selection/fallback; `g5.architecture.git_native_runner` and the reused-operation review rule require the new caller's actual policy to remain compatible. This is a newly exposed API regression from reusing an existing predicate, not a request for a wider audit of inherited host behavior.

Proposed repair: make SCM's Unix candidate check reflect execution permission for the current effective process identity (including applicable access controls), continue searching on an unusable candidate, and retain the existing explicit override and final fallback. Add a discriminating lookup regression for a first candidate with an inapplicable execute bit and a valid second candidate. Exercise the API startup selection or an extracted owner-selection helper as well as the SCM locator. Do not reintroduce a lookup subprocess.

## Commitment ledger

| Commitment | Actual path and discriminating check | Result / limit |
|---|---|---|
| `g5.architecture.git_execution`; native-only dependency closure | `g5-contracts/Cargo.toml`, `g5-common/Cargo.toml`, SCM feature graph, API/g5d aggregators; offline locked metadata and normal/build dependency trees | Contracts has no git2 dependency/feature; only SCM has normal direct backend dependency. API/g5d fixtures remain dev-only. Foundation/runtime/worker/host/dispatcher normal/build trees contain neither git2 nor libgit2-sys. g5d still intentionally links through CLI → SCM, as the design explicitly permits. Native-only SCM check passes. |
| `g5.architecture.git_outcomes`; compatibility | Private `backend_error` returns the same `G5Error::GitError { message: error.message().to_owned() }` as the removed From impl. Inspected preparation, repo, resolution, safety and worktree edits, plus contracts' unchanged Display/HTTP sanitization and actual workspace/API consumers | All five edited SCM operation files are byte-equivalent after removing the new explicit mapper and formatting differences. Existing operation-specific wrapping, error kind, sanitized HTTP shape and resolution effect mapping remain intact. 152 focused SCM tests pass. No observed error compatibility defect. |
| `g5.architecture.git_native_runner`; API context compatibility | AppState startup resolver → SCM locator → API proof/protocol/GC callers; existing public locator test and exact-source adverse-permission probe | **Violated in R3-1.** Override precedence and `/usr/bin/git` fallback otherwise remain source-equivalent for ordinary valid string inputs. Full API runtime startup was not exercised. |
| `g5.architecture.git_guard`; `g5.quality.review.guards`; enforcement | Backend crate discovery; every non-SCM src tree; Cargo metadata direct dependency inspection; existing renamed-dependency, test-only cfg, private helper rename, public interface and missing-target controls | Current 22-test guard executable passes. Dynamic crate inventory broadens coverage beyond the old explicit list, and git2/libgit2_sys identifiers are refused. Manifest checks inspect resolved package names and distinguish dev from normal/build dependencies. Structural parsing still is not full Rust type/macro expansion or transitive semantic proof. |
| `g5.design.minimal-api`, `g5.design.abstraction`, `g5.design.information-hiding`, `g5.architecture.dependencies`, `g5.architecture.compatibility` | Private mapper plus one Git-specific locator, owned error values, feature placement and existing facade consumers | Minimal mechanism for the stated closure; no raw backend type, argument vector or environment map is exposed, no upward dependency is added, and g5d's removed normal SCM/workspace-ops imports are unused in its source. Production g5d check passes with no default features. |
| `g5.architecture.git_ref_updates`, locations, credentials | Mechanical mapping changes include ref cuts, preparation, push, reads and worktrees; checked unchanged preconditions/effect sites and existing downstream owner paths | No ref, storage, credential or policy behavior change after normalization. Repository, resolution and worktree tests cover existing refusal, isolation and effect semantics. Real remote provider/VM boundary cases are not re-executed for this mechanical increment. |
| `g5.architecture.git_retry` | Full diff and changed dependency/configuration surfaces | No retry state, loop, queue, table, replay or worker added. Existing resolution uncertainty test passes without replay. |

## Executed evidence

- `/tmp/g5-host-guard-repaired --test-threads=1`: 22 passed. The supplied current binary is newer than the frozen guard source; it reads current sources/manifests.
- `target/debug/deps/g5_scm-55905a683e8286c2 repo::tests --test-threads=2`: 132 passed.
- Same SCM binary, `safety::`, `worktree::`, `resolution::` and exact native executable lookup test: 3 + 7 + 9 + 1 passed. Total focused SCM checks: 152.
- `cargo check --offline --locked -p g5d --no-default-features`: passed in 21.85 seconds.
- `cargo check --offline --locked -p g5-scm --no-default-features --features native`: passed in 1.34 seconds.
- `cargo metadata --offline --locked --format-version=1 --no-deps`: passed; inventory saved at `/tmp/backend-closure-r3-metadata.json`.
- Normal/build Cargo trees for contracts (all features), foundation, runtime-core, worker, host and dispatcher contain no libgit2. g5d's normal tree confirms the deliberately retained CLI → SCM → git2 path.
- `git diff --check`: passed.
- Exact-source normalization of `commit_preparation.rs`, `repo.rs`, `resolution.rs`, `safety.rs` and `worktree.rs`: only conversion/formatting changes.
- Effective-permission lookup comparison: reproduces R3-1. It executes the exact private resolver source in isolation; the API call chain and public wrapper were inspected, not runtime-initialized.
- `cargo check --offline --locked -p g5-api --lib`: started during discovery; its final result will be appended below.

## Metadata and remaining boundaries

After consolidation/repair, update the affected `git_execution`, `git_native_runner`, `git_guard` and dependency/compatibility footer evidence for the completed closure. In particular, the current `git_execution` phrase “CLI backend linkage is dev-only” needs qualification: CLI direct backend dependencies are dev-only, but g5d/CLI intentionally retains the normal transitive SCM/libgit2 path. The plan already states this correctly. Keep provider migration gaps and native runtime/VM limits explicit.

No production repair has been applied. This report records one correctness finding. Full live API/DB/provider integration, Windows executable selection, default libkrun linkage and the wider provider migration are not established by these checks.

### Check completion before sharing

`cargo check --offline --locked -p g5-api --lib` passed in 40.99 seconds, including contracts, SCM, workspace-ops, API core, routes and the API facade. This closes the compile-evidence item above; it does not exercise API startup permissions at runtime.
