//! Permission service — OAuth scope management and enforcement.
//!
//! Implements scope resolution, enforcement, and management per
//! API_AND_STORAGE.md §2.12 and §3.16. This is the single source of truth
//! for permission resolution in `g5-api`.
//!
//! # Callers
//!
//! 1. **`AuthService`** — during JWT minting to compute the `scopes` claim.
//! 2. **Auth middleware** — via `check_scopes()` for runtime scope enforcement.
//! 3. **Route handlers** — for the Permissions API (§3.16) to manage scopes.
//!
//! # Design
//!
//! Scope resolution uses a **union** algorithm:
//! ```text
//! effective_scopes(user, org, workspace?) =
//!     org_default_scopes[org]
//!     ∪ user_scope_grants[org, user]
//!     ∪ workspace_permission_overrides[workspace, user]
//! ```
//!
//! This ensures that adding permissions in one place never removes permissions
//! from another place (principle of least surprise).

use std::collections::BTreeSet;

use chrono::{DateTime, Utc};
use g5_authz::ScopedDb;
use g5_contracts::error::G5Error;
use g5_contracts::types::enums::OrgRole;
use serde::{Deserialize, Serialize};
use sqlx::{PgConnection, PgPool};
use tracing::info;
use uuid::Uuid;

use crate::db::scoped::{workspace_access_for_role, WorkspaceAccessResult};
use crate::errors::AppError;

// ---------------------------------------------------------------------------
// DTO types
// ---------------------------------------------------------------------------

/// Result of a scope check operation.
///
/// When `allowed` is `true`, `missing` is empty. When `allowed` is `false`,
/// `missing` contains the scope strings the user lacks.
///
/// Note: This is a result, not an error. The caller decides whether to return
/// a 403 based on the `allowed` field.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScopeCheckResult {
    pub allowed: bool,
    pub missing: Vec<String>,
}

/// User scope grant with full metadata.
///
/// Returned by `get_user_scopes` and `set_user_scopes`. Contains all
/// columns from the `user_scope_grants` table formatted for API responses.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UserScopeGrantDTO {
    pub id: Uuid,
    pub org_id: Uuid,
    pub user_id: Uuid,
    pub scope: String,
    pub granted_by: Option<Uuid>,
    pub granted_at: String,
    pub expires_at: Option<String>,
}

/// Workspace permission override with full metadata.
///
/// Returned by `get_workspace_overrides` and `set_workspace_override`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceOverrideDTO {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub user_id: Uuid,
    pub scope: String,
    pub granted_by: Option<Uuid>,
    pub granted_at: String,
}

/// Effective scopes response with breakdown by source.
///
/// Used by `GET /api/v1/auth/me/scopes` to show the user their permission
/// sources.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EffectiveScopesDTO {
    pub scopes: Vec<String>,
    pub org_defaults: Vec<String>,
    pub user_grants: Vec<String>,
    pub workspace_overrides: Vec<String>,
}

/// Request to set user scopes (from route handler).
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetUserScopesRequest {
    pub scopes: Vec<String>,
    pub expires_at: Option<String>,
}

/// Request to set workspace overrides (from route handler).
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetWorkspaceOverrideRequest {
    pub scopes: Vec<String>,
}

/// Request to check scopes (from route handler).
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScopeCheckRequest {
    pub scopes: Vec<String>,
    pub workspace_id: Option<Uuid>,
}

// ---------------------------------------------------------------------------
// PermissionService
// ---------------------------------------------------------------------------

/// OAuth scope management and enforcement service.
///
/// Provides the scope resolution algorithm that computes a user's effective
/// permissions within an organization and optionally within a specific
/// workspace context.
#[derive(Default)]
pub struct PermissionService;

impl PermissionService {
    /// Hold current membership through the operation consuming it. Execution
    /// additionally excludes cleanup retained across early reactivation.
    pub async fn require_active_member_on_conn(
        conn: &mut PgConnection,
        user_id: Uuid,
        org_id: Uuid,
        execution: bool,
    ) -> Result<OrgRole, G5Error> {
        let row =
            sqlx::query_as::<_, (String, Option<chrono::DateTime<chrono::Utc>>, Option<Uuid>)>(
                "SELECT role, deactivated_at, deactivation_cleanup_id
             FROM authz.lock_org_membership($1, $2)",
            )
            .bind(org_id)
            .bind(user_id)
            .fetch_optional(conn)
            .await?;
        let Some((role, None, cleanup)) = row else {
            return Err(G5Error::Forbidden {
                missing_scopes: vec![],
            });
        };
        if execution && cleanup.is_some() {
            return Err(G5Error::WorkspaceStopping);
        }
        role.parse().map_err(|_| G5Error::Internal {
            message: "Invalid membership role".into(),
        })
    }
    // -----------------------------------------------------------------------
    // Scope resolution
    // -----------------------------------------------------------------------

    /// The additive authority a membership may actually use
    /// (`g5.organization.viewer-ceiling`): organization defaults, individual
    /// grants and workspace overrides raise a viewer only to read-only scopes,
    /// so a configured write or administrative grant confers nothing. Other
    /// roles keep their configured grants pending the grant-lifecycle
    /// question. Every resolver of current or issued authority applies this.
    pub fn grants_within_role_ceiling(
        role: OrgRole,
        grants: impl IntoIterator<Item = String>,
    ) -> Vec<String> {
        grants
            .into_iter()
            .filter(|scope| {
                role != OrgRole::Viewer
                    || scope
                        .parse::<g5_auth::Scope>()
                        .is_ok_and(|scope| Self::scope_within_role_ceiling(role, &scope))
            })
            .collect()
    }

