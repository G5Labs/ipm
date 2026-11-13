//! Platform operator authority and cross-tenant facts
//! (`g5.organization.platform-operator`, G5-Lean#776).
//!
//! The operator organization (`orgs.kind = 'operator'`) holds the platform
//! roster in ordinary memberships. This module owns what that membership
//! authorizes outside the operator organization: observing the bounded
//! platform facts of every tenant and creating organizations for a named
//! Admin. It never opens tenant content; tenant rows stay behind RLS.

use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::PgConnection;
use uuid::Uuid;

use g5_contracts::error::G5Error;
use g5_contracts::types::enums::OrgRole;

use super::org_service::OrgKind;
use super::permission_service::PermissionService;

/// The dev/E2E fixture operator persona. Only the `G5_E2E_AUTH` test-token
/// path gives it operator membership; production operators come from the
/// platform seed's invitation and later operator invitations.
pub const E2E_OPERATOR_EMAIL: &str = "e2e-operator@g5labs.ai";

/// What a platform operation needs (`g5.organization.platform-operator.authority`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlatformAccess {
    /// Any operator role, including viewer.
    Observe,
    /// Operator member or administrator.
    Operate,
}

/// The caller's current operator membership.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlatformOperator {
    pub org_id: Uuid,
    pub role: OrgRole,
}

/// The operator organization, if bootstrap has created it.
pub async fn operator_org_id<'e, E>(executor: E) -> Result<Option<Uuid>, G5Error>
where
    E: sqlx::PgExecutor<'e>,
{
    Ok(sqlx::query_scalar("SELECT id FROM orgs WHERE kind = $1")
        .bind(OrgKind::Operator.as_str())
        .fetch_optional(executor)
        .await?)
}

/// Hold the caller's active operator membership through the operation
/// consuming it. `conn` must carry an authz context for `operator_org_id`.
pub async fn require_platform_operator(
    conn: &mut PgConnection,
    user_id: Uuid,
    operator_org_id: Uuid,
    access: PlatformAccess,
) -> Result<PlatformOperator, G5Error> {
    let refused = |scope: &str| G5Error::Forbidden {
        missing_scopes: vec![scope.to_string()],
    };
    let role =
        PermissionService::require_active_member_on_conn(conn, user_id, operator_org_id, false)
            .await
            .map_err(|error| match error {
                G5Error::Forbidden { .. } => refused("platform:observe"),
                other => other,
            })?;
    if access == PlatformAccess::Operate && role == OrgRole::Viewer {
        return Err(refused("platform:operate"));
    }
    Ok(PlatformOperator {
        org_id: operator_org_id,
        role,
    })
}

/// Give `user_id` an administrator membership in the operator organization
/// unless it already has one. Only the dev/E2E operator fixture calls this;
/// production operators join through invitations. Returns whether a
/// membership was added.
pub async fn join_operator_org<'e, E>(
    executor: E,
    operator_org_id: Uuid,
    user_id: Uuid,
) -> Result<bool, G5Error>
where
    E: sqlx::PgExecutor<'e>,
{
    Ok(sqlx::query(
        "INSERT INTO org_memberships (org_id, user_id, role)
         VALUES ($1, $2, 'admin')
         ON CONFLICT (org_id, user_id) DO NOTHING",
    )
    .bind(operator_org_id)
    .bind(user_id)
    .execute(executor)
    .await?
    .rows_affected()
        > 0)
}

/// Platform facts about one tenant
/// (`g5.organization.platform-operator.tenant-boundary`). Nothing else about
/// a tenant may be added here without amending that boundary.
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct PlatformOrgSummary {
    pub id: Uuid,
    pub name: String,
    pub slug: String,
    pub created_at: DateTime<Utc>,
    pub member_count: i64,
}

