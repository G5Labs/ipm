# TECH_DEBT Epic Ticket Stubs

**Source review**: [TECH_DEBT_PRIORITY_REVIEW.md](/Users/lumost/workspace/g5/generation/designs/TECH_DEBT_PRIORITY_REVIEW.md)  
**Source backlog**: [TECH_DEBT_EXECUTION_BACKLOG.md](/Users/lumost/workspace/g5/generation/designs/TECH_DEBT_EXECUTION_BACKLOG.md)  
**Prepared on**: 2026-03-23

## Usage

Each section below is written as a copy-paste-ready epic stub for Linear, Jira,
or GitHub Issues. The goal is to reduce reformatting work before execution.

---

## Epic 1: Workspace Visibility and Tenant Isolation

**Priority**: P0  
**Wave**: Wave 0  
**Suggested owner**: Backend platform or authz lead  
**Size**: L

**Problem**

Invite-only and tenant isolation rules are not enforced consistently across
workspace surfaces. Multiple file, doc, comment, chat, job, task, notification,
and permission paths still allow org-scoped access where workspace-scoped
membership should be required.

**Impact**

- Cross-tenant or cross-workspace data exposure risk
- Unauthorized reads and writes inside invite-only workspaces
- Release-blocking trust and compliance risk

**Source issues**

`§38.11–§38.12`, `§38.21–§38.30`, `§38.37–§38.40`, `§38.72`, `§38.89–§38.94`

**Scope**

- Shared workspace visibility guard
- File, doc, comment, chat, job, task, notification, and permission routes
- Any service method that still treats org membership as sufficient

**Acceptance criteria**

- Non-members cannot read or mutate invite-only workspace state
- Raw IDs cannot bypass workspace membership checks
- All relevant routes use the same visibility contract
- Integration coverage exists for each major surface

**Suggested first tasks**

- Add a shared `scoped_db_for_workspace` or equivalent guard
- Migrate file and doc routes first
- Migrate chat, comments, jobs, tasks, notifications, and permissions second
- Add invite-only regression tests across all migrated surfaces

---

## Epic 2: Worker Auth and Scoped Execution

**Priority**: P0  
**Wave**: Wave 0  
**Suggested owner**: Backend platform lead  
**Size**: XL

**Problem**

Workers still rely on unscoped fallback authority for tenant data access. For
the current remediation phase, the team is keeping one broad worker token model
and tightening fail-closed route, workspace, org, and task ownership checks
instead of introducing per-task execution tokens immediately.

**Impact**

- Broad worker authority and tenant-boundary risk
- Inability to reason cleanly about worker task ownership
- Hidden coupling behind false-success and RLS bypass issues

**Source issues**

`§1.46`, `§1.48–§1.51`, `§1.55`, `§38.52–§38.53`, `§38.85–§38.88`

**Scope**

- Worker registration and execution auth
- `ScopedDb` worker path
- Control-plane versus task-plane token design
- Worker-owned task and workspace enforcement

**Acceptance criteria**

- No worker tenant-data path uses `ScopedDb::unscoped()` fallback
- Tenant-data routes fail closed on org, workspace, and task ownership checks
- The broad worker token model is explicitly documented as an interim state
- E2E tests cover the tightened worker authorization paths

**Suggested first tasks**

- Remove worker unscoped DB branch where possible
- Bind internal task and chat mutation routes to task ownership
- Add explicit org and workspace ownership checks on tenant-data routes
- Record per-task execution tokens as future-state architecture, not current-phase scope

---

## Epic 3: Browser Trust and Secret Handling

**Priority**: P0  
**Wave**: Wave 0  
**Suggested owner**: Security or frontend platform  
**Size**: M

**Problem**

The browser still has content execution and secret-handling risks, including
stored XSS paths, unsafe URL handling, and sensitive token or secret exposure.

**Impact**

- Browser session compromise
- Secret leakage and unsafe auth posture
- Release-blocking security risk

**Source issues**

`§1.33`, `§1.34`, `§1.49`, `§38.66`, `§38.94`, `§38.110`

**Scope**

- Search rendering
- Markdown link rendering
- Token storage behavior
- Secret artifact cleanup and prevention

**Acceptance criteria**

- User-controlled content cannot execute arbitrary script in-browser
- Dangerous URL schemes are rejected or stripped
- Sensitive browser auth state is not stored unsafely
- Known leaked secret artifacts are removed or rotated

