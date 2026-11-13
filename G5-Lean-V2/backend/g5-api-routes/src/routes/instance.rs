//! Instance route handlers under `/api/v1/instance`.
//!
//! These endpoints provide server metadata and health information used by the
//! frontend shell during initialization and by host bootstrap processes.
//! They are the simplest endpoints in the API — read-only, with mixed
//! authentication requirements.
//!
//! Endpoints:
//! - `GET /info` — Server version, features, auth providers, upload limits, MOTD (unauthenticated)
//! - `GET /health` — Health check probing the database (unauthenticated)
//! - `GET /chat/models` — Available LLM models (authenticated)

use std::sync::Arc;
use std::time::Instant;

use axum::{extract::State, routing::get, Json, Router};
use serde::Serialize;

use crate::errors::AppError;
use crate::middleware::AuthContext;
use crate::routes::audited::AuditedRouter;
use crate::routes::ResponseEnvelope;
use crate::services::AppState;

// ─── Router ─────────────────────────────────────────────────────────────────

/// Returns an axum `Router` with all 3 instance endpoints registered.
///
/// This router is nested under `/api/v1/instance` by `build_router()` in
/// `routes/mod.rs`.
///
/// Authentication:
/// - `/info` and `/health` are unauthenticated (no `AuthContext` extractor)
/// - `/chat/models` requires authentication (`AuthContext`)
pub fn router() -> Router<Arc<AppState>> {
    // §38.141: Initialize startup timestamp at router construction (server startup),
    // not lazily on first health check.
    let _ = started_at();

    AuditedRouter::new()
        .exempt_route(
            "/info",
            get(get_instance_info),
            "Public instance metadata — no auth required",
        )
        .exempt_route(
            "/health",
            get(health_check),
            "Load balancer health probe — no auth required",
        )
        .exempt_route(
            "/chat/models",
            get(get_chat_models),
            "Model list — authenticated but no scope needed",
        )
        .into_router()
}

// ─── DTOs ───────────────────────────────────────────────────────────────────

/// Server metadata response for `GET /api/v1/instance/info`.
///
/// Used by the frontend shell during initialization to determine which
/// features are available, which OAuth providers to show on the login screen,
/// and the server version for compatibility checks.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstanceInfoDTO {
    pub version: String,
    pub features: Vec<String>,
    pub auth_providers: Vec<String>,
    pub max_upload_size_mb: u32,
    /// Whether this API is running in customer self-hosted deployment mode.
    pub self_hosted: bool,
    /// Whether dev/E2E login (without OAuth) is enabled.
    /// True when `G5_E2E_AUTH=true` is set.
    pub dev_login_enabled: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub motd: Option<String>,
}

/// Health check response for `GET /api/v1/instance/health`.
///
/// Wrapped in `ResponseEnvelope` like all other API endpoints so the
/// frontend `api.get()` helper can extract `.data` consistently (§38.352).
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HealthCheckResponse {
    pub status: String,
    pub checks: HealthChecks,
    pub uptime: u64,
}

/// Individual subsystem health check results.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HealthChecks {
    pub database: String,
}

/// Available LLM model for `GET /api/v1/instance/chat/models`.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatModelDTO {
    pub id: String,
    pub name: String,
    pub provider: String,
    pub context_window: Option<u64>,
    pub max_output_tokens: Option<u64>,
    pub supports_streaming: bool,
    pub cost_per_1k_input: Option<f64>,
    pub cost_per_1k_output: Option<f64>,
    pub agent_provider: g5_contracts::types::ChatAgentProvider,
    pub route: String,
    pub efforts: Vec<String>,
    pub default_effort: String,
    pub modes: Vec<String>,
    pub cli_version: String,
}

// ─── Startup Timestamp ──────────────────────────────────────────────────────

/// Module-level startup timestamp for computing uptime in the health check.
///
/// Initialized lazily on first access (effectively at server startup when the
/// first health check arrives). Using `std::sync::OnceLock` for thread-safe
/// one-time initialization.
static STARTED_AT: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();

