# Workspace operations backend closure — independent discovery 3

Initial findings saved on 2026-10-02 before reading the author's review explanation or any peer report. This is an independent, full-scope, read-only discovery pass. Production, tests and ontology were not edited. Investigative files and Git repositories are confined to `/tmp/g5-workspace-ops-reviewer-3`. No subagents were used.

## Scope and inputs

- Increment base: `7054bdfb`; frozen patch, copied files and SHA-256 inventory: `/tmp/g5-workspace-ops-review-snapshot`.
- Read `AGENTS.md`, the ontology-review skill and its design-review guide; began with `Ontology changes and governance` and its saved ontology diff, then the original request, complete design including the workspace-operations increment, current governing blocks and affected metadata.
- Inspected the whole implementation/test patch, removed operations and their caller inventory, migrated preparation bodies, remaining checkpoint/merge composition, shared clone/init/current-branch operations, credential assembly, worktree consumers, API bootstrap/preparation consumers and CLI materialization/add/init consumers.
- The increment contains no ontology block edits. The governance section's cumulative ontology diff remains the pre-increment handoff. Relevant metadata still describes workspace-ops/raw-handle closure as remaining work; main should update those assessments after consolidation and verification.
- Deliberately withheld `.g5/ontology-review-git-abstraction.md`, including its patch hunk, because it is the author's explanation. That withheld evidence was not used to discover the results below.
- User constraints preserved: no new retry tracking/replay; sibling-object confidentiality redesign is parked. The retained global safe-directory helper is explicitly later native-context work, not a new finding against this increment.

## Raw initial findings

### R3-1 — Public-interface guard checks only the three former escape-hatch names (P2, guard-coverage defect)

**Rule:** `g5.architecture.git_guard`, `g5.quality.review.guards`, and the workspace-operations design's requirement to extend public-interface guards.

**Path:** `backend/g5-scm/src/lib.rs:29` and `backend/g5-api/tests/authoritative_workspace_reader_guard.rs:167`.

The new compile-fail cases prove only that `from_raw`, `lock_inner`, and `configure_callbacks` are inaccessible. The structural scanner treats `g5-scm/*` as a native owner and checks process construction, but does not inspect public backend-bearing signatures. A newly named public handle accessor therefore passes the scanner while all three compile-fail cases continue passing.

**Counterexample:** copied the actual scanner functions into an isolated Rust probe and passed a syntactically valid `g5-scm/repo.rs` fixture containing `pub fn raw_backend(&self) -> std::sync::MutexGuard<'_, git2::Repository> { self.repo().unwrap() }` plus the four required lease operation names. The scanner returned `[]`. The matching private-helper fixture also returned `[]`. Probe source: `/tmp/g5-workspace-ops-reviewer-3/guard_probe.rs`.

**Consequence:** current production visibility is closed, but the claimed architectural ratchet does not prevent reintroducing the same backend leak under a new name. This is a missing enforcement test, not evidence that the current patch exposes a raw handle.

**Proposed fix/check:** add a scoped public-surface guard for SCM backend types/re-exports/callbacks (including imported aliases), with a public-handle violating fixture and private-helper valid fixture. Retain compile-fail evidence for the former APIs and a positive typed-operation fixture. Do not ban private backend implementation or legitimate typed operations.

### R3-2 — Parentless initial-files preparation does not validate the whole path batch before writing objects (P2, inherited behavior in the migrated scope)

**Rule:** workspace-operations design's complete-validation/exact-files preservation and invalid-batch-before-object-write obligation; `g5.architecture.git_execution`/complete operations govern the newly shared owner.

**Path:** `backend/g5-scm/src/commit_preparation.rs:227` (`validate_isolated_files`) and `:419`–`:443` (`prepare_initial_files_commit`).

Initial preparation retains the weaker isolated-file validator. It does not reject NUL, `.git` components, repeated separators, or ancestor collisions before `repo.blob`/index insertion. A request containing both `nested/child` and `nested` succeeds, but its final tree contains only `nested`: one requested file is silently discarded. Other invalid paths refuse only after prior blobs have been written.

