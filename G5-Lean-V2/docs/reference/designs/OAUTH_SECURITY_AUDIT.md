# OAuth & Authorization Security Audit

_Date: 2026-02-23_
_Scope: g5-api route authorization, g5d credential handling, OAuth scope enforcement_

---

## Executive Summary

The G5 API has **~125 public route handlers** but only **9 have `RequireScopes` middleware applied**. The remaining 116 authenticated routes rely solely on the presence of a valid JWT — any authenticated user can perform any operation on any workspace or org regardless of their role. The worker authentication system uses a **static shared service token** with a **hardcoded fallback** (`"test-service-token"`), grants **blanket admin access** to all internal AND public API routes, and has **no org/user binding**. These issues must be addressed before any production deployment.

---

## Part 1: API Route Authorization Audit

### 1.1 Routes WITH Scope Enforcement (9 of ~125)

| Route | Scope Required | File |
|-------|---------------|------|
| `POST /api/v1/workspaces/:wid/jobs` | `jobs:execute` | jobs.rs |
| `POST /api/v1/tasks/:id/pause` | `jobs:execute` | tasks.rs |
| `POST /api/v1/tasks/:id/resume` | `jobs:execute` | tasks.rs |
| `POST /api/v1/tasks/:id/abort` | `jobs:execute` | tasks.rs |
| `POST /api/v1/tasks/:id/complete` | `jobs:execute` | tasks.rs |
| `POST /api/v1/tasks/:id/rollback` | `jobs:execute` | tasks.rs |
| `GET /api/v1/workspaces/:wid/search` | `workspaces:read` | search.rs |
| `GET /api/v1/.../spend/history` | `billing:admin` | settings.rs |
| `GET /api/v1/resolve` | `workspaces:read` | resolve.rs |

### 1.2 Critical Routes MISSING Scope Enforcement

#### Workspace Mutations (should require `workspaces:write`)
- `POST /api/v1/orgs/:orgId/workspaces` — create workspace
- `PATCH /api/v1/workspaces/:id` — update workspace
- `DELETE /api/v1/workspaces/:id` — **delete workspace**
- `POST /api/v1/workspaces/:id/branches` — create branch
- `DELETE /api/v1/workspaces/:id/branches/:bid` — delete branch
- `POST /api/v1/workspaces/:id/sync` — sync workspace
- `POST /api/v1/workspaces/:id/merge` — **merge branches**
- `POST /api/v1/workspaces/:id/clone` — clone workspace

#### Workspace Reads (should require `workspaces:read`)
- `GET /api/v1/orgs/:orgId/workspaces` — list workspaces
- `GET /api/v1/workspaces/:id` — get workspace
- `GET /api/v1/workspaces/:id/stats` — workspace stats
- `GET /api/v1/workspaces/:id/branches` — list branches
- `GET /api/v1/workspaces/:id/dependencies` — dependency graph

#### Job Mutations (should require `jobs:execute`, parity gap with tasks)
- `POST /api/v1/jobs/:id/pause` — pause job
- `POST /api/v1/jobs/:id/resume` — resume job
- `POST /api/v1/jobs/:id/cancel` — **cancel job**
- `POST /api/v1/jobs/:id/rollback` — **rollback job**

#### Draft/Commit Mutations (should require `workspaces:write`)
- `POST /api/v1/drafts/:id/commit` — **commit draft to git**
- `POST /api/v1/workspaces/:wid/commit-all` — **commit all drafts**
- `POST /api/v1/drafts/:id/discard` — discard draft
- `POST /api/v1/branches/:bid/drafts` — create draft

#### Org Admin (should require `org:admin`)
- `PATCH /api/v1/orgs/:orgId` — **update org** (doc says `org:admin`)
- `PATCH /api/v1/orgs/:orgId/members/:userId` — **change member role** (doc says `org:admin`)
- `DELETE /api/v1/orgs/:orgId/members/:userId` — **remove member** (doc says `org:admin`)
- `DELETE /api/v1/orgs/:orgId/invitations/:id` — revoke invitation (doc says `org:admin`)

#### Permission Admin (should require `org:admin`)
- `PUT /api/v1/orgs/:orgId/scopes` — **set org default scopes** (doc says `org:admin`)
- `PUT /api/v1/orgs/:orgId/scopes/members/:userId/scopes` — **set user scopes** (doc says `org:admin`)
- `PUT /api/v1/workspaces/:wid/permissions/:userId` — **set workspace permissions**
- `DELETE /api/v1/workspaces/:wid/permissions/:userId` — remove workspace permissions

