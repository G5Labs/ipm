//! Git smart HTTP protocol routes (§14 of WORKER_STORAGE_SEPARATION.md).
//!
//! Implements the server-side of git's smart HTTP transport so workers (and
//! CLI users) can `git clone` / `git push` via HTTPS against the API.
//!
//! Endpoints:
//! - `GET  /git/:repoId/info/refs?service=...`     — ref advertisement
//! - `POST /git/:repoId/git-upload-pack`            — clone / fetch
//! - `POST /git/:repoId/git-receive-pack`           — push
//!
//! Auth is handled by the [`GitAuth`] extractor (HTTP Basic with JWT),
//! NOT the global auth middleware (the `/git/` prefix is exempt).

use std::io::Read;
use std::path::PathBuf;
use std::sync::Arc;

use axum::{
    body::{to_bytes, Body},
    extract::{Path, Query, State},
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Router,
};
use flate2::read::{DeflateDecoder, GzDecoder};
use serde::Deserialize;
use sqlx::{Postgres, Row, Transaction};
use uuid::Uuid;

use crate::db::scoped::user_can_access_workspace;
use crate::services::git_policy::protected_branches_for_repo;
use crate::services::workspace_incarnation::{self, IncarnationLock, LiveIncarnation};
use crate::services::AppState;
use crate::ws::events::BranchUpdateHint;
use crate::ws::{Channel, WsEvent};
use g5_auth::{JwtValidator, Scope, WorkspaceIncarnationClaims};
use g5_authz::ScopedDb;
use g5_contracts::types::ids::UserId;

const GIT_SMART_HTTP_BODY_LIMIT_BYTES: usize = 256 * 1024 * 1024;
// Fail fast instead of queueing blocking jobs that retain request buffers.
static GIT_DECODE_SLOTS: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(2);

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

/// Git-specific error — maps to HTTP status codes expected by git clients.
#[derive(Debug)]
pub enum GitError {
    RepoNotFound,
    Unauthorized(String),
    Forbidden(String),
    /// The pushed ref update is stale — its declared old SHA no longer matches
    /// the current server tip (a lost-update race). Distinct
    /// from `Forbidden` so a pushing client can tell a recoverable stale-ref
    /// conflict (pull-forward + retry) apart from a fatal authorization denial.
    /// Maps to HTTP 409 Conflict.
    Conflict(String),
    BadRequest(String),
    PayloadTooLarge,
    DecodeBusy,
    Native(g5_scm::native::NativeGitError),
    NativeCompletion {
        error: g5_scm::native::NativeGitError,
        confirmed_refs: Vec<String>,
    },
    Internal(String),
}

impl IntoResponse for GitError {
    fn into_response(self) -> Response {
        let (status, msg) = match self {
            Self::RepoNotFound => (StatusCode::NOT_FOUND, "repository not found".to_string()),
            Self::Unauthorized(m) => {
                tracing::warn!("git unauthorized: {m}");
                (StatusCode::UNAUTHORIZED, m)
            }
            Self::Forbidden(m) => {
                tracing::warn!("git forbidden: {m}");
                (StatusCode::FORBIDDEN, m)
            }
            Self::Conflict(m) => {
                tracing::warn!("git conflict: {m}");
                (StatusCode::CONFLICT, m)
            }
            Self::BadRequest(m) => (StatusCode::BAD_REQUEST, m),
            Self::PayloadTooLarge => (
                StatusCode::PAYLOAD_TOO_LARGE,
                "git request body exceeds size limit".into(),
            ),
            Self::DecodeBusy => (
                StatusCode::SERVICE_UNAVAILABLE,
                "git request decompression is busy; retry later".into(),
            ),
            Self::Native(error) => {
                tracing::warn!(%error, "Git operation failed");
                (StatusCode::INTERNAL_SERVER_ERROR, error.public_message())
            }
            Self::NativeCompletion {
                error,
                confirmed_refs,
            } => {
                tracing::warn!(%error, ?confirmed_refs, "Git completion failed");
                (StatusCode::INTERNAL_SERVER_ERROR, format!(
                    "{}; confirmed accepted refs: {}; inspect current refs before retrying; use Update PR if publication needs recovery",
                    error.public_message(),
                    if confirmed_refs.is_empty() { "none confirmed".into() } else { confirmed_refs.join(", ") },
                ))
            }
            Self::Internal(m) => {
                tracing::error!("git internal error: {m}");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "internal error".to_string(),
                )
            }
        };

        // Git clients expect a plain-text body on error, with
        // WWW-Authenticate on 401.
        let mut builder = Response::builder().status(status);
        if status == StatusCode::UNAUTHORIZED {
            builder = builder.header("WWW-Authenticate", "Basic realm=\"g5 git\"");
        }
        builder
            .body(Body::from(msg))
            .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
    }
}

// ---------------------------------------------------------------------------
// GitAuth — HTTP Basic auth extractor
// ---------------------------------------------------------------------------

/// Authenticated git identity extracted from HTTP Basic credentials.
///
/// Git clients pass credentials via `Authorization: Basic base64(user:pass)`.
/// Accepted usernames distinguish generic and explicit user credentials:
/// - `x-access-token:<jwt>` → workspace-incarnation or user JWT
/// - `x-user-token:<jwt>` → user identity
/// - Bearer token in Authorization header also accepted
#[derive(Debug, Clone)]
pub struct GitAuth {
    /// The authenticated user's ID.
    pub actor_id: String,
    /// Present only for the typed, owner-bound workspace VM credential.
    pub workspace_incarnation: Option<WorkspaceIncarnationClaims>,
    pub host_source: Option<g5_auth::HostSourceClaims>,
}

impl GitAuth {
    fn actor_uuid(&self) -> Result<Uuid, GitError> {
        if self.host_source.is_some() {
            return Err(GitError::Forbidden(
                "source preparation is read-only".into(),
            ));
        }
        self.actor_id
            .parse()
            .map_err(|_| GitError::Internal("invalid actor_id in JWT".into()))
    }