    /// The one ceiling rule, shared by current authority and issued tokens: a
    /// viewer may use only read-only scopes; other roles use what is
    /// configured.
    #[must_use]
    pub fn scope_within_role_ceiling(role: OrgRole, scope: &g5_auth::Scope) -> bool {
        role != OrgRole::Viewer || scope.is_read_only()
    }

    /// The current active role of `user_id` in `org_id`, if any.
    async fn active_role_on_conn(
        conn: &mut PgConnection,
        org_id: Uuid,
        user_id: Uuid,
    ) -> Result<Option<OrgRole>, G5Error> {
        let role: Option<String> = sqlx::query_scalar(
            "SELECT role FROM org_memberships WHERE org_id = $1 AND user_id = $2 AND deactivated_at IS NULL",
        )
        .bind(org_id)
        .bind(user_id)
        .fetch_optional(conn)
        .await?;
        role.map(|role| {
            role.parse::<OrgRole>().map_err(|_| G5Error::Internal {
                message: format!("Invalid role in org_memberships: {role}"),
            })
        })
        .transpose()
    }

    /// Compute the effective scopes for a user within an org.
    ///
    /// Optionally includes workspace-level overrides. This is the core
    /// algorithm of the permission system.
    ///
    /// # Scope resolution algorithm (NON-NEGOTIABLE)
    ///
    /// ```text
    /// effective_scopes(user, org, workspace?) =
    ///     org_default_scopes[org]
    ///     ∪ user_scope_grants[org, user]        (non-expired only)
    ///     ∪ workspace_permission_overrides[workspace, user]   (if workspace)
    /// ```
    ///
    /// Returns a **sorted** `Vec<String>` of effective scope strings.
    ///
    /// # Performance
    ///
    /// Completes in a single database round-trip using a UNION query.
    pub async fn resolve_effective_scopes(
        &self,
        db: &mut ScopedDb,
        user_id: Uuid,
        org_id: Uuid,
        workspace_id: Option<Uuid>,
    ) -> Result<Vec<String>, AppError> {
        if let Some(ws_id) = workspace_id {
            self.ensure_workspace_in_org(db, org_id, ws_id).await?;
        }

        let scopes = match workspace_id {
            Some(ws_id) => {
                sqlx::query_scalar!(
                    r#"
                    SELECT DISTINCT scope AS "scope!"
                    FROM (
                        SELECT scope FROM org_default_scopes WHERE org_id = $1
                        UNION
                        SELECT scope FROM user_scope_grants
                        WHERE org_id = $1 AND user_id = $2
                          AND (expires_at IS NULL OR expires_at > now())
                        UNION
                        SELECT scope FROM workspace_permission_overrides
                        WHERE workspace_id = $3 AND user_id = $2
                    ) AS all_scopes
                    ORDER BY scope
                    "#,
                    org_id,
                    user_id,
                    ws_id,
                )
                .fetch_all(db.conn())
                .await?
            }
            None => {
                sqlx::query_scalar!(
                    r#"
                    SELECT DISTINCT scope AS "scope!"
                    FROM (
                        SELECT scope FROM org_default_scopes WHERE org_id = $1
                        UNION
                        SELECT scope FROM user_scope_grants
                        WHERE org_id = $1 AND user_id = $2
                          AND (expires_at IS NULL OR expires_at > now())
                    ) AS all_scopes
                    ORDER BY scope
                    "#,
                    org_id,
                    user_id,
                )
                .fetch_all(db.conn())
                .await?
            }
        };

        Ok(
            match Self::active_role_on_conn(db.conn(), org_id, user_id).await? {
                Some(role) => Self::grants_within_role_ceiling(role, scopes),
                None => scopes,
            },
        )
    }

    /// Return whether `user_id` has `required_scope` for `workspace_id`,
    /// preserving the full scope union: role scopes, org defaults, user
    /// grants, and workspace overrides.
    pub async fn user_has_workspace_scope(
        system_pool: &PgPool,
        db: &mut ScopedDb,
        user_id: Uuid,
        org_id: Uuid,
        workspace_id: Uuid,
        required_scope: &str,
    ) -> Result<bool, G5Error> {
        let role: Option<String> = sqlx::query_scalar(
            "SELECT role FROM org_memberships WHERE org_id = $1 AND user_id = $2 AND deactivated_at IS NULL",
        )
        .bind(org_id)
        .bind(user_id)
        .fetch_optional(system_pool)
        .await?;

        let Some(role) = role else {
            return Ok(false);
        };

        role.parse::<OrgRole>().map_err(|_| G5Error::Internal {
            message: format!("Invalid role in org_memberships: {role}"),
        })?;
        if workspace_access_for_role(db.conn(), &role, user_id, org_id, workspace_id).await?
            != WorkspaceAccessResult::Allowed
        {
            return Ok(false);
        }

        Self::user_has_effective_scope(
            system_pool,
            user_id,
            org_id,
            Some(workspace_id),
            required_scope,
        )
        .await
    }

    /// Whether `user_id` holds `required_scope` in `workspace_id` through the
    /// scope union (role scopes, org defaults, user grants and workspace
    /// overrides), for a caller that has already checked workspace access
    /// with its own rule.
    pub async fn user_holds_workspace_scope(
        system_pool: &PgPool,
        user_id: Uuid,
        org_id: Uuid,
        workspace_id: Uuid,
        required_scope: &str,
    ) -> Result<bool, G5Error> {
        Self::user_has_effective_scope(
            system_pool,
            user_id,
            org_id,
            Some(workspace_id),
            required_scope,
        )
        .await
    }

