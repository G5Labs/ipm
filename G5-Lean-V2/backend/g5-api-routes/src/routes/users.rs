//! User route handlers under `/api/v1/users`.
//!
//! These endpoints provide user profile retrieval and update functionality.
//! The handlers follow the thin handler pattern: extract request parameters,
//! delegate to `UserService`, and format the response using `ResponseEnvelope`.
//!
//! Endpoints:
//! - `GET /me` — Get the current authenticated user's full profile
//! - `PATCH /me` — Update the current user's display name and/or avatar URL
//! - `GET /:id` — Get a public user profile (minimal: id, display name, avatar URL)

use std::sync::Arc;

use axum::{
    extract::{Path, State},
    routing::get,
    Json, Router,
};
use uuid::Uuid;

use crate::db::scoped::ensure_public_user_id;
use crate::errors::AppError;
use crate::middleware::{extract_session_user_id, AuthContext};
use crate::routes::audited::AuditedRouter;
use crate::routes::ResponseEnvelope;
use crate::services::user_service::{CurrentUserDTO, PublicUserDTO, UpdateUserRequest};
use crate::services::AppState;
use g5_contracts::types::ids::UserId;

// ——— Router ———————————————————————————————————————————————————————————

/// Returns an axum `Router` with all 3 user endpoints registered.
///
/// This router is nested under `/api/v1/users` by `build_router()` in
/// `routes/mod.rs`.
///
/// Authentication:
/// - All endpoints require authentication (`AuthContext` extractor)
/// - `/me` endpoints require a person session (reject API keys and worker JWTs)
/// - `/:id` accepts any authenticated token
pub fn router() -> Router<Arc<AppState>> {
    AuditedRouter::new()
        .exempt_route(
            "/me",
            get(get_current_user).patch(update_current_user),
            "Account profile — person session required in handler",
        )
        .exempt_route(
            "/:id",
            get(get_user_public),
            "Public user info — any authenticated user",
        )
        .into_router()
}

// ——— Handlers ————————————————————————————————————————————————————————

/// `GET /api/v1/users/me`
///
/// Returns the full profile of the currently authenticated user, including
/// their org memberships and SCM connection summaries. This is the primary
/// endpoint the frontend calls on initial page load to populate the sidebar
/// (org list), settings page (SCM connections), and header (avatar/name).
///
/// **Authenticated (person session only)** — API keys and worker JWTs are rejected.
async fn get_current_user(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
) -> Result<Json<ResponseEnvelope<CurrentUserDTO>>, AppError> {
    let user_id = extract_session_user_id(&auth)?;

    tracing::debug!(user_id = %user_id, "Getting current user profile");

    let active_org_id = auth.org_id().and_then(|s| Uuid::parse_str(&s).ok());

    let user = state
        .user_service
        .get_current_user(&state.system_pool, user_id, active_org_id)
        .await?;

    Ok(Json(ResponseEnvelope::ok(user)))
}

/// `PATCH /api/v1/users/me`
///
/// Updates the authenticated user's display name, avatar URL and onboarding
/// tour progress (users g5.user.onboarding: the step never goes down).
/// Returns the full updated `CurrentUserDTO` so the client can update its
/// local state in one round-trip.
///
/// **Authenticated (person session only)** — API keys and worker JWTs are rejected.
async fn update_current_user(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Json(body): Json<UpdateUserRequest>,
) -> Result<Json<ResponseEnvelope<CurrentUserDTO>>, AppError> {
    let user_id = extract_session_user_id(&auth)?;

    tracing::debug!(user_id = %user_id, "Updating current user profile");

    let user = state
        .user_service
        .update_user(&state.system_pool, user_id, body)
        .await?;

    tracing::info!(user_id = %user_id, "User profile updated");

    Ok(Json(ResponseEnvelope::ok(user)))
}

/// `GET /api/v1/users/:id`
///
/// Returns a minimal public profile for any user. Used by the frontend
/// to display avatars and names in comments, chat messages, and activity
/// feeds.
///
/// **Authenticated (any token)** — accepts any authenticated JWT.
/// The `_auth` extractor enforces authentication but the value is not used.
async fn get_user_public(
    State(state): State<Arc<AppState>>,
    _auth: AuthContext,
    Path(user_id): Path<UserId>,
) -> Result<Json<ResponseEnvelope<PublicUserDTO>>, AppError> {
    let user_id = user_id.into_uuid();
    ensure_public_user_id(&state.system_pool, user_id, "userId").await?;
    tracing::debug!(user_id = %user_id, "Getting public user profile");

    let user = state
        .user_service
        .get_user(&state.system_pool, user_id)
        .await?;

    Ok(Json(ResponseEnvelope::ok(user)))
}