    /// Extract and validate git auth from request headers.
    ///
    /// Supports HTTP Basic `x-access-token:<jwt>` / `x-user-token:<jwt>` and
    /// `Authorization: Bearer <jwt>`.
    pub fn from_headers(
        headers: &HeaderMap,
        jwt_validator: &JwtValidator,
    ) -> Result<Self, GitError> {
        let auth_header = headers
            .get(header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .ok_or_else(|| GitError::Unauthorized("missing Authorization header".into()))?;

        // Try Bearer first.
        if let Some(token) = auth_header.strip_prefix("Bearer ") {
            let token = token.trim();
            if token.is_empty() {
                return Err(GitError::Unauthorized("empty bearer token".into()));
            }
            if let Ok(claims) = jwt_validator.validate_host_source(token) {
                return Ok(Self {
                    actor_id: String::new(),
                    workspace_incarnation: None,
                    host_source: Some(claims),
                });
            }
            if let Ok(claims) = jwt_validator.validate_workspace_incarnation(token) {
                return Ok(Self::from_workspace_incarnation(claims));
            }
            let claims = jwt_validator
                .validate(token)
                .map_err(|e| GitError::Unauthorized(format!("invalid JWT: {e}")))?;

            if claims.scopes.contains(&Scope::WorkerHost) {
                return Err(GitError::Unauthorized(
                    "host JWT is not valid for git transport".into(),
                ));
            }
            return Ok(Self {
                actor_id: claims.sub,
                workspace_incarnation: None,
                host_source: None,
            });
        }

        // Try HTTP Basic.
        if let Some(encoded) = auth_header.strip_prefix("Basic ") {
            let decoded = base64_decode_basic(encoded.trim())?;
            let (username, password) = decoded
                .split_once(':')
                .ok_or_else(|| GitError::Unauthorized("malformed Basic credentials".into()))?;

            if let Ok(claims) = jwt_validator.validate_host_source(password) {
                if username != "x-access-token" {
                    return Err(GitError::Unauthorized(
                        "source grant requires x-access-token".into(),
                    ));
                }
                return Ok(Self {
                    actor_id: String::new(),
                    workspace_incarnation: None,
                    host_source: Some(claims),
                });
            }
            if let Ok(claims) = jwt_validator.validate_workspace_incarnation(password) {
                if username != "x-access-token" {
                    return Err(GitError::Unauthorized(
                        "workspace token requires x-access-token".into(),
                    ));
                }
                return Ok(Self::from_workspace_incarnation(claims));
            }
            let claims = jwt_validator
                .validate(password)
                .map_err(|e| GitError::Unauthorized(format!("invalid JWT: {e}")))?;

            match username {
                "x-user-token" | "x-access-token" => {}
                _ => {
                    return Err(GitError::Unauthorized(format!(
                        "unknown git auth username '{username}', expected x-access-token or x-user-token"
                    )));
                }
            }
            if claims.scopes.contains(&Scope::WorkerHost) {
                return Err(GitError::Unauthorized(
                    "host JWT is not valid for git transport".into(),
                ));
            }

            return Ok(Self {
                actor_id: claims.sub,
                workspace_incarnation: None,
                host_source: None,
            });
        }

        Err(GitError::Unauthorized(
            "expected Authorization: Bearer <jwt> or Basic <base64>".into(),
        ))
    }

    fn from_workspace_incarnation(claims: WorkspaceIncarnationClaims) -> Self {
        Self {
            actor_id: claims.owner_user_id.to_string(),
            workspace_incarnation: Some(claims),
            host_source: None,
        }
    }

    /// Verify the workspace incarnation or user can push to a repo.
    pub async fn require_push(&self, state: &Arc<AppState>, repo_id: Uuid) -> Result<(), GitError> {
        if let Some(claims) = &self.workspace_incarnation {
            // Members, and the owner's mounted personal repository
            // (g5.personal.mount-push). A personal workspace's reference
            // checkouts of Base are readable, never writable
            // (g5.workspace.identity.reference).
            match current_incarnation_repo_target(state, claims, repo_id).await {
                Ok(_) => return Ok(()),
                Err(GitError::Forbidden(reason)) => {
                    return match incarnation_personal_mount(state, claims, repo_id).await? {
                        Some(_) => Ok(()),
                        None => Err(GitError::Forbidden(reason)),
                    };
                }
                Err(error) => return Err(error),
            }
        }
        let user_id = self.actor_uuid()?;
        let mut db =
            scoped_db_for_user_actor_all_orgs(state, user_id, &["workspaces:write"]).await?;
        let targets = repo_workspace_targets(db.conn(), repo_id).await?;

        let mut has_access = false;
        for target in targets {
            if user_can_write_workspace(
                &state.system_pool,
                &mut db,
                user_id,
                target.org_id,
                target.workspace_id,
            )
            .await?
            {
                has_access = true;
                break;
            }
        }

        let _ = db.rollback().await;

        if !has_access {
            return Err(GitError::Forbidden(
                "no write access to this repository's workspace".into(),
            ));
        }
        Ok(())
    }

    async fn require_push_ref_updates(
        &self,
        state: &Arc<AppState>,
        repo_id: Uuid,
        ref_updates: &[RefUpdate],
    ) -> Result<Vec<PushTarget>, GitError> {
        if let Some(claims) = &self.workspace_incarnation {
            return require_incarnation_ref_updates(state, claims, repo_id, ref_updates).await;
        }
        let actor_id = self.actor_uuid()?;
        let mut db =
            scoped_db_for_user_actor_all_orgs(state, actor_id, &["workspaces:write"]).await?;
        let protected = protected_branches_for_repo(db.conn(), repo_id)
            .await
            .map_err(|e| match e {
                sqlx::Error::RowNotFound => GitError::RepoNotFound,
                other => GitError::Internal(format!("protected branch lookup: {other}")),
            })?;
        let targets = sqlx::query_as::<_, PushTarget>(
            r#"
            SELECT w.id AS workspace_id, w.org_id,
                   CASE WHEN w.is_base THEN r.default_branch ELSE w.branch_name END AS branch_name,
                   false AS immutable_create
            FROM workspace_repos m
            JOIN workspaces w ON w.id = m.workspace_id AND w.org_id = m.org_id
            JOIN repos r ON r.id = m.repo_id AND r.org_id = m.org_id
            WHERE m.repo_id = $1 AND w.deleted_at IS NULL
            "#,
        )
        .bind(repo_id)
        .fetch_all(db.conn())
        .await
        .map_err(|e| GitError::Internal(format!("push target lookup: {e}")))?;

        let mut authorized = Vec::new();
        for update in ref_updates {
            // Checkpoints are reserved for the workspace token introduced by
            // P4.4 and enabled by P4.3. Tags and staging refs are also denied.
            let branch = update.ref_name.strip_prefix("refs/heads/").ok_or_else(|| {
                GitError::Forbidden(format!(
                    "network push namespace denied: {}",
                    update.ref_name
                ))
            })?;
            if protected.contains(branch) {
                return Err(GitError::Forbidden(format!("protected branch: {branch}")));
            }
            if update.new_sha.is_zero() {
                return Err(GitError::Forbidden(
                    "branch deletion is not permitted".into(),
                ));
            }
            let mut allowed = None;
            for target in targets.iter().filter(|target| target.branch_name == branch) {
                let current_owner: Option<Uuid> = sqlx::query_scalar(
                    "SELECT owner_user_id FROM workspaces WHERE id = $1 AND org_id = $2 AND deleted_at IS NULL",
                )
                .bind(target.workspace_id)
                .bind(target.org_id)
                .fetch_optional(db.conn())
                .await
                .map_err(|e| GitError::Internal(format!("workspace owner lookup: {e}")))?
                .flatten();
                let can_write = current_owner == Some(actor_id);
                if can_write {
                    allowed = Some(target.clone());
                    break;
                }
            }
            authorized.push(allowed.ok_or_else(|| {
                GitError::Forbidden(format!("no write access to workspace branch: {branch}"))
            })?);
        }
        let _ = db.rollback().await;
        Ok(authorized)
    }

    /// Verify the actor may read the repo and return its storage path.
    ///
    /// The path is resolved only after authorization, for the organization
    /// the verified credential carries (workspace incarnation or host
    /// source) or the one row-level security returned for a user's
    /// workspace, so the store's tenancy guard (`g5.repo.storage-guard`)
    /// sees the same organization the read was authorized in.
    pub async fn require_read(
        &self,
        state: &Arc<AppState>,
        repo_id: Uuid,
    ) -> Result<PathBuf, GitError> {
        if let Some(claims) = &self.host_source {
            g5_api_core::services::hosts::source_preparation::require_source_read(
                state, claims, repo_id,
            )
            .await
            .map_err(|_| GitError::Forbidden("host source scope is no longer authorized".into()))?;
            return repo_path(state, claims, claims.org_id, repo_id);
        }
        if let Some(claims) = &self.workspace_incarnation {
            require_current_workspace_incarnation(state, claims, repo_id).await?;
            return repo_path(state, claims, claims.org_id, repo_id);
        }
        let user_id = self.actor_uuid()?;
        let mut db = scoped_db_for_user_actor_all_orgs(state, user_id, &[]).await?;
        let targets = repo_workspace_targets(db.conn(), repo_id).await?;

        if targets.is_empty() {
            return Err(GitError::RepoNotFound);
        }

        for target in targets {
            if user_has_workspace_access(
                &state.system_pool,
                &mut db,
                user_id,
                target.org_id,
                target.workspace_id,
            )
            .await?
            {
                let path = repo_path(state, &db, target.org_id, repo_id);
                let _ = db.rollback().await;
                return path;
            }
        }

        let _ = db.rollback().await;

        Err(GitError::Forbidden(
            "no read access to this repository's workspace".into(),
        ))
    }
}

/// Decode Base64-encoded HTTP Basic credentials.
fn base64_decode_basic(encoded: &str) -> Result<String, GitError> {
    use base64::Engine;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .map_err(|_| GitError::Unauthorized("invalid base64 in Basic auth".into()))?;
    String::from_utf8(bytes)
        .map_err(|_| GitError::Unauthorized("non-UTF-8 Basic auth credentials".into()))
}

// ---------------------------------------------------------------------------
// Query / Path params
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct InfoRefsParams {
    pub service: String,
}

#[derive(Debug, Deserialize)]
pub struct RepoPathParams {
    #[serde(rename = "repoId")]
    pub repo_id: Uuid,
}

// ---------------------------------------------------------------------------
// Router
// ---------------------------------------------------------------------------

/// Git smart HTTP router — mounted at `/git/:repoId`.
///
/// These routes are auth-exempt from the global middleware; authentication
/// is handled inline by [`GitAuth`].
pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/info/refs", get(info_refs))
        .route("/git-upload-pack", post(upload_pack))
        .route("/git-receive-pack", post(receive_pack))
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

/// `GET /git/:repoId/info/refs?service=git-upload-pack|git-receive-pack`
///
/// Returns the ref advertisement for clone/fetch or push discovery.
async fn info_refs(
    State(state): State<Arc<AppState>>,
    Path(params): Path<RepoPathParams>,
    Query(qs): Query<InfoRefsParams>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, GitError> {
    let jwt_validator = state
        .jwt_validator
        .as_ref()
        .ok_or_else(|| GitError::Internal("JWT validator not configured".into()))?;
    let auth = GitAuth::from_headers(&headers, jwt_validator)?;

    // Validate service parameter — only the two standard git services.
    let service = &qs.service;
    if service != "git-upload-pack" && service != "git-receive-pack" {
        return Err(GitError::BadRequest(format!("unknown service '{service}'")));
    }

    // Authorize the read, then resolve the repo path in its organization.
    let repo_path = auth.require_read(&state, params.repo_id).await?;
    if service == "git-receive-pack" {
        auth.require_push(&state, params.repo_id).await?;
    }

    let output = read_protocol_for_actor(&state, &auth, params.repo_id)
        .await?
        .advertise_refs(&repo_path, git_service_subcommand(service)?)
        .await
        .map_err(GitError::Native)?;

    let content_type = format!("application/x-{service}-advertisement");
    let mut body = pkt_line_header(service);
    body.extend_from_slice(&output);

    Ok((StatusCode::OK, [(header::CONTENT_TYPE, content_type)], body))
}

/// `POST /git/:repoId/git-upload-pack`
///
/// Handles clone / fetch requests by streaming the pack data.
async fn upload_pack(
    State(state): State<Arc<AppState>>,
    Path(params): Path<RepoPathParams>,
    headers: HeaderMap,
    body: Body,
) -> Result<impl IntoResponse, GitError> {
    let jwt_validator = state
        .jwt_validator
        .as_ref()
        .ok_or_else(|| GitError::Internal("JWT validator not configured".into()))?;
    let auth = GitAuth::from_headers(&headers, jwt_validator)?;
    // Read access check; the path is resolved only once it passes.
    let repo_path = auth.require_read(&state, params.repo_id).await?;

    let membership = if auth.workspace_incarnation.is_none() && auth.host_source.is_none() {
        let user = auth.actor_uuid()?;
        let mut db = scoped_db_for_user_actor_all_orgs(&state, user, &[]).await?;
        let targets = repo_workspace_targets(db.conn(), params.repo_id).await?;
        let org = targets.first().ok_or(GitError::RepoNotFound)?.org_id;
        Some(
            crate::services::identity::membership_activity::MembershipActivityGuard::capture(
                &state.system_pool,
                org,
                user,
            )
            .await
            .map_err(|_| GitError::Forbidden("organization access changed".into()))?,
        )
    } else {
        None
    };
    let decoded_body = decode_git_request_body(&headers, body).await?;

    // Spawn git upload-pack, stream stdin/stdout.
    let stream = read_protocol_for_actor(&state, &auth, params.repo_id)
        .await?
        .upload_pack(&repo_path, decoded_body)
        .await
        .map_err(GitError::Native)?;
    use futures_util::StreamExt as _;
    let chunks =
        tokio_util::io::ReaderStream::with_capacity(stream, 256 * 1024).then(move |chunk| {
            let membership = membership.clone();
            async move {
                if let Some(guard) = membership {
                    if !guard.authorize().await {
                        return Err(std::io::Error::other("Organization access changed"));
                    }
                }
                chunk
            }
        });
    let response_body = Body::from_stream(chunks);

    Ok((
        StatusCode::OK,
        [(
            header::CONTENT_TYPE,
            "application/x-git-upload-pack-result".to_string(),
        )],
        response_body,
    ))
}

/// `POST /git/:repoId/git-receive-pack`
///
/// Update authorized workspace or assigned execution branches and publish a hint.
/// Git owns the ref CAS; no head-operation or projection row is written here.
async fn receive_pack(
    State(state): State<Arc<AppState>>,
    Path(params): Path<RepoPathParams>,
    headers: HeaderMap,
    body: Body,
) -> Result<Response, GitError> {
    // Dropping an HTTP request must not release admission while receive-pack
    // still owns a ref mutation. This task owns both admission and completion;
    // SCM's deadline bounds Git execution and reaps it before admission ends.
    tokio::spawn(async move {
        receive_pack_owned(state, params, headers, body)
            .await
            .map(IntoResponse::into_response)
    })
    .await
    .map_err(|error| GitError::Native(g5_scm::native::NativeGitError {
        operation: "receive-pack completion",
        target: None,
        kind: g5_scm::native::FailureKind::Io,
        effect: g5_scm::native::FailureEffect::Unknown,
        detail: format!("Push completion task failed: {error}; inspect current refs and use Update PR if publication needs recovery"),
    }))?
}

async fn receive_pack_owned(
    state: Arc<AppState>,
    params: RepoPathParams,
    headers: HeaderMap,
    body: Body,
) -> Result<impl IntoResponse, GitError> {
    let jwt_validator = state
        .jwt_validator
        .as_ref()
        .ok_or_else(|| GitError::Internal("JWT validator not configured".into()))?;
    let auth = GitAuth::from_headers(&headers, jwt_validator)?;
    let repo_path = auth.require_read(&state, params.repo_id).await?;
    auth.require_push(&state, params.repo_id).await?;
    if headers.contains_key("X-G5-Force-Push") {
        return Err(GitError::Forbidden(
            "X-G5-Force-Push is not supported".into(),
        ));
    }
    // Repository authorization must precede collecting or expanding the body.
    // Exact-ref policy still follows parsing and is rechecked under admission.
    let decoded_body = decode_git_request_body(&headers, body).await?;
    let ref_updates = parse_receive_pack_ref_updates(&decoded_body)?;
    let targets = auth
        .require_push_ref_updates(&state, params.repo_id, &ref_updates)
        .await?;
    let admission = acquire_push_admission(&state, &auth, &ref_updates, &targets).await?;

    // One runner serves every observation of this request.
    let observer = native_observer(&state.git_bin)?;
    let mut immutable_replays = 0_usize;
    for (update, target) in ref_updates.iter().zip(&targets) {
        if observer
            .symbolic_ref_target(&repo_path, &update.ref_name)
            .await
            .map_err(GitError::Native)?
            .is_some()
        {
            return Err(GitError::Forbidden(
                "symbolic workspace refs cannot be pushed".into(),
            ));
        }
        let current = current_ref_tip(&observer, &repo_path, &update.ref_name).await?;
        let immutable_replay = target.immutable_create
            && update.old_sha.is_zero()
            && current.as_deref() == Some(update.new_sha.0.as_str());
        let matches = match current {
            Some(ref tip) => !update.old_sha.is_zero() && tip == &update.old_sha.0,
            None => update.old_sha.is_zero(),
        } || immutable_replay;
        immutable_replays += usize::from(immutable_replay);
        if current.is_none() && !target.immutable_create {
            return Err(GitError::Forbidden(format!(
                "workspace branch creation is reserved for the fork service: {}",
                update.ref_name
            )));
        }
        if !matches {
            return Err(GitError::Conflict(format!(
                "stale ref update to {}",
                update.ref_name
            )));
        }
    }

    if !ref_updates.is_empty() && immutable_replays == ref_updates.len() {
        admission.commit().await?;
        return Ok((
            StatusCode::OK,
            [(
                header::CONTENT_TYPE,
                "application/x-git-receive-pack-result".to_string(),
            )],
            successful_receive_pack_replay(&ref_updates, &decoded_body),
        ));
    }

    let admitted: Vec<_> = ref_updates
        .iter()
        .zip(&targets)
        .map(|(update, target)| g5_scm::native::ReceiveRefUpdate {
            reference: update.ref_name.clone(),
            old_oid: update.old_sha.0.clone(),
            new_oid: update.new_sha.0.clone(),
            fast_forward_only: target.fast_forward_only,
        })
        .collect();
    let output = native_protocol(&state.git_bin)?
        .receive_pack(&repo_path, decoded_body, &admitted)
        .await
        .map_err(GitError::Native)?;

    // Process failure can coexist with accepted commands. Finish each known
    // effect's existing continuation before returning the actionable error.
    let mut stale = None;
    let mut observation_error = None;
    let mut confirmed_refs = Vec::new();
    let mut accepted_updates = Vec::new();
    // Workspace branch heads this push moved, by workspace: reviewers with a
    // live copy of that workspace are told once the push is admitted.
    let mut moved: std::collections::BTreeMap<
        (Uuid, Uuid),
        Vec<crate::routes::workspace_reviews::SourceHeadMove>,
    > = std::collections::BTreeMap::new();
    for (update, target) in ref_updates.iter().zip(targets) {
        let reported_ok = output
            .statuses
            .iter()
            .any(|status| status.reference == update.ref_name && status.accepted);
        // A protocol-confirmed ref needs no subsequent read to establish its
        // acceptance. Needed fallback failures must not discard accepted siblings.
        let current = if reported_ok {
            None
        } else {
            match current_ref_tip(&observer, &repo_path, &update.ref_name).await {
                Ok(current) => current,
                Err(GitError::Native(error)) => {
                    observation_error.get_or_insert(error);
                    continue;
                }
                Err(error) => return Err(error),
            }
        };
        // A same-OID immutable create remains an accepted replay even if Git
        // refused that command after another writer created the same leaf.
        let accepted = reported_ok
            || ((!output.has_report || target.immutable_create)
                && current.as_deref() == Some(update.new_sha.0.as_str()));
        if accepted {
            confirmed_refs.push(update.ref_name.clone());
            if target.immutable_create {
                continue;
            }
            // A branch whose tip did not change (same old and new commit)
            // is accepted by git but moved nowhere: no round is outdated and
            // no reviewer is told.
            if update.ref_name.starts_with("refs/heads/")
                && !update.old_sha.0.eq_ignore_ascii_case(&update.new_sha.0)
            {
                moved
                    .entry((target.workspace_id, target.org_id))
                    .or_default()
                    .push(crate::routes::workspace_reviews::SourceHeadMove {
                        branch: target.branch_name.clone(),
                        new_head: update.new_sha.0.clone(),
                    });
            }
            accepted_updates.push((update, target));
        } else if current.as_deref() != Some(update.old_sha.0.as_str())
            && !(current.is_none() && update.old_sha.is_zero())
        {
            // The ref can move after our preflight too. Git's locked CAS is
            // definitive; preserve the HTTP conflict contract for that race.
            stale = Some(update.ref_name.clone());
        }
    }

    // Capture each complete workspace cut while push admission still holds
    // its commit fence. This transaction writes observation evidence only;
    // a failure cannot undo Git acceptance and is reported as a history gap.
    for (workspace_id, org_id) in moved.keys() {
        let evidence: Result<(), GitError> = async {
            let mut db = scoped_db_for_user_actor_all_orgs(&state, auth.actor_uuid()?, &[]).await?;
            let heads = crate::services::git::workspace_revision_history::published_heads(
                &mut db,
                &state.repo_storage,
                *workspace_id,
            )
            .await
            .map_err(|e| GitError::Internal(e.to_string()))?;
            let repos = state.repo_storage.scoped(&db);
            crate::services::git::workspace_revision_history::record(
                db.conn(),
                &repos,
                *org_id,
                *workspace_id,
                heads,
                chrono::Utc::now(),
                &[params.repo_id.to_string()],
            )
            .await
            .map_err(|e| GitError::Internal(e.to_string()))?;
            db.commit()
                .await
                .map_err(|e| GitError::Internal(e.to_string()))
        }
        .await;
        if let Err(error) = evidence {
            tracing::warn!(?error, %workspace_id, "Accepted push has no retained revision evidence");
        }
    }

    // Git has finished and accepted refs are captured under admission. Release
    // the workspace/VM locks before any independent database transaction: an
    // integration finalizer can own the queue row while waiting for this same
    // workspace, so enqueueing with admission held would deadlock across the
    // two request transactions. Reviewer notifications must run after release
    // too, including when only some refs in this push were accepted.
    let mut continuation_failures = Vec::new();
    let admission_released = match admission.commit().await {
        Ok(()) => true,
        Err(error) => {
            tracing::warn!(?error, "accepted Git push admission release failed");
            continuation_failures.push("release push admission");
            false
        }
    };
    for (update, target) in accepted_updates {
        // Keep each confirmed ref's signals even if an independent publication
        // continuation fails. Do not retry or hold admission across these DBs.
        if admission_released {
            let publication: Result<(), GitError> = async {
                let mut db =
                    scoped_db_for_user_actor_all_orgs(&state, auth.actor_uuid()?, &[]).await?;
                crate::services::remote_integrations::enqueue_after_push(
                    db.conn(),
                    g5_contracts::types::WorkspaceRepoKey {
                        org_id: target.org_id,
                        workspace_id: target.workspace_id,
                        repo_id: params.repo_id,
                    },
                )
                .await
                .map_err(|e| GitError::Internal(format!("provider sync enqueue: {e}")))?;
                db.commit()
                    .await
                    .map_err(|e| GitError::Internal(format!("provider sync commit: {e}")))
            }
            .await;
            if let Err(error) = publication {
                tracing::warn!(?error, workspace_id=%target.workspace_id, "accepted Git push publication continuation failed");
                continuation_failures.push("schedule provider publication");
            }
        }
        state.event_bus.publish(
            &Channel::Workspace(target.workspace_id),
            &WsEvent::WorkspaceBranchUpdated {
                workspace_id: target.workspace_id,
                repo_id: params.repo_id,
                branch: target.branch_name,
                head_commit: update.new_sha.0.clone(),
                hint: Some(BranchUpdateHint {
                    ref_name: update.ref_name.clone(),
                    old: update.old_sha.0.clone(),
                    new: update.new_sha.0.clone(),
                    actor: auth.actor_id.clone(),
                }),
            },
        );
    }
    // The branch moved: rounds reviewed against the old head are outdated
    // and every reviewer's copy wants an Update. Never in a way that can
    // fail the push, and also when a sibling ref of the same push was stale:
    // the accepted refs already moved and their branch events went out
    // above, so the reviewers of those workspaces are told the same way.
    let notify_moved = || async {
        if !admission_released {
            return;
        }
        if let Ok(actor_user_id) = auth.actor_uuid() {
            for ((workspace_id, org_id), moves) in &moved {
                crate::routes::workspace_reviews::notify_reviewers_of_source_head(
                    &state,
                    *org_id,
                    *workspace_id,
                    moves,
                    actor_user_id,
                )
                .await;
            }
        }
    };
    if !continuation_failures.is_empty() {
        notify_moved().await;
        if let Some(error) = observation_error.as_ref().or(output.failure.as_ref()) {
            tracing::warn!(%error, "Git execution also failed before continuation");
        }
        let error = g5_scm::native::NativeGitError {
            operation: "receive-pack continuation",
            target: None,
            kind: g5_scm::native::FailureKind::Io,
            effect: if confirmed_refs.is_empty() { g5_scm::native::FailureEffect::Unknown } else { g5_scm::native::FailureEffect::Partial },
            detail: format!("Git finished; confirmed accepted refs: {}; failed to {}; inspect current refs before retrying; use Update PR to recover publication",
                if confirmed_refs.is_empty() { "none confirmed".into() } else { confirmed_refs.join(", ") },
                continuation_failures.join("; ")),
        };
        return Err(GitError::NativeCompletion {
            error,
            confirmed_refs,
        });
    }
    if let Some(mut error) = observation_error {
        notify_moved().await;
        error.operation = "receive-pack result verification";
        error.effect = if confirmed_refs.is_empty() {
            g5_scm::native::FailureEffect::Unknown
        } else {
            g5_scm::native::FailureEffect::Partial
        };
        error.detail = format!(
            "receive-pack finished; confirmed accepted refs: {}; other ref outcomes remain unverified: {}; inspect current refs before retrying; use Update PR if publication needs recovery",
            if confirmed_refs.is_empty() { "none confirmed".into() } else { confirmed_refs.join(", ") },
            error.detail,
        );
        return Err(GitError::NativeCompletion {
            error,
            confirmed_refs,
        });
    }
    if let Some(mut error) = output.failure {
        notify_moved().await;
        if !confirmed_refs.is_empty() {
            error.effect = g5_scm::native::FailureEffect::Partial;
        }
        error.detail = format!("{}; confirmed accepted refs: {}; inspect current refs before retrying; use Update PR if publication needs recovery", error.detail,
            if confirmed_refs.is_empty() { "none confirmed".into() } else { confirmed_refs.join(", ") });
        return Err(GitError::NativeCompletion {
            error,
            confirmed_refs,
        });
    }
    if let Some(ref_name) = stale {
        notify_moved().await;
        return Err(GitError::Conflict(format!(
            "stale ref update to {ref_name}"
        )));
    }
    notify_moved().await;

    Ok((
        StatusCode::OK,
        [(
            header::CONTENT_TYPE,
            "application/x-git-receive-pack-result".to_string(),
        )],
        output.response,
    ))
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// The storage path of an authorized repository, for an organization
/// `scope` covers; another organization reads as a missing repository.
fn repo_path(
    state: &AppState,
    scope: &(impl g5_api_core::services::git::repo_store::OrgScope + ?Sized),
    org_id: Uuid,
    repo_id: Uuid,
) -> Result<PathBuf, GitError> {
    state
        .repo_storage
        .repo_path(scope, org_id, repo_id)
        .map_err(|_| GitError::RepoNotFound)
}

/// User credentials and ordinary owner VMs cannot enumerate or fetch a
/// temporary review's refs. Bootstrap/incarnation authority exposes only its
/// own review and base pins; retained reports never enter this protocol.
async fn read_protocol_for_actor(
    state: &Arc<AppState>,
    auth: &GitAuth,
    repo: Uuid,
) -> Result<g5_scm::native::NativeGit, GitError> {
    let binding = auth
        .workspace_incarnation
        .as_ref()
        .map(|c| (c.org_id, c.workspace_id))
        .or_else(|| {
            auth.host_source
                .as_ref()
                .map(|c| (c.org_id, c.source_workspace_id))
        });
    let mut db = if let Some((org, workspace)) = binding {
        let scope = g5_contracts::types::formal_review_runtime_scope(workspace);
        crate::db::scoped::scoped_db_for_system_org(state, org, &[&scope])
            .await
            .map_err(|e| GitError::Internal(e.to_string()))?
    } else {
        scoped_db_for_user_actor_all_orgs(state, auth.actor_uuid()?, &[]).await?
    };
    let refs = state
        .approval_review_service
        .hidden_git_refs(&mut db, repo, binding.map(|(_, workspace)| workspace))
        .await
        .map_err(|e| GitError::Internal(e.to_string()))?;
    db.rollback()
        .await
        .map_err(|e| GitError::Internal(e.to_string()))?;
    native_protocol(&state.git_bin)?
        .with_hidden_protocol_refs(refs)
        .map_err(GitError::Native)
}

fn native_protocol(binary: &std::path::Path) -> Result<g5_scm::native::NativeGit, GitError> {
    g5_scm::native::NativeGit::api(binary, std::time::Duration::from_secs(300))
        .map_err(GitError::Native)
}

fn successful_receive_pack_replay(updates: &[RefUpdate], request: &[u8]) -> Vec<u8> {
    fn packet(payload: &[u8]) -> Vec<u8> {
        let mut encoded = format!("{:04x}", payload.len() + 4).into_bytes();
        encoded.extend_from_slice(payload);
        encoded
    }

    let mut report = packet(b"unpack ok\n");
    for update in updates {
        report.extend(packet(format!("ok {}\n", update.ref_name).as_bytes()));
    }
    report.extend_from_slice(b"0000");
    if request
        .windows(b"side-band-64k".len())
        .any(|window| window == b"side-band-64k")
    {
        let mut band = Vec::with_capacity(report.len() + 9);
        let mut payload = Vec::with_capacity(report.len() + 1);
        payload.push(1);
        payload.extend(report);
        band.extend(packet(&payload));
        band.extend_from_slice(b"0000");
        band
    } else {
        report
    }
}

fn git_service_subcommand(service: &str) -> Result<g5_scm::native::GitService, GitError> {
    match service {
        "git-upload-pack" => Ok(g5_scm::native::GitService::UploadPack),
        "git-receive-pack" => Ok(g5_scm::native::GitService::ReceivePack),
        _ => Err(GitError::BadRequest(format!("unknown service '{service}'"))),
    }
}

/// Called only after repository authorization. A raw Body extractor leaves the
/// wire bytes uncollected until this explicit limit can be applied.
async fn decode_git_request_body(headers: &HeaderMap, body: Body) -> Result<Vec<u8>, GitError> {
    let encoding = headers
        .get(header::CONTENT_ENCODING)
        .map(|value| value.to_str())
        .transpose()
        .map_err(|_| GitError::BadRequest("invalid Content-Encoding header".into()))?
        .unwrap_or("identity")
        .trim()
        .to_ascii_lowercase();
    if !matches!(
        encoding.as_str(),
        "" | "identity" | "gzip" | "x-gzip" | "deflate"
    ) {
        return Err(GitError::BadRequest(format!(
            "unsupported Content-Encoding '{encoding}' for git smart HTTP request"
        )));
    }
    let body = to_bytes(body, GIT_SMART_HTTP_BODY_LIMIT_BYTES)
        .await
        .map_err(|error| {
            use std::error::Error;
            if error
                .source()
                .is_some_and(|source| source.is::<http_body_util::LengthLimitError>())
            {
                GitError::PayloadTooLarge
            } else {
                GitError::BadRequest(format!("invalid git request body: {error}"))
            }
        })?;
    if matches!(encoding.as_str(), "" | "identity") {
        // Reuse uniquely owned storage when possible instead of always copying.
        return Ok(body.into());
    }

    let permit = GIT_DECODE_SLOTS
        .try_acquire()
        .map_err(|_| GitError::DecodeBusy)?;
    tokio::task::spawn_blocking(move || {
        // A dropped HTTP future cannot free the slot while decoding continues.
        let _permit = permit;
        decode_compressed_git_body(&encoding, &body, GIT_SMART_HTTP_BODY_LIMIT_BYTES)
    })
    .await
    .map_err(|error| GitError::Internal(format!("git request decoder failed: {error}")))?
}

fn decode_compressed_git_body(
    encoding: &str,
    body: &[u8],
    limit: usize,
) -> Result<Vec<u8>, GitError> {
    let reader: Box<dyn Read + '_> = match encoding {
        "gzip" | "x-gzip" => Box::new(GzDecoder::new(body)),
        "deflate" => Box::new(DeflateDecoder::new(body)),
        _ => {
            return Err(GitError::BadRequest(
                "unsupported git request compression".into(),
            ))
        }
    };
    read_decoded_git_body(
        reader,
        encoding,
        body.len().saturating_mul(4).min(limit),
        limit,
    )
}

fn read_decoded_git_body(
    reader: impl Read,
    encoding: &str,
    initial_capacity: usize,
    limit: usize,
) -> Result<Vec<u8>, GitError> {
    let mut decoded = Vec::with_capacity(initial_capacity);
    // Limit the reader itself, so a compression bomb never expands fully. One
    // extra byte distinguishes an exact-limit body from an oversized one.
    reader
        .take(limit as u64 + 1)
        .read_to_end(&mut decoded)
        .map_err(|error| {
            GitError::BadRequest(format!("invalid {encoding} git request body: {error}"))
        })?;
    if decoded.len() > limit {
        return Err(GitError::PayloadTooLarge);
    }
    Ok(decoded)
}

/// Build the pkt-line service header per the git smart HTTP spec.
///
/// Format: `<4-hex-length># service=<service>\n` + flush packet (`0000`).
#[must_use]
pub fn pkt_line_header(service: &str) -> Vec<u8> {
    let line = format!("# service={service}\n");
    let len = line.len() + 4; // 4 hex digits for the length prefix
    let mut result = format!("{len:04x}{line}").into_bytes();
    result.extend_from_slice(b"0000"); // flush packet
    result
}

/// A single ref update parsed from the pkt-line stream.
#[derive(Debug, Clone)]
pub struct RefUpdate {
    pub old_sha: OidStr,
    pub new_sha: OidStr,
    pub ref_name: String,
}

/// A 40-hex-char SHA-1 object ID (or the zero OID for create/delete).
#[derive(Debug, Clone)]
pub struct OidStr(String);

impl OidStr {
    /// The zero OID (`0000...0000`) indicates ref creation or deletion.
    #[must_use]
    pub fn is_zero(&self) -> bool {
        self.0.chars().all(|c| c == '0')
    }
}

impl std::fmt::Display for OidStr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Parse ref updates from the pkt-line stream in a receive-pack request.
///
/// The format is: `<4-hex-len><old-sha> <new-sha> <ref-name>\n` repeated,
/// terminated by a flush packet (`0000`).  Capabilities on the first line
/// are separated by `\0` and stripped.
pub fn parse_receive_pack_ref_updates(body: &[u8]) -> Result<Vec<RefUpdate>, GitError> {
    let mut updates = Vec::new();
    let mut pos = 0;

    while pos + 4 <= body.len() {
        let len_hex = std::str::from_utf8(&body[pos..pos + 4])
            .map_err(|_| GitError::BadRequest("invalid pkt-line length".into()))?;

        // Flush packet — end of ref updates.
        if len_hex == "0000" {
            break;
        }

        let pkt_len: usize = usize::from_str_radix(len_hex, 16)
            .map_err(|_| GitError::BadRequest(format!("invalid pkt-line length: {len_hex}")))?;

        if pkt_len < 4 || pos + pkt_len > body.len() {
            return Err(GitError::BadRequest("truncated pkt-line".into()));
        }

        let line = std::str::from_utf8(&body[pos + 4..pos + pkt_len])
            .map_err(|_| GitError::BadRequest("non-UTF-8 pkt-line".into()))?;

        // Strip capabilities (separated by \0 on the first line).
        let line = line.split('\0').next().unwrap_or(line).trim();

        if !line.is_empty() {
            // Format: "<old-sha> <new-sha> <ref-name>"
            let parts: Vec<&str> = line.splitn(3, ' ').collect();
            if parts.len() == 3 {
                updates.push(RefUpdate {
                    old_sha: OidStr(parts[0].to_string()),
                    new_sha: OidStr(parts[1].to_string()),
                    ref_name: parts[2].to_string(),
                });
            }
        }

        pos += pkt_len;
    }

    Ok(updates)
}

/// The API selects the executable and per-invocation deadline; SCM owns
/// process context and cleanup. These reads confer no ref-write authority.
fn native_observer(git_bin: &std::path::Path) -> Result<g5_scm::native::NativeGit, GitError> {
    g5_scm::native::NativeGit::api(git_bin, std::time::Duration::from_secs(30))
        .map_err(GitError::Native)
}

/// Resolve a ref under the caller's existing authorization/admission.
async fn current_ref_tip(
    observer: &g5_scm::native::NativeGit,
    repo_path: &std::path::Path,
    ref_name: &str,
) -> Result<Option<String>, GitError> {
    observer
        .ref_tip(repo_path, ref_name)
        .await
        .map_err(GitError::Native)
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct PushTarget {
    workspace_id: Uuid,
    org_id: Uuid,
    branch_name: String,
    immutable_create: bool,
    /// A mount push of the owner's personal branch (`g5.personal.mount-push`):
    /// Git refuses anything but a fast-forward.
    #[sqlx(default)]
    fast_forward_only: bool,
}

/// A database admission lease held for the complete receive-pack effect. The
/// workspace row share lock makes stop/transfer wait until Git has either
/// committed or rejected its ref transaction, and the checks are repeated
/// under that lock so an earlier authorization result cannot outlive ownership.
struct PushAdmission {
    tx: Transaction<'static, Postgres>,
}

impl PushAdmission {
    async fn commit(self) -> Result<(), GitError> {
        self.tx
            .commit()
            .await
            .map_err(|error| GitError::Internal(format!("push admission commit: {error}")))
    }
}

async fn acquire_push_admission(
    state: &Arc<AppState>,
    auth: &GitAuth,
    updates: &[RefUpdate],
    targets: &[PushTarget],
) -> Result<PushAdmission, GitError> {
    let mut workspace_ids: Vec<_> = targets.iter().map(|target| target.workspace_id).collect();
    workspace_ids.sort_unstable();
    workspace_ids.dedup();
    let actor_id = auth.actor_uuid()?;
    let mut tx = state
        .system_pool
        .begin()
        .await
        .map_err(|error| GitError::Internal(format!("begin push admission: {error}")))?;
    // A mount push moves the owner's personal branch (g5.personal.mount-push).
    // Take that workspace's commit fence before any of its rows, the order
    // Start uses, so Start can neither capture heads nor decide restore
    // between this admission and Git's ref transaction.
    if let Some(claims) = auth.workspace_incarnation.as_ref() {
        for workspace_id in workspace_ids
            .iter()
            .filter(|workspace_id| **workspace_id != claims.workspace_id)
        {
            crate::services::branch_commit::acquire_workspace_commit_fence(&mut tx, *workspace_id)
                .await
                .map_err(|error| GitError::Internal(format!("personal commit fence: {error}")))?;
        }
    }

    // Same order as server landings and review captures: fence before rows.
    for workspace_id in &workspace_ids {
        crate::services::branch_commit::acquire_workspace_commit_fence(&mut tx, *workspace_id)
            .await
            .map_err(|e| GitError::Internal(e.to_string()))?;
    }
    for workspace_id in workspace_ids {
        let workspace = sqlx::query(
            "SELECT authority.owner_user_id, durable.is_base, authority.pending_transfer
             FROM authz.lookup_workspace_runtime_authority($1) AS authority
             JOIN LATERAL (
                 SELECT is_base
                 FROM authz.lookup_workspace_runtime_durable_members($1, $2)
                 LIMIT 1
             ) AS durable ON true
             WHERE authority.deleted_at IS NULL",
        )
        .bind(workspace_id)
        .bind(actor_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|error| GitError::Internal(format!("workspace admission lock: {error}")))?
        .ok_or_else(|| GitError::Forbidden("workspace is no longer writable".into()))?;
        if workspace
            .try_get::<bool, _>("is_base")
            .map_err(|error| GitError::Internal(format!("workspace admission shape: {error}")))?
        {
            return Err(GitError::Forbidden("base workspaces are read-only".into()));
        }
        let owner_user_id = workspace
            .try_get::<Option<Uuid>, _>("owner_user_id")
            .map_err(|error| GitError::Internal(format!("workspace admission owner: {error}")))?
            .ok_or_else(|| GitError::Forbidden("workspace owner is unresolved".into()))?;

        let workspace_targets: Vec<_> = targets
            .iter()
            .enumerate()
            .filter(|(_, target)| target.workspace_id == workspace_id)
            .collect();
        let immutable_only = workspace_targets
            .iter()
            .all(|(index, target)| target.immutable_create && updates[*index].old_sha.is_zero());
        let pending_transfer =
            workspace
                .try_get::<bool, _>("pending_transfer")
                .map_err(|error| {
                    GitError::Internal(format!("workspace transfer admission: {error}"))
                })?;
        if pending_transfer && !immutable_only {
            return Err(GitError::Forbidden("workspace transfer is pending".into()));
        }

        // Ordinary branch publication uses current owner authority. Immutable
        // lifecycle snapshots retain the incarnation-bound saving path.
        if !immutable_only {
            let active: bool =
                sqlx::query_scalar("SELECT authz.lock_workspace_member_activity($1)")
                    .bind(workspace_id)
                    .fetch_one(&mut *tx)
                    .await
                    .map_err(|error| {
                        GitError::Internal(format!("membership admission: {error}"))
                    })?;
            if !active {
                return Err(GitError::Forbidden("workspace owner is inactive".into()));
            }
        }

        if let Some(claims) = auth.workspace_incarnation.as_ref() {
            if claims.workspace_id != workspace_id {
                admit_personal_mount_push(state, &mut tx, claims, workspace_id, owner_user_id)
                    .await?;
                continue;
            }
            let live = if claims.workspace_id == workspace_id {
                workspace_incarnation::current_incarnation_on_system(
                    &mut tx,
                    claims,
                    IncarnationLock::Share,
                )
                .await
                .map_err(|error| {
                    GitError::Internal(format!("workspace incarnation admission: {error}"))
                })?
            } else {
                None
            };
            let Some(live) = live else {
                return Err(GitError::Forbidden(
                    "workspace incarnation is no longer current".into(),
                ));
            };
            // Lifecycle refs land while the VM runs or stops, not while it
            // starts; branch pushes only while it runs.
            if immutable_only {
                if live.status == "starting" {
                    return Err(GitError::Forbidden(
                        "workspace incarnation is no longer current".into(),
                    ));
                }
            } else {
                require_running(&live.status)?;
            }
            continue;
        }
        if immutable_only {
            return Err(GitError::Forbidden(
                "immutable lifecycle refs require workspace authority".into(),
            ));
        }

        let status: String = sqlx::query_scalar(
            "SELECT status FROM workspace_vms WHERE workspace_id = $1 FOR SHARE",
        )
        .bind(workspace_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|error| GitError::Internal(format!("workspace VM admission lock: {error}")))?
        .ok_or_else(|| GitError::Forbidden("WORKSPACE_NOT_RUNNING".into()))?;
        // Read with the row locked: PostgreSQL rechecks a row it waited for
        // only if the row changed, so a lease that ran out during the wait
        // would pass a check made in the locking statement. NULL once the
        // incarnation is released (no lease at all): as stale as an expired
        // lease, not a server fault.
        let lease_valid: Option<bool> = sqlx::query_scalar(
            "SELECT lease_expires_at > clock_timestamp() FROM workspace_vms WHERE workspace_id = $1",
        )
        .bind(workspace_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(|error| GitError::Internal(format!("workspace VM lease: {error}")))?;
        if !lease_valid.unwrap_or(false) {
            return Err(GitError::Forbidden(
                "workspace incarnation lease expired".into(),
            ));
        }
        require_running(&status)?;
        if actor_id != owner_user_id {
            return Err(GitError::Forbidden(
                "only the current workspace owner may push".into(),
            ));
        }
    }
    Ok(PushAdmission { tx })
}

/// Admit a mount push into the owner's personal workspace `personal_id`
/// (g5.personal.mount-push), inside push admission's transaction, which
/// already holds that workspace's commit fence. The pusher must be a current,
/// running incarnation of a workspace that is not personal, owned by the
/// personal workspace's owner; the workspace lifecycle decides whether the
/// personal workspace's state admits the push and holds its VM row until Git
/// commits or refuses. A busy personal workspace refuses, never queues.
async fn admit_personal_mount_push(
    state: &Arc<AppState>,
    tx: &mut Transaction<'static, Postgres>,
    claims: &WorkspaceIncarnationClaims,
    personal_id: Uuid,
    personal_owner: Uuid,
) -> Result<(), GitError> {
    use crate::services::workspace_vm_service::{admit_personal_mount_push, PersonalMountPush};
    if personal_owner != claims.owner_user_id {
        return Err(GitError::Forbidden(
            "the personal repository belongs to another owner".into(),
        ));
    }
    let live =
        workspace_incarnation::current_incarnation_on_system(tx, claims, IncarnationLock::Share)
            .await
            .map_err(|error| {
                GitError::Internal(format!("workspace incarnation admission: {error}"))
            })?
            .ok_or_else(|| {
                GitError::Forbidden("workspace incarnation is no longer current".into())
            })?;
    require_running(&live.status)?;
    match admit_personal_mount_push(state, tx, claims, personal_id).await {
        Ok(PersonalMountPush::Admitted) => Ok(()),
        Ok(PersonalMountPush::NotTheOwners) => Err(GitError::Forbidden(
            "only the owner's other workspaces push the personal branch".into(),
        )),
        Ok(PersonalMountPush::PersonalBusy) => Err(GitError::Conflict(
            "the personal workspace is starting or stopping, or is stopped with unsaved Git work; \
             start it, push from it, then retry"
                .into(),
        )),
        Err(error) => Err(GitError::Internal(format!(
            "personal workspace admission: {error}"
        ))),
    }
}

fn require_running(status: &str) -> Result<(), GitError> {
    match status {
        "running" => Ok(()),
        "stopping" => Err(GitError::Forbidden("WORKSPACE_STOPPING".into())),
        _ => Err(GitError::Forbidden("WORKSPACE_NOT_RUNNING".into())),
    }
}

#[derive(Debug, sqlx::FromRow)]
struct BranchAdvanceTarget {
    workspace_id: Uuid,
    org_id: Uuid,
}

/// The token's current incarnation, if `repo_id` is one of its workspace's
/// repositories.
async fn current_incarnation_repo_target(
    state: &Arc<AppState>,
    claims: &WorkspaceIncarnationClaims,
    repo_id: Uuid,
) -> Result<LiveIncarnation, GitError> {
    let mut conn =
        state.system_pool.acquire().await.map_err(|error| {
            GitError::Internal(format!("workspace incarnation lookup: {error}"))
        })?;
    let target = workspace_incarnation::current_incarnation_on_system(
        &mut conn,
        claims,
        IncarnationLock::None,
    )
    .await
    .map_err(|error| GitError::Internal(format!("workspace incarnation lookup: {error}")))?
    .ok_or_else(|| GitError::Forbidden("workspace incarnation is no longer current".into()))?;
    if !target.repo_ids.contains(&repo_id) {
        return Err(GitError::Forbidden(
            "repository is not a workspace member".into(),
        ));
    }
    Ok(target)
}

async fn require_current_workspace_incarnation(
    state: &Arc<AppState>,
    claims: &WorkspaceIncarnationClaims,
    repo_id: Uuid,
) -> Result<(), GitError> {
    match current_incarnation_repo_target(state, claims, repo_id).await {
        Ok(_) => Ok(()),
        // A personal workspace also reads Base's members as reference
        // checkouts (g5.workspace.identity.reference); never writes them.
        // Any other workspace reads its owner's mounted personal repository
        // (g5.personal.mount).
        Err(GitError::Forbidden(reason)) => {
            let readable = workspace_incarnation::personal_read_allowed_for_incarnation(
                state, claims, repo_id,
            )
            .await
            .map_err(|error| {
                GitError::Internal(format!("reference read lookup: {}", error.inner))
            })?;
            if readable {
                Ok(())
            } else {
                Err(GitError::Forbidden(reason))
            }
        }
        Err(error) => Err(error),
    }
}

/// The owner's personal workspace when `repo_id` is the incarnation's mounted
/// personal repository (`g5.personal.mount`).
async fn incarnation_personal_mount(
    state: &Arc<AppState>,
    claims: &WorkspaceIncarnationClaims,
    repo_id: Uuid,
) -> Result<Option<workspace_incarnation::PersonalMountTarget>, GitError> {
    workspace_incarnation::personal_mount_target_for_incarnation(state, claims, repo_id)
        .await
        .map_err(|error| GitError::Internal(format!("personal mount lookup: {}", error.inner)))
}

/// A mount push moves only the personal branch, update-only, and only as a
/// fast-forward (`g5.personal.mount-push`). The mount has no lifecycle refs.
fn require_mount_ref_updates(
    mount: &workspace_incarnation::PersonalMountTarget,
    updates: &[RefUpdate],
) -> Result<Vec<PushTarget>, GitError> {
    updates
        .iter()
        .map(|update| {
            let branch = update.ref_name.strip_prefix("refs/heads/").ok_or_else(|| {
                GitError::Forbidden(format!(
                    "the personal repository accepts only its branch: {}",
                    update.ref_name
                ))
            })?;
            if branch != mount.branch_name {
                return Err(GitError::Forbidden(format!(
                    "workspace incarnation cannot update branch {branch}"
                )));
            }
            if update.old_sha.is_zero() || update.new_sha.is_zero() {
                return Err(GitError::Forbidden(
                    "workspace branches are update-only".into(),
                ));
            }
            Ok(PushTarget {
                workspace_id: mount.workspace_id,
                org_id: mount.org_id,
                branch_name: mount.branch_name.clone(),
                immutable_create: false,
                fast_forward_only: true,
            })
        })
        .collect()
}

async fn require_incarnation_ref_updates(
    state: &Arc<AppState>,
    claims: &WorkspaceIncarnationClaims,
    repo_id: Uuid,
    updates: &[RefUpdate],
) -> Result<Vec<PushTarget>, GitError> {
    let target = match current_incarnation_repo_target(state, claims, repo_id).await {
        Ok(target) => target,
        Err(GitError::Forbidden(reason)) => {
            return match incarnation_personal_mount(state, claims, repo_id).await? {
                Some(mount) => require_mount_ref_updates(&mount, updates),
                None => Err(GitError::Forbidden(reason)),
            };
        }
        Err(error) => return Err(error),
    };
    // A Base workspace never starts a VM, so a current token has a branch.
    let branch_name = target
        .branch_name
        .ok_or_else(|| GitError::Forbidden("base workspaces are read-only".into()))?;
    let mut authorized = Vec::with_capacity(updates.len());
    for update in updates {
        if let Some(branch) = update.ref_name.strip_prefix("refs/heads/") {
            if branch != branch_name {
                return Err(GitError::Forbidden(format!(
                    "workspace incarnation cannot update branch {branch}"
                )));
            }
            if update.old_sha.is_zero() || update.new_sha.is_zero() {
                return Err(GitError::Forbidden(
                    "workspace branches are update-only".into(),
                ));
            }
            authorized.push(PushTarget {
                workspace_id: claims.workspace_id,
                org_id: claims.org_id,
                branch_name: branch_name.clone(),
                immutable_create: false,
                fast_forward_only: false,
            });
            continue;
        }

        let Some((flush_id, leaf)) = parse_flush_ref(&update.ref_name, claims.workspace_id) else {
            return Err(GitError::Forbidden(format!(
                "network push namespace denied: {}",
                update.ref_name
            )));
        };
        if !matches!(leaf, "snapshot" | "branch")
            || !update.old_sha.is_zero()
            || update.new_sha.is_zero()
        {
            return Err(GitError::Forbidden(
                "flush refs are immutable create-only leaves".into(),
            ));
        }
        let known: bool = sqlx::query_scalar(
            r#"
            SELECT EXISTS (
                SELECT 1 FROM workspace_flushes
                WHERE flush_id = $1 AND workspace_id = $2 AND org_id = $3
                  AND origin = 'flush' AND vm_id = $4 AND epoch = $5
                  AND error IS NULL
                  AND $6 = ANY(required_repo_ids)
                  AND (
                      complete_at IS NULL
                      OR EXISTS (
                          SELECT 1 FROM jsonb_array_elements(repos) AS repo
                          WHERE (repo ->> 'repo_id')::uuid = $6
                            AND CASE $7
                                WHEN 'snapshot' THEN repo ->> 'snapshot_oid'
                                WHEN 'branch' THEN repo ->> 'local_branch_oid'
                                ELSE NULL
                            END = $8
                      )
                  )
            )
            "#,
        )
        .bind(flush_id)
        .bind(claims.workspace_id)
        .bind(claims.org_id)
        .bind(claims.vm_id.to_string())
        .bind(claims.epoch as i64)
        .bind(repo_id)
        .bind(leaf)
        .bind(update.new_sha.0.as_str())
        .fetch_one(&state.system_pool)
        .await
        .map_err(|error| GitError::Internal(format!("flush ref lookup: {error}")))?;
        if !known {
            return Err(GitError::Forbidden("FLUSH_REF_UNKNOWN".into()));
        }
        authorized.push(PushTarget {
            workspace_id: claims.workspace_id,
            org_id: claims.org_id,
            branch_name: branch_name.clone(),
            immutable_create: true,
            fast_forward_only: false,
        });
    }
    Ok(authorized)
}

fn parse_flush_ref(ref_name: &str, workspace_id: Uuid) -> Option<(Uuid, &str)> {
    let rest = ref_name.strip_prefix("refs/g5/flush/")?;
    let mut parts = rest.split('/');
    let workspace = Uuid::parse_str(parts.next()?).ok()?;
    let flush = Uuid::parse_str(parts.next()?).ok()?;
    let leaf = parts.next()?;
    (parts.next().is_none() && workspace == workspace_id).then_some((flush, leaf))
}

async fn scoped_db_for_user_actor_all_orgs(
    state: &Arc<AppState>,
    user_id: Uuid,
    scopes: &[&str],
) -> Result<ScopedDb, GitError> {
    let actor_id = UserId::from_uuid(user_id);
    let org_ids = state
        .authz
        .user_org_ids(actor_id)
        .await
        .map_err(|e| GitError::Forbidden(format!("user org membership lookup: {e}")))?;
    if org_ids.is_empty() {
        return Err(GitError::Forbidden("user has no org memberships".into()));
    }

    let scope_strings: Vec<String> = scopes.iter().map(|scope| (*scope).to_string()).collect();
    let capability = state
        .authz
        .mint_db_capability(actor_id, &org_ids, &scope_strings)
        .await
        .map_err(|e| GitError::Forbidden(format!("user org scope: {e}")))?;

    ScopedDb::from_capability(&state.tenant_pool, capability)
        .await
        .map_err(|e| GitError::Internal(format!("scoped DB setup: {e}")))
}

async fn repo_workspace_targets(
    conn: &mut sqlx::PgConnection,
    repo_id: Uuid,
) -> Result<Vec<BranchAdvanceTarget>, GitError> {
    sqlx::query_as::<_, BranchAdvanceTarget>(
        r#"
        SELECT m.workspace_id, w.org_id
        FROM workspace_repos m
        JOIN workspaces w ON w.id = m.workspace_id AND w.org_id = m.org_id
        WHERE m.repo_id = $1
          AND w.deleted_at IS NULL
        "#,
    )
    .bind(repo_id)
    .fetch_all(conn)
    .await
    .map_err(|e| GitError::Internal(format!("repo workspace target lookup: {e}")))
}

async fn user_can_write_workspace(
    system_pool: &sqlx::PgPool,
    db: &mut ScopedDb,
    user_id: Uuid,
    org_id: Uuid,
    workspace_id: Uuid,
) -> Result<bool, GitError> {
    if !user_has_workspace_access(system_pool, db, user_id, org_id, workspace_id).await? {
        return Ok(false);
    }
    crate::services::permission_service::PermissionService::user_holds_workspace_scope(
        system_pool,
        user_id,
        org_id,
        workspace_id,
        "workspaces:write",
    )
    .await
    .map_err(|e| GitError::Internal(format!("workspace write scope lookup: {e}")))
}

async fn user_has_workspace_access(
    system_pool: &sqlx::PgPool,
    db: &mut ScopedDb,
    user_id: Uuid,
    org_id: Uuid,
    workspace_id: Uuid,
) -> Result<bool, GitError> {
    let workspace_is_live: bool = sqlx::query_scalar(
        r#"
        SELECT EXISTS(
            SELECT 1
            FROM workspaces
            WHERE id = $1
              AND org_id = $2
              AND deleted_at IS NULL
        )
        "#,
    )
    .bind(workspace_id)
    .bind(org_id)
    .fetch_one(db.conn())
    .await
    .map_err(|e| GitError::Internal(format!("workspace liveness lookup: {e}")))?;
    if !workspace_is_live {
        return Ok(false);
    }

    user_can_access_workspace(system_pool, db, user_id, org_id, workspace_id)
        .await
        .map_err(|e| GitError::Internal(format!("workspace access lookup: {}", e.inner)))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    use axum::http::HeaderValue;
    use flate2::{
        write::{DeflateEncoder, GzEncoder},
        Compression,
    };

    #[tokio::test]
    async fn native_http_errors_keep_effects_without_server_diagnostics() {
        use g5_scm::native::{FailureEffect, FailureKind, NativeGitError};
        for effect in [
            FailureEffect::Refused,
            FailureEffect::Partial,
            FailureEffect::Unknown,
        ] {
            for continuation in [false, true] {
                let error = NativeGitError {
                    operation: "receive-pack",
                    target: Some("/srv/private/repos/secret.git".into()),
                    kind: FailureKind::Io,
                    effect,
                    detail: "stderr-private-evidence".into(),
                };
                let expected = error.public_message();
                let response = if continuation {
                    GitError::NativeCompletion {
                        error,
                        confirmed_refs: vec!["refs/heads/workspace".into()],
                    }
                } else {
                    GitError::Native(error)
                }
                .into_response();
                assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
                let body = axum::body::to_bytes(response.into_body(), usize::MAX)
                    .await
                    .unwrap();
                let body = String::from_utf8(body.to_vec()).unwrap();
                assert!(body.contains(&expected));
                assert!(!body.contains("/srv/") && !body.contains("stderr-private"));
                if continuation {
                    assert!(body.contains("refs/heads/workspace") && body.contains("Update PR"));
                }
            }
        }
    }

    #[test]
    fn stale_ref_conflict_maps_to_409_distinct_from_auth_403() {
        // A stale ref is a recoverable conflict, not an authorization denial.
        // Git clients may see only the transport status, so receive-pack must
        // preserve 409 versus 403.
        let conflict = GitError::Conflict("stale old SHA for refs/heads/workspace".to_string())
            .into_response();
        assert_eq!(conflict.status(), StatusCode::CONFLICT);

        let forbidden = GitError::Forbidden("missing push scope".to_string()).into_response();
        assert_eq!(forbidden.status(), StatusCode::FORBIDDEN);
    }

    #[test]
    fn test_pkt_line_header_upload_pack() {
        let header = pkt_line_header("git-upload-pack");
        let s = String::from_utf8(header).unwrap();
        // "# service=git-upload-pack\n" = 26 chars + 4 hex prefix = 30 → "001e"
        assert!(s.starts_with("001e# service=git-upload-pack\n"));
        assert!(s.ends_with("0000"));
    }

    #[test]
    fn test_pkt_line_header_receive_pack() {
        let header = pkt_line_header("git-receive-pack");
        let s = String::from_utf8(header).unwrap();
        assert!(s.starts_with("001f# service=git-receive-pack\n"));
        assert!(s.ends_with("0000"));
    }

    #[test]
    fn test_parse_ref_updates_single() {
        // Construct a pkt-line: "00670000...0000 aaaa...aaaa refs/heads/main\0 report-status\n"
        let old = "0".repeat(40);
        let new = "a".repeat(40);
        let line = format!("{old} {new} refs/heads/main\0 report-status\n");
        let pkt_len = line.len() + 4;
        let pkt = format!("{pkt_len:04x}{line}0000");

        let updates = parse_receive_pack_ref_updates(pkt.as_bytes()).unwrap();
        assert_eq!(updates.len(), 1);
        assert_eq!(updates[0].ref_name, "refs/heads/main");
        assert!(updates[0].old_sha.is_zero());
        assert!(!updates[0].new_sha.is_zero());
    }

    #[test]
    fn test_parse_ref_updates_empty_flush() {
        let updates = parse_receive_pack_ref_updates(b"0000").unwrap();
        assert!(updates.is_empty());
    }

    #[test]
    fn test_oid_str_is_zero() {
        assert!(OidStr("0".repeat(40)).is_zero());
        assert!(!OidStr("a".repeat(40)).is_zero());
        assert!(!OidStr(format!("{}a", "0".repeat(39))).is_zero());
    }

    fn compress_git_body(encoding: &str, payload: &[u8]) -> Vec<u8> {
        if encoding == "deflate" {
            let mut encoder = DeflateEncoder::new(Vec::new(), Compression::fast());
            encoder.write_all(payload).unwrap();
            encoder.finish().unwrap()
        } else {
            let mut encoder = GzEncoder::new(Vec::new(), Compression::fast());
            encoder.write_all(payload).unwrap();
            encoder.finish().unwrap()
        }
    }

    #[test]
    fn compressed_git_body_enforces_decoded_boundary_for_every_encoding() {
        let limit = 64 * 1024;
        for encoding in ["gzip", "x-gzip", "deflate"] {
            for size in [0, limit - 1, limit, limit + 1, 16 * limit] {
                let payload = vec![0; size];
                let compressed = compress_git_body(encoding, &payload);
                let result = decode_compressed_git_body(encoding, &compressed, limit);
                if size <= limit {
                    assert_eq!(result.unwrap(), payload, "{encoding}, {size}");
                } else {
                    assert!(
                        matches!(result, Err(GitError::PayloadTooLarge)),
                        "{encoding}, {size}"
                    );
                }
            }
            assert!(matches!(
                decode_compressed_git_body(encoding, &[0xff; 32], limit),
                Err(GitError::BadRequest(_))
            ));
        }
    }

    #[test]
    fn decoded_reader_stops_after_one_byte_over_limit() {
        // An infinite source stands in for an arbitrarily large expansion.
        // Count actual reads, rather than just checking the final status.
        struct CountingReader(usize);
        impl Read for CountingReader {
            fn read(&mut self, output: &mut [u8]) -> std::io::Result<usize> {
                output.fill(0);
                self.0 += output.len();
                Ok(output.len())
            }
        }
        let mut reader = CountingReader(0);
        assert!(matches!(
            read_decoded_git_body(&mut reader, "gzip", 32, 1024),
            Err(GitError::PayloadTooLarge)
        ));
        assert_eq!(reader.0, 1025);
    }

    #[test]
    fn git_body_ingestion_preserves_encodings_and_bounds_blocking_jobs() {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .max_blocking_threads(1)
            .build()
            .unwrap()
            .block_on(async {
                let payload = b"0016want deadbeef\n0000";
                for encoding in [
                    None,
                    Some("identity"),
                    Some(""),
                    Some("gzip"),
                    Some("x-gzip"),
                    Some("deflate"),
                    Some(" GZip "),
                ] {
                    let mut headers = HeaderMap::new();
                    let body = match encoding {
                        Some(encoding) => {
                            headers.insert(
                                header::CONTENT_ENCODING,
                                HeaderValue::from_str(encoding).unwrap(),
                            );
                            if matches!(encoding, "identity" | "") {
                                payload.to_vec()
                            } else {
                                compress_git_body(encoding, payload)
                            }
                        }
                        None => payload.to_vec(),
                    };
                    assert_eq!(
                        decode_git_request_body(&headers, Body::from(body))
                            .await
                            .unwrap(),
                        payload
                    );
                }
                let mut headers = HeaderMap::new();
                headers.insert(header::CONTENT_ENCODING, HeaderValue::from_static("gzip"));
                let permits = GIT_DECODE_SLOTS.try_acquire_many(2).unwrap();
                let body = compress_git_body("gzip", payload);
                assert!(matches!(
                    decode_git_request_body(&headers, Body::from(body.clone())).await,
                    Err(GitError::DecodeBusy)
                ));
                drop(permits);
                // A malformed job releases its permit too, so later valid work succeeds.
                assert!(matches!(
                    decode_git_request_body(&headers, Body::from(vec![0xff; 32])).await,
                    Err(GitError::BadRequest(_))
                ));
                assert_eq!(GIT_DECODE_SLOTS.available_permits(), 2);
                assert_eq!(
                    decode_git_request_body(&headers, Body::from(body))
                        .await
                        .unwrap(),
                    payload
                );
                for encoding in [
                    HeaderValue::from_static("br"),
                    HeaderValue::from_bytes(&[0xff]).unwrap(),
                ] {
                    headers.insert(header::CONTENT_ENCODING, encoding);
                    assert!(matches!(
                        decode_git_request_body(&headers, Body::empty()).await,
                        Err(GitError::BadRequest(_))
                    ));
                }
                assert_eq!(
                    GitError::PayloadTooLarge.into_response().status(),
                    StatusCode::PAYLOAD_TOO_LARGE
                );
                assert_eq!(
                    GitError::DecodeBusy.into_response().status(),
                    StatusCode::SERVICE_UNAVAILABLE
                );

                // Occupy the only blocking worker, then queue a real decode. The async
                // worker must stay responsive, and cancelling its waiter must not free
                // the decode slot until the queued closure actually finishes.
                let (started, ready) = tokio::sync::oneshot::channel();
                let (release, wait) = std::sync::mpsc::channel();
                let blocker = tokio::task::spawn_blocking(move || {
                    started.send(()).unwrap();
                    let _ = wait.recv();
                });
                ready.await.unwrap();
                let job = tokio::spawn(async move {
                    let mut headers = HeaderMap::new();
                    headers.insert(header::CONTENT_ENCODING, HeaderValue::from_static("gzip"));
                    decode_git_request_body(
                        &headers,
                        Body::from(compress_git_body("gzip", b"0000")),
                    )
                    .await
                });
                tokio::time::timeout(std::time::Duration::from_secs(5), async {
                    while GIT_DECODE_SLOTS.available_permits() != 1 {
                        tokio::task::yield_now().await;
                    }
                })
                .await
                .expect("decoder must yield to the async worker");
                assert!(!job.is_finished());
                job.abort();
                assert!(job.await.unwrap_err().is_cancelled());
                assert_eq!(GIT_DECODE_SLOTS.available_permits(), 1);
                release.send(()).unwrap();
                blocker.await.unwrap();
                tokio::time::timeout(std::time::Duration::from_secs(5), async {
                    while GIT_DECODE_SLOTS.available_permits() != 2 {
                        tokio::task::yield_now().await;
                    }
                })
                .await
                .expect("decoder must release its slot on completion");
            });
    }

    #[test]
    fn git_workspace_target_queries_require_live_workspaces() {
        let source = include_str!("git.rs");
        // user_can_write_workspace delegates the tombstone check to
        // user_has_workspace_access before consulting roles and grants, and
        // current_incarnation_repo_target to the lifecycle's incarnation
        // reader (`workspace_incarnation.rs`, whose own test checks it).
        for helper in ["repo_workspace_targets", "user_has_workspace_access"] {
            // Anchor on the definition, not on this test's own list.
            let definition = format!("\nasync fn {helper}(");
            let start = source
                .find(&definition)
                .unwrap_or_else(|| panic!("{helper} is defined in git.rs"));
            let body = &source[start + 1..];
            let end = body.find("\nasync fn ").unwrap_or(body.len());
            let helper_body = &body[..end];
            assert!(
                helper_body.contains("deleted_at IS NULL"),
                "{helper} must exclude tombstoned workspace rows"
            );
        }
    }
}
