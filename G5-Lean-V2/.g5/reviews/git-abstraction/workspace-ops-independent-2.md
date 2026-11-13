# Workspace operations backend closure — independent review 2

Initial discovery saved 2026-10-02, before reading the implementation author's review explanation or another reviewer's findings. This is a fresh independent, read-only review under `ontology-review`; no subagents were used. Production, tests and ontology were not edited. Investigative code, binaries and repositories are confined to `/tmp/g5-workspace-ops-review-2`.

## Scope and inputs

Comparison base: `7054bdfb`. Frozen patch: `/tmp/g5-workspace-ops-review-snapshot/increment.patch`; source manifest: `/tmp/g5-workspace-ops-review-snapshot/sha256`. All twelve backend source/manifest files match their frozen hashes at inspection. Reviewed the complete production/test diff, including new `commit_preparation.rs`, removed APIs/tests, visibility changes, clone/init/branch moves and guards. The author's appended `.g5/ontology-review-git-abstraction.md` explanation was intentionally excluded from discovery.

Started with the pinned design's first **Ontology changes and governance** section and its actual saved ontology diff, then the original request, full design including **Workspace operations backend closure**, current Git/governance/workspace/boundary contracts and affected footers, relevant data authority, security, durability, design principles and guard calibration contracts. The increment changes no ontology body or footer against `7054bdfb`; the saved broader design diff remains the governing inventory. Its ontology metadata still describes workspace-ops/raw handles as migration work and needs the main agent's scoped assessment after consolidation.

User constraints remain binding: no new retry tracking/replay, and sibling-object confidentiality redesign is parked. The live global `safe.directory` helper and broader host/API/CLI/backend/provider migration are explicitly deferred by the saved design; I do not count them as new defects here.

## Initial findings

### F1 — P2: public-interface regression coverage protects old names, not the closed boundary

Commitments: `g5.architecture.git_guard`, `g5.architecture.git_execution`, `g5.quality.review.guards`; the increment explicitly requires extending public-interface guards.

Affected paths: `backend/g5-scm/src/lib.rs:28` and `backend/g5-api/tests/authoritative_workspace_reader_guard.rs:151`.

The three new compile-fail examples prove that `from_raw`, `lock_inner` and `configure_callbacks` are unavailable. They do not detect reintroduction of the same capability under a different name. The shared source checker treats `g5-scm` as a native-execution owner but never inspects its public backend signatures or re-exports. An isolated copy of the exact checker accepted both:

```rust
impl GitRepo {
    pub fn backend_handle(&self) -> std::sync::MutexGuard<'_, git2::Repository> {
        self.repo().unwrap()
    }
}
pub use git2::Repository as BackendRepository;
```

`typed_git_violations("g5-scm/repo.rs", source, &[])` returned `[]` for each fixture. The old compile-fail names would remain unavailable, and the production dependency checks would remain green. This is a regression-enforcement gap, not an existing raw public accessor in the frozen implementation: current raw APIs are successfully private/removed.

Proposed fix: add a public-interface ownership check that rejects backend types/exports in the shared crate's externally callable surface (including a renamed accessor or re-export), while allowing equivalent private helpers. Keep the existing compile-fail examples as additional visibility checks. Calibration should include a forbidden renamed public handle/callback/type alias, an allowed private rename and a missing expected surface case. Scratch evidence: `/tmp/g5-workspace-ops-review-2/guard-probe.rs` and `guard-probe.log`.

### F2 — P2, inherited preparation defect: initial exact-files batches can lose entries and write objects before rejection

Commitments: the increment's preserved complete-validation/exact-files semantics, `g5.architecture.commands`, `g5.design.minimal-api`, `g5.architecture.git_outcomes`.

Affected path: `backend/g5-scm/src/commit_preparation.rs:224` and `:410`.

