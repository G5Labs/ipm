# TECH_DEBT Decision Log

**Related docs**: [TECH_DEBT_PRIORITY_REVIEW.md](/Users/lumost/workspace/g5/generation/designs/TECH_DEBT_PRIORITY_REVIEW.md), [TECH_DEBT_ROADMAP.md](/Users/lumost/workspace/g5/generation/designs/TECH_DEBT_ROADMAP.md), [TECH_DEBT_RISK_REGISTER.md](/Users/lumost/workspace/g5/generation/designs/TECH_DEBT_RISK_REGISTER.md)  
**Prepared on**: 2026-03-23

## Purpose

This file is a lightweight decision log for the tech-debt remediation program.
Use it to capture decisions that affect scope, sequencing, architecture, risk,
or release criteria.

## Decision Template

```md
## D-###

**Date**: YYYY-MM-DD  
**Status**: Proposed / Decided / Revisited / Superseded  
**Owner**:

### Decision

One-sentence statement of what was decided.

### Context

Why this decision was needed and what issue or risk it addresses.

### Options considered

1. 
2. 
3. 

### Decision rationale

Why this option was chosen over the alternatives.

### Impact

- Affected epics:
- Affected issue refs:
- Delivery impact:
- Risk impact:

### Follow-up actions

1. 
2. 
3. 
```

## Suggested Early Decisions

These are the decisions most likely to unblock the first phases of work.

| ID | Decision topic | Why it matters |
| --- | --- | --- |
| D-001 | Worker control-plane versus execution-token model | Unblocks worker auth and scoped execution |
| D-002 | Shared workspace visibility enforcement design | Prevents surface-by-surface invite-only drift |
| D-003 | False-success handling policy | Aligns backend and UI on when to fail versus warn |
| D-004 | Validation policy for admin and settings APIs | Reduces generic 500s and misconfiguration |
| D-005 | Phase 1 release gate definition | Prevents P0 work from being declared done too early |
| D-006 | CRDT draft identity key | Prevents stale CRDT state from surviving rebases, resets, or branch reuse |
| D-007 | Repo-storage versus workspace-materialization contract | Clarifies where branch and commit correctness should live |
| D-008 | View architecture | Keep views doc-centric for now; defer ontology-native identity as future cleanup |

## Active Decisions

## D-001

**Date**: 2026-03-23  
**Status**: Decided  
**Owner**: Product and backend leads

### Decision

Keep one broad worker token model for now; defer per-task execution tokens to a
future architecture pass.

### Context

The original recommendation was to split worker lifecycle auth from task-scoped
execution auth immediately. That is cleaner long-term, but it is larger than
what is needed for the current remediation phase.

### Options considered

1. Keep one broad worker token for now and fail closed through stricter route,
   workspace, org, and task ownership checks.
2. Split control-plane and execution-plane auth immediately.
3. Keep the current model without additional fail-closed tightening.

### Decision rationale

Option 1 reduces delivery risk and keeps Phase 1 tractable. The future model can
still move to per-task execution tokens later, but current work should focus on
eliminating unscoped fallback and tightening ownership checks.

### Impact

- Affected epics: Worker auth and scoped execution; False-success and execution truthfulness
- Affected issue refs: `§1.46`, `§1.48–§1.51`, `§38.52–§38.53`, `§38.85–§38.88`
- Delivery impact: Medium
- Risk impact: Keeps worker-auth cleanup in current scope while deferring the larger token redesign

### Follow-up actions

1. Remove or quarantine `ScopedDb::unscoped()` worker tenant-data paths
2. Fail closed on internal tenant-data routes with explicit org, workspace, and task ownership checks
3. Record per-task execution tokens as future-state architecture, not current-phase scope

## D-002

**Date**: 2026-03-23  
**Status**: Decided  
**Owner**: Backend platform lead

### Decision

All workspace-scoped reads and writes should converge on one shared workspace
visibility enforcement path instead of ad hoc route checks.

### Context

Invite-only access is currently being repaired surface by surface, which makes
partial fixes fragile and inconsistent.

### Options considered

1. Continue route-by-route checks.
2. Add one shared workspace visibility guard and migrate surfaces onto it.

### Decision rationale

Option 2 is the only reliable way to prevent drift across files, docs, chat,
comments, jobs, tasks, notifications, and settings.

### Impact

- Affected epics: Workspace visibility and tenant isolation
- Affected issue refs: `§38.11–§38.12`, `§38.21–§38.30`, `§38.37–§38.40`, `§38.72`, `§38.89–§38.94`
- Delivery impact: High
- Risk impact: Reduces `R1` directly

