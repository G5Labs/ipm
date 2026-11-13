use std::collections::HashMap;
use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    http::{header, HeaderValue},
    middleware::map_response,
    response::Response,
    routing::{get, post, put},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::db::scoped::{ensure_workspace_org_id, scoped_db_for_org, scoped_db_for_workspace};
use crate::errors::AppError;
use crate::middleware::AuthContext;
use crate::routes::audited::AuditedRouter;
use crate::routes::ResponseEnvelope;
use crate::services::{
    AgentLaunchSettingsPreview, AgentLaunchSettingsPreviewScope, AppState, ConfigService,
    EffectiveConfig, ScopeConfigView, ScopeUpdateInput,
};
use g5_auth::Scope;
use g5_contracts::types::{AgentLaunchPreferencesV1, ResolvedAgentExecutionV1};

#[derive(Debug, Deserialize)]
pub struct OrgPathParams {
    #[serde(rename = "orgId")]
    pub org_id: Uuid,
}

#[derive(Debug, Deserialize)]
pub struct WorkspacePathParams {
    #[serde(rename = "workspaceId")]
    pub workspace_id: Uuid,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OrgQueryParams {
    pub org_id: Uuid,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateScopedConfigRequest {
    #[serde(default)]
    pub settings: Option<serde_json::Value>,
    /// Top-level `settings` keys to delete after the merge. Deletion is explicit
    /// because a partial `settings` write merges (preserves unspecified keys)
    /// rather than replacing — see config route ontology D4 and config_service
    /// ontology D6. Mirrors `removeSecretKeys` on the secret channel.
    #[serde(default)]
    pub remove_setting_keys: Vec<String>,
    #[serde(default)]
    pub set_secrets: HashMap<String, String>,
    #[serde(default)]
    pub remove_secret_keys: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ResolveAgentLaunchRequest {
    pub preferences: AgentLaunchPreferencesV1,
    #[serde(default)]
    pub settings_preview: Vec<AgentLaunchSettingsPreviewRequest>,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentLaunchSettingsPreviewScopeRequest {
    Org,
    Workspace,
    User,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AgentLaunchSettingsPreviewRequest {
    pub scope: AgentLaunchSettingsPreviewScopeRequest,
    #[serde(default)]
    pub settings: Option<serde_json::Value>,
    #[serde(default)]
    pub remove_setting_keys: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolveAgentLaunchResponse {
    pub resolved: ResolvedAgentExecutionV1,
}

impl From<UpdateScopedConfigRequest> for ScopeUpdateInput {
    fn from(value: UpdateScopedConfigRequest) -> Self {
        Self {
            settings: value.settings,
            remove_setting_keys: value.remove_setting_keys,
            set_secrets: value.set_secrets,
            remove_secret_keys: value.remove_secret_keys,
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScopedConfigResponse {
    pub scope: String,
    pub settings: serde_json::Value,
    pub secret_keys: Vec<String>,
    /// Bounded beginning-of-key display hints for this scope's own secrets
    /// (config route D6): key → first 12 characters of the decrypted value,
    /// present only for hint-eligible values (see config_service D9;
    /// sanctioned by security_policy secret_custody_and_redaction D9). The
    /// audience is exactly the audience that can already read `secret_keys`;
    /// no response field ever carries a full secret value.
    pub secret_key_hints: HashMap<String, String>,
    pub effective_settings: serde_json::Value,
    pub effective_secret_keys: Vec<String>,
    /// Bounded display hints for the effective/winning secret values (config
    /// route D6): each hint reflects the same winning scope as the effective
    /// value itself.
    pub effective_secret_key_hints: HashMap<String, String>,
    /// Typed workspace-wide preferred coding-agent provider id derived from the
    /// effective settings (e.g. `"claude_code"`, `"codex"`), or `null` when no
    /// scope sets one. The raw value also appears inside `effective_settings`;
    /// this field is a convenience so the client does not re-parse the settings
    /// JSON. See config route ontology D2 and config_service ontology D3.
    pub default_agent_provider: Option<String>,
    /// Compatibility capability marker. Always true in G5 Lean.
    pub fork_chat_enabled: bool,
    pub updated_by: Option<String>,
    pub updated_at: Option<String>,
}

/// Scoped-config responses carry secret key names and bounded display hints
/// (config route D6): stamp `Cache-Control: no-store` on every response leaving
/// these routers — GET and PUT, success and error — so no browser or
/// intermediary cache retains secret-derived material.
async fn no_store(mut response: Response) -> Response {
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

pub fn org_router() -> Router<Arc<AppState>> {
    AuditedRouter::new()
        .audited_route("/", get(get_org_config), &[Scope::WorkspacesRead])
        .audited_route("/", put(update_org_config), &[Scope::OrgAdmin])
        .into_router()
        .layer(map_response(no_store))
}

pub fn workspace_router() -> Router<Arc<AppState>> {
    AuditedRouter::new()
        .audited_route("/", get(get_workspace_config), &[Scope::WorkspacesRead])
        .audited_route("/", put(update_workspace_config), &[Scope::WorkspacesWrite])
        .audited_route(
            "/agent-launch/resolve",
            post(resolve_workspace_agent_launch),
            &[Scope::WorkspacesRead],
        )
        .into_router()
        .layer(map_response(no_store))
}

async fn resolve_workspace_agent_launch(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(params): Path<WorkspacePathParams>,
    Json(body): Json<ResolveAgentLaunchRequest>,
) -> Result<Json<ResponseEnvelope<ResolveAgentLaunchResponse>>, AppError> {
    let actor_id = auth.require_user_id()?;
    let mut db =
        scoped_db_for_workspace(&state, &auth, params.workspace_id, &["workspaces:read"]).await?;
    let workspace_org_id = db.primary_org_id().map_err(AppError::from)?.into_uuid();
    let settings_preview: Vec<AgentLaunchSettingsPreview> = body
        .settings_preview
        .into_iter()
        .map(|preview| AgentLaunchSettingsPreview {
            scope: match preview.scope {
                AgentLaunchSettingsPreviewScopeRequest::Org => AgentLaunchSettingsPreviewScope::Org,
                AgentLaunchSettingsPreviewScopeRequest::Workspace => {
                    AgentLaunchSettingsPreviewScope::Workspace
                }
                AgentLaunchSettingsPreviewScopeRequest::User => {
                    AgentLaunchSettingsPreviewScope::User
                }
            },
            settings: preview.settings,
            remove_setting_keys: preview.remove_setting_keys,
        })
        .collect();
    // The account preference and any staged assignment key resolve through
    // the same projection dispatch uses (routes/config D9; config_service
    // D12, D13); nothing staged is persisted.
    let effective = state
        .config_service
        .resolve_effective_for_agent_launch(
            &mut db,
            workspace_org_id,
            Some(params.workspace_id),
            Some(actor_id),
            &settings_preview,
            &body.preferences,
            &state.provider_account_service,
        )
        .await?;
    db.commit().await.map_err(AppError::from)?;

    let resolved = ConfigService::preview_agent_launch(&effective, &body.preferences)?;
    Ok(Json(ResponseEnvelope::ok(ResolveAgentLaunchResponse {
        resolved,
    })))
}

pub fn user_router() -> Router<Arc<AppState>> {
    AuditedRouter::new()
        .audited_route("/me/config", get(get_user_config), &[Scope::WorkspacesRead])
        .audited_route(
            "/me/config",
            put(update_user_config),
            &[Scope::WorkspacesWrite],
        )
        .into_router()
        .layer(map_response(no_store))
}

async fn get_org_config(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(params): Path<OrgPathParams>,
) -> Result<Json<ResponseEnvelope<ScopedConfigResponse>>, AppError> {
    let mut db = scoped_db_for_org(&state, &auth, params.org_id, &["workspaces:read"]).await?;
    let local = state
        .config_service
        .get_org_scope(&mut db, params.org_id)
        .await?;
    let effective = state
        .config_service
        .resolve_effective(&mut db, params.org_id, None, None)
        .await?;
    db.commit().await.map_err(AppError::from)?;

    Ok(Json(ResponseEnvelope::ok(to_response(
        "org", local, effective,
    ))))
}

async fn update_org_config(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(params): Path<OrgPathParams>,
    Json(body): Json<UpdateScopedConfigRequest>,
) -> Result<Json<ResponseEnvelope<ScopedConfigResponse>>, AppError> {
    // Every organization setting and secret can affect other owners' launches.
    // Hold current target-org admin authority in the mutation transaction.
    let (mut db, actor_id) =
        crate::routes::github_app::org_admin_db(&state, &auth, params.org_id).await?;
    let local = state
        .config_service
        .update_org_scope(&mut db, params.org_id, actor_id, body.into())
        .await?;
    let effective = state
        .config_service
        .resolve_effective(&mut db, params.org_id, None, None)
        .await?;
    db.commit().await.map_err(AppError::from)?;

    Ok(Json(ResponseEnvelope::ok(to_response(
        "org", local, effective,
    ))))
}

async fn get_workspace_config(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(params): Path<WorkspacePathParams>,
    Query(query): Query<OrgQueryParams>,
) -> Result<Json<ResponseEnvelope<ScopedConfigResponse>>, AppError> {
    let actor_id = auth.require_user_id()?;
    let mut db =
        scoped_db_for_workspace(&state, &auth, params.workspace_id, &["workspaces:read"]).await?;
    let workspace_org_id = db.primary_org_id().map_err(AppError::from)?.into_uuid();
    ensure_workspace_org_id(workspace_org_id, query.org_id, "orgId")?;
    let local = state
        .config_service
        .get_workspace_scope(&mut db, params.workspace_id)
        .await?;
    let effective = state
        .config_service
        .resolve_effective(
            &mut db,
            query.org_id,
            Some(params.workspace_id),
            Some(actor_id),
        )
        .await?;
    db.commit().await.map_err(AppError::from)?;

    Ok(Json(ResponseEnvelope::ok(to_response(
        "workspace",
        local,
        effective,
    ))))
}

async fn update_workspace_config(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(params): Path<WorkspacePathParams>,
    Query(query): Query<OrgQueryParams>,
    Json(body): Json<UpdateScopedConfigRequest>,
) -> Result<Json<ResponseEnvelope<ScopedConfigResponse>>, AppError> {
    let actor_id = auth.require_user_id()?;
    let mut db =
        scoped_db_for_workspace(&state, &auth, params.workspace_id, &["workspaces:write"]).await?;
    let workspace_org_id = db.primary_org_id().map_err(AppError::from)?.into_uuid();
    ensure_workspace_org_id(workspace_org_id, query.org_id, "orgId")?;
    let local = state
        .config_service
        .update_workspace_scope(
            &mut db,
            query.org_id,
            params.workspace_id,
            actor_id,
            body.into(),
        )
        .await?;
    let effective = state
        .config_service
        .resolve_effective(
            &mut db,
            query.org_id,
            Some(params.workspace_id),
            Some(actor_id),
        )
        .await?;
    db.commit().await.map_err(AppError::from)?;

    Ok(Json(ResponseEnvelope::ok(to_response(
        "workspace",
        local,
        effective,
    ))))
}

async fn get_user_config(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Query(query): Query<OrgQueryParams>,
) -> Result<Json<ResponseEnvelope<ScopedConfigResponse>>, AppError> {
    let actor_id = auth.require_user_id()?;
    let mut db = scoped_db_for_org(&state, &auth, query.org_id, &["workspaces:read"]).await?;
    let local = state
        .config_service
        .get_user_scope(&mut db, query.org_id, actor_id)
        .await?;
    let effective = state
        .config_service
        .resolve_effective(&mut db, query.org_id, None, Some(actor_id))
        .await?;
    db.commit().await.map_err(AppError::from)?;

    Ok(Json(ResponseEnvelope::ok(to_response(
        "user", local, effective,
    ))))
}

async fn update_user_config(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Query(query): Query<OrgQueryParams>,
    Json(body): Json<UpdateScopedConfigRequest>,
) -> Result<Json<ResponseEnvelope<ScopedConfigResponse>>, AppError> {
    let actor_id = auth.require_user_id()?;
    let mut db = scoped_db_for_org(&state, &auth, query.org_id, &["workspaces:write"]).await?;
    let local = state
        .config_service
        .update_user_scope(&mut db, query.org_id, actor_id, actor_id, body.into())
        .await?;
    let effective = state
        .config_service
        .resolve_effective(&mut db, query.org_id, None, Some(actor_id))
        .await?;
    db.commit().await.map_err(AppError::from)?;

    Ok(Json(ResponseEnvelope::ok(to_response(
        "user", local, effective,
    ))))
}

fn to_response(
    scope: &str,
    local: ScopeConfigView,
    effective: EffectiveConfig,
) -> ScopedConfigResponse {
    let default_agent_provider =
        ConfigService::default_agent_provider_from_settings(&effective.settings)
            .map(|provider| provider.as_str().to_string());
    ScopedConfigResponse {
        scope: scope.to_string(),
        settings: local.settings,
        secret_keys: local.secret_keys,
        secret_key_hints: local.secret_hints,
        effective_settings: effective.settings,
        effective_secret_keys: effective.secret_keys,
        effective_secret_key_hints: effective.secret_hints,
        default_agent_provider,
        fork_chat_enabled: true,
        updated_by: local.updated_by.map(|id| id.to_string()),
        updated_at: local.updated_at.map(|value| value.to_rfc3339()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};

    fn response_for_effective_settings(settings: Value) -> ScopedConfigResponse {
        to_response(
            "workspace",
            ScopeConfigView {
                settings: json!({}),
                secret_keys: Vec::new(),
                secret_hints: HashMap::new(),
                updated_by: None,
                updated_at: None,
            },
            EffectiveConfig {
                settings,
                secret_keys: Vec::new(),
                secrets: HashMap::new(),
                secret_hints: HashMap::new(),
                account: None,
            },
        )
    }

    #[test]
    fn agent_launch_settings_preview_request_is_scope_aware_and_non_secret() {
        let request: ResolveAgentLaunchRequest = serde_json::from_value(json!({
            "preferences": { "schemaVersion": 1 },
            "settingsPreview": [{
                "scope": "workspace",
                "settings": { "OPENAI_BASE_URL": "https://openrouter.ai/api/v1" },
                "removeSettingKeys": ["OPENAI_MODEL"]
            }]
        }))
        .expect("scope-aware preview request deserializes");
        assert_eq!(request.settings_preview.len(), 1);
        assert!(matches!(
            request.settings_preview[0].scope,
            AgentLaunchSettingsPreviewScopeRequest::Workspace
        ));

        let rejected = serde_json::from_value::<ResolveAgentLaunchRequest>(json!({
            "preferences": { "schemaVersion": 1 },
            "settingsPreview": [{
                "scope": "workspace",
                "setSecrets": { "OPENAI_API_KEY": "must-not-enter-preview" }
            }]
        }));
        assert!(
            rejected.is_err(),
            "secret channels are not part of preview DTOs"
        );
    }

    #[test]
    fn scoped_config_response_carries_bounded_hints_and_never_full_values() {
        // Config route D6: the response surfaces exactly the service-derived
        // hint maps — own-scope and effective — and nothing value-shaped
        // beyond them.
        let own_secret = "sk-ant-oat01-0123456789abcdefghijklmnop";
        let effective_secret = "sk-or-v1-0123456789abcdef0123456789abcdef";
        let own_hints: HashMap<String, String> = [(
            "ANTHROPIC_AUTH_TOKEN".to_string(),
            ConfigService::secret_display_hint("ANTHROPIC_AUTH_TOKEN", own_secret)
                .expect("own value is hint-eligible"),
        )]
        .into();
        let effective_hints: HashMap<String, String> = [(
            "ANTHROPIC_AUTH_TOKEN".to_string(),
            ConfigService::secret_display_hint("ANTHROPIC_AUTH_TOKEN", effective_secret)
                .expect("effective value is hint-eligible"),
        )]
        .into();

        let response = to_response(
            "user",
            ScopeConfigView {
                settings: json!({}),
                secret_keys: vec!["ANTHROPIC_AUTH_TOKEN".to_string()],
                secret_hints: own_hints,
                updated_by: None,
                updated_at: None,
            },
            EffectiveConfig {
                settings: json!({}),
                secret_keys: vec!["ANTHROPIC_AUTH_TOKEN".to_string()],
                secrets: HashMap::from([(
                    "ANTHROPIC_AUTH_TOKEN".to_string(),
                    effective_secret.to_string(),
                )]),
                secret_hints: effective_hints,
                account: None,
            },
        );

        assert_eq!(
            response
                .secret_key_hints
                .get("ANTHROPIC_AUTH_TOKEN")
                .map(String::as_str),
            Some("sk-ant-oat01")
        );
        assert_eq!(
            response
                .effective_secret_key_hints
                .get("ANTHROPIC_AUTH_TOKEN")
                .map(String::as_str),
            Some("sk-or-v1-012")
        );

        // No serialized field may carry secret material beyond the sanctioned
        // 12-character hints (security_policy D9 required evidence).
        let serialized = serde_json::to_string(&response).expect("response serializes");
        assert!(!serialized.contains(own_secret));
        assert!(!serialized.contains(effective_secret));
        assert!(!serialized.contains(&own_secret[..13]));
        assert!(!serialized.contains(&effective_secret[..13]));
    }

    #[test]
    fn scoped_config_always_advertises_fork_chat() {
        // Retired scoped settings cannot disable the standard capability.
        let org_enabled = response_for_effective_settings(json!({ "fork_chat_enabled": true }));
        assert!(org_enabled.fork_chat_enabled);
        let org_disabled = response_for_effective_settings(json!({ "fork_chat_enabled": false }));
        assert!(org_disabled.fork_chat_enabled);

        // The capability is also available with no scoped setting.
        assert!(response_for_effective_settings(json!({})).fork_chat_enabled);
    }

    #[test]
    fn scoped_config_response_has_no_diff_graph_exposure_field() {
        let response = response_for_effective_settings(json!({}));
        let serialized = serde_json::to_value(response).expect("config response serializes");
        assert!(serialized
            .get("workspaceDiffGraphComparisonEnabled")
            .is_none());
    }
}
