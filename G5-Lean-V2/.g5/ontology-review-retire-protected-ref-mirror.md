# Review: retire the protected-ref mirror and track Base like any workspace

**Mode:** self-review, using three fresh read-only reviewer contexts that ran in parallel. Each reviewer received the request, the pinned [design](ontology-plan-retire-protected-ref-mirror.md), the skill and the full scope. None received implementation history or the other reviewers' findings. Their raw reports are preserved in [r1](ontology-review-retire-protected-ref-mirror.r1.md), [r2](ontology-review-retire-protected-ref-mirror.r2.md) and [r3](ontology-review-retire-protected-ref-mirror.r3.md). The main agent consolidated them and made one repair pass.

**Reviewed snapshot:** commits `7a131d3c`, `50535baf` and `9e5eb122` on `origin/main` `3d629bd6`, plus the uncommitted maintenance-count fix. All three reviewers confirmed that the recorded ontology diff matches the ontology changes.

## Consolidated findings and dispositions

| # | Finding (reviewers) | Governing rule | Disposition |
|---|---|---|---|
| 1 | A provider rewrite pauses Base permanently (r1, r2, r3). The existing approval publishes a VM-prepared candidate and keeps G5's prior history (`normalize_reviewed_publication_tip`). On Base it is refused, and lifting the refusal would republish removed provider history. The new `base-parity` text wrongly promised that the approval works on Base. | `g5.workspace.remote.base-parity` | **Amended and kept safe.** The base-parity text is corrected, and the new `Q · g5.workspace.remote.base-provider-rewrite` records the decision with a recommendation. Base stays paused and loses nothing. The popover hides the approval and the Resolve action on Base and shows a paused notice instead. The refusal message now says why. Needs a human decision. |
| 2 | "Base cannot gain local commits" is false for a pinned Base that received merges. Make Remote then binds it paused as diverged, which Base cannot resolve (r2, r3). | `g5.workspace.remote.base-parity` | **Fixed.** Make Remote refuses a Base that is ahead, diverged or rewritten (`BASE_REMOTE_BIND_REQUIRES_FAST_FORWARD`) and stores no policy. The ontology states the rule. Test: `make_remote_outcomes.rs::base_make_remote_requires_fast_forward_and_starts_with_auto_pull_on`. |
| 3 | `rustfmt --check` failed on 8 files, and Prettier regressed `WorkspaceStatusBand.test.tsx` (r1, r2, r3). | Coding quality | **Fixed.** `cargo fmt --all` is clean. The test file is back to its main-branch Prettier state; the other flagged files were already non-compliant on main. |
| 4 | Stale tests: the maintenance handle count (14 → 12, r2, r3), and `scheduler_skips_base_workspace_remote_tracking_rows`, which now fails (r1). | Base parity | **Fixed.** The count is 12, and the scheduler test is inverted into `scheduler_enqueues_base_like_any_workspace`. |
| 5 | Missing evidence: the Base default through Make Remote (`bind_policy`), the pinger including Base, and a misleading renamed permission test (r1, r2, r3). | `base-auto-pull-default`, `base-parity` | **Fixed:** added the Make Remote default test and `advisory_ping_observes_base_like_any_workspace`, and renamed the permission test to state what it proves. **Still unexercised:** Base fetches after an ownership transfer, a provider rewrite on Base at runtime, and docs projection after a Base import. |
| 6 | The UI offered Resolve with agent and the rewrite approval on Base, and its behind text told users to ask an agent in a running workspace (r1, r2). | `base-parity` | **Fixed** in `BranchTrackingPopover.tsx`. Test: `BranchTrackingInline.test.tsx` "offers no VM-only reconciliation on Base". |
| 7 | Stale text (r1, r2, r3) in five places: the default-branch bind message, the auto-pull route comment, the mutation-policy comment, the `docs/data-model-cleanup-plan.md` Base admission, coverage and credential lines, and footers that cite mirror freshness. | One home per fact | **Fixed.** |
| 8 | `CycleState.is_base` was dead (r3, r1). | Minimal implementation | **Removed.** |
| 9 | Design text errors (r1, r2, r3): "auto-pull off → fetched / behind" cannot happen, the QA reset was described as holding auto-pull off, and the open assumptions were unresolved. | Design accuracy | **Fixed** in the design. Inspection resolved both open assumptions. |
| 10 | Unbound Base repositories now report `pinned` rather than `tracking` (r2, r3). | `base-parity` | **Accepted.** The new value is truthful: an unbound Base no longer follows the provider. |
| 11 | During the Helm upgrade, old pods may fail on the columns and table dropped by 056/057 (r1, r2; CodeRabbit on the PR). Old pods read the columns in ordinary workspace queries, not just the scheduler. | Deployment | **Fixed after PR review.** 056 is now expand-only: it switches Base bindings on and clears the retired setting. 057 is withdrawn; the column and table drops move to a follow-up contract migration. |
| 12 | Base's default push mode is `auto`, so background cycles run even with auto-pull off (r1). | `base-parity` | **No change.** Base has no local commits once tracked, and a provider move backwards classifies as a watermark violation, never a push. |
| 13 | A provider-PR merge import into Base uses the importing user's credentials (r2). | Credentials | **Recorded, out of scope.** That path is unchanged by this work. |

