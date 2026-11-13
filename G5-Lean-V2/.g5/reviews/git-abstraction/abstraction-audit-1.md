# Git abstraction audit — independent reviewer 1

Reviewed frozen `b310368d3132f5989908cbe1f651536e3045aac1` against
`093e08a74d447ceeb5ad5b212890c67d24c7a5c4`, 2026-10-02. Read-only discovery;
no production, tests or ontology changed. This report was saved before sharing
findings and before reading prior review reports or implementation commentary.
Inputs were the request, pinned design's governance/diff/design/acceptance sections,
current governing ontology and its scoped metadata, branch diff, implementation and
consumers. Metadata's historical evidence claims were not treated as current runtime proof.

## Answer to the requested audit

The execution boundary is substantially consolidated: production backend access is
in SCM, provider protocol is behind private transport and scoped grants, and native
operations share one actual runner today. Native/libgit2 backends, API/host/guest/CLI
contexts, login/report authority and storage layouts are legitimate variants, not
duplicate owners. However, there are still public unchecked ref-writing alternatives,
and the guards do not catch some ordinary ways of adding new paths. Existing tests
are strong for many local properties, but do not establish the requested composed
in-platform merge/fork concurrency guarantee.

## Initial findings

### A1 — P2: the public API still exports unchecked ref-writing alternatives

- **Evidence:** `backend/g5-scm/src/repo.rs:2071` exposes `update_branch_ref`
  with `repo.reference(..., true, ...)` and no expected OID; `:2565` exposes the
  fixture/bootstrap `write_baseline_ref` without a fixture gate; `:3137` exposes
  unconditional `push_force`; `:2587` retains `merge_branch_no_checkout`, which
  writes the target while preparing a merge. Checked alternatives exist at
  `:2174`/`:2238`/`:2256` and lease publication at `:3202` onward.
- **Material consumers and limits:** CLI local materialization still calls
  `update_branch_ref` (`backend/g5d-cli/src/push.rs:154`,
  `workspace_manifest.rs:1216,1232`); local checkout policy is a legitimate variant,
  so those calls are not evidence of an authoritative-server corruption today.
  `backend/g5-workspace-ops/src/merge.rs:126,145,173` preserves alternate pull and
  `mirror_task_commits` writers, exported at `lib.rs:40`. A repository-wide symbol
  search found no production consumer of the mirror operation, the old workspace
  merge wrapper or `push_force`; their test callers do not justify production APIs.
- **Consequence:** a future caller can bypass the expected-state transaction while
  staying completely within `g5-scm` and passing the new dependency/native/public-
  backend guards. This prevents claiming that alternatives have all been closed.
- **Check:** isolated `audit_unchecked_ref_api_passes_guard` passes for a new API
  service calling `repo.update_branch_ref(branch, oid, "overwrite")`; the guard
  reports no violation. This is a checker probe, not a runtime corruption test.
- **Rules:** `g5.architecture.git_ref_updates`, `git_execution`,
  `g5.design.minimal-api`, `g5.design.information-hiding`.
- **Proposed repair/test:** remove unused legacy writer/workflow exports; put pure
  fixture setters behind `test-helpers`; give required checkout changes a named
  context-specific operation and expected state. Add a calibrated guard or public
  API compile-fail check against the obsolete unchecked surface, retaining valid
  fixture/local-checkout cases. Do not merge legitimate local and authoritative
  publication policies just because both move refs.

### A2 — P2: new SCM native modules escape the one-runner guard

- **Evidence:** `backend/g5-api/tests/authoritative_workspace_reader_guard.rs:592`
  skips the entire SCM crate in the all-backend native scan. The extra SCM inventory
  at `:1230` checks only `repo.rs`, `native/mod.rs` and `native/maintenance.rs`.
  SCM public-surface checking (`:1150`) checks leaked types, not method bodies.
- **Counterexample:** add `native/new_operation.rs` with an owned public boolean
  result and `std::process::Command::new("git").arg("status")` in its body, and
  declare the module. None of the inventories invoke the native checker for it.
  The checker itself would reject it if called. The source has no public backend
  type or HTTP/provider-dispatch violation.
- **Executed evidence:** `audit_scm_native_coverage_gap` asserts all of those
  predicates against the actual guard functions and passes. Current production
  native process construction was inspected and is in `native/process.rs:151`;
  this is a safeguard gap, not an observed second current runner.
- **Rules:** `g5.architecture.git_native_runner`, `git_guard`,
  `g5.quality.review.guards`.
- **Proposed repair/test:** enumerate every SCM Rust source as well; permit
  process construction only in the declared runner operation(s) and gated fixtures.
  Calibrate against a new SCM sibling and a new helper in an existing SCM module,
  and preserve private helper renaming and owned typed-operation positives.

