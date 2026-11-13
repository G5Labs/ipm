# TECH_DEBT Priority Review

**Source**: [TECH_DEBT.md](/Users/lumost/workspace/g5/generation/designs/TECH_DEBT.md)  
**Review date**: 2026-03-23

## Purpose

This document groups the open issues from `TECH_DEBT.md` into product and
operational priority buckets so we can decide what to fix first.

The buckets used here are:

- product blocking defect
- product ease of use defect
- product paper cut defect
- security issues
- production services are compromised issues
- production services are broken issues

## Method

- Counted open items from the tracked issue inventory in `TECH_DEBT.md`
- Excluded entries explicitly marked `RESOLVED`
- Treated each issue as having one primary bucket, even when it could fit more
  than one category
- Included the major standalone debt sections that describe active defects, and
  included the accessibility issue rows as usability defects

These counts are best used as **priority-sized estimates**, not exact audit or
compliance numbers.

## Priority Summary

| Priority | Bucket | Approx open items | What belongs here | Representative examples |
| --- | --- | ---: | --- | --- |
| P0 | Production services are compromised issues | ~50 | Cross-org exposure, invite-only visibility bypasses, unscoped or read-scoped auth defects, XSS, secret leakage | `§1.33`, `§1.34`, `§1.46`, `§1.48`, `§38.11–§38.30`, `§38.72`, `§38.89` |
| P1 | Production services are broken issues | ~140 | False-success behavior, data integrity loss, CRDT or branch identity bugs, panics, worker or task control not actually working, severe performance amplification | `§1.8b`, `§1.16`, `§36.1–§36.2`, `§38.17`, `§38.31`, `§38.74`, `§38.1183` |
| P2 | Security issues | ~150 | Serious hardening and least-privilege gaps that are not obviously active compromise, but could become one | `§1.8`, `§1.32`, `§1.35`, `§1.53`, `§30`, `§38.110`, `§38.111` |
| P3 | Product blocking defect | ~45 | Core workflows users expect to work but currently do not | `§16.1–§16.5`, `§19`, `§22.1–§22.3`, `§38.33`, `§38.95`, `§38.101` |
| P4 | Product ease of use defect | ~125 | Confusing or misleading behavior: truncation, stale UI, bad fallbacks, wrong labels, weak error handling | `§1.39`, `§1.40`, `§38.36`, `§38.55`, `§38.82`, `§38.100` |
| P5 | Product paper cut defect | ~750 | Low-severity UX and contract drift, plus broad maintainability debt | `§3.*`, `§4.*`, `§5.*`, `§7.*`, many smaller `§38.*` validation and DTO items |

## Recommended Triage Order

1. **P0 first**: close tenant-isolation, visibility, secret-exposure, and XSS
   gaps before feature work.
2. **P1 second**: eliminate false-success paths and state-corruption risks so
   the platform behaves truthfully and predictably in production.
3. **P3 third**: repair the broken user-facing workflows that block adoption.
4. **P2 in parallel where possible**: hardening work should move alongside P0
   and P1 when it shares the same codepaths.
5. **P4 and P5 last**: these matter, but should not outrank compromise, broken
   runtime behavior, or blocked workflows.

## What This Means Practically

- The highest-risk cluster is **tenant isolation and authz correctness**.
- The next most important cluster is **false-success and broken execution
  semantics**.
- The user-visible shipping backlog should prioritize **P0 + P1 + P3**.
- Most of the highest-priority work now sits in the `§38` follow-up section,
  not the older top-level buckets.

## Suggested Program Buckets

If we want a remediation program instead of a flat issue list, the cleanest
execution order is:

1. **Authz and isolation**
   Fix invite-only visibility, cross-org reads or writes, unscoped fallbacks,
   read-scoped write routes, token handling, and XSS.
2. **False-success and runtime truthfulness**
   Fix endpoints and UI flows that report success when work was not actually
   executed, persisted, or delivered.
