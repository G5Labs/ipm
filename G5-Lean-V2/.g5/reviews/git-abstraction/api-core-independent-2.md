# API core Git backend closure — independent review 2

Initial discovery report, 2026-10-02. Review mode: independent, full assigned scope, read-only production/ontology discovery. No delegation. The reviewer received the approved request/design and the scope statement (no new retry state/replay; sibling-object isolation parked), not the author's implementation narrative or peers' findings. Those scope constraints are supplied hints, not independently discovered defects.

## Snapshot and inputs

- Increment base: `d8f6e729`; frozen patch and source hashes: `/tmp/g5-api-core-review-snapshot/increment.patch` and `/tmp/g5-api-core-review-snapshot/sha256`. Every listed hash matched current source during review.
- Read `AGENTS.md`, ontology-review skill and design-review guide, original `.g5/chat_intent/design-git-abstraction/request.md`, pinned `.g5/ontology-plan-git-abstraction.md` beginning with `Ontology changes and governance`, including the saved actual ontology diff, approved design and API core backend closure scope.
- The saved ontology diff exactly equals `git diff --unified=0 093e08a7 -- .g5/ontology/concepts/software_architecture.md .g5/ontology/software_architecture/git_governance.g5.md`. The current increment does not change ontology.
- Read current Git/governance owning commitments/footer evidence, workspace reader/registry/fork blocks and footer evidence, Git content authority, complete operations/dependency direction, minimal API/abstraction/enforcement, and guard/case calibration commitments. Previously recorded wider migration limitations are not treated as new findings.
- Inspected all changed production, manifest, test and guard paths. The author's `.g5/ontology-review-git-abstraction.md` narrative was intentionally not read.

## Initial findings

No confirmed correctness or scope defect found in this bounded increment. The migration preserves the existing operation policy and error categories on inspected paths. This finding is limited to API core's direct backend closure; it is not a claim that the whole platform Git abstraction is closed.

The source inspection specifically confirms:

- `CommittedRepoReadView::committed_at` retains its captured immutable OID; `read_repository_versions` resolves a selected branch once and obtains the committer timestamp from that exact OID. `resolve_branch_head` peels the exact local `refs/heads/...` target, without remote/default/checkout fallback. `commit_committed_at` uses `find_commit` rather than the mutable branch or author time.
- `branch_commit_state` retains the old bootstrap distinction: present commit; selected ref absent plus unborn HEAD and no refs of any kind; unavailable selected branch in other states. A found dangling symbolic ref, missing object or non-commit target still errors. Missing storage remains separately handled by the existing reader policy.
- `open_or_init_with_default_branch` validates branch syntax before its own filesystem effects and only opens/initializes storage and sets symbolic HEAD. The API owner also validates before `RepoStorage::init_repo`. `prepare_initial_files_commit` still creates a parentless unattached candidate using an isolated index. The route consumer subsequently calls `prepare_initial_branch_commit_in_tx(...).land(...)` rather than treating preparation as publication.
- Hash/OID/ref helpers preserve the same libgit2 primitives. Artifact limits, SHA256, content-addressed paths and storage policy remain in the review service; observation TTL, integration identity and cache failure behavior remain in `RemoteObservations`; workspace name length, reserved-name and `@` checks remain in the fork owner.

## Commitment → actual path → check/result or gap

