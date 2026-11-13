# Independent abstraction audit 3

Initial discovery saved 2026-10-02 before sharing findings. Read-only review of frozen `b310368d` on `design/git-abstraction`, against `093e08a74d447ceeb5ad5b212890c67d24c7a5c4`.

## Inputs and limits

Read AGENTS.md, ontology-review and design-review instructions; the pinned plan's governance inventory and actual ontology diff, original request, design and acceptance outcomes, current Git/governance, durability, workspace/fork and authority commitments and scoped metadata. The supplied plan itself contains historical implementation/review summaries (including at its tail); those appeared in the initial whole-plan read. No prior review files or peers' findings were read before saving this report. Thus this is a fresh independent code investigation, with the plan's embedded historical summaries disclosed as an input.

Reviewed SCM public/backend/native/provider boundaries; production consumers and manifest ownership; guard traversal/calibration and CI inclusion; actual fork capture, checked-fork effects, merge admission/application and branch-commit publication; relevant primitive/API tests. No production, test or ontology edits; no Cargo builds or shared test-database use. Isolated Rust guard harness reused already-built dependency libraries. Detailed evidence files live temporarily at `/private/tmp/g5-abstraction-audit-3/`.

## Findings

### A3-1 — P1: Fork source capture does not participate in the merge fence and can freeze a mixed multi-repository cut

This is an inherited integration defect, not a newly introduced line regression. It directly defeats the requested assurance about simultaneous in-platform merge/fork, so it is material to this audit.

`workspace_fork_service.rs:568-588,640-664` loads the source and its members without row locks or the workspace commit fence. At `:743` / `:804` it calls `read_branch_head_oids`; that helper (`workspace_reader.rs:1240-1253`) opens and reads each repository independently. Only after constructing the complete immutable manifest does admission acquire repository rows (`workspace_fork_service.rs:878-881`) and persist that already-captured manifest (`:928-950`). These later locks never revalidate source heads.

Meanwhile, real local workspace merge takes `g5:workspace-commit` fences (`job_merge_service.rs:1639-1648`) and lands through `branch_commit.rs:273-303,343-348`. Its SCM ref cut locks/checks all affected refs before writes, but commits per-ref transactions in a loop (`g5-scm/src/repo.rs:2413-2432`). Git ref locks exclude writers, not readers. Fork does not take the fence and can read member A before merge, then member B after merge, persisting `[A_old,B_new]` even if the source's only accepted complete cuts were `[A_old,B_old]` and `[A_new,B_new]`. It can also freeze a transient Git state before a definite SQL-commit failure compensates it. `preflight_refs` validates the captured commits/destination names, not whole-source coherence (`workspace_fork_service.rs:1464-1517`); finalization validates child proofs (`:1782-1805`), so neither repairs the cut.

Governing: `g5.architecture.fork_owner` owns the complete source capture; `g5.fork.source` and `g5.data.git-authority` require the selected durable Git state; `g5.design.transaction-composition` requires coherent coupled effects. The plan's acceptance retains checked fork/merge behavior and complete pinned versions. A pre-merge or post-merge vector is fine; a hybrid vector that was never an accepted workspace state is not.

Repair: obtain a coherent source capture under the same workspace commit fence used by in-platform landings, locking/revalidating membership in a consistent lock order, and retain/pin the selected state before releasing capture custody. Do not serialize slow provider/fork materialization unnecessarily. Preserve the separate existing retention/pin concern rather than pretending this one fence proves pruning safety.

Regression: two-repository real API local merge plus real checked-fork requests, using a deterministic barrier during multi-member capture/landing; assert the final child Git refs AND persisted fork manifest equal one complete before/after vector, never mixed. Include source/destination fork variants, definite landing failure/compensation, and fork replay. A fixture merely moving refs or a stale-CAS unit does not prove the public-path concurrency claim. This report establishes the source-level counterexample; it does not claim executed merge/fork runtime proof.

### A3-2 — P2: A new native runner inside SCM is outside the guard traversal

`authoritative_workspace_reader_guard.rs:593-596` explicitly skips the entire `g5-scm` crate during all-backend native/backend traversal. SCM native ownership is then checked only in finite file lists at `:1230-1246` and `:1511-1634`. Adding `g5-scm/src/native/alternate.rs` (and its ordinary `mod alternate;`) with a private `std::process::Command::new("git")` is not scanned by either list. Public surface checks scan it, but private commands are correctly irrelevant to that check; provider HTTP checks also have no reason to reject it.

The isolated compiled harness confirmed for the new sibling source: public-surface violations `[]`, provider violations `[]`; calling the existing native checker directly DOES reject it. This is an enumeration omission, not an unavoidable Rust macro/dataflow limitation.

Governing: `g5.architecture.git_native_runner`, `g5.architecture.git_guard` and `g5.design.enforcement`. A future agent can accidentally establish a second command/environment/cleanup implementation while all relevant integration guards remain green.