## Commitment coverage after repair

- `g5.git.mirror` → `maintenance/mod.rs`, `g5-scm`, `repo_mirror_refs` readers → no remaining server-side provider writer outside remote integration → **preserved**. Evidence: inspection by all three reviewers, plus `workspace_repository_model.rs`.
- `g5.git.network.ref-scope` → `git_policy::protected_branches_for_repo` → unchanged → **preserved**.
- `g5.git.mirror.update` → readers of the default-branch ref → all go through Base membership → **preserved** (inspection).
- `g5.workspace.remote.base-parity` → scheduler, pinger, worker, `/sync`, set_auto_pull, Make Remote and popover → **preserved**, runtime-tested at each point except ownership transfer and rewrite.
- `g5.workspace.remote.base-auto-pull-default` → all four binding inserts and migration 056 → **preserved**, runtime-tested (Publish, Make Remote, populated upgrade).
- `g5.workspace.identity.base` and `g5.workspace.base-ui.*` → unchanged gates and presentation → **preserved** (inspection, existing tests).
- `g5.workspace.remote.base-provider-rewrite` → **open question.** Safe paused behavior is implemented.

## Verification

Each run below executed against the local dev Postgres and passed:

- **Backend unit tests:** `cargo test --lib` for every touched crate.
- **Backend integration tests:**
  - `remote_repo_tracking_integration`
  - `remote_tracking_background_loop` (Base scheduler)
  - `make_remote_outcomes`
  - `remote_advisory_pinger`
  - `maintenance_integration`
  - `workspace_repository_model`
  - `migration_056_base_remote_tracking_parity`
  - `remote_tracking_multi_workspace`
  - `make_remote_adopt`
  - `api_workspace_pull_requests`
- **Frontend:** Vitest on the changed components, `tsc`, and `cargo fmt --check`.

**Final full run** after repairs: `cargo test -p g5-api --tests --no-fail-fast`. 116 of 119 integration binaries passed. The other three failed because the local Postgres hit its connection limit (`53300 too many clients`) under default parallelism: `api_home`, `api_workspace_pull_requests` and `indexer_pipeline_integration`. Rerun with `--test-threads=2`, they pass. That rerun also exposed a second stale maintenance-count assertion, in `indexer_pipeline_integration` (14 → 12), which is now fixed.

Also passing:
- the full frontend suite (256 files, 4179 tests);
- workspace `cargo clippy --all-targets -D warnings`;
- `cargo fmt --check`;
- `just validation-preflight`, all checks: architecture, migrations, QA reset and the rest.

**Not exercised at runtime:**
- Base fetches after an ownership transfer;
- a provider rewrite on Base;
- the migration against the live QA database.

## Follow-up: accept provider history on Base

**Mode:** a single self-review in the implementing session; no independent reviewers. The change is one owner operation reusing the existing convergence landing.

- `g5.workspace.remote.base-provider-rewrite` → preview and accept routes (both POST under write scope), Base-only preflight, `BaseAcceptance` admission, convergence consumer, pin ref → **preserved**. Runtime coverage:
  - a provider rewrite on the real worker;
  - Make Remote adoption;
  - a stale review refused with nothing changed;
  - a non-Base workspace refused;
  - nothing to accept once Base matches the provider;
  - no provider push;
  - open runs closed;
  - the pin pointing at the replaced head.
- `g5.workspace.remote.base-parity` → the Make Remote refusal is removed. A non-fast-forward Base binds paused, and the popover replaces VM-only actions with acceptance → **preserved** (`BranchTrackingInline.test.tsx`).
- Concurrency: the reviewed tips are rechecked against a fresh provider tip at accept time. Landing CAS, the convergence pause and branch guards, and the admission ownership recheck all run under the workspace fence. A concurrent landing racing acceptance is not exercised.
- Verification:
  - affected `g5-api` integration binaries;
  - `g5-api-core` and `g5-api-routes` unit tests;
  - the full frontend suite (4180);
  - workspace clippy `-D warnings` and `cargo fmt`.

## Follow-up: Base push invariant, co-landed imports, acceptance lock order

