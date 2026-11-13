//! User service — user profile management operations.
//!
//! Implements the user-facing profile endpoints and the OAuth upsert operation
//! used by `AuthService` during login callbacks. Per API_AND_STORAGE.md §3.11:
//!
//! - `GET /api/v1/users/me` → [`UserService::get_current_user`]
//! - `GET /api/v1/users/:id` → [`UserService::get_user`]
//! - `PATCH /api/v1/users/me` → [`UserService::update_user`]
//! - OAuth user upsert → [`UserService::upsert_user_from_oauth`]
//!
//! This is a lightweight service with no event emission and no
//! complex business logic. It reads from and writes to the `users`,
//! `org_memberships`, and `user_scm_connections` tables.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Deserializer, Serialize};
use sqlx::PgPool;
use std::collections::HashMap;
use tracing::{info, warn};
use uuid::Uuid;

/// Serde helper: wraps deserialized `T` in `Some(T)` so that
/// `#[serde(default, deserialize_with = "deserialize_some")]`
/// on `Option<Option<V>>` distinguishes absent → `None` from
/// explicit `null` → `Some(None)`.
fn deserialize_some<'de, T, D>(deserializer: D) -> Result<Option<T>, D::Error>
where
    T: Deserialize<'de>,
    D: Deserializer<'de>,
{
    T::deserialize(deserializer).map(Some)
}

use crate::errors::{AppError, G5Error};
use crate::models::User;

// ---------------------------------------------------------------------------
// DTO Types — Public API Surface
// ---------------------------------------------------------------------------

/// Full profile for the authenticated user (`GET /api/v1/users/me` response).
///
/// Aggregates data from `users`, `org_memberships` (joined with `orgs`), and
/// `user_scm_connections` tables into a single response.
/// The finished value of `onboarding_step` (users g5.user.onboarding): the
/// tour has seven steps, 0 to 6, and 7 means the person finished it.
pub const ONBOARDING_STEP_COUNT: i16 = 7;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CurrentUserDTO {
    pub id: Uuid,
    pub email: String,
    pub display_name: String,
    pub avatar_url: Option<String>,
    pub oauth_provider: Option<String>,
    /// The org ID from the current JWT session context.
    pub active_org_id: Option<Uuid>,
    pub orgs: Vec<OrgMembershipSummary>,
    pub scm_connections: Vec<ScmConnectionSummary>,
    /// ISO 8601 formatted creation timestamp.
    pub created_at: String,
    /// Highest onboarding-tour step reached (users g5.user.onboarding):
    /// null until the welcome card is shown, 7 when finished.
    pub onboarding_step: Option<i16>,
    /// Whether the person skipped the tour at `onboarding_step`.
    pub onboarding_skipped: bool,
    /// Account-wide "Approval requests" email preference, for both approval emails
    /// (users g5.user.notifications). Default true for existing and new users.
    pub email_on_approval_request: bool,
    /// Personal default for stopping owned workspaces after a successful merge.
    pub stop_vm_after_merge_default: bool,
}

/// Org membership summary included in [`CurrentUserDTO`].
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OrgMembershipSummary {
    pub org_id: Uuid,
    pub org_name: String,
    pub org_slug: String,
    /// One of: `admin`, `member`, `viewer`.
    pub role: String,
    /// `tenant`, or `operator` for the platform-operator organization
    /// (`g5.organization.platform-operator.identity`).
    pub org_kind: String,
}

/// SCM connection summary included in [`CurrentUserDTO`].
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScmConnectionSummary {
    /// One of: `github`, `gitlab`, `bitbucket`.
    pub provider: String,
    pub provider_username: String,
    pub scopes: Vec<String>,
    /// Whether a GitHub App installation is associated with this connection.
    /// Derived from `installation_id IS NOT NULL`.
    pub has_installation: bool,
    /// ISO 8601 formatted connection timestamp.
    pub connected_at: String,
}

/// Minimal public profile (`GET /api/v1/users/:id` response).
///
/// Does NOT include email, OAuth provider, org memberships, or SCM connections
/// — those are private to the authenticated user.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PublicUserDTO {
    pub id: Uuid,
    pub display_name: String,
    pub avatar_url: Option<String>,
}