#### Chat Session Mutations (should require `sessions:write`)
- `POST /api/v1/sessions/:id/messages` — send message
- `POST /api/v1/sessions/:id/fork` — fork session
- `POST /api/v1/sessions/:id/inject` — inject context

#### Ontology Mutations (should require `workspaces:write`)
- `POST /api/v1/workspaces/:wid/ontology` — create node
- `DELETE /api/v1/workspaces/:wid/ontology/:nid` — delete node
- `PUT /api/v1/.../ontology/:nid/content` — update node content

#### Worker Management (should require `workers:read` / `workers:admin`)
- `GET /api/v1/workspaces/:wid/workers` — list workers (doc says `workers:read`)
- `GET /api/v1/workers/:id` — get worker (doc says `workers:read`)

### 1.3 Scope Inventory vs. Enforcement

| Scope (from design doc) | Defined | Enforced in Middleware | Gap |
|--------------------------|---------|----------------------|-----|
| `workspaces:read` | Yes | 2 routes (search, resolve) | ~8 read routes unprotected |
| `workspaces:write` | Yes | 0 routes | ~15 write routes unprotected |
| `jobs:read` | Yes | 0 routes | ~3 read routes unprotected |
| `jobs:execute` | Yes | 6 routes (create job + 5 task mutations) | 4 job mutations unprotected |
| `workers:read` | Yes | 0 routes | 2 routes unprotected |
| `workers:admin` | Yes | 0 routes | No worker provisioning endpoints exist yet |
| `sessions:read` | Yes | 0 routes | ~4 read routes unprotected |
| `sessions:write` | Yes | 0 routes | ~7 write routes unprotected |
| `org:admin` | Yes | 0 routes | ~6 admin routes unprotected |
| `billing:admin` | Yes | 1 route (spend history) | Complete |

---

## Part 2: Worker/Service Token Security Issues

### 2.1 Hardcoded Fallback Token

**Location:** `g5-api/src/routes/mod.rs` line 380 (CRDT WebSocket adapter)

```rust
let expected_token =
    std::env::var("G5_SERVICE_TOKEN").unwrap_or_else(|_| "test-service-token".to_string());
```

If `G5_SERVICE_TOKEN` is not set, any client sending `Authorization: Bearer test-service-token` gets **full service-level access** to the CRDT WebSocket and by extension to all workspace document state.

**Severity: CRITICAL** — This is a backdoor in any deployment where the env var is unset.

### 2.2 Service Token = Blanket Admin

**Location:** `g5-api/src/middleware/auth_middleware.rs` line 542

```rust
fn service_claims() -> JwtClaims {
    JwtClaims {
        sub: "00000000-0000-0000-0000-000000000001",
        org: "00000000-0000-0000-0000-000000000001",
        scopes: vec!["admin", "workspaces:read", "workspaces:write",
                     "jobs:read", "jobs:execute", "orgs:manage"],
        iat: 0, exp: i64::MAX,
    }
}
```

When a service token hits a `/api/v1/` route, it gets synthetic admin claims with **hardcoded org UUID** and **all scopes**. This means:
- A compromised workers can hit ANY public API endpoint as admin
- The org is always the sentinel UUID, not the worker's actual org
- The token never expires (`exp: i64::MAX`)

### 2.3 No Org/User Binding on Worker Registration

**Current flow:**
1. Operator sets `G5_SERVICE_TOKEN=<some-value>` on both g5-api and g5d
2. g5d starts with `--service-token <value>` or `G5D_SERVICE_TOKEN=<value>`
3. g5d calls `POST /api/internal/workers/register` with that token
4. Any process with the token can register as a worker for **any org**

**Problem:** A single leaked service token gives full access to ALL orgs' internal API. There's no mechanism to:
- Scope a worker to a specific org
- Scope a worker to specific capabilities (e.g., only poll tasks, no file upload)
- Revoke a single worker without rotating the shared token (affecting all workers)

### 2.4 SCM Credentials Not Scoped to Workspace

**Gap (from Architecture Doc §7.2, GAP 1):** When a worker needs to clone a repo, it receives the user's full SCM token, which grants access to ALL repos the user can see — not just the workspace's repo. A compromised worker could use this token to access or modify any repository the user has connected.

### 2.5 g5d Credential Storage

