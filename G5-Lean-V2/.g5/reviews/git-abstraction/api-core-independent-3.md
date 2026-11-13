# API core backend closure — independent review 3

Review date: 2026-10-02. Comparison base: `d8f6e729`. Discovery was read-only for production and ontology. No peer report or author review explanation was read, and no work was delegated. The supplied scope notes identified the no-new-retry-state constraint, parked sibling-object confidentiality extension, and main-agent ownership of API/database test runs; they supplied no defect diagnosis.

## Initial findings

No actionable defect found in this increment. The new operations are narrow enough for the migrated callers, preserve the previous backend behavior, and leave branch-selection, bootstrap admission, naming policy, artifact storage and observation expiry with their existing owners.

This is an incremental conclusion. It does not establish completion of the wider Git abstraction. Public `GitRepo::from_raw`/`lock_inner`, workspace-ops backend access, other native execution, provider transport closure and the parked shared-object confidentiality issue remain inherited migration work or accepted limitations. They are not newly introduced defects in this patch.

## Inputs and scope

Read `AGENTS.md`, the ontology-review skill, its design-review guide, the original saved request, and the pinned plan starting with `Ontology changes and governance`, including the complete saved ontology diff, design, previous increment boundaries, API core backend closure, acceptance outcomes and remaining assumptions. Read current relevant blocks and their metadata in Git/governance, workspace, data-model, boundaries, system-design principles and coding principles.

The saved ontology diff's added/deleted lines exactly match the actual ontology diff against `093e08a7`. There are no new ontology-body changes in this increment. All plan, production, manifest and test files listed in the frozen snapshot hash manifest matched the current checkout at review time. The author's `.g5/ontology-review-git-abstraction.md` was deliberately neither read nor hashed.

Reviewed all 13 changed backend paths in `/tmp/g5-api-core-review-snapshot/increment.patch` against current source:

- `g5-api-core/Cargo.toml`; services `remote_integrations/observations.rs`, `workspace_fork_service.rs`, `workspace_reader.rs`, `workspace_repositories.rs`, `workspace_review_service.rs`, `workspace_service.rs`.
- `g5-api/tests/api_orgs.rs`, `authoritative_workspace_reader_guard.rs`, `workspace_repository_model.rs`.
- `g5-scm/src/lib.rs`, `repo.rs`, `safety.rs`.

Followed affected consumers through `approval_config_service::capture_source_versions`, `branch_sync_runs_service::load_branch_tracking_summary`, remote-integration `worker::load_cycle_state`, workspace open-question/bootstrap admission, `routes/orgs::bootstrap_base_code_repo`, `branch_commit::prepare_initial_branch_commit_in_tx`, and `g5-workspace-ops::prepare_initial_files_commit`/`commit_parentless_index`. The latter still constructs an isolated index and an unattached parentless commit before the checked owner publishes the branch.

## Commitment → actual path → check → result or gap