/// Returns the server startup `Instant`, initializing it on first call.
fn started_at() -> &'static Instant {
    STARTED_AT.get_or_init(Instant::now)
}

/// Whether this deployment advertises the `guided_journey` instance feature —
/// the Guided sidebar section of the alignment workbench (workbench
/// `g5.ui.guided-toggle`). Default-ON kill-switch:
/// the feature is advertised unless `G5_GUIDED_JOURNEY_ENABLED` is exactly
/// `"false"` (2026-10-12: enabled everywhere once the section shipped).
/// Gates UI visibility only — the journey routes serve every layout.
fn guided_journey_feature_enabled() -> bool {
    !g5_contracts::env::is_env_flag_disabled("G5_GUIDED_JOURNEY_ENABLED")
}

/// Whether this deployment advertises the `provider_accounts` instance
/// feature — the per-environment carrier for the provider-accounts selection
/// surfaces' visibility gate (external_agent_provider_contract D14; instance
/// spec D16). Default-ON kill-switch semantics: the feature
/// is advertised unless `G5_ENABLE_PROVIDER_ACCOUNTS` is exactly `"false"`
/// (config_safe_defaults D6 — shipped product, not a dev/test gate, so the
/// safe posture is enabled). Gates UI visibility only — no backend route,
/// resolver, or persisted account/assignment branches on it.
fn provider_accounts_feature_enabled() -> bool {
    !g5_contracts::env::is_env_flag_disabled("G5_ENABLE_PROVIDER_ACCOUNTS")
}

// ─── Handlers ───────────────────────────────────────────────────────────────