    /// Return whether `user_id` can execute tasks in `workspace_id`, preserving
    /// the worker scheduler's full scope union: role scopes, org defaults,
    /// user grants, and workspace overrides.
    pub async fn user_has_workspace_execute(
        system_pool: &PgPool,
        db: &mut ScopedDb,
        user_id: Uuid,
        org_id: Uuid,
        workspace_id: Uuid,
    ) -> Result<bool, G5Error> {
        Self::user_has_workspace_scope(
            system_pool,
            db,
            user_id,
            org_id,
            workspace_id,
            "tasks:execute",
        )
        .await
    }

    pub async fn user_has_workspace_execute_on_conn(
        conn: &mut PgConnection,
        user_id: Uuid,
        org_id: Uuid,
        workspace_id: Uuid,
    ) -> Result<bool, G5Error> {
        let role: Option<String> = sqlx::query_scalar(
            "SELECT role FROM org_memberships WHERE org_id = $1 AND user_id = $2 AND deactivated_at IS NULL",
        )
        .bind(org_id)
        .bind(user_id)
        .fetch_optional(&mut *conn)
        .await?;

        let Some(role) = role else {
            return Ok(false);
        };

        role.parse::<OrgRole>().map_err(|_| G5Error::Internal {
            message: format!("Invalid role in org_memberships: {role}"),
        })?;
        if workspace_access_for_role(&mut *conn, &role, user_id, org_id, workspace_id).await?
            != WorkspaceAccessResult::Allowed
        {
            return Ok(false);
        }

        Self::user_has_effective_scope_on_conn(
            conn,
            user_id,
            org_id,
            Some(workspace_id),
            "tasks:execute",
        )
        .await
    }

    /// Return whether `user_id` holds `required_scope` at org level in
    /// `org_id` — role scopes, org defaults, and non-expired user grants —
    /// read from the privileged system pool against current membership state
    /// (role_based_access_control D9). Used for org-scoped resource authority
    /// such as org-scoped provider accounts (`provider_account_service` D3).
    pub async fn user_has_org_scope(
        system_pool: &PgPool,
        user_id: Uuid,
        org_id: Uuid,
        required_scope: &str,
    ) -> Result<bool, G5Error> {
        Self::user_has_effective_scope(system_pool, user_id, org_id, None, required_scope).await
    }

    async fn user_has_effective_scope(
        system_pool: &PgPool,
        user_id: Uuid,
        org_id: Uuid,
        workspace_id: Option<Uuid>,
        required_scope: &str,
    ) -> Result<bool, G5Error> {
        let role: Option<String> = sqlx::query_scalar(
            "SELECT role FROM org_memberships WHERE org_id = $1 AND user_id = $2 AND deactivated_at IS NULL",
        )
        .bind(org_id)
        .bind(user_id)
        .fetch_optional(system_pool)
        .await?;
        let Some(role) = role else {
            return Ok(false);
        };

        let org_role = role.parse::<OrgRole>().map_err(|_| G5Error::Internal {
            message: format!("Invalid role in org_memberships: {role}"),
        })?;
        let mut scopes: BTreeSet<String> = Self::get_role_scopes(org_role).into_iter().collect();

        let org_default_scopes: Vec<String> =
            sqlx::query_scalar("SELECT scope FROM org_default_scopes WHERE org_id = $1")
                .bind(org_id)
                .fetch_all(system_pool)
                .await?;
        scopes.extend(Self::grants_within_role_ceiling(
            org_role,
            org_default_scopes,
        ));

        let user_scope_grants: Vec<String> = sqlx::query_scalar(
            r#"
            SELECT scope
            FROM user_scope_grants
            WHERE org_id = $1 AND user_id = $2
              AND (expires_at IS NULL OR expires_at > now())
            "#,
        )
        .bind(org_id)
        .bind(user_id)
        .fetch_all(system_pool)
        .await?;
        scopes.extend(Self::grants_within_role_ceiling(
            org_role,
            user_scope_grants,
        ));

        if let Some(workspace_id) = workspace_id {
            let workspace_overrides: Vec<String> = sqlx::query_scalar(
                "SELECT scope FROM workspace_permission_overrides WHERE workspace_id = $1 AND user_id = $2",
            )
            .bind(workspace_id)
            .bind(user_id)
            .fetch_all(system_pool)
            .await?;
            scopes.extend(Self::grants_within_role_ceiling(
                org_role,
                workspace_overrides,
            ));
        }

        Ok(scopes.contains(required_scope))
    }

    /// Resolve and hold the current actor's authority in the transaction that
    /// consumes it. The definer function exposes only this verified session's
    /// actor/org, so tenant connections need no direct access to auth tables.
    pub async fn require_user_scopes_on_conn(
        conn: &mut PgConnection,
        user_id: Uuid,
        org_id: Uuid,
        workspace_id: Option<Uuid>,
        required_scopes: &[&str],
    ) -> Result<(), G5Error> {
        if required_scopes.is_empty() {
            return Self::require_active_member_on_conn(conn, user_id, org_id, false)
                .await
                .map(|_| ());
        }
        let authority = sqlx::query_as::<_, (String, Vec<String>)>(
            "SELECT role, granted_scopes FROM authz.lock_user_scope_authority($1, $2, $3, $4)",
        )
        .bind(user_id)
        .bind(org_id)
        .bind(workspace_id)
        .bind(required_scopes)
        .fetch_optional(conn)
        .await?;
        let mut effective = BTreeSet::new();
        if let Some((role, granted_scopes)) = authority {
            let role = role.parse::<OrgRole>().map_err(|_| G5Error::Internal {
                message: format!("Invalid role in org_memberships: {role}"),
            })?;
            effective.extend(
                crate::services::identity::auth_service::AuthService::default_scopes_for_role(role)
                    .into_iter()
                    .map(|scope| scope.as_str().to_owned()),
            );
            effective.extend(Self::grants_within_role_ceiling(
                role,
                granted_scopes
                    .into_iter()
                    .filter(|scope| scope.parse::<g5_auth::Scope>().is_ok()),
            ));
        }
        let missing_scopes: Vec<String> = required_scopes
            .iter()
            .filter(|scope| !effective.contains(**scope))
            .map(|scope| (*scope).to_owned())
            .collect();
        if missing_scopes.is_empty() {
            Ok(())
        } else {
            Err(G5Error::Forbidden { missing_scopes })
        }
    }

