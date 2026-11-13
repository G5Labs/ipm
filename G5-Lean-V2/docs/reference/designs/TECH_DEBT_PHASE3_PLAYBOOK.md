# TECH_DEBT Phase 3 Playbook

**Related docs**: [TECH_DEBT_PRIORITY_REVIEW.md](/Users/lumost/workspace/g5/generation/designs/TECH_DEBT_PRIORITY_REVIEW.md), [TECH_DEBT_EXECUTION_BACKLOG.md](/Users/lumost/workspace/g5/generation/designs/TECH_DEBT_EXECUTION_BACKLOG.md), [TECH_DEBT_ROADMAP.md](/Users/lumost/workspace/g5/generation/designs/TECH_DEBT_ROADMAP.md), [TECH_DEBT_PHASE1_PLAYBOOK.md](/Users/lumost/workspace/g5/generation/designs/TECH_DEBT_PHASE1_PLAYBOOK.md), [TECH_DEBT_PHASE2_PLAYBOOK.md](/Users/lumost/workspace/g5/generation/designs/TECH_DEBT_PHASE2_PLAYBOOK.md)  
**Prepared on**: 2026-03-23

> **Status note (2026-04-19)**: This is a historical phase playbook. For
> current item status, use `TECH_DEBT.md` and `BATCH_PLAN.md`.

## Purpose

This file is a concrete execution guide for **Phase 3: Repair visible broken
workflows**. It is meant to help the team convert the now-safer and more
truthful backend into a product that visibly works in the shipped UI.

## Phase 3 Goal

Repair the most visible workflow failures:

- workspace switching and project creation
- repo onboarding and workspace entry points
- rich-doc editing bootstrap and doc deletion
- chat resume, artifact navigation, and dead visible controls

## Phase 3 Exit Criteria

Phase 3 is done when all of the following are true:

- The shipped UI no longer exposes major visible no-op controls on core flows
- Workspace switching, project creation, repo onboarding, and doc deletion work
- Chat resume behaves like resume, not fork or no-op
- The repaired workflows have regression coverage

## Scope

### In scope

- Core workspace and project workflows epic
- Chat and collaboration product correctness epic
- UI fixes required to expose the now-correct backend behavior

### Out of scope

- Broad validation cleanup that belongs to Phase 4
- Structural refactors that do not unblock user workflows
- New product features

## Recommended Workstreams

| Workstream | Objective | Primary issues |
| --- | --- | --- |
| Workspace and project flows | Make the top-level product shell behave correctly | `§15.1–§15.3`, `§16.1–§16.5`, `§19`, `§22.1–§22.3`, `§38.9`, `§38.95`, `§38.101` |
| Chat and collaboration flows | Make chat and collaboration controls behave like their labels imply | `§38.33`, `§38.45`, `§38.47`, `§38.50–§38.51`, `§38.55`, `§38.80–§38.82`, `§38.115`, `§38.1180`, `§38.1185–§38.1187` |
| Dead-control cleanup | Remove or wire visible controls that currently do nothing | `§38.34–§38.35`, `§38.47`, `§38.50–§38.51`, `§38.60–§38.61`, `§38.99`, `§38.101` |

## First 30 Days

### Week 1

- Fix workspace switcher behavior end to end
- Fix create-project navigation and empty-org edge cases
- Remove or wire the highest-traffic visible no-op controls

Expected result:

- Users can change projects from the main shell without silent failure
- The product stops advertising at least a few obviously broken controls

### Week 2

- Fix repo onboarding entry points and browser-completable SCM flows
- Restore editable rich-doc bootstrap for new docs
- Implement doc deletion path end to end

Expected result:

- Core workspace creation and content-management paths work from the product

### Week 3

- Implement true chat resume behavior
- Fix artifact and citation navigation
- Fix session and message pagination or truncation on the highest-traffic chat surfaces

Expected result:

- Chat controls behave more like users expect
- Long-running sessions no longer feel obviously broken

### Week 4

- Sweep remaining dead controls
- Fill regression gaps
- Recount visible no-op flows and evaluate Gate C

Expected result:

- The team can make an honest call on whether the core product is now usable

## Ticket Order

Use this order unless support or product incidents change the priority:

1. `§16.1–§16.5`
2. `§38.9`
3. `§15.1–§15.3`
4. `§38.95`
5. `§19`
6. `§22.1–§22.3`
7. `§38.33`
8. `§38.50–§38.51`
9. `§38.80–§38.82`
10. `§38.34–§38.35`, `§38.47`, `§38.60–§38.61`, `§38.99`, `§38.101`

## Suggested Owner Split

| Owner type | Primary focus |
| --- | --- |
| Product engineer | Main workflow wiring, UI flows, and user-facing error states |
| Backend engineer | API fixes needed to support the repaired workflows cleanly |
| Frontend engineer | Chat, navigation, and dead-control cleanup |

## Required Tests

Phase 3 should not be declared done without:

- workspace switcher workflow coverage
- project creation workflow coverage
- repo onboarding path coverage
- new-doc editable bootstrap coverage
- doc deletion coverage
- chat resume coverage
- artifact or citation navigation coverage

## Review Checklist

Use this checklist at the end of each Phase 3 sprint:

- [ ] Did we fix a real user-visible workflow, not just the label or styling?
- [ ] Did we remove or wire visible no-op controls?
- [ ] Did we verify the workflow from the shipped UI, not just the API?
- [ ] Did we add regression coverage for the repaired path?
- [ ] Did we avoid introducing new misleading fallbacks while fixing the flow?

## Common Failure Modes

| Failure mode | Warning sign | Response |
| --- | --- | --- |
| UI wiring is fixed but backend contract is still unstable | The same workflow re-breaks after backend changes | Stop and confirm the runtime contract is actually stable enough |
| Dead controls remain because they are “low effort to ignore” | The app still shows actions that do nothing | Remove or hide the control until it is real |
| Resume-like flows are fixed by changing copy only | The button label changes but behavior still forks or no-ops | Reopen the issue and verify actual mutation behavior |
| Pagination bugs are tolerated as “performance work” | Lists still silently stop after page one | Keep it in Phase 3 if it breaks everyday product use |

## Handoff To Phase 4

Do not move to Phase 4 just because some broken buttons were fixed. Move only
when the team can show:

- the highest-traffic workflows are usable from the shipped UI
- visible no-op controls on core surfaces are gone or real
- the repaired paths have regression coverage

At that point, the team can shift toward validation and UX-quality tightening
without leaving the product obviously broken.
