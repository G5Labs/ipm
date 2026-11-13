# TECH_DEBT Status Template

**Related docs**: [TECH_DEBT_PRIORITY_REVIEW.md](/Users/lumost/workspace/g5/generation/designs/TECH_DEBT_PRIORITY_REVIEW.md), [TECH_DEBT_EXECUTION_BACKLOG.md](/Users/lumost/workspace/g5/generation/designs/TECH_DEBT_EXECUTION_BACKLOG.md), [TECH_DEBT_ROADMAP.md](/Users/lumost/workspace/g5/generation/designs/TECH_DEBT_ROADMAP.md), [TECH_DEBT_RISK_REGISTER.md](/Users/lumost/workspace/g5/generation/designs/TECH_DEBT_RISK_REGISTER.md)  
**Prepared on**: 2026-03-23

## Purpose

This file provides a repeatable status format for weekly updates, sprint
check-ins, and milestone reviews during the tech-debt remediation program.

## Weekly Status Template

```md
# TECH_DEBT Weekly Status

**Week of**: YYYY-MM-DD  
**Current phase**: Phase N  
**Overall status**: On track / At risk / Off track

## Summary

1. Biggest win this week:
2. Biggest remaining risk:
3. Most important next step:

## Phase Gate Status

| Gate | Status | Notes |
| --- | --- | --- |
| Gate A: Safe To Operate | Green / Yellow / Red | |
| Gate B: Truthful Runtime | Green / Yellow / Red | |
| Gate C: Core Product Usable | Green / Yellow / Red | |

## Metrics Snapshot

| Metric | Last week | This week | Trend | Notes |
| --- | --- | --- | --- | --- |
| Open P0 items | | | Up / Flat / Down | |
| Open cross-tenant findings | | | Up / Flat / Down | |
| Open false-success findings | | | Up / Flat / Down | |
| Visible no-op controls | | | Up / Flat / Down | |
| Validation-related generic 500s | | | Up / Flat / Down | |
| P0/P1 regression tests added | | | Up / Flat / Down | |

## Epic Status

| Epic | Status | Progress this week | Blockers | Next step |
| --- | --- | --- | --- | --- |
| Workspace visibility and tenant isolation | Not started / In progress / At risk / Done | | | |
| Worker auth and scoped execution | Not started / In progress / At risk / Done | | | |
| Browser trust and secret handling | Not started / In progress / At risk / Done | | | |
| False-success and execution truthfulness | Not started / In progress / At risk / Done | | | |
| CRDT, branch identity, and state consistency | Not started / In progress / At risk / Done | | | |
| Core workspace and project workflows | Not started / In progress / At risk / Done | | | |
| Chat and collaboration product correctness | Not started / In progress / At risk / Done | | | |
| Config, settings, and validation hygiene | Not started / In progress / At risk / Done | | | |
| UI trustworthiness and fallback cleanup | Not started / In progress / At risk / Done | | | |
| Long-tail maintainability | Not started / In progress / At risk / Done | | | |

## Decisions Needed

1. Decision:
   Owner:
   By when:
2. Decision:
   Owner:
   By when:

## Risks And Escalations

| Risk ID | Status | What changed | Action |
| --- | --- | --- | --- |
| R1 | Open / Mitigated / Closed | | |
| R2 | Open / Mitigated / Closed | | |
| R3 | Open / Mitigated / Closed | | |

## Next Week Focus

1. 
2. 
3. 
```

## Sprint Kickoff Template

```md
# TECH_DEBT Sprint Kickoff

**Sprint**:  
**Phase**:  
**Goal**:

## Committed Outcomes

1. 
2. 
3. 

## Included Epics

- 
- 

## Planned Tickets

| Ticket | Epic | Owner | Why now |
| --- | --- | --- | --- |
| | | | |

## Exit Criteria

- 
- 
- 

## Risks To Watch

- 
- 
```

## Milestone Review Template

```md
# TECH_DEBT Milestone Review

**Milestone**: M1 / M2 / M3 / M4 / M5  
**Date**:

## Outcome

- Met / Partially met / Missed

## What Shipped

- 
- 
- 

## What Did Not Land

- 
- 

## Gate Check

| Gate | Expected | Actual | Result |
| --- | --- | --- | --- |
| | | | |

## Evidence

- Links to PRs:
- Links to tests:
- Links to incidents or regressions:

## Lessons

1. 
2. 
3. 

## Next Adjustment

- Keep doing:
- Stop doing:
- Start doing:
```

## Suggested Usage

- Use the weekly status template for sprint check-ins
- Use the sprint kickoff template when locking sprint scope
- Use the milestone review template when moving between roadmap phases
- Keep metrics consistent week to week so trend direction stays meaningful