3. **Core broken workflows**
   Fix workspace switching, resume flows, deletion flows, repo onboarding, and
   other user-blocking paths.
4. **Usability cleanup**
   Fix truncation, stale state, poor fallback behavior, and misleading labels.
5. **Paper cuts and maintainability**
   Address structural code quality, DTO drift, duplication, and low-severity
   consistency issues.

## Bucket-to-Issue Mapping

This table maps the abstract buckets to the concrete issue clusters that appear
in `TECH_DEBT.md`. The ranges below are representative and focus on the main
concentrations of risk, not every single matching item.

| Bucket | Main issue clusters | Representative issue ranges | Why it matters |
| --- | --- | --- | --- |
| production services are compromised issues | Tenant isolation, invite-only visibility bypasses, unscoped worker or service access, XSS, secret leakage | `§1.2b`, `§1.7`, `§1.8c`, `§1.15`, `§1.27`, `§1.29`, `§1.33–§1.34`, `§1.46`, `§1.48–§1.51`, `§1.55`, `§38.11–§38.12`, `§38.16`, `§38.21–§38.26`, `§38.29–§38.30`, `§38.37–§38.40`, `§38.72`, `§38.83`, `§38.89–§38.94`, `§38.111`, `§38.1138`, `§38.1143`, `§38.1149`, `§38.1154`, `§38.1163`, `§38.1170`, `§38.1178–§38.1179`, `§38.1186` | These are the issues most likely to let one tenant see, affect, or exploit another tenant's data or to compromise a user session in-browser. |
| production services are broken issues | False-success APIs, data integrity loss, CRDT or branch identity defects, worker execution/control failures, major load amplification | `§1.8b`, `§1.10`, `§1.16`, `§1.20`, `§1.54`, `§2.2`, `§2b.1–§2b.9`, `§20`, `§21`, `§24.1–§24.6`, `§25`, `§35.2–§35.3`, `§36.1–§36.2`, `§38.3`, `§38.6–§38.8`, `§38.17–§38.20`, `§38.31`, `§38.41`, `§38.52–§38.53`, `§38.57–§38.60`, `§38.69–§38.71`, `§38.74–§38.78`, `§38.84–§38.88`, `§38.102–§38.105`, `§38.1167`, `§38.1174–§38.1183` | These create the operational pattern where the system says work happened, but nothing executed correctly, or state becomes stale, corrupted, or misleading. |
| security issues | Hardening gaps, privilege boundary drift, weak validation on sensitive paths, token and permission lifecycle gaps | `§1.3`, `§1.8`, `§1.9`, `§1.11–§1.12`, `§1.32`, `§1.35`, `§1.44–§1.45`, `§1.50`, `§1.53`, `§7.1b`, `§7.1d`, `§9.4`, `§28.1`, `§28.4`, `§30`, `§38.1`, `§38.63–§38.65`, `§38.66`, `§38.89`, `§38.97–§38.98`, `§38.109–§38.111`, `§38.1121–§38.1123`, `§38.1159–§38.1166` | These do not always imply active compromise on their own, but they increase the chance that a bad input, leaked token, or policy mistake turns into one. |
| product blocking defect | Core flows that appear available but do not work or do not exist end-to-end | `§15.1–§15.3`, `§16.1–§16.5`, `§19`, `§22.1–§22.3`, `§37.1–§37.3`, `§38.9`, `§38.33–§38.35`, `§38.47`, `§38.50–§38.51`, `§38.60–§38.61`, `§38.71`, `§38.95`, `§38.99`, `§38.101`, `§38.1091`, `§38.1150–§38.1152` | These are the defects users hit directly when trying to perform basic product tasks like switching workspaces, resuming chat, deleting docs, importing SCM, or using visible action buttons. |
| product ease of use defect | Confusing or misleading UX, stale or truncated views, poor fallback behavior, weak validation or poor error presentation | `§1.39–§1.40`, `§8.5`, `§10`, `§22.4`, `§23.1–§23.4`, `§38.27`, `§38.29 Settings Panel`, `§38.36`, `§38.45`, `§38.49`, `§38.55`, `§38.79–§38.82`, `§38.96`, `§38.100`, `§38.115`, `§38.135`, `§38.145`, `§38.148`, `§38.196–§38.197`, `§38.1106–§38.1110` | These make the product feel inconsistent or untrustworthy even when the backend is technically functioning. |
| product paper cut defect | Structural code quality, DTO drift, maintainability debt, placeholder contracts, and lower-severity validation mismatches | `§3.*`, `§4.*`, `§5.*`, `§6.3–§6.4`, `§7.*` except the security-sensitive entries, `§8.2–§8.3d`, `§9.5–§9.6`, `§13`, `§26`, `§28.3`, `§34.2`, `§35.1`, `§37.4–§37.6`, plus many smaller `§38.*` API-contract and UI-consumer gaps | These should be cleaned up, but they should not outrank compromise, runtime breakage, or blocked user workflows. |

