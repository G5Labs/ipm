# TECH_DEBT Phase 1 Playbook

**Related docs**: [TECH_DEBT_PRIORITY_REVIEW.md](/Users/lumost/workspace/g5/generation/designs/TECH_DEBT_PRIORITY_REVIEW.md), [TECH_DEBT_EXECUTION_BACKLOG.md](/Users/lumost/workspace/g5/generation/designs/TECH_DEBT_EXECUTION_BACKLOG.md), [TECH_DEBT_ROADMAP.md](/Users/lumost/workspace/g5/generation/designs/TECH_DEBT_ROADMAP.md), [TECH_DEBT_RISK_REGISTER.md](/Users/lumost/workspace/g5/generation/designs/TECH_DEBT_RISK_REGISTER.md)  
**Prepared on**: 2026-03-23

> **Status note (2026-04-19)**: This is a historical phase playbook. For
> current item status, use `TECH_DEBT.md` and `BATCH_PLAN.md`.

## Purpose

This file is a concrete execution guide for **Phase 1: Remove compromise
paths**. It is meant to help the team start immediately without re-deriving the
first month of work from the higher-level roadmap.

## Phase 1 Goal

Remove the most credible compromise paths:

- cross-tenant and invite-only exposure
- worker unscoped tenant-data access
- browser content-execution risk
- unsafe secret or token handling

## Phase 1 Exit Criteria

Phase 1 is done when all of the following are true:

- No known cross-tenant read path remains on high-value product surfaces
- No known cross-tenant write path remains on high-value product surfaces
- No known stored XSS path remains
- No worker tenant-data path relies on unscoped fallback authority
- The new protections have regression coverage

## Scope

### In scope

- Workspace visibility and tenant isolation epic
- Worker auth and scoped execution epic, Phase 1 slices
- Browser trust and secret handling epic

### Out of scope

- Broad CRDT correctness cleanup
- Workflow polish that depends on runtime truthfulness
- Large maintainability refactors

## Recommended Workstreams

| Workstream | Objective | Primary issues |
| --- | --- | --- |
| Visibility enforcement | Make invite-only and tenant checks consistent everywhere | `§38.11–§38.12`, `§38.21–§38.30`, `§38.37–§38.40`, `§38.72`, `§38.89–§38.94` |
| Worker authority reduction | Keep the broad worker token model for now, but stop tenant-data access from flowing through unscoped fallback and weak ownership checks | `§1.46`, `§1.48–§1.51`, `§38.52–§38.53`, `§38.85–§38.88` |
| Browser trust hardening | Remove in-browser content execution and obvious secret exposure risks | `§1.33`, `§1.34`, `§1.49`, `§38.66`, `§38.94`, `§38.110` |

## First 30 Days

### Week 1

- Close `§1.33` and `§1.34`
- Record the decided interim worker-auth direction in the decision log
- Define shared workspace-visibility enforcement path
- Add or outline regression strategy for invite-only and worker auth changes

Expected result:

- Stored XSS path is either fixed or in final review
- Core design choices for the highest-risk backend work are recorded and no longer ambiguous

### Week 2

- Apply shared visibility enforcement to files and docs first
- Remove or quarantine the most dangerous unscoped worker-service paths
- Audit committed or exposed secret artifacts and complete required cleanup

Expected result:

- Two high-value data surfaces are tenant-safe
- Worker auth work is no longer blocked by design ambiguity

### Week 3

- Extend visibility enforcement to chat, comments, jobs, and tasks
- Bind internal worker-sensitive mutation routes to task or workspace ownership
- Replace unsafe browser token-storage behavior if still present

Expected result:

- Most high-value user and task surfaces share the same access model
- Worker authority is meaningfully narrower than at the start of the phase

### Week 4

- Sweep remaining notification, settings, and permission surfaces
- Fill regression gaps
- Recount remaining P0 items and evaluate Gate A

Expected result:

- The team can make an honest call on whether Phase 1 is actually complete

## Ticket Order

Use this order unless new evidence changes the risk picture:

1. `§1.33`
2. `§1.34`
3. `§1.46`
4. `§1.48`
5. `§1.51`
6. `§38.11–§38.12`
7. `§38.21–§38.30`
8. `§38.37–§38.40`
9. `§38.72`
10. `§1.49`, `§38.66`, `§38.94`, `§38.110`

## Suggested Owner Split

| Owner type | Primary focus |
| --- | --- |
| Backend platform | Visibility enforcement and interim worker authority model |
| Backend runtime | Worker-sensitive route binding, org or workspace checks, and regression coverage |
| Frontend or security-minded engineer | Browser trust and token handling fixes |
| Product engineer | Frontend follow-through where visibility or auth changes require UI contract updates |

## Required Tests

Phase 1 should not be declared done without:

- invite-only read rejection coverage for files
- invite-only read rejection coverage for docs
- invite-only read and mutation rejection coverage for chat
- invite-only read and mutation rejection coverage for comments
- task or worker ownership coverage for internal mutation routes
- XSS regression coverage for search rendering and markdown links

## Review Checklist

Use this checklist at the end of each Phase 1 sprint:

- [ ] Did we eliminate a real compromise path, not just move it?
- [ ] Did we add regression coverage for the fixed path?
- [ ] Did we use one shared enforcement model where possible?
- [ ] Did we avoid opening new visible workflow regressions while fixing authz?
- [ ] Did we record any architecture or sequencing decisions that changed?

## Common Failure Modes

| Failure mode | Warning sign | Response |
| --- | --- | --- |
| Fixes land surface-by-surface with different rules | Similar routes still use different guards | Stop and consolidate enforcement before continuing |
| Worker auth design remains open too long | Multiple tickets stay blocked on execution-token semantics | Escalate design decision immediately |
| UI work starts outrunning backend safety work | Product controls are being polished on still-unsafe routes | Reprioritize back to Phase 1 exit criteria |
| Security fixes land without regression tests | Review says “fixed” but no coverage was added | Reopen before calling the issue done |

## Handoff To Phase 2

Do not move to Phase 2 just because the XSS fixes are done. Move only when the
team can show:

- consistent high-value workspace visibility enforcement
- a real worker auth boundary for tenant-data paths
- regression coverage for the fixed compromise surfaces

At that point, Phase 2 can focus on runtime truthfulness without carrying the
same unresolved compromise risk.
