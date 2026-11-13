# Spec Change Proposal — Security Hardening + Distributed FS Migration

> **Status**: PROPOSAL — not yet approved for execution.
> **Date**: 2026-02-23
> **Source documents**:
> - `designs/SECURITY_HARDENING.md` (v2, ~1589 lines)
> - `designs/DISTRIBUTED_FS_MIGRATION.md` (~1323 lines)
> - `designs/OAUTH_SECURITY_AUDIT.md` (605 lines)

Each change includes the full spec text diff and Rust/SQL implementation code
so the proposal is self-contained and auditable.

---

## 1. Executive Summary

This document proposes concrete modifications to the `.semlift/specs/` design specifications
to incorporate the security hardening plan, the OAuth audit remediation, and the distributed
filesystem migration.

| Category | New specs | Modified specs | Scope |
|----------|-----------|----------------|-------|
| **Security — crate boundaries** | 2 | 0 | New `g5-authz` and `g5-blob` crate specs |
| **Security — cross-cutting** | 0 | 9 | JWT, routes, DB, services, infra, worker, root |
| **Distributed FS** | 2 | 3 | RepoStorage trait, ManifestService, infra, services, worker |

Total: **4 new specs**, **10 modified specs** (some overlap between categories), plus
cascading child-spec adjustments.

---

## 2. Gap Analysis — Current Specs vs. Design Requirements

### 2.1 Security Hardening Gaps

| # | Gap | Current spec | Design requirement | Source |
|---|-----|-------------|-------------------|--------|
| S1 | No `iss`/`aud` in JWT claims | `g5-common/auth.semlift.md` — `JwtClaims { sub, org, scopes, iat, exp }` | Add `iss`, `aud`, `jti` fields; validate on every decode | SECURITY_HARDENING §2.3 |
| S2 | No OrgGuard middleware | `g5-api/routes.semlift.md` — handler pattern has `RequireScopes` only | Add `OrgGuard` layer that checks `is_org_member(user_id, org_id)` | SECURITY_HARDENING §2.1 |
| S3 | Services hold raw `PgPool` | `g5-api/services.semlift.md` — `AppState { pub db: PgPool, ... }` | Services receive `ScopedDb` minted by `AuthzService` | SECURITY_HARDENING §3, §4 |
| S4 | Single DB role | `g5-api/db.semlift.md` — one pool, one role, no RLS | Three roles (`g5_app`, `authz_writer`, `authz_ctx_writer`), `authz` schema, `session_ctx` table, `current_org_ids()` function, RLS on all tenant tables | SECURITY_HARDENING §4 |
| S5 | No `AuthzService` / `BlobService` | No spec exists | New crate `g5-authz` with `AuthzService` trait + `InProcessAuthz`; new crate `g5-blob` with `BlobService` trait + `InProcessBlob` | SECURITY_HARDENING §3 |
| S6 | 116/125 routes missing `RequireScopes` | `g5-api/routes.semlift.md` — scope table defined but only 9 routes enforce | All 125 routes MUST enforce scopes per the audit's appendix mapping | OAUTH_AUDIT §Appendix A |
| S7 | Service token is blanket admin | `g5-common/auth.semlift.md` — `ServiceTokenValidator`; `g5-api/services.semlift.md` — `service_claims()` synthetic admin | Replace with `AuthContext::Worker { worker_id, org_id, task_id }` with org-scoped, time-limited session tokens | OAUTH_AUDIT Phase 2 |
| S8 | No blob capability layer | No spec exists | `BlobCapability { git_oid, max_bytes }` minted per-request; blob access gated by capability | SECURITY_HARDENING §6 |
| S9 | No startup guardrails | `g5-api/main.semlift.md` — starts without key validation | Refuse startup if `JWT_SIGNING_KEY` is missing/weak or `SERVICE_TOKEN_SECRET` is default | OAUTH_AUDIT Phase 4 |
| S10 | No `authz` pool in startup | `g5-api/main.semlift.md` — single pool | Two pools: `app_pool` (g5_app role) + `authz_pool` (authz_writer role) | SECURITY_HARDENING §4.1 |

### 2.2 Distributed FS Gaps

| # | Gap | Current spec | Design requirement | Source |
|---|-----|-------------|-------------------|--------|
| F1 | No `RepoStorage` trait | No spec exists | `RepoStorage` trait with `LocalRepoStorage` and `FilestoreRepoStorage` impls | DISTRIBUTED_FS §3 |
| F2 | No Filestore in infra | `infra.semlift.md` — GCS buckets only | Add Google Cloud Filestore (managed NFS) for multi-worker shared repos | DISTRIBUTED_FS §9 |
| F3 | Worker assumes local FS | `g5d/worker.semlift.md` — direct filesystem paths | Worker lifecycle uses `RepoStorage` trait for PREPARE/CLONE/COLLECT phases | DISTRIBUTED_FS §4 |
| F4 | No `ManifestService` | No spec exists | `ManifestService` for repo checkout manifest tracking + cache invalidation | DISTRIBUTED_FS §7 |

### 2.3 CRUD Lifecycle Gaps

| # | Gap | Current spec | Design requirement | Source |
|---|-----|-------------|-------------------|--------|
| C1 | CRUD flows use `PgPool` directly | `org_service.semlift.md` / `workspace_service.semlift.md` — `OrgService { db: PgPool }` | `create_org` uses `system_pool`; `create_workspace`/`add_repo` use `ScopedDb` via `tenant_pool` | §4.11 |
| C2 | No `RepoStorage` init in CRUD | `workspace_service.semlift.md` — no storage calls | `create_org`, `create_workspace`, `add_repo` must init repo directories + trigger git clone | §4.11, DISTRIBUTED_FS §6 |
| C3 | No ontology reconciliation on file change | `search_indexer.semlift.md` — search-only | Ontology nodes auto-created/updated/deleted when files change; runs alongside search indexer | §4.12 |
| C4 | No linked document auto-indexing | `doc_node_links` table exists but no background indexer | YAML frontmatter `linked_nodes:` parsed and `doc_node_links` rows maintained automatically | §4.12 |

---

## 3. Proposed Changes — New Specs (Full Text)

### 3.1 NEW: `g5/g5-authz.semlift.md`

**Location**: `.semlift/specs/g5/g5-authz.semlift.md`

<details>
<summary>Full spec text (click to expand)</summary>

```yaml
---
child_spec_ids: []
complexity: high
component_id: g5/g5-authz
dependencies:
  - g5/g5-common
  - g5/g5-common/auth
  - g5/g5-common/error
granularity_score: 0.6
hierarchy_level: 1
is_root: false
language: rust
parent_spec_id: g5
source_files:
  - g5-authz/Cargo.toml
  - g5-authz/src/lib.rs
  - g5-authz/src/capability.rs
  - g5-authz/src/signing.rs
type: downlift
version: 0.1.0
---
```

# G5 Authorization Service Crate — AuthzService, DbCapability, BlobCapability

## Overview

The `g5-authz` crate is **pure policy + capability minting**. It provides the
`AuthzService` trait (object-safe, `Send + Sync`) for org membership checks and
capability minting (`DbCapability`, `BlobCapability`), plus the corresponding
error types and signing logic.

**OrgGuard middleware lives in `g5-api`** (see §4.3), NOT in this crate. The
middleware calls `AuthzService::is_org_member()` via `Arc<dyn AuthzService>`
injected from `AppState`. This avoids pulling `g5-api` types (e.g. `AppState`,
`AppError`) into `g5-authz`, which would violate the crate-boundary invariant.

**Critical design constraint**: `g5-authz` MUST NOT accept or use the tenant
data `PgPool` (`g5_app` role). It may use `sqlx` with restricted pools only:
`membership_pool` (read-only SELECT on `org_memberships`) and `authz_pool`
(UPSERT/DELETE on `authz.user_sessions`). It MUST NOT depend on `g5-api`,
`g5-blob`, or any `g5-api` types (`AppState`, `AppError`). This compile-time
boundary ensures service-layer code cannot bypass authorization.

When extracted to a separate service (Phase 5), `InProcessAuthz` moves into the
`g5-authz` binary. `g5-api` gets an `AuthzHttpClient` implementing the same trait.

## Behavior

### Capability Types

```rust
// g5-authz/src/capability.rs

use uuid::Uuid;

/// Capability to execute queries scoped to one or more orgs.
/// Contains the signed context needed for DB session injection.
///
/// **In-process mode** (Phases 1–4): `session_id` is a `Uuid` referencing
/// a row in `authz.user_sessions`, looked up by `authz.set_ctx(UUID)`.
/// Sessions are **reusable** — the same session_id is valid for multiple
/// requests within the 60-minute TTL. `set_ctx()` does NOT consume
/// (delete) the session; it reads the session and writes to `session_ctx`.
/// This is the ONLY form `ScopedDb::from_capability()` accepts.
///
/// **Extracted mode** (Phase 5): When authz runs as a separate service,
/// a separate `DbCtxJwt(String)` type wraps a KMS-signed JWT. The
/// extracted service validates the JWT and returns a `DbCapability`
/// with a `Uuid` session_id via the `AuthzHttpClient`. The trait
/// contract for `mint_db_capability` always returns `DbCapability`
/// with a `Uuid` — the JWT is an internal transport detail, not
/// exposed on this struct.
///
/// **Multi-org support**: `org_ids` is an array, enabling cross-org queries
/// (e.g., dashboard showing workspaces from all user orgs). Single-org
/// requests pass `&[org_id]` — the common case is a one-element array.
/// The org membership check happens at mint time (application layer) —
/// RLS trusts whatever orgs were written into session_ctx by set_ctx().
#[derive(Debug, Clone)]
pub struct DbCapability {
    /// The org(s) this capability is scoped to.
    /// Single-org routes: `[url_org_id]` (one element).
    /// Cross-org routes: all orgs the user belongs to.
    pub org_ids: Vec<Uuid>,
    /// The actor (user or worker) who requested this capability.
    /// Maps to `user_sessions.user_id` in the DB (column name retained
    /// for schema compatibility; stores actor IDs of any type).
    pub actor_id: Uuid,
    /// Effective scopes for this session.
    pub scopes: Vec<String>,
    /// Reusable UUID referencing an `authz.user_sessions` row.
    /// Looked up (NOT consumed) by `authz.set_ctx()` on each request.
    /// Valid for multiple requests within the session TTL.
    pub session_id: Uuid,
    /// Session TTL (seconds from now). 3600s (60 min) default.
    /// Org membership is re-verified when the session is created or
    /// refreshed; within the TTL window, set_ctx() trusts the session.
    pub ttl_secs: u32,
}

/// Capability to read a specific blob.
///
/// Uses content-addressed git_oid instead of path to avoid TOCTOU bugs.
/// Path → git_oid resolution happens in the DB metadata layer before
/// minting the capability.
#[derive(Debug, Clone)]
pub struct BlobCapability {
    pub org_id: Uuid,
    pub repo_id: Uuid,
    /// Git object ID (the hash Git uses for the blob object).
    ///
    /// **Hash semantics**: This is the Git object ID as returned by
    /// `git hash-object` / stored in Git's object DB. For SHA-1 repos
    /// (current default) it is a 40-char hex string; for SHA-256 repos
    /// (git's `extensions.objectFormat = sha256`) it is 64-char hex.
    /// We store it as a string and treat it as opaque — the blob module
    /// never re-hashes; it uses `git cat-file -p <git_oid>` to extract
    /// content, then stores the result under `.blobs/<fanout>/<git_oid>`.
    ///
    /// **NOT** a raw content hash (i.e., not SHA-256 of file bytes).
    /// Git object hashes include a type+length header, so
    /// `git_oid != sha256(file_bytes)`. If we ever need pure content
    /// hashing (e.g., deduplication across repos), that would be a
    /// separate field.
    pub git_oid: String,
    /// Maximum allowed read size in bytes (prevents abuse).
    pub max_bytes: u64,
    /// Signed token consumed by the blob service.
    pub token: String,
    pub ttl_secs: u32,
}
```

### Blob Capability Token Wire Format

The `BlobCapability.token` is a **JWS Compact Serialization** (RFC 7515)
signed with Ed25519 (`EdDSA` algorithm). This uses standard JWT tooling
while keeping the token compact (~300 bytes).

**Payload fields** (all required):

| Field | Type | Description |
|-------|------|-------------|
| `org_id` | UUID string | Tenant org |
| `repo_id` | UUID string | Repository |
| `git_oid` | string | Git object ID (SHA-1 40-char or SHA-256 64-char hex) |
| `max_bytes` | u64 | Maximum read size |
| `exp` | i64 | Expiry (Unix timestamp, short-lived: 300s default) |
| `purpose` | string | Token type discriminator — always `"blob_read"` |
| `token_use` | string | `"capability"` — distinguishes from user/worker JWTs |
| `jti` | UUID string | Unique token ID — required for structure and tracing; provides **no replay protection** until a server-side jti store is added (see phase-optional replay cache below). A token with a valid jti is accepted indefinitely within TTL. |

> **Why `purpose`?** Prevents token-confusion attacks if we later add other
> capability token types (e.g., `blob_write`, `repo_admin`) that share
> similar fields. Verification MUST reject tokens where `purpose != "blob_read"`.
>
> **Why `token_use`?** Prevents cross-protocol confusion between user JWTs,
> worker JWTs, and capability tokens. All three share EdDSA/ES256 signing
> but serve different roles. Middleware MUST reject tokens where
> `token_use != "capability"` when expecting a blob token (and vice versa:
> user/worker JWT validation MUST reject tokens that contain `token_use`).
>
> **Required test case** (auth spec, JWT validation unit tests):
> ```rust
> #[test]
> fn rejects_jwt_with_token_use_claim() {
>     // A user/worker JWT that contains `"token_use": "capability"` (or any
>     // value) MUST be rejected by JwtValidator. This prevents a capability
>     // token from being accepted as a user JWT if a future library upgrade
>     // silently ignores unknown claims.
>     let mut claims = valid_user_claims();
>     claims["token_use"] = json!("capability");
>     let token = mint_with_claims(&claims);
>     assert!(jwt_validator.decode(&token).is_err());
> }
> ```
> This enforces the negative side of the `token_use` discrimination: blob
> validators check `token_use == "capability"` (positive), and JWT validators
> check `token_use` is absent (negative). Both sides need explicit tests.

**Canonical serialization**: Payload is standard JSON with no guaranteed
claim ordering. Claim ordering is an **internal encoding detail** of
`g5-authz` — NOT part of the wire format contract. Verification checks
the Ed25519 signature over the received base64url-encoded bytes, which
is order-independent. Do not depend on alphabetical ordering for
interoperability or testing; use the signed bytes as received.

Header: `{"alg":"EdDSA","typ":"JWT"}`.

**Clock skew**: No leeway. Blob tokens are short-lived (5 min) and minted
by the same process; clock skew is not a concern in-process. For Phase 5
(extracted), allow 5s leeway configurable via `BLOB_TOKEN_CLOCK_SKEW_SECS`.

```rust
// g5-authz/src/lib.rs

use uuid::Uuid;

/// Authorization service contract.
///
/// In-process: function calls via InProcessAuthz.
/// Extracted: HTTP calls to g5-authz service via AuthzHttpClient.
#[async_trait]
pub trait AuthzService: Send + Sync + 'static {
    /// Check if a user is a member of an org.
    /// Used by OrgGuard middleware before minting any capabilities.
    ///
    /// **RLS exemption**: `org_memberships` is intentionally NOT subject to
    /// tenant RLS. Membership checks must work *before* a DB session is
    /// minted (chicken-and-egg: you can't call `set_ctx` without knowing
    /// which org the user belongs to). The `membership_pool` uses a
    /// read-only role with SELECT-only access to `org_memberships` — no
    /// session context is set. If RLS is later applied broadly,
    /// `org_memberships` MUST either: (a) remain exempt from RLS, or
    /// (b) have a policy that allows lookup by `(org_id, user_id)` without
    /// requiring `authz.current_org_ids()`.
    ///
    /// **Caching**: If membership checks become a bottleneck, add a Redis
    /// cache with key `org_member:{org_id}:{actor_id}` → `bool`, TTL 60s.
    /// Invalidate on membership change events (org member add/remove).
    /// Cache hit avoids the `membership_pool` round-trip. Revocation latency
    /// is bounded by the cache TTL (60s), which is shorter than the DB
    /// capability TTL (3600s) and acceptable for the threat model.
    ///
    /// **Negative caching**: Cache BOTH `true` AND `false` results.
    /// Without negative caching, repeated requests for non-members hit the DB
    /// every time (timing-based membership oracle + DB amplification vector).
    /// Negative cache entries use the same 60s TTL.
    ///
    /// **Invalidation**: On `org_member_added` or `org_member_removed` events,
    /// DELETE the Redis key `org_member:{org_id}:{actor_id}`. For bulk
    /// operations (org deletion), use `SCAN` + `DEL` with the `org_member:{org_id}:*`
    /// pattern (or key-prefix expiry if using Redis 7.4+ with key expiry policies).
    /// Additionally, invalidate the affected user's DB session:
    /// `DELETE FROM authz.user_sessions WHERE user_id = $1`
    /// This forces re-verification of org membership on the next request.
    /// **Revocation urgency**: cache TTL (60s) < session TTL (3600s), so
    /// a revoked membership stops granting new sessions within 60s.
    async fn is_org_member(
        &self,
        actor_id: Uuid,
        org_id: Uuid,
    ) -> Result<bool, AuthzError>;

    /// Mint a DB session capability for the given actor + org(s).
    /// 1. Verifies org membership for each org in `target_org_ids`
    /// 2. Generates UUID session_id
    /// 3. UPSERTs into authz.user_sessions (via authz_writer conn)
    /// 4. Returns DbCapability { org_ids, actor_id, session_id, ttl: 3600s }
    ///
    /// Accepts identity components (not `JwtClaims`) so both
    /// `AuthContext::User` and `AuthContext::Worker` callers can mint
    /// capabilities. The route handler extracts `actor_id` and `scopes`
    /// from the `AuthContext` enum.
    ///
    /// **Single-org vs cross-org**: Most routes pass `&[url_org_id]` (one org).
    /// Cross-org endpoints (dashboard, global search) call `user_org_ids()`
    /// first, then pass the full list. Membership is verified for every org.
    ///
    /// **Session semantics**: DB sessions are reusable within their 60-minute
    /// TTL. Each call UPSERTs the actor's session — if one exists, its
    /// `org_ids` are **merged** (union, not replaced) with the new org_ids,
    /// preventing single-org minting from invalidating concurrent multi-org
    /// sessions. Scopes and TTL are refreshed to the latest values.
    ///
    /// **Intended widening behavior**: The `array_cat` union means a user's
    /// `user_sessions.org_ids` accumulates all orgs they've been minted for
    /// within the 60-minute TTL window. This is the **intended security model**:
    /// the session represents the actor's active authorization envelope, not a
    /// single request's scope. Rationale:
    ///   - Narrowing (replace) breaks concurrent cross-org requests
    ///   - Widening is bounded by membership: `mint_db_capability` verifies
    ///     membership for every org_id before UPSERT, so the union can only
    ///     contain orgs the actor actually belongs to
    ///   - TTL (60 min) bounds the window: revoked memberships stop appearing
    ///     in new sessions after the DB session expires + membership cache TTL
    ///   - `set_ctx()` reads from `user_sessions` (not the capability), so
    ///     `session_ctx.org_ids` may be **broader** than the `DbCapability.org_ids`
    ///     returned to a specific handler. This is acceptable because RLS
    ///     policies grant visibility to all of the actor's verified orgs, and
    ///     route-level `OrgGuard` constrains which org's data the handler
    ///     actually queries.
    /// If a handler fails *after* minting, the session remains valid (it is
    /// NOT consumed). Expired sessions are pruned by the periodic cleanup
    /// task (step 17 in main startup):
    /// `DELETE FROM authz.user_sessions WHERE expires_at < now()`
    /// every 5 minutes. On org membership change (revocation), invalidate
    /// the affected actor's session: `DELETE FROM authz.user_sessions WHERE user_id = $1`.
    async fn mint_db_capability(
        &self,
        actor_id: Uuid,
        target_org_ids: &[Uuid],
        scopes: &[String],
    ) -> Result<DbCapability, AuthzError>;

    /// Mint a blob read capability for a specific Git object ID.
    async fn mint_blob_capability(
        &self,
        actor_id: Uuid,
        org_id: Uuid,
        repo_id: Uuid,
        git_oid: &str,
        max_bytes: u64,
    ) -> Result<BlobCapability, AuthzError>;

    /// Retrieve all org IDs the user is a member of.
    /// Used by cross-org endpoints (dashboard, global search, notifications)
    /// that need to mint a capability spanning multiple orgs.
    ///
    /// Returns org IDs in no guaranteed order. The result is NOT cached
    /// by this method — caching is handled by the Redis membership cache
    /// layer inside `is_org_member`. For cross-org endpoints, callers should
    /// fetch this once per request and pass the result to `mint_db_capability`.
    async fn user_org_ids(
        &self,
        actor_id: Uuid,
    ) -> Result<Vec<Uuid>, AuthzError>;
}

// NOTE: No capability *verification* methods on this trait.
// - DB capability verification is handled by `authz.set_ctx(UUID)` in plpgsql.
// - Blob capability verification is handled by `BlobService` using the
//   Ed25519 public key — the blob module parses and verifies tokens locally.
// AuthzService is the *minter*; verification is the consumer's responsibility.
// This separation ensures that when extracted (Phase 5), the blob service
// can verify tokens with only the public key, without calling back to authz.
```

### Scope Enforcement Architecture

Scope checking happens at **two** layers:

1. **Route layer** (`RequireScopes` middleware in `g5-api`): Primary gate.
   Every route declares its required scopes in `scope_map.yaml`. The
   middleware rejects requests that lack the necessary scopes before the
   handler runs. This is the first line of defense.

2. **Capability minting** (`mint_db_capability`): The minter embeds the
   caller's scopes into the `DbCapability` as metadata. These scopes are
   stored in `authz.user_sessions.scopes` and propagated to
   `session_ctx` by `set_ctx()`. Minting does **NOT** re-check scopes
   (it trusts the middleware). Scopes on the capability are for audit
   and fine-grained service-layer checks only.

3. **Service-layer checks** (optional, defense-in-depth): Services may
   inspect `ScopedDb::scopes()` for operations that require elevated
   privileges (e.g., `org:admin` for destructive operations). This is
   not mandatory but provides defense-in-depth.

Rule summary:

| Layer | Checks scopes? | Blocks on failure? | Purpose |
|-------|---------------|-------------------|----------|
| `RequireScopes` (route) | Yes | Yes | Primary enforcement |
| `mint_db_capability` | No | No | Embeds scopes as metadata |
| Service methods (opt.) | Yes (selective) | Yes | Defense-in-depth |

#[derive(Debug, thiserror::Error)]
pub enum AuthzError {
    #[error("not a member of org {0}")]
    NotMember(Uuid),
    /// Reserved for optional defense-in-depth scope checks in the service
    /// layer (via `ScopedDb::scopes()`). NOT raised by `mint_db_capability`.
    /// Primary scope enforcement is at the route layer (`RequireScopes`).
    #[error("insufficient scopes: missing {}", .0.join(", "))]
    InsufficientScopes(Vec<String>),
    #[error("capability expired")]
    Expired,
    #[error("invalid capability signature")]
    InvalidSignature,
    #[error("internal: {0}")]
    Internal(String),
}
```

### InProcessAuthz Implementation

```rust
// g5-authz/src/lib.rs

pub struct InProcessAuthz {
    /// Read-only DB pool — only has SELECT on org_memberships.
    membership_pool: PgPool,
    /// Authz-writer pool — UPSERT/DELETE on authz.user_sessions.
    authz_pool: PgPool,
    /// Ed25519 signing key for blob capabilities.
    /// The corresponding VerifyingKey is given to InProcessBlob.
    blob_signing_key: ed25519_dalek::SigningKey,
}

impl InProcessAuthz {
    pub fn new(
        membership_pool: PgPool,
        authz_pool: PgPool,
        blob_signing_key: ed25519_dalek::SigningKey,
    ) -> Self {
        Self { membership_pool, authz_pool, blob_signing_key }
    }
}

#[async_trait]
impl AuthzService for InProcessAuthz {
    async fn is_org_member(
        &self,
        actor_id: Uuid,
        org_id: Uuid,
    ) -> Result<bool, AuthzError> {
        sqlx::query_scalar!(
            "SELECT EXISTS(
                SELECT 1 FROM org_memberships
                WHERE org_id = $1 AND user_id = $2
            ) as \"exists!\"",
            org_id, actor_id,
        )
        .fetch_one(&self.membership_pool)
        .await
        .map_err(|e| AuthzError::Internal(e.to_string()))
    }

    async fn mint_db_capability(
        &self,
        actor_id: Uuid,
        target_org_ids: &[Uuid],
        scopes: &[String],
    ) -> Result<DbCapability, AuthzError> {
        // 1. Verify org membership for EVERY org in the list
        for &oid in target_org_ids {
            if !self.is_org_member(actor_id, oid).await? {
                return Err(AuthzError::NotMember(oid));
            }
        }

        // 2. Generate session ID (may be overwritten by UPSERT if session exists)
        let session_id = Uuid::new_v4();
        let ttl_secs: u32 = 3600; // 60 minutes
        let expires_at = chrono::Utc::now()
            + chrono::Duration::seconds(ttl_secs as i64);

        // 3. UPSERT into authz.user_sessions (authz_writer role)
        //    One row per user — ON CONFLICT merges org_ids (union, not replace)
        //    to avoid invalidating concurrent cross-org sessions.
        //
        //    **Race resolution**: Without union semantics, a single-org route
        //    minting with &[url_org_id] would overwrite org_ids to a one-element
        //    array, invalidating any concurrent multi-org session for the same
        //    user. The array_cat + DISTINCT union prevents this.
        let row = sqlx::query_scalar!(
            "INSERT INTO authz.user_sessions
                (session_id, user_id, org_ids, scopes, expires_at, refreshed_at)
             VALUES ($1, $2, $3, $4, $5, now())
             ON CONFLICT (user_id) DO UPDATE SET
                org_ids      = (SELECT ARRAY(SELECT DISTINCT unnest(array_cat(authz.user_sessions.org_ids, EXCLUDED.org_ids)))),
                scopes       = EXCLUDED.scopes,
                expires_at   = EXCLUDED.expires_at,
                refreshed_at = now()
             RETURNING session_id",
            session_id, actor_id,
            target_org_ids, scopes, expires_at,
        )
        .fetch_one(&self.authz_pool)
        .await
        .map_err(|e| AuthzError::Internal(e.to_string()))?;

        Ok(DbCapability {
            org_ids: target_org_ids.to_vec(),
            actor_id,
            scopes: scopes.to_vec(),
            session_id: row,
            ttl_secs,
        })
    }

    async fn user_org_ids(
        &self,
        actor_id: Uuid,
    ) -> Result<Vec<Uuid>, AuthzError> {
        sqlx::query_scalar!(
            "SELECT org_id FROM org_memberships WHERE user_id = $1",
            actor_id,
        )
        .fetch_all(&self.membership_pool)
        .await
        .map_err(|e| AuthzError::Internal(e.to_string()))
    }

    // ... mint_blob_capability (signs with blob_signing_key)
}
```

## Dependencies

### Internal Dependencies
- **g5/g5-common** — `G5Error`, `JwtClaims`, `AuthContext`
- **g5/g5-common/auth** — `JwtClaims` struct

### External Dependencies
- **sqlx** — For `membership_pool` (SELECT on `org_memberships`) and `authz_pool` (UPSERT/DELETE on `authz.user_sessions`). **NOT the tenant data pool.**
- **hmac** / **sha2** — HMAC-SHA256 for DB capability nonce signing; Ed25519 for blob capabilities (see §A5 note below)
- **uuid** — `Uuid` for org/user/capability IDs
- **chrono** — Capability expiry timestamps
- **async-trait** — Object-safe async trait definition
- **thiserror** — Error derive

### Must NOT Depend On
- `g5-api` — would create circular dependency (no `AppState`, no `AppError`, no axum middleware types)
- `g5-blob` — blob service must not be coupled to authz
- `axum` — middleware lives in `g5-api`, not here
- Tenant data `PgPool` (`g5_app` role) — authz module uses its own restricted pools only

## Implementation Requirements

### File Structure
```
g5-authz/
├── Cargo.toml
└── src/
    ├── lib.rs          — AuthzService trait, InProcessAuthz, AuthzError
    ├── capability.rs   — DbCapability, BlobCapability structs
    └── signing.rs      — HMAC/Ed25519 signing/verification for capabilities
