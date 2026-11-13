# API core backend closure — independent review 1

## Review identity and inputs

Independent, full-scope discovery review of the increment based on `d8f6e729`.
Production and ontology were left unchanged. No delegation, peer reports, author's
current review explanation, or evaluator feedback were used. The supplied constraints
were the approved implementation scope, no new retry storage/replay, and the parked
sibling-object confidentiality redesign; these were scope instructions, not defect hints.

Read `AGENTS.md`, the ontology-review skill and its design-review guide, the original
saved request, the pinned plan beginning at **Ontology changes and governance**,
the complete embedded ontology diff, the API core closure section, and current relevant
blocks plus footer entries. Compared the embedded ontology diff with
`git diff 093e08a7 --unified=0` for its two ontology files: exact equality. All fifteen
snapshot SHA-256 entries matched the checkout before runtime review. The current author
review file was hashed for snapshot identity but its content was not read.

## Initial findings, saved before sharing

**No actionable defect found in this increment.** The migration preserves the inspected
operations' value/error semantics, exact commit selection, bootstrap distinction, and
separation of preparation from branch publication. This is a scoped result, not a claim
that the complete Git abstraction or every existing owner satisfies the final design.

No governing meaning amendment is proposed. The main agent should update implementation
metadata for `g5.architecture.git_execution` and `g5.architecture.git_guard` after
consolidation: their present footer wording still describes remaining API backend use
and the earlier seven guard tests. Keep workspace-ops/raw-handle, native/API lifecycle,
provider-client and confidentiality gaps explicit. That final evidence reconciliation is
not a production defect in the frozen discovery snapshot.

## Commitment → actual path → check → result or gap

| Commitment | Actual path and discriminating case | Result / evidence limit |
|---|---|---|
| `g5.architecture.git_execution` | All six changed API service owners call `g5-scm` instead of `git2`; `g5-api-core/Cargo.toml` moves only the direct dependency to dev dependencies. Guard walks production core syntax and Cargo metadata. | Preserved by source inspection and 9 exact-source guard tests. Metadata guard rejects renamed and target-specific normal/build backend dependencies, absent package inventory, and new production backend imports; fixtures remain allowed. Indirect libgit2 linkage through `g5-scm` is expected in this API increment. |
| `g5.architecture.content_reader`; `g5.data.git-authority` | `CommittedRepoReadView::committed_at` passes its captured OID to existing `GitRepo::commit_committed_at`, whose implementation calls `find_commit` and uses committer time. `DocService::capture_docs_list` consumes that view. | Inspected: no re-resolution of branch/HEAD or checkout fallback. Added API regression moves the branch after capture, separates author/committer times and time zones, and rejects absent/blob OIDs. API/database execution is assigned to the main agent and was not independently executed here. |
| `g5.architecture.workspace_registry`; `g5.data.git-authority` | `workspace_repositories::read_repository_versions` gets storage from `RepoStorage`, resolves each exact member branch once, then reads the resulting immutable OID's timestamp. Followed consumers in approval configuration, branch tracking summary and remote-integration worker. | Inspected equivalence: reused `resolve_branch_head` peels the named `refs/heads` ref to a commit just as the removed code did. No author-time substitution or second mutable-ref read. Multi-repository atomicity is not newly claimed; consumers retain fencing responsibility. Added API regression covers captured version and timestamp; execution belongs to main. |
| `g5.architecture.content_reader`; API core closure bootstrap contract | `expected_unborn_workspace_authority_cuts` retains org/Base admission and missing-storage handling; `GitRepo::branch_commit_state` distinguishes selected committed ref, unborn HEAD with no refs, unavailable selected branch, and found invalid/dangling targets. Followed its open-question metrics and Home Project Docs consumers. | Runtime shared-operation test passed: custom default independent of `init.defaultBranch`, unattached objects, unrelated tag, present/missing branch, dangling symbolic ref and blob target. Source comparison confirms unchanged Base/working-workspace admission and no error-to-empty conversion. Actual HTTP/Home execution not independently run. |
| `g5.architecture.git_ref_updates`; `g5.architecture.git_owner`; `g5.data.git-authority` | `WorkspaceService::initialize_local_workspace_repo` validates before `RepoStorage::init_repo`, calls typed preparation, then `prepare_initial_files_commit`. Inspected that operation's private in-memory index and `commit(None, ...)`; followed `routes/orgs.rs::bootstrap_base_code_repo` to `prepare_initial_branch_commit_in_tx` and `land`. | Preparation still writes objects and symbolic HEAD only, without publishing the branch. Runtime shared-operation test passed for invalid name before filesystem creation, bare initialization/reopen, existing branch preservation and unchanged worktree/index bytes. Added API preparation test asserts unattached parentless README candidate and no storage for invalid input; full checked publication is source-inspected here and awaiting main's integration evidence. Existing CAS/ref-scope authority was not moved into preparation. |
| `g5.architecture.git_locations` | Both reader paths and initialization obtain paths from `RepoStorage`; `open_or_init_with_default_branch` privately performs bare/worktree opening and bare initialization. | Source inspection and bare/worktree preparation test passed. No API layout reconstruction, dev-mirror conflation or new remote URL logic. Other location-kind migration remains outside this increment. |
| `g5.architecture.git_execution`; `g5.workspace.identity.branch`; `g5.workspace.names`; `g5.fork.names` | `check_workspace_name` retains length, `@`, reserved-name checks and exact spelling; only the same `refs/heads/<name>` Git syntax check moves behind existing `is_valid_ref_name`. | Source equivalence plus both valid/invalid shared ref-name tests passed. Fork operation/namespace reservation remains with its existing owner. This review does not re-prove its provider races or all namespace collisions. |
| `g5.architecture.git_execution`; `g5.architecture.review_owner` | `store_review_artifact` and non-storing `generate_review_changes` use `hash_blob`; size limits, SHA-256, storage owner/location and no-store behavior stay in API core. | Runtime Git blob framing assertions passed for empty bytes and `test content\n`; source confirms both call sites still map failures to `G5Error::Internal` and do not store blobs in the Git object DB. Full upload/download/generated-artifact workflows not independently run. |
| `g5.architecture.git_execution`; `g5.architecture.git_outcomes` | `RemoteObservations::{decode,observe_tip}` delegates only parsed-OID zero detection to `object_id_is_zero`. URL/branch/age matching, zero refusal, cache outage policy and 500ms I/O bound remain unchanged. | Shared value test passed for zero, nonzero uppercase OID and malformed input preserving `G5Error::GitError`. Source inspection confirms parser and null semantics are exactly those removed from callers; no new full-length normalization or authority interpretation. Redis/consumer suites belong to main. |
| `g5.architecture.git_guard`; `g5.design.enforcement`; `g5.quality.review.guards`; `g5.quality.review.cases` | Expanded migrated-operation inventory, crate-qualified source scopes, core-wide backend identifier check, precise projection reader owner and Cargo direct dependency inventory. | All 9 current guard tests passed from freshly compiled exact source. Negative fixtures cover backend aliases, missing operations/packages and maintenance-reader bypass; harmless comments/names, helper renames and test fixtures pass. Existing content-reader/projection guards still pass. These are structural checks, not whole-platform or dynamic-bypass proof. |
| `g5.design.minimal-api`; `g5.design.abstraction`; `g5.architecture.commands`; `g5.architecture.dependencies` | New surface is two complete repository operations plus two value helpers and a backend-independent state enum; timestamp logic reuses the existing method. No database/runtime dependency is added to `g5-scm`. | Source inspection: public inputs/results added by this increment expose no backend handle, command/environment map, or callback policy. Existing public raw handles and workspace-ops backend access remain migration gaps; they were not introduced or asserted closed here. |
| `g5.architecture.git_retry`; `g5.architecture.recovery` | Entire frozen production diff and consumers. | No new schema, retry tracking, worker, queue, loop, automatic replay or sleep/retry path. Existing publication, cache and lifecycle owners remain. The increment retains existing `G5Error` categories and does not claim the wider native/provider outcome migration is complete. |
| `g5.architecture.git_native_runner`, `git_credentials`, provider client/scopes/transport commitments | No changed native invocation, credential selection/injection, provider transport, or lifecycle path. | Inspected as unaffected boundaries. Existing guard tests remain passing, but this review does not revalidate native process cancellation, live provider grants or host/guest dependency closure. Sibling-object confidentiality is explicitly parked and not established by this work. |