### A3 — P2: the authorized-content-reader guard misses ordinary associated calls

- **Evidence:** `backend/g5-api/tests/authoritative_workspace_reader_guard.rs:10`
  and `:1426` use literal `source.contains` matches such as `.show_file(`.
  `typed_git_violations` only checks the special maintenance read names, not this
  general content boundary.
- **Counterexample:** a new content service can call
  `g5_scm::GitRepo::show_file(repo, oid, path)` directly. This is normal Rust
  associated-function syntax with the same effect as the forbidden method call;
  it passes both guards. Whitespace between a method and `(` is another untested
  spelling of the same operation. This is independent of exotic macros/types.
- **Executed evidence:** `audit_authorized_reader_ufcs_bypass` passes against the
  actual `RAW_READER_CALLS` and `typed_git_violations` functions.
- **Consequence/rules:** future application reads can skip authorized source
  selection/pinning while CI stays green, contrary to `g5.architecture.content_reader`,
  `git_execution`, `git_guard` and `g5.quality.review.guards`.
- **Proposed repair/test:** use the existing Rust syntax parsing infrastructure to
  recognize owned content operations in both method and associated-call forms;
  calibrate multiline/whitespace/alias cases, new API modules, allowed reader
  implementation and non-content policy proof operations. Do not blanket-ban all
  `GitRepo` use by policy/commit-preparation owners.

### A4 — P1 acceptance gap: merge/fork concurrency lacks composed proof, and source permits a torn fork vector

This is a material pre-existing interaction, not a new regression introduced by this
branch's one-line fork/merge consumer changes. It directly limits the user's requested
in-platform consistency claim.

- **Real fork path:** `/clone` reaches `WorkspaceForkService::fork_workspace` and
  `admit_operation`. Source membership is read without locks at
  `backend/g5-api-core/src/services/workspace_fork_service.rs:640`; source refs are
  captured at `:743` or `:805`. `workspace_reader.rs:1226` reads each repository
  sequentially (`:1246`) through independent `GitRepo` handles. Only afterward does
  fork roll back its capture transaction and acquire canonical repository rows
  (`workspace_fork_service.rs:878–881`); it persists the already-captured manifest
  without recapturing/revalidating the source vector. Later target proof verifies
  the captured per-member refs, not a coherent source instant.
- **Real merge path:** workspace merge prepares exact child/parent snapshots at
  `workspace_service.rs:6946` and lands via the authority finalizer. Job/PR merge
  revalidates workspace vectors and obtains workspace/member/repository rows in
  `job_merge_service.rs:2011–2075` / `workspace_repositories.rs:22–47`.
  `branch_commit.rs:1389–1404` delegates the complete cut to
  `GitRepo::apply_ref_commits`, which locks/validates every writer ref and then
  commits them in order (`repo.rs:2409`). Ref locks exclude competing writers,
  not readers; the independent fork capture does not participate in the SQL fence.
- **Source-level counterexample:** with two source members initially `(A0,B0)`,
  fork reads `A0`; a wholly in-platform successful merge lands `(A1,B1)`; fork
  then reads `B1` and freezes `(A0,B1)`. Later repository admission locks cannot
  repair that mixed capture. Writer CAS and fork destination proof can both pass.
  No external Git writer or provider is needed. This interleaving is established
  from the source, but was not run through the real API in this audit.
- **Existing evidence:** `api_workspaces.rs:2783,2855` exercises concurrent forks;
  `api_workspace_vms.rs:465` tests competing live-fork retries. `branch_commit_landing.rs:833`
  tests plain-Git interference and `:942` tests partial cross-repository writes.
  These are useful discriminating cases, but none runs the merge owner and fork
  owner concurrently. The branch adds no such scenario.
- **Rules:** complete fork/source capture (`g5.architecture.fork_owner`,
  `g5.fork.proof`, `g5.fork.source`), Git-authoritative selected state
  (`g5.data.git-authority`), and honest scoped evidence (`g5.quality.review.cases`).
  The design explicitly does not promise cross-repository Git atomicity; a coherent
  in-platform reader therefore needs an owner-level fence, not a stronger claim
  about libgit2 transactions.
- **Proposed repair/test:** capture fork membership and refs under the same
  canonical in-platform fence used by landing, or implement an equivalent
  validated snapshot protocol. Add a deterministic two-member public `/merge`
  versus `/clone` test with a narrow test rendezvous during capture, assert the
  child vector is wholly pre-merge or wholly post-merge (or explicit retryable
  refusal), and assert final Git refs, fork manifest/proofs and database membership.
  Cover both orderings, unchanged receipt versus newly advanced heads, conflict/
  refusal, and one unrelated workspace proceeding concurrently. A fixture
  directly mutating refs is insufficient to claim the requested owner composition.