```

### Success Criteria Checklist

- [ ] `AuthzService` trait is object-safe (`Send + Sync + 'static`)
- [ ] `InProcessAuthz` verifies org membership via `membership_pool` (NOT app pool)
- [ ] `mint_db_capability` UPSERTs into `authz.user_sessions` via `authz_pool`
- [ ] `DbCapability.session_id` is a UUID (not text) — in-process only; see A3 note
- [ ] `BlobCapability` uses `git_oid` (Git object ID, not filesystem path)
- [ ] Crate compiles without depending on `g5-api`, `axum`, or tenant `PgPool`
- [ ] `AuthzError` is converted to `AppError` at the `g5-api` boundary via `impl From<AuthzError> for AppError` **in g5-api**, not in g5-authz
- [ ] DB session TTL is 3600 seconds (60 minutes)
- [ ] DB sessions are reusable within TTL (NOT consumed by `set_ctx`); one session per user (UNIQUE on user_id)
- [ ] Blob capabilities are multi-use within TTL (stateless signed tokens — see D2 note)
- [ ] OrgGuard middleware lives in g5-api/routes (§4.3), calls `AuthzService::is_org_member()`

</details>

---

### 3.2 NEW: `g5/g5-blob.semlift.md`

**Location**: `.semlift/specs/g5/g5-blob.semlift.md`

<details>
<summary>Full spec text (click to expand)</summary>

```yaml
---
child_spec_ids: []
complexity: medium
component_id: g5/g5-blob
dependencies:
  - g5/g5-common
  - g5/g5-common/storage
  - g5/g5-authz
granularity_score: 0.5
hierarchy_level: 1
is_root: false
language: rust
parent_spec_id: g5
source_files:
  - g5-blob/Cargo.toml
  - g5-blob/src/lib.rs
  - g5-blob/src/capability.rs
type: downlift
version: 0.1.0
---
```

# G5 Blob Service Crate — BlobService, Capability-Gated File Access

## Overview

The `g5-blob` crate mediates all blob (artifact/file) storage access through
capability-gated operations. No service can read or list blobs without a valid
`BlobCapability` minted by `AuthzService`.