`prepare_initial_files_commit` reuses `validate_isolated_files`, which checks neither ancestor collisions nor NUL paths. It writes each blob before `Index::add` discovers backend-invalid input. Independent calls through the current public `g5_scm` API reproduced both outcomes:

- Files `parent` and `parent/child` return `Ok(commit)`, but only `parent/child` exists as a blob in that commit. The index silently replaces the earlier requested file with a tree. The result therefore does not contain the exact accepted batch.
- A fresh repository and batch `[valid.txt, bad\0name]` return a generic Git error for the second path, but the first blob has already been written. The probe explicitly observed that blob absent before invocation and present afterward (`74a978e3e648f1246bcebd8e0bd461c9c4e1c6ea`). This falls short of the design's invalid-batch-before-object-writes expectation.

This logic is inherited unchanged from `7054bdfb` and is **not a semantic regression caused by the move**. The new public shared operation carries that gap into the claimed complete preparation boundary. The presently found production initial-files caller supplies a fixed single `README.md`; no current API input exploiting the collision was found. Treat this as a scoped reused-operation conformance defect, not a claim that ordinary bootstrap now loses data.

Proposed fix: validate the complete normalized initial file batch, including unsupported paths and ancestor collisions, before any object creation; retain its intended existing initial-file policy and valid path behavior. Regressions should assert refusal and unchanged object inventory for an invalid later path, collision refusal in both input orders, and exact multiple-file parentless success without ref/index/checkout changes. Scratch evidence: `/tmp/g5-workspace-ops-review-2/probe.rs`, `probe.log`; built against the current cached `libg5_scm-d0a5004204312319.rlib`.

## Commitment → actual path → counterexample/check → result or gap

