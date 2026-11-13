# TECH_DEBT Execution Backlog

**Source review**: [TECH_DEBT_PRIORITY_REVIEW.md](/Users/lumost/workspace/g5/generation/designs/TECH_DEBT_PRIORITY_REVIEW.md)  
**Source inventory**: [TECH_DEBT.md](/Users/lumost/workspace/g5/generation/designs/TECH_DEBT.md)  
**Prepared on**: 2026-03-23

## Purpose

This file converts the priority review into a flat execution backlog that can
be used for roadmap planning, milestone definition, or epic creation.

## How To Read This

- `Wave` is the recommended sequence
- `Priority` reflects urgency relative to the rest of the backlog
- `Size` is a rough planning estimate, not a commitment
- `Issue refs` point back to the detailed inventory in `TECH_DEBT.md`
- Each row is intended to be split into multiple implementation tickets

## Flat Backlog

| Rank | Epic | Wave | Priority | Suggested owner | Size | Depends on | Issue refs | Done criteria |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 1 | Workspace visibility and tenant isolation | Wave 0 | P0 | Backend platform or authz lead | L | None | `§38.11–§38.12`, `§38.21–§38.30`, `§38.37–§38.40`, `§38.72`, `§38.89–§38.94` | Invite-only and tenant boundaries are enforced consistently across files, docs, comments, chat, jobs, tasks, notifications, settings, and permissions |
| 2 | Worker auth and scoped execution | Wave 0 | P0 | Backend platform lead | XL | None | `§1.46`, `§1.48–§1.51`, `§1.55`, `§38.52–§38.53`, `§38.85–§38.88` | Workers keep the current broad token model for now, but tenant-data routes fail closed through explicit org, workspace, and task ownership checks and no unscoped fallback remains |
| 3 | Browser trust and secret handling | Wave 0 | P0 | Security or frontend platform | M | None | `§1.33`, `§1.34`, `§1.49`, `§38.66`, `§38.94`, `§38.110` | No stored XSS path remains and sensitive secrets or tokens are not exposed or stored unsafely in browser-accessible surfaces |
| 4 | False-success and execution truthfulness | Wave 1 | P1 | Backend runtime lead | XL | Worker auth and scoped execution | `§1.8b`, `§1.16`, `§1.20`, `§38.17–§38.20`, `§38.31`, `§38.41`, `§38.69–§38.71`, `§38.1167`, `§38.1174–§38.1183` | APIs and UI do not report success unless work was actually delivered, persisted, resumed, or enforced |
| 5 | CRDT, branch identity, and state consistency | Wave 1 | P1 | Backend runtime lead | XL | False-success and execution truthfulness | `§2b.*`, `§35.2–§35.3`, `§36.1–§36.2`, `§38.3`, `§38.74–§38.78`, `§38.949–§38.991` | Draft, branch, and sync state remain consistent under reconnects, branch changes, and concurrent edits |
| 6 | Core workspace and project workflows | Wave 2 | P1 | Product engineering | L | False-success and execution truthfulness | `§15.1–§15.3`, `§16.1–§16.5`, `§19`, `§22.1–§22.3`, `§38.9`, `§38.95`, `§38.101` | Workspace switching, project creation, repo onboarding, doc editing, and doc deletion all work from the shipped UI |
| 7 | Chat and collaboration product correctness | Wave 2 | P1 | Product engineering | L | Workspace visibility and tenant isolation | `§38.33`, `§38.45`, `§38.47`, `§38.50–§38.51`, `§38.55`, `§38.80–§38.82`, `§38.115`, `§38.1180`, `§38.1185–§38.1187` | Resume, pagination, session ordering, notification read state, artifact navigation, and session naming all behave as users expect |
| 8 | Config, settings, and validation hygiene | Wave 3 | P2 | Backend API team | M | Browser trust and secret handling | `§9.4`, `§30`, `§38.96–§38.98`, `§38.1121–§38.1123`, `§38.1159–§38.1166`, `§38.1173–§38.1183`, `§38.1188–§38.1194` | Invalid input produces explicit validation errors, empty writes are rejected or no-op safely, and admin settings cannot silently misconfigure runtime behavior |
| 9 | UI trustworthiness and fallback cleanup | Wave 3 | P2 | Frontend team | M | Browser trust and secret handling | `§1.39–§1.40`, `§10`, `§23.1–§23.4`, `§38.36`, `§38.49`, `§38.100`, `§38.1106–§38.1110` | The UI preserves context, shows clear fallbacks, and does not silently misroute, mislabel, or hide invalid states |
| 10 | Long-tail maintainability | Wave 4 | P3 | Any team after stabilization | XL | Most earlier epics | `§3.*`, `§4.*`, `§5.*`, `§7.*`, `§13`, `§26`, `§34.2`, `§37.4–§37.6` | Code structure, DTOs, and placeholder cleanup reduce future regressions without reopening solved product or security risks |

## Suggested First Tickets

| Epic | First tickets to create |
| --- | --- |
| Workspace visibility and tenant isolation | Add shared workspace-visibility guard; enforce it on files, docs, comments, chat, jobs, tasks, notifications, and settings |
| Worker auth and scoped execution | Remove `ScopedDb::unscoped()` worker path where possible; add explicit org/workspace/task ownership checks; add end-to-end auth tests; defer per-task execution tokens to future-state design |
| Browser trust and secret handling | Sanitize search result HTML; add markdown URL allowlist; remove or rotate unsafe token storage; clean committed secret artifacts |
| False-success and execution truthfulness | Audit endpoints that swallow delivery or lifecycle failures; convert false-success to explicit failures; add regression tests |
| CRDT, branch identity, and state consistency | Introduce branch-correct storage reads; fix draft identity semantics; harden outbound and inbound sync guarantees |
| Core workspace and project workflows | Wire workspace switcher callbacks; fix create-project navigation; implement doc delete flow; restore editable rich-doc bootstrap |
| Chat and collaboration product correctness | Implement true resume action; fix artifact navigation; fix notification read-state contract; fix first-page truncation paths |
| Config, settings, and validation hygiene | Add request validation for settings and config mutations; reject empty writes; turn DB constraint failures into user-visible validation errors |
| UI trustworthiness and fallback cleanup | Fix wrong-org fallback, invalid slug handling, accessibility gaps, and missing management surfaces |
| Long-tail maintainability | Split oversized frontend files; remove dead code annotations after verification; clean up duplicated DTO and type shapes |

## Milestone View

| Milestone | Includes epics |
| --- | --- |
| M1 | Workspace visibility and tenant isolation; Worker auth and scoped execution; Browser trust and secret handling |
| M2 | False-success and execution truthfulness; CRDT, branch identity, and state consistency |
| M3 | Core workspace and project workflows; Chat and collaboration product correctness |
| M4 | Config, settings, and validation hygiene; UI trustworthiness and fallback cleanup |
| M5 | Long-tail maintainability |

## Release Gate Checklist

- [ ] No known cross-tenant read path remains
- [ ] No known cross-tenant write path remains
- [ ] No known stored XSS path remains
- [ ] No worker tenant-data path relies on unscoped fallback authority
- [ ] No core user workflow advertised in the UI is a no-op
- [ ] No major API path reports success when work was not actually carried out
- [ ] High-risk settings and config mutations return validation errors instead of generic internal failures
- [ ] P0 and P1 epics each have regression coverage before being marked complete