**Current state:**
- `service_token` is passed via CLI arg or env var (`G5D_SERVICE_TOKEN`) and held in `WorkerConfig` (plain `String` in heap)
- `InternalApiClient` stores `service_token: String` and sends it via `Authorization: Bearer` on every request
- The `credential_encryption_key` received during registration is held in memory for encrypting SCM tokens at rest
- `G5D_TOKEN` env var is used by `g5d clone` for SCM authentication — passed directly as a credential
- No credential rotation, no secure memory (zeroize), no credential store integration

---

## Part 3: Concrete Hardening Steps

### Phase 1 — Lock Down OAuth Scopes (P0, sprint-level)

#### Step 1.1: Apply `RequireScopes` to ALL Public Routes

Every route handler that currently takes `_auth: AuthContext` or `auth: AuthContext` must have appropriate scope middleware. The mapping should follow the design doc's scope taxonomy:

| Route Group | Read Scope | Write Scope |
|-------------|-----------|-------------|
| Workspaces, Branches, Files, Ontology, Drafts, Docs, Search | `workspaces:read` | `workspaces:write` |
| Jobs, Tasks | `jobs:read` | `jobs:execute` |
| Workers | `workers:read` | `workers:admin` |
| Chat Sessions | `sessions:read` | `sessions:write` |
| Org Members, Invitations | (authenticated) | `org:admin` |
| Permissions | (authenticated) | `org:admin` |
| Spend/Billing | `billing:admin` | `billing:admin` |
| Comments, Notifications | `workspaces:read` | `workspaces:write` |

**Implementation:** In each router function, apply `RequireScopes` via `route_layer`:

```rust
pub fn workspace_detail_router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/:id", get(get_workspace))              // workspaces:read
        .route("/:id/stats", get(get_workspace_stats))   // workspaces:read
        .route_layer(RequireScopes::new(vec!["workspaces:read".into()]))
        .route("/:id", patch(update_workspace))          // workspaces:write
        .route("/:id", delete(delete_workspace))         // workspaces:write
        .route_layer(RequireScopes::new(vec!["workspaces:write".into()]))
}
```

For routes needing different scopes for GET vs POST/PUT/DELETE, split into sub-routers or use method-specific layers.

#### Step 1.2: Fix Job Mutation Parity Gap

Add `RequireScopes::new(vec!["jobs:execute".into()])` to `job_detail_router()` for `pause_job`, `resume_job`, `cancel_job`, `rollback_job` — matching the enforcement already on `task_detail_router()`.

#### Step 1.3: Add Workspace-Level Authorization Checks

Beyond scope checks, services must verify the user is a **member of the org** that owns the workspace. Currently, any user with `workspaces:read` scope could access ANY workspace across ALL orgs if they know the UUID. Each service method should:
1. Resolve `workspace → org_id`
2. Verify `auth.org_id == workspace.org_id` OR user has explicit cross-org grant
3. Return 403 if mismatched

---

### Phase 2 — Replace Static Service Token with Worker Registration Tokens (P0)

#### Step 2.1: Define Worker Registration Token Format

Replace the shared `G5_SERVICE_TOKEN` with **org-scoped worker registration tokens** (as proposed in Architecture Doc §7.5.1):

```rust
/// Worker registration token — signed JWT issued by org admin
pub struct WorkerRegistrationToken {
    /// "g5wt" prefix for easy identification
    pub token_type: String,  // "g5wt"
    /// Org this worker is authorized for
    pub org_id: Uuid,
    /// Optional: restrict to specific workspace
    pub workspace_id: Option<Uuid>,
    /// Capabilities: ["poll_tasks", "execute_tasks", "push_git"]
    pub capabilities: Vec<String>,
    /// Issued-at timestamp
    pub iat: i64,
    /// Expiration timestamp (e.g., 90 days)
    pub exp: i64,
    /// Issued by (user who created the token)
    pub issued_by: Uuid,
}
```

#### Step 2.2: Worker Registration Flow

```
1. Org admin → POST /api/v1/orgs/:orgId/worker-tokens
   Body: { capabilities: [...], expiresIn: "90d", workspaceId?: "..." }
   Response: { token: "g5wt_eyJ...", id: "...", expiresAt: "..." }

2. Operator configures g5d with the token:
   G5D_REGISTRATION_TOKEN=g5wt_eyJ...

3. g5d starts → POST /api/internal/workers/register
   Header: Authorization: Bearer g5wt_eyJ...
   g5-api validates signature, extracts org_id + capabilities
   Response: { worker_id, session_token: "g5ws_...", credential_encryption_key }

4. g5d uses session_token for subsequent API calls
   Session token is short-lived (e.g., 4hr), auto-renewed via heartbeat
```

