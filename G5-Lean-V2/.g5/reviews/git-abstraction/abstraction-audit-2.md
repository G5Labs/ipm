# Independent abstraction audit 2

Frozen implementation: `b310368d3132f5989908cbe1f651536e3045aac1`; comparison base
`093e08a74d447ceeb5ad5b212890c67d24c7a5c4`. Read-only independent discovery, saved before
sharing findings or reading earlier review reports/author implementation commentary.
No production, test, or ontology edits; investigation artifacts are under
`/private/tmp/g5-abstraction-audit-2`. No Cargo build or database test was run.

Inputs: supplied user abstraction/concurrency questions; AGENTS.md; ontology-review
and design-review guide; pinned design governance, complete ontology diff, design and
acceptance sections; saved original request; current Git/governance, workspace,
durability, data-authority, dependency and design-principle commitments. The saved
ontology diff exactly matches `git diff -U0 BASE...HEAD -- .g5/ontology`. Current
ontology metadata supplied known limitations; earlier review artifacts were not read.

## Findings

### A2-1 — P1: fork source capture does not join the in-platform merge fence

**Source-demonstrated concurrency defect; no end-to-end runtime reproduction in this
review.** `workspace_fork_service.rs:743` and `:803-805` obtain a complete member
vector through `read_branch_head_oids`, but the admission transaction never acquires
the source workspace commit fence. `workspace_reader.rs:1240-1253` reads members one
at a time, without a cross-member lock. Fork takes canonical repository row locks only
after this vector is frozen (`workspace_fork_service.rs:878-881`), and does not reread
it under the merge fence before persisting `repo_manifest` (`:937-967`).

The actual platform writer takes `g5:workspace-commit:<workspace>` in
`branch_commit.rs:273-283`, then applies the member cut through
`GitRepo::apply_ref_commits` (`branch_commit.rs:1376-1404`). That SCM operation commits
its per-ref transactions sequentially (`g5-scm/src/repo.rs:2415-2431`). Git write locks
do not prevent readers observing old values. The public clone merge enters this
landing protocol through `workspace_service.rs:6922-7141` and the workspace merge
route; direct/PR merge also lands through the branch-commit owner.

Concrete interleaving for two member repos: fork reads A-old; the ordinary in-platform
merge publishes A-new/B-new; fork reads B-new. The child can persist and publish
A-old/B-new, which was never the source workspace's complete before/after cut. Even a
fork starting during the writer's commit prefix can observe A-new/B-old. No external
Git writer is necessary. The initial capture code predates this refactor (the fork
diff is only a helper migration), so this is a material dependency exposed by the
user's explicit consistency question, not a newly introduced regression.

Governing: requested merge/fork concurrency outcome, `g5.architecture.fork_owner`,
`g5.fork.source`, `g5.fork.replay`, Git-authority and the design acceptance requiring
fork/merge behavior to survive the execution migration. This does **not** require
cross-repository Git atomicity; cooperating platform capture can use the existing
workspace fence.

Repair: capture membership, receipt selection/validity and branch vector while holding
the source workspace's existing publication fence; keep source selection and receipt
pinning inside an operation that prevents those observations drifting apart. Preserve
the existing canonical creator lock order and avoid retaining the fence across slow
provider or live VM work. A captured old complete cut remains a valid result.

Regression: exercise the real public fork and real public merge owner on a code+docs
workspace, with a deterministic barrier during capture/publication. Assert the child
and frozen fork manifest are wholly old or wholly new (or a clear refusal), never a
mixed vector; verify Git refs, durable child membership and replay. Repeat for Branch
and Durable fallback/receipt selection. Existing tests in `api_workspaces.rs` cover
fork name races, lagging head projections, and merge-versus-transfer/binding races;
`branch_commit_landing.rs` covers real cut refusal and partial ref failure. None found
couples the public fork and merge operations in this way. Those tests cannot establish
the requested concurrent-owner guarantee.

### A2-2 — P2: adding a second native runner inside a new SCM file escapes the guard

**Runtime-calibrated structural defect.** The whole-backend native/backend source
walk explicitly skips `g5-scm` (`authoritative_workspace_reader_guard.rs:591-594`).
The remaining SCM native checks enumerate existing files in
`shared_repository_leases_use_the_native_runner` (`:1229-1248`) and
`native_observation_owners_remain_present_and_free_of_process_bypasses` (`:1511-1643`).
A new SCM module is not checked for process construction. Public-interface checks do
not help when the second process runner is private.

In an isolated shadow tree, I copied SCM sources, appended
`mod new_native_backend;` to its lib, and added the private function:

```rust
fn duplicate_git() {
    let _ = std::process::Command::new("git").arg("--version").status();
}
```

