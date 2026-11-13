//! # G5 Authorization Service Crate
//!
//! Pure policy + capability minting. Provides the [`AuthzService`] trait
//! (object-safe, `Send + Sync`) for org membership checks and scoped
//! database capability minting ([`DbCapability`]).
//!
//! Membership checks run inside the capability-minting operations of
//! `g5-api`, which call [`AuthzService::is_org_member()`] via
//! `Arc<dyn AuthzService>` injected from `AppState`.
//!
//! **Critical design constraint**: This crate MUST NOT depend on `g5-api`
//! or any `g5-api` types (`AppState`, `AppError`, `axum`).

// Compile-time guards: flag public functions returning Result/Option without #[must_use]
#![warn(clippy::must_use_candidate)]
#![warn(clippy::return_self_not_must_use)]

pub mod capability;
pub mod scoped_db;

use async_trait::async_trait;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use g5_contracts::types::ids::{OrgId, SessionId, UserId};

pub use capability::DbCapability;
pub use scoped_db::{ScopedDb, ScopedDbError, ScopedDbReopener};

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

/// Authorization errors.
///
/// Converted to `AppError` at the `g5-api` boundary via
/// `impl From<AuthzError> for AppError` in g5-api — NOT here.
#[derive(Debug, thiserror::Error)]
pub enum AuthzError {
    #[error("not a member of org {0}")]
    NotMember(Uuid),

    /// Defense-in-depth scope checks in the service layer.
    /// NOT raised by `mint_db_capability` — primary scope enforcement
    /// is at the route layer (`RequireScopes`).
    #[error("insufficient scopes: missing {}", .0.join(", "))]
    InsufficientScopes(Vec<String>),

    #[error("internal: {0}")]
    Internal(String),
}

// ---------------------------------------------------------------------------
// AuthzService trait
// ---------------------------------------------------------------------------

/// Authorization service contract.
///
/// In-process: function calls via [`InProcessAuthz`].
/// Extracted (Phase 5): HTTP calls to g5-authz service via `AuthzHttpClient`.
#[async_trait]
pub trait AuthzService: Send + Sync + 'static {
    /// Check if a user is a member of an org.
    ///
    /// Used by the capability-minting operations before minting any capabilities.
    /// Uses `membership_pool` (read-only SELECT on `org_memberships`).
    async fn is_org_member(&self, actor_id: UserId, org_id: OrgId) -> Result<bool, AuthzError>;

    /// Mint a DB session capability for the given actor + org(s).
    ///
    /// 1. Verifies org membership for each org in `target_org_ids`
    /// 2. Creates or reuses an exact-context `authz.user_sessions` row
    /// 3. Returns `DbCapability { org_ids, actor_id, session_id, ttl }`
    ///
    /// Accepts identity components (not `JwtClaims`) so both
    /// Both request-bound and system-owned callers can use this path.
    async fn mint_db_capability(
        &self,
        actor_id: UserId,
        target_org_ids: &[OrgId],
        scopes: &[String],
    ) -> Result<DbCapability, AuthzError>;

    /// Mint a DB session capability for a server-internal flow whose org
    /// context has already been constrained by the caller.
    ///
    /// This intentionally skips user membership checks and returns only an
    /// RLS-scoped DB session, not a user credential. Callers must not expose
    /// tenant data or mint user tokens from this capability without a later
    /// authenticated authorization check.
    async fn mint_service_db_capability(
        &self,
        actor_id: UserId,
        org_id: OrgId,
        scopes: &[String],
    ) -> Result<DbCapability, AuthzError>;

    /// Retrieve all org IDs the user is a member of.
    ///
    /// Used by cross-org endpoints (dashboard, global search, notifications).
    async fn user_org_ids(&self, actor_id: UserId) -> Result<Vec<OrgId>, AuthzError>;
}

// ---------------------------------------------------------------------------
// InProcessAuthz implementation
// ---------------------------------------------------------------------------

use sqlx::PgPool;

/// In-process authz implementation using restricted DB pools.
///
/// - `membership_pool`: read-only SELECT on org membership
/// - `authz_pool`: INSERT/UPDATE/DELETE on `authz.user_sessions`
///
/// Membership is read from `org_memberships` on every check (one primary-key
/// probe) so a removed member loses access on their next request; there is
/// deliberately no cache in front of it.
///
/// Neither pool is the tenant data pool (`g5_app` role).
pub struct InProcessAuthz {
    /// Read-only DB pool for membership checks.
    membership_pool: PgPool,
    /// Authz-writer pool — INSERT/UPDATE/DELETE on `authz.user_sessions`.
    authz_pool: PgPool,
}