**Critical design constraint**: `g5-blob` has NO access to `PgPool` (the tenant data
DB). It holds only a `RepoStorage` reference and an Ed25519 *public* verification
key (the verifying counterpart of `InProcessAuthz`'s Ed25519 signing key). This
asymmetric split is intentional: even in-process, the blob module cannot mint
capabilities, only verify them. When extracted (Phase 5), the blob service holds
ONLY the public key, so compromising it cannot forge capabilities.

### Blob Storage Layout

`BlobCapability.git_oid` is a Git object ID, **not** a filesystem path.
Blobs are stored in a **fanout directory** within the repo's storage root,
separate from the git worktree layout:

```
{base_dir}/{repo_id}/
├── HEAD, objects/, refs/         # bare git repo (managed by GitService)
└── .blobs/                       # extracted blob store (managed by BlobService)
    └── ab/
        └── cd/
            └── abcdef0123456789...  # blob content, named by sha256
```

**Blob extraction**: When a file is committed (or fetched), a background task
extracts blobs into `.blobs/<oid[0:2]>/<oid[2:4]>/<oid>` using
`git cat-file -p <oid>`. The DB metadata layer records the mapping
`(repo_id, file_path, commit) → git_oid`.

**read_blob path resolution**: `BlobService::read_blob()` converts `git_oid`
to a deterministic path via `blob_path(repo_id, oid)` → reads via RepoStorage:

```rust
fn blob_path(oid: &str) -> String {
    format!(".blobs/{}/{}/{}", &oid[..2], &oid[2..4], oid)
}
```

> **SHA-1 vs SHA-256 fanout safety**: The 2+2 char fanout uses the first 4 hex
> chars of the OID, which is the same strategy git uses internally (`objects/xx/`
> uses 2 chars). For repos mixing `extensions.objectFormat` (SHA-1 = 40-char,
> SHA-256 = 64-char), there is no collision risk in the fanout directory because:
> (a) SHA-1 and SHA-256 OIDs occupy different length namespaces (the full OID is
> the filename, not just the fanout prefix), and (b) git's SHA-256 objects use a
> different internal format header, so the same content produces different OIDs
> under each hash. A single `.blobs/` tree can safely store both SHA-1 and SHA-256
> OIDs without conflict.

This means `RepoStorage::read_file(repo_id, blob_path(oid))` is a valid
filesystem operation. The oid→path mapping is deterministic and internal
to the blob module — callers only ever see `git_oid`.

## Behavior

### BlobService Trait

```rust
// g5-blob/src/lib.rs

use bytes::Bytes;
use g5_authz::capability::{BlobCapability, DbCapability};
use g5_common::storage::{RepoStorage, StorageEntry};

/// Blob service contract.
///
/// In-process: reads from RepoStorage directly.
/// Extracted: HTTP calls to g5-blob service.
#[async_trait]
pub trait BlobService: Send + Sync + 'static {
    /// Read blob content by capability token.
    /// The implementation parses + verifies the Ed25519-signed token,
    /// extracts {org_id, repo_id, git_oid, max_bytes, purpose, token_use, exp}, then
    /// verifies `purpose == "blob_read"` (rejects other token types), then
    /// converts git_oid to a deterministic path via blob_path()
    /// and reads from RepoStorage.
    async fn read_blob(
        &self,
        token: &str,
    ) -> Result<Bytes, BlobError>;

    /// List file tree by capability token.
    /// The implementation parses + verifies the Ed25519-signed token
    /// (same as `read_blob`), then verifies `purpose == "blob_read"`.
    /// Both `read_blob` and `list_tree` share the same purpose claim —
    /// a single `blob_read` capability authorizes both content reads and
    /// tree listings for the same repo. If a distinct `tree_list` purpose
    /// is needed later (e.g., to issue read-only tree tokens without blob
    /// access), add it as a backwards-compatible extension.
    async fn list_tree(
        &self,
        token: &str,
        path: &str,
        depth: u32,
    ) -> Result<Vec<StorageEntry>, BlobError>;
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
    #[error("size exceeds max_bytes")]
    TooLarge,
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
}
```

### InProcessBlob Implementation

```rust
// g5-blob/src/lib.rs

pub struct InProcessBlob {
    storage: Arc<dyn RepoStorage>,
    /// Ed25519 public key to verify blob capability signatures.
    ///
    /// Phase 1–4 (in-process): Uses Ed25519 — the authz module signs
    /// with the private key, the blob module verifies with the public
    /// key. This is asymmetric by design so that when extracted
    /// (Phase 5), the blob service holds ONLY the public key and
    /// cannot mint capabilities.
    ///
    /// NOTE: An earlier draft used HMAC (symmetric). HMAC means both
    /// signer and verifier share the same secret, which defeats
    /// process-isolation guarantees in Phase 5. Ed25519 gives us
    /// a true sign/verify split with minimal overhead (~10µs/op).
    verify_key: ed25519_dalek::VerifyingKey,
}

impl InProcessBlob {
    pub fn new(
        storage: Arc<dyn RepoStorage>,
        verify_key: ed25519_dalek::VerifyingKey,
    ) -> Self {
        Self { storage, verify_key }
    }
}

#[async_trait]
impl BlobService for InProcessBlob {
    async fn read_blob(
        &self,
        token: &str,
    ) -> Result<Bytes, BlobError> {
        // 1. Parse + verify Ed25519 signed token → BlobCapability fields
        let cap = self.parse_and_verify(token)?;

        // 2. Read from storage via deterministic sha→path mapping
        let path = Self::blob_path(&cap.git_oid);
        let data = self.storage
            .read_file(&cap.repo_id.to_string(), &path)
            .await
            .map_err(|e| match e {
                StorageError::NotFound(p) => BlobError::NotFound(p),
                other => BlobError::Io(std::io::Error::other(other.to_string())),
            })?;

        // 3. Enforce max_bytes
        if data.len() as u64 > cap.max_bytes {
            return Err(BlobError::TooLarge);
        }

        Ok(data)
    }

    // ... list_tree
}

impl InProcessBlob {
    /// Deterministic sha→path mapping for content-addressed blob storage.
    /// `repo_id` is NOT part of the blob path — it is used as the first
    /// argument to `RepoStorage::read_file(repo_id, path)` at the call site.
    fn blob_path(sha: &str) -> String {
        format!(".blobs/{}/{}/{}", &sha[..2], &sha[2..4], sha)
    }
}
```

> **Internal file reads** (git diff, draft commit): These do NOT go through
> `BlobService`. Workers and services that need to read files by *path* (not
> by content-addressed sha) use `RepoStorage::read_file()` directly. Access
> is scoped by the fact that the worker already has a checked-out worktree
> for that repo (via `GitService`), and file path operations happen within
> `ScopedDb` queries that enforce RLS. There is no `read_file_internal`
> method on `BlobService` — the blob service exclusively handles
> capability-gated, content-addressed reads.
```

## Dependencies

### Internal Dependencies
- **g5/g5-common** — `StorageEntry`, `StorageError`
- **g5/g5-common/storage** — `RepoStorage` trait
- **g5/g5-authz** — `BlobCapability` struct (for token parsing; `DbCapability` is NOT used here)

### Must NOT Depend On
- `g5-api` — no coupling to API server
- `sqlx` / `PgPool` — the blob service has no database access

## Implementation Requirements

### File Structure
```
g5-blob/
├── Cargo.toml
└── src/
    ├── lib.rs          — BlobService trait, InProcessBlob, BlobError
    └── capability.rs   — Capability verification logic
```

### Success Criteria Checklist

- [ ] `BlobService` trait is object-safe (`Send + Sync + 'static`)
- [ ] `InProcessBlob` verifies Ed25519 capability signature before any read
- [ ] `InProcessBlob` rejects blob tokens where `purpose != "blob_read"`
- [ ] `InProcessBlob` enforces `max_bytes` limit on blob reads
- [ ] `InProcessBlob` uses deterministic `blob_path(sha)` mapping (→ `.blobs/<sha[0:2]>/<sha[2:4]>/<sha>`) for content-addressed reads
- [ ] No `read_file_internal` on BlobService — internal path-based reads use RepoStorage directly
- [ ] Crate compiles without depending on `g5-api` or `sqlx`
- [ ] `BlobError` maps to appropriate HTTP status codes
- [ ] Blob capabilities are **multi-use within TTL** (stateless signed tokens — no server-side nonce store). This is an explicit design choice: DB sessions are also reusable within their 60-minute TTL (one session per user, looked up by `set_ctx`), and blob capabilities follow the same reusable pattern. Blob replay within the TTL window is acceptable because: (a) blob reads are idempotent, (b) git_oid is content-addressed (immutable), (c) max_bytes prevents abuse.

> **Phase-optional: single-use blob tokens** (higher-security deployments).
> If replay abuse is observed, add an optional best-effort replay cache:
> - Redis key `blob_jti:{jti}` with TTL = blob token TTL (300s)
> - On `read_blob()`, `SETNX` the jti key. If already exists → reject (replay).
> - Gate behind feature flag `blob-single-use` (off by default).
> - This is opt-in because: (a) adds Redis dependency to blob reads,
>   (b) increases latency (~1ms), (c) current threat model accepts replay.
> - Formalize in Phase 5 security review if abuse is observed.

</details>

---

### 3.3 NEW: `g5/g5-common/storage.semlift.md`

**Location**: `.semlift/specs/g5/g5-common/storage.semlift.md`

<details>
<summary>Full spec text (click to expand)</summary>

```yaml
---
child_spec_ids: []
complexity: medium
component_id: g5/g5-common/storage
dependencies:
  - g5/g5-common/error
granularity_score: 0.5
hierarchy_level: 1
is_root: false
language: rust
parent_spec_id: g5/g5-common
source_files:
  - g5-common/src/storage/mod.rs
  - g5-common/src/storage/local.rs
  - g5-common/src/storage/filestore.rs
  - g5-common/src/storage/config.rs
type: downlift
version: 0.1.0
---
```

# G5 Common Storage — RepoStorage Trait Abstraction

## Overview

The `storage` module in `g5-common` abstracts all repo filesystem operations behind
a `RepoStorage` trait with two providers: `LocalRepoStorage` (dev/single-node) and
`FilestoreRepoStorage` (prod/multi-node via Google Cloud Filestore NFS).

Both providers expose POSIX paths via `local_path()` because git operations require
a real filesystem.

## Behavior

### RepoStorage Trait

```rust
// g5-common/src/storage/mod.rs

use std::path::{Path, PathBuf};
use async_trait::async_trait;
use bytes::Bytes;

/// Metadata about a file or directory entry.
#[derive(Debug, Clone)]
pub struct StorageEntry {
    pub path: String,          // relative to repo root
    pub is_dir: bool,
    pub size: Option<u64>,
    pub last_modified: Option<chrono::DateTime<chrono::Utc>>,
}

/// Abstraction over repository storage backends.
/// All paths are relative to the repo root (`{repo_id}/`).
#[async_trait]
pub trait RepoStorage: Send + Sync + 'static {
    /// List entries in a directory, up to `depth` levels deep.
    async fn list_tree(
        &self, repo_id: &str, path: &str, depth: u32,
    ) -> Result<Vec<StorageEntry>, StorageError>;

    /// Read file content as bytes.
    async fn read_file(
        &self, repo_id: &str, path: &str,
    ) -> Result<Bytes, StorageError>;

    /// Write file content (creates intermediate dirs).
    async fn write_file(
        &self, repo_id: &str, path: &str, content: &[u8],
    ) -> Result<(), StorageError>;

    /// Check if a path exists.
    async fn exists(
        &self, repo_id: &str, path: &str,
    ) -> Result<bool, StorageError>;

    /// Delete a file.
    async fn delete_file(
        &self, repo_id: &str, path: &str,
    ) -> Result<(), StorageError>;

    /// Get the local filesystem path for a repo (bare repo root).
    ///
    /// **Invariant**: Returns `Some(path)` for ALL currently supported backends
    /// (Local and Filestore). Git operations require a real filesystem path.
    /// `None` is reserved for future object-store backends where git operations
    /// are unsupported — callers that need git MUST treat `None` as a hard error,
    /// not a graceful degradation.
    fn local_path(&self, repo_id: &str) -> Option<PathBuf>;

    /// Get the worktree root directory.
    ///
    /// Worktrees are created under this directory, NOT under `local_path()`.
    /// This path MUST be on local SSD (not NFS) for performance.
    /// Returns `None` only for object-store backends that don't support worktrees.
    ///
    /// Callers construct per-(worker, repo, branch) worktree paths as:
    /// `worktree_root() / "{worker_id}-{repo_id}-{branch}"`
    fn worktree_root(&self) -> Option<PathBuf>;

    /// Initialize storage for a new repo (create directories, etc.)
    async fn init_repo(&self, repo_id: &str) -> Result<PathBuf, StorageError>;

    /// Remove all storage for a repo.
    async fn remove_repo(&self, repo_id: &str) -> Result<(), StorageError>;
}

#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    #[error("not found: {0}")]
    NotFound(String),
    #[error("path traversal blocked: {0}")]
    PathTraversal(String),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("storage backend error: {0}")]
    Backend(String),
}
```

### LocalRepoStorage Provider

```rust
// g5-common/src/storage/local.rs

pub struct LocalRepoStorage {
    base_dir: PathBuf,         // e.g. /data/repos (original, for display)
    canonical_base: PathBuf,   // canonicalized once at construction
    worktree_dir: PathBuf,     // e.g. /data/worktrees (local SSD)
}

impl LocalRepoStorage {
    pub fn new(base_dir: PathBuf, worktree_dir: PathBuf) -> Result<Self, StorageError> {
        let canonical_base = base_dir.canonicalize()
            .map_err(|e| StorageError::Backend(
                format!("base_dir does not exist or is inaccessible: {e}"),
            ))?;
        // Ensure worktree_dir exists
        std::fs::create_dir_all(&worktree_dir)
            .map_err(|e| StorageError::Backend(
                format!("worktree_dir cannot be created: {e}"),
            ))?;
        Ok(Self { base_dir, canonical_base, worktree_dir })
    }

    fn repo_dir(&self, repo_id: &str) -> PathBuf {
        self.canonical_base.join(repo_id)
    }

    /// Path traversal protection — **lexical validation** only.
    ///
    /// `canonicalize()` requires the target to already exist on disk, which
    /// breaks `write_file()` to new paths and `create_dir_all()` patterns.
    /// Instead we validate *before* touching the filesystem:
    ///   1. Reject absolute paths
    ///   2. Reject `..` (ParentDir) components — this means paths like
    ///      `a/b/../c` are intentionally rejected even when they resolve
    ///      to a safe target. Callers are responsible for path normalization
    ///      before invoking storage methods.
    ///   3. Join against the canonicalized root (init-time)
    /// This prevents path traversal while allowing writes to non-existent files.
    fn validate_path(&self, rel_path: &Path) -> Result<PathBuf, StorageError> {
        use std::path::Component;

        // Reject absolute paths
        if rel_path.is_absolute() {
            return Err(StorageError::PathTraversal(
                format!("absolute path rejected: {}", rel_path.display()),
            ));
        }

        // Reject any `..` component (even if followed by valid segments)
        for component in rel_path.components() {
            if matches!(component, Component::ParentDir) {
                return Err(StorageError::PathTraversal(
                    format!("parent traversal rejected: {}", rel_path.display()),
                ));
            }
        }

        // Safe to join — root was canonicalized at construction time
        Ok(self.canonical_base.join(rel_path))
    }
}

#[async_trait]
impl RepoStorage for LocalRepoStorage {
    async fn list_tree(&self, repo_id: &str, path: &str, depth: u32)
        -> Result<Vec<StorageEntry>, StorageError>
    {
        let root = self.repo_dir(repo_id);
        let target = self.validate_path(&Path::new(repo_id).join(path))?;
        self.walk_dir(&root, &target, depth).await
    }

    async fn read_file(&self, repo_id: &str, path: &str)
        -> Result<Bytes, StorageError>
    {
        let full = self.validate_path(&Path::new(repo_id).join(path))?;
        tokio::fs::read(&full).await
            .map(Bytes::from)
            .map_err(|e| match e.kind() {
                std::io::ErrorKind::NotFound =>
                    StorageError::NotFound(path.into()),
                _ => StorageError::Io(e),
            })
    }

    fn local_path(&self, repo_id: &str) -> Option<PathBuf> {
        Some(self.canonical_base.join(repo_id))
    }

    fn worktree_root(&self) -> Option<PathBuf> {
        Some(self.worktree_dir.clone())
    }

    async fn init_repo(&self, repo_id: &str) -> Result<PathBuf, StorageError> {
        // init_repo validates repo_id then creates the directory
        let dir = self.validate_path(Path::new(repo_id))?;
        tokio::fs::create_dir_all(&dir).await?;
        Ok(dir)
    }

    async fn remove_repo(&self, repo_id: &str) -> Result<(), StorageError> {
        let dir = self.validate_path(Path::new(repo_id))?;
        if dir.exists() {
            tokio::fs::remove_dir_all(&dir).await?;
        }
        Ok(())
    }

    async fn write_file(&self, repo_id: &str, path: &str, content: &[u8])
        -> Result<(), StorageError>
    {
        let full = self.validate_path(&Path::new(repo_id).join(path))?;
        // create_dir_all for the parent — safe because validate_path
        // already rejected traversal; the parent is within canonical_base
        if let Some(parent) = full.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        tokio::fs::write(&full, content).await?;
        Ok(())
    }

    // ... exists, delete_file follow same validate_path pattern
}
```

> **`write_file` caller enumeration** — all code paths that call
> `RepoStorage::write_file()` MUST be validated for path-injection safety:
>
> | Caller | Path source | Safe? |
> |--------|-------------|-------|
> | `GitService::create_worktree` | Worker-initiated; path derived from internal repo_id + branch name (no user input) | Yes |
> | Blob extraction (`BlobService::extract_to_repo`) | Worker-initiated; path from `git_oid` deterministic mapping (no user input) | Yes |
> | Draft-commit API (`POST /repos/:id/files`) | **User-supplied `path` parameter** — MUST pass through `validate_path()` before `write_file()` | Yes, if enforced |
>
> No other callers exist in the current codebase. Any new `write_file()` call
> site MUST be reviewed for path-injection risk. The draft-commit API is the
> only path where user-controlled input reaches `write_file()`, and
> `validate_path()` rejects absolute paths and `..` components lexically.
```

### FilestoreRepoStorage Provider

```rust
// g5-common/src/storage/filestore.rs

pub struct FilestoreRepoStorage {
    /// NFS mount point, e.g. /mnt/g5-repos (bare repos live here)
    mount_point: PathBuf,
    /// Tenant prefix for multi-tenancy, e.g. "tenants/acme"
    tenant_prefix: Option<String>,
    /// Local SSD path for worktrees (NOT on NFS).
    /// e.g. /tmp/g5-worktrees or emptyDir volume mount.
    worktree_dir: PathBuf,
}

impl FilestoreRepoStorage {
    pub fn new(
        mount_point: PathBuf,
        tenant_prefix: Option<String>,
        worktree_dir: PathBuf,
    ) -> Self {
        Self { mount_point, tenant_prefix, worktree_dir }
    }
}

#[async_trait]
impl RepoStorage for FilestoreRepoStorage {
    // Inherits lexical validate_path() from shared base (same as LocalRepoStorage).
    // Additional Filestore-specific behavior:
    // - Health check verifies NFS mount is live (statfs)
    // - Locking on bare repo directories during fetch/update (see below)
    // - Worktree directories are per-(worker, repo, branch) — no locking needed

    fn local_path(&self, repo_id: &str) -> Option<PathBuf> {
        let mut path = self.mount_point.clone();
        if let Some(prefix) = &self.tenant_prefix {
            path = path.join(prefix);
        }
        Some(path.join(repo_id))
    }

    fn worktree_root(&self) -> Option<PathBuf> {
        Some(self.worktree_dir.clone())
    }

    // ... all other methods delegate to same logic as LocalRepoStorage
    // with adjusted base_dir
}

/// Bare repo locking for concurrent fetch safety on NFS.
///
/// The locking mechanism is **lockfile creation with `O_CREAT | O_EXCL`**
/// (atomic on all POSIX filesystems including NFSv3+).
/// We do NOT use `flock()` because its semantics vary across NFS versions
/// and mount options (it may silently degrade to local-only locking).
///
/// Protocol:
///   1. Attempt `open("{repo_dir}/.g5-lock", O_CREAT | O_EXCL | O_WRONLY)`
///   2. Write `{worker_id}:{pid}:{unix_timestamp}` to the lockfile
///   3. Perform git fetch / pack operations
///   4. Close + unlink the lockfile
///   5. If step 1 fails (EEXIST): read lockfile, extract `worker_id:pid:timestamp`.
///      Stale detection is NOT purely timestamp-based (a large `git fetch` may
///      exceed 5 minutes). Instead:
///        a. Parse `worker_id` from lockfile content
///        b. Ping worker via API: `GET /workers/{worker_id}/health`
///        c. If worker is unreachable or reports no active fetch → lock is stale → unlink + retry
///        d. If worker is alive AND reports active fetch → lock is valid → back off
///        e. Fallback: if API ping fails (network partition), use timestamp threshold (15 min)
///           as a conservative last resort — chosen to exceed the longest plausible `git fetch`
///      Back off (100ms–2s jitter) and retry up to 30s between attempts.
///
/// This provides:
///   - Atomic lock acquisition (O_EXCL is atomic even on NFSv3)
///   - Stale lock detection (worker_id + PID + timestamp in lockfile, with API liveness check)
///   - No dependency on NFS lock daemon (statd/lockd)
///
/// **Required NFS assumptions**:
///   - NFSv3+ or NFSv4 (for `O_EXCL` atomicity guarantee)
///   - Mount option `hard` (not `soft`) to prevent silent data loss
///   - Mount option `noac` or `actimeo=0` on the bare repo parent directory
///     to ensure lockfile visibility is not delayed by attribute caching
///
/// **Git performance mount options** (recommended for all NFS-backed repos):
///   - `lookupcache=pos` — cache only positive lookups, reduces `LOOKUP` RPCs
///     during `git status` / `git diff` on large repos (thousands of files)
///   - `noatime` — suppresses access-time updates on reads, avoids a SETATTR
///     RPC per file read. Critical for `git status` which stats every tracked file.
///   - `rsize=1048576,wsize=1048576` — 1MB read/write sizes, reduces round-trips
///     for pack-file transfers during `git fetch`
///   - `nconnect=4` (NFSv4.1+ / kernel 5.3+) — multiple TCP connections per mount,
///     improves throughput for concurrent `git` operations across workers
///
///   Example mount: `mount -t nfs -o hard,noac,lookupcache=pos,noatime,\
///     rsize=1048576,wsize=1048576,nconnect=4 10.0.0.2:/vol1 /mnt/g5-repos`
///
/// **Conformance test** (CI, runs against real NFS mount):
///   - Two processes attempt concurrent `create_lock()` on same path
///   - Exactly one succeeds; the other retries after backoff
///   - Stale lock is detected via worker API liveness check (not purely timestamp-based)
impl FilestoreRepoStorage {
    async fn acquire_repo_lock(&self, repo_id: &str) -> Result<LockGuard, StorageError> {
        let lock_path = self.repo_dir(repo_id).join(".g5-lock");
        // O_CREAT | O_EXCL atomic creation — see protocol above
        // Returns LockGuard that unlinks on Drop
        todo!("lockfile implementation")
    }
}
```

### Directory Layout

RepoStorage manages **two independent directory trees**:

```
repo_root/                               # NFS (Filestore) or local disk
├── {repo_id}/                           # bare git repo (shared, read-heavy)
│   ├── HEAD
│   ├── objects/
│   ├── refs/
│   ├── .blobs/                          # content-addressed blob fanout
│   │   └── {oid[0:2]}/{oid[2:4]}/{oid}
│   └── .g5-lock                         # O_CREAT|O_EXCL lockfile (Filestore only)

worktree_root/                           # MUST be local SSD (separate mount)
└── {worker_id}-{repo_id}-{branch}/      # per-(worker, repo, branch) — UNIQUE
    ├── src/
    └── ...
```

> **Critical**: `repo_root` and `worktree_root` are **separate filesystem paths**.
> In the Filestore provider, `repo_root` is an NFS mount (`/mnt/g5-repos`) while
> `worktree_root` is local SSD (`/tmp/g5-worktrees` or `emptyDir` volume).
> In the Local provider, they MAY be the same disk but are still configured
> separately. `RepoStorage` exposes both via `fn repo_root()` and
> `fn worktree_root()` — callers must never assume they share a filesystem.

**Sharing & locking rules**:
- **Bare repos** (`{repo_id}/`): Shared across all workers **on NFS**. Read-heavy
  after initial clone. Lockfile-based locking (`O_CREAT|O_EXCL`) required during
  `git fetch` to prevent concurrent pack corruption (Filestore only; local mode
  is single-worker).
- **Worktrees** (`{worker_id}-{repo_id}-{branch}/`): Unique per
  (worker, repo, branch) combination. **No locking needed** — each worker
  operates on its own directory. Lives on local SSD, not on NFS.
- **Local provider**: Single-worker; locking is unnecessary (but harmless).
- **Filestore provider**: Multi-worker; bare repo locking is REQUIRED.

**Hard storage placement requirement** (non-negotiable for performance):
- **Bare repos + `.blobs/`**: Live on NFS (Filestore). Shared, read-heavy,
  write-infrequent (only on fetch/extraction). NFS latency is acceptable.
- **Worktrees**: **MUST live on local ephemeral SSD** (per-worker tmpdir or
  emptyDir volume). Git checkout, diff, and file editing are latency-sensitive
  (sub-ms per file). NFS round-trip (~1-5ms/op) on worktree file I/O would
  destroy interactive editing and diff performance.
- This split is a deployment invariant: Kubernetes pod spec must mount NFS
  PVC for bare repos and use `emptyDir` or local SSD for worktrees.
- `worktree_root` is **ephemeral** — contents are lost on pod restart.
  This is acceptable because worktrees are derived from bare repos (re-checkout).
```

### StorageConfig

```rust
// g5-common/src/storage/config.rs

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "provider", rename_all = "snake_case")]
pub enum StorageConfig {
    Local {
        base_dir: PathBuf,
        worktree_dir: PathBuf,
    },
    Filestore {
        mount_point: PathBuf,
        tenant_prefix: Option<String>,
        worktree_dir: PathBuf,
    },
}

impl StorageConfig {
    pub fn build(&self) -> Result<Arc<dyn RepoStorage>, StorageError> {
        match self {
            Self::Local { base_dir, worktree_dir } =>
                Ok(Arc::new(LocalRepoStorage::new(
                    base_dir.clone(), worktree_dir.clone(),
                )?)),
            Self::Filestore { mount_point, tenant_prefix, worktree_dir } =>
                Ok(Arc::new(FilestoreRepoStorage::new(
                    mount_point.clone(), tenant_prefix.clone(),
                    worktree_dir.clone(),
                ))),
        }
    }
}
```

| Env Var | Default | Description |
|---------|---------|-------------|
| `G5_STORAGE_PROVIDER` | `local` | `local` or `filestore` |
| `G5_STORAGE_BASE_DIR` | `/data/repos` | Local provider base dir |
| `G5_STORAGE_WORKTREE_DIR` | `/tmp/g5-worktrees` | Worktree root (MUST be local SSD, not NFS) |
| `G5_STORAGE_MOUNT_POINT` | — | Filestore NFS mount point |
| `G5_STORAGE_TENANT_PREFIX` | — | Multi-tenant path prefix |

## Dependencies

### Internal Dependencies
- **g5/g5-common/error** — `G5Error` for error conversions

### External Dependencies
- **async-trait** — Object-safe async trait
- **bytes** — `Bytes` for file content
- **chrono** — `DateTime<Utc>` for entry timestamps
- **tokio** — Async filesystem operations
- **serde** — Configuration deserialization
- **thiserror** — Error derive

## Implementation Requirements

### File Structure
```
g5-common/src/storage/
├── mod.rs          — RepoStorage trait, StorageError, StorageEntry
├── local.rs        — LocalRepoStorage implementation
├── filestore.rs    — FilestoreRepoStorage (NFS mount) implementation
└── config.rs       — StorageConfig enum (Local | Filestore)
```

Feature-gated: `storage-local` (default), `storage-filestore`.

### Success Criteria Checklist

- [ ] `RepoStorage` trait is object-safe (`Send + Sync + 'static`)
- [ ] `LocalRepoStorage` wraps the current inline `repos_base_dir` logic
- [ ] `LocalRepoStorage::validate_path()` uses **lexical** validation (reject absolute + `..` components), NOT `canonicalize()` — so writes to new files work
- [ ] `LocalRepoStorage::new()` canonicalizes `base_dir` once at init; `validate_path()` never canonicalizes `target`
- [ ] `FilestoreRepoStorage` uses NFS mount path, adds lockfile-based (`O_CREAT|O_EXCL`) bare-repo locking
- [ ] NFS assumptions documented: NFSv3+, `hard` mount, `noac`/`actimeo=0` for lock directory
- [ ] Conformance test: two processes attempt concurrent lock acquisition on same bare repo
- [ ] Both providers implement path traversal protection via `validate_path()`
- [ ] `local_path()` returns `Some(PathBuf)` for BOTH providers; `None` reserved for future object-store only
- [ ] `worktree_root()` returns `Some(PathBuf)` for BOTH providers; path MUST be on local SSD (not NFS)
- [ ] `worktree_root()` and `local_path()` return different filesystem paths when using Filestore provider
- [ ] `GitService::create_worktree()` uses `storage.worktree_root()` for worktree placement (never `local_path()`)
- [ ] Bare repos + `.blobs/` on NFS; worktrees on local ephemeral SSD (hard deployment invariant)
- [ ] `StorageConfig` constructs the correct provider from env vars
- [ ] Feature gates control compilation of each provider

</details>

---

### 3.4 NEW: `g5/g5-api/manifest_service.semlift.md`

**Location**: `.semlift/specs/g5/g5-api/manifest_service.semlift.md`

> Addresses gap F4 ("No ManifestService"). Previously listed only in Open Questions;
> this stub spec gives it a real spec home so the verification criterion
> "FS migration key abstractions have spec homes" is satisfied.

<details>
<summary>Full spec text (click to expand)</summary>

```yaml
---
child_spec_ids: []
complexity: medium
component_id: g5/g5-api/manifest_service
dependencies:
  - g5/g5-common/storage
  - g5/g5-api/db
granularity_score: 0.5
hierarchy_level: 2
is_root: false
language: rust
parent_spec_id: g5/g5-api
source_files:
  - g5-api/src/services/manifest_service.rs
type: downlift
version: 0.1.0
---
```

# G5 API Manifest Service — Repo Checkout Manifest Tracking

## Overview

The `ManifestService` tracks which repos are checked out on which workers,
their current HEAD SHAs, and whether local copies are stale relative to
upstream. It is the single source of truth for "does worker X already have
repo Y at commit Z?" and drives cache-invalidation decisions for the
`RepoStorage` layer.

## Behavior

### ManifestEntry DTO

```rust
// g5-api/src/services/manifest_service.rs (or g5-common/src/manifest.rs for DTOs)

use uuid::Uuid;
use chrono::{DateTime, Utc};

/// One row per (worker, repo) combination.
#[derive(Debug, Clone)]
pub struct ManifestEntry {
    pub worker_id: Uuid,
    pub repo_id: Uuid,
    pub local_head: String,           // git SHA of locally checked-out HEAD
    pub upstream_head: Option<String>, // latest known upstream HEAD (from last fetch)
    pub last_fetched_at: Option<DateTime<Utc>>,
    pub storage_path: String,         // path on the storage backend
    pub stale: bool,                  // local_head != upstream_head
}
```

### ManifestService Interface

```rust
pub struct ManifestService;

impl ManifestService {
    /// Register that a worker has checked out a repo at a given SHA.
    pub async fn register_checkout(
        &self,
        db: &mut ScopedDb<'_>,
        worker_id: Uuid,
        repo_id: Uuid,
        head_sha: &str,
        storage_path: &str,
    ) -> Result<ManifestEntry, G5Error>;

    /// Update upstream_head after a fetch; marks entry stale if diverged.
    pub async fn update_upstream_head(
        &self,
        db: &mut ScopedDb<'_>,
        repo_id: Uuid,
        upstream_head: &str,
    ) -> Result<u64, G5Error>;  // returns count of entries marked stale

    /// Get all stale entries for a worker (needs re-fetch).
    pub async fn stale_repos(
        &self,
        db: &mut ScopedDb<'_>,
        worker_id: Uuid,
    ) -> Result<Vec<ManifestEntry>, G5Error>;

    /// Remove manifest entry when a worker removes a repo checkout.
    pub async fn deregister_checkout(
        &self,
        db: &mut ScopedDb<'_>,
        worker_id: Uuid,
        repo_id: Uuid,
    ) -> Result<(), G5Error>;
}
```

### Schema Addition

```sql
CREATE TABLE repo_manifests (
    worker_id       UUID NOT NULL REFERENCES workers(id) ON DELETE CASCADE,
    -- NOTE: `workers` is a system-global table (NOT tenant-scoped). It does
    -- not have an org_id column and is exempt from RLS. The FK here crosses
    -- the RLS boundary intentionally: repo_manifests is tenant-scoped (via
    -- org_id + RLS), but the worker it references is a system resource.
    -- This is safe because: (a) the FK only enforces referential integrity
    -- (worker must exist), not access control, and (b) RLS on repo_manifests
    -- already restricts which manifest rows a given session can see.
    repo_id         UUID NOT NULL REFERENCES workspaces(id),  -- repos are workspace children
    -- NOTE: In the current schema, repos ARE workspaces — the `workspaces`
    -- table stores repository metadata (name, url, default_branch). The FK
    -- target is correct. If repos are ever separated into a distinct table,
    -- this FK must be migrated to `REFERENCES repos(id)` and the column
    -- should be renamed accordingly.
    org_id          UUID NOT NULL,
    local_head      TEXT NOT NULL,
    upstream_head   TEXT,
    last_fetched_at TIMESTAMPTZ,
    storage_path    TEXT NOT NULL,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (worker_id, repo_id)
);

CREATE INDEX idx_manifests_org ON repo_manifests(org_id);
CREATE INDEX idx_manifests_stale ON repo_manifests(repo_id)
    WHERE upstream_head IS DISTINCT FROM local_head;

-- RLS (applied in Phase 3 with other tenant tables)
ALTER TABLE repo_manifests ENABLE ROW LEVEL SECURITY;
ALTER TABLE repo_manifests FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON repo_manifests
    USING (org_id = ANY(authz.current_org_ids()));
```

## Dependencies

### Internal Dependencies
- **g5/g5-common/storage** — `RepoStorage` (for storage_path references)
- **g5/g5-api/db** — `ScopedDb` for tenant-scoped queries

### External Dependencies
- **sqlx** — Postgres queries (via `ScopedDb`)
- **uuid**, **chrono** — Standard types

## Implementation Requirements

### Success Criteria Checklist

- [ ] `ManifestEntry` DTO defined with `worker_id`, `repo_id`, `local_head`, `upstream_head`, `stale`
- [ ] `ManifestService` methods accept `&mut ScopedDb<'_>` (not raw PgPool)
- [ ] `repo_manifests` table has org_id + RLS
- [ ] `update_upstream_head` marks entries stale when `local_head != upstream_head`
- [ ] `deregister_checkout` cascades on worker deletion
- [ ] DTOs defined in `g5-common/src/manifest.rs` for shared use by workers

</details>

---

## 4. Proposed Changes — Modified Specs (Before/After Diffs)

### 4.1 MODIFY: `g5/g5-common/auth.semlift.md`

**Gaps addressed**: S1 (iss/aud), S7 (worker auth context)

#### Change 1: Add `iss`, `aud`, `jti` to `JwtClaims`

**BEFORE** (auth.semlift.md §Behavior → JwtClaims Struct):
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JwtClaims {
    pub sub: String,          // user ID (UUID string)
    pub org: String,          // current org ID (UUID string)
    pub scopes: Vec<String>,  // effective OAuth scopes
    pub iat: i64,             // issued-at Unix timestamp
    pub exp: i64,             // expires-at Unix timestamp
}
```

**AFTER**:
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JwtClaims {
    pub sub: String,          // user ID (UUID string)
    /// Selected org context — advisory, used for UX defaults (e.g. which
    /// org dashboard to show). NOT used for authorization.
    /// Authorization uses OrgGuard membership check + capability minting.
    /// A user may belong to multiple orgs; this field reflects the org
    /// they last selected in the UI, not a security boundary.
    pub org: String,          // active org ID (UUID string) — advisory only
    pub scopes: Vec<String>,  // effective OAuth scopes
    pub iss: String,          // issuer URL (e.g. "g5-api")
    pub aud: Vec<String>,     // audience (e.g. ["g5-api"])
    pub jti: String,          // JWT ID (UUID, for request tracing and future replay detection capability)
    pub iat: i64,             // issued-at Unix timestamp
    pub exp: i64,             // expires-at Unix timestamp
}
```

> **D1 clarification**: `JwtClaims.org` is the user's *selected* org context,
> not an authorization claim. OrgGuard does NOT compare `claims.org` to the
> URL's `:orgId` — it performs a real membership check via
> `AuthzService::is_org_member(user_id, url_org_id)`. This means a user can
> access any org they belong to, regardless of which org is in their JWT.

#### Change 2: Add `iss`/`aud` validation to `JwtValidator`

**BEFORE** (auth.semlift.md §Behavior → JwtValidator Struct → Construction):
```
The `Validation` is configured to:
- Accept ONLY the `ES256` algorithm (ECDSA using P-256 and SHA-256)
- Validate the `exp` claim (expiration)
- Use zero clock skew tolerance (the `leeway` field must be set to `0`)
- NOT validate `iss`, `aud`, or `sub` claims (G5 tokens don't use these standard validations)
```

**AFTER**:
```
The `Validation` is configured to:
- Accept ONLY the `ES256` algorithm (ECDSA using P-256 and SHA-256)
- Validate the `exp` claim (expiration)
- Use zero clock skew tolerance (the `leeway` field must be set to `0`)
- Validate `iss` matches configured issuer
- Validate `aud` includes configured audience
- Require `exp`, `sub`, `org`, `iss`, `aud`, `jti` claims

> **`iss`/`aud` source** (A3 consistency): Both values are injected via
> environment variables (`G5_JWT_ISSUER`, `G5_JWT_AUDIENCE`), NOT hardcoded.
> This prevents iss/aud mismatch between issuing service (g5-api at token
> creation) and validating service (g5-api at token decode, or g5d worker).
> Both services MUST load the same env vars (typically from the same
> Kubernetes ConfigMap / Secret). **Startup guardrail**: g5-api MUST refuse
> to start if either `G5_JWT_ISSUER` or `G5_JWT_AUDIENCE` is unset.
> Default values (e.g. `"g5-api"`) are intentionally NOT provided — an
> explicit configuration forces operators to set consistent values across
> all deployments and prevents silent misconfiguration where the issuer
> and audience silently diverge.

Implementation:
```rust
let iss = std::env::var("G5_JWT_ISSUER")
    .expect("G5_JWT_ISSUER must be set");
let aud = std::env::var("G5_JWT_AUDIENCE")
    .expect("G5_JWT_AUDIENCE must be set");
validation.set_issuer(&[&iss]);
validation.set_audience(&[&aud]);
validation.required_spec_claims =
    HashSet::from(["exp", "sub", "org", "iss", "aud", "jti"]);
```

**Required unit tests** (mandatory — JWT crate behavior is subtly inconsistent
across versions):
```rust
#[test] fn rejects_missing_iss() { /* decode fails */ }
#[test] fn rejects_missing_aud() { /* decode fails */ }
#[test] fn rejects_missing_jti() { /* decode fails */ }
#[test] fn rejects_wrong_iss() { /* iss != "g5-api" */ }
#[test] fn rejects_wrong_aud() { /* aud doesn't contain "g5-api" */ }
#[test] fn rejects_expired_token() { /* exp < now */ }
#[test] fn rejects_missing_sub() { /* decode fails */ }
```

These tests MUST be written against the actual `jsonwebtoken` crate version
pinned in `Cargo.lock`, not assumed from docs. If upgrading the JWT crate,
re-run these tests first.
```

#### Change 3: Add `AuthContext::Worker` variant

**BEFORE** (auth.semlift.md — implicit, only User and Service in module re-exports):
```rust
// AuthContext is defined in middleware, but g5-common conceptually supports:
// User(JwtClaims) and Service
```

**AFTER** — add to module scope:
```rust
pub enum AuthContext {
    User(JwtClaims),
    Worker {
        worker_id: Uuid,
        org_id: Uuid,
        task_id: Uuid,
        scopes: Vec<Scope>,
    },
}

impl AuthContext {
    /// Returns the actor's UUID (user_id or worker_id).
    /// Both variants can provide this, so it works for capability minting.
    pub fn actor_id(&self) -> Result<Uuid, G5Error> {
        match self {
            Self::User(claims) => Uuid::parse_str(&claims.sub)
                .map_err(|_| G5Error::Internal {
                    message: "invalid user ID in JWT".into(),
                }),
            Self::Worker { worker_id, .. } => Ok(*worker_id),
        }
    }

    /// Returns the effective scopes for this auth context.
    /// Used by `mint_db_capability` to embed scopes in the capability.
    pub fn scopes(&self) -> Vec<String> {
        match self {
            Self::User(claims) => claims.scopes.clone(),
            Self::Worker { scopes, .. } => scopes.iter().map(|s| s.to_string()).collect(),
        }
    }

    pub fn require_user_id(&self) -> Result<Uuid, G5Error> {
        match self {
            Self::User(claims) => Uuid::parse_str(&claims.sub)
                .map_err(|_| G5Error::Internal {
                    message: "invalid user ID in JWT".into(),
                }),
            Self::Worker { .. } => Err(G5Error::Forbidden {
                missing_scopes: vec!["user-auth-required".into()],
            }),
        }
    }

    /// Returns the caller's *selected* org context from their JWT.
    /// **WARNING**: This is advisory only — do NOT use for authorization.
    /// Authorization decisions MUST go through OrgGuard + capability minting.
    /// Named `selected_org_id` (not `require_org_id`) to make the advisory
    /// nature explicit. For workers, returns the org_id they were assigned.
    pub fn selected_org_id(&self) -> Result<Uuid, G5Error> {
        match self {
            Self::User(claims) => Uuid::parse_str(&claims.org)
                .map_err(|_| G5Error::Internal {
                    message: "invalid org ID in JWT".into(),
                }),
            Self::Worker { org_id, .. } => Ok(*org_id),
        }
    }
}
```

Worker session tokens are org-scoped, time-limited (15 min), task-bound, and carry
only the scopes needed for task execution.

> **Worker vs User JWT claim shapes**: User JWTs decode into `JwtClaims`.
> Worker session JWTs carry a custom `task_id` claim and decode into
> `AuthContext::Worker { worker_id, org_id, task_id, scopes }` — a
> *different* claim shape. Both are validated by the same `JwtValidator`
> (same signing key, same `iss`/`aud` checks), but the middleware inspects
> the `task_id` field to determine which `AuthContext` variant to construct.
> This is explicitly NOT the same struct.
>
> **Discriminator rule** (pinned, deterministic):
> 1. Decode JWT into `serde_json::Value` (not a typed struct)
> 2. Validate standard claims (`iss`, `aud`, `exp`, `jti`) on the raw Value
> 3. **If `task_id` key exists and is a valid UUID string → `AuthContext::Worker`**
> 4. **If `task_id` is absent → `AuthContext::User`** (decode Value into `JwtClaims`)
> 5. User JWTs MUST NOT include a `task_id` field; if present, the token is
>    treated as a worker token (and will fail user-only endpoints).
> 6. Worker `sub` field == `worker_id` (string-encoded UUID). This is an
>    invariant: `worker_id == Uuid::parse_str(&value["sub"])`.
>
> **Implementation**: Decode into `serde_json::Value` first, then branch.
> Do NOT use `serde(untagged)` enum (error messages are poor and order-
> dependent). Separate typed structs (`JwtClaims` for users, `WorkerClaims`
> for workers) with shared validation is the recommended pattern.
>
> **Required tests**:
> ```rust
> #[test] fn user_token_without_task_id_is_user() { ... }
> #[test] fn worker_token_with_task_id_is_worker() { ... }
> #[test] fn worker_sub_equals_worker_id() { ... }
> #[test] fn user_token_with_task_id_treated_as_worker() { ... }
> ```

#### Change 4: Remove `ServiceTokenValidator` / deprecate `Service` variant

**BEFORE**:
```
pub use service_token::ServiceTokenValidator;
```

**AFTER**:
```
// ServiceTokenValidator is REMOVED.
// Workers authenticate via short-lived session tokens (JWTs with
// AuthContext::Worker claims), not static service tokens.
// The `Service` variant of AuthContext is removed; use `Worker` instead.
```

**Migration note**: `service_claims()` (which synthesizes admin-equivalent claims with
`i64::MAX` expiry) is deleted. Workers receive real scoped tokens.

#### Change 5: Update success criteria

**ADD** to Success Criteria:
```
- [ ] `JwtClaims` includes fields: `iss: String`, `aud: Vec<String>`, `jti: String`
- [ ] `JwtValidator` validates `iss` and `aud` claims
- [ ] `AuthContext` enum has `User(JwtClaims)` and `Worker { worker_id, org_id, task_id, scopes }` — no `Service` variant
- [ ] `AuthContext::require_user_id()` and `selected_org_id()` replace the 7 copies of `extract_user_id`
- [ ] `ServiceTokenValidator` is removed from the crate
- [ ] Unit tests: rejects missing `iss`, missing `aud`, missing `jti`, wrong `iss`, wrong `aud`, expired token, missing `sub`
- [ ] Worker/user discrimination: `task_id` present → Worker, absent → User (deterministic, tested)
- [ ] Decode strategy: `serde_json::Value` first → branch by `task_id` → typed struct (no `serde(untagged)`)
- [ ] Worker `sub` field == `worker_id` (string-encoded UUID, invariant)
```

---

### 4.2 MODIFY: `g5/g5-api/auth.semlift.md`

**Gaps addressed**: S1 (iss/aud in minting), S7 (worker token minting), S9 (startup guardrails)

#### Change 1: Add `iss`/`aud`/`jti` to minted JWTs

**BEFORE** (auth.semlift.md §Behavior → JWT Minting):
```
JWT payload structure (matches g5-common::auth::JwtClaims):
{
  sub: string,        // user ID (UUID as string)
  org: string,        // current org ID (UUID as string)
  scopes: string[],   // effective OAuth scopes
  iat: number,        // issued-at Unix timestamp
  exp: number         // expires-at Unix timestamp
}
```

**AFTER**:
```
JWT payload structure (matches g5-common::auth::JwtClaims):
{
  sub: string,        // user ID (UUID as string)
  org: string,        // current org ID (UUID as string)
  scopes: string[],   // effective OAuth scopes
  iss: "g5-api",      // issuer (prevents cross-env token reuse)
  aud: ["g5-api"],    // audience
  jti: string,        // JWT ID (UUID, for request tracing and future replay detection capability)
  iat: number,        // issued-at Unix timestamp
  exp: number         // expires-at Unix timestamp
}
```

#### Change 2: Add worker session token minting

**ADD** new section after JWT minting:

```
### Worker Session Token Minting

- **`JwtMinter::mint_worker_session_token(worker_id, org_id, task_id, scopes)
    -> Result<String, AppError>`** — Mints a short-lived JWT (15 minutes)
    for a registered worker. The token carries:
    - `sub: worker_id.to_string()`
    - `org: org_id.to_string()`
    - `scopes: scopes` (restricted — only task-execution scopes)
    - `iss: "g5-api"`, `aud: ["g5-api"]`
    - `jti: Uuid::new_v4().to_string()`
    - Custom claim `task_id: task_id.to_string()`
    - `exp: iat + 900` (15 minutes)

- **`JwtMinter::refresh_worker_session_token(worker_id, current_token)
    -> Result<String, AppError>`** — Validates the current token, checks
    that the bound task is still `running` in the DB, and mints a
    fresh 15-minute token. Called on worker heartbeat.
    **Expiration leeway**: The refresh endpoint MUST accept tokens that
    expired ≤ 30 seconds ago. Without this leeway, a worker whose token
    expires during a long-running request has no valid token to present
    for refresh, creating a deadlock. Implementation: set
    `validation.leeway = 30` (seconds) in the `jsonwebtoken::Validation`
    config used by the refresh endpoint ONLY — all other endpoints use
    zero leeway. **Implementation detail**: This leeway MUST be configured
    on a **separate `JwtValidator` instance** used exclusively by the
    refresh endpoint handler. The shared `JwtValidator` in middleware
    (which validates all other requests) MUST retain `leeway = 0`. Do NOT
    pass a single `JwtValidator` with `leeway = 30` to both the middleware
    and the refresh handler — that would silently accept expired tokens
    on all endpoints.

Worker tokens differ from user tokens:
- Shorter lifetime (15 min vs 1 hour)
- Scoped to exactly one org_id
- Bound to a specific task_id
- Restricted scopes (no `org:admin`, no `billing:admin`)
```

#### Change 3: Add startup guardrails

**ADD** new section:

```
### Startup Guardrails

The auth module MUST validate critical secrets at startup and refuse to
start if they are missing or insecure:

1. `JWT_SIGNING_KEY` — MUST be present, non-empty, and not match known
   test values (e.g., the test key from the repo's test fixtures).
2. `SERVICE_TOKEN_SECRET` — MUST NOT equal `"test-service-token"` or
   be unset in production (`G5_ENV != "development"`).

Implementation in `JwtMinter::new()`:
```rust
impl JwtMinter {
    pub fn new(private_key_pem: &str, public_key_pem: &str) -> Result<Self, AppError> {
        if private_key_pem.is_empty() {
            return Err(AppError::Internal {
                message: "JWT_SIGNING_KEY is empty — refusing to start".into(),
            });
        }
        // ... existing key parsing ...
    }
}
```
```

#### Change 4: Update success criteria

**ADD**:
```
- [ ] Minted JWTs include `iss`, `aud`, `jti` fields
- [ ] `mint_worker_session_token` creates 15-minute org-scoped token
- [ ] `refresh_worker_session_token` validates task is still running; accepts tokens expired ≤ 30s (leeway on refresh endpoint ONLY)
- [ ] Startup refuses if `JWT_SIGNING_KEY` is empty or matches test key
- [ ] Startup refuses if `SERVICE_TOKEN_SECRET` == "test-service-token" in prod
```

---

### 4.3 MODIFY: `g5/g5-api/routes.semlift.md`

**Gaps addressed**: S2 (OrgGuard), S6 (RequireScopes enforcement)

#### Change 1: Add OrgGuard to router assembly

**OrgGuard now lives in `g5-api`** (moved from `g5-authz` to avoid pulling
`AppState`/`AppError`/`axum` into the authz crate — see A1). The middleware
calls `Arc<dyn AuthzService>::is_org_member()` via `AppState.authz`.

```rust
// g5-api/src/routes/middleware/org_guard.rs

use axum::{extract::{Path, State}, http::Request, middleware::Next, response::Response};

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
            let is_member = state.authz
                .is_org_member(user_id, params.org_id)
                .await
                .map_err(|e| AppError::from(e))?;  // From<AuthzError> for AppError in g5-api
            if !is_member {
                return Err(AppError::forbidden("not a member of this org"));
            }
        }
        AuthContext::Worker { org_id, .. } => {
            if *org_id != params.org_id {
                return Err(AppError::forbidden("worker not scoped to this org"));
            }
        }
    }
    Ok(next.run(request).await)
}
```

**Router assembly —**
```rust
pub fn build_router(state: Arc<AppState>) -> Router {
    Router::new()
        // Public API v1
        .nest("/api/v1/orgs", orgs::router())
        .nest("/api/v1/workspaces", workspaces::router())
        // ...
```

**AFTER**:
```rust
pub fn build_router(state: Arc<AppState>) -> Router {
    Router::new()
        // Org-scoped routes — OrgGuard validates membership
        .nest("/api/v1/orgs/:orgId",
            orgs::org_detail_router()
                .merge(workspaces::org_workspaces_router())
                .layer(axum::middleware::from_fn_with_state(
                    state.clone(), org_guard)))
        // Resource-scoped routes — resource org guard
        .nest("/api/v1/workspaces",
            workspaces::router()
                .layer(axum::middleware::from_fn_with_state(
                    state.clone(), workspace_org_guard)))
        // Non-org routes (auth, users, instance) — no OrgGuard
        .nest("/api/v1/auth", auth::router())
        .nest("/api/v1/users", users::router())
        .nest("/api/v1/instance", instance::router())
        // Internal API — worker session token auth
        .nest("/api/internal", internal::router())
        .with_state(state)
}
```

#### Change 2: Update handler pattern

**BEFORE** (routes.semlift.md §Behavior → Thin handler pattern):
```rust
async fn handler(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(params): Path<PathParams>,
    Query(query): Query<QueryParams>,
    Json(body): Json<RequestDTO>,
) -> Result<Json<ResponseEnvelope<ResponseDTO>>, AppError> {
    let result = state.some_service.some_method(params, body).await?;
    Ok(Json(ResponseEnvelope::ok(result.into())))
}
```

**AFTER**:
```rust
async fn handler(
    // 1. OrgGuard has already validated org membership (tower layer)
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(params): Path<PathParams>,
    // 2. RequireScopes has already checked required scopes (tower layer)
) -> Result<Json<ResponseEnvelope<ResponseDTO>>, AppError> {
    // 3. Mint scoped DB capability (identity-agnostic — works for User and Worker)
    let actor_id = auth.actor_id()?;
    let scopes = auth.scopes();
    let db_cap = state.authz
        .mint_db_capability(actor_id, params.org_id, &scopes).await
        .map_err(AppError::from)?;

    // 4. Create scoped DB connection (RLS-enforced)
    let mut scoped = ScopedDb::from_capability(
        &state.tenant_pool, &db_cap).await?;

    // 5. Call service with scoped connection
    let result = state.some_service
        .some_method(&mut scoped, params).await?;

    // 6. ScopedDb dropped → clear_ctx + rollback/commit
    Ok(Json(ResponseEnvelope::ok(result.into())))
}
```

#### Change 3: Mandate RequireScopes enforcement

**BEFORE** (routes.semlift.md §Behavior → Scope Requirements):
```
(Scope table is documented but states "9 routes enforce")
```

**AFTER**:
```
All 125 routes MUST enforce scopes via RequireScopes middleware.
The scope table below is MANDATORY — not advisory. Every route handler
route group MUST apply RequireScopes with the scopes listed.

Example application:
```rust
pub fn workspace_detail_router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/:id", get(get_workspace)
            .layer(RequireScopes::new(&["workspaces:read"])))
        .route("/:id",
            patch(update_workspace)
                .delete(delete_workspace)
                .layer(RequireScopes::new(&["workspaces:write"])))
}
```

Machine-readable scope map (embedded in spec, authoritative):

```yaml
# scope_map.yaml — compiled into build.rs for verification
# NOTE: The full 125-route mapping MUST be populated from
# OAUTH_SECURITY_AUDIT.md Appendix A before Phase 1 launch.
# The abbreviated listing below is for spec brevity only —
# the CI test `all_routes_have_required_scopes` will fail
# if any route is missing from this file.
routes:
  # Workspace routes
  - path: "/api/v1/orgs/:orgId/workspaces"
    method: GET
    scopes: ["workspaces:read"]
  - path: "/api/v1/orgs/:orgId/workspaces"
    method: POST
    scopes: ["workspaces:write"]
  - path: "/api/v1/workspaces/:id"
    method: [GET]
    scopes: ["workspaces:read"]
  - path: "/api/v1/workspaces/:id"
    method: [PATCH, DELETE]
    scopes: ["workspaces:write"]
  # ... (full 125-route mapping from OAUTH_SECURITY_AUDIT Appendix A)