    /// The service boundary uses the same locked authority as route admission,
    /// including revocable org defaults and individual grants.
    pub async fn require_org_admin_on_conn(
        conn: &mut PgConnection,
        user_id: Uuid,
        org_id: Uuid,
    ) -> Result<(), G5Error> {
        Self::require_user_scopes_on_conn(conn, user_id, org_id, None, &["org:admin"]).await
    }

    async fn user_has_effective_scope_on_conn(
        conn: &mut PgConnection,
        user_id: Uuid,
        org_id: Uuid,
        workspace_id: Option<Uuid>,
        required_scope: &str,
    ) -> Result<bool, G5Error> {
        let role: Option<String> = sqlx::query_scalar(
            "SELECT role FROM org_memberships WHERE org_id = $1 AND user_id = $2 AND deactivated_at IS NULL",
        )
        .bind(org_id)
        .bind(user_id)
        .fetch_optional(&mut *conn)
        .await?;
        let Some(role) = role else {
            return Ok(false);
        };

        let org_role = role.parse::<OrgRole>().map_err(|_| G5Error::Internal {
            message: format!("Invalid role in org_memberships: {role}"),
        })?;
        let mut scopes: BTreeSet<String> = Self::get_role_scopes(org_role).into_iter().collect();

        let org_default_scopes: Vec<String> =
            sqlx::query_scalar("SELECT scope FROM org_default_scopes WHERE org_id = $1")
                .bind(org_id)
                .fetch_all(&mut *conn)
                .await?;
        scopes.extend(Self::grants_within_role_ceiling(
            org_role,
            org_default_scopes,
        ));

        let user_scope_grants: Vec<String> = sqlx::query_scalar(
            r#"
            SELECT scope
            FROM user_scope_grants
            WHERE org_id = $1 AND user_id = $2
              AND (expires_at IS NULL OR expires_at > now())
            "#,
        )
        .bind(org_id)
        .bind(user_id)
        .fetch_all(&mut *conn)
        .await?;
        scopes.extend(Self::grants_within_role_ceiling(
            org_role,
            user_scope_grants,
        ));

        if let Some(workspace_id) = workspace_id {
            let workspace_overrides: Vec<String> = sqlx::query_scalar(
                "SELECT scope FROM workspace_permission_overrides WHERE workspace_id = $1 AND user_id = $2",
            )
            .bind(workspace_id)
            .bind(user_id)
            .fetch_all(&mut *conn)
            .await?;
            scopes.extend(Self::grants_within_role_ceiling(
                org_role,
                workspace_overrides,
            ));
        }

        Ok(scopes.contains(required_scope))
    }

    /// Like [`resolve_effective_scopes`] but returns the breakdown of where
    /// each scope comes from (org defaults, user grants, workspace overrides).
    ///
    /// Used by `GET /api/v1/auth/me/scopes`.
    pub async fn resolve_effective_scopes_detailed(
        &self,
        db: &mut ScopedDb,
        user_id: Uuid,
        org_id: Uuid,
        workspace_id: Option<Uuid>,
    ) -> Result<EffectiveScopesDTO, AppError> {
        if let Some(ws_id) = workspace_id {
            self.ensure_workspace_in_org(db, org_id, ws_id).await?;
        }

        // Query org defaults
        let org_defaults: Vec<String> = sqlx::query_scalar!(
            r#"SELECT scope AS "scope!" FROM org_default_scopes WHERE org_id = $1 ORDER BY scope"#,
            org_id,
        )
        .fetch_all(db.conn())
        .await?;

        // Query user grants (non-expired)
        let user_grants: Vec<String> = sqlx::query_scalar!(
            r#"
            SELECT scope AS "scope!"
            FROM user_scope_grants
            WHERE org_id = $1 AND user_id = $2
              AND (expires_at IS NULL OR expires_at > now())
            ORDER BY scope
            "#,
            org_id,
            user_id,
        )
        .fetch_all(db.conn())
        .await?;

        // Query workspace overrides (if workspace context)
        let workspace_overrides: Vec<String> = if let Some(ws_id) = workspace_id {
            sqlx::query_scalar!(
                r#"
                SELECT scope AS "scope!"
                FROM workspace_permission_overrides
                WHERE workspace_id = $1 AND user_id = $2
                ORDER BY scope
                "#,
                ws_id,
                user_id,
            )
            .fetch_all(db.conn())
            .await?
        } else {
            Vec::new()
        };

        // Compute union (sorted). The components list what is configured;
        // the union is what the membership may use under its role ceiling.
        let mut all_scopes = BTreeSet::new();
        for s in &org_defaults {
            all_scopes.insert(s.clone());
        }
        for s in &user_grants {
            all_scopes.insert(s.clone());
        }
        for s in &workspace_overrides {
            all_scopes.insert(s.clone());
        }
        let scopes = match Self::active_role_on_conn(db.conn(), org_id, user_id).await? {
            Some(role) => Self::grants_within_role_ceiling(role, all_scopes),
            None => all_scopes.into_iter().collect(),
        };

        Ok(EffectiveScopesDTO {
            scopes,
            org_defaults,
            user_grants,
            workspace_overrides,
        })
    }

