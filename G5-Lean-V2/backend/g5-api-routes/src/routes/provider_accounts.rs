//! Provider account route handlers
//! (`software_architecture/g5-api/routes/provider_accounts` D1–D5).
//!
//! Endpoints (nested at `/api/v1/orgs/:orgId/provider-accounts`):
//! - `GET    /?workspaceId=`      — list accounts visible to the caller
//! - `POST   /`                   — create an account at org/workspace/user scope
//! - `PATCH  /:accountId`         — rename, change base URL, rotate the secret
//! - `DELETE /:accountId`         — remove (409 `account_assigned` while assigned)
//!
//! Handlers are thin: authenticate the JWT user, open the org-scoped tenant
//! transaction, delegate to `ProviderAccountService`, commit, and map the
//! service view to the wire DTO. Every response is `Cache-Control: no-store`
//! because success bodies carry secret-derived display hints
//! (security_policy/secret_custody_and_redaction D9).

use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    http::{header, HeaderValue, StatusCode},
    middleware::map_response,
    response::Response,
    routing::{delete, get, patch, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::db::scoped::scoped_db_for_org;
use crate::errors::AppError;
use crate::middleware::AuthContext;
use crate::models::ProviderQuotaSnapshot;
use crate::routes::audited::AuditedRouter;
use crate::routes::ResponseEnvelope;
use crate::services::{
    AppState, CreateProviderAccount, ProviderAccountView, UpdateProviderAccount,
};
use g5_auth::Scope;
use g5_contracts::types::ProviderAccountKind;

// ─── Path / query parameters ─────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct OrgPathParams {
    #[serde(rename = "orgId")]
    pub org_id: Uuid,
}

#[derive(Debug, Deserialize)]
pub struct AccountPathParams {
    #[serde(rename = "orgId")]
    pub org_id: Uuid,
    #[serde(rename = "accountId")]
    pub account_id: Uuid,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListProviderAccountsQuery {
    /// Optional workspace whose workspace-scoped accounts should be included
    /// (only when the caller can read that workspace).
    #[serde(default)]
    pub workspace_id: Option<Uuid>,
}

// ─── Request DTOs (D2) ───────────────────────────────────────────────────────

/// `POST` body. No `Debug` derive: `secret` must never reach a log line.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreateProviderAccountRequest {
    pub name: String,
    pub kind: String,
    pub scope: String,
    #[serde(default)]
    pub workspace_id: Option<Uuid>,
    #[serde(default)]
    pub base_url: Option<String>,
    #[serde(default)]
    pub secret: Option<String>,
}

/// `PATCH` body: absent fields are unchanged; a blank secret is unchanged.
/// `kind` and `scope` are immutable and therefore have no field here; an
/// unknown field is rejected by `deny_unknown_fields`.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UpdateProviderAccountRequest {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub base_url: Option<String>,
    #[serde(default)]
    pub secret: Option<String>,
}

impl From<CreateProviderAccountRequest> for CreateProviderAccount {
    fn from(value: CreateProviderAccountRequest) -> Self {
        Self {
            name: value.name,
            kind: value.kind,
            scope: value.scope,
            workspace_id: value.workspace_id,
            base_url: value.base_url,
            secret: value.secret,
        }
    }
}

impl From<UpdateProviderAccountRequest> for UpdateProviderAccount {
    fn from(value: UpdateProviderAccountRequest) -> Self {
        Self {
            name: value.name,
            base_url: value.base_url,
            secret: value.secret,
        }
    }
}

// ─── Response DTO (D3) ───────────────────────────────────────────────────────

/// Display-only usage projection of a Claude subscription account's latest
/// quota snapshot across the org's workspaces (D6;
/// external_agent_provider_contract D13). Windows are fractions in
/// `0.0..=1.0` and timestamps RFC 3339; it carries no key components, claim
/// text, status string, or anything secret-derived.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderAccountUsageDto {
    pub five_hour_utilization: Option<f32>,
    pub five_hour_reset_at: Option<String>,
    pub seven_day_utilization: Option<f32>,
    pub seven_day_reset_at: Option<String>,
    pub captured_at: String,
}

impl From<ProviderQuotaSnapshot> for ProviderAccountUsageDto {
    fn from(snapshot: ProviderQuotaSnapshot) -> Self {
        Self {
            five_hour_utilization: snapshot.five_hour_utilization,
            five_hour_reset_at: snapshot.five_hour_reset_at.map(|at| at.to_rfc3339()),
            seven_day_utilization: snapshot.seven_day_utilization,
            seven_day_reset_at: snapshot.seven_day_reset_at.map(|at| at.to_rfc3339()),
            captured_at: snapshot.captured_at.to_rfc3339(),
        }
    }
}

