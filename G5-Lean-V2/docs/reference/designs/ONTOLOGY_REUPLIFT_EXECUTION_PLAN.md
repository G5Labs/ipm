# G5 Ontology Re-Uplift Execution Plan

**Prepared on**: 2026-03-23  
**Related docs**:
- [TECH_DEBT.md](/Users/lumost/workspace/g5/generation/designs/TECH_DEBT.md)
- [TECH_DEBT_DECISION_LOG.md](/Users/lumost/workspace/g5/generation/designs/TECH_DEBT_DECISION_LOG.md)
- [VIEW_SYNC_UNIFICATION.md](/Users/lumost/workspace/g5/generation/designs/VIEW_SYNC_UNIFICATION.md)
- [WORKER_INFRASTRUCTURE.md](/Users/lumost/workspace/g5/generation/designs/WORKER_INFRASTRUCTURE.md)
- [G5D_CLI.md](/Users/lumost/workspace/g5/generation/designs/G5D_CLI.md)
- [README.md](/Users/lumost/workspace/g5/README.md)

## Purpose

This file is a concrete execution plan for bringing the current repository back
under the active G5 ontology workflow, using `generation/.semlift/specs/` as
the current ontology source of truth.

The goal is not only to run one successful uplift. The goal is to make the
uplift and follow-on view/downlift workflow trustworthy enough that the result
can become the basis for future ontology-driven project work.

## Operating Assumptions

These assumptions define the intended execution model for this plan.

1. The current ontology is the contents of `generation/.semlift/specs/`, not
   the prose design docs.
2. Views remain doc-centric for this effort, per decision `D-008` in
   [TECH_DEBT_DECISION_LOG.md](/Users/lumost/workspace/g5/generation/designs/TECH_DEBT_DECISION_LOG.md).
   We should use the existing `docs` plus `doc_node_links` view workflow rather
   than waiting for ontology-native views.
3. The first production-like validation should happen on remote `dev`, not
   only locally. Local runs are still required as a fast feedback loop and as a
   fallback when remote workers are unstable.
4. Code promotion and workspace promotion are different concerns:
   code should be promoted through the normal deploy pipeline,
   while workspace state should be recreated from a deterministic seed path in
   each environment.
5. Until multi-repo regular job routing is fixed, the first self-uplift
   workspace should be intentionally narrow: one primary source repo, one
   explicit ontology root or explicit target roots, and no dependence on
   ambiguous "pick the first repo/root" behavior.

## Desired Outcomes

At the end of this program we want all of the following to be true:

- The uplift and downlift pipeline fails closed on the known ontology
  correctness hazards instead of silently degrading.
- We have a reproducible G5 workspace whose checked-out repo and
  `.semlift/specs` tree are aligned.
- The same seeded workspace can be recreated in `dev`, `staging`, and `prod`
  without manual database surgery.
- A remote uplift can run successfully and produce reviewable ontology deltas.
- `TECH_DEBT.md` has a corresponding view hierarchy that can act as a planning
  and decomposition surface.
- Cascading downlift can proceed in controlled, explicitly targeted slices
  instead of one giant unsafe full-project blast.

## Non-Goals

This plan does not assume we will solve every long-tail ontology or UI issue.
In particular, the following are out of scope unless they become direct
blockers:

- Converting views into first-class ontology nodes
- Broad frontend paper-cut cleanup unrelated to uplift/downlift truthfulness
- Full long-term multi-repo orchestration if a safe single-repo or explicit
  target-root path is sufficient for the first self-uplift
- General maintainability cleanup that does not affect correctness,
  rollout safety, or reviewability

## Release Strategy

The program should run in this order:

1. Fix the ontology-pipeline correctness blockers locally and in CI
2. Create a deterministic G5 self-uplift workspace shape
3. Deploy the code changes to remote `dev`
4. Seed the self-uplift workspace in remote `dev`
5. Run and validate the first real uplift in remote `dev`
6. Create the `TECH_DEBT` view and subviews in that validated environment
7. Use targeted cascading downlift to spread the plan into the codebase
8. Only after `dev` is proven, promote the same code and workspace-seeding
   recipe to `staging`, then `prod`

This is intentionally different from "push everything everywhere, then try the
workflow." The workflow itself is part of what is being validated.

## Known Blockers To Clear First

These are the issues that would make any self-uplift untrustworthy if left in
place.