## Release Sequence

The backlog becomes much easier to manage if we treat it as a sequence of
release gates instead of one flat list.

| Wave | Goal | Buckets | Main issue focus | Exit condition |
| --- | --- | --- | --- | --- |
| Wave 0 | Prevent compromise | production services are compromised issues + top security issues | Tenant isolation, invite-only enforcement, unscoped worker or service access, XSS, secret exposure, read-scoped write routes | No known cross-org read or write path remains, and browser content cannot execute arbitrary script from user-controlled data |
| Wave 1 | Make production truthful | production services are broken issues | False-success APIs, broken worker control paths, CRDT identity problems, stale search or branch state, high-risk load amplification | The platform no longer reports success when no durable or live effect occurred |
| Wave 2 | Unblock core workflows | product blocking defect | Workspace switching, repo onboarding, deletion flows, resume flows, visible controls that currently no-op | A user can complete the core workflows the UI advertises without hidden admin workarounds |
| Wave 3 | Reduce friction and support load | product ease of use defect | Truncation, stale UI, wrong fallback behavior, misleading labels, poor validation or error surfaces | Common workflows feel coherent, and support tickets should no longer be driven by misleading UI states |
| Wave 4 | Clean up long-tail debt | product paper cut defect | Maintainability, placeholder cleanup, DTO drift, low-severity validation mismatches | The codebase is easier to evolve and less likely to reintroduce the same classes of defects |

## Immediate Release-Blocker Shortlist

If we need a smaller first cut than the full `P0 + P1 + P3` backlog, this is
the shortlist I would use to define release-blocking work:

1. **Cross-org and invite-only access control**
   `§1.8c`, `§1.27`, `§1.29`, `§1.46`, `§1.48–§1.51`, `§38.11–§38.12`,
   `§38.16`, `§38.21–§38.30`, `§38.37–§38.40`, `§38.72`, `§38.89–§38.94`
2. **Browser compromise and secret exposure**
   `§1.33`, `§1.34`, `§1.49`, `§38.66`, `§38.94`, `§38.110`
3. **False-success and broken execution semantics**
   `§1.8b`, `§1.16`, `§1.20`, `§36.1–§36.2`, `§38.17–§38.20`, `§38.31`,
   `§38.41`, `§38.52–§38.53`, `§38.69–§38.71`, `§38.84–§38.88`,
   `§38.1167`, `§38.1174–§38.1183`
4. **Core product flows that visibly do not work**
   `§16.1–§16.5`, `§19`, `§22.1–§22.3`, `§38.33`, `§38.47`, `§38.51`,
   `§38.95`, `§38.101`

## Ownership Suggestion

The issue list is large enough that ownership matters more than the exact
count. A workable split would be:

| Workstream | Primary buckets | Likely surfaces |
| --- | --- | --- |
| Authz and isolation | production services are compromised issues, security issues | `g5-api` auth middleware, `ScopedDb`, route guards, workspace visibility, permission routes |
| Runtime correctness | production services are broken issues | worker APIs, CRDT, task and job lifecycle, search and repo state transitions |
| Product workflows | product blocking defect | workspace switching, chat resume, doc deletion, SCM onboarding, settings surfaces |
| UX and validation | product ease of use defect | client fallbacks, pagination, stale-state handling, request validation, response contracts |
| Maintainability | product paper cut defect | decomposition, dead code, DTO cleanup, duplication, placeholder removal |

## Recommended Epic Breakdown

The issue inventory is too large to execute as one queue. These are the epics I
would use to turn the priority buckets into a workable program.

| Epic | Priority | Primary buckets | Representative issues | Intended outcome |
| --- | --- | --- | --- | --- |
| Workspace visibility and tenant isolation | P0 | production services are compromised issues | `§38.11–§38.12`, `§38.21–§38.30`, `§38.37–§38.40`, `§38.72`, `§38.89–§38.94` | No org member can discover or mutate invite-only workspace state without explicit membership |
| Worker auth and scoped execution | P0 | production services are compromised issues, security issues | `§1.46`, `§1.48–§1.51`, `§1.55`, `§38.52–§38.53`, `§38.85–§38.88` | Workers run with task-scoped, org-scoped authority instead of broad fallback access |
| Browser trust and secret handling | P0 | production services are compromised issues, security issues | `§1.33`, `§1.34`, `§1.49`, `§38.66`, `§38.94`, `§38.110` | Browsers cannot be compromised by product content, and high-value credentials are not exposed or stored unsafely |
| False-success and execution truthfulness | P1 | production services are broken issues | `§1.8b`, `§1.16`, `§1.20`, `§38.17–§38.20`, `§38.31`, `§38.41`, `§38.69–§38.71`, `§38.1167`, `§38.1174–§38.1183` | If the UI or API says something happened, it actually happened or clearly failed |
| CRDT, branch identity, and state consistency | P1 | production services are broken issues | `§2b.*`, `§35.2–§35.3`, `§36.1–§36.2`, `§38.3`, `§38.74–§38.78`, `§38.949–§38.991` | Drafts, file state, branch reads, and sync behavior stay consistent under real usage |
| Core workspace and project workflows | P1 | product blocking defect | `§15.1–§15.3`, `§16.1–§16.5`, `§19`, `§22.1–§22.3`, `§38.9`, `§38.95`, `§38.101` | Users can switch workspaces, create projects, onboard repos, edit docs, and delete docs successfully |
| Chat and collaboration product correctness | P1 | product blocking defect, product ease of use defect | `§38.33`, `§38.45`, `§38.47`, `§38.50–§38.51`, `§38.55`, `§38.80–§38.82`, `§38.115`, `§38.1180`, `§38.1185–§38.1187` | Chat, notifications, and collaborative flows behave as users expect and do not silently fork, truncate, or mislead |
| Config, settings, and validation hygiene | P2 | security issues, product ease of use defect | `§9.4`, `§30`, `§38.96–§38.98`, `§38.1121–§38.1123`, `§38.1159–§38.1166`, `§38.1173–§38.1183`, `§38.1188–§38.1194` | Administrative settings and limits reject bad input explicitly and do not degrade into silent misconfiguration |
| UI trustworthiness and fallback cleanup | P2 | product ease of use defect | `§1.39–§1.40`, `§10`, `§23.1–§23.4`, `§38.36`, `§38.49`, `§38.100`, `§38.1106–§38.1110` | The browser app shows accurate labels, good fallback states, and predictable navigation |
| Long-tail maintainability | P3 | product paper cut defect | `§3.*`, `§4.*`, `§5.*`, `§7.*`, `§13`, `§26`, `§34.2`, `§37.4–§37.6` | The codebase becomes easier to maintain and less likely to regress |

## Suggested First Three Milestones

If we need a concrete starting plan rather than a full multi-quarter program,
this is the sequence I would choose.