#### Step 2.3: Scope Worker's Internal API Access

The `AuthContext::Service` variant should carry the worker's org and capabilities:

```rust
pub enum AuthContext {
    User(JwtClaims),
    Worker {
        worker_id: Uuid,
        org_id: Uuid,
        capabilities: Vec<String>,
    },
}
```

Internal route handlers must then verify the worker is operating within its org:
- `worker_poll()` returns only tasks for the worker's org
- `task_transition()` verifies the task belongs to the worker's org
- `upload_file()` verifies the workspace belongs to the worker's org

#### Step 2.4: Remove the Hardcoded Fallback

The `validate_service_token()` function in `routes/mod.rs` must **never** fall back to `"test-service-token"`:

```rust
fn validate_service_token(token: &str) -> Result<JwtClaims, AppError> {
    let expected = std::env::var("G5_SERVICE_TOKEN")
        .map_err(|_| AppError::from_error(G5Error::Internal {
            message: "G5_SERVICE_TOKEN not configured".into(),
        }))?;
    // constant-time comparison...
}
```

In test environments, use a test fixture that explicitly sets the env var. In production, `g5-api` should **refuse to start** if `G5_SERVICE_TOKEN` is not set.

#### Step 2.5: Token Lifecycle Endpoints

```
POST   /api/v1/orgs/:orgId/worker-tokens          — create registration token (org:admin)
GET    /api/v1/orgs/:orgId/worker-tokens          — list active tokens (org:admin)
DELETE /api/v1/orgs/:orgId/worker-tokens/:tokenId — revoke token (org:admin)
GET    /api/v1/orgs/:orgId/worker-tokens/:tokenId — get token metadata (org:admin)
```

---

### Phase 3 — Harden g5d Credential Handling (P0)

#### Step 3.1: Remove Credential Env Var Fallbacks

Current state allows credentials via multiple channels with no hierarchy:
- `--service-token` CLI arg (visible in `ps aux`)
- `G5D_SERVICE_TOKEN` env var (visible in `/proc/*/environ`)

**Change to:**
1. **Primary:** `G5D_REGISTRATION_TOKEN` env var set by container orchestrator (K8s Secret → env)
2. **Secondary:** `--registration-token-file /path/to/token` flag (reads from file, then zeroes file content)
3. **Never:** CLI arg value directly (would appear in process listing)

```rust
pub struct WorkerArgs {
    /// Path to file containing the registration token.
    /// Token is read and the file reference is dropped immediately.
    #[arg(long, env = "G5D_REGISTRATION_TOKEN_FILE")]
    pub registration_token_file: Option<PathBuf>,

    /// Registration token via env var (for container orchestrators).
    /// Prefer token-file in non-containerized deployments.
    #[arg(long, env = "G5D_REGISTRATION_TOKEN", hide = true)]
    pub registration_token: Option<String>,

    // Remove: --service-token / G5D_SERVICE_TOKEN
}
```

#### Step 3.2: Secure Memory for Credentials

Use `secrecy` / `zeroize` crates for credential fields:

```rust
use secrecy::{Secret, ExposeSecret, Zeroize};

pub struct WorkerConfig {
    pub registration_token: Secret<String>,  // zeroized on drop
    // ...
}

pub struct InternalApiClient {
    session_token: tokio::sync::RwLock<Secret<String>>,  // rotatable
    // ...
}
```

#### Step 3.3: Session Token Rotation

After registration, the worker receives a short-lived session token. The heartbeat response should include a refreshed token:

```rust
// Heartbeat response
pub struct HeartbeatResponse {
    pub status: String,
    pub session_token: Option<String>,  // new token if close to expiry
    pub token_expires_at: Option<i64>,
}
```

The `InternalApiClient` should swap the token atomically:

```rust
impl InternalApiClient {
    async fn refresh_session_token(&self, new_token: String) {
        let mut guard = self.session_token.write().await;
        *guard = Secret::new(new_token);
    }
}
```

#### Step 3.4: No Client Access to Unscoped Credentials

The frontend/client must **never** see:
- Service tokens or worker registration tokens
- SCM OAuth tokens (these stay server-side in `user_scm_connections.access_token_enc`)
- The `credential_encryption_key` returned during worker registration
- Any LLM API keys