```

CI enforcement gate (REQUIRED test):

**Canonical mechanism**: The `scoped_route!` macro is **mandatory** for all
route registrations. It statically registers `(method, path, scopes)` in a
global `inventory` registry and applies the `RequireScopes` layer. Routes
added without the macro will not compile (no `Router::route()` call is
generated).

> **`inventory` crate dependency note**: `inventory` uses linker-section magic
> (`ctor` / `.init_array` on ELF, `__DATA,__mod_init_func` on Mach-O) to
> collect registrations at link time. **Platform constraints**:
> - No WASM support (`ctor` doesn't work without a traditional linker)
> - Limited cross-compilation targets (requires ELF or Mach-O)
> - Compile-time footprint: each `submit!` adds a static constructor
>
> These constraints are acceptable for g5-api (Linux server binary) but
> would block future WASM targets. If WASM support is ever needed, replace
> `inventory` with a build.rs-generated registry.
>
> **Conditional compilation gap**: If a `scoped_route!` call is guarded by
> `#[cfg(feature = "x")]`, the route exists at runtime only when that feature
> is enabled, but the `inventory` registration is also absent — so the
> `all_routes_have_required_scopes` CI test won't detect the missing entry
> unless the test is run **with `--all-features`**.  Mitigation: CI MUST run
> the scope verification test with `cargo test --all-features` (or with a
> feature matrix covering every combination that adds routes). If a feature
> flag is used purely to disable dead code (not to add/remove routes), this
> gap does not apply.

```rust
/// Macro: registers a route with mandatory scopes.
/// Expands to:
///   1. Router::route() call with RequireScopes layer
///   2. inventory::submit!(RouteRegistration { method, path, scopes })
macro_rules! scoped_route {
    ($router:expr, $method:ident, $path:expr, $handler:expr, $scopes:expr) => {{
        inventory::submit!(RouteRegistration {
            method: stringify!($method).to_string(),
            path: $path.to_string(),
            scopes: $scopes.iter().map(|s| s.to_string()).collect(),
        });
        $router.route($path, $method($handler)
            .layer(RequireScopes::new($scopes)))
    }};
}
```

CI test verifies the registry matches `scope_map.yaml` exactly:

```rust
#[test]
fn all_routes_have_required_scopes() {
    let scope_map = load_scope_map(); // from include_str!("scope_map.yaml")
    let registry: Vec<_> = inventory::iter::<RouteRegistration>().collect();
    for entry in &registry {
        let expected = scope_map.get(&(entry.path.clone(), entry.method.clone()))
            .unwrap_or_else(|| panic!("route {} {} not in scope_map", entry.method, entry.path));
        assert_eq!(entry.scopes, *expected,
            "scope mismatch for {} {}", entry.method, entry.path);
    }
    assert_eq!(registry.len(), scope_map.len(),
        "scope_map has entries for routes that don't exist");
}
```

**Bypass prevention** (enforcing that `scoped_route!` is truly mandatory):

The macro claim "routes without macro won't compile" is only true if
direct `Router::route()` calls are banned. In Rust, any code can call
`Router::route()` outside the macro. To make this *actually* enforceable:

1. **CI lint gate** (REQUIRED from Phase 1, lowest friction):
   ```bash
   # In CI pipeline (e.g., Justfile lint target):
   # Fail if any .rs file in g5-api/src/ directly calls .route( outside the macro.
   # Scans the ENTIRE crate — not just routes/ — because .route() could appear
   # in api/, middleware/, or any other module that builds a Router.
   if grep -rn '\.route(' g5-api/src/ \
       --include='*.rs' \
       | grep -v 'scoped_route!' \
       | grep -v '// bypass:allowed' \
       | grep -v '#\[cfg(test)\]' \
       | grep -v 'mod tests' \
       | grep -v '#\[test\]' \
       | grep -v '/// ' \
       | grep -v '///' \
       | grep -v 'doc(hidden)'; then
       echo "ERROR: Direct .route() call found outside scoped_route! macro"
       echo "All routes MUST use scoped_route!() for scope enforcement."
       echo "If this is a test or doc example, add '// bypass:allowed' comment."
       exit 1
   fi
   ```

2. **Wrapper type** (defense-in-depth, recommended Phase 2):
   Route modules return `Vec<RouteDef>` assembled centrally, rather than
   building `Router` directly. This makes bypass structurally impossible:
   ```rust
   pub struct RouteDef {
       pub method: Method,
       pub path: &'static str,
       pub handler: BoxedHandler,
       pub scopes: &'static [&'static str],
   }
   // Routes are assembled by a single build_all_routes() function
   // that calls scoped_route! for each RouteDef.
   ```

3. **clippy deny** (optional, if custom lint available):
   `#![deny(clippy::disallowed_methods)]` with `.route` added to the
   disallowed list in `clippy.toml`. Provides compile-time enforcement.

The CI grep gate (option 1) is **required from Phase 1**. The wrapper type
(option 2) is recommended for Phase 2 as a structural guarantee.
```

#### Change 4: Update internal worker endpoints

**ADD**:
```
Internal worker endpoints (`/api/internal/workers/*`) MUST:
- Require `AuthContext::Worker` (not `AuthContext::Service`)
- Validate that `worker_id` in the URL path matches the worker_id in the token
  **on every endpoint** (registration, heartbeat, task fetch, result submit).
  This prevents a compromised worker from impersonating another.
- Accept worker session tokens (15-minute JWTs), not static service tokens

Example enforcement pattern:
```rust
async fn worker_endpoint(
    auth: AuthContext,
    Path(params): Path<WorkerPathParams>,
) -> Result<..., AppError> {
    let AuthContext::Worker { worker_id, .. } = &auth else {
        return Err(AppError::forbidden("worker auth required"));
    };
    if *worker_id != params.worker_id {
        return Err(AppError::forbidden("worker_id mismatch"));
    }
    // ... handler logic
}
```
```

---

### 4.4 MODIFY: `g5/g5-api/services.semlift.md`

**Gaps addressed**: S3 (raw PgPool), S5 (AuthzService/BlobService in AppState)

#### Change 1: Replace AppState

**BEFORE** (services.semlift.md §Behavior → mod.rs):
```rust
pub struct AppState {
    pub workspace_service: WorkspaceService,
    pub ontology_service: OntologyService,
    pub job_service: JobService,
    pub task_service: TaskService,
    pub chat_service: ChatService,
    pub draft_service: DraftService,
    pub comment_service: CommentService,
    pub search_service: SearchService,
    pub auth_service: AuthService,
    pub org_service: OrgService,
    pub user_service: UserService,
    pub permission_service: PermissionService,
    pub spend_service: SpendService,
    pub clone_service: CloneService,
    pub event_bus: EventBus,
    pub db: PgPool,
}
```

**AFTER**:
```rust
pub struct AppState {
    // Authorization — trait object, swappable for testing
    pub authz: Arc<dyn AuthzService>,
    // Blob access — trait object, swappable for testing
    pub blob: Arc<dyn BlobService>,
    // Repository storage — trait object (Local or Filestore)
    pub repo_storage: Arc<dyn RepoStorage>,

    // Tenant data pool (g5_app role, RLS-enforced via ScopedDb)
    pub tenant_pool: PgPool,
    // System pool for non-tenant operations (migrations, health, global)
    pub system_pool: PgPool,

    // Services — receive ScopedDb per request, NOT PgPool
    pub workspace_service: WorkspaceService,
    pub ontology_service: OntologyService,
    pub job_service: JobService,
    pub task_service: TaskService,
    pub chat_service: ChatService,
    pub draft_service: DraftService,
    pub comment_service: CommentService,
    pub search_service: SearchService,
    pub auth_service: AuthService,
    pub org_service: OrgService,
    pub user_service: UserService,
    pub permission_service: PermissionService,
    pub spend_service: SpendService,
    pub clone_service: CloneService,
    pub event_bus: EventBus,
    pub redis: RedisPool,
}
```

#### Change 2: Update service struct pattern

**BEFORE** (services.semlift.md §Behavior → Service struct pattern):
```rust
pub struct WorkspaceService {
    db: PgPool,
    redis: Option<RedisPool>,
}

impl WorkspaceService {
    pub fn new(db: PgPool, redis: Option<RedisPool>) -> Self {
        Self { db, redis }
    }

    pub async fn get_workspace(&self, id: Uuid) -> Result<Workspace, G5Error> {
        // sqlx query + business logic
    }
}
```

**AFTER**:
```rust
pub struct WorkspaceService {
    // No PgPool — receives ScopedDb per-request from handler
}

impl WorkspaceService {
    pub fn new() -> Self { Self {} }

    pub async fn get_workspace(
        &self,
        db: &mut ScopedDb<'_>,
        id: Uuid,
    ) -> Result<Workspace, G5Error> {
        // org_id filter is IMPLICIT via RLS — no need to bind it
        sqlx::query_as!(Workspace,
            "SELECT * FROM workspaces WHERE id = $1",
            id,
        )
        .fetch_optional(db.tx())
        .await?
        .ok_or(G5Error::NotFound { entity: "workspace".into() })
    }
}
```

> **Migration note (Phase 1)**: During Phase 1, services may temporarily
> retain an internal `PgPool` for non-tenant queries (e.g., global config,
> health checks). The `ScopedDb` pattern is introduced for all tenant-data
> queries. In Phase 3, after `org_id` denormalization and RLS enablement,
> the raw pool is removed entirely from service structs.

#### Change 3: Update transaction pattern

**BEFORE**:
```rust
pub async fn clone_workspace(&self, source_id: Uuid, req: ...) -> Result<...> {
    let mut tx = self.db.begin().await?;
    // ... all mutations within tx ...
    tx.commit().await?;
}
```

**AFTER**:
```rust
pub async fn clone_workspace(
    &self,
    db: &mut ScopedDb<'_>,
    source_id: Uuid,
    req: ...,
) -> Result<...> {
    // Transaction is already begun by ScopedDb::from_capability()
    // All mutations go through db.tx() which is RLS-scoped
    // ...
    // Do NOT commit here — the route handler commits via db.commit()
    // or the ScopedDb is dropped and the transaction rolls back
}
```

#### Change 4: Update dependencies

**ADD** to dependencies list:
```yaml
dependencies:
  - g5/g5-authz    # AuthzService trait, DbCapability, BlobCapability
  - g5/g5-blob     # BlobService trait
  - g5/g5-common/storage  # RepoStorage trait
```

---

### 4.5 MODIFY: `g5/g5-api/db.semlift.md`

**Gaps addressed**: S4 (single DB role), S10 (no authz pool)

#### Change 1: Replace single pool with dual pool

**BEFORE** (db.semlift.md §Behavior → Database Pool Initialization):
```rust
pub async fn init_pool(config: &DatabaseConfig) -> Result<PgPool, DbError>
```

**AFTER**:
```rust
/// Create the tenant data pool using the `g5_app` role.
/// This pool is used by ScopedDb — all queries are subject to RLS.
pub async fn init_app_pool(config: &DatabaseConfig) -> Result<PgPool, DbError>

/// Create the authz pool using the `authz_writer` role.
/// This pool is used by InProcessAuthz to INSERT capabilities
/// and is NOT exposed to the service layer.
pub async fn init_authz_pool(config: &AuthzDatabaseConfig) -> Result<PgPool, DbError>

/// Pool sizing (shared Cloud SQL budget):
/// - app_pool: max=15, min=3
/// - authz_pool: max=10, min=2
/// - membership_pool: max=5, min=1 (read-only, low concurrency)
/// Total (30) ≤ Cloud SQL max_connections
```

#### Change 2: Add authz schema migration

**ADD** new migration file `XXX_authz_session_capability.sql`:

```sql
-- 1. Create the authz schema
CREATE SCHEMA IF NOT EXISTS authz;

-- 2. Session context table — keyed by pg_backend_pid()
--    Populated per-transaction by set_ctx(); cleared by clear_ctx() / ROLLBACK.
CREATE TABLE authz.session_ctx (
    pid        INT PRIMARY KEY,  -- always inserted as pg_backend_pid(); no DEFAULT to reduce confusion
    org_ids    UUID[] NOT NULL,  -- array of org UUIDs the session is scoped to
    user_id    UUID NOT NULL,
    scopes     TEXT[] NOT NULL,
    session_id UUID,             -- references user_sessions.session_id (audit trail)
                                 -- nullable for backward compat during Phase 1; NOT NULL from Phase 2
    set_at     TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- REVOKE all direct access from g5_app
REVOKE ALL ON authz.session_ctx FROM g5_app;

-- 3. User sessions table — reusable sessions scoped per-user (60 min TTL)
--    Replaces the old single-use active_capabilities model.
--    One row per user (UNIQUE on user_id); UPSERT refreshes on each mint.
CREATE TABLE authz.user_sessions (
    session_id   UUID PRIMARY KEY,
    user_id      UUID NOT NULL UNIQUE,  -- one active session per actor (user or worker)
                                        -- column is named user_id for schema compatibility;
                                        -- stores actor_id from AuthContext::actor_id()
    org_ids      UUID[] NOT NULL,       -- all orgs the actor belongs to (at mint time)
    scopes       TEXT[] NOT NULL,
    expires_at   TIMESTAMPTZ NOT NULL,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    refreshed_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_session_expires ON authz.user_sessions(expires_at);
CREATE INDEX idx_session_user ON authz.user_sessions(user_id);

-- 4. set_ctx() — SECURITY DEFINER, validates session, writes session_ctx
--    Reads from user_sessions but does NOT delete the row (sessions are reusable).
--    Updates refreshed_at to track last usage.
CREATE OR REPLACE FUNCTION authz.set_ctx(p_session_id UUID)
RETURNS VOID
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = authz, public
AS $$
DECLARE
    v_sess authz.user_sessions%ROWTYPE;
BEGIN
    SELECT * INTO v_sess
    FROM authz.user_sessions
    WHERE session_id = p_session_id AND expires_at > now();

    IF NOT FOUND THEN
        RAISE EXCEPTION 'invalid or expired session: %', p_session_id
            USING ERRCODE = '28000';
    END IF;

    INSERT INTO authz.session_ctx (pid, org_ids, user_id, scopes, session_id)
    VALUES (pg_backend_pid(), v_sess.org_ids, v_sess.user_id, v_sess.scopes, v_sess.session_id)
    -- NOTE: session_ctx.org_ids is populated from user_sessions.org_ids (the
    -- merged union), NOT from the DbCapability.org_ids that the handler received.
    -- This means session_ctx may grant broader org visibility than any single
    -- DbCapability nominally authorizes. This is the intended design — see the
    -- "Intended widening behavior" section in mint_db_capability's doc comment.
    -- RLS sees all of the actor's verified orgs; OrgGuard at the route layer
    -- constrains which org's data the handler actually accesses.
    ON CONFLICT (pid) DO UPDATE SET
        org_ids    = EXCLUDED.org_ids,
        user_id    = EXCLUDED.user_id,
        scopes     = EXCLUDED.scopes,
        session_id = EXCLUDED.session_id,
        set_at     = now();

    -- Defensive: if an existing row has set_at in the future (clock skew),
    -- log a warning. The ON CONFLICT DO UPDATE overwrites it regardless,
    -- but this helps surface infrastructure issues.
    -- In production, add pg_notify('authz_anomaly', ...) here.

    -- Update refreshed_at to track session activity (no DELETE — session is reusable).
    -- NOTE: There is a cosmetic race between this UPDATE and the UPSERT in
    -- mint_db_capability (which also sets refreshed_at via ON CONFLICT UPDATE).
    -- Both write now(); the loser's timestamp is harmlessly overwritten.
    -- This is acceptable in Phase 1 — refreshed_at is advisory, not load-bearing.
    UPDATE authz.user_sessions SET refreshed_at = now()
    WHERE session_id = v_sess.session_id;
END;
$$;

-- 5. current_org_ids() — STABLE, PARALLEL RESTRICTED, called by RLS policies
--    **CI enforcement**: A migration regression test MUST verify that this
--    function retains `PARALLEL RESTRICTED` across schema changes:
--
--    DO $$ BEGIN
--        ASSERT (SELECT proparallel FROM pg_proc
--                WHERE proname = 'current_org_ids'
--                  AND pronamespace = (SELECT oid FROM pg_namespace WHERE nspname = 'authz'))
--               = 'r',
--            'current_org_ids() must be PARALLEL RESTRICTED (proparallel = r)';
--    END $$;
--
--    This prevents accidental changes (e.g., a future `CREATE OR REPLACE` that
--    omits the PARALLEL RESTRICTED clause) from silently breaking RLS semantics
--    in parallel queries.
--    Returns UUID[] (array of org IDs) for multi-org RLS support.
CREATE OR REPLACE FUNCTION authz.current_org_ids()
RETURNS UUID[]
LANGUAGE sql
STABLE
SECURITY DEFINER
PARALLEL RESTRICTED
SET search_path = authz
AS $$
    SELECT org_ids FROM authz.session_ctx WHERE pid = pg_backend_pid();
$$;

-- 6. clear_ctx() — cleanup on transaction end
CREATE OR REPLACE FUNCTION authz.clear_ctx()
RETURNS VOID
LANGUAGE sql
SECURITY DEFINER
SET search_path = authz
AS $$
    DELETE FROM authz.session_ctx WHERE pid = pg_backend_pid();
$$;

-- 7. Grants for g5_app (can call functions, cannot touch tables)
GRANT USAGE ON SCHEMA authz TO g5_app;
GRANT EXECUTE ON FUNCTION authz.set_ctx(UUID) TO g5_app;
GRANT EXECUTE ON FUNCTION authz.current_org_ids() TO g5_app;
GRANT EXECUTE ON FUNCTION authz.clear_ctx() TO g5_app;

-- 8. Grants for authz_writer (session minting — UPSERT + DELETE expired)
GRANT INSERT, UPDATE, DELETE ON authz.user_sessions TO authz_writer;

-- 9. Grants for authz_ctx_writer (set_ctx/clear_ctx function owner)
GRANT INSERT, UPDATE, DELETE ON authz.session_ctx TO authz_ctx_writer;
GRANT SELECT ON authz.session_ctx TO authz_ctx_writer;
GRANT SELECT, UPDATE ON authz.user_sessions TO authz_ctx_writer;

-- 10. Set SECURITY DEFINER function ownership
-- These functions run as authz_ctx_writer regardless of the calling role.
-- **How SECURITY DEFINER + NOLOGIN works**: When g5_app calls set_ctx(),
-- PostgreSQL executes the function body with authz_ctx_writer's privileges
-- (which include INSERT/UPDATE on session_ctx). The NOLOGIN attribute only
-- prevents direct connections as that role — it does NOT prevent SECURITY
-- DEFINER functions owned by the role from executing. This is the standard
-- PostgreSQL privilege-separation pattern for sensitive operations.
-- Without explicit ownership, they default to the migration user
-- which defeats the privilege-separation model.
ALTER FUNCTION authz.set_ctx(UUID) OWNER TO authz_ctx_writer;
ALTER FUNCTION authz.current_org_ids() OWNER TO authz_ctx_writer;
ALTER FUNCTION authz.clear_ctx() OWNER TO authz_ctx_writer;
```

#### Change 3: Add DB role setup migration

**ADD** new migration `XXX_db_roles.sql`:

```sql
-- NOTE: Role provisioning (CREATE ROLE + passwords) is handled by
-- Terraform / IaC, NOT by SQL migrations. Migrations contain ONLY
-- schema changes + GRANT statements. Passwords are injected from
-- GCP Secret Manager at infrastructure provisioning time.
--
-- This migration assumes roles already exist (created by IaC).
-- It applies grants idempotently.

-- 1. Application role grants (g5_app — used by g5-api service layer)
GRANT USAGE ON SCHEMA public TO g5_app;
GRANT SELECT, INSERT, UPDATE, DELETE ON ALL TABLES IN SCHEMA public TO g5_app;
GRANT USAGE, SELECT ON ALL SEQUENCES IN SCHEMA public TO g5_app;
ALTER DEFAULT PRIVILEGES IN SCHEMA public
    GRANT SELECT, INSERT, UPDATE, DELETE ON TABLES TO g5_app;

-- 2. Authz writer role grants (authz_writer — used by g5-authz module)
GRANT USAGE ON SCHEMA authz TO authz_writer;
GRANT INSERT, UPDATE, DELETE ON authz.user_sessions TO authz_writer;

-- 3. Authz context writer role grants (authz_ctx_writer — owns SECURITY DEFINER funcs)
-- NOTE: authz_ctx_writer role itself is created by Terraform/IaC (NOLOGIN role).
-- This migration only applies grants, consistent with the invariant that
-- CREATE ROLE + passwords are handled by IaC, not SQL migrations.
GRANT USAGE ON SCHEMA authz TO authz_ctx_writer;
GRANT SELECT, UPDATE ON authz.user_sessions TO authz_ctx_writer;
GRANT INSERT, UPDATE, DELETE, SELECT ON authz.session_ctx TO authz_ctx_writer;
```

#### Change 4: Add org_id denormalization migration

**ADD** new migration `XXX_add_org_id_for_rls.sql`:

```sql
-- Step 1: Add nullable org_id columns to 20 tables
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
ALTER TABLE draft_updates ADD COLUMN org_id UUID;
ALTER TABLE timeline_entries ADD COLUMN org_id UUID;
ALTER TABLE task_checkpoints ADD COLUMN org_id UUID;
ALTER TABLE search_documents ADD COLUMN org_id UUID;
ALTER TABLE workspace_members ADD COLUMN org_id UUID;
ALTER TABLE workspace_settings ADD COLUMN org_id UUID;
ALTER TABLE workspace_dependencies ADD COLUMN org_id UUID;
ALTER TABLE node_dependencies ADD COLUMN org_id UUID;
ALTER TABLE doc_node_links ADD COLUMN org_id UUID;
ALTER TABLE draft_participants ADD COLUMN org_id UUID;

-- Step 2: Backfill wave 1 (1-join)
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

-- Step 3: Backfill wave 2 (2-join, uses wave-1 cols)
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

-- Step 4: Backfill wave 3 (3-join)
UPDATE timeline_entries te SET org_id = t.org_id
    FROM tasks t WHERE te.task_id = t.id;
UPDATE task_checkpoints tc SET org_id = t.org_id
    FROM tasks t WHERE tc.task_id = t.id;
-- NOTE: node_dependencies.org_id is set from source_node_id's org.
-- Cross-org dependency edges (source_node in org A, target_node in org B)
-- are NOT supported. All dependency edges MUST be intra-org. If cross-org
-- edges exist in the data, the RLS policy org_id = ANY(current_org_ids())
-- would hide edges whose org_id doesn't match the session. Before running
-- this migration, run a validation query:
--   SELECT nd.id FROM node_dependencies nd
--     JOIN ontology_nodes src ON nd.source_node_id = src.id
--     JOIN ontology_nodes tgt ON nd.target_node_id = tgt.id
--     WHERE src.org_id != tgt.org_id;
-- If any rows are returned, they must be resolved (deleted or reassigned)
-- before enabling RLS, or the cross-org edges will become invisible.
UPDATE node_dependencies nd SET org_id = n.org_id
    FROM ontology_nodes n WHERE nd.source_node_id = n.id;
UPDATE doc_node_links dl SET org_id = d.org_id
    FROM docs d WHERE dl.doc_id = d.id;
UPDATE draft_updates du SET org_id = ds.org_id
    FROM draft_sessions ds WHERE du.session_id = ds.id;
UPDATE draft_participants dp SET org_id = ds.org_id
    FROM draft_sessions ds WHERE dp.session_id = ds.id;

-- Step 5: NOT NULL + FK
ALTER TABLE workspace_branches ALTER COLUMN org_id SET NOT NULL;
ALTER TABLE workspace_branches ADD CONSTRAINT fk_wb_org
    FOREIGN KEY (org_id) REFERENCES orgs(id);
-- ... repeat for all 20 tables ...

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

#### Change 5: Add RLS policy migration

**ADD** new migration `XXX_enable_rls.sql`:

```sql
-- Apply tenant_isolation RLS to every tenant-scoped table.
-- Uses ANY(authz.current_org_ids()) to support multi-org sessions.
-- For single-org requests, org_ids is a one-element array — ANY() still works.

ALTER TABLE workspaces ENABLE ROW LEVEL SECURITY;
ALTER TABLE workspaces FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON workspaces
    USING (org_id = ANY(authz.current_org_ids()));

ALTER TABLE workspace_branches ENABLE ROW LEVEL SECURITY;
ALTER TABLE workspace_branches FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON workspace_branches
    USING (org_id = ANY(authz.current_org_ids()));

ALTER TABLE jobs ENABLE ROW LEVEL SECURITY;
ALTER TABLE jobs FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON jobs
    USING (org_id = ANY(authz.current_org_ids()));

ALTER TABLE tasks ENABLE ROW LEVEL SECURITY;
ALTER TABLE tasks FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON tasks
    USING (org_id = ANY(authz.current_org_ids()));

ALTER TABLE ontology_nodes ENABLE ROW LEVEL SECURITY;
ALTER TABLE ontology_nodes FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON ontology_nodes
    USING (org_id = ANY(authz.current_org_ids()));

-- ... repeat for all 20+ tenant tables ...

-- NOTE: org_memberships is intentionally NOT RLS'd.
-- Membership checks (AuthzService::is_org_member) run BEFORE any session
-- is minted, so authz.current_org_ids() would be NULL.
-- Access is gated by the membership_pool role (SELECT-only on this table).
-- See §3.1 AuthzService for the full rationale.

-- Auto-set trigger (safety net for INSERT without explicit org_id)
-- For multi-org sessions, defaults to the FIRST org in the array.
-- Callers should always set org_id explicitly; this is a safety net only.
CREATE OR REPLACE FUNCTION set_org_id_from_session()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
    IF NEW.org_id IS NULL THEN
        -- PostgreSQL arrays are 1-based: [1] is the first element.
        NEW.org_id := (authz.current_org_ids())[1];
    END IF;
    RETURN NEW;
END;
$$;

CREATE TRIGGER trg_set_org_id BEFORE INSERT ON workspace_branches
    FOR EACH ROW EXECUTE FUNCTION set_org_id_from_session();
CREATE TRIGGER trg_set_org_id BEFORE INSERT ON jobs
    FOR EACH ROW EXECUTE FUNCTION set_org_id_from_session();
-- ... repeat for all tables with org_id ...
```

#### Change 6: Add ScopedDb to spec

**ADD** new section:

```
### ScopedDb Abstraction

`ScopedDb` is the core contract replacing `PgPool` for tenant-scoped queries.
Cannot be constructed directly — only via `ScopedDb::from_capability()`.

> **Transaction invariant** (MUST hold at all times):
> `ScopedDb::from_capability()` MUST call `set_ctx()` only *after* beginning
> a transaction (`pool.begin()`), and MUST NOT use autocommit connections.
> The entire correctness model — RLS scoping, Drop cleanup, session_ctx
> isolation — relies on `set_ctx()` executing inside a transaction that is
> either committed (with `clear_ctx()` called first) or rolled back.
>
> **Session-mode pooling is FORBIDDEN** in production. If a connection
> returns to the pool without transaction boundaries (session-mode),
> `session_ctx` keyed by `pg_backend_pid()` can linger across requests.
> Transaction-mode pooling (PgBouncer `transaction`, sqlx default) is
> required. If session-mode pooling is ever needed, add `clear_ctx()`
> to the pool's checkout/checkin hooks (sqlx's `after_connect` callback)
> — but this is NOT currently supported and MUST NOT be deployed without
> explicit review.
>
> **Runtime enforcement**: Add a startup-time assertion that verifies the
> pooling mode is `transaction`. This prevents accidental session-mode
> deployment:
>
> ```rust
> // main.rs startup sequence — after pool init, before serving traffic
> let pool_mode: String = sqlx::query_scalar(
>     "SHOW pool_mode"  // PgBouncer admin; or use config env var
> ).fetch_optional(&pool).await?.unwrap_or_default();
>
> // If connecting through PgBouncer, verify transaction mode.
> // If direct-to-Postgres (no PgBouncer), pool_mode query fails — that's fine.
> if !pool_mode.is_empty() && pool_mode != "transaction" {
>     panic!("FATAL: PgBouncer pool_mode is '{}', expected 'transaction'", pool_mode);
> }
>
> // Also enforce via config:
> let configured_mode = std::env::var("G5_DB_POOL_MODE").unwrap_or_else(|_| "transaction".into());
> if configured_mode != "transaction" {
>     panic!("FATAL: G5_DB_POOL_MODE='{}', only 'transaction' is supported", configured_mode);
> }
> ```
>
> **CI gate**: Add an integration test that asserts the pool mode check
> passes. This catches Terraform/Helm chart regressions that change
> PgBouncer mode.

```rust
pub struct ScopedDb<'a> {
    tx: sqlx::Transaction<'a, sqlx::Postgres>,
    org_ids: Vec<Uuid>,
    actor_id: Uuid,
    scopes: Vec<String>,
}

impl ScopedDb<'_> {
    pub async fn from_capability(
        pool: &PgPool,
        capability: &DbCapability,
    ) -> Result<Self, G5Error> {
        let mut tx = pool.begin().await?;
        sqlx::query("SELECT authz.set_ctx($1)")
            .bind(capability.session_id) // UUID parameter — references user_sessions row
            .execute(&mut *tx)
            .await
            .map_err(|_| G5Error::Unauthorized)?;
        Ok(Self {
            tx,
            org_ids: capability.org_ids.clone(),
            actor_id: capability.actor_id,
            scopes: capability.scopes.clone(),
        })
    }

    pub fn tx(&mut self) -> &mut sqlx::Transaction<'_, sqlx::Postgres> {
        &mut self.tx
    }

    /// All org IDs this session is scoped to.
    /// Single-org routes: one-element slice. Cross-org routes: all user orgs.
    pub fn org_ids(&self) -> &[Uuid] { &self.org_ids }

    /// Convenience: returns the first (or only) org ID.
    /// Returns `G5Error` if org_ids is empty (violates invariant — should never
    /// happen, but panicking in production makes debugging harder).
    /// Use this in single-org routes where the URL contains one org_id.
    ///
    /// **Naming note**: "primary" does not imply a default or privileged org.
    /// It means "the first element of the org_ids array," which for single-org
    /// routes is the only element. If multi-org routes need a different
    /// selection strategy, add a separate method (e.g. `selected_org_id()`).
    /// Renaming to `first_org_id()` was considered but `primary_org_id` is
    /// more self-documenting for the common single-org case.
    pub fn primary_org_id(&self) -> Result<Uuid, G5Error> {
        self.org_ids.first().copied().ok_or_else(|| G5Error::Internal {
            message: "ScopedDb has empty org_ids — invariant violation (capability \
                      should always have at least one org)".into(),
        })
    }

    pub fn actor_id(&self) -> Uuid { self.actor_id }

    /// Effective scopes for this session.
    /// Available for optional service-layer defense-in-depth checks.
    /// Primary scope enforcement is at the route layer (RequireScopes).
    pub fn scopes(&self) -> &[String] { &self.scopes }

    pub async fn commit(mut self) -> Result<(), G5Error> {
        // clear_ctx() failure is FATAL — a leaked session_ctx row means
        // the next request on this backend PID could see stale org context.
        // We abort the commit rather than risk context leakage.
        sqlx::query("SELECT authz.clear_ctx()")
            .execute(&mut *self.tx)
            .await
            .map_err(|e| G5Error::Internal {
                message: format!("clear_ctx failed — aborting commit: {e}"),
            })?;
        self.tx.commit().await.map_err(|e| G5Error::Internal {
            message: format!("commit failed: {e}"),
        })
    }
}