| Milestone | Scope | Main issues | Success criteria |
| --- | --- | --- | --- |
| M1: Secure tenancy and browser surfaces | Isolation, invite-only enforcement, XSS, unsafe token and secret handling | `§1.33–§1.34`, `§1.46`, `§1.48–§1.51`, `§38.11–§38.12`, `§38.16`, `§38.21–§38.30`, `§38.37–§38.40`, `§38.66`, `§38.72`, `§38.89–§38.94` | No known cross-tenant read path, no known cross-tenant write path, no stored XSS path, and no obviously unsafe credential surface remains |
| M2: Make execution paths truthful | False-success routes, worker or task lifecycle correctness, state integrity, severe load or pagination pathologies | `§1.8b`, `§1.16`, `§1.20`, `§2b.*`, `§36.1–§36.2`, `§38.17–§38.20`, `§38.31`, `§38.41`, `§38.69–§38.88`, `§38.1167`, `§38.1174–§38.1183` | APIs and UI no longer report successful execution when work was not actually delivered, persisted, resumed, or enforced |
| M3: Repair visible product breaks | Workspace switching, repo onboarding, doc deletion, chat resume, dead buttons and missing first-party management flows | `§15.1–§15.3`, `§16.1–§16.5`, `§19`, `§22.1–§22.3`, `§38.33`, `§38.47`, `§38.50–§38.51`, `§38.95`, `§38.101`, `§38.1150–§38.1152` | A user can complete the workflows the product visibly advertises without hidden no-ops or admin-only workarounds |

## What Can Wait

These areas are worth doing, but I would not let them outrank the first three
milestones unless they are bundled into the same codepaths:

- Broad frontend decomposition work in `§3.*`
- Code duplication and dead-code cleanup in `§4.*` and `§5.*`
- Most type-system cleanup in `§7.*`, except the entries with direct auth or
  validation impact
- Build and fixture consistency cleanup in `§9.5–§9.6`
- Placeholder and commentary cleanup in `§37.4–§37.6`

## Planning Advice

The document strongly suggests prioritizing by **failure mode** instead of by
section number:

1. Fix everything that can cross tenant boundaries or execute arbitrary script.
2. Fix everything that creates false success, silent failure, or corrupted
   runtime state.
3. Fix the user-visible workflows that currently do not work at all.
4. Then clean up confusing UX and the long tail of contract drift.

That ordering is likely to produce a much safer and more credible product
faster than taking the sections in numeric order.

## Delivery Dependencies

Some of the workstreams are only worth doing after their prerequisites are in
place. This dependency order should help avoid churn.

| Depends on first | Then do | Why |
| --- | --- | --- |
| Workspace visibility and tenant isolation | Chat and collaboration product correctness | Collaboration fixes are not trustworthy if session, comment, and doc visibility can still cross workspace boundaries |
| Worker auth and scoped execution | False-success and execution truthfulness | Several false-success paths are symptoms of broad worker authority and weak task binding |
| Browser trust and secret handling | UI trustworthiness and fallback cleanup | It is better to make the browser safe before polishing the experience around unsafe flows |
| False-success and execution truthfulness | Core workspace and project workflows | Repairing user-facing workflows before the backend is truthful often creates more rework |
| Config, settings, and validation hygiene | Long-tail maintainability | Contract cleanup is easier once validation and settings behavior are stable |

## Ticket-Ready Execution Board

This is the smallest planning artifact I would use to convert the review into
real work.