    // -----------------------------------------------------------------------
    // Scope enforcement
    // -----------------------------------------------------------------------

    /// Check whether a user has all required scopes.
    ///
    /// Returns a [`ScopeCheckResult`] indicating whether access is allowed
    /// and which scopes are missing (if any).
    ///
    /// **Note:** This method does NOT return an error when scopes are missing —
    /// it returns a result indicating what's missing. The caller decides
    /// whether to return a 403.
    pub async fn check_scopes(
        &self,
        db: &mut ScopedDb,
        user_id: Uuid,
        org_id: Uuid,
        required_scopes: &[String],
        workspace_id: Option<Uuid>,
    ) -> Result<ScopeCheckResult, AppError> {
        let effective = self
            .resolve_effective_scopes(db, user_id, org_id, workspace_id)
            .await?;

        let effective_set: BTreeSet<&str> = effective.iter().map(|s| s.as_str()).collect();

        let missing: Vec<String> = required_scopes
            .iter()
            .filter(|s| !effective_set.contains(s.as_str()))
            .cloned()
            .collect();

        Ok(ScopeCheckResult {
            allowed: missing.is_empty(),
            missing,
        })
    }

    // -----------------------------------------------------------------------
    // Org default scopes
    // -----------------------------------------------------------------------

    /// List the default scopes for an organization.
    ///
    /// These scopes are automatically granted to every member of the org.
    pub async fn get_org_default_scopes(
        &self,
        db: &mut ScopedDb,
        org_id: Uuid,
    ) -> Result<Vec<String>, AppError> {
        let scopes = sqlx::query_scalar!(
            r#"SELECT scope AS "scope!" FROM org_default_scopes WHERE org_id = $1 ORDER BY scope"#,
            org_id,
        )
        .fetch_all(db.conn())
        .await?;

        Ok(scopes)
    }

    /// Replace the org's default scopes with the provided set.
    ///
    /// This is a **full replacement** — not additive. All existing default
    /// scopes are removed and replaced with the provided set.
    ///
    /// # Validation
    ///
    /// All scope strings are validated against the `oauth_scopes` registry
    /// table. Returns `AppError::ValidationError` if any are unknown.
    ///
    /// # Transaction
    ///
    /// The DELETE + INSERT operations execute in a single transaction.
    pub async fn set_org_default_scopes(
        &self,
        db: &mut ScopedDb,
        org_id: Uuid,
        scopes: Vec<String>,
        set_by: Uuid,
    ) -> Result<Vec<String>, AppError> {
        // Validate scopes
        self.validate_scopes(db, &scopes).await?;

        // Delete existing
        sqlx::query!("DELETE FROM org_default_scopes WHERE org_id = $1", org_id)
            .execute(db.conn())
            .await?;

        // Insert new
        for scope in &scopes {
            sqlx::query!(
                "INSERT INTO org_default_scopes (org_id, scope) VALUES ($1, $2)",
                org_id,
                scope,
            )
            .execute(db.conn())
            .await?;
        }

        info!(
            org_id = %org_id,
            set_by = %set_by,
            count = scopes.len(),
            "Updated org default scopes"
        );

        let mut sorted = scopes;
        sorted.sort();
        Ok(sorted)
    }

    // -----------------------------------------------------------------------
    // User scope grants
    // -----------------------------------------------------------------------

    /// List a user's additional scope grants within an org.
    ///
    /// Returns full grant details including expiration and who granted them.
    pub async fn get_user_scopes(
        &self,
        db: &mut ScopedDb,
        org_id: Uuid,
        user_id: Uuid,
    ) -> Result<Vec<UserScopeGrantDTO>, AppError> {
        let rows = sqlx::query_as!(
            UserScopeGrantRow,
            r#"
            SELECT
                id,
                org_id,
                user_id,
                scope,
                granted_by,
                granted_at,
                expires_at
            FROM user_scope_grants
            WHERE org_id = $1 AND user_id = $2
            ORDER BY scope
            "#,
            org_id,
            user_id,
        )
        .fetch_all(db.conn())
        .await?;

        Ok(rows.into_iter().map(|r| r.into_dto()).collect())
    }

    /// Replace a user's scope grants within an org.
    ///
    /// This is a **full replacement** — not additive.
    ///
    /// # Validation
    ///
    /// All scope strings are validated against the `oauth_scopes` registry.
    ///
    /// # Transaction
    ///
    /// The DELETE + INSERT operations execute in a single transaction.
    pub async fn set_user_scopes(
        &self,
        db: &mut ScopedDb,
        org_id: Uuid,
        user_id: Uuid,
        scopes: Vec<String>,
        expires_at: Option<DateTime<Utc>>,
        granted_by: Uuid,
    ) -> Result<Vec<UserScopeGrantDTO>, AppError> {
        // Validate scopes
        self.validate_scopes(db, &scopes).await?;

        // Delete existing
        sqlx::query!(
            "DELETE FROM user_scope_grants WHERE org_id = $1 AND user_id = $2",
            org_id,
            user_id,
        )
        .execute(db.conn())
        .await?;

        // Insert new grants
        let mut results = Vec::with_capacity(scopes.len());
        for scope in &scopes {
            let row = sqlx::query_as!(
                UserScopeGrantRow,
                r#"
                INSERT INTO user_scope_grants (org_id, user_id, scope, granted_by, granted_at, expires_at)
                VALUES ($1, $2, $3, $4, now(), $5)
                RETURNING id, org_id, user_id, scope, granted_by, granted_at, expires_at
                "#,
                org_id,
                user_id,
                scope,
                granted_by as Uuid,
                expires_at,
            )
            .fetch_one(db.conn())
            .await?;
            results.push(row.into_dto());
        }

        info!(
            org_id = %org_id,
            user_id = %user_id,
            granted_by = %granted_by,
            count = scopes.len(),
            "Updated user scope grants"
        );

        Ok(results)
    }