| Blocker | Why it matters | Required action | Required tests |
| --- | --- | --- | --- |
| Scoped uplift keeps running after ontology upsert mapping failure | The system can report success while scoped targeting and `node_id` linkage are broken | Make scoped uplift fail when `component_id_to_node_id` cannot be produced | Worker bootstrap regression test for missing upsert mapping |
| Cascading downlift treats missing target specs as a successful no-op | A missing spec can look like "no work to do" | Make unreadable target specs fail the bootstrap or return explicit degraded status | Downlift tests for missing child/view target specs |
| Shared ontology DTO still advertises old `node_type` vocabulary | First-party components can speak the wrong ontology contract | Unify `g5-common`, API, tests, and DB vocabulary on the same allowed set | DTO serde tests plus API validation tests |
| Ontology upsert is still not truly `component_id` authoritative | Identity can be stolen by structural name collisions | Make upsert reconcile and return IDs by `component_id`, not by sibling name conflict | API route tests for `component_id` collision and stable mapping |
| Regular task routing still picks first repo or first root | A job can execute against the wrong repo/root while appearing healthy | Either implement explicit routing or reject ambiguous cases up front | Multi-root and multi-repo job creation/assignment tests |
| Accept/merge semantics are still underspecified for uplift review | A user could click accept without invoking a real canonical merge boundary, or a partial merge could be mistaken for success | Make `Accept` call the existing G5 workspace merge operation and treat any branch failure as acceptance failure | Workspace merge success/failure tests, including partial-merge handling |

### Recommended Scope Decision

For the first self-uplift program, we should treat all six blockers as
mandatory, but keep the merge model intentionally minimal:

- the uplift result remains isolated in the uplift workspace before review,
- `Accept` invokes the existing G5 workspace merge operation,
- the result is canonical only if that merge succeeds with `branches_failed = 0`,
- and merge failure remains visible and retryable or remediable depending on
  whether the failure was clean or partial.

This avoids inventing a second publish primitive while still giving the pilot a
real merge boundary.

## Workstreams

This plan is easiest to execute as four coordinated workstreams.

| Workstream | Focus | Main deliverables |
| --- | --- | --- |
| A. Pipeline correctness | Fail-closed uplift/downlift behavior, stable ontology identity, explicit routing | Code fixes, route fixes, worker bootstrap fixes, regression tests |
| B. Workspace materialization | Reproducible self-uplift workspace shape in local and remote environments | Seed script, manifest validation, recorded environment inputs |
| C. Remote rollout | Deploy and validate on `dev`, then promote to `staging` and `prod` | Cloud Build rollout, remote seed, smoke checklist |
| D. View-driven decomposition | `TECH_DEBT` umbrella view, issue-cluster subviews, targeted cascading downlift | View docs, sync runs, clustered downlift waves |

## Phase Plan

## Phase 1 - Fix The Pipeline And Test It

### Goal

Make the ontology pipeline truthful enough that a successful uplift actually
means something.

### Required code changes

#### 1. Fail closed on scoped uplift mapping failure

- Change scoped uplift bootstrap so `upsert_ontology_nodes()` failure is fatal
  for scoped jobs.
- Preserve the current happy path for unscoped full uplift only if we can
  justify that degraded mode explicitly.
- Surface the error clearly in job state so the UI and logs show "scope mapping
  failed" rather than "completed with zero tasks."

#### 2. Fail closed on missing cascading-downlift target specs

- Replace silent `read_to_string(...).ok()` fallback for target specs with a
  structured error path.
- Differentiate:
  - "no linked children / no child specs" and
  - "the target specs required to decide what to do could not be read"
- Keep true no-op semantics only for genuinely empty work, not missing input.

#### 3. Unify ontology `node_type` contract

- Update `g5-common` internal DTOs to match the API and schema vocabulary.
- Update serde tests that still encode `"module"` and similar retired values.
- Audit worker-side payload generation and route comments so the shared contract
  and the server's accepted values are identical.

#### 4. Make ontology upsert truly `component_id` authoritative

- New inserts should reconcile on `(workspace_branch_id, component_id)` when
  `component_id` is the advertised stable identity.
- The response mapping must be based on the actual stored row IDs, not the UUIDs
  preallocated before conflict resolution.
- Structural sibling-name collisions should either:
  - update the same `component_id`, or
  - fail as a real identity conflict,
  but should never silently rebind ownership.

