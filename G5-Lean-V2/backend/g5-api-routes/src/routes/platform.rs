//! Platform-operator routes (`g5.organization.platform-operator`, G5-Lean#776).
//!
//! Nested under `/api/v1/platform`. Every handler requires a person session
//! and the caller's current active membership in the operator organization;
//! operator membership opens nothing inside a tenant. The user-action audit
//! layer records every request here, reads included.
//!
//! Endpoints:
//! - `GET /orgs` — every tenant organization's platform facts (any operator role)
//! - `POST /orgs` — create an organization for a named initial Admin (operator member+)

use std::sync::Arc;

use axum::{extract::State, http::StatusCode, routing::get, Json, Router};
use g5_authz::{DbCapability, ScopedDb};
use g5_contracts::types::ids::{SessionId, UserId};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::db::scoped::scoped_system_db_for_org;
use crate::errors::AppError;
use crate::middleware::{extract_session_user_id, AuthContext};
use crate::routes::audited::AuditedRouter;
use crate::routes::orgs::{create_org_with_owner, OrgSummaryDTO};
use crate::routes::ResponseEnvelope;
use crate::services::org_service::{self, OrgKind};
use crate::services::platform_operator::{
    self, PlatformAccess, PlatformOrgSummary, E2E_OPERATOR_EMAIL,
};
use crate::services::AppState;
use g5_contracts::error::G5Error;

/// Request body for `POST /api/v1/platform/orgs`.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreatePlatformOrgRequestDTO {
    pub name: String,
    pub slug: Option<String>,
    /// Canonical email of the initial administrator.
    pub admin_email: String,
}

/// The initial administrator of an operator-created organization.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlatformOrgAdminDTO {
    pub user_id: String,
    pub email: String,
    pub account_created: bool,
}

/// Response for `POST /api/v1/platform/orgs`.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreatePlatformOrgResponseDTO {
    pub org: OrgSummaryDTO,
    pub admin: PlatformOrgAdminDTO,
}

pub fn router() -> Router<Arc<AppState>> {
    AuditedRouter::new()
        .exempt_route(
            "/orgs",
            get(list_orgs).post(create_org),
            "Platform operator — person session and current operator membership checked in handler",
        )
        .into_router()
}

/// Open a transaction holding the caller's current operator membership
/// (`g5.organization.platform-operator.authority`).
async fn operator_db(
    state: &Arc<AppState>,
    auth: &AuthContext,
    access: PlatformAccess,
) -> Result<(ScopedDb, Uuid), AppError> {
    let user_id = extract_session_user_id(auth)?;
    let operator_org_id = platform_operator::operator_org_id(&state.system_pool)
        .await?
        .ok_or_else(|| G5Error::Forbidden {
            missing_scopes: vec!["platform:observe".to_string()],
        })?;
    let mut db = scoped_system_db_for_org(state, auth, operator_org_id, &[])
        .await
        .map_err(|error| match error.inner {
            G5Error::Forbidden { .. } | G5Error::NotFound { .. } => {
                AppError::from_error(G5Error::Forbidden {
                    missing_scopes: vec!["platform:observe".to_string()],
                })
            }
            _ => error,
        })?;
    platform_operator::require_platform_operator(db.conn(), user_id, operator_org_id, access)
        .await?;
    Ok((db, user_id))
}

/// `GET /api/v1/platform/orgs`
async fn list_orgs(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
) -> Result<Json<ResponseEnvelope<Vec<PlatformOrgSummary>>>, AppError> {
    let (mut db, _) = operator_db(&state, &auth, PlatformAccess::Observe).await?;
    let orgs = platform_operator::list_tenant_orgs(db.conn()).await?;
    db.commit().await.map_err(AppError::from)?;
    Ok(Json(ResponseEnvelope::ok(orgs)))
}

/// `POST /api/v1/platform/orgs`
///
/// Creates an organization through the ordinary cascade with the named
/// account, created when absent, as its initial Admin and Base owner
/// (`g5.organization.platform-operator.create-org`). The operator gains no
/// membership. Returns 201 Created.
async fn create_org(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Json(body): Json<CreatePlatformOrgRequestDTO>,
) -> Result<
    (
        StatusCode,
        Json<ResponseEnvelope<CreatePlatformOrgResponseDTO>>,
    ),
    AppError,
> {
    extract_session_user_id(&auth)?;
    if state.is_self_hosted {
        return Err(AppError::from_error(G5Error::Forbidden {
            missing_scopes: vec!["self_hosted_default_org_only".to_string()],
        }));
    }
    let admin_email = platform_operator::normalize_admin_email(&body.admin_email)?;

    let (mut db, actor_id) = operator_db(&state, &auth, PlatformAccess::Operate).await?;
    let admin = platform_operator::resolve_or_create_admin(db.conn(), &admin_email).await?;
    let created = create_org_with_owner(
        &state,
        db,
        admin.user_id,
        org_service::CreateOrgRequest {
            name: body.name,
            slug: body.slug,
            kind: OrgKind::Tenant,
        },
    )
    .await?;

    tracing::info!(
        target: "g5_audit",
        event = "platform.org_created",
        actor_user_id = %actor_id,
        org_id = %created.org.id,
        admin_user_id = %admin.user_id,
        admin_account_created = admin.account_created,
        "Platform operator created an organization"
    );

    let admin = PlatformOrgAdminDTO {
        user_id: admin.user_id.to_string(),
        email: admin.email,
        account_created: admin.account_created,
    };
    Ok((
        StatusCode::CREATED,
        Json(ResponseEnvelope::ok(CreatePlatformOrgResponseDTO {
            org: created.org,
            admin,
        })),
    ))
}

/// Dev/E2E operator fixture (`G5_E2E_AUTH` only, via the test-token path):
/// give [`E2E_OPERATOR_EMAIL`] an operator membership, creating the operator
/// organization with that persona as Admin when none exists. Production
/// deployments get the operator organization only from the platform seed.
pub(crate) async fn ensure_e2e_operator(
    state: &Arc<AppState>,
    email: &str,
    user_id: Uuid,
) -> Result<(), AppError> {
    if !email.eq_ignore_ascii_case(E2E_OPERATOR_EMAIL) {
        return Ok(());
    }
    if let Some(org_id) = platform_operator::operator_org_id(&state.system_pool).await? {
        let joined =
            platform_operator::join_operator_org(&state.system_pool, org_id, user_id).await?;
        if joined {
            crate::routes::personal::after_membership_gain(state, org_id, user_id).await;
        }
        return Ok(());
    }
    let db = ScopedDb::unscoped(
        &state.system_pool,
        DbCapability {
            org_ids: vec![],
            actor_id: UserId::from_uuid(user_id),
            scopes: vec![],
            session_id: SessionId::new(),
            ttl_secs: 3600,
        },
    )
    .await
    .map_err(AppError::from)?;
    let request = org_service::CreateOrgRequest {
        name: "Platform operator".to_string(),
        slug: None,
        kind: OrgKind::Operator,
    };
    match create_org_with_owner(state, db, user_id, request).await {
        Ok(_) => Ok(()),
        // A concurrent fixture mint created it first; join it instead.
        Err(error) if matches!(error.inner, G5Error::Conflict { .. }) => {
            Box::pin(ensure_e2e_operator(state, email, user_id)).await
        }
        Err(error) => Err(error),
    }
}
