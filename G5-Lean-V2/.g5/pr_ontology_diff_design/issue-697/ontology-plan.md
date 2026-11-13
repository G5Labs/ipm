# Approval versions and shared merge targets — issue #697

Request: [G5Labs/G5-Lean#697](https://github.com/G5Labs/G5-Lean/issues/697).
Analyzed after fetching `origin/main` on 2026-10-28:
`9e72c262bc1b3491f606d6e26750cd0c4ce9e03b`. The authorized workspace branch is
`issue-697-fix-now-approval-version-and-merge-target`, initially at that same commit.
The owner's chat requires analysis, an implementation plan and risk assessment before
explicit implementation approval. The owner approved this plan in chat on 2026-10-28;
implementation and qualification are recorded below on the authorized branch.

## Findings on current main

The issue's audited base is older than main. Inspection of the current code and intervening
changes confirms both root causes remain; merge when ready is now an additional consumer.

- `RecordDecisionRequestDTO` and service `DecisionRequest` accept an optional
  `reviewed_git_heads`. `record_human_decision` substitutes current heads when absent.
  The browser already submits `draft.heads`. A complete recorded older vector is a valid
  deliberate selection and must stay attached to that older revision.
- `merge_provider_pull_requests` locks one PR and builds all provider plans before invoking
  their merges. Different PRs can verify the same live target independently. Its merge
  request pins the source SHA, but cannot pin the target.
- `merge_one_provider_pr_after_preflight` writes the checked `target_head_sha` only after
  provider success, in the same transaction as acceptance. A lost response or rollback can
  therefore leave a publication-time target. `reconcile_provider_acceptance` recognizes a
  merged source but does not recover the lost merge-time target.
- `admit_pull_request_merge` already commits approval admission before provider effects;
  admission contains source evidence, not the checked provider target vector. The fresh
  target vector is carried in `ProviderFirstMergeRequest`, not durable attempt evidence.
- `remote_integrations::import_hold` detects a multi-repository merge in flight through
  the PR row lock or a recorded merged member. After rollback, neither identifies a still
  unresolved provider effect; this consumer must also recognize durable attempt evidence.
- Existing loss/partial-acceptance tests preserve admission after withdrawal, but simulate
  loss by updating stored statuses. They do not establish correctness when a real provider
  effect outlives its response or database transaction.

The provider protocol already supplies `ProviderError.effect` (`Refused`, `Partial`,
`Unknown`). The lifecycle's `ProviderHttpError` conversion discards it. Preserve this
classification for merge recovery rather than infer effects from HTTP status or error text.
GitHub's documented [merge endpoint](https://docs.github.com/en/rest/pulls/pulls#merge-a-pull-request)
offers a source `sha` condition and no expected target parameter. This design retains the
current rebase merge operation.

## Governing commitments and ontology diff

- Amend [reviewed versions](../../ontology/features/reviews.g5.md#g5.governance.approval.version)
  to require the explicit vector at the human decision boundary and retain explicit historical
  selections. Preserve the deferred general post-push invalidation decision; do not change
  existing quorum, eligibility or approval invalidation rules.
- Preserve [merge recovery](../../ontology/software_architecture/git.g5.md#g5.git.pr-recovery-cut)
  and add [shared-target serialization](../../ontology/software_architecture/git.g5.md#g5.git.provider-merge-target-serialization):
  reserve before dispatch, retain ambiguous effects, and fence stale completions.
- Keep [tenant isolation](../../ontology/concepts/system_invariants.g5.md#g5.security.tenant),
  [owner operations](../../ontology/concepts/system_invariants.g5.md#g5.architecture.commands)
  and [additive migrations](../../ontology/concepts/system_invariants.g5.md#g5.invariant.migrations).
  Existing local landing and provider-first import remain the owners of atomic local refs,
  merge runs and PR completion, under [multi-repository observation](../../ontology/software_architecture/git.g5.md#g5.git.multi-repo-merge).

## Implementation plan

1. **Require the reviewed source vector.** Make the route DTO and service input mandatory;
   remove the current-head fallback. Reuse normalization and recorded-vector validation.
   Missing/null, empty, malformed, duplicate, forged and incomplete vectors fail without
   inserting a decision or settling its request. Explicit H1 after H2 records H1 if it is a
   valid recorded revision; it never becomes approval evidence for H2. Keep canonical vector
   settlement fingerprints compatible so equal lost-response retries continue to work.
   The only current constructor of `DecisionRequest` is the route; its comment about internal
   omission callers is stale. Preserve the browser's captured draft vector and formal agent
   reviews' separately frozen assignments.

2. **Give the existing provider records target exclusion and durable attempt evidence.**
   The provider lifecycle owns a reservation identified by the existing G5 repository and
   target remote branch, not the G5 PR or target workspace alone: multiple workspaces may
   track the same remote ref. Extend `workspace_pull_request_provider_repos` with the minimal
   attempt identity, immutable provider/source/checked-target evidence and dispatch/outcome
   state needed for recovery. A partial unique constraint/index excludes another active
   reservation for that target across replicas. Repository bindings already enforce unique
   provider repository IDs within an organization. No new queue, scan owner or service is
   needed. New schema uses an additive migration, current RLS and organization references.

3. **Reserve and commit before any provider merge.** Within the lifecycle, validate the
   source admission and binding identities, reserve all changed provider targets in a stable
   order and commit their immutable attempt evidence before effects. If any target is busy,
   perform no provider mutation and roll back that new reservation batch. A bounded conflict
   lets the caller retry; it does not wait on another PR while holding a partial batch.
   Revalidate the workspace target and live provider tip under the reservation immediately
   before dispatch. A waiting PR prepared against T0 must refuse after another PR moves the
   target to T1, then recapture after the target workspace imports T1. Target movement still
   needs no source republication when the source is unchanged.

   Keep existing per-PR/source lock ordering and provider call deadlines. This task need not
   redesign the existing transaction held during provider I/O: a separate committed reserve
   phase supplies durability, and the provider phase keeps its current PR exclusion. Expose
   one lifecycle operation owning reserve, execution and settlement; route callers must not
   implement a new locking protocol. Adapt the coordinated publication handoff and person,
   workspace-agent and merge-when-ready callers to that operation.

4. **Reconcile outcomes without changing the selected target.** Success atomically records
   acceptance and its reserved target and releases active exclusion. A failed local import
   retains accepted evidence and the existing approval admission for retry. A definitive
   refusal releases a fresh attempt only when no previous invocation remains uncertain.
   Transport/deadline failures, invalid responses and database rollback retain the reservation.
   After restart, reconcile the same provider PR/source/target operation before admitting a
   competitor. An open provider PR alone does not prove an older request cannot still merge.
   Retrying that same immutable operation is permitted after revalidation; a refusal of the
   retry cannot erase uncertainty about an earlier invocation. Terminal accepted evidence
   resolves the reservation without a second merge of the provider PR.

   Fence every completion/release by attempt identity and state, including a delayed response
   from an older attempt. Refresh, explicit replacement, provider status upserts and local
   close/deletion paths must not repin or erase an unresolved attempt. In particular, existing
   workspace/PR deletion cascades must not silently remove active target exclusion; refuse
   that cleanup until the provider operation is resolved. Use the existing reconciliation
   entry point from retries/refresh to recover, including a closed local PR; do not depend on
   an in-memory caller surviving. Leave legacy records without merge-time evidence unknown
   rather than backfill publication targets as proven merge targets.

   Update `remote_integrations::import_hold` to recognize unresolved attempts when an open
   multi-repository PR has no live database lock after a crash. Preserve its existing explicit
   pull and local-close exceptions. Description comparison and Activity readers must continue
   to distinguish publication context from accepted merge evidence; turning merge when ready
   off does not clear an already-dispatched attempt.

5. **Exercise production boundaries and run final checks.** Add discriminating regressions
   before relying on the implementation; the schedules below are required. Run affected
   backend and browser suites on the final candidate, inspect their nonzero test counts,
   and record commands/results and any gaps here. Commit and push implementation only after
   approval, on the authorized branch. Publishing/merging a parent PR retains its separate
   workspace shipping rules.

## Why this mechanism

| Candidate | Assessment |
|---|---|
| Require input and keep existing normalization/history | Chosen for approvals; no new review lifecycle or invalidation policy. |
| Per-PR lock or process mutex | Different PRs or API replicas can still race. |
| Target transaction/session lock alone | Serializes live calls, but loses ownership and target evidence on rollback/crash while a provider request may still complete. |
| Reuse `remote_ref_leases` unchanged | Its integration worker lease expires and permits generation takeover. That protects local finalization, not a provider request already in flight. It also belongs to fetch/push/convergence and publication drain, rather than a particular PR's irreversible merge. Extending it would change worker takeover and cleanup semantics. |
| Reuse the existing provider PR row plus database exclusion | Chosen: the row already owns provider identity, source pins, status and recovery. Additional state records the missing durable target selection and unresolved effect; one partial unique index excludes competitors. |
| Derive the checked target after merge | GitHub's PR creation base and an ancestry check do not identify the immediate pre-merge target of a rebase. Do not invent exact evidence from them. |

An unresolved outcome is independently observable evidence, not merely a computed status.
Deriving exclusion from the existing durable attempt is sufficient; avoid a parallel target
reservation table or generic operation journal. Accepted target evidence remains with its
provider record after active exclusion ends.

## Risk assessment and operating limits

Overall risk is **medium to high**: input enforcement is small; recovery around an irreversible
external effect is the consequential part.

| Risk | Mitigation and acceptance condition |
|---|---|
| Older clients omit the vector | Intentional 422 boundary change; preserve current browser payloads and update every test/helper producer. Do not synthesize vectors for compatibility. |
| Ambiguity blocks one target | Persist it and show an actionable retry/reconciliation reason. No TTL-only release. Progress depends on eventual provider evidence or a safe same-operation retry; permanent provider uncertainty cannot guarantee automatic progress. Other target keys remain available. |
| Earlier ambiguous request finishes after a retry | Same immutable provider PR/source/target operation; attempt-fenced writes. Later refusal cannot clear earlier uncertainty. Test a delayed provider effect, not just delayed database status. |
| Multi-repository deadlock/partial acceptance | Stable reservation order, all-or-none reservation transaction before effects, per-repository terminal settlement, retained admission after the first accepted repository. |
| Refresh, close or deletion loses recovery state | Guard all provider row writers and cascades; reconcile closed-local records without permitting new merge authority. Include existing cleanup consumers in tests. |
| Additive schema/rolling deployment | No guessed legacy target backfill. Old binaries do not honor new exclusion: drain in-flight merge requests and deploy all API merge writers together before claiming the guarantee. Ordinary PR reads remain compatible. |
| Provider target changed by an uncoordinated writer | The exclusion covers competing G5 provider PR merges in this organization/repository. Direct GitHub/Git writes and cross-organization actors do not participate. The source-only GitHub API cannot provide target CAS; retain this existing limitation and ancestry checks, and do not claim protection against those writers. |

No general approval invalidation, merge-method change, migration-history rewrite, TLA+/TLC
work, or new automatic review campaign is part of this plan.

## Required verification after approval

- Real HTTP/Postgres/Git approval flow: observe H1, publish H2, submit missing/null vector
  and assert no status/request settlement; submit recorded H1 and assert exact H1 evidence;
  submit current H2 successfully. Cover malformed/partial/forged vectors, equal lost-response
  replay after another push and conflicting retargeted replay. Preserve captured-vector
  component and RV7 browser coverage.
- Real database plus a stateful provider/Git double: pause PR A between target observation
  and provider effect; PR B shares its target and cannot dispatch. After A completes, B's
  old target refuses; import/re-preflight permits B on the new target and records that target.
  A distinct target progresses during A's pause. Exercise separate connections/service
  instances, Base/non-Base targets and workspaces tracking one remote branch.
- Provider refusal, unknown response before/after effect, rollback after acceptance, restart
  over the same database, safe retry, late refusal/success, stale completion and partial
  multi-repository recovery. Independently assert the double's actual pre-merge target against
  retained database evidence and assert local refs/merge-run/PR settlement. Exercise refreshed,
  replaced, closed and deleted-local-PR consumers while an attempt is unresolved.
- Retain existing provider import/auto-pull, stale-source admission, external checks,
  withdrawal-after-acceptance and automatic-refresh regressions. Run merge when ready through
  successful provider-first completion as well as contention/recovery. Provider doubles assume
  a provider PR merges at most once, the source SHA guard is binding, and merged/closed
  terminal reads are authoritative; these are not live-GitHub execution evidence.
- Extend `auto_pull_holds_a_partly_merged_multi_repository_pull_request` in
  `remote_repo_tracking_integration.rs` across a lost provider transaction: a durable unresolved
  attempt preserves the open PR's automatic-import hold, while the existing explicit-pull
  exception still works.

Planned commands (using the repository's configured test database/stack):

```sh
just validation-preflight
cargo fmt --all --check
cargo clippy -p g5-api-core -p g5-api-routes -p g5-api --all-targets -- -D warnings
cargo test -p g5-api-core --lib
cargo nextest run -p g5-api --test integration -E 'test(/^api_workspace_approvals::/) | test(/^api_workspace_review_flow::/) | test(/^api_workspace_pull_requests::/) | test(/^workspace_pull_request_auto_merge::/) | test(/^api_approval_policy_locks::/)'
```

Add the migration and boundary-guard test modules affected by the final schema/owner changes
to the focused selection. The existing provider test harness uses isolated subprocesses;
the new races must exercise concurrent requests inside a single isolated scenario, without
sharing process-global redirect configuration across tests. From `frontend/`, run the review
action and approvals-page Vitest suites, the normal TypeScript build/lint checks, and the
affected real Playwright review-content/recovery flows using the documented stack harness.
The normal current-head Rust/E2E CI checks remain required if the change is published.

## Design self-review

The explicit-vector path preserves historical selections and never substitutes a newer
revision. Target reservation covers the full provider boundary and survives transaction loss;
unknown effects cannot time out into another PR's permission. Target keys do not serialize
unrelated branches. Source approval admission and target attempt evidence remain distinct,
and local import retains its own atomic landing contract. The extra durable evidence is
needed specifically because an irreversible provider merge can outlive its caller. Existing
expiring worker leases and source-only admission do not supply that guarantee.

## Implementation and validation

The owner explicitly approved implementation on 2026-10-28. The provider record retains
an attempt ID, per-invocation execution ID, target SHA and pending/uncertain/accepted facts.
A resumed invocation keeps the original attempt but receives a fresh execution ID; an earlier
unknown invocation prevents a later refusal from releasing exclusion. The accepted fact
separates proven merge-time targets from legacy or refused evidence. The API exposes the
accepted target separately from publication-time context.

The reserve phase commits before provider effects; the execution phase calls the existing
merger's authorization operation again instead of assuming the saved RLS scope still proves
workspace ownership. Repository identity is protected during capture and through unresolved
recovery, and shared workspace fences keep the checked source/target Git cuts stable through
the provider transaction. Metadata contention refuses promptly rather than reversing removal's
lock order. These are implementations of the approved binding and commit guarantees.

The omitted-vector HTTP regression failed against the pre-fix code (200, recording H2),
then passed with required input. The final broad affected native selection passed **197 tests** on implementation commit
`a198b15e7` (nextest run `517ef022-eadd-40a3-9fa0-e9fbd63e8544`, 798.3 seconds), covering approval/review HTTP flows,
provider lifecycle and Base/non-Base imports, automatic refresh, merge when ready, overrides,
workspace deletion, Home approval consumers, multi-repository import holds and owner guards.
The real two-provider partial-batch regression also passed separately before inclusion in
the final broad run (`f08d4126-dabc-4ab7-83bf-7e10c8c5dd75`): one provider accepted while the other lost its response;
both local targets stayed unchanged, and recovery retained the selected targets and original
admission after approval withdrawal, without merging the accepted member twice.

The new recovery module additionally exercises delayed effects, independent targets, database
rollback after real acceptance, lost database connections, fresh service instances, stale
completion, refusal of a retry after earlier uncertainty, and closed-PR/deletion consumers.
The final six-test rerun passed in 47.0 seconds (run
`923bd436-941d-47a0-94fc-1355f51092f9`), including different G5 target workspaces tracking
the same remote ref, reconciliation through the real closed-PR retry route, and fresh versus
uncertain reservation cleanup after reauthorization refusal. The final broad run includes
all of these strengthened cases after that cleanup change. The import hold covers the last unresolved member after all siblings accept, while
preserving explicit-pull and local-close exceptions.

`cargo test -q -p g5-api-core --lib` passed **1,071 tests** after the final behavior changes.
The review-action and approvals-page Vitest suites passed **37 tests** on integrated main;
the normal TypeScript/Vite production build passed. Changed TypeScript files pass selected
ESLint with one inherited unused `Page` import warning in `alignment-shell.spec.ts`.
`cargo fmt --all --check` and
`cargo clippy -p g5-api-core -p g5-api-routes -p g5-api --all-targets -- -D warnings` pass.
Migration checksums and version uniqueness pass (additive migrations **135 and 136**, after main's
132–134). Current-head runtime qualification is supplied by the normal PR CI checks
linked below; discovery and local stack preparation are not browser execution evidence.

The original preflight inventory failure on `9e72c262b` is resolved by main's existing
`e87791272`; no separate CI repair was added. Standalone preflight passed at `64d925a1c`
(`.dev/validation/runs/20261028-202033-f97621/`); fresh browser preparation must certify the
later final candidate. Source-only preflight checks passed for the
integrated candidate before the cold Docker preparation in
`.dev/validation/runs/20261028-193451-26a4ff/`. That preparation started at `f6608193c` and
built the cold image caches; later source/head changes require fresh candidate preparation
before browser execution. Its stub dependency warmup's zero-test summaries are not application
validation. The intended browser selection discovers 11 Chromium cases (plus auth setup):
RV7, RV10 and the cross-phase approval case across review-content, review-recovery and
alignment-shell. Runtime qualification is recorded by the PR's current-head CI checks, linked below.

Regression fixes preserve ordinary admission for a provider merge completed outside G5
without a G5 attempt, and the existing explicit replacement flow for such a PR. Refresh
reconciles unresolved G5 attempts before repinning; ordinary publication retains its existing
outside-merge handling. Reconciliation uses the caller's credential scope so a queued request
cannot exhaust the pool while waiting on its PR lock. Provider fixtures repairing their clone
identity now update both repository metadata and the provider record, rather than masking a
real identity mismatch. These retain existing intent while enforcing the approved boundary. Final handoff inspection
also found that a failed reauthorization returned before known-unsent cleanup. The same
execution-fenced cleanup now covers that refusal: it releases a fresh, undispatched attempt
while retaining any earlier uncertainty. This implements the planned safe-release distinction; it does not grant
merge authority or relax an unresolved reservation.

The first full PR CI run exposed affected consumers absent from the focused selection:
organization bootstrap updates repository identity under the system role, which cannot
read provider PR rows, and two agent-quorum fixtures still omitted human reviewed vectors.
Migration 136 gives only the repository trigger the existing restricted NOLOGIN lookup
owner's authority, with fixed search path and no public execution; system callers retain
their existing table permissions. The target-state SQL stays unchanged when renumbered. The pending-attempt test
also exercises the system role's identity guard, while real organization-creation tests
cover the permitted bootstrap path. The merge module is registered in the existing Git
context inventory. These fixes preserve the approved exclusion and tenant boundaries.
All **17 focused follow-up tests pass** (run `40f65c4d-71ac-4074-ac1e-3c0318f270b2`,
60.2 seconds), including the six strengthened provider recovery cases, both agent-quorum
consumers, organization creation, the complete context checks and fresh catalog/replay
under a restricted migrator. Strict Clippy, formatting, migration checks and fresh standalone
preflight (`.dev/validation/runs/20261028-214720-99f369/`) pass. An initial local rerun
could not connect to a stopped PostgreSQL service and was interrupted; the successful run
used an explicit connection to the restarted local test database.

## Integration with updated main

Before code integration, inspected incoming main `a88c75abcea0f071c862cca1e85f35ab116e3380`
against the analyzed `9e72c262b`. Its new override and provider-rules commitments are compatible:
an override preserves the actor's authority, shown blockers and recorded source admission;
it cannot waive mechanical target exclusion. Carry its returned admission through the shared
reserve/execution capture so the existing GitHub bypass rules and bypass-first ordering also
apply to this lifecycle. Ordinary, override and merge-when-ready entry points keep using the
same protected provider operation. Preserve the browser-only override boundary and the
person's GitHub identity when bypassing, without changing approval decisions.

Main also attributes provider merges whose successful response G5 did not record as observed
outside G5, including lost acknowledgements. Preserve that existing conservative attribution;
the durable target attempt proves what G5 selected and may have dispatched, not who caused
an independently observed provider effect. Update its now-outdated statement that no such
attempt record exists, referencing the target-evidence owner rather than duplicating it.

Renumber this branch's additive migration after main's 132 and 133, keeping their checksums
unchanged. Main's `e87791272` includes the missing SCM integration lane, so rerun preflight
after integration instead of carrying a separate CI fix. Revalidate the new override/provider
attribution consumers as well as the approved recovery and input scenarios. The existing
finite model summaries remain historical evidence, not proofs of this code integration.

The follow-up rebase onto `ce6e4bb50cea3ea48971c4449429a6eaa386a777` preserves main's
harness guidance and packaging changes. Its ontology changes move authoring references
and clarify guidance delivery and maintenance; they do not conflict with this issue's
approval or provider-recovery commitments.

Main then advanced to `8e54b3ebc5c5f89d97b6dc919bbc574eed56d0dd`, adding migration
134 to remove the retired review artifact UUID after explicit projections have shipped.
Preserve that migration and its rollout prerequisites unchanged. It does not alter this
issue's reviewed source vectors or provider attempts. Renumber this PR's target state and
trigger authority migrations to 135 and 136, preserving their SQL bytes and checksums;
only their catalog filenames and version order change. No shared migration is rewritten.

CodeRabbit's remaining suggestion to reuse an older admission after the current source
advances is not adopted: both admission and provider capture intentionally require the
same selected source cut. Terminal reconciliation runs before admission and can release
the reservation even when the new local cut must refuse; the strengthened delayed-effect
regression exercises that boundary and preserves the original admission and checked target.
The conflict also stays generic because another workspace's reserving PR can be private.
Exposing its ID or relaxing the current-cut gate would require behavior beyond this fix.

The review summary also identified a stale-completion fallback that wrote `failed`
status and an event after the attempt-fenced acceptance update refused. A native delayed
provider-response regression reproduced the overwrite of a superseding execution's status
and error. Stop that stale executor before fallback or further dispatch, and make the
shared status update atomically compare captured attempt/execution identities and protect
newly accepted state before recording its event. Exercise delayed success and refusal.
Do not adopt the summary's blanket `merge_accepted` deletion block: accepted evidence is
terminal audit state, not active exclusion. The extended close/reconcile/delete regression
already passes before this fix and proves cleanup retains that evidence after refusing
deletion during uncertainty.

An earlier summary comment also identified merge-when-ready's fresh-preview shortcut
blocking the same recorded-admission recovery. Let retained pending or accepted provider
evidence reach the existing merge route, which still reauthorizes the person and validates
the exact recorded cut. The summary's target SHA is projected only for accepted attempts;
a released refusal does not create this exception. Keep the ordinary approval wait, checks
settlement and retry budget. A real worker regression withdraws approval after an unknown
first dispatch and recovers the same attempt, target and admission on its next eligible pass.

The combined schema passed all nine focused recovery/upgrade tests (run
`baa06c35-2b49-4e48-ac9c-5d7ac21f0f88`, 58.3 seconds) and strict Clippy. CI on
`8364cafbf` passed the full Rust check. Its one underlying browser failure was the
personal Changes case: the first Save reported a Git push failure, and subsequent
attempts encountered files retained by the failed attempt. Other VMs can sync the shared
default user's personal memory. Give this exact-diff case a fresh authenticated owner
per attempt, retaining every Save, private-file and reference-preservation assertion;
this removes shared personal state without weakening the product's push checks. The
bounded logs lack the original Git failure detail, so current-head CI must qualify the
isolated case. The provider-agent tests passed; their receipt and the value-loop receipt
rejected the same failed browser selector. The next rebase to
`ee162e9e02ca197f2f8fd1150ba9c9e28581baff` preserves comment-only worker-host cleanup.

CI on `14898fb4e4604bc0eae755fe24d3dc3b8f34f270` passed Rust, E2E and CodeRabbit.
All eleven affected approval browser cases and the isolated personal Changes Save case
passed on their first attempt. Main advanced during that run to
`0949090213b257272d80d25fb43c483372e75ca7`: preserve its local worker/preview runtime
staging, inherited-limit and foreign-mount safeguards. Those changes and their existing
design are compatible with this issue's approval and target-exclusion commitments;
the rebased published head must pass its own current-head checks.

CI on `d7caa6d7391b6054b64330ddebc46a433b8ec8b3` passed Rust and E2E; CodeRabbit
skipped the equivalent rebase. All eleven affected approval cases and personal Changes
again passed without retries. Main then advanced to
`e72106c8b17af06b47e48c2a24c2c7487e54629c`. Its per-host cache profile projection
is independent of approval and merge ownership: preserve its read-only per-boot input,
caller override and no-profile behavior, and explicit boot refusal for invalid inputs.
The shared runtime continues to carry only the empty placeholder. Revalidate this
combined head through current-head CI, including main's guest/runtime consumers.

CI on `02f8a2060ab39e3e3fa544d8ed5cea85a04da966` passed Rust and E2E; all eleven
affected approval browser cases and personal Changes passed without retries. CodeRabbit's
automatic review is paused at `14898fb4e` and its later checks are skipped; all four
available review bodies and the inline threads were inspected for this follow-up.
The stale-status regression failed before the fix (run `e64f8259-201f-42fa-b44d-241f3baace79`),
while terminal cleanup already passed. The fix passed all 91 workspace PR regressions
(run `bbd0206f-42b4-4035-83f8-1d4b02ad2b13`, 368.6 seconds), followed by all 20
provider recovery/merge-when-ready cases including the new worker recovery (run
`8ac9c184-5c7d-4183-aa91-81e460b96a29`, 79.9 seconds), and final strict Clippy.
The new published head still requires its own full CI qualification.


## Browser qualification on supported CI

Local preparation built the application and worker images, started healthy API/frontend/database
services and seeded the CI organization. The first run stopped on cleanup of root-owned
Docker-exported runtime files. Running the unchanged harness with the necessary filesystem
privileges passed source-only preflight on `d05a63c9c` and progressed to worker startup
(`.dev/validation/runs/20261028-205617-0cf0c8/`).

The local test Compose worker uses a private cgroup namespace, while its existing entrypoint
requires the complete Docker hierarchy. A disposable probe with the required host namespace
and read-only hierarchy confirmed a separate host limitation: this MicroVM's kernel exposes
only the `capability` security module and has no AppArmor interface. The worker requires
AppArmor and explicitly refuses a fallback. No isolation guard was changed or bypassed;
no browser scenario ran locally. Local preparation did not complete browser qualification.

The implementation is published as [PR #930](https://github.com/G5Labs/G5-Lean/pull/930).
Its [live current-head checks](https://github.com/G5Labs/G5-Lean/pull/930/checks) are the
authoritative runtime qualification: `g5-ci-e2e` must pass on the published head, along with
`g5-ci-rust`. The Cloud E2E launchers supply the required host cgroup view and read-only
ancestor mount. Native HTTP/database/Git evidence does not substitute for browser execution;
PR check results and the PR's validation description record that evidence without treating
local preparation as success. Merge remains a separate owner decision.