All four relevant guard tests still passed: whole-backend typed-owner walk, shared
repository leases, native observation owners, and SCM public privacy. The copied
guard was compiled directly with `rustc --test` against cached dependencies; this
did not change the checkout or build SCM. Evidence:
`/private/tmp/g5-abstraction-audit-2/shadow-guard-output.txt` and `shadow/backend/g5-scm`.

Consequence: an agent can add a second private SCM Git runner and miss timeout,
credential, environment and cancellation guarantees while CI remains green.
Governing: `g5.architecture.git_native_runner`, `g5.architecture.git_guard`,
`g5.design.enforcement`.

Repair/test: include every SCM production source file in the native process walk and
exempt only the canonical process-construction operation(s), with genuinely gated
test fixtures. Add the new-sibling fixture above plus an allowed helper invoking the
shared runner. Retain the existing public-operation presence checks separately.

### A2-3 — P2: reader guard misses ordinary Rust syntax and receiver names

**Runtime-calibrated structural defect.** The authorized-reader guard at
`authoritative_workspace_reader_guard.rs:1401-1431` uses `source.contains` over
`RAW_READER_CALLS` (`:10-22`), including the receiver-specific
`"repo.commit_file_diff("`. Ordinary Rust syntax can therefore introduce an alternate
application read path undetected:

```rust
GitRepo::show_file(repo, head, path);
selected.commit_file_diff(base, head, path);
repo.show_file (head, path);
```

I evaluated each using the exact current raw-reader matcher plus the tree-sitter
`typed_git_violations` check. All three escaped both; the expected spelling
`repo.show_file(head, path)` was caught. Evidence:
`/private/tmp/g5-abstraction-audit-2/guard-probe-output.txt`. This requires neither
macro expansion nor sophisticated dynamic dataflow, which the design correctly
excludes from its structural-proof claims.

Consequence: a new API consumer can read arbitrary SCM content outside the authorized
reader and lose the reader's version selection/pinning/admission contract.
Governing: `g5.architecture.git_execution`, `g5.architecture.git_guard`,
`g5.architecture.content_reader`, `g5.design.enforcement`.

Repair/test: move this check into the existing syntax-aware collector; detect both
method and associated-function paths independent of receiver spelling/whitespace,
resolve ordinary imported aliases, and name legitimate reader operations rather than
exempting every same-named `services/workspace_reader.rs` path. Add negative controls
for these exact forms and positive controls for harmless helper/receiver renames.

### A2-4 — P2: unchecked ref mutation remains a public alternative to checked effects

**Source-confirmed abstraction gap; no new production corruption claim.**
`GitRepo::update_branch_ref` (`g5-scm/src/repo.rs:2071-2087`) takes no expected state
and calls `repo.reference(..., true, ...)`, while checked `apply_branch_commit` and
`apply_ref_commits` are also public. This is live through
`g5d-cli/src/push.rs:152-163` and `workspace_manifest.rs:1216,1232`;
`g5-workspace-ops/src/merge.rs:126,145` exposes another workflow composition using it.
The public `push_force` at `repo.rs:3137-3152` is an unleased alternative to the leased
push and has no production caller found. Public `merge_branch_no_checkout` mutates
the target ref directly (`:2799-2816`), unlike exact-head unattached preparation plus
checked landing; its workspace-ops wrapper `mirror_task_commits` has no external live
caller found. These are inherited APIs retained within this branch's execution owner,
not native/libgit2 backend variants that inherently require different policies.

The migration successfully centralizes backend mechanics, but its public surface still
lets a future caller choose an unchecked update rather than expressing the expected
state promised by `g5.architecture.git_ref_updates`. The source guards check backend
type leakage and process ownership, not choosing these alternatives. Merely locating
both methods inside SCM does not consolidate the ref-effect contract.

Repair: inventory and retire or make private unused unleased/target-mutating helpers;
route live local alignment/replay through a named operation that declares its expected
state and materialization semantics. Preserve legitimate host/local checkout variants
without granting mapped-branch authority. Test a stale local-head/alignment operation
refuses instead of overwriting an intervening platform commit, and add compile-fail or
surface tests for retired unchecked entry points. If an unconditional local operation
must remain, explicitly limit its location/purpose and document that deliberate
exception in the design instead of exposing a general repository writer.

## Coverage and evidence without findings

The ledger distinguishes inspection from runtime evidence. It is not an exhaustive
correctness proof of every line in this 188-file branch.