## Changed-path coverage

Reviewed all thirteen changed production/test/manifest/guard paths in the frozen patch:

- `backend/g5-api-core/Cargo.toml`.
- `backend/g5-api-core/src/services/remote_integrations/observations.rs`.
- `backend/g5-api-core/src/services/workspace_fork_service.rs`.
- `backend/g5-api-core/src/services/workspace_reader.rs`.
- `backend/g5-api-core/src/services/workspace_repositories.rs`.
- `backend/g5-api-core/src/services/workspace_review_service.rs`.
- `backend/g5-api-core/src/services/workspace_service.rs`.
- `backend/g5-api/tests/api_orgs.rs`.
- `backend/g5-api/tests/authoritative_workspace_reader_guard.rs`.
- `backend/g5-api/tests/workspace_repository_model.rs`.
- `backend/g5-scm/src/lib.rs`.
- `backend/g5-scm/src/repo.rs`.
- `backend/g5-scm/src/safety.rs`.

The plan was reviewed as governing context. The author review artifact was deliberately
excluded from initial discovery as required.

## Executed checks and limits

- `shasum -a 256 -c /tmp/g5-api-core-review-snapshot/sha256`: all fifteen entries matched.
- Embedded ontology diff equals the actual two-file ontology diff against `093e08a7`.
- Reused `target/debug/deps/g5_scm-08d6ca130345ed39`, confirming its test inventory
  contains the newly added tests. Five distinct relevant tests passed: branch state,
  local repository preparation, blob/OID values, valid refs and invalid refs. An initial
  short-name invocation with `--exact` selected zero tests; the corrected full-name
  branch-state invocation selected and passed one, and is the evidence counted here.
- Compiled the exact current guard source with `rustc --test`, using existing built
  dependencies and its real `CARGO_MANIFEST_DIR`; all nine tests passed, including
  current Cargo metadata. Isolated binary/log:
  `/tmp/g5-api-core-reviewer-1/guard` and `/tmp/g5-api-core-reviewer-1/guard.log`.
- API/database consumer suites were intentionally not duplicated because the main
  agent owns their execution. Their successful execution must be established during
  consolidation; source inspection and shared tests alone do not prove those public
  workflows. No external provider/guest tests were run or needed to claim this
  increment's unchanged transport scope.

Implementation review is complete for this assigned snapshot, with no required code
repair identified. Verification is scoped as above; broader migration and current
ontology footer gaps remain distinct from defects in this increment.