// Drop guard: correctness relies on transaction semantics, not async cleanup.
//
// NO async work in Drop. The design is:
// - set_ctx() runs inside `self.tx` (the same transaction used for all queries)
// - If commit() was never called, sqlx::Transaction::Drop issues ROLLBACK
// - ROLLBACK undoes the session_ctx INSERT from set_ctx() — row is never committed
// - Therefore: no async clear_ctx() call is needed or attempted in Drop
// - clear_ctx() is called only on the commit() happy path (before COMMIT)
// - Periodic cleanup (step 16 in main startup) handles edge cases:
//   process crash, OOM kill, connection pool leak, etc.
impl Drop for ScopedDb<'_> {
    fn drop(&mut self) {
        // No-op. sqlx::Transaction::Drop handles ROLLBACK.
        // session_ctx modifications from set_ctx() are rolled back with the tx.
    }
}
```

> **Cleanup semantics**:
> - **Happy path**: Handler calls `scoped_db.commit()` → `clear_ctx()` + `COMMIT`.
>   If `clear_ctx()` fails, commit is **aborted** (returns error, transaction rolls
>   back). A leaked session_ctx row is worse than a failed request.
> - **Error path**: Handler returns `Err` → `ScopedDb::drop()` → `ROLLBACK` (sqlx).
>   The session_ctx INSERT from `set_ctx()` was inside the same transaction, so
>   it is rolled back automatically. No async cleanup needed.
> - **Safety net**: Periodic task (every 5 min) deletes stale session_ctx rows:
>   `DELETE FROM authz.session_ctx WHERE set_at < now() - interval '1 hour'`.
>   This handles process-crash edge cases only (OOM kill, SIGKILL, connection
>   pool leak). It is NOT needed for normal operation — transaction rollback
>   handles the error path, and `clear_ctx()` handles the happy path.
>
> **Trust boundary for `current_org_ids()`**:
> - The ONLY way `session_ctx` gets populated is via `authz.set_ctx(uuid)` (SECURITY DEFINER).
> - `g5_app` cannot INSERT/UPDATE/DELETE on `authz.session_ctx` directly.
> - `current_org_ids()` returns NULL if no session is set for the current pid.
> - RLS policies use `USING (org_id = ANY(authz.current_org_ids()))`. When `current_org_ids()` is
>   NULL, the `ANY()` comparison evaluates to NULL (not TRUE), which means **no rows are visible**.
>   This is the correct fail-closed behavior: queries without a set session context see
>   zero tenant data.
>
> **Proof (include in migration as a regression test comment)**:
> ```sql
> -- Verify fail-closed: ANY(NULL) returns zero rows
> DO $$ BEGIN
>     ASSERT (SELECT count(*) FROM (SELECT 1 WHERE 'a0eebc99-9c0b-4ef8-bb6d-6bb9bd380a11'::uuid = ANY(NULL::uuid[])) t) = 0,
>         'ANY(NULL::uuid[]) must return zero rows (fail-closed invariant)';
> END $$;
> ```
> This assertion MUST be included in the authz schema migration to prevent
> future PostgreSQL version changes from altering `ANY(NULL)` semantics.
>
> **Connection pooler compatibility**: Works with transaction-mode connection pooling
> (e.g., PgBouncer in `transaction` mode). Both `set_ctx()` and queries execute inside
> the same SQL transaction on the same backend connection.
>
> **Pooling mode analysis**:
> - **Transaction-mode** (REQUIRED for production): Each transaction is pinned to one
>   backend. `set_ctx()` → queries → `clear_ctx()` all happen atomically. Rollback
>   clears `session_ctx`. No cross-request leakage is possible.
> - **Session-mode**: `session_ctx` is keyed by `pg_backend_pid()`, so it *works*,
>   but the risk profile is different: if a code path ever calls `set_ctx()` without
>   a wrapping transaction (violation of the transaction invariant above), the
>   `session_ctx` row persists until the next `set_ctx()` (ON CONFLICT overwrites)
>   or periodic cleanup. This is not inherently broken, but it **amplifies** bugs.
>   Session-mode is not prohibited for dev/test but is **not recommended** for
>   production without adding `clear_ctx()` to sqlx `after_connect` hooks.
>
> **Stale-PID risk under pooler reconnect**: When PgBouncer in transaction mode
> reuses a backend PID across different logical clients (e.g., a backend disconnects
> and reconnects, inheriting an old PID from the OS), a new request could briefly
> see stale `session_ctx` from a previous connection in the window between pooler
> checkout and the `set_ctx()` call. The `ON CONFLICT DO UPDATE` in `set_ctx()`
> handles the steady-state case, and the periodic cleanup task mitigates stale rows,
> but neither eliminates the inter-checkout window entirely.
>
> **Mitigation**: Configure sqlx `after_acquire` and `before_release` hooks to call
> `clear_ctx()` on every connection checkout/checkin. **This is REQUIRED from Phase 1
> onward** (before RLS goes live in Phase 2). Rationale: shipping Phase 1 without
> the safety net means the code path that Phase 2 depends on has *never been
> exercised in production*. Making it Phase 1 ensures the hooks are battle-tested
> before RLS enforcement raises the stakes. A stale `session_ctx` row from a
> crashed connection, combined with PID reuse, produces a **cross-tenant data leak**
> once RLS is live — not just an empty result set — because the stale row's
> `org_ids` may belong to a different tenant than the new request's caller.
>
> **Phase 0** (pre-authz schema): hooks are defense-in-depth (recommended but not
> critical, since `session_ctx` table does not yet exist).
> **Phase 1+**: hooks are **mandatory**.
>
> ```rust
> // sqlx pool configuration — REQUIRED from Phase 1
> PgPoolOptions::new()
>     .after_acquire(|conn, _meta| Box::pin(async move {
>         sqlx::query("SELECT authz.clear_ctx()")
>             .execute(&mut *conn).await.ok();
>         Ok(true)
>     }))
> ```
>
> **Accepted residual risk (Phase 0 only)**: If `after_acquire` hooks are not
> configured in Phase 0, the `authz` schema does not yet exist, so `session_ctx`
> is not populated and PID reuse has no security impact.

Performance characteristics:
- `current_org_ids()` is `STABLE` — evaluated once per statement, not per row
- PK-indexed on `session_ctx.pid` — index-only scan on tiny table
- Table bounded by pool size (~25 rows max), never grows with load
- `PARALLEL RESTRICTED` — runs on leader only; parallel workers have different
  `pg_backend_pid()` values, so they see their own (empty) `session_ctx` row,
  not the leader's. Without `PARALLEL RESTRICTED`, a parallel worker calling
  `current_org_ids()` would return NULL → RLS would filter all rows (fail-closed,
  which is safe but returns incorrect empty results). This is **non-negotiable**.
```

#### ScopedDb Integration with Service Layer

This section bridges the gap between the `ScopedDb` abstraction (above) and
service handlers. It shows how a `PgPool` in `AppState` becomes a `ScopedDb`
scoped to a specific org — the critical path for every tenant-scoped request.

**Route handler flow** (complete lifecycle):

```rust
// g5-api/src/routes/workspaces.rs
//
// 1. RequireScopes middleware already validated JWT scopes
// 2. OrgGuard already validated org membership
// 3. Handler receives AuthContext + AppState

async fn list_workspaces(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
    Path(org_id): Path<Uuid>,
) -> Result<Json<Vec<Workspace>>, AppError> {
    // Step 1: Mint a DB session (UPSERT into user_sessions — reusable, 60 min)
    // Single-org route: pass &[org_id] (one element).
    let capability = state.authz.mint_db_capability(
        auth.actor_id(),
        &[org_id],
        auth.scopes(),
    ).await.map_err(AppError::from)?;

    // Step 2: Convert PgPool → ScopedDb (begin tx + set_ctx)
    let mut db = ScopedDb::from_capability(&state.app_pool, &capability)
        .await
        .map_err(AppError::from)?;

    // Step 3: Run queries — RLS enforces org isolation automatically
    let workspaces = sqlx::query_as!(
        Workspace,
        "SELECT * FROM workspaces WHERE org_id = $1",
        db.primary_org_id()?,
    )
    .fetch_all(db.tx())
    .await?;

    // Step 4: Commit (clear_ctx + COMMIT). If this fails, tx is rolled back.
    db.commit().await.map_err(AppError::from)?;

    Ok(Json(workspaces))
}
```

> **Key invariants demonstrated**:
> - `ScopedDb` is the **only** way to run tenant queries. Services never
>   receive `PgPool` directly — they receive `ScopedDb` (or `&mut ScopedDb`).
> - The `WHERE org_id = $1` in queries is **defense-in-depth** — RLS already
>   filters by org. The explicit predicate helps the query planner (index scan)
>   and makes intent clear to code reviewers.
> - `db.commit()` MUST be called explicitly. If the handler returns `Err`,
>   `ScopedDb::drop()` fires → sqlx `ROLLBACK` → `set_ctx()` INSERT is undone.
> - The `mint_db_capability` → `ScopedDb::from_capability` two-step is
>   intentional: capability minting uses the `authz_pool` (authz_writer role),
>   while `ScopedDb` uses the `app_pool` (g5_app role). These are separate
>   connection pools with different DB permissions.

**Service method pattern** (for services that receive ScopedDb):

```rust
// g5-api/src/services/workspace_service.rs

pub struct WorkspaceService;

impl WorkspaceService {
    /// Services receive `&mut ScopedDb`, NOT `PgPool`.
    /// They don't know or care how the DB was scoped — they just run queries.
    pub async fn list_for_org(
        db: &mut ScopedDb<'_>,
    ) -> Result<Vec<Workspace>, G5Error> {
        let rows = sqlx::query_as!(
            Workspace,
            "SELECT * FROM workspaces WHERE org_id = $1",
            db.primary_org_id()?,
        )
        .fetch_all(db.tx())
        .await?;
        Ok(rows)
    }
}
```

> **Migration path** (Phase 1 → Phase 2):
> - **Phase 1**: Services keep their existing `PgPool` signatures internally.
>   `ScopedDb` is introduced *alongside* existing code. Route handlers mint
>   capabilities and construct `ScopedDb`, but may pass `db.tx()` directly
>   to services that still expect a generic executor.
> - **Phase 2**: Services are refactored to accept `&mut ScopedDb` directly.
>   This is a mechanical change (replace `pool: &PgPool` with `db: &mut ScopedDb`
>   in service signatures). Compile errors guide the migration — any service
>   that still accepts `PgPool` is either not yet migrated or is intentionally
>   unsoped (e.g., health checks, system-level queries).

#### Change 7: Update success criteria

**ADD**:
```
- [ ] `init_app_pool` creates pool with `g5_app` role
- [ ] `init_authz_pool` creates pool with `authz_writer` role
- [ ] `authz` schema with `session_ctx` table + `user_sessions` table + 3 SECURITY DEFINER functions
- [ ] Three DB roles: `g5_app`, `authz_writer`, `authz_ctx_writer`
- [ ] `g5_app` cannot INSERT/UPDATE/DELETE on `authz.session_ctx`
- [ ] `current_org_ids()` is STABLE, PARALLEL RESTRICTED, SECURITY DEFINER — returns `UUID[]`
- [ ] org_id column added to 20 tables with backfill migration
- [ ] RLS enabled + forced on all tenant tables — policies use `ANY(authz.current_org_ids())`
- [ ] Auto-set trigger on org_id for INSERT safety net (uses `(current_org_ids())[1]` for multi-org)
- [ ] `ScopedDb::from_capability()` calls `set_ctx()` with session_id UUID parameter
- [ ] `ScopedDb::from_capability()` calls `set_ctx()` only AFTER `pool.begin()` — never on autocommit
- [ ] Session-mode pooling FORBIDDEN — documented, enforced via startup assertion (`G5_DB_POOL_MODE`), and CI-gated
- [ ] `session_ctx` includes `session_id` column tracing back to `user_sessions` row
- [ ] `ScopedDb::commit()` calls `clear_ctx()` before commit
- [ ] `user_sessions` has UNIQUE on `user_id` — one session per user, UPSERT semantics
```

---

### 4.6 MODIFY: `g5/g5-api/main.semlift.md`

**Gaps addressed**: S9 (startup guardrails), S10 (authz pool)

#### Change 1: Update startup sequence

**BEFORE** (implied by db.semlift.md and services.semlift.md):
```
1. Parse configuration
2. Initialize tracing
3. Create CancellationToken
4. Initialize database pool
5. Run migrations
6. Connect to Redis
7. Construct services (each gets PgPool)
8. Assemble AppState
9. Build router
10. Start health check
11. Bind and serve
```

**AFTER**:
```
1.  Load config from environment
2.  Startup guardrails — refuse if JWT_SIGNING_KEY absent/weak,
    SERVICE_TOKEN_SECRET == "test-service-token" in prod
3.  Initialize tracing subscriber
4.  Create CancellationToken
5.  Initialize app_pool (g5_app role)
6.  Initialize authz_pool (authz_writer role)
6a. Initialize membership_pool (read-only role with SELECT on org_memberships)
    - **Role**: `g5_membership` (read-only, SELECT on `org_memberships` only)
    - **Env var**: `G5_MEMBERSHIP_DATABASE_URL` (same DB, different role credentials)
    - **Pool sizing**: max=5, min=1 (low concurrency — membership checks are
      cached in Redis after first hit; pool handles cache misses only)
    - **Config struct**: `MembershipDatabaseConfig { url, max_connections, min_connections }`
    - The role is provisioned by Terraform/IaC alongside the other DB roles
7.  Initialize migration_pool (g5_migrator role — DDL + GRANT privileges).
    This pool is used ONLY for running migrations and is closed after step 7b.
7a. Run migrations via migration_pool (NOT via app_pool or authz_pool).
    **Prerequisite**: Roles (g5_app, authz_writer, authz_ctx_writer, g5_migrator)
    MUST already exist — they are provisioned by Terraform/IaC, not by migrations.
    If roles don't exist, GRANT statements will fail. This is intentional:
    it prevents migrations from running against an unprovisioned database.
7b. Close migration_pool — runtime code never uses the migrator role.
8.  Connect to Redis
9.  Construct RepoStorage (Local or Filestore based on config)
10. Construct AuthzService (InProcessAuthz with membership_pool, authz_pool)
11. Construct BlobService (InProcessBlob with storage, verify_key)
12. Construct services (stateless — no PgPool injection)
13. Assemble AppState { authz, blob, repo_storage, tenant_pool, system_pool,
     services, event_bus, redis }
14. Build router with OrgGuard + RequireScopes layers
15. Start health check task
16. Start periodic session_ctx cleanup task (every 5 min):
    DELETE FROM authz.session_ctx WHERE set_at < now() - interval '1 hour'
    **Pool**: This task requires `authz_ctx_writer` privileges (the role
    that owns `session_ctx`). It MUST NOT use `g5_app` or the tenant pool.
    Use a dedicated connection from `authz_pool` with an `authz_ctx_writer`
    role, or (if authz_pool uses `authz_writer`) grant DELETE on
    `authz.session_ctx` to `authz_writer` explicitly.
17. Start periodic user_sessions pruning task (every 5 min):
    DELETE FROM authz.user_sessions WHERE expires_at < now()
    **Pool**: `authz_pool` (`authz_writer` role, which has DELETE on
    `authz.user_sessions`).
    This handles expired sessions. Sessions have a 60-minute TTL
    (refreshed on each mint call). Since sessions are reusable (not
    consumed by set_ctx), expired sessions simply accumulate until
    pruned. Bounded by concurrent user count, not request rate.
18. Bind and serve
```

---

### 4.7 MODIFY: `g5/infra.semlift.md`

**Gaps addressed**: F2 (no Filestore), S4 (no DB roles)

#### Change 1: Add Google Cloud Filestore section

**ADD** after GCS section:

```
### Google Cloud Filestore

Managed NFS share for multi-worker shared repository storage.

- **Instance**: `g5-repos` (Basic HDD tier, 1 TiB initially)
- **Mount path**: `/mnt/g5-repos/`
- **VPC peering**: Same VPC as GKE cluster
- **Firewall**: Allow NFS (port 2049) from GKE node CIDR
- **PV/PVC**: ReadWriteMany PersistentVolumeClaim via Filestore CSI driver
- **Sizing**: 1 TiB initial, expandable; Basic HDD for cost efficiency
  (repos are read-heavy after initial checkout)

GKE manifest:
```yaml
apiVersion: v1
kind: PersistentVolume
metadata:
  name: g5-repos-pv
spec:
  capacity:
    storage: 1Ti
  accessModes:
    - ReadWriteMany
  nfs:
    server: <FILESTORE_IP>   # Replace with Filestore instance IP from provisioning output
    path: /g5_repos          # Replace with Filestore file share name
  storageClassName: ""
---
apiVersion: v1
kind: PersistentVolumeClaim
metadata:
  name: g5-repos-pvc
spec:
  accessModes:
    - ReadWriteMany
  resources:
    requests:
      storage: 1Ti
  storageClassName: ""
  volumeName: g5-repos-pv
```
```

#### Change 2: Add DB roles section

**ADD** after Cloud SQL section:

```
### Database Roles

Three application roles provisioned via Terraform/migration:

| Role | Login | Permissions | Used By |
|------|-------|-------------|---------|
| `g5_app` | Yes | DML on `public` schema, EXECUTE on `authz.set_ctx/clear_ctx/current_org_ids` | g5-api service layer (app_pool) |
| `authz_writer` | Yes | INSERT/UPDATE/DELETE on `authz.user_sessions` | g5-authz module (authz_pool) |
| `authz_ctx_writer` | No | DML on `authz.session_ctx`, SELECT/UPDATE on `authz.user_sessions` | Owns SECURITY DEFINER functions |

Secrets stored in GCP Secret Manager:
- `g5-app-db-password` → g5_app role password
- `authz-writer-db-password` → authz_writer role password
- `jwt-signing-key` → ES256 private key PEM

> **Migration runner role**: Migrations run as a dedicated `g5_migrator`
> role with elevated privileges (schema DDL, GRANT, ALTER). This role is
> provisioned by Terraform with credentials rotated via Secret Manager.
> It is separate from `g5_app` and `authz_writer` to avoid granting DDL
> privileges to runtime roles. Not required for Phase 0–1; formalize in
> Phase 2 when authz schema DDL is introduced.
```

#### Change 3: Add Secret Manager entries

**ADD**:

```
### Secret Manager

Mandatory secrets (startup refuses if absent):
- `jwt-signing-key` — ES256 private key PEM for JWT minting
- `jwt-public-key` — ES256 public key PEM for JWT validation
- `service-hmac-key` — HMAC-SHA256 key for capability signing
- `g5-app-db-password` — Postgres g5_app role password
- `authz-writer-db-password` — Postgres authz_writer role password
```

---

### 4.8 MODIFY: `g5/g5d/worker.semlift.md`

**Gaps addressed**: F3 (worker assumes local FS), S7 (blanket service token)

#### Change 1: Replace service token with session token

**BEFORE** (implied):
```
Worker uses a static SERVICE_TOKEN for all API calls.
Token is long-lived and has blanket admin scope.
```

**AFTER**:
```
Worker registration returns a short-lived session token (15-min JWT).
Worker stores token via `secrecy::SecretString` (zeroize-on-drop).
Heartbeat response includes refreshed session token.
All API calls use `Authorization: Bearer <session_token>`.

Token rotation via `tokio::sync::watch` channel:
```rust
// Worker auth state
struct WorkerAuth {
    token_tx: tokio::sync::watch::Sender<SecretString>,
    token_rx: tokio::sync::watch::Receiver<SecretString>,
}