struct DbSessionLease {
    session_id: SessionId,
    ttl_secs: u32,
}

impl InProcessAuthz {
    const DB_SESSION_TTL_SECS: u32 = 3600;
    const DB_SESSION_REFRESH_BUFFER_SECS: i64 = 30;

    #[must_use]
    pub fn new(membership_pool: PgPool, authz_pool: PgPool) -> Self {
        Self {
            membership_pool,
            authz_pool,
        }
    }

    fn canonicalize_context(
        org_ids: impl IntoIterator<Item = OrgId>,
        scopes: &[String],
    ) -> (Vec<OrgId>, Vec<Uuid>, Vec<String>) {
        let mut raw_org_ids: Vec<Uuid> = org_ids.into_iter().map(OrgId::into_uuid).collect();
        raw_org_ids.sort_unstable();
        raw_org_ids.dedup();

        let canonical_org_ids: Vec<OrgId> =
            raw_org_ids.iter().copied().map(OrgId::from_uuid).collect();

        let mut canonical_scopes = scopes.to_vec();
        canonical_scopes.sort_unstable();
        canonical_scopes.dedup();

        (canonical_org_ids, raw_org_ids, canonical_scopes)
    }

    fn scope_hash(scopes: &[String]) -> Vec<u8> {
        let mut hasher = Sha256::new();
        hasher.update(b"g5-authz-scope-v1;");
        for scope in scopes {
            let bytes = scope.as_bytes();
            hasher.update(bytes.len().to_string().as_bytes());
            hasher.update(b":");
            hasher.update(bytes);
        }
        hasher.finalize().to_vec()
    }

    fn org_hash(org_ids: &[Uuid]) -> Vec<u8> {
        let mut hasher = Sha256::new();
        hasher.update(b"g5-authz-org-v1;");
        for org_id in org_ids {
            let text = org_id.to_string();
            let bytes = text.as_bytes();
            hasher.update(bytes.len().to_string().as_bytes());
            hasher.update(b":");
            hasher.update(bytes);
        }
        hasher.finalize().to_vec()
    }

    fn db_session_expires_at() -> chrono::DateTime<chrono::Utc> {
        chrono::Utc::now() + chrono::Duration::seconds(i64::from(Self::DB_SESSION_TTL_SECS))
    }

    fn db_session_refresh_after() -> chrono::DateTime<chrono::Utc> {
        chrono::Utc::now() + chrono::Duration::seconds(Self::DB_SESSION_REFRESH_BUFFER_SECS)
    }

    fn db_session_lease_from_row(
        (session_id, expires_at): (Uuid, chrono::DateTime<chrono::Utc>),
    ) -> DbSessionLease {
        let remaining = (expires_at - chrono::Utc::now()).num_seconds();
        DbSessionLease {
            session_id: SessionId::from_uuid(session_id),
            ttl_secs: remaining.clamp(0, i64::from(u32::MAX)) as u32,
        }
    }

    async fn select_fresh_db_session(
        &self,
        actor_uuid: Uuid,
        raw_org_ids: &[Uuid],
        scopes: &[String],
    ) -> Result<Option<DbSessionLease>, AuthzError> {
        let refresh_after = Self::db_session_refresh_after();
        let row = sqlx::query_as::<_, (Uuid, chrono::DateTime<chrono::Utc>)>(
            "SELECT session_id, expires_at
               FROM authz.user_sessions
              WHERE user_id = $1
                AND org_ids = $2
                AND scopes = $3
                AND expires_at > $4
              LIMIT 1",
        )
        .bind(actor_uuid)
        .bind(raw_org_ids)
        .bind(scopes)
        .bind(refresh_after)
        .fetch_optional(&self.authz_pool)
        .await
        .map_err(|e| AuthzError::Internal(e.to_string()))?;

        Ok(row.map(Self::db_session_lease_from_row))
    }

    async fn insert_db_session(
        &self,
        actor_uuid: Uuid,
        raw_org_ids: &[Uuid],
        scopes: &[String],
        expires_at: chrono::DateTime<chrono::Utc>,
    ) -> Result<Option<DbSessionLease>, AuthzError> {
        let candidate_session_id = SessionId::new().into_uuid();
        let org_hash = Self::org_hash(raw_org_ids);
        let scope_hash = Self::scope_hash(scopes);

        let row = sqlx::query_as::<_, (Uuid, chrono::DateTime<chrono::Utc>)>(
            "INSERT INTO authz.user_sessions
                (session_id, user_id, org_ids, scopes, org_hash, scope_hash, expires_at, refreshed_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, now())
             ON CONFLICT (user_id, org_hash, scope_hash) DO NOTHING
             RETURNING session_id, expires_at",
        )
        .bind(candidate_session_id)
        .bind(actor_uuid)
        .bind(raw_org_ids)
        .bind(scopes)
        .bind(org_hash)
        .bind(scope_hash)
        .bind(expires_at)
        .fetch_optional(&self.authz_pool)
        .await
        .map_err(|e| AuthzError::Internal(e.to_string()))?;

        Ok(row.map(Self::db_session_lease_from_row))
    }