Repair: traverse every SCM production source file for native execution, with narrow operation exceptions for the actual process runner, instead of treating the crate as a whole exception. Retain finite required-owner assertions separately. Regression: add a new private SCM sibling with aliased/qualified native construction and show rejection; renamed legitimate private helpers and actual runner operations must remain accepted.

### A3-3 — P2: The authorized-reader guard misses ordinary associated-function calls

`authoritative_workspace_reader_guard.rs:10-22,1401-1435` uses literal `.show_file(` / similar substrings. A consumer can write `g5_scm::GitRepo::show_file(repo, "main", "README.md")` and bypass the authorized workspace reader without any raw backend dependency. This is ordinary Rust syntax, not dynamic or macro evasion. The typed checker only separately owns `head_ref_commits` / `ref_commit_oid` (`:511-533`), so it does not cover this gap.

Compiled guard calibration: the existing `repo.show_file(...)` example is rejected by the raw-reader list; the semantically equivalent `g5_scm::GitRepo::show_file(repo,...)` yields raw-reader violations `[]` and typed violations `[]`. The reader loop also covers a fixed list of API crates only, unlike the new backend/provider traversals; a future application consumer in a new backend crate is another uncalibrated boundary.

Governing: `g5.architecture.git_execution` explicitly retains source selection/pinning/read authority above SCM, `g5.architecture.content_reader`, `g5.architecture.git_guard`; the plan explicitly requires typed-Git content-read bypass rejection.

Repair: bring content-reader operation ownership into the syntax-aware traversal, handling method and associated-function forms with alias resolution and scoped legitimate reader/maintenance/policy exceptions. Regression: UFCS, alias, whitespace/refactor variants, a new consumer file/crate, and valid authorized-reader internals. This is a regression-enforcement defect; no current production consumer using this UFCS bypass was found.

### A3-4 — P2: Public raw native command escape hatches are not covered by the public-surface guard

`authoritative_workspace_reader_guard.rs:906-920` recognizes backend types only when rooted at `git2`, `libgit2_sys` or `reqwest`. `std::process::Command` / `tokio::process::Command` are omitted. `native/process.rs` is the legitimate process owner and is not included in the native prohibition lists. A new `pub fn backend_command() -> std::process::Command` there passes the full public-SCM type check. The isolated compiled harness reports `[]` for this concrete escape hatch.

Governing: `g5.architecture.git_execution` requires command/environment assembly private, `g5.architecture.git_guard` explicitly rejects generic command escape hatches, and `g5.design.information-hiding`.

Repair: include native command/builder/process handles in the forbidden exported surface and add compile-fail or public-surface calibration controls for direct paths, aliases, wrappers/re-exports and callbacks. This does not imply the current public API already leaks such a command; current `NativeGit` and invocation state are private. Generic string-argument or macro-generated APIs remain a separate limit requiring review even after this concrete hole is fixed.

## Consolidation and test assessment

The implementation generally consolidates actual mechanics: production direct `git2` dependency belongs to SCM, raw repository/callback access is private, typed operations replace API/CLI/host/guest command construction, and provider clients use one private bounded transport. Thin compatibility/composition wrappers in workspace-ops are not independent backend implementations. Native/libgit2, API/host/guest context policy, login identity, repository grants and report grants are legitimate variants with different authority; forcing one policy API would erase required distinctions. No new duplicate production native/provider mechanism was identified in sampled current consumers.

Future duplicates are partially caught, not guaranteed caught. Dependency, public backend/HTTP, native caller, provider dispatch, required-owner and calibration guards are substantive. A3-2 through A3-4 are concrete untested shapes beneath those claims, beyond the already-disclosed arbitrary macro/type/dataflow limitation.

The new implementation has substantial behavioral test sources: native lease/push/refusal/unknown-outcome/credential/cancellation tests (`native/tests.rs`), streaming protocol tests, member checkpoint/flush/update/restore tests, provider protocol tests and actual API push/flush/provider-owner tests. SCM resolution tests cover collision, replacement, stale expectations and lock contention. Existing `branch_commit_landing.rs` covers multi-member stale cuts and compensation; `api_workspaces.rs:703-838` overlaps a real merge and ownership transfer. These are useful but do not run checked fork concurrently with a multi-repository merge. `api_workspace_fork_names.rs` executes serial name/replay cases; `api_workspace_manifest.rs` checks exact pins sequentially; provider-merge/fork coverage observed in `api_workspace_pull_requests.rs` is sequential. No public merge+fork overlapping regression was located.