impl WorkerAuth {
    fn new(initial_token: SecretString) -> Self {
        let (tx, rx) = tokio::sync::watch::channel(initial_token);
        Self { token_tx: tx, token_rx: rx }
    }

    /// Called on heartbeat response with refreshed token
    fn rotate(&self, new_token: SecretString) {
        let _ = self.token_tx.send(new_token);
    }

    /// Called by API client to get current token
    fn current_token(&self) -> SecretString {
        self.token_rx.borrow().clone()
    }
}
```
```

#### Change 2: Replace direct filesystem with RepoStorage

**BEFORE** (worker.semlift.md lifecycle):
```
PREPARE:  Direct `git clone` / `git fetch` to local path
CLONE:    Direct `git worktree add`
COLLECT:  Direct `git add` + `git commit`
CLEANUP:  Direct `rm -rf` worktree
```

**AFTER**:
```
RepoStorage remains FS-only (read/write/list/exists). A separate GitService
handles git lifecycle operations using `RepoStorage::local_path()` for bare
repos and `RepoStorage::worktree_root()` for worktree placement:

PREPARE:  git_service.clone_or_fetch(repo_url, storage.local_path(repo_id))
CLONE:    git_service.create_worktree(repo_id, branch, worker_id)
            → worktree created at: storage.worktree_root() / "{worker_id}-{repo_id}-{branch}"
COLLECT:  git_service.commit_and_push(worktree_path, branch)
CLEANUP:  git_service.remove_worktree(worktree_path)

GitService is a thin wrapper around `tokio::process::Command` running git
CLI commands. It uses `local_path()` for bare repo operations and
`worktree_root()` for worktree placement. It does NOT live on the
RepoStorage trait because:
  - RepoStorage is FS-only (read/write/list/exists) — no git semantics
  - GitService needs repo URLs, branch names, credentials — different concerns
  - Object-store backends (future) can't support git ops
  - Worktree placement (local SSD) is separate from repo storage (NFS)

```rust
// g5d/src/git_service.rs
pub struct GitService {
    storage: Arc<dyn RepoStorage>,
    credential_helper: CredentialHelper,
}

impl GitService {
    pub async fn clone_or_fetch(
        &self, repo_url: &str, repo_id: &str,
    ) -> Result<PathBuf, WorkerError> {
        let path = self.storage.local_path(repo_id)
            .ok_or(WorkerError::NoLocalPath)?;
        if path.join(".git").exists() || path.join("HEAD").exists() {
            self.git_fetch(&path).await?;
        } else {
            self.git_clone_bare(repo_url, &path).await?;
        }
        Ok(path)
    }

    /// Create a worktree on local SSD (via `storage.worktree_root()`),
    /// linked to the bare repo (via `storage.local_path(repo_id)`).
    ///
    /// The worktree path is deterministic:
    /// `worktree_root / "{worker_id}-{repo_id}-{branch}"`
    ///
    /// This ensures worktrees are NEVER placed on NFS even when
    /// the bare repos are on a Filestore mount.
    pub async fn create_worktree(
        &self, repo_id: &str, branch: &str, worker_id: &str,
    ) -> Result<PathBuf, WorkerError> {
        let bare_path = self.storage.local_path(repo_id)
            .ok_or(WorkerError::NoLocalPath)?;
        let wt_root = self.storage.worktree_root()
            .ok_or(WorkerError::NoLocalPath)?;
        let wt_name = format!("{worker_id}-{repo_id}-{branch}");
        let wt_path = wt_root.join(&wt_name);
        // git worktree add --detach <wt_path> <branch>
        // (runs against bare_path)
        self.git_worktree_add(&bare_path, &wt_path, branch).await?;
        Ok(wt_path)
    }

    pub async fn commit_and_push(
        &self, worktree_path: &Path, branch: &str,
    ) -> Result<(), WorkerError>;

    pub async fn remove_worktree(
        &self, worktree_path: &Path,
    ) -> Result<(), WorkerError>;
}
```

Worker config adds:
```rust
storage_backend: StorageConfig, // "local" or "filestore"
```

Worker startup constructs RepoStorage via `StorageConfig::build()`,
then constructs `GitService { storage, credential_helper }`.
```

#### Change 3: Add credential storage

**ADD**:
```
SCM credentials and session token stored via `secrecy` crate for
zeroize-on-drop. The `SecretString` wrapper ensures credentials
are zeroed from memory when dropped, preventing credential
leakage via core dumps or memory forensics.
```

---

### 4.9 MODIFY: `g5/g5d/connection.semlift.md`

**Gaps addressed**: S7 (static service token)

#### Change 1: Replace static auth

**BEFORE**:
```
API client uses static SERVICE_TOKEN header for all outbound calls.
```

**AFTER**:
```
API client auth uses dynamic session_token that rotates on heartbeat.

Implementation:
- `tokio::sync::watch` channel distributes token to concurrent tasks
- On heartbeat response: `auth.rotate(new_token)` updates the channel
- On each API call: `auth.current_token()` reads latest token
- On 401 response: trigger immediate heartbeat for token refresh
- Token stored as `secrecy::SecretString` (zeroize-on-drop)
```

---

### 4.10 MODIFY: `g5.semlift.md` (root spec)

**Gaps addressed**: S5 (new crates)

#### Change 1: Add new crate entries

**BEFORE** (g5.semlift.md frontmatter):
```yaml
child_spec_ids:
  - g5/g5-api
  - g5/g5d
  - g5/g5-common
  - g5/g5-analysis
  - g5/client
  - g5/infra
  - g5/e2e
  - g5/integration-stack
```

**AFTER**:
```yaml
child_spec_ids:
  - g5/g5-api
  - g5/g5-authz       # NEW — Authorization service crate
  - g5/g5-blob         # NEW — Blob service crate
  - g5/g5d
  - g5/g5-common
  - g5/g5-analysis
  - g5/client
  - g5/infra
  - g5/e2e
  - g5/integration-stack
```

#### Change 2: Add Cargo workspace members

**ADD** to Cargo.toml workspace members description:
```toml
[workspace]
members = [
    "g5-api",
    "g5-authz",   # NEW
    "g5-blob",    # NEW
    "g5-common",
    "g5d",
    "g5-analysis",
    "e2e",
]
```

#### Change 3: Add subsystem descriptions

**ADD**:
```
### g5-authz — Authorization Service Crate
Encapsulates org membership checks and capability minting (DbCapability,
BlobCapability). Pure policy + signing — no middleware, no axum types.
OrgGuard middleware lives in g5-api/routes and calls `AuthzService::is_org_member()`.
Enforces crate-level dependency boundaries: cannot access tenant PgPool,
cannot access RepoStorage, cannot depend on g5-api types.
In-process first, mechanical extraction to separate service in Phase 5.

### g5-blob — Blob Storage Service Crate
Mediates all blob/file storage access through capability-gated operations.
Holds RepoStorage reference and Ed25519 public verification key (cannot
mint capabilities, only verify). Cannot access PgPool.
In-process first, extraction in Phase 5.
```

#### Change 4: Update dependency graph

**ADD**:
```
Dependency graph:
  g5-api → g5-authz → g5-common
  g5-api → g5-blob  → g5-common, g5-authz (capability types)
  g5-api → g5-common/storage
  g5d    → g5-common (auth, storage, types)
  g5-authz ✗→ g5-api (prohibited)
  g5-blob  ✗→ g5-api (prohibited)
```

#### Change 5: Add security invariant section

**ADD** new section:

```
### Multi-Tenant Isolation (System Invariant)

All tenant data queries flow through RLS-enforced connections via the
DbCapability → ScopedDb → authz.current_org_ids() pipeline. The
session_ctx table is keyed by pg_backend_pid() and bounded by pool
size. This invariant holds at every phase:

- Phase 0: OrgGuard prevents URL-based cross-org access
- Phase 2: RLS prevents DB-level cross-org access
- Phase 4: BlobCapability prevents file-level cross-org access
- Phase 5: Process isolation prevents in-memory key extraction
```

---

### 4.11 MODIFY: CRUD Flows — Create Org / Create Workspace / Add Repo

**Gaps addressed**: The CRUD lifecycle flows defined in `g5.semlift.md §3.12`,
`workspace_service.semlift.md`, and `DISTRIBUTED_FS_MIGRATION.md §6` all
predate the authz/storage redesign. They use `PgPool` directly, have no
`OrgGuard` context, don't mint capabilities, and don't initialize
`RepoStorage`. This section threads the new model through every user-facing
button: **Create Organization**, **Create Project (Workspace)**, and
**Add Repo**.

#### 4.11.1 Create Organization (`POST /api/v1/orgs`)

This is a **non-org-scoped** route (no OrgGuard) because the org doesn't
exist yet. The caller must be an authenticated user.

```rust
// g5-api/src/routes/orgs.rs
async fn create_org(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Json(req): Json<CreateOrgRequest>,
) -> Result<Json<ResponseEnvelope<CreateOrgResponse>>, AppError> {
    let user_id = auth.actor_id()?;

    // No OrgGuard — org doesn't exist yet.
    // No ScopedDb — we need system_pool because this INSERT creates the
    // org that will be the RLS boundary. The org_memberships row is also
    // created here, which is what future OrgGuard checks rely on.
    //
    // Scope enforcement: RequireScopes("orgs:write") via scoped_route!
    // prevents unauthorized users from reaching this handler.
    let response = state.org_service
        .create_org(&state.system_pool, user_id, req).await?;

    // Post-transaction side effects (async, non-blocking):
    // 1. Initialize docs repo on storage:
    //    state.repo_storage.init_repo(&response.docs_repo.id.to_string()).await?
    // 2. If SCM connected, clone from provider
    // 3. Enqueue search indexing for the base workspace
    // 4. Send welcome notification
    tokio::spawn({
        let storage = state.repo_storage.clone();
        let docs_repo_id = response.docs_repo.id.to_string();
        async move {
            if let Err(e) = storage.init_repo(&docs_repo_id).await {
                tracing::error!(%e, "Failed to init docs repo on storage");
            }
        }
    });

    Ok(Json(ResponseEnvelope::ok(response)))
}
```

**OrgService::create_org** changes:
```rust
impl OrgService {
    // BEFORE: pub async fn create_org(&self, user_id: Uuid, req: ...) -> ...
    //         Used self.db (PgPool owned by OrgService)
    //
    // AFTER:  Receives pool explicitly (system_pool from AppState).
    //         OrgService is stateless — no PgPool field.
    pub async fn create_org(
        &self,
        pool: &PgPool,      // system_pool — not tenant_pool (org doesn't exist yet)
        user_id: Uuid,
        req: CreateOrgRequest,
    ) -> Result<CreateOrgResponse, G5Error> {
        let mut tx = pool.begin().await?;

        // 1. INSERT org
        let org = sqlx::query_as!(Org,
            "INSERT INTO orgs (name, slug) VALUES ($1, $2) RETURNING *",
            req.name, slug,
        ).fetch_one(&mut *tx).await?;

        // 2. INSERT org_memberships (owner)
        sqlx::query!(
            "INSERT INTO org_memberships (org_id, user_id, role) VALUES ($1, $2, 'owner')",
            org.id, user_id,
        ).execute(&mut *tx).await?;

        // 3. INSERT docs repo
        let docs_repo = sqlx::query_as!(Repo,
            "INSERT INTO repos (org_id, name, slug, default_branch) VALUES ($1, 'docs', 'docs', 'main') RETURNING *",
            org.id,
        ).fetch_one(&mut *tx).await?;

        // 4. INSERT base workspace
        let base_ws = sqlx::query_as!(Workspace,
            "INSERT INTO workspaces (org_id, name, is_base, status, docs_repo_id, \
             docs_branch_name, docs_tracking_mode, visibility, created_by) \
             VALUES ($1, 'Base', true, 'active', $2, 'main', 'tracking', 'org', $3) RETURNING *",
            org.id, docs_repo.id, user_id,
        ).fetch_one(&mut *tx).await?;

        // 5. INSERT workspace_settings for base workspace
        sqlx::query!(
            "INSERT INTO workspace_settings (workspace_id) VALUES ($1)",
            base_ws.id,
        ).execute(&mut *tx).await?;

        tx.commit().await?;

        Ok(CreateOrgResponse { org, base_workspace: base_ws, docs_repo })
    }
}
```

**Key design decisions:**
- Uses `system_pool`, NOT `tenant_pool`. The org doesn't exist yet, so there
  are no RLS policies to enforce. This is the same pattern as `list_orgs`
  (which queries `org_memberships` filtered by `user_id`, not by `org_id`).
- `OrgService` is **stateless** under the new model — it doesn't hold a
  `PgPool`. Route handlers pass in the appropriate pool.
- `RepoStorage::init_repo()` is called **after** the transaction commits,
  as an async side effect. If storage init fails, the org exists in the DB
  but the docs repo directory is missing. This is acceptable because:
  (a) the repo has no content yet, (b) the first `add_repo` or analysis
  job will trigger clone/init anyway, (c) a retry or manual init resolves it.

#### 4.11.2 Create Workspace / Project (`POST /api/v1/orgs/:orgId/workspaces`)

This is an **org-scoped** route. OrgGuard validates membership before the
handler runs.

```rust
// g5-api/src/routes/workspaces.rs
async fn create_workspace(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(params): Path<OrgIdParam>,
    Json(req): Json<CreateWorkspaceRequest>,
) -> Result<Json<ResponseEnvelope<WorkspaceDetail>>, AppError> {
    // 1. OrgGuard already validated membership (tower layer)
    // 2. RequireScopes("workspaces:write") already validated (scoped_route!)
    let actor_id = auth.actor_id()?;
    let scopes = auth.scopes();

    // 3. Mint capability for DB access
    let db_cap = state.authz
        .mint_db_capability(actor_id, params.org_id, &scopes).await?;

    // 4. Create ScopedDb — RLS restricts to this org
    let mut scoped = ScopedDb::from_capability(&state.tenant_pool, &db_cap).await?;

    // 5. Create workspace (transaction inside ScopedDb)
    let ws = state.workspace_service
        .create_workspace(&mut scoped, params.org_id, actor_id, req.clone()).await?;

    // 6. Commit
    scoped.commit().await?;

    // 7. Post-commit: initialize repos on storage + enqueue indexing
    for branch_req in &req.branches {
        let storage = state.repo_storage.clone();
        let repo_id = branch_req.repo_id.to_string();
        tokio::spawn(async move {
            if let Err(e) = storage.init_repo(&repo_id).await {
                tracing::error!(%e, %repo_id, "Failed to init repo on storage");
            }
        });
    }

    Ok(Json(ResponseEnvelope::ok(ws)))
}
```

**WorkspaceService::create_workspace** changes:
```rust
impl WorkspaceService {
    // BEFORE: &self + self.db (PgPool)
    // AFTER:  stateless, receives ScopedDb
    pub async fn create_workspace(
        &self,
        db: &mut ScopedDb<'_>,
        org_id: Uuid,
        created_by: Uuid,
        req: CreateWorkspaceRequest,
    ) -> Result<WorkspaceDetail, G5Error> {
        // org_id filter is IMPLICIT via RLS — all INSERTs into workspaces,
        // workspace_branches, etc. have org_id populated by the
        // set_org_id_from_session() trigger.
        //
        // Validation still uses explicit org_id for uniqueness checks:
        let existing = sqlx::query_scalar!(
            "SELECT COUNT(*) FROM workspaces WHERE org_id = $1 AND name = $2",
            org_id, req.name,
        ).fetch_one(db.tx()).await?;
        if existing.unwrap_or(0) > 0 {
            return Err(G5Error::Conflict { message: "Workspace name already exists".into() });
        }

        // INSERT workspace (org_id set by trigger from session_ctx)
        let ws = sqlx::query_as!(Workspace,
            "INSERT INTO workspaces (name, status, visibility, created_by, org_id) \
             VALUES ($1, 'idea', 'org', $2, $3) RETURNING *",
            req.name, created_by, org_id,
        ).fetch_one(db.tx()).await?;

        // INSERT branches, dependencies, settings (same pattern)
        // ...

        Ok(WorkspaceDetail::from(ws))
    }
}
```

#### 4.11.3 Add Repo (`POST /api/v1/orgs/:orgId/workspaces/:workspaceId/repos`)

```rust
async fn add_repo(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(params): Path<OrgWorkspaceParams>,
    Json(req): Json<AddRepoRequest>,
) -> Result<Json<ResponseEnvelope<WorkspaceBranch>>, AppError> {
    // 1. OrgGuard validated membership
    // 2. RequireScopes("workspaces:write") validated
    let actor_id = auth.actor_id()?;
    let scopes = auth.scopes();

    let db_cap = state.authz
        .mint_db_capability(actor_id, params.org_id, &scopes).await?;
    let mut scoped = ScopedDb::from_capability(&state.tenant_pool, &db_cap).await?;

    // Validate workspace belongs to this org (RLS already constrains, but
    // explicit check gives a better error message)
    let ws = state.workspace_service
        .get_workspace(&mut scoped, params.workspace_id).await?;

    // INSERT workspace_branch
    let branch = state.workspace_service
        .add_branch(&mut scoped, params.workspace_id, req.clone()).await?;

    scoped.commit().await?;

    // Post-commit: clone repo into storage
    let storage = state.repo_storage.clone();
    let repo_id = branch.repo_id.to_string();
    let git_url = req.git_url.clone();
    tokio::spawn(async move {
        // 1. Init repo directory
        if let Err(e) = storage.init_repo(&repo_id).await {
            tracing::error!(%e, %repo_id, "init_repo failed");
            return;
        }
        // 2. Git clone --mirror (via GitService in production)
        //    This is a worker-initiated operation in the full flow:
        //    the API enqueues a "repo_sync" background task, and a
        //    worker picks it up via the task queue.
    });

    Ok(Json(ResponseEnvelope::ok(branch)))
}
```

**Add Repo flow with ManifestService:**
```
User clicks "Add Repo" in UI
        │
        ▼
POST /api/v1/orgs/:orgId/workspaces/:wsId/repos
  { git_url, name, default_branch, credentials }
        │
        ▼
  1. OrgGuard validates org membership
  2. RequireScopes("workspaces:write")
  3. mint_db_capability → ScopedDb
  4. INSERT workspace_branch row
  5. COMMIT
        │
        ▼
  Post-commit (async):
  6. storage.init_repo(repo_id)
  7. GitService.clone_or_fetch(git_url, repo_id)
  8. ManifestService.refresh_manifest(workspace_id)
  9. Enqueue search reindex for new repo files
 10. Enqueue ontology reconciliation (§4.12)
 11. EventBus → WebSocket → UI refreshes workspace view
```

#### 4.11.4 Verification Checklist

- [ ] `create_org` uses `system_pool` (not `tenant_pool`) — org doesn't exist yet, no RLS context
- [ ] `create_org` is a non-org-scoped route — no OrgGuard layer, only RequireScopes("orgs:write")
- [ ] `OrgService` is stateless — no `PgPool` field, receives pool/ScopedDb per call
- [ ] `create_workspace` uses `ScopedDb` with `tenant_pool` — RLS enforces org isolation
- [ ] `add_repo` post-commit initializes storage + triggers clone (async, non-blocking)
- [ ] `RepoStorage::init_repo()` called after transaction commit, not inside transaction
- [ ] Manifest refresh enqueued after repo addition (ManifestService.refresh_manifest)
- [ ] Search reindex enqueued after repo clone completes (search_reindex_queue insert)
- [ ] Ontology reconciliation enqueued after repo clone completes (§4.12)
- [ ] All three flows emit EventBus notifications for WebSocket → UI refresh

---

### 4.12 MODIFY: Indexer Pipeline — Ontology Reconciliation + Linked Document Indexing

**Gaps addressed**: The existing search indexer (search_indexer.semlift.md) and
IndexerService (DISTRIBUTED_FS_MIGRATION.md §5) handle search documents but
do NOT update ontology nodes or linked documents (`doc_node_links`) when files
change. This creates stale ontology trees and broken document links that users
cannot discover until they manually trigger a re-analysis job.

The fix: extend the indexer pipeline with two new processors that run alongside
the search indexer, consuming the same `search_reindex_queue` events.

#### 4.12.0 Deployment Model — `g5-api-indexer` Nodes

The indexer pipeline is **not** a separate Rust binary. It shares the same
`g5-api` crate code (same `Cargo.toml`, same `main.rs`). However, it runs as
a **dedicated deployment role** enabled by a runtime flag:

```
G5_ROLE=api          # default — serves HTTP routes, NO maintenance tasks
G5_ROLE=indexer      # runs MaintenanceRunner only, NO HTTP listener
G5_ROLE=all          # both (single-node / dev mode)
```

**Docker Compose** (production-like):

```yaml
# docker-compose.yml
services:
  g5-api:
    image: g5-api:latest
    environment:
      G5_ROLE: api
    deploy:
      replicas: 2   # horizontal HTTP scaling

  g5-api-indexer:
    image: g5-api:latest          # SAME image as g5-api
    environment:
      G5_ROLE: indexer
    deploy:
      replicas: 2   # horizontal indexer scaling
    volumes:
      - repo_storage:/data/repos  # needs RepoStorage access for file reads
```

**`main.rs` role dispatch** — see §4.12.0.1 for the full implementation
with security rationale. The key invariant: `G5_ROLE=indexer` never
creates an `axum::Router` or binds a `TcpListener`.

Dev mode (`just dev-up`) uses `G5_ROLE=all` — the existing single-process
behavior. Production splits into `g5-api` (stateless HTTP, scales freely)
and `g5-api-indexer` (stateful maintenance — needs RepoStorage volume mount).

**Why same binary, not a new crate?** The indexer processors call into
`SearchService`, `OntologyService`, `DocService` — all defined in `g5-api`.
Extracting a separate crate would duplicate or split these service dependencies.
A role flag is cheaper and keeps the build matrix simple.

#### 4.12.0.1 Security — No HTTP Listener on Worker/Indexer Nodes

**Constraint: neither `g5d` workers nor `g5-api-indexer` nodes bind an
HTTP listener.** This is a deliberate security hardening decision — every
listening port is an attack surface, and these nodes have elevated DB access
(direct pool, service-level credentials) that must not be reachable via
network requests.

| Node type | Network posture | Rationale |
|-----------|----------------|-----------|
| `g5-api` (`G5_ROLE=api`) | **Inbound** — binds `0.0.0.0:7500`, serves HTTP routes behind Edge/Caddy reverse proxy | Only node exposed to external traffic; routes protected by OrgGuard + RequireScopes + RLS |
| `g5-api-indexer` (`G5_ROLE=indexer`) | **No listener** — outbound-only DB connections, no HTTP bind | Runs `MaintenanceRunner` tasks only; no routes to attack. Docker Compose: no `ports:` mapping, no health check HTTP endpoint. Health monitored via DB heartbeat or container liveness probe on process exit. |
| `g5d` worker | **Outbound-only** — polls `g5-api` via HTTP client, no server socket | Pure pull model: registers with API, polls for tasks, pushes results back. No `axum::Router`, no `TcpListener`. Container has no `ports:` mapping. |

**Enforcement in `main.rs`**:

```rust
let role = std::env::var("G5_ROLE").unwrap_or_else(|_| "all".into());

match role.as_str() {
    "api" => {
        // HTTP server only — no maintenance tasks
        run_http_server(app_state, cancel).await;
    }
    "indexer" => {
        // Maintenance tasks only — NO TcpListener, NO axum Router
        // This node never accepts inbound connections.
        let runner = MaintenanceRunner::new(
            pool.clone(), maintenance_config, cancel.clone(), app_config,
        );
        let handles = runner.spawn_all();
        cancel.cancelled().await;
        futures::future::join_all(handles).await;
    }
    "all" => {
        // Both — dev mode only, NOT for production
        let http = tokio::spawn(run_http_server(app_state, cancel.clone()));
        let runner = MaintenanceRunner::new(
            pool.clone(), maintenance_config, cancel.clone(), app_config,
        );
        let mut handles = runner.spawn_all();
        handles.push(http);
        futures::future::join_all(handles).await;
    }
    other => {
        panic!("Unknown G5_ROLE: {other}. Expected: api | indexer | all");
    }
}
```

**Docker Compose hardening** (production):

```yaml
g5-api-indexer:
  image: g5-api:latest
  environment:
    G5_ROLE: indexer
  # NO ports: mapping — this container accepts zero inbound connections
  # NO healthcheck HTTP endpoint — use process liveness instead
  deploy:
    replicas: 2
  volumes:
    - repo_storage:/data/repos
  networks:
    - g5-internal           # internal-only network, no edge exposure

g5d-worker:
  image: g5d:latest
  # NO ports: mapping — pure outbound poller
  # Talks to g5-api via G5D_API_URL (outbound HTTP only)
  environment:
    G5D_API_URL: http://g5-api:7500
  networks:
    - g5-internal           # can reach g5-api, but not reachable from outside
```

**Why this matters**: `g5-api-indexer` has a `PgPool` with
`system_pool` access (no RLS) for some maintenance queries. If it exposed an
HTTP listener, a network attacker could potentially reach service endpoints
that bypass RLS. By never binding a socket, the attack surface is zero — the
only way to influence indexer behavior is through the `search_reindex_queue`
table, which is write-protected by RLS on the API side.

#### 4.12.0.1 Coordination — Multi-Replica Indexer Safety

Multiple `g5-api-indexer` replicas are safe because of the existing
`FOR UPDATE SKIP LOCKED` pattern in `search_indexer.rs`:

```sql
-- From search_indexer.rs line 190
WITH claimed AS (
    SELECT id
    FROM search_reindex_queue
    WHERE status = 'pending'
      AND attempts < max_attempts
    ORDER BY created_at ASC
    LIMIT $1
    FOR UPDATE SKIP LOCKED   -- ← each replica claims disjoint rows
)
UPDATE search_reindex_queue
SET status = 'processing', started_at = now(), attempts = attempts + 1
WHERE id IN (SELECT id FROM claimed)
RETURNING ...
```

**How it works with N replicas**:

1. All replicas poll `search_reindex_queue` on the same 10-second interval.
2. `FOR UPDATE SKIP LOCKED` ensures each replica claims a **disjoint** batch — no double-processing.
3. If a replica crashes mid-batch, the `status = 'processing'` rows timeout
   (a separate reaper query marks them `pending` again after `stale_after` interval).
4. The ontology reconciler and linked doc indexer run **inside the same transaction**
   as the search indexer, so the single `FOR UPDATE SKIP LOCKED` claim governs all three.

**No advisory lock needed.** Unlike a leader-election model (`pg_advisory_lock`),
the queue-claim model allows **true parallel** processing: 2 indexer replicas
process 2x the events. Advisory locks would serialize to a single active
replica — wasteful when the queue depth is high (e.g., after a large repo import).

**Coordination summary**:

| Concern | Mechanism |
|---------|----------|
| Duplicate processing prevention | `FOR UPDATE SKIP LOCKED` on `search_reindex_queue` |
| Crash recovery | Reaper marks `processing` rows → `pending` after timeout |
| Parallel scaling | N replicas = N concurrent batches (disjoint claim sets) |
| Ordering guarantee | `ORDER BY created_at ASC` — FIFO within each batch, but batches interleave across replicas |
| Processor atomicity | All three processors (search + ontology + linked docs) commit in one transaction |

#### 4.12.1 Architecture