/// `GET /api/v1/instance/info`
///
/// Returns server metadata used by the frontend shell during initialization.
/// This is the first API call the frontend makes on load.
///
/// **Unauthenticated** — the frontend needs this information before the user
/// has logged in (e.g., to determine which OAuth providers to show on the
/// login screen).
async fn get_instance_info(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ResponseEnvelope<InstanceInfoDTO>>, AppError> {
    tracing::debug!("Instance info requested");

    // Server version from Cargo.toml (compile-time constant)
    let version = env!("CARGO_PKG_VERSION").to_string();

    // Dynamically compute enabled features from server configuration.
    let mut features = vec!["chat".to_string()];
    features.push("codex_ui".to_string());
    // Guided journey section kill-switch (default ON; see the helper).
    if guided_journey_feature_enabled() {
        features.push("guided_journey".to_string());
    }
    // Provider-accounts kill-switch (default ON — advertised unless
    // G5_ENABLE_PROVIDER_ACCOUNTS is exactly "false"); instance spec D16.
    if provider_accounts_feature_enabled() {
        features.push("provider_accounts".to_string());
    }

    // Platform bug reporting: use the same startup-resolved capability
    // predicate as request admission so the client affordance never appears
    // where the route would fail closed.
    if state.platform_issue_reporting_available() {
        features.push("platform_issue_reporting".to_string());
    }

    // Issues are available in every deployment: an organization without an
    // external tracker uses G5's own (`g5.issue.native`).
    features.push("issues".to_string());

    // GitHub App organization bindings: advertised only when the complete
    // App configuration resolved at startup, the same predicate the routes use.
    if state.github_app.is_some() {
        features.push("github_app".to_string());
        // GitHub issue domains read GitHub through the same App.
        features.push("external_issues".to_string());
    }

    // Make Remote is available in every G5 Lean deployment.
    features.push("make_remote".to_string());

    // OAuth feature — present if at least one OAuth provider is configured.
    // Delegate to auth_service to determine which providers are enabled.
    let auth_providers = state.auth_service.enabled_providers();
    if !auth_providers.is_empty() {
        features.push("oauth".to_string());
    }

    // Upload limit — default 100 MB
    let max_upload_size_mb = 100u32;

    // Dev/E2E login — allows login without OAuth credentials.
    // Production-safe default: missing/unset/anything-other-than-"true" → false.
    let dev_login_enabled = g5_contracts::env::is_env_flag_enabled("G5_E2E_AUTH");

    // Optional MOTD — not yet configurable, defaults to None
    let motd: Option<String> = None;

    let dto = InstanceInfoDTO {
        version,
        features,
        auth_providers,
        max_upload_size_mb,
        self_hosted: state.is_self_hosted,
        dev_login_enabled,
        motd,
    };

    Ok(Json(ResponseEnvelope::ok(dto)))
}

/// `GET /api/v1/instance/health`
///
/// Returns the health status of the server and its dependent subsystems.
/// Used by Kubernetes liveness/readiness probes, load balancers, and
/// monitoring systems.
///
/// **Unauthenticated** — used by infrastructure tooling that doesn't have
/// user credentials.
///
/// Uses `ResponseEnvelope` for consistency with the frontend API client
/// which expects all responses to be wrapped in `{ data: T }` (§38.352).
///
/// Always returns HTTP 200, even when subsystems are unhealthy. The response
/// body indicates which subsystems are degraded. This prevents Kubernetes
/// from restarting pods due to downstream service failures (a database
/// outage doesn't mean the pod itself is broken).
async fn health_check(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ResponseEnvelope<HealthCheckResponse>>, AppError> {
    tracing::debug!("Health check requested");

    // Initialize startup timestamp on first call
    let start = started_at();
    let uptime = Instant::now().duration_since(*start).as_secs();

    // Check database connectivity: execute `SELECT 1` against the PgPool.
    // This is the most critical check — if the DB is down, most API
    // operations will fail.
    let db_healthy = match sqlx::query_scalar!(r#"SELECT 1 AS "one!""#)
        .fetch_one(&state.system_pool)
        .await
    {
        Ok(_) => true,
        Err(e) => {
            tracing::warn!(error = %e, "DB health check failed");
            false
        }
    };

    let all_healthy = db_healthy;
    let status = if all_healthy {
        "healthy".to_string()
    } else {
        "degraded".to_string()
    };

    if !all_healthy {
        tracing::info!(db = db_healthy, "Health check: server is degraded");
    }

    let response = HealthCheckResponse {
        status,
        checks: HealthChecks {
            database: if db_healthy { "healthy" } else { "unhealthy" }.to_string(),
        },
        uptime,
    };

    Ok(Json(ResponseEnvelope::ok(response)))
}

/// `GET /api/v1/instance/chat/models`
///
/// Returns the list of available LLM models that can be used for chat sessions.
///
/// **Authenticated** — requires valid user or service token.
async fn get_chat_models(
    State(state): State<Arc<AppState>>,
    _auth: AuthContext,
) -> Result<Json<ResponseEnvelope<Vec<ChatModelDTO>>>, AppError> {
    tracing::debug!("Chat models requested");

    let models = state.chat_service.get_available_models().await?;

    // Convert service-level model representations to route-level DTOs.
    // The service returns model info in its own internal format; we map
    // to the public API DTO defined in this file.
    let model_dtos: Vec<ChatModelDTO> = models
        .into_iter()
        .map(|m| ChatModelDTO {
            id: m.id,
            name: m.name,
            provider: m.provider,
            context_window: m.context_window,
            max_output_tokens: m.max_output_tokens,
            supports_streaming: m.supports_streaming,
            cost_per_1k_input: m.cost_per_1k_input,
            cost_per_1k_output: m.cost_per_1k_output,
            agent_provider: m.agent_provider,
            route: m.route,
            efforts: m.efforts,
            default_effort: m.default_effort,
            modes: m.modes,
            cli_version: m.cli_version,
        })
        .collect();

    Ok(Json(ResponseEnvelope::ok(model_dtos)))
}

// ─── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_instance_info_dto_serialization() {
        let dto = InstanceInfoDTO {
            version: "0.1.0".to_string(),
            features: vec!["chat".to_string()],
            auth_providers: vec!["github".to_string()],
            max_upload_size_mb: 100,
            self_hosted: true,
            dev_login_enabled: true,
            motd: Some("Welcome!".to_string()),
        };
        let json = serde_json::to_value(&dto).unwrap();

        assert_eq!(json["version"], "0.1.0");
        assert_eq!(json["features"], serde_json::json!(["chat"]));
        assert_eq!(json["authProviders"], serde_json::json!(["github"]));
        assert_eq!(json["maxUploadSizeMb"], 100);
        assert_eq!(json["selfHosted"], true);
        assert_eq!(json["devLoginEnabled"], true);
        assert_eq!(json["motd"], "Welcome!");
    }

    #[test]
    fn test_instance_info_dto_motd_none_omitted() {
        let dto = InstanceInfoDTO {
            version: "0.1.0".to_string(),
            features: vec![],
            auth_providers: vec![],
            max_upload_size_mb: 100,
            self_hosted: false,
            dev_login_enabled: false,
            motd: None,
        };
        let json = serde_json::to_value(&dto).unwrap();

        // motd should be absent (not null) due to skip_serializing_if
        assert!(json.get("motd").is_none());
    }

    /// A default-on kill-switch is advertised while its variable is missing or
    /// anything but the exact literal `"false"`. Env mutation is
    /// process-global: callers are `#[serial]` and the prior value is restored.
    fn assert_default_on_kill_switch(var: &str, enabled: fn() -> bool) {
        let prior = std::env::var(var).ok();

        std::env::remove_var(var);
        assert!(
            enabled(),
            "missing {var} must keep the default-on feature advertised"
        );
        for value in [
            "",
            "0",
            "1",
            "no",
            "yes",
            "true",
            "FALSE",
            "False",
            "  false  ",
        ] {
            std::env::set_var(var, value);
            assert!(
                enabled(),
                "only the exact value \"false\" disables {var} (got {value:?})"
            );
        }
        std::env::set_var(var, "false");
        assert!(!enabled(), "{var}=false must disable the feature");

        match prior {
            Some(value) => std::env::set_var(var, value),
            None => std::env::remove_var(var),
        }
    }

    #[test]
    #[serial_test::serial]
    fn guided_journey_feature_defaults_on_and_only_exact_false_disables() {
        assert_default_on_kill_switch("G5_GUIDED_JOURNEY_ENABLED", guided_journey_feature_enabled);
    }

    #[test]
    #[serial_test::serial]
    fn provider_accounts_feature_defaults_on_and_only_exact_false_disables() {
        assert_default_on_kill_switch(
            "G5_ENABLE_PROVIDER_ACCOUNTS",
            provider_accounts_feature_enabled,
        );
    }

    #[test]
    fn test_instance_info_dto_camel_case_fields() {
        let dto = InstanceInfoDTO {
            version: "1.0.0".to_string(),
            features: vec![],
            auth_providers: vec!["github".to_string()],
            max_upload_size_mb: 50,
            self_hosted: false,
            dev_login_enabled: false,
            motd: None,
        };
        let json = serde_json::to_value(&dto).unwrap();

        // Verify camelCase field names
        assert!(json.get("authProviders").is_some());
        assert!(json.get("maxUploadSizeMb").is_some());
        assert!(json.get("selfHosted").is_some());
        // Verify snake_case does NOT appear
        assert!(json.get("auth_providers").is_none());
        assert!(json.get("max_upload_size_mb").is_none());
        assert!(json.get("self_hosted").is_none());
    }

    #[test]
    fn test_health_check_response_all_healthy() {
        let response = HealthCheckResponse {
            status: "healthy".to_string(),
            checks: HealthChecks {
                database: "healthy".to_string(),
            },
            uptime: 3600,
        };
        let json = serde_json::to_value(&response).unwrap();

        assert_eq!(json["status"], "healthy");
        assert_eq!(json["checks"]["database"], "healthy");
        assert_eq!(json["uptime"], 3600);
    }

    #[test]
    fn test_health_check_response_degraded() {
        let response = HealthCheckResponse {
            status: "degraded".to_string(),
            checks: HealthChecks {
                database: "unhealthy".to_string(),
            },
            uptime: 1800,
        };
        let json = serde_json::to_value(&response).unwrap();

        assert_eq!(json["status"], "degraded");
        assert_eq!(json["checks"]["database"], "unhealthy");
    }

    #[test]
    fn test_health_check_response_envelope_structure() {
        // Verify the health check response is wrapped in a ResponseEnvelope (§38.352)
        let response = HealthCheckResponse {
            status: "healthy".to_string(),
            checks: HealthChecks {
                database: "healthy".to_string(),
            },
            uptime: 100,
        };
        let envelope = ResponseEnvelope::ok(response);
        let json = serde_json::to_value(&envelope).unwrap();

        // Should have a "data" wrapper with the health check fields inside
        assert!(json.get("data").is_some());
        let data = json.get("data").unwrap();
        assert!(data.get("status").is_some());
        assert!(data.get("checks").is_some());
        assert!(data.get("uptime").is_some());
    }

    #[test]
    fn test_chat_model_dto_serialization() {
        let dto = ChatModelDTO {
            id: "claude-sonnet-4-20250514".to_string(),
            name: "Claude Sonnet 4".to_string(),
            provider: "anthropic".to_string(),
            context_window: Some(200000),
            max_output_tokens: Some(64000),
            supports_streaming: true,
            cost_per_1k_input: Some(0.003),
            cost_per_1k_output: Some(0.015),
            agent_provider: g5_contracts::types::ChatAgentProvider::ClaudeCode,
            route: "direct".into(),
            efforts: vec!["high".into()],
            default_effort: "high".into(),
            modes: vec!["default".into()],
            cli_version: g5_contracts::claude_models::CLAUDE_CODE_CLI_VERSION
                .trim()
                .into(),
        };
        let json = serde_json::to_value(&dto).unwrap();

        assert_eq!(json["id"], "claude-sonnet-4-20250514");
        assert_eq!(json["name"], "Claude Sonnet 4");
        assert_eq!(json["provider"], "anthropic");
        assert_eq!(json["contextWindow"], 200000);
        assert_eq!(json["maxOutputTokens"], 64000);
        assert_eq!(json["supportsStreaming"], true);
        assert_eq!(json["costPer1kInput"], 0.003);
        assert_eq!(json["costPer1kOutput"], 0.015);
    }

    #[test]
    fn test_chat_model_dto_camel_case_fields() {
        let dto = ChatModelDTO {
            id: "test".to_string(),
            name: "Test".to_string(),
            provider: "test".to_string(),
            context_window: Some(1000),
            max_output_tokens: Some(100),
            supports_streaming: false,
            cost_per_1k_input: Some(0.0),
            cost_per_1k_output: Some(0.0),
            agent_provider: g5_contracts::types::ChatAgentProvider::ClaudeCode,
            route: "direct".into(),
            efforts: vec!["high".into()],
            default_effort: "high".into(),
            modes: vec!["default".into()],
            cli_version: g5_contracts::claude_models::CLAUDE_CODE_CLI_VERSION
                .trim()
                .into(),
        };
        let json = serde_json::to_value(&dto).unwrap();

        // Verify camelCase field names
        assert!(json.get("contextWindow").is_some());
        assert!(json.get("maxOutputTokens").is_some());
        assert!(json.get("supportsStreaming").is_some());
        assert!(json.get("costPer1kInput").is_some());
        assert!(json.get("costPer1kOutput").is_some());
        // Verify snake_case does NOT appear
        assert!(json.get("context_window").is_none());
        assert!(json.get("max_output_tokens").is_none());
        assert!(json.get("supports_streaming").is_none());
    }

    #[test]
    fn test_started_at_returns_consistent_value() {
        let first = started_at();
        let second = started_at();
        // Both calls should return the same Instant
        assert_eq!(*first, *second);
    }
}
