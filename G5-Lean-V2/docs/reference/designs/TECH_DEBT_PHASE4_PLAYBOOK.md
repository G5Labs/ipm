# TECH_DEBT Phase 4 Playbook

**Related docs**: [TECH_DEBT_PRIORITY_REVIEW.md](/Users/lumost/workspace/g5/generation/designs/TECH_DEBT_PRIORITY_REVIEW.md), [TECH_DEBT_EXECUTION_BACKLOG.md](/Users/lumost/workspace/g5/generation/designs/TECH_DEBT_EXECUTION_BACKLOG.md), [TECH_DEBT_ROADMAP.md](/Users/lumost/workspace/g5/generation/designs/TECH_DEBT_ROADMAP.md), [TECH_DEBT_PHASE3_PLAYBOOK.md](/Users/lumost/workspace/g5/generation/designs/TECH_DEBT_PHASE3_PLAYBOOK.md)  
**Prepared on**: 2026-03-23

> **Status note (2026-04-19)**: This is a historical phase playbook. For
> current item status, use `TECH_DEBT.md` and `BATCH_PLAN.md`.

## Purpose

This file is a concrete execution guide for **Phase 4: Tighten admin and UX
quality**. It is meant to help the team turn a safe, truthful, and basically
usable product into one that behaves consistently for operators and end users.

## Phase 4 Goal

Improve the quality of admin and UX-facing behavior:

- explicit validation instead of generic internal errors
- no-op and empty-body mutations handled clearly
- cleaner fallback behavior in the UI
- better not-found, label, and accessibility behavior

## Phase 4 Exit Criteria

Phase 4 is done when all of the following are true:

- High-risk settings and config mutations reject invalid input explicitly
- Empty or no-op writes no longer present as successful meaningful updates
- The UI no longer silently substitutes the wrong org or hides invalid states
- High-traffic accessibility and fallback issues have regression coverage

## Scope

### In scope

- Config, settings, and validation hygiene epic
- UI trustworthiness and fallback cleanup epic
- Selected UX fixes that reduce support burden and operator confusion

### Out of scope

- Major new workflow work that belongs in Phase 3
- Broad decomposition or maintainability refactors that belong in Phase 5
- Net-new product features

## Recommended Workstreams

| Workstream | Objective | Primary issues |
| --- | --- | --- |
| Validation hygiene | Replace generic 500s and silent coercion with explicit validation | `§9.4`, `§30`, `§38.96–§38.98`, `§38.1121–§38.1123`, `§38.1159–§38.1166`, `§38.1173–§38.1183`, `§38.1188–§38.1194` |
| UI fallback cleanup | Make stale or invalid states visible instead of silently redirected or misrouted | `§1.39–§1.40`, `§38.36`, `§38.49`, `§38.100`, `§38.1106–§38.1110` |
| Accessibility and structural UX trust | Improve keyboard, labeling, and hierarchy behavior on core UI surfaces | `§10`, `§23.1–§23.4` |

## First 30 Days

### Week 1

- Fix the highest-value validation gaps on settings and config routes
- Replace the most misleading empty-body or no-op mutation behavior
- Define shared API validation expectations for admin-facing mutations

Expected result:

- Operators stop seeing generic internal errors for common bad input
- The team has one consistent expectation for what constitutes a valid mutation

### Week 2

- Fix wrong-org and stale-slug fallback behavior in the UI
- Add clear not-found or invalid-state behavior where silent substitution exists
- Start accessibility fixes on high-traffic controls

Expected result:

- The UI stops quietly operating on the wrong org or resource
- Users get clearer feedback when a route or slug is invalid

### Week 3

- Sweep remaining token, settings, and config validation gaps
- Complete the highest-priority accessibility work
- Add contract tests for bad input and fallback cases

Expected result:

- Admin and settings surfaces are materially more predictable
- UX cleanup is supported by real regression coverage

### Week 4

- Recount validation-related generic 500s
- Sweep remaining Phase 4 blockers
- Evaluate whether the product is ready to shift toward Phase 5 maintainability work

Expected result:

- The team can make an evidence-based decision on whether Phase 4 actually improved quality

## Ticket Order

Use this order unless support or product feedback changes the priority:

1. `§9.4`
2. `§38.1173–§38.1183`
3. `§38.1188–§38.1194`
4. `§38.1159–§38.1166`
5. `§38.96–§38.98`
6. `§38.1121–§38.1123`
7. `§38.49`
8. `§38.100`
9. `§38.1106–§38.1110`
10. `§1.39–§1.40`
11. `§10`
12. `§23.1–§23.4`

## Suggested Owner Split

| Owner type | Primary focus |
| --- | --- |
| Backend API team | Validation rules, empty-write behavior, and admin-surface contract quality |
| Frontend team | Wrong-state fallbacks, not-found behavior, labels, accessibility, and stale-state UX |
| Product engineer | End-to-end fixes where backend validation changes require UI behavior updates |

## Required Tests

Phase 4 should not be declared done without:

- request-validation coverage for high-risk settings routes
- request-validation coverage for config mutation routes
- coverage for empty-body or no-op mutation handling
- wrong-org or stale-slug UI coverage
- not-found-state coverage for invalid routes or resources
- accessibility coverage for the highest-traffic corrected controls

## Review Checklist

Use this checklist at the end of each Phase 4 sprint:

- [ ] Did we replace a confusing or misleading behavior with an explicit contract?
- [ ] Did we reduce generic 500s caused by ordinary bad input?
- [ ] Did we verify the UI no longer silently falls back to the wrong state?
- [ ] Did we add tests for the corrected validation or UX behavior?
- [ ] Did we avoid pulling large maintainability refactors into a quality-focused phase?

## Common Failure Modes

| Failure mode | Warning sign | Response |
| --- | --- | --- |
| Validation logic stays fragmented | Similar routes still handle invalid input differently | Stop and define shared validation rules before continuing |
| UI fallback fixes mask bad backend state instead of surfacing it | The app looks smoother but still uses the wrong resource or org | Revisit the contract and make the invalid state explicit |
| Accessibility work gets deferred as “polish” | Keyboard and labeling issues remain on high-traffic flows | Keep high-traffic accessibility issues in Phase 4 scope |
| Empty-write behavior is “fixed” by changing copy only | The mutation still updates audit fields with no real change | Reopen the backend mutation semantics |

## Handoff To Phase 5

Do not move to Phase 5 just because the number of visible bugs is smaller. Move
only when the team can show:

- high-risk admin surfaces validate input explicitly
- the UI handles invalid and stale states clearly
- the highest-traffic accessibility and fallback issues are covered

At that point, Phase 5 can focus on maintainability without leaving obvious
quality debt on operator and UX-critical surfaces.