| Commitment | Actual changed path and affected consumer | Counterexample/check | Initial result or gap |
|---|---|---|---|
| `g5.architecture.git_execution`; `g5.architecture.dependencies` | API core manifest; all seven migrated service files; `g5-scm::{lib,repo,safety}` | Production source scan plus Cargo metadata; inspect public helper inputs/outcomes and backend implementation placement | Direct production `git2` use removed from API core; dependency now dev-only. Exact guard runtime passes. Transitive libgit2 remains intentionally through `g5-scm`/workspace-ops; this is not native-only dependency closure. |
| `g5.architecture.content_reader`; `g5.data.git-authority` | `workspace_reader::CommittedRepoReadView::committed_at` → `GitRepo::commit_committed_at`; `doc_service::capture_docs_list` | Change branch after capturing view; missing/non-commit selected OID must error; inspect actual owner authorization and exact-OID propagation | Source preserved; new API integration test discriminates committer vs author time and later branch movement. Main agent owns API/database execution; not independently executed here. |
| `g5.architecture.workspace_registry`; `g5.data.git-authority` | `workspace_repositories::read_repository_versions` → `resolve_branch_head`/`commit_committed_at`; approval source capture, branch tracking and integration-worker consumers | Timestamp must belong to captured head, including distinct author/committer time; no mutable default/remote fallback | Source preserved. New test asserts exact captured OID/time and later view behavior. API/database execution delegated to main's shared validation. |
| `g5.architecture.content_reader`; `g5.architecture.workspace_registry` | `expected_unborn_workspace_authority_cuts` → `branch_commit_state`; Home/open-question source capture | Nondefault unborn HEAD with no refs allowed; tag-only/populated missing branch unavailable; found dangling/non-commit target errors; absent storage remains caller case | Source is equivalent to former reader algorithm. New shared regression covers custom init default, tag-only repo, valid/missing branches, dangling and blob refs. Full Home consumer runtime not independently executed. |
| `g5.architecture.git_ref_updates`; `g5.architecture.git_owner`; `g5.architecture.commands`; `g5.data.git-authority` | `WorkspaceService::initialize_local_workspace_repo` → `open_or_init_with_default_branch` → `ops::prepare_initial_files_commit`; `routes/orgs::bootstrap_base_code_repo` → checked initial landing | Invalid branch must have no storage effect; valid preparation sets HEAD but no branch; existing worktree index/content/branch must survive; later checked publication remains necessary | Source preserves policy/effects. Added shared tests and API preparation test assert real ref/index/content state. API preparation/landing runtime belongs to main validation; checked publication code unchanged. |
| `g5.architecture.git_locations` | API storage paths supplied by `RepoStorage`; `GitRepo::open` and preparation adapter | Bare and worktree paths remain accepted; no reconstructed host/guest/dev mirror layout | Preserved by inspection; preparation test covers bare and worktree storage. Location-kind redesign is inherited remaining migration. |
| `g5.architecture.git_execution`; `g5.design.minimal-api`; `g5.design.abstraction` | `hash_blob`, `object_id_is_zero`, reused `is_valid_ref_name`, `BranchCommitState` | No backend handles in new APIs; Git blob framing, malformed/zero OID semantics; preserve workspace naming policy | New APIs are bounded Git value/state operations. Existing timestamp operation is reused. New value tests cover known hashes, uppercase nonzero ID and invalid inputs; source retains error categories. Helpers remain available only with libgit2 feature, without introducing a native-only consumer requirement. |
| `g5.architecture.git_retry`; `g5.architecture.git_outcomes` | All changed production paths | Added replay, storage, queue, worker or new implicit recovery; changed error categories | None added. Existing `GitError`/`Internal`/`ValidationError` categories and owner retry behavior remain. New preparation errors identify init/HEAD failure; full richer outcomes migration is inherited work. |
| `g5.architecture.git_guard`; `g5.quality.review.guards`; `g5.quality.review.cases`; `g5.design.enforcement` | `authoritative_workspace_reader_guard.rs` source walker, required operations and Cargo metadata check | Reject backend aliases, production/build dependencies, missing package/operations and unauthorized maintenance reads; allow fixture-only backend access and harmless subordinate helper renames | Compiled frozen exact source against cached dependencies and ran all 9 tests: PASS. Source scan traverses every API-core production Rust file and retains projection owner exception. Structural evidence only, not runtime authority proof. |
| `g5.architecture.projection_owner` | Guard scope prefix changes in API crate scan | Core maintenance remains allowed; same-named route or other application reader remains disallowed | Preserved by source and passing full guard suite. No production projection algorithm changed. |

## Verification receipts

- All frozen source SHA256 values: match.
- Pinned saved ontology diff compared byte-for-byte with current zero-context diff against `093e08a7`: match.
- Standalone exact-source guard compiled with existing `target/g5-dev/deps` tree-sitter, serde_json and walkdir artifacts; binary `/tmp/g5-api-core-independent-2-guard`. Invocation: `/tmp/g5-api-core-independent-2-guard --test-threads=1`. Result: 9 passed, 0 failed, 2.28 seconds.
- API/database suites are intentionally not duplicated; main agent owns those runs. This report does not claim their runtime results.

## Inherited migration gaps and unexecuted guarantees

- `GitRepo::lock_inner`/`from_raw` and workspace-ops backend access remain public/in use outside this increment. The current identifier/dependency guard does not establish complete raw-handle or whole-platform closure. The pinned API core scope explicitly leaves that migration for later.
- Host/API/guest native execution consolidation, provider HTTP consolidation, distinct typed location kinds and complete native-only consumers remain inherited work. No provider/lifecycle behavior was changed or verified here.
- Existing reader pin/prune races and incomplete broader reader coverage remain recorded in governing metadata; this narrow exact-OID migration does not repair or prove those outcomes.
- No claim of sibling-object confidentiality. Its separate storage/transport redesign remains parked by the user's instruction.
- Failure injection for init/HEAD filesystem errors, concurrent GC during exact-OID metadata reads and database-backed authority/bootstrap/consumer tests were not independently executed. Shared current implementation tests and main's consumer suites provide the appropriate follow-up evidence; static inspection is not substituted for them.

No production or ontology repair proposed from the initial discovery. Relevant metadata should be updated by the main agent after consolidation with actual current-checkout results, while preserving the wider migration limits.

## Additional independent runtime checks after initial save

The main agent supplied the current artifact directory (`target/debug/deps`) after the initial report was saved. Using the existing `g5_scm-08d6ca130345ed39` test binary, independently reran these three changed shared-operation regressions with `--exact ... --nocapture`:

- `repo::tests::branch_commit_state_distinguishes_unborn_missing_and_invalid_targets`: PASS.
- `repo::tests::local_repository_preparation_configures_head_without_publishing_or_checkout`: PASS.
- `safety::tests::object_values_preserve_git_framing_and_null_sentinel_errors`: PASS.

These provide runtime evidence for the shared operation rows above, including real ref/index/worktree inspection. They do not execute the API/database consumers or replace the separate authority/publication checks. No new finding resulted.
