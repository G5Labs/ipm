//! Resolve route handler under `/api/v1/resolve`.
//!
//! Implements the Slug Resolver API endpoint (API_AND_STORAGE.md §3.18).
//! Provides a single `GET /api/v1/resolve` endpoint that resolves
//! human-readable slug combinations (`org`, `repo`, `workspace`) to their
//! canonical UUIDs.
//!
//! The endpoint supports an optional `redirect` query parameter that triggers
//! a 302 redirect with the resolved IDs interpolated into a local path template.

use std::sync::Arc;

use axum::{
    extract::{Query, State},
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};
use serde::{Deserialize, Serialize};

use crate::db::scoped::scoped_db_for_user_orgs;
use crate::errors::{AppError, G5Error};
use crate::middleware::AuthContext;
use crate::routes::audited::AuditedRouter;
use crate::routes::ResponseEnvelope;
use crate::services::AppState;
use g5_auth::Scope;

// ─── Router ─────────────────────────────────────────────────────────────────

/// Returns an axum `Router` with the resolve endpoint registered.
///
/// This router is nested under `/api/v1/resolve` by `build_router()` in
/// `routes/mod.rs`.
///
/// Authentication: Requires valid JWT (not in exempt paths list).
/// Scopes: Requires `workspaces:read`.
pub fn router() -> Router<Arc<AppState>> {
    AuditedRouter::new()
        .audited_route("/", get(resolve_slugs), &[Scope::WorkspacesRead])
        .into_router()
}

// ─── DTOs ───────────────────────────────────────────────────────────────────

/// Query parameters for `GET /api/v1/resolve`.
///
/// The `org` parameter is required; `repo` and `workspace` are optional.
/// If `redirect` is provided, the endpoint returns a 302 redirect with
/// resolved IDs interpolated into a local path template.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolveQuery {
    /// Org slug — required.
    pub org: String,
    /// Repo slug within the org — optional.
    pub repo: Option<String>,
    /// Workspace slug within the org — optional.
    pub workspace: Option<String>,
    /// Local redirect path template — optional.
    /// If provided, returns 302 with resolved IDs interpolated, or 400 if
    /// the resulting destination is not a valid local return path.
    /// Template placeholders: {orgId}, {repoId}, {workspaceId}
    pub redirect: Option<String>,
}

/// Response DTO for `GET /api/v1/resolve` (200 — JSON resolution).
///
/// Contains the resolved UUIDs as hyphenated strings. Optional fields
/// are only present when the corresponding query parameter was provided
/// and resolved successfully.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolveDTO {
    /// Always present — the resolved org UUID.
    pub org_id: String,
    /// Present only if `repo` query param was provided.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repo_id: Option<String>,
    /// Present only if `workspace` query param was provided.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workspace_id: Option<String>,
    /// The workspace's current name, present with `workspace_id`. It differs
    /// from the query when a former name resolved; clients replace the former
    /// name with it (`g5.workspace.former-names`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workspace_name: Option<String>,
}

// ─── Handler ────────────────────────────────────────────────────────────────

/// `GET /api/v1/resolve`
///
/// Resolves human-readable slug strings to canonical UUIDs.
///
/// The `org` parameter is required. `repo` and `workspace` are optional.
/// If any provided slug does not resolve, the endpoint returns 404.
///
/// When the `redirect` query parameter is present, returns a 302 redirect
/// with resolved IDs interpolated into the local path template. An invalid
/// final return path yields 400. Otherwise, without `redirect`, returns a 200
/// JSON response with the resolved IDs in a `ResponseEnvelope`.
///
/// **Authenticated** — requires valid JWT with `workspaces:read` scope.
async fn resolve_slugs(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Query(params): Query<ResolveQuery>,
) -> Result<Response, AppError> {
    let mut db = scoped_db_for_user_orgs(&state, &auth, &["workspaces:read"]).await?;
    tracing::debug!(
        org = %params.org,
        repo = ?params.repo,
        workspace = ?params.workspace,
        "Resolving slugs"
    );

    let result = state
        .org_service
        .resolve_slugs(
            &state.system_pool,
            &mut db,
            &params.org,
            params.repo.as_deref(),
            params.workspace.as_deref(),
        )
        .await?;

    if let Some(redirect_template) = params.redirect {
        let url = redirect_template
            .replace("{orgId}", &result.org_id.to_string())
            .replace(
                "{repoId}",
                &result.repo_id.map(|id| id.to_string()).unwrap_or_default(),
            )
            .replace(
                "{workspaceId}",
                &result
                    .workspace_id
                    .map(|id| id.to_string())
                    .unwrap_or_default(),
            );

        // Check the final destination: an absent optional ID can turn a local
        // template such as /{repoId}/host into a protocol-relative URL.
        if !crate::auth::return_path::is_local_return_path(&url) {
            return Ok((
                StatusCode::BAD_REQUEST,
                AppError::from_error(G5Error::ValidationError {
                    details: serde_json::json!({
                        "redirect": "must resolve to a local return path"
                    }),
                }),
            )
                .into_response());
        }

        Ok((StatusCode::FOUND, [(header::LOCATION, url)]).into_response())
    } else {
        let dto = ResolveDTO {
            org_id: result.org_id.to_string(),
            repo_id: result.repo_id.map(|id| id.to_string()),
            workspace_id: result.workspace_id.map(|id| id.to_string()),
            workspace_name: result.workspace_name,
        };

        Ok(Json(ResponseEnvelope::ok(dto)).into_response())
    }
}

