# Backend dependency closure — independent review 1

## Initial independent result

Read-only discovery against `59c81a763d3c29760d8f4834e6cad73ce952d60b` on 2026-10-02. All 18 uncommitted files in `/tmp/g5-backend-closure-review/files.txt` were reviewed. The frozen patch matches the working tree; its SHA-256 is `6dd30ce49544395e5d61d5280a07481ec6961c995780b208e81556893289bd3c`.

This is a fresh independent review. Inputs were the original request, pinned design (starting with “Ontology changes and governance”), current governing ontology and metadata, frozen implementation, affected callers, and review/authoring guidance. No implementation-session history, peer findings or prior review reports were inputs. The coordinator supplied existing executable paths as build setup only. No delegation and no production or ontology edits were performed. Probes are isolated under `/tmp/g5-backend-review-1`.

**One correctness finding remains.** The owned backend-error migration and direct dependency closure otherwise preserve the inspected behavior. Provider consolidation remains outside this increment and is not assessed as complete. The parked branch-confidentiality extension is not a requirement of this increment. No new retry state or automatic replay appears in this diff.

## Finding BCR1-1 — P2: PATH lookup accepts a Git executable that this process cannot execute

- **Affected change:** `backend/g5-api-core/src/services/mod.rs:850`, through the new `g5_scm::native::locate_executable` at `backend/g5-scm/src/native/mod.rs:219` and reused `executable_on_path` at `backend/g5-scm/src/native/process.rs:56-74`.
- **Governing obligation:** final backend dependency closure preserves API executable selection/fallback; `g5.architecture.git_native_runner` keeps context-selected binary behavior; `g5.architecture.policy` preserves a valid caller variant when sharing a helper.
- **Counterexample:** put two `git` files on the selected PATH. The first is owned by the running user with mode `0610` (owner cannot execute; group can); the second has mode `0700`. The old `/usr/bin/which git` skips the first and selects the second. The new public SCM lookup accepts the first because it tests whether *any* `0111` bit is set. Executing the selected path returns `PermissionDenied`. Thus an API instance with a usable later Git now selects a non-executable earlier file, causing its native Git protocol/proof/GC operations to fail instead of retaining the previous working selection.
- **Actual runtime evidence:** `/tmp/g5-backend-review-1/lookup.rs` links the current `libg5_scm-a5c2fcd82abd6e5a.rlib` and invokes the real public lookup. On this macOS non-root session it printed:

  ```text
  SCM selected /private/tmp/g5-backend-review-1/lookup/first/git
  which selected /tmp/g5-backend-review-1/lookup/second/git
  SCM selected candidate execution: Err(Os { code: 13, kind: PermissionDenied, message: "Permission denied" })
  ```

- **Proposed repair:** preserve executable-access semantics for the running process in the centralized lookup (including applicable Unix user/group/ACL access), skipping unusable candidates and continuing PATH search. Do not reintroduce a lookup subprocess. Keep explicit API overrides and `/usr/bin/git` fallback policy in the API owner.
- **Regression check:** add the owner-not-executable/group-executable first-candidate case to the public lookup test, with a later executable control; retain the existing all-bits-clear, missing-candidate and successful-selection cases. Where supported, cover access restrictions beyond mode bits. Verify the API lookup still honors override precedence and fallback.

The underlying mode-bit helper predates this diff, but the new API call reuses it in place of `which`; this is an introduced API behavior regression, not a request to audit unrelated native behavior.

## Scope and commitment ledger