/// Wire shape of one provider account. Carries at most the bounded
/// 12-character `secretHint`. `usage` is populated only by the list route for
/// `claude_subscription` accounts that have a snapshot (D6); it is `null`
/// for every other kind and on the single-row mutation responses.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderAccountDto {
    pub id: Uuid,
    pub org_id: Uuid,
    pub name: String,
    pub kind: String,
    pub family: String,
    pub scope: String,
    pub workspace_id: Option<Uuid>,
    pub user_id: Option<Uuid>,
    pub base_url: Option<String>,
    pub has_secret: bool,
    pub secret_hint: Option<String>,
    pub created_by: Uuid,
    pub created_at: String,
    pub updated_at: String,
    pub usage: Option<ProviderAccountUsageDto>,
}

impl From<ProviderAccountView> for ProviderAccountDto {
    fn from(view: ProviderAccountView) -> Self {
        Self {
            id: view.id,
            org_id: view.org_id,
            name: view.name,
            kind: view.kind.as_str().to_string(),
            family: view.family.as_str().to_string(),
            scope: view.scope.as_str().to_string(),
            workspace_id: view.workspace_id,
            user_id: view.user_id,
            base_url: view.base_url,
            has_secret: view.has_secret,
            secret_hint: view.secret_hint,
            created_by: view.created_by,
            created_at: view.created_at.to_rfc3339(),
            updated_at: view.updated_at.to_rfc3339(),
            usage: None,
        }
    }
}

// ─── Router (D1, D5) ─────────────────────────────────────────────────────────