```
Draft commit / Job completion / Branch sync / Add repo
        │
        ▼
  search_reindex_queue (existing Postgres job queue)
        │
        ├─── Search Indexer (existing — tsvector + pgvector UPSERT)
        │
        ├─── Ontology Reconciler (NEW)
        │      │
        │      ├── For each changed file path:
        │      │   1. Look up ontology_nodes WHERE file_path = changed_path
        │      │      AND workspace_branch_id = event.branch_id
        │      │   2. If node exists AND content changed (content_hash differs):
        │      │      → UPDATE ontology_nodes SET content_hash = new_hash,
        │      │        content_updated_at = now(), sync_status = 'modified'
        │      │   3. If file is NEW (no matching node):
        │      │      → INSERT ontology_nodes with node_type = 'code',
        │      │        file_path, parent_id = nearest ancestor module node,
        │      │        sync_status = 'added'
        │      │   4. If file is DELETED (node exists, file does not):
        │      │      → UPDATE ontology_nodes SET sync_status = 'deleted'
        │      │        (or DELETE if cascading is safe)
        │      │   5. If file is RENAMED (old path gone, new path appeared):
        │      │      → UPDATE ontology_nodes SET file_path = new_path,
        │      │        sync_status = 'modified'
        │      │
        │      └── Emit EventBus: ontology.changed { workspace_id, branch_id }
        │          → WebSocket → UI ontology viewer auto-refreshes
        │
        └─── Linked Document Indexer (NEW)
               │
               ├── For each changed markdown/doc file:
               │   1. Read file content from git tree at commit_sha
               │   2. Parse YAML frontmatter for `linked_nodes:` field
               │   3. Resolve node references (by component_id or UUID)
               │   4. UPSERT into doc_node_links:
               │      DELETE FROM doc_node_links WHERE doc_id = $1;
               │      INSERT INTO doc_node_links (doc_id, node_id, org_id)
               │        SELECT $1, unnest($2::uuid[]), $3
               │   5. Update docs.content_updated_at = now()
               │
               └── Emit EventBus: docs.links_changed { workspace_id, doc_id }
                   → WebSocket → UI linked document badges refresh
```

#### 4.12.2 Ontology Reconciler Implementation

```rust
// g5-api/src/maintenance/ontology_reconciler.rs

pub async fn reconcile_ontology_for_reindex_event(
    tx: &mut sqlx::Transaction<'_, Postgres>,
    event: &ReindexEvent,
    changed_files: &ChangedFileSet,  // from git diff old_sha..new_sha
) -> Result<ReconcileResult, G5Error> {
    let mut created = 0u32;
    let mut updated = 0u32;
    let mut deleted = 0u32;

    for file in &changed_files.added {
        // New file → create ontology node if it's a recognized source file
        if !is_source_file(file) { continue; }
        let parent_id = find_nearest_module_ancestor(tx, event.branch_id, file).await?;
        let content_hash = compute_content_hash(file, event.commit_sha)?;
        sqlx::query!(
            "INSERT INTO ontology_nodes \
             (workspace_branch_id, parent_id, name, node_type, file_path, \
              content_hash, sync_status, content_updated_at, org_id) \
             VALUES ($1, $2, $3, 'code', $4, $5, 'added', now(), $6) \
             ON CONFLICT (workspace_branch_id, parent_id, name) DO UPDATE \
             SET file_path = EXCLUDED.file_path, content_hash = EXCLUDED.content_hash, \
                 content_updated_at = now(), sync_status = 'modified'",
            event.branch_id, parent_id, file_stem(file), file,
            content_hash, event.org_id,
        ).execute(&mut **tx).await?;
        created += 1;
    }

    for file in &changed_files.modified {
        let content_hash = compute_content_hash(file, event.commit_sha)?;
        let rows = sqlx::query!(
            "UPDATE ontology_nodes SET content_hash = $1, content_updated_at = now(), \
             sync_status = 'modified' \
             WHERE workspace_branch_id = $2 AND file_path = $3 AND content_hash != $1",
            content_hash, event.branch_id, file,
        ).execute(&mut **tx).await?.rows_affected();
        updated += rows as u32;
    }

    for file in &changed_files.deleted {
        let rows = sqlx::query!(
            "UPDATE ontology_nodes SET sync_status = 'deleted' \
             WHERE workspace_branch_id = $1 AND file_path = $2",
            event.branch_id, file,
        ).execute(&mut **tx).await?.rows_affected();
        deleted += rows as u32;
    }

    // Handle renames: file appears in both deleted + added with same content_hash
    // (handled implicitly by the add/delete logic above — the old node gets
    //  'deleted' status and a new node is created. A follow-up dedup pass
    //  could merge them, but for correctness this is sufficient.)

    Ok(ReconcileResult { created, updated, deleted })
}

/// Determine the nearest module ancestor for a new file.
/// Walks the path components upward looking for existing ontology nodes
/// of type 'node' (module aggregates). If none found, returns None
/// (root-level node).
async fn find_nearest_module_ancestor(
    tx: &mut sqlx::Transaction<'_, Postgres>,
    branch_id: Uuid,
    file_path: &str,
) -> Result<Option<Uuid>, G5Error> {
    let parent_dir = Path::new(file_path).parent();
    let mut current = parent_dir;
    while let Some(dir) = current {
        let dir_str = dir.to_string_lossy();
        if dir_str.is_empty() { break; }
        let node = sqlx::query_scalar!(
            "SELECT id FROM ontology_nodes \
             WHERE workspace_branch_id = $1 AND file_path = $2 \
               AND node_type = 'node'",
            branch_id, dir_str.as_ref(),
        ).fetch_optional(&mut **tx).await?;
        if let Some(id) = node {
            return Ok(Some(id));
        }
        current = dir.parent();
    }
    Ok(None) // root-level
}
```

#### 4.12.2.1 `linked_nodes` Frontmatter Format

The `linked_nodes` field in YAML frontmatter uses **`component_id`** strings
(human-readable ontology path identifiers), **not** UUIDs. This matches the
design in DESIGN.md §2.4 and the existing `PUT /docs/:docId/links` API.

**Source of truth**: the YAML frontmatter in the markdown file on disk. The
`doc_node_links` table is a **queryable mirror** — same pattern as how
`ontology_nodes` mirrors spec metadata.

**Example doc file** (`.g5/docs/auth-adr.md`):

```markdown
---
linked_nodes:
  - "auth/validator"       # component_id of an ontology node
  - "auth/claims"          # another component_id
  - "api/middleware/cors"  # cross-module link
doc_type: adr
---
# Auth Token ADR

We chose RS256 over HS256 because ...
```

**Resolution pipeline** (`resolve_node_references()`):

```rust
/// Resolve component_id strings to node UUIDs within a branch.
async fn resolve_node_references(
    tx: &mut sqlx::Transaction<'_, Postgres>,
    branch_id: Uuid,
    linked_nodes_value: &serde_yaml::Value,
) -> Result<Vec<Uuid>, G5Error> {
    let component_ids: Vec<&str> = match linked_nodes_value {
        serde_yaml::Value::Sequence(seq) => seq.iter()
            .filter_map(|v| v.as_str())
            .collect(),
        _ => return Ok(vec![]),
    };

    let mut resolved = Vec::with_capacity(component_ids.len());
    for cid in component_ids {
        let node_id = sqlx::query_scalar!(
            "SELECT id FROM ontology_nodes \
             WHERE workspace_branch_id = $1 AND component_id = $2",
            branch_id, cid,
        ).fetch_optional(&mut **tx).await?;

        match node_id {
            Some(id) => resolved.push(id),
            None => {
                tracing::warn!(
                    component_id = cid,
                    branch_id = %branch_id,
                    "linked_nodes: component_id not found — skipping",
                );
            }
        }
    }
    Ok(resolved)
}
```

**Bidirectional queries** enabled by `doc_node_links`:

| Query | SQL |
|-------|-----|
| "Which nodes does this doc link to?" | `SELECT node_id FROM doc_node_links WHERE doc_id = $1` |
| "Which docs reference this node?" | `SELECT doc_id FROM doc_node_links WHERE node_id = $1` |
| "Is this doc stale?" | Compare `docs.content_updated_at` vs linked nodes' `content_updated_at` |

**Frontend rendering and editing** — the `linked_nodes` field is managed
through the existing `FrontmatterCard` component in `SpecEditor.tsx`, not
through a separate link-picker UI.

**Editing flow**:

1. User opens a View doc in the editor (SpecEditor / DraftEditor)
2. In **source mode**, the YAML frontmatter is directly editable — user adds/removes
   `linked_nodes:` entries as `component_id` strings (e.g., `"auth/validator"`)
3. In **rich mode**, the `FrontmatterCard` renders `linked_nodes` as clickable pills
   that navigate to the linked ontology node
4. On draft commit, the indexer pipeline (§4.12.3) parses the frontmatter and
   syncs `doc_node_links` — the user never interacts with the `PUT /docs/:docId/links`
   endpoint directly

**Required `FrontmatterCard` changes** (`SpecEditor.tsx`):

```tsx
// Add 'linked_nodes' to NAVIGABLE_FIELDS so entries render as clickable links
const NAVIGABLE_FIELDS = new Set([
  'component_id',
  'parent_spec',
  'parent_spec_id',
  'child_specs',
  'child_spec_ids',
  'src_files',
  'source_files',
  'linked_nodes',     // ← NEW: renders as navigable pills
]);

// In renderValue(), linked_nodes entries navigate with type: 'spec'
// because they are component_id strings that resolve to ontology nodes.
// Clicking "auth/validator" navigates to the ontology node with that
// component_id — same as clicking a parent_spec or child_spec link.
```

**`PUT /docs/:docId/links` remains as internal/programmatic API** — used by:

- The indexer pipeline (§4.12.3) after parsing frontmatter on commit
- Bootstrap (Layer 5 in bidirectional-doc-links.md) during initial project scan
- Future automation (e.g., agent-generated docs that need links set programmatically)

Users do **not** call this endpoint through a UI — the YAML frontmatter
is the sole user-facing interface for managing doc-node links.

#### 4.12.3 Linked Document Indexer Implementation

```rust
// g5-api/src/maintenance/linked_doc_indexer.rs

pub async fn index_linked_documents(
    tx: &mut sqlx::Transaction<'_, Postgres>,
    event: &ReindexEvent,
    changed_files: &ChangedFileSet,
    repo_storage: &dyn RepoStorage,
) -> Result<LinkIndexResult, G5Error> {
    let mut indexed = 0u32;

    // Only process markdown/doc files
    let doc_files: Vec<&str> = changed_files.all_changed()
        .filter(|f| is_doc_file(f))
        .collect();

    for file_path in doc_files {
        // 1. Find the doc row for this file
        let doc = sqlx::query!(
            "SELECT id FROM docs WHERE workspace_branch_id = $1 AND file_path = $2",
            event.branch_id, file_path,
        ).fetch_optional(&mut **tx).await?;

        let doc_id = match doc {
            Some(d) => d.id,
            None => continue,  // Not a tracked doc — skip
        };

        // 2. Read file content and parse YAML frontmatter
        let content = repo_storage
            .read_file(&event.repo_id, file_path).await
            .unwrap_or_default();
        let frontmatter = parse_yaml_frontmatter(&content);

        // 3. Extract linked_nodes from frontmatter
        let linked_node_ids: Vec<Uuid> = match frontmatter.get("linked_nodes") {
            Some(nodes) => resolve_node_references(tx, event.branch_id, nodes).await?,
            None => vec![],
        };

        // 4. Replace doc_node_links atomically
        sqlx::query!(
            "DELETE FROM doc_node_links WHERE doc_id = $1",
            doc_id,
        ).execute(&mut **tx).await?;

        if !linked_node_ids.is_empty() {
            sqlx::query!(
                "INSERT INTO doc_node_links (doc_id, node_id, org_id) \
                 SELECT $1, unnest($2::uuid[]), $3",
                doc_id, &linked_node_ids, event.org_id,
            ).execute(&mut **tx).await?;
        }

        // 5. Update doc content timestamp
        sqlx::query!(
            "UPDATE docs SET content_updated_at = now() WHERE id = $1",
            doc_id,
        ).execute(&mut **tx).await?;

        indexed += 1;
    }

    Ok(LinkIndexResult { indexed })
}

fn is_doc_file(path: &str) -> bool {
    let ext = Path::new(path).extension().and_then(|e| e.to_str());
    matches!(ext, Some("md" | "mdx" | "markdown"))
}
```

#### 4.12.4 Integration with Search Indexer Loop

The existing `process_reindex_batch` in `search_indexer.rs` is extended to
call both new processors after the search indexing step:

```rust
// In search_indexer.rs — process_reindex_batch
async fn process_single_event(
    pool: &PgPool,
    search_service: &SearchService,
    repo_storage: &dyn RepoStorage,
    event: &ReindexEvent,
) -> Result<(), G5Error> {
    let mut tx = pool.begin().await?;

    // Compute changed files (git diff old_sha..new_sha)
    let changed = compute_changed_files(repo_storage, &event).await?;

    // 1. Search indexing (existing)
    search_service.reindex_changed_files(&mut tx, event, &changed).await?;

    // 2. Ontology reconciliation (NEW)
    let reconcile = reconcile_ontology_for_reindex_event(&mut tx, event, &changed).await?;
    if reconcile.created + reconcile.updated + reconcile.deleted > 0 {
        tracing::info!(
            created = reconcile.created,
            updated = reconcile.updated,
            deleted = reconcile.deleted,
            "Ontology reconciled for branch {}",
            event.branch_id,
        );
    }

    // 3. Linked document indexing (NEW)
    let links = index_linked_documents(&mut tx, event, &changed, repo_storage).await?;
    if links.indexed > 0 {
        tracing::info!(indexed = links.indexed, "Linked documents re-indexed");
    }

    tx.commit().await?;

    // 4. Emit events for UI refresh
    event_bus.publish(Event::OntologyChanged {
        workspace_id: event.workspace_id,
        branch_id: event.branch_id,
    });
    event_bus.publish(Event::SearchIndexed {
        workspace_id: event.workspace_id,
    });

    Ok(())
}
```

#### 4.12.5 Trigger Points

All existing search_reindex_queue producers already cover ontology + linked doc
indexing because the new processors run on the same queue. Summary of trigger
points:

| Trigger | Queue Producer | Ontology Effect | Linked Doc Effect |
|---------|---------------|-----------------|-------------------|
| **Draft commit** (`POST /drafts/:id/commit`) | DraftService enqueues reindex event | Updates content_hash + content_updated_at on modified nodes | Re-parses frontmatter, refreshes doc_node_links |
| **Job completion** (task → `completed`) | TaskService enqueues reindex event | Creates new nodes for generated files, marks deleted files | Links new spec docs to generated code nodes |
| **Branch sync** (IndexerService git fetch) | IndexerService enqueues changed paths | Syncs ontology with upstream changes (new/deleted/renamed files) | Updates links if upstream changed doc frontmatter |
| **Add repo** (§4.11.3) | Post-commit handler enqueues full-tree reindex | Creates initial ontology nodes for all source files | Indexes all doc links in the new repo |
| **Manual trigger** (`POST /workspaces/:id/reindex`) | Admin endpoint enqueues full-tree reindex | Full reconciliation pass | Full doc link refresh |

#### 4.12.6 Verification Checklist

- [ ] Ontology reconciler runs inside same transaction as search indexer (atomic consistency)
- [ ] New source files auto-create ontology nodes with `sync_status = 'added'` and correct `parent_id`
- [ ] Modified files update `content_hash` and `content_updated_at` only when hash actually changed
- [ ] Deleted files set `sync_status = 'deleted'` (soft delete, not hard delete — preserves dependency edges)
- [ ] `find_nearest_module_ancestor()` walks path components upward to find parent module node
- [ ] Linked document indexer parses YAML frontmatter `linked_nodes:` field (component_id strings, not UUIDs)
- [ ] `resolve_node_references()` looks up `ontology_nodes.component_id` within the branch scope
- [ ] Unresolved component_ids log a warning and are skipped (no hard error)
- [ ] `FrontmatterCard` includes `linked_nodes` in `NAVIGABLE_FIELDS` — renders as clickable pills
- [ ] Clicking a `linked_nodes` entry in `FrontmatterCard` navigates to the ontology node (type: 'spec')
- [ ] `linked_nodes` is editable in source mode via YAML frontmatter (no separate link-picker UI)
- [ ] `PUT /docs/:docId/links` is internal/programmatic only — not exposed through a user-facing UI component
- [ ] `doc_node_links` replacement is atomic: DELETE + INSERT in same transaction
- [ ] `doc_node_links` rows include `org_id` for RLS compatibility
- [ ] `G5_ROLE=indexer` starts `MaintenanceRunner` without HTTP listener — no `axum::Router`, no `TcpListener`
- [ ] `G5_ROLE=api` starts HTTP listener without `MaintenanceRunner`
- [ ] `G5_ROLE=all` (default) starts both (dev mode / single-node — NOT for production)
- [ ] `g5-api-indexer` Docker service has NO `ports:` mapping
- [ ] `g5d-worker` Docker service has NO `ports:` mapping (outbound-only poller)
- [ ] Both indexer and worker containers use `g5-internal` network (not edge-exposed)
- [ ] Multiple indexer replicas are safe: `FOR UPDATE SKIP LOCKED` prevents double-processing
- [ ] Crash recovery: reaper marks stale `processing` rows back to `pending`
- [ ] CI test: assert `G5_ROLE=indexer` process exits cleanly without binding any port
- [ ] EventBus emits `ontology.changed` after reconciliation → WebSocket → UI ontology viewer refreshes
- [ ] EventBus emits `docs.links_changed` after link indexing → WebSocket → UI doc badges refresh
- [ ] All trigger points (draft commit, job completion, branch sync, add repo) produce reindex events
- [ ] Ontology reconciler is idempotent — re-running on same commit_sha produces no changes (content_hash guard)
- [ ] Linked document indexer is idempotent — re-running replaces links with same result

---

## 5. Cascading Child-Spec Adjustments

These specs require minor adjustments due to upstream changes:

| Spec | Change | Reason |
|------|--------|--------|
| `g5/g5-api.semlift.md` | Add `g5/g5-authz`, `g5/g5-blob` to dependencies | New crate deps |
| `g5/g5-common.semlift.md` | Add `g5/g5-common/storage` to `child_spec_ids` | New storage module |
| `g5/g5d.semlift.md` | Add `g5/g5-common/storage` to dependencies | Worker uses RepoStorage |
| `g5/g5d/config.semlift.md` | Add `storage_backend: StorageConfig` field | Worker config |
| All child service specs | Update method signatures to accept `&mut ScopedDb<'_>` | ScopedDb pattern |
| `g5/g5-api/services/org_service.semlift.md` | Remove `PgPool` from `OrgService`; `create_org` receives `pool` param; use `system_pool` | §4.11.1 — stateless service pattern |
| `g5/g5-api/services/workspace_service.semlift.md` | `create_workspace` receives `&mut ScopedDb`; post-commit calls `storage.init_repo()` | §4.11.2 — ScopedDb + storage init |
| `g5/g5-api/routes/workspaces.semlift.md` | `add_repo` handler mints capability → ScopedDb; post-commit enqueues repo sync + indexing | §4.11.3 — full add-repo lifecycle |
| `g5/g5-api/maintenance/search_indexer.semlift.md` | Extend `process_reindex_batch` to call ontology reconciler + linked doc indexer | §4.12.4 — shared queue, three processors |
| `g5/g5-api/services/ontology_service.semlift.md` | Add `reconcile_for_changed_files()` method (called by indexer, not route handlers) | §4.12.2 — incremental ontology sync |
| `docker/docker-compose.dev.yml` | Add `g5-api-indexer` service (`G5_ROLE=indexer`, same image as `g5-api`) | §4.12.0 — dedicated indexer replicas |
| `g5/g5-api/src/main.rs` | Add `G5_ROLE` dispatch: api-only, indexer-only, or both | §4.12.0 — deployment model |
| `g5/e2e.semlift.md` | Update test expectations for worker session tokens, multi-pool | Test adjustments |

---

## 6. Phasing Alignment

| Phase | Spec changes | Blocking? |
|-------|-------------|-----------|
| **Phase 0 — Immediate** | §4.1 (iss/aud), §4.2 (guardrails), §4.6 (startup) | No — independent |
| **Phase 1 — Crate bounds** | §3.1 (g5-authz), §3.2 (g5-blob), §4.3 (OrgGuard), §4.4 (AppState), §4.10 (root) | Yes — needs §4.1 |
| **Phase 2 — DB/RLS** | §4.5 (authz schema + ScopedDb), §4.6 (dual pools) | Yes — needs Phase 1 |
| **Phase 3 — Denorm** | §4.5 (org_id migration + RLS), §4.4 (remove raw PgPool) | Yes — needs Phase 2 |
| **Phase 4 — Blob cap** | §3.2 (BlobService), §4.4 (BlobService in AppState) | Yes — needs Phase 1 |
| **Phase 5 — Extract** | No spec changes — mechanical extraction | Future |
| **FS Migration** | §3.3 (RepoStorage), §4.7 (Filestore), §4.8 + §4.9 (worker) | Independent track |
| **CRUD Lifecycle** | §4.11 (create org/workspace/repo flows) | Needs Phase 1 (ScopedDb) + FS Migration (RepoStorage) |
| **Indexer Pipeline** | §4.12 (ontology reconciler + linked doc indexer + `G5_ROLE` flag) | Needs Phase 2 (org_id on all tables) + FS Migration (RepoStorage for file reads) |

---

## 7. Dependency Order

```
                    ┌────────────────────────┐
                    │  Phase 0: Immediate     │
                    │  §4.1 JwtClaims         │
                    │  §4.2 JWT minting       │
                    │  §4.6 startup guards    │
                    └───────────┬────────────┘
                                │
                    ┌───────────▼────────────┐
                    │  Phase 1: Crate bounds  │
                    │  §3.1 g5-authz (NEW)    │
                    │  §3.2 g5-blob (NEW)     │
                    │  §4.3 routes + OrgGuard │
                    │  §4.4 services/AppState │
                    │  §4.10 root spec        │
                    └───────────┬────────────┘
                                │
              ┌─────────────────┼─────────────────┐
              │                 │                  │
  ┌───────────▼──────┐  ┌──────▼───────┐  ┌──────▼───────────┐
  │ Phase 2: DB/RLS  │  │ Phase 4:     │  │ FS Migration     │
  │ §4.5 authz schema│  │ Blob cap     │  │ §3.3 RepoStorage │
  │ §4.5 ScopedDb    │  │ (§3.2 done)  │  │ §4.7 infra       │
  │ §4.6 dual pools  │  └──────────────┘  │ §4.8 worker      │
  └───────────┬──────┘                    │ §4.9 connection   │
              │                           └──────┬───────────┘
  ┌───────────▼──────┐                           │
  │ Phase 3: Denorm  │                           │
  │ §4.5 org_id cols │                           │
  │ §4.5 RLS enable  │                           │
  │ §4.4 drop raw    │                           │
  │     PgPool       │                           │
  └───────────┬──────┘                           │
              │                                  │
              └─────────────┬────────────────────┘
                            │
              ┌─────────────▼────────────────┐
              │  CRUD Lifecycle (§4.11)       │
              │  create_org (system_pool)     │
              │  create_workspace (ScopedDb)  │
              │  add_repo (ScopedDb +         │
              │    RepoStorage + ManifestSvc) │
              └─────────────┬────────────────┘
                            │
              ┌─────────────▼────────────────┐
              │  Indexer Pipeline (§4.12)     │
              │  ontology_reconciler          │
              │  linked_doc_indexer           │
              │  (requires org_id on          │
              │   ontology_nodes +            │
              │   doc_node_links → Phase 3)   │
              └──────────────────────────────┘
```

---

## 8. Risk Assessment

| Risk | Impact | Mitigation |
|------|--------|------------|
| **ScopedDb signature churn** — every service method changes | High (100+ methods) | Introduce `DbCapability` type alias initially; mechanical refactor via ast-grep |
| **RLS performance regression** — `current_org_ids()` per query | Medium | `STABLE` + PK-indexed + `PARALLEL RESTRICTED`; benchmark before/after; table bounded by pool size; `ANY()` on small arrays (~1-5 elements) is fast |
| **Multi-pool connection exhaustion** | Medium | Budget split (15 app + 10 authz + 5 membership = 30 total); monitor via `pg_stat_activity`; membership_pool is read-only with low concurrency |
| **Worker token rotation race** — expires during long request | Low | 15-min lifetime + 30s heartbeat = 14.5 min margin; retry on 401 |
| **Filestore latency vs local SSD** | Low | Repos read-heavy after checkout; worktree on local SSD; bare cache on NFS |
| **NFS metadata storm on `git status`** | Medium | `lookupcache=pos` + `noatime` mount options; bare repos are read-heavy post-clone; worktrees on local SSD avoid the issue entirely |
| **org_id backfill in production** | High | Online migration: nullable → backfill in batches → NOT NULL → RLS |
| **Breaking change to all service tests** | High | Phase 1 preserves PgPool internally; ScopedDb added alongside; cut-over in Phase 2 |
| **NFS lock reliability** — lockfile vs flock semantics | Medium | `O_CREAT\|O_EXCL` is atomic on NFSv3+; pin NFS version + mount options; conformance test in CI |
| **Phase 1 scope enforcement gap** | Medium | Between Phase 0 (guardrails) and Phase 2 (RLS), `RequireScopes` is being rolled out across 116 routes but RLS is not live. A user with a valid JWT but insufficient scopes for an endpoint could bypass scope checks if a route is missed by `scoped_route!`. Mitigated by: (a) CI grep gate catches direct `.route()` calls, (b) `all_routes_have_required_scopes` CI test verifies 125/125 coverage against `scope_map.yaml`, (c) Phase 1 is short-lived. **Not a runtime control** — no RLS backstop exists until Phase 2. |
| **validate_path false negatives** — lexical validation misses symlinks | Low | Symlink creation inside repo dirs is a pre-existing trust assumption; defense-in-depth via container read-only rootfs. **Trust model**: repo contents are controlled by Git (not arbitrary user uploads), so no untrusted symlink injection vector exists. Workers control repo checkouts; users cannot write arbitrary symlinks into the working tree. |
| **Indexer/worker HTTP exposure** — accidental listener on non-API nodes | Medium | `G5_ROLE=indexer` codepath never constructs `axum::Router` or binds `TcpListener`; `g5d` has no HTTP server code. Docker Compose: no `ports:` mapping, `g5-internal` network only. CI test verifies no port binding. |

---

## 9. Verification Criteria

Before marking this proposal as approved, verify:

- [ ] Every gap in §2 maps to at least one spec change in §3–§4
- [ ] Every Phase in SECURITY_HARDENING.md has corresponding spec changes
- [ ] Every Phase in OAUTH_SECURITY_AUDIT.md has corresponding spec changes
- [ ] The FS migration's key abstractions (RepoStorage, ManifestService) have spec homes — RepoStorage in §3.3, ManifestService in §3.4
- [ ] No circular dependencies in the new crate graph — `g5-authz ✗→ g5-api`, `g5-blob ✗→ g5-api`
- [ ] The phasing in §6 is consistent with SECURITY_HARDENING.md's phase order
- [ ] All 20 tables listed for org_id denormalization match API_AND_STORAGE.md §2
- [ ] `current_org_ids()` is marked STABLE, PARALLEL RESTRICTED, SECURITY DEFINER — returns `UUID[]`
- [ ] `set_ctx()` parameter is UUID (session_id referencing `user_sessions`)
- [ ] OrgGuard lives in `g5-api/routes` (not `g5-authz`), performs membership check (not JWT `org` equality)
- [ ] BlobCapability uses `git_oid` (Git object ID, not filesystem path)
- [ ] `session_ctx` table is keyed by `pg_backend_pid()` (not GUCs); stores `org_ids UUID[]` (not single org_id)
- [ ] SECURITY DEFINER functions have explicit `OWNER TO authz_ctx_writer`
- [ ] `ScopedDb` implements Drop guard for rollback/cleanup semantics
- [ ] DB sessions are reusable within 60-minute TTL (one per user, UPSERT); `set_ctx()` does NOT delete the session
- [ ] `user_sessions` table has UNIQUE on `user_id` — one active session per user
- [ ] RLS policies use `org_id = ANY(authz.current_org_ids())` — supports multi-org sessions
- [ ] Blob capability signing uses Ed25519 (asymmetric), not HMAC (symmetric)
- [ ] `g5-authz` does not depend on `axum`, `AppState`, or `AppError`
- [ ] `AuthzError → AppError` conversion lives in `g5-api`, not `g5-authz`
- [ ] DB role provisioning (`CREATE ROLE`) is in Terraform/IaC, not SQL migrations; migrations contain GRANT statements only
- [ ] `JwtClaims.org` is documented as advisory (selected org context), not authorization
- [ ] CI test `all_routes_have_required_scopes` gate exists and verifies 125/125 routes
- [ ] `mint_db_capability` accepts `(actor_id, target_org_ids, scopes)` — not `&JwtClaims` — so both User and Worker can mint
- [ ] `AuthContext` provides `actor_id()` and `scopes()` methods for identity-agnostic capability minting
- [ ] `BlobService::read_blob` takes `token: &str` (not `&BlobCapability`) — blob verifies locally with Ed25519 public key
- [ ] `AuthzService` has no `verify_blob_capability` — authz mints only, blob verifies only
- [ ] `org_memberships` is explicitly exempt from tenant RLS (or has a bypass policy)
- [ ] Scope enforcement architecture documented: route layer (primary) → capability metadata → service checks (optional)
- [ ] Worker vs user JWT claim shapes are explicitly documented as different structs
- [ ] `jti` is in `required_spec_claims` for request tracing and future replay detection (NOT currently enforced as a replay cache — no server-side jti storage)
- [ ] GitService wraps RepoStorage::local_path() for git lifecycle — RepoStorage is FS-only
- [ ] Filestore directory layout specifies: bare repos shared + flock'd, worktrees per-(worker,repo,branch) unique
- [ ] Lockfile stale detection uses worker API liveness check (not purely timestamp-based); 15-min fallback for network partition
- [ ] Symlink traversal risk accepted: repo contents are Git-controlled (no untrusted symlink injection vector); defense-in-depth via read-only rootfs
- [ ] Route enforcement mechanism specifies practical approach (macro/registry/build.rs), not aspirational `extract_routes`
- [ ] `BlobService` uses deterministic `blob_path(oid)` mapping for content-addressed reads — fanout `.blobs/<oid[0:2]>/<oid[2:4]>/<oid>`
- [ ] No `read_file_internal` on `BlobService` — internal reads use `RepoStorage` directly via worker checkout paths
- [ ] `AuthzError::InsufficientScopes` documented as reserved for optional service-layer defense-in-depth, not raised by `mint_db_capability`
- [ ] `scoped_route!` macro is the mandatory, canonical route enforcement mechanism — routes without macro won't compile
- [ ] `ScopedDb::Drop` does no async work; correctness relies on transaction ROLLBACK semantics undoing `set_ctx` INSERT
- [ ] `current_org_ids()` returns NULL when no session → ANY(NULL) evaluates to NULL → zero rows visible → fail-closed; migration includes regression assertion
- [ ] Membership caching strategy specified: Redis `org_member:{org_id}:{actor_id}`, 60s TTL, invalidation on membership change
- [ ] DB session semantics documented: UPSERT per user, reusable within 60-min TTL, expired sessions pruned every 5 min
- [ ] Blob capability token wire format pinned: JWS Compact Serialization (RFC 7515) with EdDSA algorithm
- [ ] `jti` is for request tracing and future replay detection — no replay cache or revocation store yet; provides **no replay protection** until server-side jti store is added
- [ ] Worker `worker_id` URL match enforced on every internal endpoint with `AuthContext::Worker` pattern match
- [ ] Connection pooler compatibility documented: transaction-mode pooling works because `set_ctx` + queries share same SQL transaction
- [ ] `LocalRepoStorage::validate_path()` uses **lexical** validation (reject absolute + `..`), NOT `canonicalize()` — writes to new files work
- [ ] `after_acquire` hooks calling `clear_ctx()` are REQUIRED from Phase 1 (not Phase 2) — battle-tested before RLS raises stakes; Phase 0 only as defense-in-depth
- [ ] `PARALLEL RESTRICTED` on `current_org_ids()` verified by migration regression test querying `pg_proc.proparallel = 'r'`
- [ ] `user_sessions` UPSERT uses `array_cat` + `DISTINCT` union for `org_ids` — single-org minting does NOT overwrite concurrent multi-org sessions
- [ ] SECURITY DEFINER + NOLOGIN pattern documented: functions execute with owner's privileges; NOLOGIN only prevents direct connections
- [ ] `inventory` crate platform constraints documented: no WASM, ELF/Mach-O only; CI grep gate excludes test/doc false positives; scope verification test MUST run with `--all-features` to catch `#[cfg(feature)]`-guarded routes
- [ ] Blob fanout SHA-1/SHA-256 safety documented as known safe assumption (different OID lengths, different format headers)
- [ ] `workers` table is system-global (not tenant-scoped); FK from `repo_manifests.worker_id` crosses RLS boundary intentionally
- [ ] `AuthContext::selected_org_id()` (not `require_org_id()`) — named to reflect advisory nature; not used for authorization
- [ ] `membership_pool` config: `g5_membership` role, `G5_MEMBERSHIP_DATABASE_URL` env var, max=5/min=1
- [ ] `ScopedDb::primary_org_id()` returns `Result<Uuid, G5Error>` (not panic) — invariant violation produces diagnostic error
- [ ] `LocalRepoStorage::new()` canonicalizes `base_dir` once at init; target paths are never canonicalized
- [ ] Filestore locking uses `O_CREAT|O_EXCL` lockfile creation (atomic on NFSv3+), NOT `flock()`
- [ ] NFS assumptions pinned: NFSv3+, `hard` mount, `noac`/`actimeo=0` for lockfile visibility
- [ ] NFS locking conformance test specified: concurrent lock acquisition + stale lock detection
- [ ] `ScopedDb::from_capability()` MUST call `set_ctx()` only after `pool.begin()` — autocommit FORBIDDEN
- [ ] Session-mode pooling not recommended for production — transaction-mode required; session-mode works but amplifies bugs
- [ ] JWT claim validation unit tests required: rejects missing/wrong `iss`, `aud`, `jti`, `sub`, expired `exp`
- [ ] Worker/user token discrimination is deterministic: `task_id` present → Worker, absent → User
- [ ] Worker token decode strategy pinned: `serde_json::Value` first, then branch — no `serde(untagged)`
- [ ] Blob capability token includes `purpose: "blob_read"` field to prevent token-confusion attacks
- [ ] Membership caching includes **negative caching** (cache both true and false results, same TTL)
- [ ] `session_ctx` table includes `session_id UUID` column for audit trail (which capability established this session)
- [ ] Bare repos + `.blobs/` on NFS; worktrees MUST be on local ephemeral SSD (hard deployment invariant)
- [ ] `local_path()` returns `Some()` for all current backends; `None` reserved for future object-store only
- [ ] `scoped_route!` bypass prevention: CI grep gate for direct `Router::route()` calls required from Phase 1
- [ ] CI grep gate scans `g5-api/src/` (entire crate), not just `g5-api/src/routes/` — routes can appear in any module
- [ ] Blob token replay cache formalized as phase-optional (`blob-single-use` feature flag, Redis `SETNX` on `jti`)
- [ ] Blob capability field is `git_oid` (Git object ID from `git hash-object`), not `blob_sha` — handles SHA-1 (40-char) or SHA-256 (64-char)
- [ ] Blob capability token includes `token_use: "capability"` field — middleware MUST reject tokens where `token_use` != expected type
- [ ] Canonical JSON ordering is an internal encoding detail of g5-authz — NOT part of the wire format contract
- [ ] `ScopedDb` exposes `scopes: Vec<String>` field and `scopes()` accessor for service-layer defense-in-depth
- [ ] `ScopedDb::commit()` treats `clear_ctx()` failure as a fatal error — leaked session_ctx is worse than a failed request
- [ ] Pooling narrative consistent: transaction-mode REQUIRED for prod; session-mode works but amplifies bugs if transaction invariant violated; startup assertion enforces `G5_DB_POOL_MODE=transaction`
- [ ] `PARALLEL RESTRICTED` rationale documented: parallel workers have different `pg_backend_pid()` values → see empty session_ctx → NULL → fail-closed
- [ ] Migrations MUST run only after roles are provisioned by IaC — GRANT statements fail intentionally if roles don't exist
- [ ] `worktree_root()` on RepoStorage trait returns local SSD path — separate from `local_path()` (NFS for Filestore)
- [ ] `GitService::create_worktree()` uses `storage.worktree_root()` for worktree placement — worktrees are NEVER placed on NFS
- [ ] `StorageConfig` includes `worktree_dir` for both Local and Filestore providers
- [ ] `G5_STORAGE_WORKTREE_DIR` env var documented with default `/tmp/g5-worktrees`
- [ ] `write_file()` caller enumeration documented: GitService::create_worktree (internal), blob extraction (internal), draft-commit API (user input — validate_path required)
- [ ] `session_ctx` cleanup task uses `authz_ctx_writer` privileges (not `g5_app`); `user_sessions` pruning uses `authz_pool` (`authz_writer`)
- [ ] `scope_map.yaml` full 125-route mapping MUST be populated from OAUTH_SECURITY_AUDIT.md Appendix A before Phase 1
- [ ] Refresh endpoint leeway (`validation.leeway = 30`) configured on a **separate `JwtValidator` instance** — NOT on the shared middleware validator
- [ ] `token_use` negative test: user/worker JWT validators MUST reject tokens containing `token_use` claim (explicit test case specified)
- [ ] Periodic `user_sessions` pruning task (every 5 min) documented in main startup — handles expired sessions
- [ ] NFS Git performance mount options documented: `lookupcache=pos`, `noatime`, `rsize=1048576,wsize=1048576`, `nconnect=4`
- [ ] JWT `iss`/`aud` values injected via env vars (`G5_JWT_ISSUER`, `G5_JWT_AUDIENCE`), NOT hardcoded — startup guardrail refuses to start if unset
- [ ] ScopedDb integration with service layer documented: route handler flow (mint → from_capability → queries → commit) with code example
- [ ] Service methods accept `&mut ScopedDb`, NOT `PgPool` — migration path from Phase 1 (alongside) to Phase 2 (replace) documented

---

## 10. Open Questions

1. ~~**g5-authz placement**: Start as a crate from Phase 1 (recommended) or module within `g5-api`?~~
   → **Resolved**: Crate from day one. OrgGuard middleware moved to g5-api/routes.

2. ~~**BlobService backend**: Wrap existing GCS client or new abstraction?~~
   → **Partially resolved**: In-process blob storage uses content-addressed fanout layout (`.blobs/<sha[0:2]>/<sha[2:4]>/<sha>`) via `RepoStorage`. GCS backend for Phase 5 multi-host is still open — recommendation remains to wrap existing GCS client.

3. ~~**ManifestService ownership**: `g5-common/storage` (DTO+client) or `g5-api/services` (impl)?~~
   → **Resolved**: DTOs in `g5-common/src/manifest.rs`, service impl in `g5-api/src/services/manifest_service.rs`. Spec in §3.4.

4. **RLS on `search_documents`**: Performance-critical (200ms SLA). Adding RLS adds overhead.
   → **Recommendation**: Benchmark first; if too slow, explicit `WHERE org_id = $1` with code comment.

5. ~~**Phase 0 scope**: Apply startup guardrails as standalone PR?~~
   → **Resolved**: Yes — low-risk, high-value, independent.

---

## 11. Alternatives Considered: Object-Store Snapshot Backend

### Context

The current design uses Google Cloud Filestore (NFS) for shared bare repos and
`.blobs/`, with local SSD for worktrees. An alternative architecture would store
**immutable repo snapshots in GCS** (object storage) and reconstruct git objects
via range reads + a local cache layer. This section documents why the object-store
path is deferred and what production signals would trigger revisiting it.

### Why NFS Is the Right Default Now

1. **Simplicity**: `RepoStorage` is a thin POSIX wrapper. Git just works because
   it's literally a filesystem. Debugging is `ls`, `stat`, `git fsck`. Recovery
   is remount + restart pods.

2. **Correctness risk budget**: This proposal already introduces RLS, capability
   minting, route scope enforcement, dual-pool architecture, and ScopedDb. Adding
   a git object reconstruction service on top would compound correctness risk
   during the highest-risk phase of the migration.

3. **Git's native expectations**: Git assumes POSIX semantics (mmap, flock, atomic
   rename). NFS provides these (with documented mount options). An object-store
   backend requires reimplementing git's object resolution layer.

4. **Worktrees are already on local SSD**: The biggest performance win is already
   captured. NFS only serves shared bare repos (read-heavy after initial clone)
   and `.blobs/` (content-addressed, immutable). These are NFS's happy path.

### What an Object-Store Backend Would Require

Building this is feasible but non-trivial — it's a "mini git object store service":

| Component | Complexity | Purpose |
|-----------|-----------|---------|
| Pack index cache | Medium | `.idx` / `.midx` / `commit-graph` in memory or local SSD |
| Range-read layer | Medium | Fetch byte ranges from GCS pack blobs |
| Object rehydration engine | High | Inflate zlib + resolve delta chains (OFS_DELTA, REF_DELTA) |
| Pack-chunk cache | Medium | LRU cache for hot pack windows to avoid repeated range reads |
| Snapshot builder | Medium | Repack strategy producing serve-optimized packs (shallow deltas) |
| Cache invalidation | Low–Medium | Invalidate on push/fetch; content-addressed so mostly append-only |

**Failure modes are non-obvious**: cache miss storms → GCS egress amplification;
pathological delta chains → high CPU tail latencies; debugging "why blob X is slow"
becomes "trace the delta graph + cache state + range reads" instead of `strace`.

### Where Object-Store Snapshots Would Win

The object-store path becomes attractive if any of these become core requirements:

- **Multi-region workers** without cross-region NFS (Filestore is single-region)
- **Immutable, globally distributable repo snapshots** (CDN-compatible)
- **Decoupled storage/compute scaling** (object store scales independently)
- **Artifact-store semantics** (non-POSIX, content-addressed, versioned)
- **Cold-start acceleration** for ephemeral workers (pull snapshot vs NFS mount)

### Recommended Deferral Path

**Phase N (future)**: "Snapshot as cache, not primary store"

Keep Filestore as source of truth. Periodically produce packed snapshot artifacts
into GCS for specific use cases:

```
Filestore (NFS)              GCS (object store)
━━━━━━━━━━━━━━━━━           ━━━━━━━━━━━━━━━━━━━
source of truth              derived / cached
hot path (online workers)    cold-start / cross-region / DR
POSIX git ops                indexing jobs that don't need POSIX
```

Do NOT switch the online hot path to range-read reconstruction until there is
hard evidence that NFS is the bottleneck.

### Revisit Triggers

Revisit the object-store architecture if any of these metrics are observed in
production (minimum 30 days of stable Filestore operation):

| Metric | Threshold | Implication |
|--------|-----------|-------------|
| p95 bare repo read latency | > 50ms sustained | NFS metadata or pack read bottleneck |
| Filestore IOPS saturation | > 80% of provisioned tier | Need to scale beyond single Filestore instance |
| Cross-region worker requirement | Any | Filestore is single-region; NFS over WAN is not viable |
| Filestore monthly cost | > 2× equivalent GCS Standard cost | Economics favor object store |
| Cold-start time (new worker) | > 30s for repo mount availability | Snapshot pull may be faster |
| Concurrent worker count | > 50 per Filestore instance | NFS connection/throughput limits |

Until these triggers fire, the NFS design is correct, simple, and aligned with
git's expectations. The `RepoStorage` trait abstraction means switching to an
object-store backend later is a provider implementation change, not an
architectural redesign.

---

## 12. Implementation Milestones

Concrete, trackable work units organized by phase. Each milestone is a
shippable PR (or small PR chain) with its own CI gate. Milestones within
a phase can be parallelized unless marked sequential.

### Milestone 0: Immediate Guardrails (Phase 0)

**Goal**: Harden JWT validation + startup checks. No breaking changes.
**Blocked by**: Nothing — can start immediately.

| # | Task | Spec ref | Est. | Deliverable |
|---|------|----------|------|-------------|
| 0.1 | Add `iss`, `aud`, `jti` to `JwtClaims` struct | §4.1 Change 1 | S | PR: struct changes + unit tests |
| 0.2 | Add `iss`/`aud` validation to `JwtValidator` | §4.1 Change 2 | S | PR: validator changes + rejection tests |
| 0.3 | Add startup guardrails (env var assertions) | §4.2 Change 3, §4.6 | S | PR: startup checks + CI |
| 0.4 | Add `after_acquire` hook (NOP clear_ctx) | §4.5 Change 6 note | S | PR: pool config + defense-in-depth |

**Exit criteria**: All JWT tests pass. Server refuses to start without `G5_JWT_ISSUER`, `G5_JWT_AUDIENCE`.

---

### Milestone 1: Crate Boundaries (Phase 1)

**Goal**: Create `g5-authz` and `g5-blob` crates. Wire `OrgGuard`, `scoped_route!`, `RequireScopes`.
**Blocked by**: Milestone 0.

| # | Task | Spec ref | Est. | Deliverable |
|---|------|----------|------|-------------|
| 1.1 | Create `g5-authz` crate skeleton | §3.1 | M | Cargo.toml, types, `AuthzService` trait, `InProcessAuthz` |
| 1.2 | Create `g5-blob` crate skeleton | §3.2 | M | Cargo.toml, `BlobService` trait, `InProcessBlob`, Ed25519 signing |
| 1.3 | Implement `scoped_route!` macro | §4.3 Change 3 | M | Macro + CI grep gate for direct `.route()` calls |
| 1.4 | Implement `OrgGuard` middleware | §4.3 Change 1 | M | Middleware + membership check + Redis caching |
| 1.5 | Refactor `AppState` (stateless services) | §4.4 Changes 1-3 | L | Remove `PgPool` from service structs, inject via method params |
| 1.6 | Populate `scope_map.yaml` (125 routes) | §4.3 Change 3 | M | YAML file + `all_routes_have_required_scopes` CI test |
| 1.7 | Add `AuthContext::Worker` variant | §4.1 Change 3 | S | Worker JWT discrimination + `worker_id` URL enforcement |
| 1.8 | Update root spec (`g5.semlift.md`) | §4.10 | S | New crate entries + dependency graph |

**Exit criteria**: All routes wrapped in `scoped_route!`. CI grep gate passes. `g5-authz` and `g5-blob` compile independently.

---

### Milestone 2: DB Schema + ScopedDb (Phase 2)

**Goal**: Dual pools, `authz` schema, `ScopedDb`, `session_ctx` table.
**Blocked by**: Milestone 1.

| # | Task | Spec ref | Est. | Deliverable |
|---|------|----------|------|-------------|
| 2.1 | Create `authz` schema migration | §4.5 Change 2 | M | `user_sessions`, `session_ctx`, `set_ctx()`, `clear_ctx()`, `current_org_ids()` |
| 2.2 | Create DB role provisioning (IaC) | §4.5 Change 3, §4.7 | M | Terraform/IaC for `g5_app`, `authz_writer`, `authz_ctx_writer` roles |
| 2.3 | Implement dual pool startup | §4.5 Change 1, §4.6 | M | `tenant_pool`, `authz_pool`, `membership_pool` + connection budget |
| 2.4 | Implement `ScopedDb` | §4.5 Change 6 | L | `from_capability()`, `commit()`, `Drop` guard, scoped transaction |
| 2.5 | Integrate `ScopedDb` into service layer | §4.5 §3.67 | XL | Migrate all service methods from `PgPool` → `&mut ScopedDb` |
| 2.6 | Add `user_sessions` pruning task | §4.5 Change 6 | S | Maintenance task in `MaintenanceRunner` |

**Exit criteria**: All service methods use `ScopedDb`. `current_org_ids()` returns correct arrays. `after_acquire` hooks clear stale context.

---

### Milestone 3: org_id Denormalization + RLS (Phase 3)

**Goal**: Add `org_id` to all 20 tables, enable RLS policies.
**Blocked by**: Milestone 2.

| # | Task | Spec ref | Est. | Deliverable |
|---|------|----------|------|-------------|
| 3.1 | Add `org_id` columns (nullable) | §4.5 Change 4 | M | Migration: ADD COLUMN org_id UUID to 20 tables |
| 3.2 | Backfill `org_id` from FK chains | §4.5 Change 4 | M | Batch UPDATE scripts for each table |
| 3.3 | Set `org_id` NOT NULL | §4.5 Change 4 | S | Migration: ALTER COLUMN SET NOT NULL |
| 3.4 | Create RLS policies | §4.5 Change 5 | L | `CREATE POLICY` for all 20 tables + ENABLE ROW LEVEL SECURITY |
| 3.5 | Remove raw `PgPool` from `AppState` | §4.4 Change 1 | M | Force all queries through `ScopedDb` |
| 3.6 | Benchmark RLS on `search_documents` | §10 Q4 | S | p95 latency comparison; decide RLS vs explicit WHERE |

**Exit criteria**: RLS enabled on all tables. Zero raw `PgPool` usage in route handlers. `current_org_ids()` governs all data access.

---

### Milestone 4: Blob Capabilities (Phase 4)

**Goal**: Ed25519-signed blob capability tokens for content-addressed reads.
**Blocked by**: Milestone 1 (g5-blob crate exists).

| # | Task | Spec ref | Est. | Deliverable |
|---|------|----------|------|-------------|
| 4.1 | Implement Ed25519 blob signing | §3.2 | M | Key generation, JWS Compact, `mint_blob_capability()` |
| 4.2 | Implement `BlobService` in `AppState` | §4.4 | S | Wire into `AppState`, read-path verification |
| 4.3 | Add `token_use` discrimination | §3.2 | S | Middleware rejects wrong `token_use` on all endpoints |

**Exit criteria**: Blob reads require valid Ed25519 capability token. Token confusion attacks prevented.

---

### Milestone F: FS Migration (Independent Track)

**Goal**: `RepoStorage` trait, `LocalRepoStorage`, Filestore provider.
**Blocked by**: Nothing — can start in parallel with Milestones 1-3.

| # | Task | Spec ref | Est. | Deliverable |
|---|------|----------|------|-------------|
| F.1 | Implement `RepoStorage` trait | §3.3 | M | Trait + `LocalRepoStorage` + `validate_path()` |
| F.2 | Implement `FilestoreRepoStorage` | §3.3 | L | NFS mount, `O_CREAT\|O_EXCL` locking, bare repo management |
| F.3 | Implement `ManifestService` | §3.4 | M | `repo_manifests` table, checkout tracking, cache invalidation |
| F.4 | Implement `StorageConfig` + env vars | §3.3 | S | `G5_STORAGE_BACKEND`, `G5_STORAGE_WORKTREE_DIR`, etc. |
| F.5 | Refactor workers to use `RepoStorage` | §4.8, §4.9 | L | Replace direct filesystem access in g5d lifecycle |
| F.6 | Replace worker service token with session token | §4.8 Change 1, §4.9 | M | Session token minting, refresh-on-heartbeat |
| F.7 | Add Filestore infra (Terraform) | §4.7 | M | GCP Filestore instance, NFS mount config, network rules |
| F.8 | NFS locking conformance test | §3.3 | S | CI test: concurrent lock + stale detection |

**Exit criteria**: Workers use `RepoStorage` for all file operations. Filestore provider passes conformance tests. `worktree_root()` on local SSD.

---

### Milestone C: CRUD Lifecycle (§4.11)

**Goal**: Create Org / Create Workspace / Add Repo flows threaded through new auth model.
**Blocked by**: Milestone 2 (ScopedDb) + Milestone F.1 (RepoStorage).

| # | Task | Spec ref | Est. | Deliverable |
|---|------|----------|------|-------------|
| C.1 | Wire `create_org` through `system_pool` | §4.11.1 | M | Stateless `OrgService`, post-commit storage init |
| C.2 | Wire `create_workspace` through `ScopedDb` | §4.11.2 | M | `RequireScopes("workspaces:write")`, post-commit repo init |
| C.3 | Wire `add_repo` through `ScopedDb` + `RepoStorage` | §4.11.3 | L | 10-step flow: DB insert → git clone → manifest → reindex |
| C.4 | Add "Create Workspace" frontend dialog | — | M | Wire `useCreateWorkspace` hook to `ProjectsPage` button |
| C.5 | Add "Create Doc" frontend form | — | S | Wire `POST /docs` to a creation dialog |

**Exit criteria**: All three CRUD flows work end-to-end. Frontend buttons functional.

---

### Milestone I: Indexer Pipeline (§4.12)

**Goal**: Ontology reconciler + linked doc indexer + `G5_ROLE` deployment split.
**Blocked by**: Milestone 3 (org_id on all tables) + Milestone F.1 (RepoStorage).

| # | Task | Spec ref | Est. | Deliverable |
|---|------|----------|------|-------------|
| I.1 | Implement `G5_ROLE` dispatch in `main.rs` | §4.12.0 | M | api / indexer / all role flag, no HTTP on indexer |
| I.2 | Implement ontology reconciler | §4.12.2 | L | `reconcile_ontology_for_reindex_event()`, `find_nearest_module_ancestor()` |
| I.3 | Implement linked doc indexer | §4.12.3 | M | `index_linked_documents()`, YAML frontmatter parsing, `resolve_node_references()` |
| I.4 | Integrate with search indexer loop | §4.12.4 | M | Three-processor `process_single_event()` in one transaction |
| I.5 | Add `linked_nodes` to `FrontmatterCard` | §4.12.2.1 | S | One-line addition to `NAVIGABLE_FIELDS` |
| I.6 | Add `g5-api-indexer` to docker-compose | §4.12.0 | S | Service definition, no ports, internal network |
| I.7 | Indexer crash recovery reaper | §4.12.0.1 | S | Mark stale `processing` rows back to `pending` |

**Exit criteria**: Ontology auto-syncs on commit. Linked doc badges update. Multiple indexer replicas scale linearly.

---

### Phase Dependency Summary

```
M0 (Guardrails)
 └─→ M1 (Crate bounds)
      ├─→ M2 (ScopedDb) ─→ M3 (RLS) ─→ MI (Indexer)
      ├─→ M4 (Blob cap)
      └─→ MF (FS Migration) ─→ MC (CRUD) ─→ MI (Indexer)
```

### Size Estimates

| Symbol | Meaning | Rough effort |
|--------|---------|-------------|
| S | Small | < 1 day |
| M | Medium | 1-3 days |
| L | Large | 3-5 days |
| XL | Extra large | 5-10 days |

### Total estimated effort

| Milestone | Tasks | Estimated days |
|-----------|-------|---------------|
| M0: Guardrails | 4 | 2-3 |
| M1: Crate bounds | 8 | 12-18 |
| M2: ScopedDb | 6 | 12-16 |
| M3: RLS | 6 | 8-12 |
| M4: Blob cap | 3 | 3-5 |
| MF: FS Migration | 8 | 14-20 |
| MC: CRUD Lifecycle | 5 | 6-10 |
| MI: Indexer Pipeline | 7 | 8-12 |
| **Total** | **47 tasks** | **65-96 days** |

Critical path: M0 → M1 → M2 → M3 → MI ≈ 34-49 days.
MF runs in parallel with M1-M3, so it doesn't add to critical path if started early.