/// Request body for `PATCH /api/v1/users/me`.
///
/// Uses `Option<Option<String>>` for `avatar_url` to distinguish between:
/// - `None` (outer) — field not provided, don't change
/// - `Some(None)` — explicitly set to null (clear avatar)
/// - `Some(Some(url))` — set to a new URL
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateUserRequest {
    pub display_name: Option<String>,
    #[serde(default, deserialize_with = "deserialize_some")]
    pub avatar_url: Option<Option<String>>,
    /// Onboarding-tour step reached (users g5.user.onboarding). The stored
    /// step only ever rises: a lower value leaves it unchanged.
    pub onboarding_step: Option<i16>,
    /// Whether the person skipped the tour; set or cleared as sent.
    pub onboarding_skipped: Option<bool>,
    /// Account-wide "Approval requests" email preference, for both approval emails
    /// (users g5.user.notifications). Absent leaves the stored value unchanged.
    pub email_on_approval_request: Option<bool>,
    /// Absent leaves the personal merge-stop default unchanged.
    pub stop_vm_after_merge_default: Option<bool>,
}

/// OAuth user data extracted from provider callback.
///
/// Used by [`AuthService::handle_callback`] to create or update a user from
/// OAuth provider data.
#[derive(Debug, Clone)]
pub struct OAuthUserData {
    pub email: String,
    pub display_name: String,
    pub avatar_url: Option<String>,
    pub oauth_provider: String,
    pub oauth_id: String,
}

// ---------------------------------------------------------------------------
// Internal Query Result Types
// ---------------------------------------------------------------------------

/// Row from the org membership join query.
#[derive(Debug, Clone, sqlx::FromRow)]
struct OrgMembershipRow {
    pub org_id: Uuid,
    pub org_name: String,
    pub org_slug: String,
    pub role: String,
    pub org_kind: String,
}

/// Row from the SCM connections query.
#[derive(Debug, Clone, sqlx::FromRow)]
struct ScmConnectionRow {
    pub provider: String,
    pub provider_username: Option<String>,
    pub scopes: Vec<String>,
    pub installation_id: Option<i64>,
    pub created_at: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// UserService
// ---------------------------------------------------------------------------

/// User profile management service.
///
/// The simplest service in the layer — only needs a database connection pool.
/// No HTTP client, no encryption keys.
#[derive(Default)]
/// One user's attribution summary (`load_summaries`).
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct UserSummary {
    pub id: Uuid,
    pub display_name: String,
    pub avatar_url: Option<String>,
}

pub struct UserService;

impl UserService {
    /// Resolve only the merge-stop preference for owners of already-authorized
    /// workspace records. Global identity reads stay on the system pool;
    /// tenant connections have no access to personal profiles.
    pub(crate) async fn merge_stop_defaults(
        system_pool: &PgPool,
        owners: &[Uuid],
    ) -> Result<HashMap<Uuid, bool>, G5Error> {
        if owners.is_empty() {
            return Ok(HashMap::new());
        }
        let rows: Vec<(Uuid, bool)> =
            sqlx::query_as("SELECT id, stop_vm_after_merge_default FROM users WHERE id = ANY($1)")
                .bind(owners)
                .fetch_all(system_pool)
                .await?;
        Ok(rows.into_iter().collect())
    }