| Commitment / changed concern | Actual path and counterexample/check | Result |
|---|---|---|
| `git_execution`, `dependencies`, backend-free shared contracts | `g5-contracts/Cargo.toml`, `error.rs`, `g5-common/Cargo.toml`, `g5-scm/Cargo.toml`, `Cargo.lock`; Cargo metadata over all 25 backend packages; search for remaining `git-errors` and `From<git2::Error>` | No remaining feature/conversion references. Only SCM has a production direct backend dependency. Other direct `git2` edges are dev-only. |
| `git_execution`, `compatibility`; API/CLI aggregator cleanup | `g5-api/Cargo.toml`, `g5d/Cargo.toml`; inspect aggregator source for removed dependency uses and test-helper feature references | Removed dependencies are unused by production aggregator source; fixture dependencies remain. CLI workflows intentionally retain transitive SCM backend use, as design permits. |
| `git_native_runner`; native consumer dependency closure | Normal/build Cargo tree for `g5d-foundation`, `g5d-runtime-core`, `g5d-worker`, `g5d-host`, `g5d-dispatch` | SCM/native and backend-free workspace paths remain reachable; neither `git2` nor `libgit2-sys` appears in the selected graph. |
| `git_outcomes`, existing API compatibility | Private `backend_error` plus `repo.rs`, `resolution.rs`, `commit_preparation.rs`, `safety.rs`, `worktree.rs`; compare old conversion and every migration site | New adapter retains exactly `G5Error::GitError { message: error.message().to_owned() }`. Removal of inserted explicit adapters plus formatting/trailing-comma normalization makes the changed implementation files equivalent to HEAD. Existing explicit category/prefix mappings are retained. 152 focused SCM tests pass. |
| `git_ref_updates`, `git_locations`, authorized reading | Converted repository, preparation, worktree, resolution and value helpers; inspect changed statements and run repository/ref/read/preparation-adjacent regressions | No operation ordering, argument, ref policy, selection or returned value change found. Repository/ref/read/worktree tests pass. Full API consumer integration execution is not claimed. |
| `minimal-api`, `abstraction`, `information-hiding` | Private SCM error mapping; owned public executable lookup; existing SCM public-surface guard | Backend types stay private. The new operation has a concrete startup caller and does not expose a command, environment mutation or generic runner. Public interface guard passes. |
| `git_guard`, `enforcement`, `g5.quality.review.guards` | `authoritative_workspace_reader_guard.rs`; dynamic crate inventory, all-source backend identifier checks, Cargo alias/target/dependency-kind checks, public surface and authorized-reader checks | Existing 22 guard tests pass. Independently compiled exact-source suite plus added new-crate/contracts `git2`/`libgit2_sys`, renamed target-specific build dependency, harmless string and `cfg(test)` fixture controls passes 23/23. Guards remain structural, not arbitrary macro/type-expansion proofs. |
| `git_native_runner`, caller-preserved selection | API `which_git` → SCM `locate_executable` → `resolve_host_binary`; route smart-HTTP/proof and maintenance GC consumers | Explicit override and fallback retained by inspection. Runtime PATH permission counterexample violates preserved behavior; see BCR1-1. |
| `git_retry` | Whole frozen diff | No tracking/persistence/retry mechanism, replay loop or changed recovery ownership introduced. |
| Saved design and ontology consistency | Only plan section added in this increment; current Git execution/native/error/guard, boundaries, principle and guard-calibration bodies and metadata read | No governing block meaning change is needed. After repair, update scoped execution/guard/dependency metadata and the saved ontology-diff evidence to describe final closure without claiming provider completion. Existing footer statements remain partial and do not establish this increment's runtime verification. |

## Check receipts and limits

- `git diff --check`: passed. Working-tree frozen patch comparison: exact match.
- Existing current SCM unit binary `target/debug/deps/g5_scm-55905a683e8286c2` with `repo::tests resolution::tests safety::tests worktree::tests native::process::tests --test-threads=2`: **152 passed, 0 failed**, 80 filtered, 1.69 seconds. These exercise migrated libgit2 error paths, ref refusal/partial behavior, reads/diffs, worktrees and the existing executable test.
- Existing current guard binary `/tmp/g5-host-guard-repaired`: **22 passed, 0 failed**, 3.57 seconds.
- Fresh standalone exact-source guard `/tmp/g5-backend-review-1/guard`, using current tree-sitter/walkdir/serde-json artifacts and one additional independent calibration test: **23 passed, 0 failed**, 3.60 seconds. This repeats the existing suite to establish the independently compiled source and expanded controls, not to claim additional runtime Git coverage.
- Offline locked Cargo metadata: 25 backend packages; SCM is the only normal direct backend edge; contracts has no `git-errors` feature. Normal/build graph inspection for the five native host/runtime/dispatcher crates excludes the backend. A direct inverse-tree query for host `git2` reports that no such package exists in that selected graph.
- Public lookup probe: reproduced BCR1-1. Initial standalone linking lacked the OpenSSL search directory; adding the existing `/opt/homebrew/opt/openssl@3/lib` resolved it. An initial probe assertion compared `/tmp` against canonical `/private/tmp`; correcting only that fixture assertion produced the final successful counterexample run recorded above.
- An attempted `cargo test --profile g5-dev` was stopped after it began rebuilding dependencies; current debug artifacts were then reused. No result is claimed for that interrupted build. The coordinator's broader aggregator compile is separate evidence, not an independently completed check in this report.
- No new end-to-end API/DB/provider/VM checks were run. Existing native lifecycle, security and provider gaps are not reclassified as findings in this bounded mechanical closure review. Full provider migration and branch confidentiality are expressly not completion claims here.

Initial findings and evidence were saved before sharing with the coordinator. Repairs and final dispositions belong to consolidation; this report preserves the original independent finding.