CI inclusion is real by source: `infra/ci/cloudbuild-ci-rust.yaml:1154-1165` selects all g5-api integration test binaries both in archive mode and ordinary `--test '*'` mode and propagates status (`:1169`), which includes this guard suite. Local `scripts/test-parallel.sh:165` also selects every g5-api integration target. This report did not observe a fresh remote CI run. The separate documentation-test lane uses `|| true` (`cloudbuild-ci-rust.yaml:1275`), so compile-fail doctests alone are not an enforced merge gate; the integration source guards provide the current failing gate.

## Commitment ledger

| Commitment | Actual path and check | Result / boundary |
|---|---|---|
| `git_owner`, `git_execution` | SCM exports/private backends; API routes retain admission; workspace-ops wrappers delegate | Source-inspected consolidation; no duplicate active backend owner found. A3-3 means future typed reads can evade enforcement. |
| `git_ref_updates` | `resolution.rs`; `GitRepo::apply_ref_commits`; typed native receive/push and fork proof operations | Exact precondition/ref scope checks inspected; coupled writer validation and refusal tests exist. Fork read coherence is violated by A3-1; no runtime cross-operation proof here. |
| `git_native_runner` | `native/process.rs`, typed API/CLI/host/member operations | One current runner inspected; stream/process cancellation tests exist. A3-2/A3-4 weaken future enforcement. Full VM/non-Unix/remote cancellation remains unexecuted here. |
| `git_outcomes` | `FailureEffect`; native typed errors; provider transport/errors and owner mappings | Refused/partial/unknown distinctions present; test sources cover lost status, deadlines, failed responses and redaction. No new finding; not exhaustive runtime validation. |
| `git_retry` | Private transport `.retry(never())`; native single invocation; diff adds no migration/tracking storage | No finding. Existing domain recovery remains outside SCM rather than generic replay. |
| `git_credentials` | `auth.rs`, native invocation credential channel, selected-origin checks, private transport | Scoped injection and redirect refusal inspected; credential/runtime suites exist. No finding. Real external SSH/provider execution unverified. |
| `git_locations` | API `RepoStorage`, host cache/materialization, member roots, CLI mirror operations | Layout variants retained; typed SCM resolves backend paths. No finding; full deployed layout not executed. |
| `scm_provider_client` | `ProviderClients`, scoped account/App/repository clients; API consumers | Provider protocol centralization inspected. Login/report are legitimate separate authority clients using shared transport. No current raw-provider duplicate found. |
| `scm_credential_scopes` | `RepositoryGrant`/capability checks, App/account/login/connection grants; API resolver | Wrong capability fails before dispatch; initiating actor/selection remains above protocol. Test sources cover scopes. No finding; external permission matrix unverified. |
| `github_transport` | `providers/transport.rs`, configured owner endpoints, private request/response | Shared headers, no redirects/replay, bounded response/evidence; scoped tests exist. No finding. |
| `git_guard` | Tree-sitter native/backend/HTTP surface checks, source reader check, manifests and CI selection | Current suite is substantive but incomplete: A3-2/A3-3/A3-4. Isolated calibrated counterexamples executed. |
| `content_reader`, `projection_owner` | Workspace reader owns reads; projection typed maintenance calls | No current duplicate production content reader found; future-path enforcement A3-3. |
| `fork_owner`, `fork.source`, transaction composition | Real checked-fork capture, fork refs, merge/branch-commit fences | A3-1 source-level race; runtime concurrency claim unverified. Existing receipt/pruning gap is disclosed in ontology and not recounted as a newly detected regression. |
| Retained durability/checkpoint/flush/restore/GC | Native modules and API/host/guest typed callers; operation test sources | No new finding in sampled paths; exhaustive VM/partial-stage matrix not executed. |
| Minimal API / abstraction / information hiding | Public owned API scan and actual callers | Stable mechanics are centralized; no generic public invocation found today. A3-4 catches a clear missing evolution safeguard. Unused compatibility wrappers alone are not labeled policy duplication. |

## Executed checks

- Isolated Rust calibration harness using the exact copied guard functions, compiled with existing `tree_sitter`, `tree_sitter_rust`, `serde_json` and `walkdir` libraries: exit 0. Results recorded at `/private/tmp/g5-abstraction-audit-3/guard-probe-results.txt`.
- Source inventory and CI selection inspected directly; no production edits or shared test DB mutations.
- The unmodified 31-test guard suite was also launched from an isolated directly compiled copy, avoiding Cargo builds; result pending at this initial save and will be appended when available.

### Check receipt appended after initial save

The isolated unmodified guard suite completed: **31 passed, 0 failed**, 18.81 seconds. This used the exact repository test source, directly compiled with cached dependency libraries, `CARGO_MANIFEST_DIR` pointed at the reviewed API crate, and its normal `cargo metadata --offline --locked --no-deps` manifest check. Receipt: `/private/tmp/g5-abstraction-audit-3/original-guard-results.txt`. Passing the suite alongside the calibrated counterexamples confirms that these missing guard shapes are not caught by current test coverage. No implementation files changed during this check.