    /// Returns the full profile for the authenticated user.
    ///
    /// Backing method for `GET /api/v1/users/me`. Aggregates data from three
    /// tables: `users`, `org_memberships` (joined with `orgs`), and
    /// `user_scm_connections`.
    ///
    /// # Errors
    ///
    /// - `AppError::NotFound` if the user does not exist.
    /// - `AppError::Internal` on database errors.
    pub async fn get_current_user(
        &self,
        system_pool: &PgPool,
        user_id: Uuid,
        active_org_id: Option<Uuid>,
    ) -> Result<CurrentUserDTO, AppError> {
        // Step 1: Get user
        let user = sqlx::query_as!(
            User,
            r#"
            SELECT id, email, display_name, avatar_url, oauth_provider, oauth_id, created_at,
                   onboarding_step, onboarding_skipped
            FROM users
            WHERE id = $1
            "#,
            user_id
        )
        .fetch_optional(system_pool)
        .await?;

        let user = match user {
            Some(u) => u,
            None => {
                warn!(user_id = %user_id, "User not found");
                return Err(G5Error::NotFound {
                    resource: format!("user {}", user_id),
                }
                .into());
            }
        };

        let (email_on_approval_request, stop_vm_after_merge_default): (bool, bool) =
            sqlx::query_as(
                "SELECT email_on_approval_request, stop_vm_after_merge_default
                 FROM users WHERE id = $1",
            )
            .bind(user_id)
            .fetch_one(system_pool)
            .await?;

        // Step 2: Get org memberships
        let org_rows = sqlx::query_as!(
            OrgMembershipRow,
            r#"
            SELECT o.id AS org_id, o.name AS org_name, o.slug AS org_slug, om.role,
                   o.kind AS org_kind
            FROM org_memberships om
            JOIN orgs o ON o.id = om.org_id
            WHERE om.user_id = $1 AND om.deactivated_at IS NULL
            ORDER BY om.created_at ASC
            "#,
            user_id
        )
        .fetch_all(system_pool)
        .await?;

        // Step 3: Get SCM connections
        let scm_rows = sqlx::query_as!(
            ScmConnectionRow,
            r#"
            SELECT provider, provider_username, scopes, installation_id, created_at
            FROM user_scm_connections
            WHERE user_id = $1
            ORDER BY created_at ASC
            "#,
            user_id
        )
        .fetch_all(system_pool)
        .await?;

        // Assemble the DTO
        let orgs = org_rows
            .into_iter()
            .map(|row| OrgMembershipSummary {
                org_id: row.org_id,
                org_name: row.org_name,
                org_slug: row.org_slug,
                role: row.role,
                org_kind: row.org_kind,
            })
            .collect();

        let scm_connections = scm_rows
            .into_iter()
            .map(|row| ScmConnectionSummary {
                provider: row.provider,
                provider_username: row.provider_username.unwrap_or_default(),
                scopes: row.scopes,
                has_installation: row.installation_id.is_some(),
                connected_at: row.created_at.to_rfc3339(),
            })
            .collect();

        Ok(CurrentUserDTO {
            id: user.id,
            email: user.email,
            display_name: user.display_name,
            avatar_url: user.avatar_url,
            oauth_provider: user.oauth_provider,
            active_org_id,
            orgs,
            scm_connections,
            created_at: user.created_at.to_rfc3339(),
            onboarding_step: user.onboarding_step,
            onboarding_skipped: user.onboarding_skipped,
            email_on_approval_request,
            stop_vm_after_merge_default,
        })
    }

    /// Returns a minimal public profile for any user.
    ///
    /// Backing method for `GET /api/v1/users/:id`. Returns only `id`,
    /// `display_name`, and `avatar_url` — no email, OAuth provider, or org
    /// memberships.
    ///
    /// # Errors
    ///
    /// - `AppError::NotFound` if the user does not exist.
    /// - `AppError::Internal` on database errors.
    pub async fn get_user(
        &self,
        system_pool: &PgPool,
        user_id: Uuid,
    ) -> Result<PublicUserDTO, AppError> {
        let row = sqlx::query_as!(
            PublicUserDTO,
            r#"
            SELECT id, display_name, avatar_url
            FROM users
            WHERE id = $1
            "#,
            user_id
        )
        .fetch_optional(system_pool)
        .await?;

        match row {
            Some(dto) => Ok(dto),
            None => {
                warn!(user_id = %user_id, "User not found");
                Err(G5Error::NotFound {
                    resource: format!("user {}", user_id),
                }
                .into())
            }
        }
    }