    async fn reuse_or_create_db_session(
        &self,
        actor_id: UserId,
        raw_org_ids: &[Uuid],
        scopes: &[String],
    ) -> Result<DbSessionLease, AuthzError> {
        let actor_uuid = actor_id.into_uuid();
        let expires_at = Self::db_session_expires_at();

        if let Some(lease) = self
            .insert_db_session(actor_uuid, raw_org_ids, scopes, expires_at)
            .await?
        {
            return Ok(lease);
        }

        if let Some(lease) = self
            .select_fresh_db_session(actor_uuid, raw_org_ids, scopes)
            .await?
        {
            return Ok(lease);
        }

        let refresh_after = Self::db_session_refresh_after();
        if let Some(row) = sqlx::query_as::<_, (Uuid, chrono::DateTime<chrono::Utc>)>(
            "UPDATE authz.user_sessions
                SET expires_at = $4,
                    refreshed_at = now()
              WHERE user_id = $1
                AND org_ids = $2
                AND scopes = $3
                AND expires_at <= $5
              RETURNING session_id, expires_at",
        )
        .bind(actor_uuid)
        .bind(raw_org_ids)
        .bind(scopes)
        .bind(expires_at)
        .bind(refresh_after)
        .fetch_optional(&self.authz_pool)
        .await
        .map_err(|e| AuthzError::Internal(e.to_string()))?
        {
            return Ok(Self::db_session_lease_from_row(row));
        }

        if let Some(lease) = self
            .insert_db_session(
                actor_uuid,
                raw_org_ids,
                scopes,
                Self::db_session_expires_at(),
            )
            .await?
        {
            return Ok(lease);
        }

        self.select_fresh_db_session(actor_uuid, raw_org_ids, scopes)
            .await?
            .ok_or_else(|| {
                AuthzError::Internal(
                    "authz exact-context session reuse could not return a fresh row".to_string(),
                )
            })
    }

    async fn mint_unchecked_db_capability(
        &self,
        actor_id: UserId,
        org_ids: Vec<OrgId>,
        scopes: &[String],
    ) -> Result<DbCapability, AuthzError> {
        let (org_ids, raw_org_ids, scopes) = Self::canonicalize_context(org_ids, scopes);
        let lease = self
            .reuse_or_create_db_session(actor_id, &raw_org_ids, &scopes)
            .await?;

        Ok(DbCapability {
            org_ids,
            actor_id,
            scopes,
            session_id: lease.session_id,
            ttl_secs: lease.ttl_secs,
        })
    }
}