**Mode:** a single self-review in the implementing session, against the follow-up section of the [design](ontology-plan-retire-protected-ref-mirror.md#follow-up-base-push-invariant-and-co-landed-imports). No independent reviewers ran.

- **`g5.workspace.remote.base-parity`** (pull-side only; Base never publishes). Every path is covered:
  - the classifier (unit test across all triggers, modes and relations);
  - the push executor and the approved-rebase publish backstops;
  - the queue refusal;
  - the `push_to_remote` refusal and the push-mode refusal;
  - migration 056;
  - the UI.

  Runtime tests:
  - a lagging watermark with a provider reset in auto mode, under background and manual triggers;
  - Make Remote on an ahead Base, then a manual Pull;
  - a working workspace that still publishes.

  **Preserved.**
- **`g5.governance.remote.pull`** (co-landing, merged-PR hold, prompt retry). Covered at runtime:
  - a single cut for both members;
  - paused, auth-failing and busy members left out;
  - a forced stale cut retried promptly (this test fails if prompt retry is disabled);
  - two concurrent workers;
  - an in-flight merge and a partly merged PR.

  **Preserved.** Recorded gap: a provider PR merged directly on GitHub is held only after G5 records it as merged.
- **Acceptance lock order.** The review found no acceptance-specific inversion. Two pre-existing inversions affecting every landing are fixed and pinned by source-order tests: the clone-merge row-before-fence, and inventory mappings-before-workspace. The foreign-key cycle through fork admission and source rebase remains, recorded in the plan.
- **Clone-merge test.** `api_workspaces.rs::test_merge_clone_workspace_advances_parent_branch_head` had asserted the old row-before-fence order: ownership stayed locked while the merge waited at the fence. It now asserts that a waiting merge holds no destination row lock. The ownership guarantee is now that the owner check after the fence refuses a former owner's landing, covered by `transfer_during_clone_merge_preparation_refuses_the_old_owners_landing`.
- **Verification.** All of the following were run and pass:
  - 27 affected `g5-api` integration binaries (364 tests, `--test-threads=2`);
  - `g5-api-core` unit tests;
  - workspace clippy with `-D warnings` and `cargo fmt`;
  - `just validation-preflight`;
  - the full frontend suite (4180 tests) and `tsc`.

  The full `g5-api` integration suite outside those binaries, and e2e, are left to CI.

## Follow-up: consistent fork capture

**Mode:** a single self-review in the implementing session, against the [design](ontology-plan-retire-protected-ref-mirror.md#follow-up-consistent-fork-capture-of-a-multi-repository-cut). One read-only agent classified the head readers.

- **`g5.git.landing.commit-point`.** Before refs move, the landing runs `SET CONSTRAINTS ALL IMMEDIATE`. It restores a partial Git cut under the fence, and never restores after COMMIT.
  - Covered at runtime:
    - a deferred check refuses with the ref untouched;
    - a landing session terminated after its refs moved keeps them;
    - a partial cross-repository Git write, forced by an obstructed reflog, is restored under the fence.
  - **CI finding, fixed.** `branch_commit_landing.rs` pinned the old keep-partial-refs behaviour. It now asserts the restore.
  - **Preserved.**
- **`g5.fork.source.cut`.** Covered at runtime:
  - fork admission is fenced: a fork before the landing freezes the old vector, a fork during it the new vector, and a busy fence refuses retryably with no fork row and an identical retry admits;
  - each changed reader refuses retryably while its fence is held.

  **Preserved.**
- **Finding: authorization queues behind a landing.** Workspace-scoped authorization queues behind a landing's workspace row lock. This is pre-existing, recorded in the plan, and keeps mid-landing forks from even reaching the fence.
- **Fixed: PR evidence check placement.** The full run caught two regressions from the first version of the PR evidence check:
  - a separate transaction's authorization queued behind a concurrent close;
  - a check placed before the PR row lock inverted with the automatic refresh.

  The check now takes try-lock fences as the last locks of the apply transaction. All 51 PR tests pass.
- **Findings recorded in the plan, accepted:**
  - the ABA limit of the after-the-fact check;
  - the double-failure partial cut.

## Follow-up: fork advisories and the explicit-Pull hold notice

**Mode:** a single self-review in the implementing session.

- **`g5.fork.source.advisory`.** Covered:
  - at runtime: the advisory route lists the held member with its PR, the behind member and the paused member;
  - the clone response carries them;
  - the dialog renders them and still submits.

  **Preserved.**
- **`g5.governance.remote.pull`.** Covered:
  - the tracking summary and the Pull response name the holding PR;
  - the explicit Pull imports the held member;
  - closing the PR lifts the hold;
  - the popover and the Pull notice name the PR.

  **Preserved.**
- **Verification.** All of the following were run and pass:
  - 14 affected `g5-api` integration binaries (291 tests);
  - `g5-api-core` unit tests;
  - clippy and fmt;
  - the full frontend suite (4184 tests) and `tsc`.