    // -----------------------------------------------------------------------
    // Workspace permission overrides
    // -----------------------------------------------------------------------

    /// List all permission overrides for a workspace.
    pub async fn get_workspace_overrides(
        &self,
        db: &mut ScopedDb,
        org_id: Uuid,
        workspace_id: Uuid,
    ) -> Result<Vec<WorkspaceOverrideDTO>, AppError> {
        self.ensure_workspace_in_org(db, org_id, workspace_id)
            .await?;

        let rows = sqlx::query_as!(
            WorkspaceOverrideRow,
            r#"
            SELECT
                id,
                workspace_id,
                user_id,
                scope,
                granted_by,
                granted_at
            FROM workspace_permission_overrides
            WHERE workspace_id = $1
            ORDER BY user_id, scope
            "#,
            workspace_id,
        )
        .fetch_all(db.conn())
        .await?;

        Ok(rows.into_iter().map(|r| r.into_dto()).collect())
    }

    /// Set per-workspace permission overrides for a specific user.
    ///
    /// Replaces all existing overrides for the `(workspace_id, user_id)` pair.
    ///
    /// # Validation
    ///
    /// All scope strings are validated against the `oauth_scopes` registry.
    ///
    /// # Transaction
    ///
    /// The DELETE + INSERT operations execute in a single transaction.
    pub async fn set_workspace_override(
        &self,
        db: &mut ScopedDb,
        org_id: Uuid,
        workspace_id: Uuid,
        user_id: Uuid,
        scopes: Vec<String>,
        granted_by: Uuid,
    ) -> Result<Vec<WorkspaceOverrideDTO>, AppError> {
        self.ensure_workspace_in_org(db, org_id, workspace_id)
            .await?;

        // Validate scopes
        self.validate_scopes(db, &scopes).await?;

        // Delete existing
        sqlx::query!(
            "DELETE FROM workspace_permission_overrides WHERE workspace_id = $1 AND user_id = $2",
            workspace_id,
            user_id,
        )
        .execute(db.conn())
        .await?;

        // Insert new overrides
        let mut results = Vec::with_capacity(scopes.len());
        for scope in &scopes {
            let row = sqlx::query_as!(
                WorkspaceOverrideRow,
                r#"
                INSERT INTO workspace_permission_overrides (workspace_id, user_id, scope, granted_by, granted_at)
                VALUES ($1, $2, $3, $4, now())
                RETURNING id, workspace_id, user_id, scope, granted_by, granted_at
                "#,
                workspace_id,
                user_id,
                scope,
                granted_by as Uuid,
            )
            .fetch_one(db.conn())
            .await?;
            results.push(row.into_dto());
        }

        info!(
            workspace_id = %workspace_id,
            user_id = %user_id,
            granted_by = %granted_by,
            count = scopes.len(),
            "Updated workspace permission overrides"
        );

        Ok(results)
    }

    /// Remove all per-workspace permission overrides for a specific user.
    ///
    /// Reverts the user to org-level permissions for this workspace.
    /// This operation is **idempotent** — succeeds even if no overrides exist.
    pub async fn remove_workspace_override(
        &self,
        db: &mut ScopedDb,
        org_id: Uuid,
        workspace_id: Uuid,
        user_id: Uuid,
    ) -> Result<(), AppError> {
        self.ensure_workspace_in_org(db, org_id, workspace_id)
            .await?;

        sqlx::query!(
            "DELETE FROM workspace_permission_overrides WHERE workspace_id = $1 AND user_id = $2",
            workspace_id,
            user_id,
        )
        .execute(db.conn())
        .await?;

        info!(
            workspace_id = %workspace_id,
            user_id = %user_id,
            "Removed workspace permission overrides"
        );

        Ok(())
    }

    // -----------------------------------------------------------------------
    // Scope validation
    // -----------------------------------------------------------------------

    /// Validate that all provided scope strings exist in the `oauth_scopes`
    /// registry table.
    ///
    /// Returns `Ok(())` if all scopes are valid, or `AppError::ValidationError`
    /// with details listing the invalid scopes.
    pub async fn validate_scopes(
        &self,
        db: &mut ScopedDb,
        scopes: &[String],
    ) -> Result<(), AppError> {
        if scopes.is_empty() {
            return Ok(());
        }

        let found: Vec<String> = sqlx::query_scalar!(
            r#"SELECT scope AS "scope!" FROM oauth_scopes WHERE scope = ANY($1)"#,
            scopes,
        )
        .fetch_all(db.conn())
        .await?;

        let found_set: BTreeSet<&str> = found.iter().map(|s| s.as_str()).collect();
        let invalid: Vec<String> = scopes
            .iter()
            .filter(|s| !found_set.contains(s.as_str()))
            .cloned()
            .collect();

        if !invalid.is_empty() {
            return Err(AppError::from(
                g5_contracts::error::G5Error::ValidationError {
                    details: serde_json::json!({ "invalid_scopes": invalid }),
                },
            ));
        }

        Ok(())
    }