### Follow-up actions

1. Design one shared workspace visibility guard
2. Migrate high-value surfaces first
3. Add invite-only regression coverage for each migrated surface

## D-003

**Date**: 2026-03-23  
**Status**: Decided  
**Owner**: Product and backend leads

### Decision

Default to fail-closed, strict APIs; only allow degraded or silent behavior when
there is an explicit reason and the state-machine contract still remains valid.

### Context

Many Phase 2 defects are really contract questions around false success,
retries, and degradation behavior.

### Options considered

1. Keep permissive success semantics and improve messaging.
2. Default to fail-closed and require explicit justification for degraded behavior.

### Decision rationale

The system operates as a state machine, so correctness and explicit state
transitions matter more than convenience-driven success responses.

### Impact

- Affected epics: False-success and execution truthfulness; Config, settings, and validation hygiene
- Affected issue refs: `§1.8b`, `§1.16`, `§1.20`, `§38.17–§38.20`, `§38.31`, `§38.41`, `§38.69–§38.71`, `§38.1174–§38.1183`
- Delivery impact: High
- Risk impact: Reduces `R3` directly

### Follow-up actions

1. Define explicit degraded-response exceptions
2. Audit success paths that currently log and continue
3. Update UI to reflect hard failures or explicit degraded states

## D-004

**Date**: 2026-03-23  
**Status**: Proposed  
**Owner**: Backend platform lead

### Decision

Introduce a scoped bootstrap auth path limited to org and user bootstrap tables
for create-org, invitation, and profile bootstrap flows.

### Context

Removing `ScopedDb::unscoped()` from org and user bootstrap flows likely needs
something narrower than full unscoped access but more permissive than ordinary
tenant-scoped sessions.

### Options considered

1. Keep `ScopedDb::unscoped()` as a special-case escape hatch.
2. Create a bootstrap-scoped auth/session type that only supports the necessary org and user tables.
3. Push all bootstrap behavior into ad hoc route logic on the system pool.

### Decision rationale

Option 2 appears to preserve policy intent better than a permanent unscoped
escape hatch, while still supporting bootstrap flows that happen before normal
membership exists.

### Impact

- Affected epics: Workspace visibility and tenant isolation
- Affected issue refs: `§1.46`, bootstrap auth follow-ups around org and user routes
- Delivery impact: Medium
- Risk impact: Could remove a lingering unscoped public-route exception

### Follow-up actions

1. Design the bootstrap-scoped auth/session boundary
2. Confirm exact table and route scope
3. Replace public-route `unscoped` usage if the design holds up

## D-005

**Date**: 2026-03-23  
**Status**: Decided  
**Owner**: Platform and auth leads

### Decision

Service tokens are not allowed on public routes; public auth should flow through
OAuth and worker registration should use worker registration tokens.

### Context

The earlier design table left open a gated public-route service-token mode.

### Options considered

1. Allow service tokens on selected public routes behind config.
2. Disallow service tokens on public routes entirely.

### Decision rationale

Option 2 matches the current product direction and removes ambiguity from the
public auth model.

### Impact

- Affected epics: Browser trust and secret handling; Worker auth and scoped execution
- Affected issue refs: `§1.2b`, service-token policy decisions in the design table
- Delivery impact: Low
- Risk impact: Reduces public auth ambiguity

### Follow-up actions

1. Remove planning assumptions that depend on public-route service tokens
2. Keep worker registration tokens and OAuth as the only intended public/bootstrap auth paths
3. Document that service tokens are internal-only if they still exist at all

## D-006

**Date**: 2026-03-23  
**Status**: Decided  
**Owner**: Product and platform leads

### Decision

CRDT draft identity should be keyed by `workspace_branch_id + repo_id +
file_path + base_commit_sha`.

### Context

The current branch-wide CRDT key is too coarse. Using only workspace, repo, and
file path is better, but it still lets old CRDT state survive rebases, resets,
or branch reuse if the Git lineage changes underneath an open draft.

### Options considered

1. Keep branch-wide CRDT documents.
2. Use `workspace_id + repo_id + file_path`.
3. Use branch-scoped, file-scoped identity plus the draft base commit SHA.

### Decision rationale

Option 3 keeps drafts isolated by branch, file, and Git lineage. The key point
is that the SHA should represent the draft's base snapshot, not just the
current branch head at some later time.

### Impact

- Affected epics: CRDT, branch identity, and state consistency
- Affected issue refs: `§36.1–§36.3`
- Delivery impact: Medium
- Risk impact: Reduces stale-state and branch-reuse bleed risk