| Commitment | Actual path | Counterexample/check | Result or gap |
|---|---|---|---|
| `g5.architecture.git_execution`, `g5.architecture.dependencies` | API-core manifest and every production API-core source file; exported `g5-scm` value/branch/preparation operations | Production direct backend dependency, aliased backend import, new direct backend use in an otherwise unlisted service; test-only backend fixture remains allowed | Current-source guard and offline Cargo metadata pass. Source scan leaves only fixture/comment mentions. New public values contain no backend handles. Existing wider raw-handle and workspace-ops access remains migration work. |
| `g5.architecture.content_reader`, `g5.data.git-authority` | `CommittedRepoReadView::committed_at`; `workspace_repositories::read_repository_versions`; existing `resolve_branch_head` and `commit_committed_at` | Branch advances after capture; author time differs from committer time; invalid/noncommit OID | Source inspection shows timestamp lookup uses the captured immutable OID and `commit.time().seconds()`, with no checkout or database-head fallback. New consumer regression discriminates author/committer times, changes the branch after capture, and rejects missing/blob targets. Its database execution is assigned to main and was not independently run here. |
| `g5.architecture.content_reader`, `g5.architecture.workspace_registry`, `g5.data.git-authority` | `expected_unborn_workspace_authority_cuts` → `GitRepo::branch_commit_state` | Unattached candidate with custom default HEAD; another existing ref; missing selected branch in populated repo; dangling/blob target; missing storage | Focused shared test passes: wholly unborn means unborn HEAD plus no refs; noncommit/dangling targets error; populated/missing branch is unavailable. Caller retains missing-storage handling, base/member/org checks and conservative unavailable result. API authorization and multi-member consumer behavior were inspected, not independently executed. |
| `g5.architecture.git_ref_updates`, `g5.architecture.git_locations`, `g5.architecture.commands`, `g5.architecture.workspace_registry` | `initialize_local_workspace_repo` → `open_or_init_with_default_branch`; initial commit preparation → checked `branch_commit` landing | Invalid branch before filesystem changes; fresh bare storage; repeat preparation; existing dirty/staged worktree; accidental publication during preparation | Focused shared test passes for invalid path noncreation, symbolic HEAD setup, no branch publication, repeat preparation, preserved index and worktree bytes. Source inspection confirms owner validation precedes `RepoStorage::init_repo`, paths come from storage, initial commit uses `commit(None, …)`, and org bootstrap still enters the checked publication flow. New API test verifies the actual service's unattached README candidate; main owns its runtime execution. |
| `g5.architecture.git_execution`, `g5.design.minimal-api`, `g5.design.abstraction` | `object_id_is_zero`, `hash_blob`, reused `is_valid_ref_name`; observations, fork naming, review artifact callers | Invalid/zero OID, uppercase supported hex, empty/blob framing, reserved and oversize workspace names, artifact path/size policy | Focused shared helper test passes known Git blob identities, invalid OID errors, zero sentinel and uppercase nonzero ID. Callers preserve prior error categories and retain expiry, URL/branch binding, naming reservation and artifact limits/storage. No repository access is added to pure value operations. Cache/storage end-to-end effects were inspected; not independently executed. |
| `g5.architecture.git_guard`, `g5.quality.review.guards`, `g5.quality.review.cases`, `g5.design.enforcement` | `authoritative_workspace_reader_guard.rs` | Alias and target/build dependency, missing expected package/operation, test fixture, harmless private helper rename, content-reader and maintenance-reader bypass | Exact current-source test compiled independently into `/tmp`; all nine tests pass, including current workspace scan, metadata, negative fixtures and legitimate refactor fixtures. Existing reader guards are retained. This remains a structural ratchet, not proof against arbitrary dynamic or existing public raw-handle bypasses. |
| `g5.architecture.git_retry`, `g5.architecture.git_outcomes`, `g5.data.outcome-evidence` | Full increment; existing observation cache and initial candidate/publication split | New persistence/replay, preparation reported as authoritative publication, changed error category | No schema/state/worker/retry changes. Source preserves existing observation cache behavior and explicit preparation/publish separation. Read/hash errors remain errors; initial preparation success claims only a candidate. Broader native/provider effect classification is outside this increment and remains unverified here. |
| `g5.architecture.projection_owner` | Existing projection ref reader and expanded guard scope prefix | New prefix accidentally rejects legitimate projection reads or permits same-named nonowner route | Current-source full API scan passes and exemptions retain the actual API-core maintenance owner. Existing owner-specific fixtures remain. Projection runtime convergence was not rerun because this increment does not alter its production operation. |

## Checks actually executed

Investigative mutable resources stayed under `/tmp/g5-api-core-independent-3`; no production or ontology edits were made.

Reused `target/debug/deps/g5_scm-08d6ca130345ed39`, first confirming that it contains the new test names. Three exact focused tests passed:

- `repo::tests::branch_commit_state_distinguishes_unborn_missing_and_invalid_targets`
- `repo::tests::local_repository_preparation_configures_head_without_publishing_or_checkout`
- `safety::tests::object_values_preserve_git_framing_and_null_sentinel_errors`

An initial shortened exact test filter selected zero tests and was immediately corrected to the fully qualified name; it is not counted as coverage.

Compiled the current `authoritative_workspace_reader_guard.rs` directly with `rustc --test`, current `CARGO_MANIFEST_DIR`, and existing dependency artifacts into `/tmp/g5-api-core-independent-3/guard`. All **9/9** tests passed, including `cargo metadata --offline --locked --format-version=1 --no-deps`. This avoids treating older prebuilt guard binaries as current-source evidence.

The API/database suites are deliberately left to the main agent's coordinated run. Their results must be added during consolidation before claiming those consumer guarantees as executed. No native/provider/network behavior or sibling-object isolation guarantee was independently executed in this review.

## Proposed disposition

No repair proposed from this discovery. After consolidating the API/database receipts, update scoped Git-execution/guard/location metadata to record this API-core increment, while retaining the remaining whole-platform migration and public-interface closure gaps. Do not broaden this increment's successful checks into a full Git abstraction completion claim.