#### 5. Remove ambiguous first-root / first-repo behavior

- For "full" downlift, either:
  - collect all roots, or
  - reject empty-target requests until multi-root support is explicit.
- For regular multi-repo jobs, either:
  - route tasks explicitly from the target nodes' repo identity, or
  - reject multi-repo regular jobs during the pilot.
- Keep bootstrap multi-repo scanning if it is already safe, but do not let
  regular execution silently collapse onto one repo.

#### 6. Define the pilot accept and merge contract

Recommended choice for the pilot:

- `Accept` must invoke the existing G5 workspace merge operation for the uplift
  workspace.
- Pre-accept ontology output remains isolated and reviewable but is not yet
  canonical.
- An accept attempt only counts as success when the merge call succeeds and
  returns `branches_failed = 0`.
- If the merge call returns an error or returns `branches_failed > 0`, the
  uplift must remain unaccepted and the merge details must be surfaced to the
  operator.
- Partial merge is its own explicit remediation state:
  - `merge_failed_clean` means `branches_merged = 0` and retry or reject is safe.
  - `merge_failed_partial` means `branches_merged > 0` and `branches_failed > 0`,
    so canonical state may already be partially updated and remediation is
    required before retry or reject.

### Required tests

This phase is not complete until the new protections have regression coverage.

#### Unit and integration tests

- `g5d` worker bootstrap test: scoped uplift fails when ontology upsert mapping
  cannot be produced
- `g5d` worker bootstrap test: cascading downlift fails when a required target
  spec is missing
- `g5-common` DTO serde tests: `node_type` vocabulary matches runtime contract
- `g5-api` internal upsert tests: `component_id` collision behavior is explicit
  and stable
- `g5-api` job/task tests: multi-root full downlift and multi-repo regular jobs
  either route correctly or are rejected explicitly
- `g5-api` workspace merge tests: accept treats `branches_failed > 0` as a
  failed acceptance even when the HTTP response succeeds

#### Happy-path validation

Keep these existing targeted checks in the validation set:

- `cargo test -p g5d test_uplift_agent_creates_spec_from_code -- --nocapture`
- `cargo test -p g5d test_downlift_then_uplift_roundtrip -- --nocapture`

#### Broader gates

- `cargo test -p g5d`
- `cargo test -p g5-api`
- Any focused E2E or API smoke covering view sync if touched by the fixes

### Exit criteria

Phase 1 is done when all of the following are true:

- Scoped uplift cannot silently proceed without ontology mapping
- Missing downlift target specs do not turn into false-success no-ops
- `node_type` vocabulary is unified across shared DTOs, API, and tests
- Ontology upsert identity is deterministic and tested
- Ambiguous first-root and first-repo behavior is either fixed or blocked
- Pilot accept semantics are tied to workspace merge and tested for failure
- Happy-path uplift/downlift tests still pass

## Phase 2 - Create A Reproducible G5 Self-Uplift Workspace

### Goal

Stand up a dedicated workspace that contains the G5 repo and the current
ontology in the same shape we expect remote workers to use.

### Recommended workspace shape

For the first iteration, keep the workspace intentionally narrow:

- one primary source repo: this repo
- current ontology copied or materialized under `.semlift/specs`
- views stored under the current doc-centric `views/` tree used by the product
- no reliance on implicit multi-root or multi-repo routing

### Why not "promote" a dev workspace directly?

Workspace state is environment-local operational data, not a release artifact.
The safe model is:

- promote code through Git and Cloud Build,
- then recreate the workspace in each environment using the same seed recipe.

That gives us repeatability and avoids hidden drift between environments.

### Deliverables

#### 1. A deterministic seed script

Proposed new script:

- `generation/scripts/seed-g5-ontology-workspace.sh`

Its job should be to:

- create or locate the target org and workspace
- attach the G5 repo at the intended branch and commit
- materialize the current ontology into `.semlift/specs`
- create any minimal root/view metadata needed for the pilot
- emit stable identifiers and a summary of what was created

The script should accept environment-specific inputs such as:

- API base URL
- org slug or ID
- workspace name
- source repo URL or repo ID
- source branch
- optional docs or view paths

#### 2. A format and validation pass for the ontology tree

Before running the first remote uplift, validate the current ontology tree as a
real input artifact, not just a directory we happen to have.

The validation pass should check at least:

- required frontmatter fields exist
- `component_id` matches path expectations
- dependency references resolve
- child-spec references resolve
- view file locations match the current `views/` convention used by create/sync
- format-specific validation for any active G5 ontology authoring rules

If no one-shot validator exists yet, this phase should add one.

#### 3. Manifest and materialization verification

Verify that:

- `g5d clone` can fetch the workspace manifest and materialize the workspace
- local `.g5/workspace.yaml` and checked-out repos match the intended state
- worker bootstrap can operate from that materialized workspace shape

### Exit criteria

Phase 2 is done when:

- a seed script can create the self-uplift workspace from scratch
- the resulting workspace contains the G5 repo and the active `.semlift/specs`
  ontology in the intended shape
- ontology validation passes
- clone or refresh against the manifest contract succeeds

## Phase 3 - Deploy To Remote Dev And Seed The Workspace There

### Goal

Run the first production-like uplift path on remote `dev` rather than assuming
local success is enough.

### Deployment path

The existing repository already documents the environment ladder and manual
promotion model:

- `dev` auto-deploys on merge to `main`
- `staging` requires approval
- `prod` requires approval

Operationally, the plan should use:

- code deployment through Cloud Build
- environment checks through `kubectl` or the existing helper targets
- workspace seeding through the deterministic seed script from Phase 2

### Recommended environment order

1. `dev`
2. `staging`
3. `prod`

But only `dev` should receive the first self-uplift workspace and workflow run.
`staging` and `prod` should wait until `dev` passes the uplift and view gates.

### Remote runbook

#### Dev

1. Merge the Phase 1 and Phase 2 changes
2. Trigger or wait for `dev` deployment
3. Verify deploy health
4. Run the self-uplift workspace seed script against `dev`
5. Record:
   - workspace ID
   - repo binding
   - ontology root IDs
   - any view doc IDs created for the pilot

#### Staging

After `dev` succeeds:

1. Approve `staging` promotion
2. Verify deploy health
3. Re-run the same seed script against `staging`
4. Run the reduced smoke checklist

#### Prod

After `staging` succeeds:

1. Approve `prod` promotion
2. Verify deploy health
3. Re-run the same seed script against `prod`
4. Run the reduced smoke checklist

### Important policy

Do not copy raw workspace rows from `dev` to `staging` or `prod`.
Recreate them using the same declared inputs.

### Exit criteria

Phase 3 is done when:

- `dev` is running the fixed code
- the self-uplift workspace exists in `dev`
- the workspace can be recreated in another environment with the same seed path
- the recorded environment inputs are sufficient to reproduce the workspace

## Phase 4 - Run The Uplift

### Goal

Execute a real uplift against the remote `dev` self-uplift workspace and verify
that the output is trustworthy enough to proceed into view-driven work.

### Execution approach

Recommended:

- remote `dev` is the primary uplift run
- local runs remain the debugging and preflight tool
- any remote failure must be reproducible locally before deeper surgery

### Accept and merge contract

For this pilot, `Accept` is not a separate publish primitive. `Accept` means
"attempt the normal G5 workspace merge for the uplift workspace."

The contract is:

- Before `Accept`, the uplift output is reviewable but not canonical.
- On `Accept`, call the existing G5 workspace merge operation.
- The uplift is only accepted when that merge succeeds and reports
  `branches_failed = 0`.
- If the merge call fails, or if it returns `branches_failed > 0`, the uplift
  remains unaccepted and the merge result becomes the authoritative failure
  record.

Operator-visible failure states:

- `merge_failed_clean`
  - `branches_merged = 0`
  - canonical workspace is unchanged
  - retry or reject remains safe
- `merge_failed_partial`
  - `branches_merged > 0`
  - `branches_failed > 0`
  - merge failed overall, but canonical state may already be partially updated
  - retry or reject is no longer a no-side-effects action and requires
    remediation

### Scope strategy

For the first run, avoid ambiguous scope.

Preferred options:

1. Explicitly target the intended root node or root nodes
2. If the system still cannot safely represent that, uplift the one narrow root
   that corresponds to the primary repo and record that limitation

Do not rely on:

- empty-target "full downlift" semantics
- implicit first-root selection
- implicit multi-repo assignment

### Validation checklist

The uplift is only considered good if all of the following are true:

- the job completes without degraded-mode warnings
- the intended scope root is the one actually processed
- `node_id` linkage is present for affected tasks where expected
- output specs land under `.semlift/specs`
- diffs are reviewable and correspond to the intended repo state
- repeated uplift does not immediately produce contradictory or explosive drift

### Required evidence to retain

- job ID
- task graph or task count
- worker logs for bootstrap and execution
- resulting spec diff
- any review notes explaining why the output is accepted or rejected

### Exit criteria

Phase 4 is done when:

- at least one remote uplift succeeds on `dev`
- the result is reviewed and accepted as a trustworthy ontology delta
- the same flow can be reproduced locally if needed

## Phase 5 - Create A `TECH_DEBT.md` View

### Goal

Create one umbrella planning surface over the tech-debt corpus that can guide
subsequent decomposition and downlift work.

### Strategy

Per the current view architecture, this should be done as a doc-centric view,
not as an ontology-native view migration project.

### Recommended artifact

Create a PM view such as:

- `views/pm_views/tech_debt_program.pm_view.md`

Recommended linked inputs:

- `generation/designs/TECH_DEBT.md`
- `generation/designs/TECH_DEBT_PRIORITY_REVIEW.md`
- `generation/designs/TECH_DEBT_ROADMAP.md`
- `generation/designs/TECH_DEBT_RISK_REGISTER.md`

### Expected contents of the umbrella view

The first `TECH_DEBT` view should focus on:

- program objective
- issue grouping model
- release blockers versus later cleanup
- execution sequencing
- environment and rollout risks
- what must be fixed before broad ontology-driven project work can resume

It should not try to encode every individual low-severity finding.

### Exit criteria

Phase 5 is done when:

- the umbrella PM view exists
- it links to the intended tech-debt source docs
- it can be opened and synced through the first-party view flow using the
  current `views/` path convention
- it is useful as a planning surface, not just a copy of `TECH_DEBT.md`

## Phase 6 - Downlift The Umbrella View Into Subviews

### Goal

Turn the one umbrella `TECH_DEBT` view into a manageable hierarchy of
issue-cluster views that can drive more targeted work.

### Recommended subview split

Start with five subviews:

1. `tech_debt_security_isolation.pm_view.md`
2. `tech_debt_runtime_truthfulness.pm_view.md`
3. `tech_debt_ontology_pipeline.pm_view.md`
4. `tech_debt_workspace_materialization.pm_view.md`
5. `tech_debt_product_workflows.pm_view.md`

Optional sixth subview later:

6. `tech_debt_long_tail_cleanup.pm_view.md`

### Why this split

This matches the actual execution pressure in the repository:

- security and isolation work should not be mixed with general workflow polish
- runtime truthfulness and ontology-pipeline correctness are distinct but
  coupled
- workspace materialization and deployment are operationally separate from the
  ontology authoring work
- visible workflow repairs deserve their own surface because they affect
  adoption and operator confidence

### Sync model

Each subview should be created and synced as its own explicit job.
Do not depend on obsolete additive multi-phase view bootstrap assumptions.

### Exit criteria

Phase 6 is done when:

- the umbrella view has a stable set of subviews
- each subview has a clear scope and no obvious overlap confusion
- sync can run on each subview independently

## Phase 7 - Cascade Downlift Into The Rest Of The Project

### Goal

Use the subviews as orchestration fronts for targeted downlift into real
project specs and code, without falling back to one unsafe full-project wave.

### Key rule

The rest-of-project downlift should proceed in explicit clusters, not by
pressing the most global button available.

### Recommended downlift waves

#### Wave A - Ontology pipeline core

Target the components most directly tied to the blockers:

- `g5-common` internal ontology DTOs
- `g5-api` internal ontology routes
- `g5-api` job and task routing around ontology scope
- `g5d` bootstrap logic

This wave should produce the code and spec changes that make the pipeline safe
enough for further ontology work.

#### Wave B - Workspace materialization and deploy flow

Target:

- workspace manifest and clone or refresh path
- worker materialization assumptions
- environment seeding and rollout docs or scripts

This wave should make the workspace recreation path explicit and repeatable.

#### Wave C - View workflow and TECH_DEBT planning surfaces

Target:

- view creation and sync usability issues required for the tech-debt program
- subview creation and linkage flow
- any missing browser or API affordances that block the planned workflow

#### Wave D - Broader project cascade

