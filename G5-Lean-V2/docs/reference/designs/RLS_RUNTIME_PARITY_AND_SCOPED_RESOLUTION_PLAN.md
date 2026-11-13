# RLS Runtime Parity And Scoped Resolution Plan

**Date:** 2026-04-16  
**Status:** Implemented in this branch and merged into `TECH_DEBT.md` on 2026-04-19  
**Priority:** High  
**Primary Goal:** Fix the current scoped-resolution outage class and prevent future regressions by making local/CI runtime credentialing match cloud runtime behavior before sweeping the remaining callsites.

---

## TL;DR

The current outage class happens because request-bound code is reading tenant tables through raw `system_pool` connections before activating authz session context.

In production-like cloud runtime, those reads are **not** privileged enough to see tenant rows without `authz.set_ctx(...)`, so they fail closed under RLS and surface as fake `404`s.

The same pattern is present in multiple helper layers:

- entity resolution in `g5-api/src/db/scoped.rs`
- workspace access checks in `g5-api/src/ws/channels.rs`
- websocket/CRDT upgrade auth in `g5-api/src/routes/mod.rs`
- escalation/worker/internal ownership resolution in `g5-api/src/routes/internal.rs` and `g5-api/src/routes/human_escalations.rs`

We did not catch this in local CI because local/CI still runs the app with the legacy `g5` login everywhere, assumes that login can bypass RLS in seed/bootstrap scripts, and wires `tenant_pool = system_pool` in test app construction. In other words, we were validating request logic in an environment that did not actually enforce the same runtime visibility rules as cloud dev.

**Plan of record:**

1. Make local/CI credentialing behave like prod runtime first.
2. Sweep all request-bound scoped-resolution callsites that currently depend on raw `system_pool` reads against RLS tables.
3. Add guardrails so this pattern cannot quietly re-enter the codebase.

## Implementation Status

This document started as the working plan for the runtime-parity / scoped
resolution batch. The branch has now implemented the main delivery items:

- local/CI runtime credentialing now bootstraps split runtime DB users instead
  of collapsing onto the admin login
- strict startup validation exists for the runtime DB split
- `just ci-docker` now runs end-to-end under the least-privilege split and is
  green in this branch
- request-bound scoped-resolution helpers and major callsite families were
  migrated away from raw `system_pool` tenant lookups
- repo metadata moved under tenant RLS, with secret material split behind
  helper-only access
- local CI now includes the route-guard suite, and the comment mention
  savepoint fix was moved into the service layer to comply with those guards

The live source of truth for remaining follow-up is now `TECH_DEBT.md`, not
this plan document.

### Remaining follow-up after this batch

- worker/service execution still needs the deeper per-task execution-token
  model tracked in `TECH_DEBT.md` §§1.48 and 1.51
- org/member/invitation catalog paths still need the remaining system-vs-tenant
  cleanup tracked in `TECH_DEBT.md` §§38.287–38.288
- any future request-path additions still need to preserve the invariant:
  no raw `system_pool` reads of RLS-protected tenant tables

---

## Problem Statement

We have a systemic drift between the intended architecture and the actual request path implementation:

- `AppState` documents `system_pool` as a privileged pool for system-level work and says route handlers should prefer tenant-scoped access via `ScopedDb`.
- Several request-path helpers still perform tenant entity resolution on raw `system_pool`.
- Those helpers query tables protected by `FORCE ROW LEVEL SECURITY`.
- When runtime credentials no longer bypass RLS, these helpers cannot see rows unless authz session context has already been activated on that connection.

This creates a false model:

- `resolve` succeeds because it uses scoped tenant access.
- the next request fails because ownership/access helpers switch back to raw `system_pool`.
- the frontend receives `404` or empty-state noise instead of a clear authorization or wiring failure.

---

## Architectural Mistake

### What we were doing

We repeatedly used this sequence:

1. accept a request with user or worker auth
2. use `system_pool` to resolve workspace/job/task/branch/doc ownership
3. only after that, create `ScopedDb` on `tenant_pool`

### Why that is wrong

For tenant tables under RLS, entity existence and ownership are themselves tenant-scoped facts. Treating raw `system_pool` as a universal visibility oracle is only safe if the runtime DB login can bypass RLS. That is not guaranteed, and it is explicitly incompatible with the security model we want.

### Correct invariant