**Runtime evidence:** isolated executable linked against the current cached SCM library, exercising the real public `prepare_initial_files_commit`:

| Second path after a valid unique first file | Result | First blob stored |
|---|---|---|
| `.git/config` | Backend invalid-path error | yes |
| `a/../escape` | ValidationError | no |
| `a//b` | Backend tree-write error | yes |
| `nested`, after first path `nested/child` | Success; `git ls-tree -r` shows only `nested` | yes |
| `nul\0name` | Backend NUL-path error | yes |

Probe source and fixture repos: `/tmp/g5-workspace-ops-reviewer-3/probe.rs`, `probe-0` through `probe-4`. The collision commit observed was `e070f179f34a8fdc35f88f76f35fed7674c488b9`.

**Scope qualification:** comparison with `7054bdfb` confirms these validator and preparation bodies are unchanged except for the private lock accessor. This is not a behavioral regression introduced by relocation. The only current production initial-files caller inspected, `WorkspaceService::initialize_local_workspace_repo`, supplies one fixed `README.md`, so that caller does not expose these malformed batches. Record this distinction during consolidation; do not characterize it as a demonstrated user-facing regression. It does contradict a broad claim that the moved shared preparation surface completely validates exact initial-file batches before object writes.

**Proposed fix/check:** validate every initial path and the whole ancestor/duplicate set before writing any blob, retaining the intentional initial-file restrictions and parentless/no-ref semantics. Add initial-files cases that check object absence on invalid batches and forbid successful loss of a requested file. If main treats preservation-only migration as excluding this inherited defect, retain the limitation explicitly rather than marking complete validation proven.

## Commitment → actual path → counterexample/check → result or gap