**Suggested first tasks**

- Sanitize search result HTML
- Add markdown URL scheme allowlist
- Replace unsafe browser token storage pattern
- Audit and clean sensitive committed artifacts

---

## Epic 4: False-Success and Execution Truthfulness

**Priority**: P1  
**Wave**: Wave 1  
**Suggested owner**: Backend runtime lead  
**Size**: XL

**Problem**

Several APIs and UI flows still return success even when the requested work was
not actually delivered, resumed, reindexed, persisted, or enforced.

**Impact**

- Users cannot trust successful actions
- Support burden rises because failures look like success
- Downstream state becomes misleading or inconsistent

**Source issues**

`§1.8b`, `§1.16`, `§1.20`, `§38.17–§38.20`, `§38.31`, `§38.41`, `§38.69–§38.71`,
`§38.1167`, `§38.1174–§38.1183`

**Scope**

- Routes that swallow delivery or lifecycle failures
- Mutation paths that log-and-continue on backend errors
- Settings and spend paths that silently reinterpret invalid inputs

**Acceptance criteria**

- Success responses imply a real durable or live effect
- Failures are surfaced explicitly with actionable error states
- Invalid config or spend inputs do not degrade into misleading success
- Regression tests cover the previous false-success cases

**Suggested first tasks**

- Audit success paths that currently log-and-ignore failures
- Convert silent fallback behavior to explicit failure or warning contracts
- Fix spend-history and settings false-success surfaces
- Add regression tests for each corrected path

---

## Epic 5: CRDT, Branch Identity, and State Consistency

**Priority**: P1  
**Wave**: Wave 1  
**Suggested owner**: Backend runtime lead  
**Size**: XL

**Problem**

CRDT identity, branch-correct reads, and sync durability are still too weak,
which creates stale state, mixed branch state, and unreliable synchronization.

**Impact**

- Edits can apply against the wrong state
- Branch-aware reads and writes are not trustworthy
- Sync and reconnect behavior can lose or misapply changes

**Source issues**

`§2b.*`, `§35.2–§35.3`, `§36.1–§36.2`, `§38.3`, `§38.74–§38.78`, `§38.949–§38.991`

**Scope**

- CRDT identity model
- Repo storage branch or commit dimension
- Sync flush, reconnect, and materialization behavior
- Client-side stale-state detection

**Acceptance criteria**

- Draft identity is file- and branch-correct
- Storage reads are branch- or commit-correct where required
- Sync reconnect and flush paths preserve queued changes reliably
- Stale-state detection is explicit and tested

**Suggested first tasks**

- Introduce branch-aware storage API shape
- Correct CRDT document identity assumptions
- Harden outbound flush and disconnect guarantees
- Add reconnect and stale-state integration tests

---

## Epic 6: Core Workspace and Project Workflows

**Priority**: P1  
**Wave**: Wave 2  
**Suggested owner**: Product engineering  
**Size**: L

**Problem**

Some of the most visible product workflows still do not work from the shipped
UI, including workspace switching, project creation, repo onboarding, rich doc
editing bootstrap, and doc deletion.

**Impact**

- Users cannot complete core workflows
- The UI advertises capabilities that do not actually work
- Adoption and trust are directly blocked

**Source issues**

`§15.1–§15.3`, `§16.1–§16.5`, `§19`, `§22.1–§22.3`, `§38.9`, `§38.95`, `§38.101`

**Scope**

- Workspace switcher
- Project creation and navigation
- Repo onboarding
- Rich-doc initialization
- Doc deletion flow

**Acceptance criteria**

- Workspace switcher actions perform real navigation and creation
- Repo onboarding works end-to-end from the product
- New docs created in views can be edited immediately
- Users can delete docs from the intended UI surface

**Suggested first tasks**

- Wire switcher callbacks on the workspace page
- Fix create-project route and empty-org handling
- Implement missing doc deletion UI and API wiring
- Verify rich-doc bootstrap on newly created docs

---

## Epic 7: Chat and Collaboration Product Correctness

**Priority**: P1  
**Wave**: Wave 2  
**Suggested owner**: Product engineering  
**Size**: L

**Problem**

Chat and collaboration flows still have broken resume behavior, pagination and
truncation defects, incorrect read-state handling, and dead artifact navigation.

**Impact**

- Users lose trust in collaboration surfaces
- Chat state and controls feel inconsistent or misleading
- Important history and navigation paths appear broken