## Checks and CI

Built an isolated guard harness with `rustc --test` using existing dependency
artifacts; no Cargo compilation, shared DB or source mutation. Current 31 repository
guard tests passed, plus the first two audit probes: **33/33**, 17.26 seconds.
After adding the content-read probe, the three audit probes passed **3/3**.
Evidence: `/private/tmp/g5-abstraction-audit-1/{guard_probe.rs,compile.log,all-guards.log,probe.log}`.

The guard suite is actually in CI: `infra/ci/cloudbuild-ci-rust.yaml:1154–1165`
includes all `g5-api` integration-test binaries via nextest (archive and normal
paths), propagating test failure through `status`. Local `scripts/test-parallel.sh:165`
also runs `cargo test -p g5-api --test '*'`. Passing CI would catch the guard's
covered bypass forms, but not the above gaps. Privacy doctests are present, while
the Cloud Build doc-test command at `:1275` uses `|| true`; rely on the hard-gating
source tests for current public-backend checks, not a claim that doctests are a gate.

## Commitment coverage and no-finding results

| Commitment(s) | Actual path / discriminator inspected | Assessment |
|---|---|---|
| `git_execution`, dependencies, compatibility | SCM exports, all backend source/dependency guards, optional `libgit2`/`native`/`providers` features, converted callers | Current direct backend ownership consolidated; no second production backend owner found. Public unsafe alternatives A1 remain. Native-only Cargo builds not rerun. |
| `git_native_runner`, runtime context/drain | `native/mod.rs` contexts, private `process.rs`, protocol/member/host/CLI adapters, cancellation/deadline tests | One current native runner; legitimate contexts retain owners. Guard gap A2. No runtime child/VM tests rerun. |
| `git_ref_updates`, network CAS/ref-scope | `apply_ref_commits`, resolution pair transaction, fork create/proof operations, receive-pack tests | Complete writer cut checks expectations before mutations and reports written prefix on I/O failure; no cross-repository atomicity claimed. A1 and A4 limit universal claims. |
| `content_reader`, projection owner | `workspace_reader`, projection typed refs, guard inventory, exact-version tests | Selected OID path retained; future bypass guard A3 and composed snapshot gap A4. No new current app-content bypass identified. |
| `git_outcomes`, `git_retry` | `FailureEffect`, provider transport/errors, native push/timeout tests, branch migration inventory | Explicit refusal/partial/unknown evidence and one HTTP attempt; no new migration/retry persistence. Inspected tests cover lost response, accepted-ref-plus-error, secret caps, provider retry hints. Not rerun. |
| `git_credentials`, credential scopes | private callbacks/native injection; repository/candidate/account/App/login/report grants and `require` checks | Distinct grants and selected origins retained; no raw credential-selection bypass found. Wrong-capability-before-HTTP and redirect/secret tests are substantive. External auth/revocation not executed. |
| `scm_provider_client`, `github_transport` | private `GithubHttp`, bounded body/deadline, no redirects/retry, typed PR/check/account/login/report consumers | Protocol sharing consolidated; no current duplicate raw provider client found; HTTP guard calibration passes. Distinct login/report owners are required variants. |
| `git_locations`, workspace registry | RepoStorage consumers, host/member contexts, backend-free path routing, bootstrap preparation | Layout meanings remain separate and object preparation is unattached. No finding in inspected callers; complete deployed-layout coverage unexecuted. |
| flush/receipt/checkpoint/restore | typed native member/flush/restore APIs, real Git tests named in source; receipt API verification tests | Tests distinguish captured/local/mapped state, expected versions, missing proofs and receipt replay. No new source defect identified in this sample; full interruption/VM matrix not run. |
| `pr_flow`, PR checks/authorship/follow-push | migrated lifecycle/read/check services and API push/PR regressions | Policy owner remains above typed provider clients; no duplicate create/update orchestration found. Real external provider and broader refresh races unexecuted. |
| `git_guard`, enforcement and calibration | 31 actual tests, three isolated probes, CI commands | Present and hard-gated in normal integration lane, but fails to cover A1–A3. Passing source guards do not prove runtime consistency. |

This is full-scope independent discovery by one reviewer, with source sampling
across all owners and deeper traces for the user's abstraction/concurrency questions;
it is not a claim to have executed every branch test or inspected every changed line.
No prior findings were supplied as hints. Runtime owner tests and any repairs remain
with the consolidating agent. No-finding entries above mean no issue found in the
inspected path, not an unqualified correctness certificate.