| Commitment | Actual path | Counterexample/check | Result / gap |
|---|---|---|---|
| `git_execution`; `commands`; `dependencies`; `compatibility`; information hiding | Workspace-ops preparation re-exports/wrapper → private `g5-scm::commit_preparation`; clone/init/current-branch → `GitRepo`; `GitWorktree` internal lock use | Search all backend consumers; inspect manifests and exported types; externally compile old raw APIs; run exact current source guard | Current API exposes no raw repository/index/callback in changed surface. `from_raw` and callback access fail as private, `lock_inner` is absent. Workspace backend dependency is dev-only. No upward dependency introduced. |
| `git_guard`; structural guard calibration | `authoritative_workspace_reader_guard.rs`; shared crate doctests | Exact guard source compiled independently; source/dependency/fixture tests; renamed raw accessor and re-export fixtures | All 10 existing checks pass; F1 records the missed public-interface violation. Tests/fixtures remain allowed. |
| Increment: immutable exact-base parent, exact bytes, no-op, mode preservation | `prepare_tree_commit_from_exact_base`; legacy files wrapper | Mixed add/write/delete; literal `:(glob)*.txt`; symlink/executable modes; unchanged-tree return; invalid base/modes/collisions | Existing workspace tests pass; two-phase path/base validation precedes object writes for covered exact-tree cases. No unintended code change found in moved logic. |
| Increment: isolation and allowed durable `.g5` content; `git_ref_updates`; Git authority | `commit_preparation.rs`; workspace-ops tests | Dirty checkout, unchanged real index/ref, concurrent independent handles, bare repository product profile | Tests pass. Preparation uses an in-memory index and `commit(None, ...)`; it does not acquire branch publication authority or mutate the checkout. |
| Increment: parentless initial preparation | `prepare_initial_files_commit`, `prepare_initial_empty_commit` | Parent count zero and unchanged ambient ref; independently probe invalid later input and colliding paths | Normal tests pass; F2 is an inherited batch-validation defect carried into the shared API. |
| `git_owner`; `git_ref_updates`; `g5.data.git-authority`; workspace registry | `workspace_service::initialize_local_workspace_repo` → initial preparation; `routes/orgs.rs::bootstrap_base_{code,docs}_repo` → `prepare_initial_branch_commit_in_tx` → `PreparedBranchCommit::land` | Inspect validation before storage, unattached candidate, Base/inventory checks, exact expected-head handling and checked landing | Source inspection preserves existing policy ownership and candidate/publication split. API/database runtime verification belongs to main; not claimed here. |
| Authorized reader; source-vector consistency | `intent_log_service::prepare_provenance_backfill` and `prepare_repo_provenance`; ontology preparation helper | Captured view equals requested base; reads via `CommittedRepoReadView`, exact base passed into shared preparer | Source checks preserve selected immutable base and reader-owned content retrieval. No new read bypass found. Repository-wide search found no external production call to the two service preparation entry points; they were inspected as retained API surfaces, not asserted live end-to-end flows. |
| Increment: remove only unused APIs; minimal sufficient API | Removed isolated-worktree file APIs, request types and local safe-directory helper | `git grep` on `7054bdfb` for deleted names plus current references | Baseline references consist of definitions, re-exports and their unit tests; no production consumers found. Removal does not bypass checked landing. Global helper retained as declared migration work. |
| Increment: selected/default branch, clone depth, transport variants; `git_locations` | `GitRepo::{clone,clone_branch,clone_backend,init,current_branch}` and workspace-ops delegates | Local clone default/selected/missing branches, origin identity, worktree/bare init, detached/unborn branch read; compare old builders/options | 48 workspace tests pass. Depth is passed unchanged; old `GitRepo::clone` transport normalization remains separate from workspace clone's unchanged supplied URL. Shallow and real SSH/HTTPS clone behavior not executed by this reviewer. |
| `git_credentials`; publication/platform-token boundaries; secret redaction | Callback visibility change and private `clone_backend` | Inspect credential selection unchanged; auth unit checks; compile callback access from outside crate | Injection is now private to SCM; 9 auth tests pass. One local HTTP challenge test cannot bind its socket under sandbox (`EPERM`), so its runtime credential mediation is unverified here. No new credential authority introduced. |
| Clone/init consumer compatibility; distinct layouts | `g5d-cli/{init,add,workspace_manifest}.rs`; API docs initialization | Inspect ordinary/remote default selection, explicit commit pinning, existing transport split and path-owner inputs | No caller signature/selection/layout regression found. Main owns CLI and API runtime runs; no end-to-end runtime claim here. |
| Worktree reuse after lock removal | `g5-scm/src/worktree.rs` → internal `repo()` | Existing worktree/bare/remove/stale metadata tests | All 7 worktree tests pass. Raw lock removal preserves internal synchronization mechanism. |
| Checkpoint, merge and multi-repository composition | Unchanged remainder of `g5-workspace-ops` and callers | Run workspace unit suite including selected-path checkpoint, merge/rebase/reset/workspace composition | All 48 pass. New preparation placement does not change these implementations; full concurrent native checkpoint/publication guarantee is outside this unit evidence. |
| `git_retry`; recovery owner; parked MVP object visibility | Complete increment; prepared candidates and caller errors | Check added fields/persistence, retry loops/workers, new credential or confidentiality mechanism | No new retry persistence/replay/worker, grant or sibling-object isolation mechanism. Native global config and broader providers remain explicit migration gaps, not newly inferred guarantees. |
| `git_outcomes`; effect evidence | Object-only preparation; clone/init wrappers; unchanged policy landing | Inspect result/error propagation, candidate vs confirmed branch effect | Refusal/ordinary errors remain legacy `G5Error`; no automatic rollback/retry claim added. F2 highlights generic error after object writes. Richer outcome migration beyond these paths is unverified/deferred. |

## Verification receipt and limits

Executed independently, using isolated temporary repositories:

- Cached current workspace unit binary `target/debug/deps/g5_workspace_ops-c47d1d938919f4b4`: **48 passed**, including all new tests in the frozen patch.
- Cached current SCM binary `target/debug/deps/g5_scm-582ca877ddba6491`, `worktree::tests::`: **7 passed**.
- Same SCM binary, `auth::tests::`: **9 passed, 1 environment-blocked failure**. `libgit2_401_challenge_cannot_extract_scm_token` fails at `auth.rs:245` when binding `127.0.0.1:0`, before Git/network behavior, with `PermissionDenied / Operation not permitted`.
- Exact current guard source compiled with cached dependencies directly to `/tmp`, with `CARGO_MANIFEST_DIR` set to the real API crate: **10 passed**. This includes the offline locked Cargo dependency metadata check.
- External privacy compilation: **3 intended compile failures**; positive public operations also compile and execute in the scratch probe.
- Renamed-public-backend checker fixtures: **2 accepted invalid surfaces**, reproduced with an isolated copy of the current checker.
- Current public initial-files probe: **2 reproduced counterexamples**, collision success with an omitted requested blob and object writes preceding invalid-path failure.

No database/API/CLI suites, full workspace rebuild, guest execution, shallow/network clone matrix, native-only feature rebuild, full rustdoc suite or real provider calls were run by this reviewer. Main owns shared API/database/CLI checks. Cached unit artifacts were supplied as current and include the new tests; the guard itself was rebuilt from exact current source. Source-file hashes matched the frozen manifest. No performance claim or measured comparison with another review mode is made.

## Proposed ontology/design reconciliation

After consolidation, update `git_execution`/`git_guard` implementation metadata to reflect the workspace backend move and actual public visibility, while retaining F1 until its enforcing check exists. Record the inherited initial-batch gap under the preparation assessment or review evidence until repaired; do not broaden the guarantee based on passing exact-base tests. Retain native/global-config/provider and live transport verification gaps. No governing meaning amendment or retry persistence is needed for either proposed fix.

## Bounded repair recheck — 2026-10-02

This appended assessment preserves the initial findings above. Inputs were main's saved consolidation in `.g5/ontology-review-git-abstraction.md`, actual repaired preparation/guard code, and the new initial-batch regression. No broad rediscovery, delegation, production/test/ontology edits, or API/database/CLI runs were performed. Scratch evidence is under `/tmp/g5-workspace-ops-review-2/recheck`.

### F2 disposition: repaired for the evaluated path/batch contract

The new shared `validate_exact_tree_paths` runs on the already-normalized initial file paths before repository lock acquisition, blob creation or index writes. Exact-base preparation reuses the same validation without losing its original order or path/change correspondence. Initial trimming and blanket initial `.g5` rejection remain unchanged; initial commits remain parentless and unattached.

Recompiled and reran my original public-API probe against current `libg5_scm-d0a5004204312319.rlib` in a fresh scratch repository:

- NUL later path now returns `ValidationError`; the first candidate blob is absent both before and after the operation.
- Ancestor collision now returns `ValidationError`, instead of a successful commit missing the first file.

Ran supplied current workspace unit binary `g5_workspace_ops-d8c832db55b2d315`: **49 passed**. Inspected the added regression, which compares complete ODB inventories, real index bytes, authoritative HEAD and dirty checkout bytes for both collision orders, invalid later paths, duplicates after trimming and empty batches. Its valid two-file case verifies both exact blobs, retained trimming, literal pathspec-like filename, parentless commit and unchanged refs/index/checkout.

This is a minimal repair of the reused validator gap: it shares the existing canonical-path/collision rule rather than adding new storage, publication or recovery policy. No directly exposed preparation regression was found. This closes F2 for the evaluated path/batch cases, not arbitrary disk/I/O failure atomicity.

Evidence: `recheck/probe.log`, `recheck/workspace-tests.log`.

### F1 disposition: original counterexamples fixed; two directly adjacent guard calibration gaps remain