For request-bound logic:

- do not read RLS-protected tenant tables from raw `system_pool`
- resolve tenant entities through a scoped capability on `tenant_pool`
- reserve `system_pool` for actual system tables and server-owned background flows that are intentionally outside tenant visibility rules

---

## Why Local CI Missed It

### Current local/CI behavior

- `docker/docker-compose.test.yml` starts Postgres with `POSTGRES_USER=g5`
- `cloudbuild-ci.yaml` starts Postgres with `POSTGRES_USER=g5`
- `cloudbuild-ci.yaml` runs the API with only `G5_DATABASE_URL=postgres://g5:...`
- `g5-api/src/services/mod.rs` test wiring uses `tenant_pool = system_pool`
- `scripts/seed-ci-org.sh` explicitly assumes the `g5` user bypasses RLS

### Cloud runtime behavior

- cloud runtime uses Cloud SQL-created users
- live dev runtime showed `current_user = g5`, `rolsuper = false`, `rolbypassrls = false`
- unscoped reads against tenant tables returned no rows until `authz.set_ctx(...)` was activated

### Parity gap

Local CI currently validates request logic in a setup where:

- system and tenant pools collapse to the same effective identity
- runtime credential semantics differ from cloud
- the failure mode only appears when a runtime login cannot bypass RLS

### Required parity rule

Local and CI must run at least one required lane where:

- runtime DB users are distinct from migration/bootstrap users
- tenant access requires `ScopedDb`
- unscoped request-path reads against RLS tables fail the same way they do in cloud runtime

---

## Objectives

### Primary

- eliminate fake `404`/empty-state failures caused by unscoped reads of tenant tables
- make CI catch this class before cloud deploy
- preserve the intended security boundary between system and tenant access

### Secondary

- centralize scoped entity resolution patterns
- reduce duplicated workspace access logic across HTTP, websocket, and internal worker routes
- add guardrails so future code review does not need to rediscover this pattern manually

### Non-goals

- rewriting every legitimate `system_pool` use
- changing non-tenant maintenance jobs that intentionally operate outside request auth
- completing the full long-term DB hardening story in one patch

---

## Phase 0: CI And Local Runtime Credential Parity

**This is the starting phase. No broad callsite sweep should be considered complete until we can reproduce the failure mode locally and in CI.**

### Deliverables

#### 0.1 Introduce explicit runtime identities

Create SQL-bootstrapped runtime users instead of depending on the Terraform-created `g5` login for app runtime.

Recommended shape:

- `g5`:
  - migration/bootstrap/admin only
- `g5_runtime_system`:
  - non-tenant system-pool runtime user
- `g5_runtime_tenant`:
  - tenant-pool runtime user subject to RLS
- existing `authz_writer` / `authz_ctx_writer` remain scoped to authz duties

Important:

- do **not** use a single runtime login that inherits both tenant and system privileges if the goal is real pool separation
- local/CI should use the same role split, even if passwords are test-only

#### 0.2 Update app config and startup validation

Add explicit runtime parity configuration support:

- continue supporting `G5_TENANT_DATABASE_URL`
- introduce an explicit strict-mode signal for parity environments
- fail startup in strict mode if the required runtime URLs are missing

Suggested behavior:

- local fast-dev can keep an escape hatch temporarily
- CI parity lane, dev cloud, staging, and prod should require strict runtime separation

#### 0.3 Update local test/runtime wiring

Update:

- `docker/docker-compose.test.yml`
- `cloudbuild-ci.yaml`
- local dev/test bootstrap scripts
- `AppState::new_for_test(...)`

So that parity integration tests use:

- separate runtime system and tenant URLs
- restricted tenant runtime semantics
- no implicit `tenant_pool = system_pool`

#### 0.4 Add parity bootstrap

Implement SQL/bootstrap logic that:

- creates the runtime users if absent
- grants the intended role memberships
- keeps `g5` as migration/admin only

This bootstrap should be usable from:

- local docker-compose test stack
- CI cloudbuild test stack
- cloud migration/init path

### Acceptance Criteria

- local parity lane reproduces the current outage before the code fix
- local parity lane passes after the code fix
- CI parity lane is required
- runtime app code no longer depends on the `g5` admin login

---

## Phase 1: Shared Resolver And Access Layer Refactor

### Goal