// ─── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_query_deserialization_required_only() {
        let json = serde_json::json!({
            "org": "acme"
        });
        let query: ResolveQuery = serde_json::from_value(json).unwrap();
        assert_eq!(query.org, "acme");
        assert!(query.repo.is_none());
        assert!(query.workspace.is_none());
        assert!(query.redirect.is_none());
    }

    #[test]
    fn test_resolve_query_deserialization_all_fields() {
        let json = serde_json::json!({
            "org": "acme",
            "repo": "backend",
            "workspace": "streaming-api",
            "redirect": "/w/{workspaceId}"
        });
        let query: ResolveQuery = serde_json::from_value(json).unwrap();
        assert_eq!(query.org, "acme");
        assert_eq!(query.repo.as_deref(), Some("backend"));
        assert_eq!(query.workspace.as_deref(), Some("streaming-api"));
        assert_eq!(query.redirect.as_deref(), Some("/w/{workspaceId}"));
    }

    #[test]
    fn test_resolve_dto_serialization_all_fields() {
        let dto = ResolveDTO {
            org_id: "a1b2c3d4-e5f6-7890-abcd-ef1234567890".to_string(),
            repo_id: Some("b2c3d4e5-f6a7-8901-bcde-f12345678901".to_string()),
            workspace_id: Some("c3d4e5f6-a7b8-9012-cdef-123456789012".to_string()),
            workspace_name: Some("streaming-api".to_string()),
        };
        let json = serde_json::to_value(&dto).unwrap();

        assert_eq!(json["orgId"], "a1b2c3d4-e5f6-7890-abcd-ef1234567890");
        assert_eq!(json["repoId"], "b2c3d4e5-f6a7-8901-bcde-f12345678901");
        assert_eq!(json["workspaceId"], "c3d4e5f6-a7b8-9012-cdef-123456789012");
    }

    #[test]
    fn test_resolve_dto_serialization_org_only() {
        let dto = ResolveDTO {
            org_id: "a1b2c3d4-e5f6-7890-abcd-ef1234567890".to_string(),
            repo_id: None,
            workspace_id: None,
            workspace_name: None,
        };
        let json = serde_json::to_value(&dto).unwrap();

        assert_eq!(json["orgId"], "a1b2c3d4-e5f6-7890-abcd-ef1234567890");
        // repo_id and workspace_id should be absent (not null) due to skip_serializing_if
        assert!(json.get("repoId").is_none());
        assert!(json.get("workspaceId").is_none());
    }

    #[test]
    fn test_resolve_dto_camel_case_fields() {
        let dto = ResolveDTO {
            org_id: "test-id".to_string(),
            repo_id: Some("repo-id".to_string()),
            workspace_id: Some("ws-id".to_string()),
            workspace_name: Some("ws".to_string()),
        };
        let json = serde_json::to_value(&dto).unwrap();

        // Verify camelCase field names
        assert!(json.get("orgId").is_some());
        assert!(json.get("repoId").is_some());
        assert!(json.get("workspaceId").is_some());
        // Verify snake_case does NOT appear
        assert!(json.get("org_id").is_none());
        assert!(json.get("repo_id").is_none());
        assert!(json.get("workspace_id").is_none());
    }

    #[test]
    fn test_resolve_dto_partial_fields() {
        // Only org and workspace, no repo
        let dto = ResolveDTO {
            org_id: "org-uuid".to_string(),
            repo_id: None,
            workspace_id: Some("ws-uuid".to_string()),
            workspace_name: Some("ws".to_string()),
        };
        let json = serde_json::to_value(&dto).unwrap();

        assert_eq!(json["orgId"], "org-uuid");
        assert!(json.get("repoId").is_none());
        assert_eq!(json["workspaceId"], "ws-uuid");
    }

    #[test]
    fn test_resolve_query_missing_org_fails() {
        // org is required — deserialization should fail without it
        let json = serde_json::json!({
            "repo": "backend"
        });
        let result: Result<ResolveQuery, _> = serde_json::from_value(json);
        assert!(result.is_err());
    }
}