/// Every tenant organization, by name. Sized for up to a few thousand
/// organizations; pagination is a follow-up beyond that.
pub async fn list_tenant_orgs(conn: &mut PgConnection) -> Result<Vec<PlatformOrgSummary>, G5Error> {
    Ok(sqlx::query_as::<_, PlatformOrgSummary>(
        r#"
        SELECT o.id, o.name, o.slug, o.created_at,
               (SELECT count(*) FROM org_memberships m
                WHERE m.org_id = o.id AND m.deactivated_at IS NULL) AS member_count
        FROM orgs o
        WHERE o.kind = $1
        ORDER BY o.name ASC, o.id ASC
        "#,
    )
    .bind(OrgKind::Tenant.as_str())
    .fetch_all(conn)
    .await?)
}

/// The initial administrator an operator named for a new organization.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrgAdmin {
    pub user_id: Uuid,
    pub email: String,
    /// The account did not exist and was created for this organization.
    pub account_created: bool,
}

/// Trim and lowercase an administrator email, refusing anything that cannot be an
/// address. Sign-in compares verified emails in lowercase.
pub fn normalize_admin_email(raw: &str) -> Result<String, G5Error> {
    let email = raw.trim().to_lowercase();
    let valid = email.len() <= 320
        && !email.chars().any(char::is_whitespace)
        && email.split_once('@').is_some_and(|(local, domain)| {
            !local.is_empty() && domain.contains('.') && !domain.contains('@')
        });
    if !valid {
        return Err(G5Error::ValidationError {
            details: serde_json::json!({
                "field": "adminEmail",
                "error": "Admin email must be an email address"
            }),
        });
    }
    Ok(email)
}

/// Find the account for `email` (already normalized), or create one with no
/// sign-in identity (`g5.organization.platform-operator.create-org`). The
/// person's first verified sign-in with that email links to it.
pub async fn resolve_or_create_admin(
    conn: &mut PgConnection,
    email: &str,
) -> Result<OrgAdmin, G5Error> {
    if let Some(owner) = find_account(conn, email).await? {
        return Ok(owner);
    }
    let display_name = email.split('@').next().unwrap_or(email);
    let created: Option<Uuid> = sqlx::query_scalar(
        "INSERT INTO users (id, email, display_name)
         VALUES (gen_random_uuid(), $1, $2)
         ON CONFLICT DO NOTHING
         RETURNING id",
    )
    .bind(email)
    .bind(display_name)
    .fetch_optional(&mut *conn)
    .await?;
    match created {
        Some(user_id) => Ok(OrgAdmin {
            user_id,
            email: email.to_string(),
            account_created: true,
        }),
        // A concurrent sign-in or creation committed the account first.
        None => find_account(conn, email)
            .await?
            .ok_or_else(|| G5Error::Conflict {
                reason: "The administrator's account changed concurrently; retry".to_string(),
            }),
    }
}

async fn find_account(conn: &mut PgConnection, email: &str) -> Result<Option<OrgAdmin>, G5Error> {
    let rows: Vec<(Uuid, String)> =
        sqlx::query_as("SELECT id, email FROM users WHERE lower(email) = $1 LIMIT 2")
            .bind(email)
            .fetch_all(&mut *conn)
            .await?;
    match rows.as_slice() {
        [] => Ok(None),
        [(user_id, email)] => Ok(Some(OrgAdmin {
            user_id: *user_id,
            email: email.clone(),
            account_created: false,
        })),
        _ => Err(G5Error::Conflict {
            reason: "Several accounts match that email; choose the administrator by exact account"
                .to_string(),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn admin_email_is_normalized_or_refused() {
        assert_eq!(
            normalize_admin_email("  Alice@Example.COM ").unwrap(),
            "alice@example.com"
        );
        for bad in [
            "",
            "alice",
            "@example.com",
            "alice@",
            "a b@example.com",
            "a@b@c.com",
            "a@localhost",
        ] {
            assert!(
                normalize_admin_email(bad).is_err(),
                "{bad} should be refused"
            );
        }
    }
}
