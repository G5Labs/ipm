# G5 Security Hardening — In-Process Capability Architecture

> In-process authz and blob modules with clear API contracts, DB session
> capability injection, and RLS enforcement. Designed so that separating
> the minting service and blob service into independent processes is a
> mechanical refactor — the security model is proven in-process first.

**Design invariant (post-extraction)**: A user has zero ability to access
resources belonging to orgs they have no permission on — even if the API is
fully compromised — because minting keys, DB capability writes, and storage
credentials are isolated to separate processes.

> **In-process caveat**: In-process module boundaries prove the call contracts
> and eliminate accidental bypasses, but a root compromise of the g5-api
> process can access the in-memory signing key. Process separation (Phase 5)
> is required to achieve assume-breach containment.

---

## Table of Contents

1. [Architecture Overview](#1-architecture-overview)
2. [Phase 0 — Immediate Fixes](#2-phase-0)
3. [Phase 1 — In-Process Module Boundaries](#3-phase-1)
4. [Phase 2 — DB Session Capability + RLS](#4-phase-2)
5. [Phase 3 — org_id Denormalization](#5-phase-3)
6. [Phase 4 — Blob Capability Layer](#6-phase-4)
7. [Phase 5 — Mechanical Extraction](#7-phase-5)
8. [Security Proof at Each Phase](#8-security-proof)
9. [Implementation Plan](#9-implementation-plan)
10. [Appendix: Full Coupling Inventory](#10-appendix)

---

## 1. Architecture Overview {#1-architecture-overview}

### 1.1 In-Process Target State

```
┌─────────────────────────────────────────────────────────────────┐
│                         g5-api process                          │
│                                                                 │
│  ┌───────────────────┐                                          │
│  │   HTTP Layer       │                                         │
│  │   (axum routes)    │                                         │
│  └─────────┬─────────┘                                         │
│            │ AuthContext                                        │
│            ▼                                                    │
│  ┌───────────────────┐     ┌──────────────────────────────┐    │
│  │  Org Guard         │────▶│  g5-authz module              │    │
│  │  Middleware         │     │                               │    │
│  │  (verifies org     │     │  • Validates org membership    │    │
│  │   membership)      │     │  • Mints DbCapability          │    │
│  └───────────────────┘     │  • Mints BlobCapability         │    │
│            │                │  • Holds signing key (Arc)      │    │
│            ▼                │  • Own read-only pool for       │    │
│  ┌───────────────────┐     │    membership tables only       │    │
│  │  Service Layer     │     └──────────────────────────────┘    │
│  │  (17 services)     │                                         │
│  │                    │     ┌──────────────────────────────┐    │
│  │  receive:          │     │  g5-blob module               │    │
│  │  • ScopedDb (from  │     │                               │    │
│  │    DbCapability)   │     │  • Validates BlobCapability    │    │
│  │  • No raw PgPool   │     │  • Reads from RepoStorage     │    │
│  │    for tenant data │     │  • Holds storage credentials   │    │
│  └───────────────────┘     │  • NO access to PgPool         │    │
│            │                └──────────────────────────────┘    │
│            ▼                                                    │
│  ┌───────────────────┐                                         │
│  │  Postgres          │  RLS enforced on every query            │
│  │  (non-owner role)  │  authz.session_ctx per connection       │
│  └───────────────────┘                                         │
└─────────────────────────────────────────────────────────────────┘
```

### 1.2 Module Isolation Rules

These rules are enforced by Rust's module visibility, `#[deny(unused)]`, and code review:

| Module | Can access | Cannot access |
|--------|-----------|---------------|
| `g5-authz` | Signing key, org membership queries (own read-only pool for `org_memberships` table) | `PgPool` (tenant data pool), service layer, RepoStorage |
| `g5-blob` | `RepoStorage`, blob signing verification key | `PgPool`, service layer, signing private key |
| Service layer | `ScopedDb` (from `DbCapability`), `BlobClient` trait | Raw `PgPool` for tenant tables, signing key, storage creds |
| Route handlers | `AuthContext`, `AuthzClient` trait | `PgPool`, signing key, storage creds |

When running in-process, these boundaries are **Rust module visibility** boundaries.
When extracted to separate services, they become **network boundaries** — the contracts
are identical.

> **Clarification**: "no access to PgPool" means the authz module must not use the
> tenant-data pool. `InProcessAuthz` holds its own `membership_pool: PgPool` with
> read-only access to `org_memberships` only. This pool is distinct from the tenant
> data pool used by services via `ScopedDb`.

### 1.3 Why In-Process First

| Concern | In-Process | Separate Service |
|---------|-----------|-----------------|
| Latency | Zero (function call) | ~0.5ms (loopback HTTP) |
| Deployment complexity | One binary | Three binaries, three deployments |
| Security boundary | Rust modules (weaker) | Process isolation (stronger) |
| Contract validation | Compile-time (trait bounds) | Runtime (HTTP schema) |
| Blast radius under compromise | Same process = same blast | Independent compromise needed |

The in-process phase **proves the contracts work** — every service gets a `ScopedDb`
instead of a raw `PgPool`, every file read goes through a `BlobCapability`, and RLS
is enforced at the DB level. Extraction to separate services changes the transport,
not the security model.

---

## 2. Phase 0 — Immediate Fixes (Pre-Requisite) {#2-phase-0}

These are critical vulnerabilities in the current codebase that must be fixed before
any architectural work. They are independent, parallelizable, and each is a 1-day task.

### 2.1 Org Guard Middleware

**Problem**: 34 of 47 HTTP handlers accept `auth: AuthContext` but never verify that
the user is a member of the org referenced by `:orgId` in the path. A user in org A
can access org B's resources by manipulating the URL.

**Fix**: Add an `OrgGuard` middleware layer applied to all `/api/v1/orgs/:orgId/*` routes.
The guard verifies **org membership** (not JWT equality) because users can belong to
multiple orgs — the JWT `org` field represents the "active org" at login time only.

```rust
// g5-api/src/middleware/org_guard.rs

use axum::{extract::Path, http::Request, middleware::Next, response::Response};

/// Middleware that verifies the user is a member of the `:orgId` org.
/// Uses a DB or Redis-cached lookup — NOT a JWT equality check, because
/// users can belong to multiple organizations.
pub async fn org_guard<B>(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(params): Path<OrgPathParams>,
    request: Request<B>,
    next: Next<B>,
) -> Result<Response, AppError> {
    match &auth {
        AuthContext::User(claims) => {
            let user_id = Uuid::parse_str(&claims.sub)
                .map_err(|_| AppError::internal("invalid user ID in JWT"))?;

            // Membership check — DB query (or Redis-cached for hot path)
            let is_member = state.authz
                .is_org_member(user_id, params.org_id)
                .await
                .map_err(|_| AppError::internal("membership check failed"))?;

            if !is_member {
                return Err(AppError::forbidden("not a member of this org"));
            }
        }
        AuthContext::Service => {
            // Service tokens on internal routes are OK.
            // Service tokens on public /api/v1/ routes get synthetic claims
            // which will never match a real org_id — this is correct behavior.
        }
    }
    Ok(next.run(request).await)
}
```

For routes not under `/orgs/:orgId/` (e.g., `/api/v1/workspaces/:id`,
`/api/v1/jobs/:jobId`), add a **resource-scoped guard** that looks up the resource's
`org_id` and verifies the user is a member of that org:

```rust
// Applied to /api/v1/workspaces/:id/* routes
pub async fn workspace_org_guard<B>(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(workspace_id): Path<Uuid>,
    request: Request<B>,
    next: Next<B>,
) -> Result<Response, AppError> {
    let user_id = extract_user_id(&auth)?;
    let workspace_org = state.workspace_service
        .get_workspace_org_id(workspace_id).await?;

    // Membership check — same as org_guard
    let is_member = state.authz
        .is_org_member(user_id, workspace_org)
        .await
        .map_err(|_| AppError::internal("membership check failed"))?;

    if !is_member {
        return Err(AppError::forbidden("workspace belongs to an org you are not a member of"));
    }
    Ok(next.run(request).await)
}
```

**Application**: In `routes/mod.rs`, wrap route groups:

```rust
// Before
.nest("/api/v1/orgs/:orgId/workspaces", workspaces::org_workspaces_router())

// After
.nest("/api/v1/orgs/:orgId/workspaces",
    workspaces::org_workspaces_router()
        .layer(axum::middleware::from_fn(org_guard)))
```

### 2.2 Apply RequireScopes

**Problem**: `RequireScopes` middleware exists but is applied to zero routes.

**Fix**: Apply to write endpoints immediately:

```rust
pub fn workspace_detail_router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/:id", get(get_workspace))  // read — no scope required
        .route("/:id",
            patch(update_workspace)
                .delete(delete_workspace)
                .layer(RequireScopes::new(&["workspaces:write"])))
        .route("/:id/clone",
            post(clone_workspace)
                .layer(RequireScopes::new(&["workspaces:write"])))
    // ...
}
```

### 2.3 JWT `iss`/`aud` Validation

**Problem**: No issuer or audience validation. A JWT from staging works in production.

**Fix**: In `g5-common/src/auth/jwt.rs`, add to `JwtValidator::new()`:

```rust
validation.set_issuer(&["g5-api"]);
validation.set_audience(&["g5-api"]);
validation.required_spec_claims = HashSet::from(["exp", "sub", "org", "iss", "aud"]);
```

Add `iss` and `aud` to `JwtClaims`:

```rust
pub struct JwtClaims {
    pub sub: String,
    pub org: String,
    pub scopes: Vec<String>,
    pub iss: String,      // NEW
    pub aud: Vec<String>,  // NEW
    pub iat: i64,
    pub exp: i64,
}
```

### 2.4 Service Token Scope Restriction

**Problem**: Service tokens on public `/api/v1/` routes get synthetic admin claims
with `i64::MAX` expiration and all scopes.

**Fix**: Service tokens should only work on `/api/internal/` routes. Remove the
`service_claims()` synthetic user path:

```rust
// auth_middleware.rs — service token on /api/v1/ routes
// Before: AuthContext::User(service_claims())
// After:
return Err(AppError::forbidden("service tokens not accepted on public routes"));
```

Workers that need public-route access should use a proper JWT minted for the
specific workspace/org context.

### 2.5 Consolidate `extract_user_id`

**Problem**: 7 independent copies of `extract_user_id` across route files.

**Fix**: Single canonical implementation on `AuthContext`:

```rust
// g5-api/src/middleware/auth_middleware.rs
impl AuthContext {
    pub fn require_user_id(&self) -> Result<Uuid, AppError> {
        match self {
            Self::User(claims) => Uuid::parse_str(&claims.sub)
                .map_err(|_| AppError::internal("invalid user ID in JWT")),
            Self::Service => Err(AppError::forbidden("user auth required")),
        }
    }

    pub fn require_org_id(&self) -> Result<Uuid, AppError> {
        match self {
            Self::User(claims) => Uuid::parse_str(&claims.org)
                .map_err(|_| AppError::internal("invalid org ID in JWT")),
            Self::Service => Err(AppError::forbidden("user auth required")),
        }
    }
}
```

Delete all 7 copies of `extract_user_id` and the 1 copy of `extract_org_id`.

---

## 3. Phase 1 — In-Process Module Boundaries {#3-phase-1}

### 3.1 Crate Structure

Create two new crates in the workspace:

```
generation/
  g5-authz/         ← NEW crate
    Cargo.toml
    src/
      lib.rs         ← AuthzService trait + InProcessAuthz implementation
      capability.rs  ← DbCapability, BlobCapability structs
      signing.rs     ← Capability signing/verification
  g5-blob/           ← NEW crate
    Cargo.toml
    src/
      lib.rs         ← BlobService trait + InProcessBlob implementation
      capability.rs  ← BlobCapability verification
```

**Dependency rules** (enforced by Cargo.toml):

| Crate | May depend on | Must NOT depend on |
|-------|--------------|-------------------|
| `g5-authz` | `g5-common` (types, errors) | `g5-api`, `g5-blob`, `sqlx` (app pool) |
| `g5-blob` | `g5-common` (types, errors, RepoStorage) | `g5-api`, `g5-authz`, `sqlx` |
| `g5-api` | `g5-authz`, `g5-blob`, `g5-common`, `sqlx` | — |
| `g5-common` | — | `g5-api`, `g5-authz`, `g5-blob` |

The crate boundary means `g5-authz` **cannot** import `PgPool` from the application,
and `g5-blob` **cannot** import the signing key. These are compile-time guarantees.

### 3.2 AuthzService Trait

```rust
// g5-authz/src/lib.rs

use g5_common::auth::JwtClaims;
use uuid::Uuid;

/// Capability to execute queries scoped to a single org.
/// Contains the signed context needed for DB session injection.
#[derive(Debug, Clone)]
pub struct DbCapability {
    /// The org this capability is scoped to.
    pub org_id: Uuid,
    /// The user who requested this capability.
    pub user_id: Uuid,
    /// Effective scopes for this session.
    pub scopes: Vec<String>,
    /// Opaque signed token consumed by authz.set_ctx().
    /// In-process: plaintext. Extracted: KMS-signed JWT.
    pub ctx_token: String,
    /// Expiry (seconds from now). Must be short-lived.
    pub ttl_secs: u32,
}

/// Capability to read a specific blob.
///
/// Uses content-addressed blob_sha instead of path to avoid TOCTOU bugs
/// (rename/move races, path normalization issues, wildcard semantics).
/// Path → blob_sha resolution happens in the DB metadata layer before
/// minting the capability.
#[derive(Debug, Clone)]
pub struct BlobCapability {
    pub org_id: Uuid,
    pub repo_id: Uuid,
    /// Content-addressed blob SHA (from DB metadata, not filesystem path).
    pub blob_sha: String,
    /// Maximum allowed read size in bytes (prevents abuse).
    pub max_bytes: u64,
    /// Signed token consumed by the blob service.
    pub token: String,
    pub ttl_secs: u32,
}

/// Authorization service contract.
///
/// In-process: function calls.
/// Extracted: HTTP calls to g5-authz service.
#[async_trait]
pub trait AuthzService: Send + Sync + 'static {
    /// Check if a user is a member of an org.
    /// Used by OrgGuard middleware before minting any capabilities.
    /// Implementation may use DB query or Redis-cached lookup.
    async fn is_org_member(
        &self,
        user_id: Uuid,
        org_id: Uuid,
    ) -> Result<bool, AuthzError>;

    /// Mint a DB capability for the given user + org.
    /// Verifies org membership before minting.
    async fn mint_db_capability(
        &self,
        claims: &JwtClaims,
        target_org_id: Uuid,
    ) -> Result<DbCapability, AuthzError>;

    /// Mint a blob read capability for a specific blob.
    /// Resolves path → blob_sha via DB metadata before minting.
    async fn mint_blob_capability(
        &self,
        claims: &JwtClaims,
        org_id: Uuid,
        repo_id: Uuid,
        blob_sha: &str,
        max_bytes: u64,
    ) -> Result<BlobCapability, AuthzError>;

    /// Verify a DB capability token (called by set_ctx function).
    /// In-process: decode and validate.
    /// Extracted: the authz service mints it, the DB function validates it.
    fn verify_db_capability(&self, token: &str) -> Result<DbCapability, AuthzError>;

    /// Verify a blob capability token.
    fn verify_blob_capability(&self, token: &str) -> Result<BlobCapability, AuthzError>;
}

#[derive(Debug, thiserror::Error)]
pub enum AuthzError {
    #[error("not a member of org {0}")]
    NotMember(Uuid),
    #[error("insufficient scopes: missing {0:?}")]
    InsufficientScopes(Vec<String>),
    #[error("capability expired")]
    Expired,
    #[error("invalid capability signature")]
    InvalidSignature,
    #[error("internal: {0}")]
    Internal(String),
}
```

### 3.3 In-Process AuthzService Implementation

```rust
// g5-authz/src/lib.rs

/// In-process implementation. Validates org membership against its own
/// read-only connection (or the authz schema), mints capabilities by
/// signing with an in-memory key.
///
/// When extracted to a separate service, this struct moves into the
/// g5-authz binary. g5-api gets an HTTP client implementation of the
/// same trait.
pub struct InProcessAuthz {
    /// Read-only DB pool — only has SELECT on org_memberships.
    /// In the extracted version, this is the authz service's own DB.
    membership_pool: PgPool,
    /// HMAC-SHA256 key for signing capabilities.
    /// In the extracted version, this is a KMS reference.
    signing_key: hmac::Key,
}

#[async_trait]
impl AuthzService for InProcessAuthz {
    async fn mint_db_capability(
        &self,
        claims: &JwtClaims,
        target_org_id: Uuid,
    ) -> Result<DbCapability, AuthzError> {
        let user_id = Uuid::parse_str(&claims.sub)
            .map_err(|_| AuthzError::Internal("bad sub".into()))?;

        // 1. Verify org membership
        let is_member = sqlx::query_scalar!(
            "SELECT EXISTS(
                SELECT 1 FROM org_memberships
                WHERE org_id = $1 AND user_id = $2
            ) as \"exists!\"",
            target_org_id,
            user_id,
        )
        .fetch_one(&self.membership_pool)
        .await
        .map_err(|e| AuthzError::Internal(e.to_string()))?;

        if !is_member {
            return Err(AuthzError::NotMember(target_org_id));
        }

        // 2. Mint signed capability
        let ttl_secs = 300; // 5 minutes
        let payload = CapabilityPayload {
            typ: "db",
            org_id: target_org_id,
            user_id,
            scopes: claims.scopes.clone(),
            exp: chrono::Utc::now().timestamp() + ttl_secs as i64,
            jti: Uuid::new_v4(),
        };
        let token = self.sign(&payload)?;

        Ok(DbCapability {
            org_id: target_org_id,
            user_id,
            scopes: claims.scopes.clone(),
            ctx_token: token,
            ttl_secs,
        })
    }

    // ... mint_blob_capability, verify_db_capability, verify_blob_capability
}
```

### 3.4 BlobService Trait

```rust
// g5-blob/src/lib.rs

use bytes::Bytes;

/// Blob service contract.
///
/// In-process: reads from RepoStorage directly.
/// Extracted: HTTP calls to g5-blob service.
#[async_trait]
pub trait BlobService: Send + Sync + 'static {
    /// Read blob content by SHA, validating the blob capability.
    async fn read_blob(
        &self,
        capability: &BlobCapability,
    ) -> Result<Bytes, BlobError>;

    /// List file tree, validating the blob capability.
    async fn list_tree(
        &self,
        capability: &BlobCapability,
        path: &str,
        depth: u32,
    ) -> Result<Vec<StorageEntry>, BlobError>;

    /// Read file content for internal use (git diff, draft commit).
    /// Requires a DbCapability to prove org membership (not a blob cap
    /// because git operations need filesystem access, not blob-level access).
    async fn read_file_internal(
        &self,
        db_cap: &DbCapability,
        repo_id: &str,
        path: &str,
    ) -> Result<Bytes, BlobError>;
}

#[derive(Debug, thiserror::Error)]
pub enum BlobError {
    #[error("invalid capability: {0}")]
    InvalidCapability(String),
    #[error("capability expired")]
    Expired,
    #[error("file not found: {0}")]
    NotFound(String),
    #[error("org/repo mismatch")]
    TenantMismatch,
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
}
```

### 3.5 In-Process BlobService Implementation

```rust
// g5-blob/src/lib.rs

pub struct InProcessBlob {
    storage: Arc<dyn RepoStorage>,
    /// Public key to verify blob capabilities.
    /// In-process: corresponds to InProcessAuthz's signing key.
    /// Extracted: loaded from config, does not have the private key.
    verify_key: hmac::Key,
}

#[async_trait]
impl BlobService for InProcessBlob {
    async fn read_blob(
        &self,
        capability: &BlobCapability,
    ) -> Result<Bytes, BlobError> {
        // 1. Verify capability signature + expiry
        self.verify_capability(capability)?;

        // 2. Read from storage by blob SHA (content-addressed)
        //    No path normalization needed — TOCTOU-free

        // 3. Read from storage (scoped to capability's repo + blob SHA)
        self.storage
            .read_blob(&capability.repo_id.to_string(), &capability.blob_sha)
            .await
            .map_err(|e| match e {
                StorageError::NotFound(p) => BlobError::NotFound(p),
                other => BlobError::Io(std::io::Error::other(other.to_string())),
            })
    }
    // ...
}
```

### 3.6 Injection into AppState

```rust
// g5-api/src/services/mod.rs — AFTER

pub struct AppState {
    // Authorization — trait object, swappable
    pub authz: Arc<dyn AuthzService>,
    // Blob access — trait object, swappable
    pub blob: Arc<dyn BlobService>,

    // Services — receive ScopedDb per request, not PgPool
    pub workspace_service: WorkspaceService,
    pub ontology_service: OntologyService,
    // ... (unchanged)

    // Raw pool for non-tenant operations (migrations, health checks, global queries)
    pub system_db: PgPool,

    // NOT available to services for tenant data:
    // pub db: PgPool,  // REMOVED from public interface
}
```

---

## 4. Phase 2 — DB Session Capability + RLS {#4-phase-2}

### 4.1 The ScopedDb Abstraction

This is the core contract that replaces `PgPool` for all tenant-scoped queries.

```rust
// g5-common/src/db/scoped.rs

/// A database connection that has been scoped to a specific org via
/// authz.session_ctx. All queries through this connection are filtered
/// by RLS policies.
///
/// Cannot be constructed directly — only via `ScopedDb::from_capability()`.
///
/// On Drop, calls `authz.clear_ctx()` to remove the session context —
/// critical for connection pool safety (prevents context leaking to the
/// next borrower of a pooled connection).
pub struct ScopedDb<'a> {
    tx: sqlx::Transaction<'a, sqlx::Postgres>,
    org_id: Uuid,
    user_id: Uuid,
}

impl ScopedDb<'_> {
    /// Create a scoped DB connection from a DbCapability.
    ///
    /// 1. Begins a transaction
    /// 2. Calls authz.set_ctx() which validates the capability and writes
    ///    a row to authz.session_ctx keyed by pg_backend_pid()
    /// 3. RLS policies read org_id via authz.current_org_id()
    /// 4. Returns a ScopedDb that can only be used within this transaction
    ///
    /// The calling code CANNOT set session context directly because:
    /// - authz.session_ctx INSERT/UPDATE requires the authz_ctx_writer role
    /// - g5_app can only EXECUTE authz.set_ctx() (SECURITY DEFINER)
    /// - g5_app has NO INSERT/UPDATE/DELETE on authz.session_ctx
    pub async fn from_capability(
        pool: &PgPool,
        capability: &DbCapability,
    ) -> Result<Self, G5Error> {
        let mut tx = pool.begin().await?;

        // This is the ONLY way to set the tenant context.
        // The function validates the capability token internally.
        sqlx::query("SELECT authz.set_ctx($1)")
            .bind(capability.ctx_token)  // UUID, not text
            .execute(&mut *tx)
            .await
            .map_err(|_| G5Error::Unauthorized)?;

        Ok(Self {
            tx,
            org_id: capability.org_id,
            user_id: capability.user_id,
        })
    }

    /// Get a reference to the transaction for executing queries.
    /// All queries will be filtered by RLS.
    pub fn tx(&mut self) -> &mut sqlx::Transaction<'_, sqlx::Postgres> {
        &mut self.tx
    }

    pub fn org_id(&self) -> Uuid {
        self.org_id
    }

    pub fn user_id(&self) -> Uuid {
        self.user_id
    }

    /// Commit the transaction. Automatically clears session context.
    pub async fn commit(mut self) -> Result<(), G5Error> {
        // Clear context before commit — defense in depth
        let _ = sqlx::query("SELECT authz.clear_ctx()")
            .execute(&mut *self.tx)
            .await;
        self.tx.commit().await.map_err(|e| G5Error::Internal {
            message: format!("commit failed: {e}"),
        })
    }
}

impl Drop for ScopedDb<'_> {
    fn drop(&mut self) {
        // Best-effort clear on drop. If the transaction is still open,
        // the rollback will also discard the session_ctx row (since
        // set_ctx is called within the transaction scope). For extra
        // safety with connection pooling, we also call clear_ctx.
        //
        // Note: async Drop isn't available in Rust, so this uses
        // spawn_blocking or is a no-op if the tx is already consumed.
        // In practice, the clear_ctx in commit() handles the happy path,
        // and transaction rollback handles the error path.
    }
}
```

### 4.2 The `authz.session_ctx` Table + SECURITY DEFINER Functions

> **Why not GUCs?** Any Postgres role can call `SET LOCAL app.org_id` —
> custom GUCs have no permission model. The original design's claim that
> "g5_app cannot call SET LOCAL directly" was incorrect. Instead, we use
> a `session_ctx` table keyed by `pg_backend_pid()`, where writes require
> the `authz_ctx_writer` role that `g5_app` does not have.

```sql
-- Migration: XXX_authz_session_capability.sql

-- 1. Create the authz schema (owned by a privileged role, not g5_app)
CREATE SCHEMA IF NOT EXISTS authz;

-- 2. Session context table — replaces GUC-based SET LOCAL.
--    Keyed by pg_backend_pid() so each connection gets exactly one context.
--    Bounded by connection pool size (~200 rows max).
CREATE TABLE authz.session_ctx (
    pid        INT PRIMARY KEY DEFAULT pg_backend_pid(),
    org_id     UUID NOT NULL,
    user_id    UUID NOT NULL,
    scopes     TEXT[] NOT NULL,
    set_at     TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- REVOKE all direct access from g5_app
REVOKE ALL ON authz.session_ctx FROM g5_app;

-- 3. Capability validation table (populated by the authz module)
CREATE TABLE authz.active_capabilities (
    ctx_id     UUID PRIMARY KEY,
    org_id     UUID NOT NULL,
    user_id    UUID NOT NULL,
    scopes     TEXT[] NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Index for cleanup
CREATE INDEX idx_cap_expires ON authz.active_capabilities(expires_at);

-- 4. set_ctx() — SECURITY DEFINER function.
--    Validates the capability token, then writes to session_ctx.
--    Runs as authz_ctx_writer role which has INSERT/UPDATE on session_ctx.
--    g5_app can EXECUTE this function, but cannot write session_ctx directly.
CREATE OR REPLACE FUNCTION authz.set_ctx(p_ctx_token UUID)
RETURNS VOID
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = authz, public
AS $$
DECLARE
    v_cap authz.active_capabilities%ROWTYPE;
BEGIN
    -- Look up the capability
    SELECT * INTO v_cap
    FROM authz.active_capabilities
    WHERE ctx_id = p_ctx_token
      AND expires_at > now();

    IF NOT FOUND THEN
        RAISE EXCEPTION 'invalid or expired capability: %', p_ctx_token
            USING ERRCODE = '28000';  -- invalid_authorization_specification
    END IF;

    -- Write session context (upsert — handles connection reuse)
    INSERT INTO authz.session_ctx (pid, org_id, user_id, scopes)
    VALUES (pg_backend_pid(), v_cap.org_id, v_cap.user_id, v_cap.scopes)
    ON CONFLICT (pid) DO UPDATE SET
        org_id  = EXCLUDED.org_id,
        user_id = EXCLUDED.user_id,
        scopes  = EXCLUDED.scopes,
        set_at  = now();

    -- Delete consumed capability (single-use)
    DELETE FROM authz.active_capabilities WHERE ctx_id = v_cap.ctx_id;
END;
$$;

-- 5. current_org_id() — called by RLS policies.
--    STABLE + SECURITY DEFINER so it can read session_ctx.
--    Returns the org_id for the current connection's backend PID.
CREATE OR REPLACE FUNCTION authz.current_org_id()
RETURNS UUID
LANGUAGE sql
STABLE                -- optimizer hint: stable within a statement
SECURITY DEFINER      -- runs as owner (can read session_ctx)
PARALLEL RESTRICTED   -- must run on leader; parallel workers have different PIDs
SET search_path = authz
AS $$
    SELECT org_id FROM authz.session_ctx WHERE pid = pg_backend_pid();
$$;

-- 6. clear_ctx() — called on transaction commit/rollback.
--    Critical for connection pool safety: prevents context from leaking
--    to the next borrower of a pooled connection.
CREATE OR REPLACE FUNCTION authz.clear_ctx()
RETURNS VOID
LANGUAGE sql
SECURITY DEFINER
SET search_path = authz
AS $$
    DELETE FROM authz.session_ctx WHERE pid = pg_backend_pid();
$$;

-- 7. Grants
GRANT USAGE ON SCHEMA authz TO g5_app;
GRANT EXECUTE ON FUNCTION authz.set_ctx(UUID) TO g5_app;
GRANT EXECUTE ON FUNCTION authz.current_org_id() TO g5_app;
GRANT EXECUTE ON FUNCTION authz.clear_ctx() TO g5_app;

-- g5_app has NO direct access to authz.session_ctx or active_capabilities
-- (the SECURITY DEFINER functions run as the function owner, not the caller)

-- 8. The authz module writes capabilities using a DIFFERENT role
--    (authz_writer) that has INSERT on active_capabilities.
--    In-process: uses a separate connection with authz_writer role.
--    Extracted: the authz service has its own DB credentials.
GRANT INSERT, DELETE ON authz.active_capabilities TO authz_writer;

-- 9. The set_ctx function needs INSERT/UPDATE on session_ctx.
--    It runs as its owner (authz_ctx_writer role), not as g5_app.
GRANT INSERT, UPDATE, DELETE ON authz.session_ctx TO authz_ctx_writer;
```

### 4.3 Performance Characteristics

The `session_ctx` lookup (`authz.current_org_id()`) is called by every RLS policy
on every query. This is performant because:

- **STABLE → evaluated once per statement, not per row.** Postgres treats the
  function result as constant within a single statement. Without STABLE, the
  planner may re-evaluate per row — that's the real performance hazard, and
  it's eliminated by this annotation.
- **PK-indexed**: `session_ctx.pid` is the primary key — the lookup is an index-only
  scan on a tiny table. Cost: microseconds.
- **Bounded by pool size, not by tenants or QPS.** The table has at most
  `pool_size` rows (typically 50–500). It fits entirely in shared buffers, vacuum
  is trivial, and the table will never grow with load.
- **PARALLEL RESTRICTED**: `current_org_id()` is marked `PARALLEL RESTRICTED` so
  it always runs on the leader backend. Parallel workers have different PIDs and
  would get the wrong (or NULL) context. This is the safe default for OLTP SaaS
  queries. For analytical queries that genuinely need parallelism, consider
  disabling parallel query for tenant-scoped sessions instead.

**Transaction safety**: The `INSERT/UPSERT` into `session_ctx` happens inside
the transaction started by `ScopedDb::from_capability()`. If the transaction
rolls back, the session_ctx write is also rolled back — context cannot leak
via partial failures. The explicit `clear_ctx()` in `commit()` is
defense-in-depth for connection pool hygiene: it ensures no stale context
survives pool reuse even under unusual connection lifecycle edge cases.

**PgBouncer / transaction pooling**: In transaction mode, the backend PID stays
constant for the duration of a transaction. `set_ctx()` at transaction start +
`clear_ctx()` at commit/rollback works correctly. In session mode, the PID is
stable for the entire client session.

**Periodic cleanup (optional)**: In a correctly-scoped transaction model,
`session_ctx` should not accumulate orphaned rows. For defense-in-depth:

```sql
-- Optional: periodic cleanup of orphaned session context rows
-- Run from a maintenance cron, not from the hot path
DELETE FROM authz.session_ctx
WHERE set_at < now() - interval '1 hour';
```

### 4.4 Why g5-api Cannot Bypass This

Even with full code execution in the g5-api process, the attacker has the `g5_app`
DB role which:

1. **Cannot write to `authz.session_ctx` directly** — `g5_app` has no INSERT/UPDATE/DELETE
   on the table. Only the `authz_ctx_writer` role (used by the SECURITY DEFINER functions)
   can write to it.

2. **Cannot INSERT into `authz.active_capabilities`** — only `authz_writer` role can.

3. **Cannot modify the `authz.set_ctx()` function** — not the owner, no `ALTER FUNCTION`.

4. **Cannot disable RLS** — not `SUPERUSER`, not `BYPASSRLS`, not the table owner.

5. **Cannot read other tenants' rows** — RLS filters on `authz.current_org_id()`
   which reads from `session_ctx` (the attacker cannot write arbitrary rows there).

> **In-process caveat**: In-process, a root compromise of the g5-api process gives
> access to the `authz_writer` connection pool (it lives in the same memory space).
> The `session_ctx` model prevents *accidental* bypasses and enforces correct call
> contracts. True assume-breach isolation requires Phase 5 (process extraction).

The attacker's only option is to:
- Use existing valid capability tokens (short-lived, 5 minutes)
- Call `authz.set_ctx()` with a guessed UUID (128-bit random, infeasible)
- Compromise the authz_writer credentials (in-process: same memory space;
  extracted: separate process with separate secrets)

### 4.5 DB Role Setup

```sql
-- Migration: XXX_db_roles.sql

-- 1. Application role (used by g5-api service layer)
CREATE ROLE g5_app LOGIN PASSWORD '...';

-- Read/write on public schema tables, but with RLS enforced
GRANT USAGE ON SCHEMA public TO g5_app;
GRANT SELECT, INSERT, UPDATE, DELETE ON ALL TABLES IN SCHEMA public TO g5_app;
GRANT USAGE, SELECT ON ALL SEQUENCES IN SCHEMA public TO g5_app;
ALTER DEFAULT PRIVILEGES IN SCHEMA public
    GRANT SELECT, INSERT, UPDATE, DELETE ON TABLES TO g5_app;

-- CRITICAL: g5_app is NOT the table owner → cannot disable RLS
-- RLS is FORCE-enabled: applies even to table owner via FORCE flag,
-- but g5_app also lacks ALTER TABLE permission

-- g5_app can call SECURITY DEFINER functions in authz schema
-- but has NO direct access to authz.session_ctx or authz.active_capabilities

-- 2. Authz writer role (used by g5-authz module — separate connection)
--    Writes capabilities that set_ctx() will later consume.
CREATE ROLE authz_writer LOGIN PASSWORD '...';
GRANT USAGE ON SCHEMA authz TO authz_writer;
GRANT INSERT, DELETE ON authz.active_capabilities TO authz_writer;
-- authz_writer has NO access to public schema tables

-- 3. Authz context writer role (owns set_ctx/clear_ctx SECURITY DEFINER functions)
--    This role can write to authz.session_ctx. Functions run as this role.
--    Neither g5_app nor authz_writer can write session_ctx directly.
CREATE ROLE authz_ctx_writer NOLOGIN;
GRANT USAGE ON SCHEMA authz TO authz_ctx_writer;
GRANT SELECT ON authz.active_capabilities TO authz_ctx_writer;
GRANT DELETE ON authz.active_capabilities TO authz_ctx_writer;
GRANT INSERT, UPDATE, DELETE ON authz.session_ctx TO authz_ctx_writer;
GRANT SELECT ON authz.session_ctx TO authz_ctx_writer;
-- set_ctx(), current_org_id(), clear_ctx() are owned by authz_ctx_writer

-- 4. System/migration role (existing superuser — for DDL only, never in app code)
-- This is the current DB credential — used only for migrations
```

### 4.6 How Services Change

Before:
```rust
impl WorkspaceService {
    pub async fn list_workspaces(&self, org_id: Uuid, ...) -> Result<...> {
        sqlx::query(...)
            .bind(org_id)
            .fetch_all(&self.db)  // raw pool, no RLS
            .await
    }
}
```

After:
```rust
impl WorkspaceService {
    pub async fn list_workspaces(&self, db: &mut ScopedDb, ...) -> Result<...> {
        // org_id filter is now IMPLICIT via RLS — no need to bind it
        sqlx::query("SELECT * FROM workspaces WHERE status = ANY($1)")
            .bind(statuses)
            .fetch_all(db.tx())  // RLS-enforced connection
            .await
    }
}
```

Key changes:
- Services **no longer hold `PgPool`** for tenant data
- Services **receive `ScopedDb`** per-request from the route handler
- `WHERE org_id = $1` clauses become **unnecessary** (RLS handles it)
- But keeping explicit `org_id` filters is fine — defense-in-depth, and the query planner
  will combine them with the RLS predicate

### 4.7 Route Handler Flow

```rust
async fn list_workspaces(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(params): Path<OrgPathParams>,
    Query(pagination): Query<Pagination>,
) -> Result<Json<ResponseEnvelope<Vec<WorkspaceListItemDTO>>>, AppError> {
    // 1. Extract claims
    let claims = auth.require_claims()?;

    // 2. Mint DB capability (verifies org membership)
    let db_cap = state.authz
        .mint_db_capability(claims, params.org_id)
        .await
        .map_err(AppError::from)?;

    // 3. Create scoped DB connection
    let mut scoped = ScopedDb::from_capability(&state.tenant_pool, &db_cap).await?;

    // 4. Call service with scoped connection
    let result = state.workspace_service
        .list_workspaces(&mut scoped, &pagination_params(&pagination))
        .await?;

    // 5. Commit (or drop for read-only — implicit rollback is fine)
    // scoped is dropped here, transaction ends

    Ok(Json(ResponseEnvelope::paginated(...)))
}
```

### 4.8 Capability Lifecycle

```
Request arrives
  │
  ▼
auth_middleware validates JWT
  │
  ▼
org_guard verifies org membership (DB/cache lookup, NOT jwt equality)
  │
  ▼
handler calls authz.mint_db_capability(claims, org_id)
  │
  ├─ authz verifies org_memberships (own DB query)
  ├─ authz generates UUID ctx_token
  ├─ authz INSERTs into authz.active_capabilities (authz_writer conn)
  └─ returns DbCapability { org_id, user_id, ctx_token, ttl: 300s }
  │
  ▼
handler calls ScopedDb::from_capability(pool, &cap)
  │
  ├─ pool.begin() — starts transaction on g5_app role
  ├─ SELECT authz.set_ctx(ctx_token)
  │    ├─ SECURITY DEFINER — runs as authz_ctx_writer role
  │    ├─ Looks up ctx_token in authz.active_capabilities
  │    ├─ Validates expiry
  │    ├─ INSERT/UPSERT into authz.session_ctx (pid, org_id, user_id, scopes)
  │    └─ DELETEs consumed capability (single-use)
  └─ returns ScopedDb (transaction with RLS active via session_ctx)
  │
  ▼
service executes queries — RLS calls authz.current_org_id() per query
  │ (STABLE function, evaluated once per query, PK lookup on pid)
  │
  ▼
ScopedDb::commit() calls authz.clear_ctx() → deletes session_ctx row
  Transaction commits. Connection returned to pool with no context.
  (On error: ScopedDb::drop() → transaction rolls back,
   clear_ctx called for pool safety)
```

---

## 5. Phase 3 — org_id Denormalization {#5-phase-3}

### 5.1 Tables Needing org_id

RLS policies require a direct `org_id` column. These 14 tables need it added:

| Table | Current path to org | Join depth |
|-------|-------------------|-----------|
| `workspace_branches` | → `workspaces.org_id` | 1 |
| `workspace_dependencies` | → `workspaces.org_id` | 1 |
| `workspace_members` | → `workspaces.org_id` | 1 |
| `workspace_settings` | → `workspaces.org_id` | 1 |
| `ontology_nodes` | → `workspace_branches` → `workspaces.org_id` | 2 |
| `node_dependencies` | → `ontology_nodes` → ... | 3 |
| `docs` | → `workspaces.org_id` | 1 |
| `doc_node_links` | → `docs` → ... | 2 |
| `draft_sessions` | → `workspace_branches` → ... | 2 |
| `draft_updates` | → `draft_sessions` → ... | 3 |
| `jobs` | → `workspaces.org_id` | 1 |
| `tasks` | → `jobs` → `workspaces.org_id` | 2 |
| `timeline_entries` | → `tasks` → `jobs` → ... | 3 |
| `task_checkpoints` | → `tasks` → ... | 3 |
| `chat_sessions` | → `workspaces.org_id` | 1 |
| `chat_messages` | → `chat_sessions` → ... | 2 |
| `comment_index` | → `workspaces.org_id` | 1 |
| `comment_replies` | → `comment_index` → ... | 2 |
| `search_documents` | → `workspace_branches` → ... | 2 |

### 5.2 Migration Strategy

Single migration, executed in a maintenance window:

```sql
-- Migration: XXX_add_org_id_for_rls.sql

-- Step 1: Add nullable org_id columns
ALTER TABLE workspace_branches ADD COLUMN org_id UUID;
ALTER TABLE jobs ADD COLUMN org_id UUID;
ALTER TABLE tasks ADD COLUMN org_id UUID;
ALTER TABLE ontology_nodes ADD COLUMN org_id UUID;
ALTER TABLE docs ADD COLUMN org_id UUID;
ALTER TABLE chat_sessions ADD COLUMN org_id UUID;
ALTER TABLE chat_messages ADD COLUMN org_id UUID;
ALTER TABLE comment_index ADD COLUMN org_id UUID;
ALTER TABLE comment_replies ADD COLUMN org_id UUID;
ALTER TABLE draft_sessions ADD COLUMN org_id UUID;
ALTER TABLE timeline_entries ADD COLUMN org_id UUID;
ALTER TABLE task_checkpoints ADD COLUMN org_id UUID;
ALTER TABLE search_documents ADD COLUMN org_id UUID;
ALTER TABLE workspace_members ADD COLUMN org_id UUID;
ALTER TABLE workspace_settings ADD COLUMN org_id UUID;
ALTER TABLE workspace_dependencies ADD COLUMN org_id UUID;
ALTER TABLE node_dependencies ADD COLUMN org_id UUID;
ALTER TABLE doc_node_links ADD COLUMN org_id UUID;
ALTER TABLE draft_updates ADD COLUMN org_id UUID;
ALTER TABLE draft_participants ADD COLUMN org_id UUID;

-- Step 2: Backfill (1-join tables)
UPDATE workspace_branches wb SET org_id = w.org_id
    FROM workspaces w WHERE wb.workspace_id = w.id;
UPDATE jobs j SET org_id = w.org_id
    FROM workspaces w WHERE j.workspace_id = w.id;
UPDATE docs d SET org_id = w.org_id
    FROM workspaces w WHERE d.workspace_id = w.id;
UPDATE chat_sessions cs SET org_id = w.org_id
    FROM workspaces w WHERE cs.workspace_id = w.id;
UPDATE comment_index ci SET org_id = w.org_id
    FROM workspaces w WHERE ci.workspace_id = w.id;
UPDATE workspace_members wm SET org_id = w.org_id
    FROM workspaces w WHERE wm.workspace_id = w.id;
UPDATE workspace_settings ws SET org_id = w.org_id
    FROM workspaces w WHERE ws.workspace_id = w.id;
UPDATE workspace_dependencies wd SET org_id = w.org_id
    FROM workspaces w WHERE wd.workspace_id = w.id;

-- Step 3: Backfill (2-join tables — use already-backfilled 1-join cols)
UPDATE ontology_nodes n SET org_id = wb.org_id
    FROM workspace_branches wb WHERE n.workspace_branch_id = wb.id;
UPDATE tasks t SET org_id = j.org_id
    FROM jobs j WHERE t.job_id = j.id;
UPDATE chat_messages cm SET org_id = cs.org_id
    FROM chat_sessions cs WHERE cm.session_id = cs.id;
UPDATE draft_sessions ds SET org_id = wb.org_id
    FROM workspace_branches wb WHERE ds.workspace_branch_id = wb.id;
UPDATE search_documents sd SET org_id = wb.org_id
    FROM workspace_branches wb WHERE sd.workspace_branch_id = wb.id;
UPDATE comment_replies cr SET org_id = ci.org_id
    FROM comment_index ci WHERE cr.comment_id = ci.id;

-- Step 4: Backfill (3-join tables)
UPDATE timeline_entries te SET org_id = t.org_id
    FROM tasks t WHERE te.task_id = t.id;
UPDATE task_checkpoints tc SET org_id = t.org_id
    FROM tasks t WHERE tc.task_id = t.id;
UPDATE node_dependencies nd SET org_id = n.org_id
    FROM ontology_nodes n WHERE nd.source_node_id = n.id;
UPDATE doc_node_links dl SET org_id = d.org_id
    FROM docs d WHERE dl.doc_id = d.id;
UPDATE draft_updates du SET org_id = ds.org_id
    FROM draft_sessions ds WHERE du.session_id = ds.id;
UPDATE draft_participants dp SET org_id = ds.org_id
    FROM draft_sessions ds WHERE dp.session_id = ds.id;

-- Step 5: Make NOT NULL + add FK
ALTER TABLE workspace_branches ALTER COLUMN org_id SET NOT NULL;
ALTER TABLE workspace_branches ADD CONSTRAINT fk_wb_org
    FOREIGN KEY (org_id) REFERENCES orgs(id);
-- ... repeat for all 20 tables ...

ALTER TABLE jobs ALTER COLUMN org_id SET NOT NULL;
ALTER TABLE jobs ADD CONSTRAINT fk_jobs_org
    FOREIGN KEY (org_id) REFERENCES orgs(id);

-- (remaining tables follow the same pattern)

-- Step 6: Indexes for RLS performance
CREATE INDEX idx_wb_org ON workspace_branches(org_id);
CREATE INDEX idx_jobs_org ON jobs(org_id);
CREATE INDEX idx_tasks_org ON tasks(org_id);
CREATE INDEX idx_onto_org ON ontology_nodes(org_id);
CREATE INDEX idx_docs_org ON docs(org_id);
CREATE INDEX idx_chat_sess_org ON chat_sessions(org_id);
CREATE INDEX idx_chat_msg_org ON chat_messages(org_id);
CREATE INDEX idx_comments_org ON comment_index(org_id);
CREATE INDEX idx_drafts_org ON draft_sessions(org_id);
CREATE INDEX idx_timeline_org ON timeline_entries(org_id);
CREATE INDEX idx_search_org ON search_documents(org_id);
```

### 5.3 Application Code Changes for org_id Propagation

Every INSERT must now include `org_id`. This is already available from `ScopedDb`:

```rust
// Before
sqlx::query!(
    "INSERT INTO jobs (workspace_id, name, ...) VALUES ($1, $2, ...)",
    workspace_id, name,
).execute(&self.db).await?;

// After — org_id comes from the scoped connection
sqlx::query!(
    "INSERT INTO jobs (workspace_id, org_id, name, ...) VALUES ($1, $2, $3, ...)",
    workspace_id, db.org_id(), name,
).execute(db.tx()).await?;
```

### 5.4 RLS Policy Application

```sql
-- Migration: XXX_enable_rls.sql

-- Apply to EVERY tenant-owned table.
-- Pattern is identical for all — macro-expandable.

-- workspaces (already has org_id)
ALTER TABLE workspaces ENABLE ROW LEVEL SECURITY;
ALTER TABLE workspaces FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON workspaces
    USING (org_id = authz.current_org_id());

-- workspace_branches
ALTER TABLE workspace_branches ENABLE ROW LEVEL SECURITY;
ALTER TABLE workspace_branches FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON workspace_branches
    USING (org_id = authz.current_org_id());

-- jobs
ALTER TABLE jobs ENABLE ROW LEVEL SECURITY;
ALTER TABLE jobs FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON jobs
    USING (org_id = authz.current_org_id());

-- tasks
ALTER TABLE tasks ENABLE ROW LEVEL SECURITY;
ALTER TABLE tasks FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON tasks
    USING (org_id = authz.current_org_id());

-- ontology_nodes
ALTER TABLE ontology_nodes ENABLE ROW LEVEL SECURITY;
ALTER TABLE ontology_nodes FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON ontology_nodes
    USING (org_id = authz.current_org_id());

-- (... repeat for all 20+ tenant tables ...)

-- Global tables: DO NOT enable RLS on:
--   orgs, users, oauth_scopes (global/cross-org by nature)
```

### 5.5 Trigger for Automatic org_id Propagation

To avoid bugs where application code forgets to set `org_id` on INSERT:

```sql
-- Trigger: auto-set org_id from session on INSERT if not explicitly provided
CREATE OR REPLACE FUNCTION set_org_id_from_session()
RETURNS TRIGGER
LANGUAGE plpgsql AS $$
BEGIN
    IF NEW.org_id IS NULL THEN
        NEW.org_id := authz.current_org_id();
    END IF;
    RETURN NEW;
END;
$$;

-- Apply to all tables with org_id
CREATE TRIGGER trg_set_org_id BEFORE INSERT ON workspace_branches
    FOR EACH ROW EXECUTE FUNCTION set_org_id_from_session();
CREATE TRIGGER trg_set_org_id BEFORE INSERT ON jobs
    FOR EACH ROW EXECUTE FUNCTION set_org_id_from_session();
-- ... etc.
```

This means existing INSERT statements that don't supply `org_id` will automatically
get it from the session — reducing the blast radius of migration bugs.

---

## 6. Phase 4 — Blob Capability Layer {#6-phase-4}

### 6.1 File Serving Flow — After

```
Client: GET /api/v1/branches/:branchId/files/src/main.rs
  │
  ▼
auth_middleware → AuthContext::User(claims)
  │
  ▼
handler:
  1. Look up branch → get workspace_id, repo_id
  2. Resolve path → blob_sha via DB metadata (file tree index)
     This path→SHA resolution is done in the DB, not the filesystem.
  3. authz.mint_blob_capability(claims, org_id, repo_id, blob_sha, max_bytes)
     ├─ Verifies org membership
     ├─ Signs capability { org_id, repo_id, blob_sha, max_bytes, exp: now+60s }
     └─ Returns BlobCapability
  4. blob.read_blob(&capability)
     ├─ Verifies capability signature + expiry
     ├─ Verifies org_id + repo_id + blob_sha match
     └─ Reads blob by SHA from RepoStorage (content-addressed, TOCTOU-free)
  5. Return file content
```

### 6.2 Internal Git Operations

Operations like `git diff`, `draft commit`, and search indexing need filesystem
access, not blob-level access. These use `DbCapability` as proof of org membership:

```rust
// ontology_service.rs — git diff
pub async fn get_node_diff(
    &self,
    db: &mut ScopedDb,
    blob: &dyn BlobService,
    node_id: Uuid,
) -> Result<String, G5Error> {
    let node = self.get_node(db, node_id).await?;
    let repo_id = node.repo_id.to_string();

    // blob.read_file_internal validates that db.org_id() owns this repo
    // then uses storage.local_path() for git operations
    let diff = blob.git_diff(
        &db.as_capability(),  // proof of org membership
        &repo_id,
    ).await?;

    Ok(diff)
}
```

### 6.3 Tree-Level Capabilities

For `list_file_tree`, no blob capability is needed — use the `DbCapability`
to query the file tree index in the DB (RLS-protected). Individual file reads
then mint per-blob capabilities:

```rust
// List tree: uses DB metadata (RLS-protected), not blob access
let tree = state.workspace_service
    .list_file_tree(&mut scoped_db, branch_id, path, depth)
    .await?;

// Read individual file: resolve path → blob_sha, then mint blob capability
let blob_sha = state.workspace_service
    .resolve_blob_sha(&mut scoped_db, branch_id, file_path)
    .await?;
let cap = authz.mint_blob_capability(
    claims, org_id, repo_id, &blob_sha, MAX_FILE_SIZE,
).await?;
let content = blob.read_blob(&cap).await?;
```

---

## 7. Phase 5 — Mechanical Extraction {#7-phase-5}

### 7.1 What Changes When Extracting g5-authz

| Aspect | In-Process | Extracted |
|--------|-----------|-----------|
| Transport | `impl AuthzService for InProcessAuthz` | `impl AuthzService for AuthzHttpClient` |
| Signing | HMAC in-memory | KMS (Cloud KMS / Vault) |
| Membership queries | `membership_pool` (same Postgres) | Authz service's own DB (or shared read replica) |
| Capability storage | `authz.active_capabilities` table via `authz_writer` conn | Same table, but only the authz service can write |
| Deployment | Same binary | Separate container, separate secrets |

The `AuthzService` trait doesn't change. Only the constructor changes:

```rust
// Before (in g5-api/main.rs)
let authz = Arc::new(InProcessAuthz::new(membership_pool, signing_key));

// After
let authz = Arc::new(AuthzHttpClient::new("https://g5-authz.internal:8443"));
```

### 7.2 What Changes When Extracting g5-blob

| Aspect | In-Process | Extracted |
|--------|-----------|-----------|
| Transport | `impl BlobService for InProcessBlob` | `impl BlobService for BlobHttpClient` |
| Storage | `Arc<dyn RepoStorage>` (same process) | Blob service has its own storage credentials |
| Capability verification | Shared HMAC key | Public key only (cannot mint) |
| Deployment | Same binary | Separate container, no DB access |

```rust
// Before
let blob = Arc::new(InProcessBlob::new(storage, verify_key));

// After
let blob = Arc::new(BlobHttpClient::new("https://g5-blob.internal:8443"));
```

### 7.3 Security Guarantee After Extraction

If g5-api is fully compromised:

| Attack vector | Blocked by |
|--------------|-----------|
| Read another org's DB rows | RLS (DB enforced, cannot bypass) |
| Mint cross-org DB capability | g5-authz is a separate process (attacker doesn't have signing key or authz_writer creds) |
| Read another org's files | g5-blob is a separate process (attacker doesn't have storage creds or signing key) |
| Forge a BlobCapability | Signing key is in g5-authz only (KMS), not in g5-api memory |
| Bypass RLS via session_ctx | `g5_app` role has no INSERT/UPDATE on `authz.session_ctx` |
| Disable RLS | `g5_app` is not superuser, not owner, no BYPASSRLS |
| Access authz.active_capabilities | `g5_app` has no GRANT on the table |

**Blast radius**: The current tenant, the current user's privileges, within the
TTL of their existing capabilities (~5 minutes).

---

## 8. Security Proof at Each Phase {#8-security-proof}

| Phase | Cross-org via URL manipulation | Cross-org via JWT forgery | Cross-org via DB | Cross-org via blob | Under API compromise |
|-------|:---:|:---:|:---:|:---:|:---:|
| **Current** | **OPEN** | Blocked (ES256) | **OPEN** (no RLS) | **OPEN** (no blob svc) | **FULL ACCESS** |
| **Phase 0** | **Blocked** (org guard) | Blocked | **OPEN** | **OPEN** | Reduced (no service token on public routes) |
| **Phase 1** | Blocked | Blocked | **OPEN** | **OPEN** | Further reduced (module boundaries) |
| **Phase 2** | Blocked | Blocked | **Blocked** (RLS) | **OPEN** | DB isolated (RLS), but blob still open |
| **Phase 3** | Blocked | Blocked | Blocked | **OPEN** | Same as Phase 2 (org_id is data migration, not new control) |
| **Phase 4** | Blocked | Blocked | Blocked | **Blocked** (capability) | In-process: blast radius = process. Extract: blast radius = session TTL |
| **Phase 5** | Blocked | Blocked | Blocked | Blocked | **SESSION TTL ONLY** — full containment |

---

## 9. Implementation Plan {#9-implementation-plan}

### 9.1 Phase 0 — Immediate Fixes (Week 1)

| Task | Est. | Dependency |
|------|------|-----------|
| `OrgGuard` middleware + apply to all `/api/v1/orgs/:orgId/*` routes | 1 day | None |
| `WorkspaceOrgGuard` for `/api/v1/workspaces/:id/*` routes (DB lookup) | 1 day | None |
| Apply `RequireScopes` to all write endpoints | 1 day | None |
| Add `iss`/`aud` to `JwtClaims` + validation | 0.5 day | None |
| Remove service token → synthetic admin claims on public routes | 0.5 day | None |
| Consolidate 7× `extract_user_id` → `AuthContext::require_user_id()` | 0.5 day | None |

### 9.2 Phase 1 — Crate Boundaries (Week 2)

| Task | Est. | Dependency |
|------|------|-----------|
| Create `g5-authz` crate with `AuthzService` trait | 1 day | None |
| Create `g5-blob` crate with `BlobService` trait | 1 day | None |
| `InProcessAuthz` implementation (HMAC signing, membership query) | 1.5 days | g5-authz crate |
| `InProcessBlob` implementation (RepoStorage + capability verification) | 1 day | g5-blob crate |
| Wire into `AppState` — all services compile with trait objects | 0.5 day | Both crates |

### 9.3 Phase 2 — DB Session Capability + RLS (Week 3)

| Task | Est. | Dependency |
|------|------|-----------|
| Create `authz` schema + `session_ctx` table + `set_ctx()`/`clear_ctx()`/`current_org_id()` SECURITY DEFINER functions | 1 day | None |
| Create `g5_app`, `authz_writer`, and `authz_ctx_writer` DB roles | 0.5 day | None |
| Implement `ScopedDb` abstraction (with `clear_ctx()` on commit/drop) | 1 day | Phase 1 |
| Migrate `WorkspaceService` to `ScopedDb` (proof of concept) | 1 day | ScopedDb |
| Migrate remaining 16 services to `ScopedDb` | 2 days | PoC validated |
| Update all route handlers to mint capability → create ScopedDb | 1.5 days | Services migrated |

### 9.4 Phase 3 — org_id Denormalization (Week 4)

| Task | Est. | Dependency |
|------|------|-----------|
| Write migration: add org_id to 20 tables | 0.5 day | None |
| Write backfill queries (3 waves by join depth) | 0.5 day | None |
| Add NOT NULL constraints + FK + indexes | 0.5 day | Backfill done |
| Update all INSERT queries to include org_id | 2 days | Migration |
| Add auto-set trigger as safety net | 0.5 day | Migration |
| Enable RLS on all tenant tables | 0.5 day | org_id populated |
| Integration test: verify cross-org queries return zero rows | 0.5 day | RLS enabled |

### 9.5 Phase 4 — Blob Capability Layer (Week 5)

| Task | Est. | Dependency |
|------|------|-----------|
| Implement `mint_blob_capability` in `InProcessAuthz` | 0.5 day | Phase 1 |
| Migrate file routes to use `BlobCapability` | 1 day | Phase 1 |
| Migrate `OntologyService` file reads to `BlobService` | 1 day | Phase 1 |
| Migrate `DocService`, `SearchService` file reads | 1 day | Phase 1 |
| Migrate `DraftService` git operations to `BlobService` | 1 day | Phase 1 |
| Integration test: verify capability-gated file access | 0.5 day | All migrated |

### 9.6 Phase 5 — Extraction (Future, M1+)

| Task | Est. | Dependency |
|------|------|-----------|
| `AuthzHttpClient` implementing `AuthzService` | 1 day | Phase 4 |
| `BlobHttpClient` implementing `BlobService` | 1 day | Phase 4 |
| g5-authz binary (HTTP server, KMS integration) | 2 days | Phase 4 |
| g5-blob binary (HTTP server, storage credentials) | 2 days | Phase 4 |
| Helm chart updates (3 deployments) | 1 day | Binaries |
| Penetration test: compromised g5-api cannot cross tenant | 1 day | Deployed |

---

## 10. Appendix: Full Coupling Inventory {#10-appendix}

### 10.1 Services Holding PgPool (Must Migrate to ScopedDb)

| Service | File | Tenant data? | Migration complexity |
|---------|------|:---:|-----|
| `WorkspaceService` | `workspace_service.rs` (2394 lines) | YES | High (many queries) |
| `OntologyService` | `ontology_service.rs` (1542 lines) | YES | High |
| `JobService` | `job_service.rs` (1289 lines) | YES | High |
| `TaskService` | `task_service.rs` (1597 lines) | YES | High |
| `ChatService` | `chat_service.rs` (1652 lines) | YES | High |
| `DraftService` | `draft_service.rs` (1083 lines) | YES | Medium |
| `CommentService` | `comment_service.rs` | YES | Medium |
| `NotificationService` | `notification_service.rs` | YES | Medium |
| `SearchService` | `search_service.rs` | YES | Medium |
| `DocService` | `doc_service.rs` | YES | Low |
| `CloneService` | `clone_service.rs` | YES | Low |
| `SpendService` | `spend_service.rs` | YES | Medium |
| `AuthService` | `auth_service.rs` | Mixed (users are global) | Medium |
| `OrgService` | `org_service.rs` | Mixed (orgs + memberships) | Medium |
| `UserService` | `user_service.rs` | NO (global) | Skip |
| `PermissionService` | `permission_service.rs` | Mixed | Medium |
| `WaitlistService` | `waitlist_service.rs` | NO (global) | Skip |

### 10.2 Internal Route Handlers Doing Direct SQL

These bypass the service layer and query `state.db` directly — they also need
migration to `ScopedDb` (or must be verified as non-tenant-scoped):

| Handler | File:Line | Tenant data? |
|---------|----------|:---:|
| `report_usage` | `internal.rs:~870` | YES (`usage_records`) |
| `upload_file` | `internal.rs:~920` | YES (workspace files) |
| `complete_chat_message` | `internal.rs:~650` | YES |
| `error_chat_message` | `internal.rs:~700` | YES |

These internal handlers are called by workers via service token. In the current
model, workers operate on behalf of a specific workspace (= specific org). The
internal routes need to either:
- Accept a `DbCapability` from the worker (minted when the task was assigned), or
- Use a service-scoped capability that is pre-minted per task assignment

### 10.3 Worker Task Assignment Capability Flow

When a worker polls for a task, the response should include a pre-minted capability:

```rust
// In task_service.rs — when dispatching a task to a worker
pub struct TaskAssignment {
    pub task_id: Uuid,
    pub workspace_id: Uuid,
    pub workspace_git_url: String,
    // NEW: pre-minted capability for this task's org
    pub db_capability_token: String,
    pub blob_capability_token: String,
}
```

The worker passes these tokens back on internal API calls:

```rust
// Worker calls: POST /api/internal/tasks/:id/timeline
// Header: X-Db-Capability: <token>
// The internal route handler creates ScopedDb from this token
```

This ensures that even the internal API enforces tenant isolation —
a compromised worker cannot access data from other tenants.