    /// The display name (email when unset) and avatar of each of `ids`, for
    /// attributing records such as comment authors and question voters. Ids
    /// without a user row are absent from the map.
    pub async fn load_summaries<I>(
        system_pool: &PgPool,
        ids: I,
    ) -> Result<HashMap<Uuid, UserSummary>, AppError>
    where
        I: IntoIterator<Item = Uuid>,
    {
        let ids: Vec<Uuid> = ids.into_iter().collect();
        if ids.is_empty() {
            return Ok(HashMap::new());
        }
        let rows = sqlx::query_as::<_, UserSummary>(
            "SELECT id, COALESCE(display_name, email) AS display_name, avatar_url \
             FROM users WHERE id = ANY($1)",
        )
        .bind(&ids)
        .fetch_all(system_pool)
        .await
        .map_err(|err| {
            AppError::from_error(G5Error::Internal {
                message: format!("Failed to load user summaries: {err}"),
            })
        })?;
        Ok(rows.into_iter().map(|row| (row.id, row)).collect())
    }

    /// Resolves a user's human-readable display name from the global `users`
    /// table, degrading to `None` when the row is absent instead of erroring.
    ///
    /// Reads the unscoped `system_pool` because callers (e.g. platform issue
    /// intake) resolve a global user identity, not a tenant row. Unlike
    /// [`get_user`], a missing user is not an error here — the caller falls
    /// back to an id-only identity rather than failing the operation.
    ///
    /// # Errors
    ///
    /// - `AppError::Internal` on database errors.
    pub async fn resolve_display_name(
        &self,
        system_pool: &PgPool,
        user_id: Uuid,
    ) -> Result<Option<String>, AppError> {
        sqlx::query_scalar::<_, String>("SELECT display_name FROM users WHERE id = $1")
            .bind(user_id)
            .fetch_optional(system_pool)
            .await
            .map_err(|err| {
                G5Error::Internal {
                    message: format!("Database error: {err}"),
                }
                .into()
            })
    }