Fix the shared helper layer first so route patches do not fork the same logic in multiple files.

### Current high-risk helper cluster

| Area | File | Problem |
| --- | --- | --- |
| entity ownership resolution | `g5-api/src/db/scoped.rs` | raw `PgPool` lookups on RLS tables |
| workspace access enforcement | `g5-api/src/db/scoped.rs` | existence/visibility checks on `workspaces` and `workspace_members` via raw pool |
| websocket workspace access | `g5-api/src/ws/channels.rs` | direct access checks and job/task/chat resolution through raw pool |

### Work Items

#### 1.1 Refactor entity resolution helpers

Replace raw-pool request-path helpers for:

- `workspaces`
- `workspace_branches`
- `draft_sessions`
- `jobs`
- `tasks`
- `docs`
- `human_escalations`

with helpers that operate on a scoped request DB handle or that mint a scoped capability first.

#### 1.2 Separate request-bound and server-owned helpers

Split helper intent clearly:

- request-bound tenant resolution helpers
- server-owned background helpers that are allowed to resolve by explicit system rules

#### 1.3 Consolidate workspace access checks

Move toward one canonical workspace access implementation used by:

- HTTP handlers
- websocket subscriptions
- CRDT upgrade checks
- internal worker routes where applicable

### Acceptance Criteria

- request-bound helper layer no longer queries RLS-protected tables through raw `system_pool`
- websocket and HTTP access checks share the same scoped semantics

---

## Phase 2: Sweep Remaining Callsites By Family

### Family A: Workspace-scoped request paths

Target files:

- `g5-api/src/db/scoped.rs`
- routes that call `scoped_db_for_workspace`, `scoped_db_for_branch`, `scoped_db_for_draft`, `scoped_db_for_task`, `scoped_db_for_job`

Must verify:

- workspace detail
- ontology
- workers
- drafts
- docs
- jobs
- comments
- notifications
- search
- settings
- spend

### Family B: WebSocket and CRDT auth

Target files:

- `g5-api/src/ws/channels.rs`
- `g5-api/src/ws/mod.rs`
- `g5-api/src/routes/mod.rs`

Must verify:

- `Workspace` channel auth
- `Job` channel auth
- `Task` channel auth
- `Chat` channel auth
- CRDT upgrade auth for user tokens
- CRDT upgrade auth for worker tokens

### Family C: Human escalation and HIL routes

Target files:

- `g5-api/src/routes/human_escalations.rs`
- `g5-api/src/routes/internal.rs`

Must verify:

- selected workspace/escalation lookup
- create escalation
- resolve escalation
- return-to-human-review
- cancel escalation
- fetch escalation context

### Family D: Internal worker ownership resolution

Target files:

- `g5-api/src/routes/internal.rs`

Must verify:

- task transition
- task status alias
- global escalation creation
- staleness update
- doc content update
- any route that resolves task/job/doc/workspace before calling `ScopedDb`

### Family E: Fire-and-forget orchestration

Target files:

- `g5-api/src/routes/internal.rs`

Must verify:

- post-resolution orchestration does not assume raw workspace visibility through `system_pool`
- background work uses explicit system-owned scoped access when needed

---

## Phase 3: Guardrails Against Regression

### 3.1 Add codebase guard tests

Add a guard that flags request-path code which:

- uses `state.system_pool`
- against known RLS-protected tenant tables
- outside an explicit allowlist

Known RLS table set includes at least:

- `workspaces`
- `workspace_branches`
- `workspace_dependencies`
- `workspace_members`
- `workspace_settings`
- `docs`
- `jobs`
- `tasks`
- `chat_sessions`
- `draft_sessions`
- `human_escalations`
- `repo_manifests`
- `org_configs`
- `workspace_configs`
- `user_configs`

### 3.2 Add architecture note

Document:

- `system_pool` is not a tenant visibility oracle
- request-bound entity resolution belongs on scoped tenant access
- any exception must be justified and tested in parity mode

### 3.3 Add explicit parity lane ownership

Define one required CI phase as the source of truth for runtime DB credential parity.

It must run before deploy promotion.

---

## Comprehensive Test Plan

### 1. Parity Environment Tests

### 1.1 Runtime visibility sanity test

In strict parity mode:

- raw unscoped query to `workspaces` returns no rows for tenant data
- scoped query with `authz.set_ctx(...)` or `ScopedDb::from_capability(...)` returns rows