**Current violation:** The `credential_encryption_key` is returned in the worker registration response DTO and stored by g5d. While this is a worker-to-server interaction (not browser-to-server), the key should be delivered via a secure side-channel or derived from a key agreement protocol, not as a plain JSON field.

**Proposed:** Use ECDH key agreement during registration:
1. Worker generates ephemeral X25519 keypair, sends public key in registration request
2. Server generates ephemeral X25519 keypair, derives shared secret via ECDH
3. Server returns its public key in registration response
4. Both sides derive `credential_encryption_key` from shared secret via HKDF
5. Key never appears in any HTTP response body

#### Step 3.5: SCM Credential Narrowing (from Architecture Doc §7.5.3)

When dispatching a task to a worker, g5-api should:
1. Look up the task's workspace → repo → SCM connection
2. Generate a **short-lived, repo-scoped token** (GitHub App installation tokens support this)
3. Encrypt with the worker's `credential_encryption_key`
4. Deliver via the task assignment response, not as a separate credential

For GitHub: Use GitHub App installation tokens scoped to specific repositories.
For GitLab: Use project-scoped deploy tokens.
For generic OAuth: This requires the supervisor/proxy model from Architecture Doc §1 — worker calls a server-side git proxy that holds the full credential.

---

### Phase 4 — Startup & Runtime Guardrails (P1)

#### Step 4.1: Mandatory Configuration Validation

g5-api must refuse to start without:
- `G5_SERVICE_TOKEN` (or `G5_WORKER_TOKEN_SIGNING_KEY` in the new model)
- `G5_JWT_PRIVATE_KEY` / `G5_JWT_PUBLIC_KEY`
- `G5_SCM_ENCRYPTION_KEY`
- `G5_OAUTH_CLIENT_ID` + `G5_OAUTH_CLIENT_SECRET` (at least one provider)

```rust
impl AppConfig {
    pub fn validate_security(&self) -> Result<(), Vec<String>> {
        let mut errors = vec![];
        if self.service_token.is_none() {
            errors.push("G5_SERVICE_TOKEN is required".into());
        }
        if self.jwt_private_key.is_none() {
            errors.push("G5_JWT_PRIVATE_KEY is required".into());
        }
        // ...
        if errors.is_empty() { Ok(()) } else { Err(errors) }
    }
}
```

#### Step 4.2: Audit Logging for Auth Events

Log all authentication decisions at `info` level with structured fields:

```rust
tracing::info!(
    auth.type = "service_token",
    auth.worker_id = %worker_id,
    auth.org_id = %org_id,
    request.path = %path,
    request.method = %method,
    "Worker authenticated"
);
```

Critical events to log:
- Login/callback (user OAuth)
- Token refresh
- Service token usage on `/api/v1/` routes (this is unusual and should be flagged)
- Scope check failures (403s)
- Worker registration
- SCM credential access by workers

#### Step 4.3: E2E Test Token Guard

The `POST /api/v1/auth/test-token` endpoint currently checks `G5_E2E_AUTH=true` at **route registration time** (router construction). This is correct — the endpoint literally doesn't exist in the router when the env var is absent. However, add a runtime double-check in the handler for defense in depth:

```rust
async fn issue_test_token(...) -> ... {
    if std::env::var("G5_E2E_AUTH").unwrap_or_default() != "true" {
        return Err(AppError::from_error(G5Error::Forbidden {
            missing_scopes: vec!["e2e:test".into()],
        }));
    }
    // ...
}
```

---

## Summary: Priority Matrix

| # | Item | Severity | Effort | Phase |
|---|------|----------|--------|-------|
| 1 | Remove hardcoded `"test-service-token"` fallback | **CRITICAL** | 1 hr | 1 |
| 2 | Apply `RequireScopes` to all ~116 unprotected routes | **HIGH** | 2-3 days | 1 |
| 3 | Add org-membership checks to service layer | **HIGH** | 2-3 days | 1 |
| 4 | Fix job mutation parity (4 routes) | **HIGH** | 1 hr | 1 |
| 5 | Replace static service token with registration tokens | **HIGH** | 3-5 days | 2 |
| 6 | Add `AuthContext::Worker` with org/capability binding | **HIGH** | 2-3 days | 2 |
| 7 | Org-scoped task polling | **HIGH** | 1 day | 2 |
| 8 | Remove `--service-token` CLI arg, use token file | **MEDIUM** | 1 day | 3 |
| 9 | Add `secrecy`/`zeroize` for credential memory | **MEDIUM** | 1 day | 3 |
| 10 | Session token rotation via heartbeat | **MEDIUM** | 2 days | 3 |
| 11 | ECDH key exchange for `credential_encryption_key` | **LOW** | 2-3 days | 3 |
| 12 | SCM credential narrowing (repo-scoped tokens) | **HIGH** | 3-5 days | 3 |
| 13 | Startup config validation (refuse without secrets) | **MEDIUM** | 1 day | 4 |
| 14 | Auth audit logging | **MEDIUM** | 1-2 days | 4 |
| 15 | E2E test-token runtime guard | **LOW** | 30 min | 4 |