    /// Updates the authenticated user's profile fields.
    ///
    /// Backing method for `PATCH /api/v1/users/me`. Only `display_name` and
    /// `avatar_url` are updatable. Returns the full updated profile (same
    /// shape as [`get_current_user`]).
    ///
    /// # Validation
    ///
    /// - At least one field must be provided.
    /// - `display_name` (if provided) must be non-empty and ≤ 255 characters.
    /// - `avatar_url` can be `Some(None)` to clear, or `Some(Some(url))` to set.
    ///
    /// # Errors
    ///
    /// - `AppError::ValidationError` for invalid input.
    /// - `AppError::NotFound` if the user does not exist.
    /// - `AppError::Internal` on database errors.
    pub async fn update_user(
        &self,
        system_pool: &PgPool,
        user_id: Uuid,
        req: UpdateUserRequest,
    ) -> Result<CurrentUserDTO, AppError> {
        // Validate: at least one field must be provided
        if req.display_name.is_none()
            && req.avatar_url.is_none()
            && req.onboarding_step.is_none()
            && req.onboarding_skipped.is_none()
            && req.email_on_approval_request.is_none()
            && req.stop_vm_after_merge_default.is_none()
        {
            return Err(G5Error::ValidationError {
                details: serde_json::json!({
                    "error": "At least one field must be provided"
                }),
            }
            .into());
        }

        // Validate display_name if provided
        if let Some(ref name) = req.display_name {
            let trimmed = name.trim();
            if trimmed.is_empty() {
                return Err(G5Error::ValidationError {
                    details: serde_json::json!({
                        "field": "display_name",
                        "error": "display_name must not be empty"
                    }),
                }
                .into());
            }
            if trimmed.len() > 255 {
                return Err(G5Error::ValidationError {
                    details: serde_json::json!({
                        "field": "display_name",
                        "error": "display_name must be at most 255 characters"
                    }),
                }
                .into());
            }
        }

        if let Some(step) = req.onboarding_step {
            if !(0..=ONBOARDING_STEP_COUNT).contains(&step) {
                return Err(G5Error::ValidationError {
                    details: serde_json::json!({
                        "field": "onboarding_step",
                        "error": format!("onboarding_step must be between 0 and {ONBOARDING_STEP_COUNT}")
                    }),
                }
                .into());
            }
        }

        // Apply only supplied fields in one statement. A preference save must
        // not restore a stale name/avatar or lose another concurrent preference.
        let avatar_url = req
            .avatar_url
            .as_ref()
            .and_then(|value| value.as_deref())
            .filter(|value| !value.trim().is_empty());
        let result = sqlx::query(
            r#"
            UPDATE users
            SET display_name = COALESCE($2, display_name),
                avatar_url = CASE WHEN $3 THEN $4 ELSE avatar_url END,
                onboarding_step = NULLIF(
                    GREATEST(COALESCE(onboarding_step, -1), COALESCE($5::smallint, -1)),
                    -1
                ),
                onboarding_skipped = COALESCE($6, onboarding_skipped),
                email_on_approval_request = COALESCE($7, email_on_approval_request),
                stop_vm_after_merge_default = COALESCE($8, stop_vm_after_merge_default)
            WHERE id = $1
            "#,
        )
        .bind(user_id)
        .bind(req.display_name.as_deref().map(str::trim))
        .bind(req.avatar_url.is_some())
        .bind(avatar_url)
        .bind(req.onboarding_step)
        .bind(req.onboarding_skipped)
        .bind(req.email_on_approval_request)
        .bind(req.stop_vm_after_merge_default)
        .execute(system_pool)
        .await?;

        if result.rows_affected() == 0 {
            warn!(user_id = %user_id, "User not found during update");
            return Err(G5Error::NotFound {
                resource: format!("user {}", user_id),
            }
            .into());
        }

        // Log the update
        let fields_changed: Vec<&str> = [
            req.display_name.as_ref().map(|_| "display_name"),
            req.avatar_url.as_ref().map(|_| "avatar_url"),
            req.onboarding_step.as_ref().map(|_| "onboarding_step"),
            req.onboarding_skipped
                .as_ref()
                .map(|_| "onboarding_skipped"),
            req.email_on_approval_request
                .as_ref()
                .map(|_| "email_on_approval_request"),
            req.stop_vm_after_merge_default
                .as_ref()
                .map(|_| "stop_vm_after_merge_default"),
        ]
        .into_iter()
        .flatten()
        .collect();

        info!(
            user_id = %user_id,
            fields = ?fields_changed,
            "User profile updated"
        );

        // Return the full updated profile
        self.get_current_user(system_pool, user_id, None).await
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_update_user_request_deserialization() {
        // Test with both fields
        let json = r#"{"displayName": "Alice", "avatarUrl": "https://example.com/avatar.png"}"#;
        let req: UpdateUserRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.display_name.as_deref(), Some("Alice"));
        assert_eq!(
            req.avatar_url,
            Some(Some("https://example.com/avatar.png".to_string()))
        );

        // Test with only display_name
        let json = r#"{"displayName": "Bob"}"#;
        let req: UpdateUserRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.display_name.as_deref(), Some("Bob"));
        assert!(req.avatar_url.is_none());

        // Test clearing avatar_url (explicit null)
        let json = r#"{"avatarUrl": null}"#;
        let req: UpdateUserRequest = serde_json::from_str(json).unwrap();
        assert!(req.display_name.is_none());
        assert_eq!(req.avatar_url, Some(None));

        // Test empty object
        let json = r#"{}"#;
        let req: UpdateUserRequest = serde_json::from_str(json).unwrap();
        assert!(req.display_name.is_none());
        assert!(req.avatar_url.is_none());
    }