| Epic | Suggested owner | Size | Dependencies | First tickets |
| --- | --- | --- | --- | --- |
| Workspace visibility and tenant isolation | Backend platform or authz lead | L | None | Enforce invite-only checks on files, docs, comments, chat, jobs, tasks, settings, notifications |
| Worker auth and scoped execution | Backend platform lead | XL | None | Replace worker unscoped fallback with explicit fail-closed org, workspace, and task ownership checks; defer per-task execution tokens to future-state architecture |
| Browser trust and secret handling | Security or frontend platform | M | None | Remove stored XSS paths, add URL scheme allowlist, remove unsafe token storage patterns |
| False-success and execution truthfulness | Backend runtime lead | XL | Worker auth and scoped execution | Audit success paths that swallow delivery, reindex, or lifecycle failures; turn them into explicit failures or warnings |
| CRDT, branch identity, and state consistency | Backend runtime lead | XL | False-success and execution truthfulness | Fix CRDT identity model, branch-correct storage reads, sync durability, and stale-state handling |
| Core workspace and project workflows | Product engineering | L | False-success and execution truthfulness | Fix workspace switcher, create-project flows, repo onboarding, doc deletion, rich-edit bootstrapping |
| Chat and collaboration product correctness | Product engineering | L | Workspace visibility and tenant isolation | Fix resume behavior, pagination, session list integrity, notification correctness, artifact navigation |
| Config, settings, and validation hygiene | Backend API team | M | Browser trust and secret handling | Add strict validation for settings, limits, config secrets, token metadata, and empty-body writes |
| UI trustworthiness and fallback cleanup | Frontend team | M | Browser trust and secret handling | Fix stale org fallback, wrong labels, not-found behavior, truncated views, and missing management surfaces |
| Long-tail maintainability | Any team after stabilization | XL | Most earlier epics | Frontend decomposition, dead code removal, DTO cleanup, duplication reduction |

## Ticket Template

To keep the remediation effort consistent, each ticket should answer the same
basic questions:

| Field | What to capture |
| --- | --- |
| Problem | What breaks today, in one sentence |
| User or system impact | Cross-tenant risk, broken workflow, false success, support cost, or maintainability drag |
| Source issues | Exact `§` references from `TECH_DEBT.md` |
| Scope | Routes, services, hooks, components, or schemas being changed |
| Done criteria | Observable behavior that proves the issue is fixed |
| Regression coverage | New unit, integration, or E2E coverage added |
| Rollout risk | Whether the change needs migration, backfill, feature flag, or staged rollout |

## Done Criteria by Priority

The team will move faster if each priority bucket has a shared definition of
"done."

| Bucket | Done means |
| --- | --- |
| production services are compromised issues | There is no known path for cross-tenant read, cross-tenant write, unsafe browser execution, or exposed secret handling in the affected surface |
| production services are broken issues | The API and UI no longer claim success when no durable or live effect occurred, and failure states are explicit |
| security issues | Sensitive paths validate inputs, authority is least-privilege, and token or permission behavior is explicit and auditable |
| product blocking defect | A real user can complete the advertised flow from the shipped UI without no-ops or hidden fallback behavior |
| product ease of use defect | The product communicates clearly, preserves context, and does not silently truncate, mislabel, or redirect to the wrong state |
| product paper cut defect | The relevant codepath is clearer, less duplicated, and less likely to regress, even if user-visible behavior changes only slightly |

## Suggested Dashboard Metrics

If this work is going to run for more than a sprint, I would track a few simple
metrics alongside the issue list:

| Metric | Why it matters |
| --- | --- |
| Open P0 issues | Tells us whether the release-blocking compromise risks are actually shrinking |
| Open false-success issues | Measures whether the system is becoming more truthful |
| Number of visible no-op controls in production UI | Keeps the product-blocking backlog grounded in user reality |
| Invite-only workspace coverage count | Shows whether visibility enforcement is becoming consistent across surfaces |
| End-to-end tests for scoped worker execution | Verifies the highest-risk backend auth change stays intact |
| Validation errors replacing generic 500s | Measures whether API contract quality is improving |

## Suggested Working Agreement

To avoid the backlog becoming another passive inventory, I would treat these as
team rules while the remediation program is active:

1. No new visible no-op controls ship in production UI.
2. No new route may mint a read-scoped DB session for a write operation.
3. No cross-org or invite-only exception may be introduced without an explicit
   documented design decision.
4. False-success behavior should be treated as a production bug, not as a UX
   detail.
5. Every fix in a P0 or P1 epic should add regression coverage before the epic
   is considered complete.