| Commitment | Actual path / discriminating case | Result |
|---|---|---|
| `git_execution`, dependency direction | SCM private backend; optional `libgit2`/`native`/`providers` features; contracts no backend conversion; full backend manifest guard | Centralization present by source and passing guards. Unchecked operation-surface residual in A2-4. Native-only build not rerun here. |
| `git_ref_updates` | Ref transactions validate complete expected set before commit, report written prefix; resolution pair has both locks and checked create/replace | Inspected; existing stale/partial/identical-pair tests are substantial. A2-1 concerns concurrent capture, A2-4 alternate public writers. No claim of cross-repo Git atomicity. |
| `git_native_runner` | `native/process.rs`, context selection, child cleanup, typed operation modules; cancellation/large-stream tests | One current production runner found; no existing second process implementation found. A2-2 prevents confidently claiming future duplication is caught. Runtime VM/interruption matrix not rerun. |
| `git_outcomes` | `FailureEffect`, native error/effect parsing, provider transport and domain projection; lost status/body/partial tests | Clear refusal/partial/unknown structures and bounded redaction by inspection. No additional defect found; runtime provider failures not rerun. |
| `git_retry` | Private provider transport `.retry(never())`, no internal retry loop, native once-per-call; no migrations in branch | No new retry persistence or replay found. Existing domain recovery correctly remains a separate legitimate owner. |
| `git_credentials` | SCM auth callbacks/native credential setup; API repository credential owner; redirect/origin tests | Injection concentrated in SCM; policy selection stays above it. No additional defect found in inspected paths. External SSH/credential permission matrices unverified. |
| `git_locations` | API RepoStorage versus native host materialization versus CLI mirror/member paths | Distinct layouts are justified variants; forcing one root would erase meaning. No duplicate path owner established. |
| `scm_provider_client`, `scm_credential_scopes` | Typed `ProviderClients`, `RepositoryGrant`, App/account/connection/login/report clients and migrated API callers | Existing clients mediate provider HTTP while selection, persistence and login policy remain separate. Scopes are checked before typed operations. No additional bypass found. Live external provider behavior unverified. |
| `github_transport` | Private `providers/transport.rs`, bounded body/deadline, redirects disabled, explicit request credentials | One transport implementation found; owner-specific configuration preserved. Source guards pass; no claim of live SaaS/Enterprise proof. |
| `git_guard` | Current test suite, temporary violating fixtures, CI lane selection | 31 original guards pass; added audit calibration test passes by asserting the observed misses. A2-2/A2-3 are demonstrated misses. |
| Authorized reads/projections | `workspace_reader`, `projection_convergence`, typed immutable reads | Current selected paths preserve owners by inspection. A2-3 weakens evolutionary guard. Fork raw-head snapshot concurrency remains A2-1. |
| Workspace fork/merge owner behavior | Fork claim/proof/publication, real workspace merge, branch-commit finalizer, test inventory | Name/authority/stale-cut coverage exists. Requested simultaneous real-owner merge/fork consistency is neither tested nor guaranteed by capture source. |
| Minimal API / compatible variants | Native/libgit2 engines, login versus repo versus report grants, provider composition, storage layouts | These separate implementations/owners are legitimate. Residual unchecked APIs in A2-4 are different: same ref effect without expected-state contract. |

## Checks and CI reachability

- Compiled an unchanged copy of the current guard source plus one audit-only fixture
  with `rustc --test`, using already built dependency rlibs and the current repository
  as `CARGO_MANIFEST_DIR`. All **31 original guard tests + 1 calibration test passed**
  in 19.39 seconds. Receipt: `/private/tmp/g5-abstraction-audit-2/all-guards-output.txt`.
- Isolated shadow SCM module bypass: **4/4 relevant guard tests passed despite the
  second process constructor**, as detailed above. No source checkout was modified.
- CI does invoke these tests. `infra/ci/cloudbuild-ci-rust.yaml:1148-1166` selects all
  g5-api test binaries both for archive and normal nextest lanes. The orchestrator's
  `is_rust_integration` (`scripts/run-rust-xfs-orchestrator.sh:152-153`) selects every
  g5-api integration binary, and `:278-290` executes that list. A green CI run would
  therefore run the guard, but cannot detect its calibrated blind spots.
- Source/test inventory inspected real ref-transaction, resolution, native
  cancellation/transport, provider credential/redirect/error, API fork and merge
  tests. Their assertion breadth is useful and materially exceeds happy-path tests.
  No DB, live provider, KVM, or new SCM behavioral suite was executed by this reviewer.
  Main owns current runtime suite execution and any repairs.

## Proposed disposition

The execution/provider consolidation is substantial and generally has one mechanical
owner. It is not yet accurate to say every operation has one safe path or that new
duplicates are reliably caught. Repair the two calibrated guard gaps, narrow the
unchecked ref-effect surface, and add real-owner merge/fork concurrency evidence after
fixing source capture. Retain deployment, live-provider and interruption limits as
unverified rather than letting passing guards stand in for runtime proof.