    #[test]
    fn test_current_user_dto_serialization() {
        let dto = CurrentUserDTO {
            id: Uuid::nil(),
            email: "alice@example.com".to_string(),
            display_name: "Alice".to_string(),
            avatar_url: Some("https://example.com/avatar.png".to_string()),
            oauth_provider: Some("github".to_string()),
            active_org_id: Some(Uuid::nil()),
            orgs: vec![OrgMembershipSummary {
                org_id: Uuid::nil(),
                org_name: "Acme Corp".to_string(),
                org_slug: "acme-corp".to_string(),
                role: "admin".to_string(),
                org_kind: "tenant".to_string(),
            }],
            scm_connections: vec![ScmConnectionSummary {
                provider: "github".to_string(),
                provider_username: "alice".to_string(),
                scopes: vec!["repo".to_string(), "read:org".to_string()],
                has_installation: true,
                connected_at: "2024-01-01T00:00:00+00:00".to_string(),
            }],
            created_at: "2024-01-01T00:00:00+00:00".to_string(),
            onboarding_step: Some(3),
            onboarding_skipped: false,
            email_on_approval_request: true,
            stop_vm_after_merge_default: false,
        };

        let json = serde_json::to_value(&dto).unwrap();

        // Verify camelCase serialization
        assert!(json.get("id").is_some());
        assert_eq!(
            json.get("emailOnApprovalRequest"),
            Some(&serde_json::json!(true))
        );
        assert!(json.get("email").is_some());
        assert!(json.get("displayName").is_some());
        assert!(json.get("avatarUrl").is_some());
        assert!(json.get("oauthProvider").is_some());
        assert!(json.get("orgs").is_some());
        assert!(json.get("scmConnections").is_some());
        assert!(json.get("createdAt").is_some());

        // Verify no snake_case keys
        assert!(json.get("display_name").is_none());
        assert!(json.get("avatar_url").is_none());
        assert!(json.get("oauth_provider").is_none());
        assert!(json.get("scm_connections").is_none());
        assert!(json.get("created_at").is_none());

        // Verify org membership camelCase
        let org = &json["orgs"][0];
        assert!(org.get("orgId").is_some());
        assert!(org.get("orgName").is_some());
        assert!(org.get("orgSlug").is_some());
        assert!(org.get("org_id").is_none());

        // Verify SCM connection camelCase
        let scm = &json["scmConnections"][0];
        assert!(scm.get("providerUsername").is_some());
        assert!(scm.get("hasInstallation").is_some());
        assert!(scm.get("connectedAt").is_some());
        assert!(scm.get("provider_username").is_none());
    }

    #[test]
    fn test_public_user_dto_serialization() {
        let dto = PublicUserDTO {
            id: Uuid::nil(),
            display_name: "Alice".to_string(),
            avatar_url: Some("https://example.com/avatar.png".to_string()),
        };

        let json = serde_json::to_value(&dto).unwrap();
        assert!(json.get("displayName").is_some());
        assert!(json.get("avatarUrl").is_some());
        assert!(json.get("display_name").is_none());
        assert!(json.get("avatar_url").is_none());
    }

    #[test]
    fn test_public_user_dto_no_private_fields() {
        let dto = PublicUserDTO {
            id: Uuid::nil(),
            display_name: "Alice".to_string(),
            avatar_url: None,
        };

        let json = serde_json::to_value(&dto).unwrap();
        // Verify no private fields are present
        assert!(json.get("email").is_none());
        assert!(json.get("oauthProvider").is_none());
        assert!(json.get("orgs").is_none());
        assert!(json.get("scmConnections").is_none());
    }

    #[test]
    fn test_scm_connection_summary_has_installation() {
        // With installation
        let summary = ScmConnectionSummary {
            provider: "github".to_string(),
            provider_username: "alice".to_string(),
            scopes: vec!["repo".to_string()],
            has_installation: true,
            connected_at: "2024-01-01T00:00:00+00:00".to_string(),
        };
        let json = serde_json::to_value(&summary).unwrap();
        assert_eq!(json["hasInstallation"], true);

        // Without installation
        let summary = ScmConnectionSummary {
            provider: "gitlab".to_string(),
            provider_username: "alice".to_string(),
            scopes: vec!["read_api".to_string()],
            has_installation: false,
            connected_at: "2024-01-01T00:00:00+00:00".to_string(),
        };
        let json = serde_json::to_value(&summary).unwrap();
        assert_eq!(json["hasInstallation"], false);
    }

    #[test]
    fn test_oauth_user_data_construction() {
        let data = OAuthUserData {
            email: "alice@example.com".to_string(),
            display_name: "Alice".to_string(),
            avatar_url: Some("https://avatars.githubusercontent.com/u/12345".to_string()),
            oauth_provider: "github".to_string(),
            oauth_id: "12345".to_string(),
        };
        assert_eq!(data.email, "alice@example.com");
        assert_eq!(data.oauth_provider, "github");
        assert_eq!(data.oauth_id, "12345");
    }
}