### 1.2 Pool identity test

Assert at runtime:

- `system_pool` user is not the migration/admin user
- `tenant_pool` user is distinct from `system_pool`
- tenant user does not bypass RLS

### 1.3 Startup contract test

In strict parity mode:

- startup fails if required runtime DB env vars are missing

### 2. Helper Regression Tests

Add focused tests for the shared helper layer:

- `scoped_db_for_workspace`
- `scoped_db_for_branch`
- `scoped_db_for_draft`
- `scoped_db_for_task`
- `scoped_db_for_job`
- workspace access checks for org-visible and invite-only workspaces

Test both:

- success with scoped capability
- failure with intentionally unscoped/raw lookup path

### 3. Route Regression Tests

### 3.1 User HTTP flows

Minimum matrix:

- `/api/v1/resolve?org=g5&workspace=base`
- `/api/v1/workspaces/:id`
- `/api/v1/workspaces/:id/ontology`
- `/api/v1/workspaces/:id/workers`
- `/api/v1/branches/:branchId/drafts`
- `/api/v1/escalations?workspaceId=:id`

### 3.2 Websocket flows

Minimum matrix:

- subscribe to workspace channel
- subscribe to job channel
- subscribe to task channel
- subscribe to chat session channel
- CRDT websocket upgrade with valid user token

### 3.3 Internal worker flows

Minimum matrix:

- transition task
- create escalation
- resolve escalation
- cancel escalation
- update doc content
- update workspace staleness

### 4. End-To-End Parity Smoke

In CI parity lane, reproduce the formerly broken flow:

1. authenticate as a real user
2. resolve `g5/base`
3. load workspace shell
4. load ontology
5. load workers
6. create or list drafts
7. verify websocket auth succeeds

---

## Rollout Plan

### Step 1

Merge CI/local runtime credential parity changes first.

### Step 2

Run parity lane red and capture current failures.

### Step 3

Refactor shared resolver/access helpers and make parity lane green.

### Step 4

Sweep websocket, escalation, and internal route callsites.

### Step 5

Promote to cloud dev and validate:

- workspace shell load
- workers
- ontology
- drafts
- websocket subscriptions

### Step 6

Promote to staging.

### Step 7

Promote to prod.

### Step 8

After stable adoption, remove any temporary non-strict fallback from CI and default dev integration flows.

---

## File Map

### Configuration and runtime

- `g5-api/src/config.rs`
- `g5-api/src/main.rs`
- `g5-api/src/services/mod.rs`
- `g5-api/src/db/mod.rs`

### Shared resolver/access layer

- `g5-api/src/db/scoped.rs`
- `g5-api/src/ws/channels.rs`
- `g5-api/src/ws/mod.rs`
- `g5-api/src/routes/mod.rs`

### Route families

- `g5-api/src/routes/human_escalations.rs`
- `g5-api/src/routes/internal.rs`

### Local/CI parity

- `docker/docker-compose.test.yml`
- `cloudbuild-ci.yaml`
- `scripts/seed-ci-org.sh`
- local bootstrap/init scripts used by test stack

### Cloud runtime hardening

- `infra/modules/cloudsql/main.tf`
- `infra/modules/external-secrets/main.tf`
- migration/init/bootstrap SQL that provisions runtime users

### Tests

- `g5-api/tests/api_internal.rs`
- integration tests around workspace-scoped routes
- websocket/CRDT tests
- any new guard tests for forbidden `system_pool` tenant lookups

---

## Acceptance Criteria

- local parity environment reproduces the original failure before the fix
- required CI parity lane reproduces the same semantics as cloud runtime
- request-bound code no longer depends on raw `system_pool` reads against RLS tenant tables
- websocket auth and HTTP auth use the same scoped visibility model
- worker/internal ownership resolution works under restricted runtime users
- startup can enforce strict runtime DB separation in parity and cloud environments
- regressions of this pattern are blocked by automated tests or guards

---

## Open Questions

1. Do we want one explicit strict-mode env flag, or do we want environment-aware startup validation with a first-class deployment environment setting?
2. Do we want local developer default flows to become strict immediately, or do we keep a temporary fast-path while the callsite sweep lands?
3. Which system-owned background flows should remain allowed to resolve tenant rows outside request scope, and how do we make those exceptions explicit?