| Commitment | Actual path | Counterexample/check | Result or gap |
|---|---|---|---|
| `g5.architecture.git_execution`, `g5.architecture.commands`, `g5.design.information-hiding` | Workspace preparation facade → private `g5-scm::commit_preparation`; `GitRepo::{clone_branch,init,current_branch}`; private `repo`/`from_raw` and callbacks | Raw constructor/lock/callback external compile attempts; search every former symbol and production workspace-ops backend access | Preserved on inspected current paths. All three external attempts failed for the intended missing/private symbol. No migrated production caller assembles a backend object. R3-1 limits future ratchet evidence. |
| `g5.architecture.compatibility`, `g5.design.minimal-api`, `g5.design.abstraction`, `g5.architecture.dependencies` | Workspace-ops re-exports and compatibility wrapper; `Cargo.toml` dev-only git2; SCM private module | Compare moved function bodies; search deleted isolated-worktree/safe-directory functions; inspect composition and dependency direction | Ten moved validators/operations/helpers are textually identical after `lock_inner`→`repo` normalization. The unused unsafe branch writer is removed, with no remaining caller. Existing wrapper retains the unused legacy path argument for compatibility; no new policy layer or upward dependency. |
| Exact-tree preparation; `git_ref_updates`, `git_owner`, `g5.data.git-authority` | `prepare_tree_commit_from_exact_base`; exact-files wrapper; in-memory index; `.commit(None,...)` | Mixed write/delete, immutable parent, unchanged index/dirty checkout/HEAD, no-op, bare repository, concurrent handles, mode preservation, literal pathspec-like filename, invalid batch object absence | Cached current workspace tests pass. Preparation writes objects only and never names an authoritative branch. Runtime coverage demonstrates the public workspace facade reaches the moved implementation. Initial preparation has R3-2. |
| `g5.architecture.workspace_registry`, Git authority and CAS ownership | Workspace local initialization → initial candidate; org routes → `prepare_initial_branch_commit_in_tx(...).land(...)`; provenance preparation returns candidate | Read current API call chains; verify candidate creation stays separate from checked landing | Source preserved. Initial-files/empty unit tests prove parentless objects and unchanged ambient HEAD. API/database owner transition was not executed by this reviewer; main owns those shared resources. |
| `g5.architecture.content_reader`, `g5.architecture.projection_owner` | Existing authorized input reads in provenance/ontology consumers; reader guard | Diff introduces no direct content-reader bypass; review actual caller inputs and preserved guard paths | No new reader-policy movement found. No new claim of exhaustive reader/API conformance; API runtime evidence left to main. |
| `g5.architecture.git_ref_updates`, `git_locations` | Shared init/clone/current branch; unchanged branch composition; internal worktree lock use | Bare/worktree init, selected/default clone branch and origin identity, missing branch error, detached/unborn HEAD, worktree cleanup from bare/worktree parent | All relevant cached tests pass. Local clone request preserves existing URL and depth mechanics. No authoritative-write grant is imported into materialization. Remote shallow/authenticated clone remains unexecuted here. |
| `g5.architecture.git_credentials` and redaction boundary | `GitAuth::configure_callbacks` becomes crate-private; `clone_backend` reuses it | Search all callback assembly callers; compare old/new clone transport behavior | No remaining external callback assembly. Existing `GitRepo::clone` still normalizes its Token transport; workspace `clone_branch` still uses caller-selected URL unchanged. This preserves existing variants. No live provider/SSH network authentication test in this pass. |
| `g5.git.checkpoint.selected`, `.isolation`, `.retry`; `g5.git.exclusions` | Unchanged checkpoint/merge/workspace composition and moved exact-tree mode/path logic | All 48 workspace-operation unit tests, including checkpoint/merge variants; durable `.g5` file vs runtime metadata exact-tree tests | Passing existing composition evidence. The selected-file live API operation is not newly migrated here; no whole-system checkpoint concurrency/retry claim. Exact-tree exclusions preserved; initial-files validation is separately limited by R3-2. |
| `g5.architecture.git_guard`, `g5.quality.review.guards/cases` | Expanded all-source workspace scan, generic dependency scanner, alias/test fixtures, SCM doctests | Inspect source/dependency fixture logic; compile former public escape hatches; run copied scanner against new public raw-handle and private-helper fixtures | Workspace production git2 normal/build dependencies and explicit aliases are covered structurally; dev fixture allowance remains. R3-1 is the public-surface coverage gap. Current API guard test binary was not rerun because main owns API runs; older cached API artifact was deliberately not presented as current evidence. |
| `g5.architecture.git_retry`, `g5.architecture.recovery`, `g5.design.minimal-mechanism` | Entire frozen increment | Search additions for persistence, loops/replay, worker ownership and provider grant changes | No new retry state, automatic replay, service, schema, or grant. Existing recovery owners unchanged. Parked sibling-object isolation and global safe-directory migration remain outside this increment's completion claim. |

## Executed checks and limits

1. Verified current implementation/design hashes against the frozen inventory; no mismatches. Author review file was deliberately excluded from content access.
2. Ran `target/debug/deps/g5_workspace_ops-c47d1d938919f4b4 --test-threads=4` with isolated `TMPDIR`: **48 passed**. Its inventory includes this increment's new tests, so an older cached binary was not mistaken for the current one.
3. Ran `target/debug/deps/g5_scm-582ca877ddba6491 worktree::tests --test-threads=4` with isolated `TMPDIR`: **7 passed**, 179 filtered.
4. Compiled three external `rustc --emit=metadata` access probes against `libg5_scm-d0a5004204312319.rlib`: `from_raw` private, `lock_inner` absent, callback configurator private.
5. Compiled/ran the initial-files behavior probe against that current library. First link attempt lacked the native OpenSSL search path; retry with `/opt/homebrew/opt/openssl@3/lib` succeeded. This was a local probe setup issue, not a product failure.
6. Compiled/ran the actual structural scanner copied into scratch against the R3-1 fixtures: both returned no violations.

No fresh whole-workspace build, API/database tests, CLI commands, provider traffic, shallow network clone, live SSH handshake, or native-only feature build was executed in this discovery pass. Main owns API/database/CLI runs. Tests above establish only their evaluated local/public-library paths. Existing cached artifacts were reused; source inspection and hash checks tie the review to the frozen checkout, but no claim of a fresh independent build is made.

No production or ontology repair was performed. The initial implementation and evidence remain available for main-agent consolidation; no peer finding or author explanation was used to derive this report.