#[async_trait]
impl AuthzService for InProcessAuthz {
    async fn is_org_member(&self, actor_id: UserId, org_id: OrgId) -> Result<bool, AuthzError> {
        // The authoritative membership table, one primary-key probe. No cache:
        // a removed member must fail on their next capability mint.
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(
                SELECT 1 FROM org_memberships WHERE org_id = $1 AND user_id = $2 AND deactivated_at IS NULL
            )",
        )
        .bind(org_id.into_uuid())
        .bind(actor_id.into_uuid())
        .fetch_one(&self.membership_pool)
        .await
        .map_err(|e| AuthzError::Internal(e.to_string()))?;
        Ok(exists)
    }

    async fn mint_db_capability(
        &self,
        actor_id: UserId,
        target_org_ids: &[OrgId],
        scopes: &[String],
    ) -> Result<DbCapability, AuthzError> {
        // 1. Verify org membership for EVERY org
        for &oid in target_org_ids {
            if !self.is_org_member(actor_id, oid).await? {
                return Err(AuthzError::NotMember(oid.into_uuid()));
            }
        }

        // 2. Reuse only an exact actor + org-set + scope-set match. Reusing a
        // broader per-actor row would widen RLS visibility across org switches.
        let (org_ids, raw_org_ids, scopes) =
            Self::canonicalize_context(target_org_ids.iter().copied(), scopes);
        let lease = self
            .reuse_or_create_db_session(actor_id, &raw_org_ids, &scopes)
            .await?;

        Ok(DbCapability {
            org_ids,
            actor_id,
            scopes,
            session_id: lease.session_id,
            ttl_secs: lease.ttl_secs,
        })
    }

    async fn mint_service_db_capability(
        &self,
        actor_id: UserId,
        org_id: OrgId,
        scopes: &[String],
    ) -> Result<DbCapability, AuthzError> {
        self.mint_unchecked_db_capability(actor_id, vec![org_id], scopes)
            .await
    }

    async fn user_org_ids(&self, actor_id: UserId) -> Result<Vec<OrgId>, AuthzError> {
        let ids: Vec<Uuid> = sqlx::query_scalar(
            "SELECT org_id FROM org_memberships WHERE user_id = $1 AND deactivated_at IS NULL",
        )
        .bind(actor_id.into_uuid())
        .fetch_all(&self.membership_pool)
        .await
        .map_err(|e| AuthzError::Internal(e.to_string()))?;

        Ok(ids.into_iter().map(OrgId::from_uuid).collect())
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn authz_error_display() {
        let err = AuthzError::NotMember(Uuid::nil());
        assert!(err.to_string().contains("not a member"));

        let err = AuthzError::InsufficientScopes(vec!["read".into(), "write".into()]);
        assert!(err.to_string().contains("read, write"));

        let err = AuthzError::Internal("db down".into());
        assert!(err.to_string().contains("db down"));
    }

    #[test]
    fn canonicalize_context_sorts_and_deduplicates_orgs_and_scopes() {
        let org_a =
            OrgId::from_uuid(Uuid::parse_str("00000000-0000-0000-0000-00000000000a").unwrap());
        let org_b =
            OrgId::from_uuid(Uuid::parse_str("00000000-0000-0000-0000-00000000000b").unwrap());
        let scopes = vec![
            "workspaces:read".to_string(),
            "tasks:write".to_string(),
            "workspaces:read".to_string(),
        ];

        let (org_ids, raw_org_ids, scopes) =
            InProcessAuthz::canonicalize_context([org_b, org_a, org_b], &scopes);

        assert_eq!(org_ids, vec![org_a, org_b]);
        assert_eq!(raw_org_ids, vec![org_a.into_uuid(), org_b.into_uuid()]);
        assert_eq!(
            scopes,
            vec!["tasks:write".to_string(), "workspaces:read".to_string()]
        );
    }

    #[test]
    fn scope_hash_is_stable_for_canonical_scope_sets() {
        let scopes = vec![
            "workspaces:read".to_string(),
            "tasks:write".to_string(),
            "workspaces:read".to_string(),
        ];
        let reversed_scopes = vec!["tasks:write".to_string(), "workspaces:read".to_string()];
        let (_, _, canonical_scopes) =
            InProcessAuthz::canonicalize_context(std::iter::empty::<OrgId>(), &scopes);
        let (_, _, reversed_canonical_scopes) =
            InProcessAuthz::canonicalize_context(std::iter::empty::<OrgId>(), &reversed_scopes);

        assert_eq!(
            InProcessAuthz::scope_hash(&canonical_scopes),
            InProcessAuthz::scope_hash(&reversed_canonical_scopes)
        );
        assert_ne!(
            InProcessAuthz::scope_hash(&canonical_scopes),
            InProcessAuthz::scope_hash(&["workspaces:read".to_string()])
        );
    }

    #[test]
    fn org_hash_is_stable_for_canonical_org_sets() {
        let org_a =
            OrgId::from_uuid(Uuid::parse_str("00000000-0000-0000-0000-00000000000a").unwrap());
        let org_b =
            OrgId::from_uuid(Uuid::parse_str("00000000-0000-0000-0000-00000000000b").unwrap());
        let (_, raw_org_ids, _) = InProcessAuthz::canonicalize_context([org_b, org_a, org_b], &[]);
        let (_, reversed_raw_org_ids, _) =
            InProcessAuthz::canonicalize_context([org_a, org_b], &[]);

        assert_eq!(
            InProcessAuthz::org_hash(&raw_org_ids),
            InProcessAuthz::org_hash(&reversed_raw_org_ids)
        );
        assert_ne!(
            InProcessAuthz::org_hash(&raw_org_ids),
            InProcessAuthz::org_hash(&[org_a.into_uuid()])
        );
    }

    #[test]
    fn db_capability_fields() {
        let cap = DbCapability {
            org_ids: vec![OrgId::new()],
            actor_id: UserId::new(),
            scopes: vec!["workspaces:read".into()],
            session_id: SessionId::new(),
            ttl_secs: 3600,
        };
        assert_eq!(cap.ttl_secs, 3600);
        assert_eq!(cap.scopes.len(), 1);
    }

    #[test]
    fn authz_service_is_object_safe() {
        // Compile-time check: AuthzService can be used as dyn trait
        fn _accepts(_s: &dyn AuthzService) {}
    }
}