/// Every response leaving this router — success and error alike — is stamped
/// `Cache-Control: no-store` (D5), mirroring the scoped-config routers.
async fn no_store(mut response: Response) -> Response {
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

pub fn router() -> Router<Arc<AppState>> {
    AuditedRouter::new()
        .audited_route("/", get(list_accounts), &[Scope::WorkspacesRead])
        .audited_route("/", post(create_account), &[Scope::WorkspacesWrite])
        .audited_route(
            "/:accountId",
            patch(update_account),
            &[Scope::WorkspacesWrite],
        )
        .audited_route(
            "/:accountId",
            delete(delete_account),
            &[Scope::WorkspacesWrite],
        )
        .into_router()
        .layer(map_response(no_store))
}

// ─── Handlers ────────────────────────────────────────────────────────────────

/// `GET /api/v1/orgs/:orgId/provider-accounts?workspaceId=`
///
/// Claude subscription rows carry their latest snapshot across the org's
/// workspaces as `usage` (D6); every other kind stays `null` because no
/// passive header source exists for it.
async fn list_accounts(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(params): Path<OrgPathParams>,
    Query(query): Query<ListProviderAccountsQuery>,
) -> Result<Json<ResponseEnvelope<Vec<ProviderAccountDto>>>, AppError> {
    let actor_id = auth.require_user_id()?;
    let mut db = scoped_db_for_org(&state, &auth, params.org_id, &["workspaces:read"]).await?;
    let accounts = state
        .provider_account_service
        .list_visible(&mut db, params.org_id, actor_id, query.workspace_id)
        .await?;
    let subscription_ids: Vec<Uuid> = accounts
        .iter()
        .filter(|account| account.kind == ProviderAccountKind::ClaudeSubscription)
        .map(|account| account.id)
        .collect();
    let mut usage = state
        .provider_quota_service
        .latest_for_accounts(&mut db, params.org_id, &subscription_ids)
        .await?;
    db.commit().await.map_err(AppError::from)?;
    Ok(Json(ResponseEnvelope::ok(
        accounts
            .into_iter()
            .map(|account| {
                let snapshot = (account.kind == ProviderAccountKind::ClaudeSubscription)
                    .then(|| usage.remove(&account.id))
                    .flatten();
                let mut dto = ProviderAccountDto::from(account);
                dto.usage = snapshot.map(ProviderAccountUsageDto::from);
                dto
            })
            .collect(),
    )))
}

/// `POST /api/v1/orgs/:orgId/provider-accounts`
async fn create_account(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(params): Path<OrgPathParams>,
    Json(body): Json<CreateProviderAccountRequest>,
) -> Result<(StatusCode, Json<ResponseEnvelope<ProviderAccountDto>>), AppError> {
    let actor_id = auth.require_user_id()?;
    let mut db = scoped_db_for_org(&state, &auth, params.org_id, &["workspaces:write"]).await?;
    let created = state
        .provider_account_service
        .create(&mut db, params.org_id, actor_id, body.into())
        .await?;
    db.commit().await.map_err(AppError::from)?;
    Ok((
        StatusCode::CREATED,
        Json(ResponseEnvelope::ok(ProviderAccountDto::from(created))),
    ))
}

/// `PATCH /api/v1/orgs/:orgId/provider-accounts/:accountId`
async fn update_account(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(params): Path<AccountPathParams>,
    Json(body): Json<UpdateProviderAccountRequest>,
) -> Result<Json<ResponseEnvelope<ProviderAccountDto>>, AppError> {
    let actor_id = auth.require_user_id()?;
    let mut db = scoped_db_for_org(&state, &auth, params.org_id, &["workspaces:write"]).await?;
    let updated = state
        .provider_account_service
        .update(
            &mut db,
            params.org_id,
            actor_id,
            params.account_id,
            body.into(),
        )
        .await?;
    db.commit().await.map_err(AppError::from)?;
    Ok(Json(ResponseEnvelope::ok(ProviderAccountDto::from(
        updated,
    ))))
}

/// `DELETE /api/v1/orgs/:orgId/provider-accounts/:accountId`
async fn delete_account(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(params): Path<AccountPathParams>,
) -> Result<StatusCode, AppError> {
    let actor_id = auth.require_user_id()?;
    let mut db = scoped_db_for_org(&state, &auth, params.org_id, &["workspaces:write"]).await?;
    state
        .provider_account_service
        .delete(&mut db, params.org_id, actor_id, params.account_id)
        .await?;
    db.commit().await.map_err(AppError::from)?;
    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use g5_contracts::types::{ProviderAccountFamily, ProviderAccountKind, ProviderAccountScope};
    use serde_json::json;

    #[test]
    fn update_request_rejects_kind_and_scope_and_unknown_fields() {
        for body in [
            json!({ "kind": "openai_api_key" }),
            json!({ "scope": "org" }),
            json!({ "name": "x", "workspaceId": Uuid::nil() }),
        ] {
            assert!(
                serde_json::from_value::<UpdateProviderAccountRequest>(body.clone()).is_err(),
                "{body} must be rejected"
            );
        }
        let ok: UpdateProviderAccountRequest =
            serde_json::from_value(json!({ "name": "Renamed", "secret": "" })).unwrap();
        assert_eq!(ok.name.as_deref(), Some("Renamed"));
        assert_eq!(ok.secret.as_deref(), Some(""));
        assert!(ok.base_url.is_none());
    }

    #[test]
    fn create_request_uses_the_camel_case_wire_shape() {
        let request: CreateProviderAccountRequest = serde_json::from_value(json!({
            "name": "Team key",
            "kind": "anthropic_api_key",
            "scope": "workspace",
            "workspaceId": Uuid::nil(),
            "baseUrl": "https://llm.example.test",
            "secret": "sk-ant-api03-0123456789abcdefghijklmnop"
        }))
        .unwrap();
        assert_eq!(request.workspace_id, Some(Uuid::nil()));
        assert!(
            serde_json::from_value::<CreateProviderAccountRequest>(json!({
                "name": "x", "kind": "anthropic_api_key", "scope": "org", "usage": null
            }))
            .is_err()
        );
    }

    #[test]
    fn dto_carries_only_the_hint_and_a_null_usage() {
        let now = Utc::now();
        let dto = ProviderAccountDto::from(ProviderAccountView {
            id: Uuid::nil(),
            org_id: Uuid::nil(),
            name: "Team key".into(),
            kind: ProviderAccountKind::AnthropicApiKey,
            family: ProviderAccountFamily::Claude,
            scope: ProviderAccountScope::Org,
            workspace_id: None,
            user_id: None,
            base_url: None,
            has_secret: true,
            secret_hint: Some("sk-ant-api03".into()),
            created_by: Uuid::nil(),
            created_at: now,
            updated_at: now,
        });
        let value = serde_json::to_value(&dto).unwrap();
        assert_eq!(value["kind"], "anthropic_api_key");
        assert_eq!(value["family"], "claude");
        assert_eq!(value["scope"], "org");
        assert_eq!(value["hasSecret"], true);
        assert_eq!(value["secretHint"], "sk-ant-api03");
        assert!(value["usage"].is_null());
        assert!(value.get("secretEnc").is_none());
        assert!(value.get("secret").is_none());
    }

    /// D6: the usage projection carries the two windows and the capture time
    /// only — no key components, claim text, or status string.
    #[test]
    fn usage_projection_carries_windows_and_capture_time_only() {
        let now = Utc::now();
        let usage = ProviderAccountUsageDto::from(ProviderQuotaSnapshot {
            org_id: Uuid::nil(),
            workspace_id: Uuid::nil(),
            provider: "claude_code".into(),
            auth_mode: "anthropic_oauth".into(),
            account_id: Uuid::nil(),
            five_hour_utilization: Some(0.42),
            five_hour_reset_at: Some(now),
            seven_day_utilization: None,
            seven_day_reset_at: None,
            representative_claim: Some("claim".into()),
            unified_status: Some("allowed".into()),
            captured_at: now,
            created_at: now,
            updated_at: now,
        });
        let value = serde_json::to_value(&usage).unwrap();
        let mut keys: Vec<&str> = value
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        keys.sort_unstable();
        assert_eq!(
            keys,
            vec![
                "capturedAt",
                "fiveHourResetAt",
                "fiveHourUtilization",
                "sevenDayResetAt",
                "sevenDayUtilization"
            ]
        );
        assert!((value["fiveHourUtilization"].as_f64().unwrap() - 0.42).abs() < 1e-6);
        assert_eq!(value["fiveHourResetAt"], now.to_rfc3339());
        assert!(value["sevenDayUtilization"].is_null());
        assert_eq!(value["capturedAt"], now.to_rfc3339());
    }
}