### Follow-up actions

1. Define where `base_commit_sha` is stored and validated
2. Refactor CRDT addressing to use file-scoped identity
3. Add rebase, reset, and branch-reuse regression coverage

## D-007

**Date**: 2026-03-23  
**Status**: Proposed  
**Owner**: Platform lead

### Decision

Keep workspace materialization as the primary execution model, but clarify when
shared repo-storage reads must become branch- or commit-correct.

### Context

The intended architecture already says workers and CLI clients materialize the
workspace locally and then layer CRDT state on top. The remaining ambiguity is
whether shared repo-storage APIs can stay coarse or need an explicit branch or
commit dimension for services like search, docs, and file browsing.

### Options considered

1. Keep shared repo storage coarse and require callers to materialize their own branch-correct state.
2. Add branch- or commit-aware dimensions to shared repo-storage reads where services need authoritative bytes.
3. Shift more workspace-state responsibility back into API-managed persistent checkouts.

### Decision rationale

Option 2 currently looks most likely: preserve workspace materialization as the
main model, but stop pretending repo-level reads are always correct when the
caller really needs branch- or commit-specific content.

### Impact

- Affected epics: CRDT, branch identity, and state consistency
- Affected issue refs: `§35.1–§35.3`, repo-storage follow-up defects
- Delivery impact: Medium
- Risk impact: Reduces protocol drift and branch-state confusion

### Follow-up actions

1. Identify all services that require branch- or commit-correct reads
2. Decide whether the shared repo-storage API grows a branch or commit dimension
3. Align worker materialization, CLI clone or refresh, and API-side consumers with one explicit contract

## D-008

**Date**: 2026-03-23  
**Status**: Decided  
**Owner**: Product and architecture leads

### Decision

Keep views as specialized docs linked to ontology for now; defer ontology-native
view identity as future architecture cleanup rather than current-phase scope.

### Context

Views conceptually operate on ontology structure, but the implemented model is
still doc-centric: `docs` rows plus `doc_node_links` and view-specific job
metadata. That distinction is not currently blocking development, and the
workflow or runtime unification work in PR `#27` addresses most of the
practical execution pain for now.

### Options considered

1. Keep the current doc-centric bridge model and harden it incrementally.
2. Make views first-class ontology nodes and treat document content as a layer on top.
3. Introduce a hybrid transition model that preserves current doc storage while moving orchestration onto ontology-native identifiers.

### Decision rationale

Option 1 is good enough for the current remediation program. The ontology-native
model may still be cleaner long-term, but it is esoteric relative to the more
urgent defects and does not presently justify migration cost or sequencing
attention.

### Impact

- Affected epics: Core workspace and project workflows; long-term ontology and view orchestration cleanup
- Affected issue refs: `§1.54` and multiple view-sync follow-ups in `§38`
- Delivery impact: Low for current phases; deferred architecture work only
- Risk impact: Accepts some long-term model inconsistency, but removes a non-blocking design discussion from the critical path

### Follow-up actions

1. Treat the doc-centric view model as the supported implementation for current phases
2. Revisit ontology-native identity only if `docs` plus `doc_node_links` becomes a real delivery blocker
3. Keep current remediation focus on authz, visibility, fail-closed APIs, repo correctness, and CRDT identity

## Example Entry

## D-001

**Date**: 2026-03-23  
**Status**: Superseded  
**Owner**: Backend platform lead

### Decision

Separate worker lifecycle auth from task-scoped execution auth.

### Context

The current worker path still relies on unscoped fallback authority and blocks
multiple P0 and P1 epics.

### Options considered

1. Keep one worker token model and harden route checks incrementally.
2. Split control-plane and execution-plane auth with scoped execution tokens.
3. Keep current auth model and rely on stronger route-level checks only.

### Decision rationale

Option 2 creates the cleanest long-term boundary and removes the hidden coupling
that currently drives both compromise and false-success risks.

### Impact

- Affected epics: Worker auth and scoped execution; False-success and execution truthfulness
- Affected issue refs: `§1.46`, `§1.48–§1.51`, `§38.52–§38.53`, `§38.85–§38.88`
- Delivery impact: High
- Risk impact: Reduces `R2` materially if implemented early

### Follow-up actions

1. Write execution-token design note
2. Add implementation slices for lifecycle and tenant-data routes
3. Add end-to-end test plan before rollout

**Superseded by**: D-001 above, which keeps the broad worker token for the
current phase and defers per-task execution tokens to future-state design.
