# TECH_DEBT Risk Register

**Source review**: [TECH_DEBT_PRIORITY_REVIEW.md](/Users/lumost/workspace/g5/generation/designs/TECH_DEBT_PRIORITY_REVIEW.md)  
**Source roadmap**: [TECH_DEBT_ROADMAP.md](/Users/lumost/workspace/g5/generation/designs/TECH_DEBT_ROADMAP.md)  
**Prepared on**: 2026-03-23

## Purpose

This file captures the main delivery and product risks implied by the current
`TECH_DEBT.md` inventory so the remediation program can be managed explicitly,
not just prioritized.

## Scale

| Field | Meaning |
| --- | --- |
| Severity | How bad the outcome is if the risk lands |
| Likelihood | How likely the risk is to materialize without intervention |
| Exposure | Combined practical priority for program management |

## Top Risks

| ID | Risk | Severity | Likelihood | Exposure | Leading signals | Primary mitigation | Suggested owner |
| --- | --- | --- | --- | --- | --- | --- | --- |
| R1 | Cross-tenant or invite-only exposure remains after partial fixes | Critical | High | Very High | Surface-by-surface fixes land without one shared guard; new invite-only regressions appear in adjacent routes | Use one shared workspace-visibility enforcement path and migrate all affected surfaces systematically | Backend platform or authz lead |
| R2 | Worker auth cleanup stalls and leaves broad worker authority under-specified | Critical | Medium | Very High | `ScopedDb::unscoped()` remains in worker paths; org, workspace, or task ownership checks stay incomplete | Keep the broad token model for now, but remove unscoped fallback and make tenant-data checks fail closed | Backend platform lead |
| R3 | False-success behavior keeps shipping because it is treated as UX, not correctness | High | High | Very High | Routes continue to log-and-ignore failures; UI still shows success while no work occurs | Treat false-success as production correctness bugs and require explicit failure contracts | Backend runtime lead |
| R4 | Browser trust issues are underestimated because they look frontend-local | Critical | Medium | Very High | XSS fixes are deferred behind larger backend work; risky token storage remains | Close stored XSS and token-storage issues in Wave 0 with security review | Security or frontend platform |
| R5 | CRDT and branch-identity fixes are delayed until after visible workflow work | High | Medium | High | Workflow fixes land on top of unstable draft or branch assumptions | Sequence runtime truthfulness and state consistency before collaboration polish | Backend runtime lead |
| R6 | Product teams repair visible controls before backend truthfulness is fixed, causing rework | Medium | High | High | No-op controls are wired to unstable backend paths; follow-up bugs reopen the same flows | Make Phase 2 exit a prerequisite for major workflow cleanup | Product engineering lead |
| R7 | Validation and admin-surface cleanup gets postponed, leaving operators with generic 500s and silent misconfiguration | Medium | Medium | Medium | Settings bugs recur; support incidents stem from invalid input or hidden coercion | Add validation hygiene as a dedicated Phase 4 epic, not an opportunistic cleanup | Backend API team |
| R8 | Structural cleanup starts too early and consumes capacity needed for release-blocking work | Medium | Medium | Medium | Large decomposition tasks begin while P0 counts stay flat | Keep maintainability work mostly behind P0 and P1 gates | Engineering manager or tech lead |
| R9 | Regression coverage lags behind fixes, so solved defects reappear | High | Medium | High | P0 or P1 issues close without new integration or E2E coverage | Require regression coverage as part of done criteria for high-priority epics | All epic owners |
| R10 | The team tracks section counts instead of actual failure modes and spreads effort too thinly | Medium | High | High | Work is distributed evenly across sections instead of concentrated on compromise and runtime truthfulness | Use failure-mode-based prioritization in sprint planning and status reviews | Program lead |

## Risk Watchlist By Phase

| Phase | Main risks to watch |
| --- | --- |
| Phase 1 | `R1`, `R2`, `R4`, `R9` |
| Phase 2 | `R2`, `R3`, `R5`, `R9` |
| Phase 3 | `R3`, `R5`, `R6`, `R9` |
| Phase 4 | `R7`, `R9`, `R10` |
| Phase 5 | `R8`, `R9`, `R10` |

## Escalation Triggers

These are the signals that should force a planning reset or leadership review.

| Trigger | Why it matters | Suggested response |
| --- | --- | --- |
| A new cross-tenant or invite-only bypass is found during Phase 2 or later | Means the isolation model is still inconsistent | Pause lower-priority work and return to the visibility epic |
| Worker auth redesign is not agreed by the end of the first implementation sprint | Multiple downstream epics depend on it | Escalate design review and narrow scope to minimum viable execution auth |
| A P0 or P1 fix lands without regression coverage | Reopens high-severity risk immediately | Do not mark the issue complete until coverage exists |
| Visible no-op controls remain in production after Phase 3 begins | Signals product-blocking work is lagging behind roadmap | Reprioritize the sprint toward user-visible blockers |
| Generic 500s from invalid admin input persist into Phase 4 | Means validation hygiene is still not treated as product quality work | Pull config and validation tickets forward and add API contract tests |

## Suggested Review Cadence

| Cadence | What to review |
| --- | --- |
| Weekly | Open P0 items, new high-risk regressions, worker-auth progress, regression coverage status |
| Phase boundary | Release gates, unresolved high-exposure risks, whether assumptions still hold |
| Incident review | Any production bug that maps back to a known `TECH_DEBT` issue should feed directly into backlog reprioritization |

## Recommended Risk Posture

1. Treat `R1`, `R2`, `R3`, and `R4` as stop-the-line risks.
2. Do not allow P3 maintainability work to displace mitigation for `R1` through
   `R5`.
3. Prefer shipping fewer fixes with strong regression coverage over more fixes
   with weak verification on P0 and P1 work.
4. Re-evaluate the roadmap if the team cannot materially reduce P0 exposure
   within the first two sprints.
