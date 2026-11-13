# TECH_DEBT Phase 2 Playbook

**Related docs**: [TECH_DEBT_PRIORITY_REVIEW.md](/Users/lumost/workspace/g5/generation/designs/TECH_DEBT_PRIORITY_REVIEW.md), [TECH_DEBT_EXECUTION_BACKLOG.md](/Users/lumost/workspace/g5/generation/designs/TECH_DEBT_EXECUTION_BACKLOG.md), [TECH_DEBT_ROADMAP.md](/Users/lumost/workspace/g5/generation/designs/TECH_DEBT_ROADMAP.md), [TECH_DEBT_RISK_REGISTER.md](/Users/lumost/workspace/g5/generation/designs/TECH_DEBT_RISK_REGISTER.md), [TECH_DEBT_PHASE1_PLAYBOOK.md](/Users/lumost/workspace/g5/generation/designs/TECH_DEBT_PHASE1_PLAYBOOK.md)  
**Prepared on**: 2026-03-23

> **Status note (2026-04-19)**: This is a historical phase playbook. For
> current item status, use `TECH_DEBT.md` and `BATCH_PLAN.md`.

## Purpose

This file is a concrete execution guide for **Phase 2: Make runtime behavior
truthful**. It is meant to convert the roadmap into a practical working plan
once the highest-risk compromise paths are under control.

## Phase 2 Goal

Remove the most damaging runtime correctness failures:

- false-success responses
- hidden delivery and lifecycle failures
- stale or inconsistent branch and draft state
- severe load-amplification and state-corruption paths

## Phase 2 Exit Criteria

Phase 2 is done when all of the following are true:

- Core mutation paths no longer claim success when work did not happen
- Delivery, reindex, resume, and lifecycle failures surface explicitly
- Branch, draft, and sync identity are materially more trustworthy
- High-risk runtime fixes have regression coverage

## Scope

### In scope

- False-success and execution truthfulness epic
- CRDT, branch identity, and state consistency epic
- High-severity performance or state-coupling defects that block truthful behavior

### Out of scope

- New feature work
- Broad UI polish not required to expose correct runtime state
- Large maintainability refactors unrelated to runtime correctness

## Recommended Workstreams

| Workstream | Objective | Primary issues |
| --- | --- | --- |
| False-success removal | Make success mean real execution or real persistence | `§1.8b`, `§1.16`, `§1.20`, `§38.17–§38.20`, `§38.31`, `§38.41`, `§38.69–§38.71`, `§38.1167`, `§38.1174–§38.1183` |
| Branch and draft correctness | Make branch-aware reads and draft identity consistent | `§35.2–§35.3`, `§36.1–§36.2`, `§38.3`, `§38.74–§38.78` |
| Sync durability | Make reconnect, flush, and materialization behavior reliable | `§2b.*`, `§38.949–§38.991` |

## First 30 Days

### Week 1

- Audit the highest-impact false-success paths
- Choose the failure-handling policy for “log and continue” routes
- Fix one end-to-end user-visible false-success path first
- Define the branch-correct storage and draft-identity direction

Expected result:

- The team agrees on what should fail hard versus warn versus retry
- At least one visible success-without-work path is eliminated

### Week 2

- Fix chat delivery and related lifecycle truthfulness paths
- Fix spend-history and settings false-success or silent-coercion cases
- Start branch-correct read-path work for repo-backed content

Expected result:

- Core user-facing execution paths are becoming honest
- Configuration and reporting paths stop lying about what happened

### Week 3

- Fix CRDT identity and branch-state assumptions
- Harden outbound sync flush and reconnect behavior
- Add regression coverage for branch and draft state transitions

Expected result:

- Draft and branch state are less likely to drift or cross-contaminate
- Sync durability improves under reconnect and shutdown conditions

### Week 4

- Sweep remaining high-severity false-success paths
- Recount runtime-truthfulness issues
- Evaluate Gate B and decide whether Phase 3 can start

Expected result:

- The team can make an honest call on whether the runtime is trustworthy enough
  to justify user-facing workflow repair

## Ticket Order

Use this order unless incident data changes the risk picture:

1. `§38.17`
2. `§38.18`
3. `§38.31`
4. `§38.41`
5. `§1.8b`
6. `§1.16`
7. `§1.20`
8. `§36.1–§36.2`
9. `§38.74–§38.78`
10. `§2b.*`
11. `§38.949–§38.991`
12. `§38.1167`, `§38.1174–§38.1183`

## Suggested Owner Split

| Owner type | Primary focus |
| --- | --- |
| Backend runtime | False-success removal, lifecycle semantics, sync durability |
| Backend platform | Branch-correct storage access, draft identity, shared correctness primitives |
| Product engineer | UI follow-through where a formerly false-success path now surfaces real failure states |

## Required Tests

Phase 2 should not be declared done without:

- chat delivery failure coverage
- task or lifecycle false-success coverage
- branch-correct doc or file read coverage
- draft identity or branch-isolation coverage
- reconnect and outbound flush durability coverage
- regression coverage for the highest-risk spend or settings reporting paths

## Review Checklist

Use this checklist at the end of each Phase 2 sprint:

- [ ] Did we remove a real false-success path, not just rename the status?
- [ ] Did we make a previously hidden failure explicit to callers?
- [ ] Did we add coverage for the corrected execution path?
- [ ] Did we reduce branch or draft ambiguity rather than adding another fallback?
- [ ] Did we avoid reopening P0 compromise risk while changing runtime behavior?

## Common Failure Modes

| Failure mode | Warning sign | Response |
| --- | --- | --- |
| Success paths still swallow downstream failures | Logs show error followed by `200` or success UI state | Convert the path to explicit failure or visible degraded status |
| Branch correctness is patched case by case | More call sites add ad hoc branch arguments or fallbacks | Stop and define a shared branch-correct storage contract |
| Sync fixes focus on logs instead of guarantees | More logging appears but reconnect loss still exists | Add delivery guarantees and test them end to end |
| UI keeps assuming old success semantics | Product still shows “done” when backend now returns partial or failed states | Pair backend truthfulness changes with explicit UI-state updates |

## Handoff To Phase 3

Do not move to Phase 3 just because a few false-success paths are fixed. Move
only when the team can show:

- major execution paths are no longer falsely successful
- branch and draft state are trustworthy enough for user-facing workflows
- the corrected behavior is covered by regression tests

At that point, user-facing workflow repair is much more likely to stick.
