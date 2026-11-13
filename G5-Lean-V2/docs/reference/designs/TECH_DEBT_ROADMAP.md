# TECH_DEBT Roadmap

**Source review**: [TECH_DEBT_PRIORITY_REVIEW.md](/Users/lumost/workspace/g5/generation/designs/TECH_DEBT_PRIORITY_REVIEW.md)  
**Source backlog**: [TECH_DEBT_EXECUTION_BACKLOG.md](/Users/lumost/workspace/g5/generation/designs/TECH_DEBT_EXECUTION_BACKLOG.md)  
**Prepared on**: 2026-03-23

## Purpose

This file turns the priority review and execution backlog into a practical
delivery roadmap.

## Planning Assumptions

These are assumptions, not facts about current team capacity:

- One backend platform lead is available for authz and execution-path work
- One backend runtime engineer is available for sync, state, and lifecycle work
- One product engineer can cover frontend and API workflow fixes
- Security review is available part-time during Wave 0
- P0 work takes precedence over feature development

## Recommended Delivery Order

| Phase | Goal | Primary epics | Why first |
| --- | --- | --- | --- |
| Phase 1 | Remove compromise paths | Workspace visibility and tenant isolation; Worker auth and scoped execution; Browser trust and secret handling | The product should not ship while tenant boundaries, worker authority, or browser trust are known-broken |
| Phase 2 | Make runtime behavior truthful | False-success and execution truthfulness; CRDT, branch identity, and state consistency | Users and operators need the system to behave predictably before workflow polish is meaningful |
| Phase 3 | Repair visible broken workflows | Core workspace and project workflows; Chat and collaboration product correctness | Once the backend is safe and truthful, user-facing repairs will stick |
| Phase 4 | Tighten admin and UX quality | Config, settings, and validation hygiene; UI trustworthiness and fallback cleanup | This reduces support cost and misconfiguration after the highest-risk work is done |
| Phase 5 | Pay down structural debt | Long-tail maintainability | This should happen after the system is stable enough that cleanup will not be invalidated |

## Suggested Timeline

This is a rough planning model for a moderate-capacity team.

| Phase | Duration estimate | Exit gate |
| --- | --- | --- |
| Phase 1 | 3-5 weeks | No known cross-tenant read or write path, no stored XSS path, no worker tenant-data path using unscoped fallback |
| Phase 2 | 3-5 weeks | No major false-success paths remain on core execution surfaces; branch and draft state are materially more trustworthy |
| Phase 3 | 2-4 weeks | Workspace switching, repo onboarding, doc deletion, chat resume, and major dead controls are fixed in the shipped UI |
| Phase 4 | 2-3 weeks | Admin and validation surfaces reject bad input clearly and no longer hide invalid or stale state behind misleading fallbacks |
| Phase 5 | Ongoing | Structural debt is reduced without reopening solved product or security risks |

## Staffing Options

### Lean Team

- 1 backend platform engineer
- 1 product engineer
- Security review as needed

Expected behavior:

- Slower progress on CRDT and runtime correctness
- Strong need to sequence work strictly
- Rough timeline: 4-6 months for Phases 1-4

### Balanced Team

- 1 backend platform engineer
- 1 backend runtime engineer
- 1 product engineer
- Security review as needed

Expected behavior:

- Best tradeoff between speed and coordination cost
- Can run Wave 0 and some Wave 1 work in parallel
- Rough timeline: 2-4 months for Phases 1-4

### Aggressive Team

- 2 backend engineers
- 2 product engineers
- 1 security reviewer or security-minded engineer

Expected behavior:

- Faster closure of P0 and P1 risk
- More coordination overhead, but better parallelism
- Rough timeline: 6-10 weeks for Phases 1-4

## Release Gates

The roadmap should not advance to later phases unless these gates are met.

### Gate A: Safe To Operate

- No known cross-tenant read path remains
- No known cross-tenant write path remains
- No known stored XSS path remains
- No worker tenant-data route depends on unscoped fallback authority

### Gate B: Truthful Runtime

- Core mutation routes do not claim success when work was not executed
- Reindex, chat delivery, and task control failures are surfaced explicitly
- Branch, draft, and sync state have regression coverage for the corrected flows

### Gate C: Core Product Usable

- Workspace switching works from the shipped UI
- Repo onboarding works end-to-end
- Doc deletion works from the intended UI
- Chat resume is not a no-op or fork masquerading as resume

## Weekly Cadence Suggestion

| Day | Focus |
| --- | --- |
| Monday | Reconfirm the top 3 P0 or P1 items and lock current sprint scope |
| Midweek | Review implementation risk, migrations, and regression coverage status |
| Friday | Recount open P0 and false-success items and decide whether release gates moved |

## Progress Metrics

| Metric | Target direction |
| --- | --- |
| Open P0 epics | Down every sprint |
| Open cross-tenant findings | Down every sprint until zero |
| Open false-success findings | Down every sprint until core flows are truthful |
| Visible no-op controls in shipped UI | Down to zero before Phase 3 exit |
| Generic 500s caused by invalid input | Down steadily during Phase 4 |
| Regression tests added for P0 and P1 work | Up every sprint |

## Risks To Manage

| Risk | Why it matters | Mitigation |
| --- | --- | --- |
| Fixing UI before backend truthfulness | Creates rework and preserves misleading success states | Keep workflow polish behind runtime correctness work |
| Worker auth redesign touches many paths | High-value but high-blast-radius change | Use staged rollout and strong E2E coverage |
| Invite-only fixes may be inconsistent by surface | Partial rollout can still leak data | Use one shared visibility guard and migrate surfaces systematically |
| Structural cleanup starts too early | Can consume capacity without reducing user or security risk | Keep maintainability work mostly in Phase 5 |

## Suggested First Sprint

If starting immediately, I would use sprint 1 for:

1. `§1.33` and `§1.34`
2. `§1.46`, `§1.48`, and `§1.51`
3. `§38.11–§38.12`
4. `§38.21–§38.26`
5. Regression coverage for the above

That sprint would create visible risk reduction quickly while also opening the
path for the larger worker-auth and invite-only remediation program.