Once Waves A through C are stable, continue downlift cluster-by-cluster through
the broader G5 codebase. Suggested order:

1. `g5-common`
2. `g5-api`
3. `g5d`
4. `client`
5. remaining design and auxiliary surfaces

### Merge discipline

After each wave:

1. review changes
2. merge accepted work
3. re-run uplift
4. confirm drift shrinks rather than grows

That loop is how we keep the ontology and code converging instead of forking.

### Exit criteria

Phase 7 is done when:

- the targeted waves have been completed and merged in order
- uplift after each wave is stable and reviewable
- the codebase is meaningfully closer to ontology-driven operation than when
  the plan began

## Commands And Operational Hooks

These are the main operational hooks already present in the repo that this
program should rely on.

### Local development and validation

- `just dev-up`
- `just worker`
- `just demo-up`
- `cargo test -p g5d ...`
- `cargo test -p g5-api ...`

### Remote deployment and status

- `just cb-deploy dev main`
- `just cb-deploy staging main`
- `just cb-deploy prod main`
- `just deploy-status`

### Remote promotion model

The repository already documents that:

- `dev` auto-deploys
- `staging` requires approval
- `prod` requires approval

This plan assumes we use that ladder, not a custom one-off rollout path.

## Milestone Summary

| Milestone | Outcome | Gate |
| --- | --- | --- |
| M1 | Pipeline blockers fixed and tested | Fail-open uplift/downlift paths removed |
| M2 | Reproducible self-uplift workspace exists | Seed script and ontology validation pass |
| M3 | Remote `dev` is running the fixed path | Workspace seeded in `dev` |
| M4 | First real uplift accepted | Remote `dev` uplift is reviewable and trustworthy |
| M5 | `TECH_DEBT` umbrella view exists | View can be created and synced through supported flow |
| M6 | `TECH_DEBT` subviews exist | Issue-cluster decomposition is stable |
| M7 | Project cascade underway | Clustered downlift waves are merging cleanly |
| M8 | `staging` and `prod` recreated safely | Code promoted and workspace reseeded reproducibly |

## Risks To Watch

| Risk | Why it matters | Mitigation |
| --- | --- | --- |
| We treat happy-path test success as proof of correctness | The degraded modes are the real risk in this program | Keep fail-closed regression tests mandatory |
| We seed a complex workspace before the routing fixes land | The first real run could target the wrong repo or root | Keep the initial workspace intentionally narrow |
| We "promote" workspace rows from one environment to another | Hidden environment drift will be hard to explain or undo | Recreate workspace state from a script in every environment |
| We block on ontology-native views | That would defer the program for a non-critical architectural cleanup | Stay with doc-centric views for this effort |
| We try a giant full-project downlift too early | This maximizes the blast radius of any remaining routing ambiguity | Use clustered, explicitly targeted waves |
| We mistake a failed or partial workspace merge for acceptance | The uplift can look approved even though canonical state is unchanged or only partially updated | Make `Accept` inspect workspace merge results and surface clean vs partial failure states |

## Recommended First Sprint

If we start immediately, the first sprint should do only this:

1. Fix scoped uplift fail-open behavior
2. Fix cascading downlift false-success behavior
3. Unify ontology `node_type` contract
4. Make `component_id` upsert identity deterministic
5. Lock the pilot `Accept -> workspace merge` contract and test failure states
6. Add regression coverage for all five
7. Draft the self-uplift workspace seed script interface, even if the full
   implementation lands in sprint 2

That sprint creates the highest leverage for everything that follows.

## Final Go/No-Go Checklist

Do not proceed to broad self-uplift and cascading downlift unless all of these
are true:

- [ ] The active ontology is validated under `generation/.semlift/specs`
- [ ] Scoped uplift cannot silently lose ontology mapping
- [ ] Cascading downlift cannot silently treat missing target specs as success
- [ ] Shared ontology DTO contract matches live API behavior
- [ ] Ontology upsert identity is stable and tested
- [ ] Ambiguous first-root and first-repo execution is fixed or blocked
- [ ] A self-uplift workspace can be recreated from a script
- [ ] Remote `dev` deploy is healthy
- [ ] The first remote uplift result has been reviewed and accepted
- [ ] `TECH_DEBT` umbrella view and subviews are created through the supported
      doc-centric view pipeline

Once those are true, the repository is in a materially better position to
resume ontology-driven work instead of merely talking about it.