---

## Appendix: Scope → Route Mapping (Target State)

```
workspaces:read →
  GET  /workspaces/:id
  GET  /workspaces/:id/stats
  GET  /workspaces/:id/dependencies
  GET  /workspaces/:id/branches
  GET  /workspaces/:id/members
  GET  /orgs/:orgId/workspaces
  GET  /branches/:branchId/files/**
  GET  /workspaces/:wid/ontology/**  (all GET)
  GET  /workspaces/:wid/docs/**  (all GET)
  GET  /workspaces/:wid/drafts
  GET  /drafts/:id
  GET  /drafts/:id/participants
  GET  /branches/:bid/drafts
  GET  /workspaces/:wid/comments/**  (all GET)
  GET  /workspaces/:wid/search
  GET  /resolve

workspaces:write →
  POST   /orgs/:orgId/workspaces
  PATCH  /workspaces/:id
  DELETE /workspaces/:id
  PUT    /workspaces/:id/dependencies
  POST   /workspaces/:id/branches
  DELETE /workspaces/:id/branches/:bid
  POST   /workspaces/:id/sync
  POST   /workspaces/:id/merge
  POST   /workspaces/:id/clone
  POST   /workspaces/:id/members
  DELETE /workspaces/:id/members/:uid
  POST   /workspaces/:wid/ontology
  DELETE /workspaces/:wid/ontology/:nid
  PUT    /workspaces/:wid/ontology/:nid/content
  POST   /workspaces/:wid/docs
  PUT    /workspaces/:wid/docs/:did
  PUT    /workspaces/:wid/docs/:did/links
  POST   /drafts/:id/commit
  POST   /workspaces/:wid/commit-all
  POST   /drafts/:id/discard
  POST   /branches/:bid/drafts
  POST   /workspaces/:wid/comments
  PATCH  /comments/:id
  DELETE /comments/:id
  POST   /comments/:id/replies
  PUT    /workspaces/:wid/settings

jobs:read →
  GET  /workspaces/:wid/jobs
  GET  /jobs/:id
  GET  /jobs/:id/tasks
  GET  /tasks/:id
  GET  /tasks/:id/timeline
  GET  /tasks/:id/diff
  GET  /tasks/:id/checkpoints

jobs:execute →
  POST /workspaces/:wid/jobs
  POST /jobs/:id/pause
  POST /jobs/:id/resume
  POST /jobs/:id/cancel
  POST /jobs/:id/rollback
  POST /tasks/:id/pause
  POST /tasks/:id/resume
  POST /tasks/:id/abort
  POST /tasks/:id/complete
  POST /tasks/:id/rollback

workers:read →
  GET /workspaces/:wid/workers
  GET /workers/:id

sessions:read →
  GET /workspaces/:wid/sessions
  GET /workspaces/:wid/sessions/search
  GET /sessions/:id
  GET /sessions/:id/messages
  GET /sessions/:id/timeline

sessions:write →
  POST /workspaces/:wid/sessions
  POST /sessions/:id/messages
  POST /sessions/:id/messages/:mid/stop
  POST /sessions/:id/resume
  POST /sessions/:id/fork
  POST /sessions/:id/summarize
  POST /sessions/:id/inject
  POST /sessions/:id/attach

org:admin →
  PATCH  /orgs/:orgId
  PATCH  /orgs/:orgId/members/:userId
  DELETE /orgs/:orgId/members/:userId
  GET    /orgs/:orgId/invitations
  POST   /invitations/:id/resend
  DELETE /orgs/:orgId/invitations/:id
  PUT    /orgs/:orgId/scopes
  PUT    /orgs/:orgId/scopes/members/:userId/scopes
  PUT    /workspaces/:wid/permissions/:userId
  DELETE /workspaces/:wid/permissions/:userId

billing:admin →
  GET /workspaces/:wid/settings/spend
  GET /workspaces/:wid/settings/spend/history
```
