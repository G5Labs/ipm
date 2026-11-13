//! API key route handlers.
//!
//! Endpoints:
//! - `POST   /api/v1/orgs/:orgId/api-keys`            — create key
//! - `GET    /api/v1/orgs/:orgId/api-keys`             — list keys
//! - `DELETE /api/v1/orgs/:orgId/api-keys/:keyId`      — revoke key

use std::sync::Arc;

use axum::{
    extract::{Path, State},
    routing::{delete, get, post},
    Json, Router,
};
use serde::Deserialize;
use uuid::Uuid;

use crate::db::scoped::{ensure_api_key_id, scoped_db_for_org};
use crate::errors::AppError;
use crate::middleware::{extract_session_user_id, AuthContext};
use crate::routes::audited::AuditedRouter;
use crate::routes::ResponseEnvelope;
use crate::services::api_key_service::{ApiKey, CreateApiKeyRequest, CreateApiKeyResult};
use crate::services::AppState;
use g5_auth::Scope;

// ─── Path Parameter Types ────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct OrgPathParams {
    #[serde(rename = "orgId")]
    pub org_id: Uuid,
}

#[derive(Debug, Deserialize)]
pub struct KeyPathParams {
    #[serde(rename = "orgId")]
    pub org_id: Uuid,
    #[serde(rename = "keyId")]
    pub key_id: Uuid,
}

// ─── Router ──────────────────────────────────────────────────────────────────

pub fn router() -> Router<Arc<AppState>> {
    AuditedRouter::new()
        .audited_route("/", post(create_key), &[Scope::OrgAdmin])
        .audited_route("/", get(list_keys), &[Scope::OrgAdmin])
        .audited_route("/:keyId", delete(revoke_key), &[Scope::OrgAdmin])
        .into_router()
}

// ─── Handlers ────────────────────────────────────────────────────────────────

/// `POST /api/v1/orgs/:orgId/api-keys`
///
/// Creates a new API key. The plaintext key is returned once and cannot be
/// retrieved again. Key management is a person's administration
/// (`g5.user.api-key-authority`): a delegated key cannot mint a credential,
/// so nothing it could issue outlives its own scope, organization or
/// revocation boundary (`g5.security.credential-classes`).
async fn create_key(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(params): Path<OrgPathParams>,
    Json(body): Json<CreateApiKeyRequest>,
) -> Result<Json<ResponseEnvelope<CreateApiKeyResult>>, AppError> {
    let user_id = extract_session_user_id(&auth)?;
    let mut db = scoped_db_for_org(&state, &auth, params.org_id, &["org:admin"]).await?;

    let result = state
        .api_key_service
        .create_key(&mut db, params.org_id, user_id, body)
        .await?;

    db.commit().await.map_err(AppError::from)?;
    Ok(Json(ResponseEnvelope::ok(result)))
}

/// `GET /api/v1/orgs/:orgId/api-keys`
///
/// Lists all API keys for an org. Returns key prefixes, not full keys.
async fn list_keys(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(params): Path<OrgPathParams>,
) -> Result<Json<ResponseEnvelope<Vec<ApiKey>>>, AppError> {
    let mut db = scoped_db_for_org(&state, &auth, params.org_id, &["org:admin"]).await?;

    let keys = state
        .api_key_service
        .list_keys(&mut db, params.org_id)
        .await?;

    db.commit().await.map_err(AppError::from)?;
    Ok(Json(ResponseEnvelope::ok(keys)))
}

/// `DELETE /api/v1/orgs/:orgId/api-keys/:keyId`
///
/// Revokes an API key (soft delete). Like creation, revocation requires the
/// administrator's own session, not a delegated key.
async fn revoke_key(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(params): Path<KeyPathParams>,
) -> Result<Json<ResponseEnvelope<()>>, AppError> {
    extract_session_user_id(&auth)?;
    let mut db = scoped_db_for_org(&state, &auth, params.org_id, &["org:admin"]).await?;
    ensure_api_key_id(&mut db, params.org_id, params.key_id, "keyId").await?;

    state
        .api_key_service
        .revoke_key(&mut db, params.org_id, params.key_id)
        .await?;

    db.commit().await.map_err(AppError::from)?;
    Ok(Json(ResponseEnvelope::ok(())))
}