    async fn ensure_workspace_in_org(
        &self,
        db: &mut ScopedDb,
        org_id: Uuid,
        workspace_id: Uuid,
    ) -> Result<(), AppError> {
        let exists = sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(
                SELECT 1
                FROM workspaces
                WHERE id = $1
                  AND org_id = $2
                  AND deleted_at IS NULL
            )",
        )
        .bind(workspace_id)
        .bind(org_id)
        .fetch_one(db.conn())
        .await?;

        if exists {
            return Ok(());
        }

        Err(AppError::from(G5Error::NotFound {
            resource: format!("workspace {workspace_id}"),
        }))
    }

    // -----------------------------------------------------------------------
    // Role → scope mapping (pure function, no DB)
    // -----------------------------------------------------------------------

    /// Returns the predefined scope set for a convenience role.
    ///
    /// This is a **pure function** (no database access) that maps role names
    /// to scope sets per the predefined role→scope table from
    /// API_AND_STORAGE.md §2.12.
    ///
    /// # Role→Scope mappings
    ///
    /// | Role       | Scopes                                                        |
    /// |------------|---------------------------------------------------------------|
    /// | `viewer`   | `workspaces:read`, `sessions:read`, `tasks:read`               |
    /// | `member`   | viewer + `workspaces:write`, `sessions:write`                 |
    /// | `admin`    | member + `tasks:execute`, `workers:read`, `workers:admin`, `org:admin`, `billing:admin` |
    ///
    /// The hierarchy is: `Viewer < Member < Admin`, matching the
    /// `org_memberships.role` CHECK constraint.
    #[must_use]
    pub fn get_role_scopes(role: OrgRole) -> Vec<String> {
        let viewer = vec![
            "sessions:read".to_string(),
            "tasks:read".to_string(),
            "workspaces:read".to_string(),
        ];

        let member = {
            let mut s = viewer.clone();
            s.push("sessions:write".to_string());
            s.push("workspaces:write".to_string());
            s.sort();
            s
        };

        let admin = {
            let mut s = member.clone();
            s.push("billing:admin".to_string());
            s.push("tasks:execute".to_string());
            s.push("org:admin".to_string());
            s.push("workers:admin".to_string());
            s.push("workers:read".to_string());
            s.sort();
            s
        };

        match role {
            OrgRole::Viewer => viewer,
            OrgRole::Member => member,
            OrgRole::Admin => admin,
            _ => Vec::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// Internal row types for sqlx query_as!
// ---------------------------------------------------------------------------

/// Internal row type for `user_scope_grants` queries.
///
/// Used with `sqlx::query_as!` for compile-time checked deserialization.
/// Converted to `UserScopeGrantDTO` for API responses.
struct UserScopeGrantRow {
    id: Uuid,
    org_id: Uuid,
    user_id: Uuid,
    scope: String,
    granted_by: Option<Uuid>,
    granted_at: DateTime<Utc>,
    expires_at: Option<DateTime<Utc>>,
}

impl UserScopeGrantRow {
    /// Convert to the API-facing DTO with ISO 8601 timestamp strings.
    fn into_dto(self) -> UserScopeGrantDTO {
        UserScopeGrantDTO {
            id: self.id,
            org_id: self.org_id,
            user_id: self.user_id,
            scope: self.scope,
            granted_by: self.granted_by,
            granted_at: self.granted_at.to_rfc3339(),
            expires_at: self.expires_at.map(|dt| dt.to_rfc3339()),
        }
    }
}

/// Internal row type for `workspace_permission_overrides` queries.
struct WorkspaceOverrideRow {
    id: Uuid,
    workspace_id: Uuid,
    user_id: Uuid,
    scope: String,
    granted_by: Option<Uuid>,
    granted_at: DateTime<Utc>,
}

impl WorkspaceOverrideRow {
    /// Convert to the API-facing DTO with ISO 8601 timestamp strings.
    fn into_dto(self) -> WorkspaceOverrideDTO {
        WorkspaceOverrideDTO {
            id: self.id,
            workspace_id: self.workspace_id,
            user_id: self.user_id,
            scope: self.scope,
            granted_by: self.granted_by,
            granted_at: self.granted_at.to_rfc3339(),
        }
    }
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -- get_role_scopes tests --

    #[test]
    fn test_viewer_scopes() {
        let scopes = PermissionService::get_role_scopes(OrgRole::Viewer);
        assert_eq!(
            scopes,
            vec!["sessions:read", "tasks:read", "workspaces:read"]
        );
    }

    #[test]
    fn test_member_scopes() {
        let scopes = PermissionService::get_role_scopes(OrgRole::Member);
        assert_eq!(
            scopes,
            vec![
                "sessions:read",
                "sessions:write",
                "tasks:read",
                "workspaces:read",
                "workspaces:write",
            ]
        );
    }

    #[test]
    fn test_admin_scopes() {
        let scopes = PermissionService::get_role_scopes(OrgRole::Admin);
        assert_eq!(
            scopes,
            vec![
                "billing:admin",
                "org:admin",
                "sessions:read",
                "sessions:write",
                "tasks:execute",
                "tasks:read",
                "workers:admin",
                "workers:read",
                "workspaces:read",
                "workspaces:write",
            ]
        );
    }

    #[test]
    fn test_role_from_str_case_insensitive() {
        // Case variations via FromStr should all resolve to the same OrgRole
        use std::str::FromStr;

        assert_eq!(OrgRole::from_str("admin").unwrap(), OrgRole::Admin);
        assert_eq!(OrgRole::from_str("ADMIN").unwrap(), OrgRole::Admin);
        assert_eq!(OrgRole::from_str("Admin").unwrap(), OrgRole::Admin);

        assert!(OrgRole::from_str("owner").is_err());
        assert!(OrgRole::from_str("OWNER").is_err());

        assert_eq!(OrgRole::from_str("viewer").unwrap(), OrgRole::Viewer);
        assert_eq!(OrgRole::from_str("Viewer").unwrap(), OrgRole::Viewer);

        // Invalid roles return Err
        assert!(OrgRole::from_str("superuser").is_err());
        assert!(OrgRole::from_str("").is_err());
    }

    #[test]
    fn test_role_hierarchy_is_cumulative() {
        let viewer = PermissionService::get_role_scopes(OrgRole::Viewer);
        let member = PermissionService::get_role_scopes(OrgRole::Member);
        let admin = PermissionService::get_role_scopes(OrgRole::Admin);

        // Each role is a superset of the previous
        for scope in &viewer {
            assert!(
                member.contains(scope),
                "member should contain viewer scope: {scope}"
            );
        }
        for scope in &member {
            assert!(
                admin.contains(scope),
                "admin should contain member scope: {scope}"
            );
        }
    }

    #[test]
    fn test_all_role_scopes_are_sorted() {
        for role in [OrgRole::Viewer, OrgRole::Member, OrgRole::Admin] {
            let scopes = PermissionService::get_role_scopes(role);
            let mut sorted = scopes.clone();
            sorted.sort();
            assert_eq!(scopes, sorted, "Scopes for role should be sorted");
        }
    }

    // -- ScopeCheckResult tests --

    #[test]
    fn test_scope_check_result_serialization() {
        let result = ScopeCheckResult {
            allowed: false,
            missing: vec!["org:admin".to_string()],
        };
        let json = serde_json::to_value(&result).unwrap();
        assert_eq!(json["allowed"], false);
        assert_eq!(json["missing"], serde_json::json!(["org:admin"]));
    }

    #[test]
    fn test_scope_check_result_allowed_serialization() {
        let result = ScopeCheckResult {
            allowed: true,
            missing: vec![],
        };
        let json = serde_json::to_value(&result).unwrap();
        assert_eq!(json["allowed"], true);
        assert_eq!(json["missing"], serde_json::json!([]));
    }

    // -- DTO serialization tests --

    #[test]
    fn test_user_scope_grant_dto_camel_case() {
        let dto = UserScopeGrantDTO {
            id: Uuid::nil(),
            org_id: Uuid::nil(),
            user_id: Uuid::nil(),
            scope: "workspaces:read".to_string(),
            granted_by: None,
            granted_at: "2024-01-01T00:00:00Z".to_string(),
            expires_at: None,
        };
        let json = serde_json::to_value(&dto).unwrap();
        // Verify camelCase keys
        assert!(json.get("orgId").is_some());
        assert!(json.get("userId").is_some());
        assert!(json.get("grantedBy").is_some());
        assert!(json.get("grantedAt").is_some());
        assert!(json.get("expiresAt").is_some());
        // Should NOT have snake_case keys
        assert!(json.get("org_id").is_none());
        assert!(json.get("user_id").is_none());
    }

    #[test]
    fn test_workspace_override_dto_camel_case() {
        let dto = WorkspaceOverrideDTO {
            id: Uuid::nil(),
            workspace_id: Uuid::nil(),
            user_id: Uuid::nil(),
            scope: "tasks:execute".to_string(),
            granted_by: Some(Uuid::nil()),
            granted_at: "2024-01-01T00:00:00Z".to_string(),
        };
        let json = serde_json::to_value(&dto).unwrap();
        assert!(json.get("workspaceId").is_some());
        assert!(json.get("userId").is_some());
        assert!(json.get("grantedBy").is_some());
        assert!(json.get("grantedAt").is_some());
    }

    #[test]
    fn test_effective_scopes_dto_camel_case() {
        let dto = EffectiveScopesDTO {
            scopes: vec!["workspaces:read".to_string()],
            org_defaults: vec!["workspaces:read".to_string()],
            user_grants: vec![],
            workspace_overrides: vec![],
        };
        let json = serde_json::to_value(&dto).unwrap();
        assert!(json.get("orgDefaults").is_some());
        assert!(json.get("userGrants").is_some());
        assert!(json.get("workspaceOverrides").is_some());
    }

    // -- Request DTO deserialization tests --

    #[test]
    fn test_set_user_scopes_request_deserialization() {
        let json = r#"{"scopes": ["workspaces:read", "tasks:execute"], "expiresAt": "2025-01-01T00:00:00Z"}"#;
        let req: SetUserScopesRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.scopes.len(), 2);
        assert!(req.expires_at.is_some());
    }

    #[test]
    fn test_set_user_scopes_request_without_expiry() {
        let json = r#"{"scopes": ["workspaces:read"]}"#;
        let req: SetUserScopesRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.scopes.len(), 1);
        assert!(req.expires_at.is_none());
    }

    #[test]
    fn test_set_workspace_override_request_deserialization() {
        let json = r#"{"scopes": ["tasks:execute", "workers:read"]}"#;
        let req: SetWorkspaceOverrideRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.scopes.len(), 2);
    }

    #[test]
    fn test_scope_check_request_deserialization() {
        let json =
            r#"{"scopes": ["org:admin"], "workspaceId": "00000000-0000-0000-0000-000000000001"}"#;
        let req: ScopeCheckRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.scopes, vec!["org:admin"]);
        assert!(req.workspace_id.is_some());
    }

    #[test]
    fn test_scope_check_request_without_workspace() {
        let json = r#"{"scopes": ["org:admin"]}"#;
        let req: ScopeCheckRequest = serde_json::from_str(json).unwrap();
        assert!(req.workspace_id.is_none());
    }
}