The repaired checker scans source signatures, fields, aliases and re-exports, follows named module-scoped aliases, and allows private raw fields and methods in owned public wrappers. It now rejects both of my original fixtures: newly named public repository-lock accessor and aliased public backend re-export. The twelve repository guard tests pass when freshly compiled from current source; a scratch copy with two additional review test functions has **14 passing tests**. The additional diagnostic cases expose these residuals:

1. **False negative: a private wildcard backend import hides a public raw return type.**

   ```rust
   use git2::*;
   pub fn handle() -> Repository { todo!() }
   ```

   `scm_public_backend_violations(&[("repo.rs", source)])` returns `[]`. `imported_paths` records the wildcard as an alias named `*`, but `backend_type` cannot relate unqualified `Repository` to that wildcard. This is still an explicit newly named public backend escape, so F1 cannot be fully closed on the current repair. The direct public wildcard re-export fixture in the production test does not exercise private wildcard import followed by public use. A scoped prohibition of backend wildcard imports, or sound treatment of their imported types, can close this without recreating all Rust name resolution.

2. **False positive: an entirely private associated-type helper is called public.**

   ```rust
   trait Private { type Item; }
   struct Hidden;
   impl Private for Hidden { type Item = git2::Repository; }
   ```

   The checker returns `repo.rs:1: public SCM surface exposes a backend type`. `collect_surface_rules` marks every `type_item` under any implementation as exported through `exported || public_trait || in_impl`, without considering whether the implementation's self type and trait are private. This rejects valid backend-private implementation detail; it is a directly exposed calibration regression under the required harmless-private-helper case. Preserve rejection for public wrappers exposing raw associated types, while allowing genuinely private implementations.

Both results are checker behavior, not a newly discovered raw API in production. They require small targeted calibration/logic changes before claiming F1's boundary and private-helper requirements are covered. The existing 12 green tests do not distinguish these cases.

Evidence: `recheck/guard-recheck.rs`, `recheck/guard.log`; the original fixtures returned violations, the private wildcard/public return returned none, and the private associated-type helper returned a violation. No API/CLI runtime conclusions are added by this recheck.

## Narrow final guard recheck — 2026-10-02

Rechecked only the two residual F1 calibration cases and the original public-interface examples against the final guard source, SHA-256 `1b270507ff2d12d62eb0e8f33ec5b4ef4452c1c5f3ffd873d4304af0d6a5fbd8`. The current implementation now accounts for backend wildcard imports and retains the implementing trait's identity, including imported aliases, when deciding whether an associated type is public. I inspected those changes and the added negative/positive fixtures; production and ontology remained untouched.

Freshly compiled the exact guard source plus two isolated review test functions to `/tmp/g5-workspace-ops-review-2/final-recheck`. All **14 tests passed**: the 12 repository guard tests and my two review functions. Independently verified:

- A renamed public raw repository-lock accessor and aliased public backend re-export are rejected.
- The original private `use git2::*` followed by public `Repository` return is rejected.
- A public wrapper exposing backend `Iterator::Item` is rejected; the private-trait exception does not blanket-exempt associated types.
- The original wholly private trait implementation is accepted.
- Private implementations using same-module trait aliases, parent-module aliases and cross-file `crate::` aliases are accepted.
- An owned public wrapper with a private backend field/helper and an owned public result is accepted.

**F1 disposition: closed for the reviewed explicit surface and calibration obligations.** The previous false negative and false positive no longer reproduce. These are test-only enforcement changes and add no production API, persistence or recovery mechanism. As documented in the checker, this is a structural safeguard over explicit syntax rather than complete Rust compiler name/type resolution; no universal dynamic-bypass claim is made. F2's earlier repaired disposition and 49-test preparation evidence remain unchanged.

Evidence: `final-recheck/guard-final.rs`, `final-recheck/compile.log`, `final-recheck/guard.log`. Main's separately reported API results were not rerun or counted as independent evidence here. No additional finding arose within this narrow final recheck.