**Source issues**

`§38.33`, `§38.45`, `§38.47`, `§38.50–§38.51`, `§38.55`, `§38.80–§38.82`,
`§38.115`, `§38.1180`, `§38.1185–§38.1187`

**Scope**

- Chat resume and fork behavior
- Pagination and session ordering
- Notification read-state contract
- Artifact and citation navigation
- Session naming and summarization UX

**Acceptance criteria**

- Resume performs a true resume action
- Lists and histories do not silently truncate at page one
- Notification read state matches server truth
- Artifacts and citations navigate where the UI implies they should

**Suggested first tasks**

- Implement resume mutation and wire it into visible controls
- Fix list and message pagination contracts
- Correct notification read-state field usage
- Wire artifact and citation navigation actions

---

## Epic 8: Config, Settings, and Validation Hygiene

**Priority**: P2  
**Wave**: Wave 3  
**Suggested owner**: Backend API team  
**Size**: M

**Problem**

Many settings, config, and admin-facing mutation paths still accept invalid
input, rely on DB errors for validation, or treat empty writes as successful
mutations.

**Impact**

- Admins can accidentally misconfigure production behavior
- Users see generic internal failures for ordinary input mistakes
- Audit metadata becomes unreliable because no-op writes look real

**Source issues**

`§9.4`, `§30`, `§38.96–§38.98`, `§38.1121–§38.1123`, `§38.1159–§38.1166`,
`§38.1173–§38.1183`, `§38.1188–§38.1194`

**Scope**

- Settings mutation validation
- Runtime config validation
- Token metadata validation
- Empty-body and no-op write handling

**Acceptance criteria**

- Invalid input yields structured validation errors
- No-op writes are rejected or treated safely as no-ops
- DB constraint failures are not the first validation surface users hit
- Settings and config semantics are explicit and predictable

**Suggested first tasks**

- Add schema or route-level validation for settings and config payloads
- Reject empty update bodies where appropriate
- Normalize blank strings and invalid enums before DB writes
- Add API contract tests for invalid input cases

---

## Epic 9: UI Trustworthiness and Fallback Cleanup

**Priority**: P2  
**Wave**: Wave 3  
**Suggested owner**: Frontend team  
**Size**: M

**Problem**

The UI still has stale-state, wrong-org fallback, wrong-label, accessibility,
and not-found behavior defects that make the product feel inconsistent even
when backend behavior is correct.

**Impact**

- Users are misled by the UI
- Support and product confusion increase
- Accessibility and recoverability suffer

**Source issues**

`§1.39–§1.40`, `§10`, `§23.1–§23.4`, `§38.36`, `§38.49`, `§38.100`,
`§38.1106–§38.1110`

**Scope**

- Navigation and return-to behavior
- Accessibility fixes
- Fallback state selection and invalid slug handling
- Missing management surfaces

**Acceptance criteria**

- Invalid routes and stale slugs show clear failure states
- The UI does not silently substitute the wrong org or resource
- Keyboard and accessibility behavior works for intended controls
- Labels and fallback text match real runtime behavior

**Suggested first tasks**

- Fix wrong-org fallback in settings and related routes
- Add explicit not-found behavior for invalid slugs
- Complete accessibility fixes for high-traffic controls
- Audit labels and fallback paths against actual backend contracts

---

## Epic 10: Long-Tail Maintainability

**Priority**: P3  
**Wave**: Wave 4  
**Suggested owner**: Any team after stabilization  
**Size**: XL

**Problem**

The codebase still has broad structural debt across oversized files,
duplication, dead code, DTO drift, and placeholder implementation paths.

**Impact**

- Slower feature work
- Higher regression risk
- Harder onboarding and review cost

**Source issues**

`§3.*`, `§4.*`, `§5.*`, `§7.*`, `§13`, `§26`, `§34.2`, `§37.4–§37.6`

**Scope**

- Frontend decomposition
- Dead code audit cleanup
- DTO and type-shape cleanup
- Placeholder and stub cleanup

**Acceptance criteria**

- The largest files are decomposed into clearer units
- Verified dead code or stale annotations are removed
- DTO and type contracts are less duplicated and more explicit
- Placeholder behavior is either removed or clearly isolated

**Suggested first tasks**

- Split the largest frontend files first
- Remove stale dead-code annotations after verification
- Consolidate duplicated DTO shapes
- Remove or isolate placeholder runtime paths
