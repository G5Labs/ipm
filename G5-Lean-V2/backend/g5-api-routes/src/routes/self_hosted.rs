use std::sync::Arc;

use axum::{
    http::{header, HeaderMap},
    routing::post,
    Json, Router,
};

use crate::errors::{AppError, G5Error};
use crate::routes::audited::AuditedRouter;
use crate::services::self_hosted_license_service::{AdminLicenseRequest, RegistryTokenRequest};
use crate::services::AppState;

pub fn admin_router() -> Router<Arc<AppState>> {
    AuditedRouter::new()
        .exempt_route(
            "/licenses",
            post(provision_license),
            "self-hosted license provisioning authenticates with admin bearer token",
        )
        .into_router()
}

pub fn registry_router() -> Router<Arc<AppState>> {
    AuditedRouter::new()
        .exempt_route(
            "/token",
            post(issue_registry_token),
            "self-hosted registry token exchange authenticates with license key bearer token",
        )
        .into_router()
}

async fn provision_license(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    headers: HeaderMap,
    Json(request): Json<AdminLicenseRequest>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let admin_token = bearer_from_headers(&headers)?;
    let response = state
        .self_hosted_license_service
        .provision_license(&admin_token, request)
        .await?;

    Ok(Json(response))
}

async fn issue_registry_token(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    headers: HeaderMap,
    Json(request): Json<RegistryTokenRequest>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let license_key = bearer_from_headers(&headers)?;
    let response = state
        .self_hosted_license_service
        .issue_registry_token(&license_key, request)
        .await?;

    Ok(Json(response))
}

fn bearer_from_headers(headers: &HeaderMap) -> Result<String, AppError> {
    let value = headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .filter(|token| !token.trim().is_empty())
        .ok_or(G5Error::Unauthorized)?;

    Ok(value.trim().to_string())
}
