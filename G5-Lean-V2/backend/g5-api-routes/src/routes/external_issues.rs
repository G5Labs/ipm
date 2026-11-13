//! Issue domains, issue reads and writes, and workspace issue links, for
//! GitHub and G5's own tracker (`g5.issue.native.adapter`).
//!
//! - Organization-admin domain routes at `/api/v1/orgs/:orgId/issue-domains`
//!   (`Scope::OrgAdmin` plus the path-org admin re-check used by the
//!   GitHub App binding routes).
//! - Organization-member reads at `/api/v1/orgs/:orgId/issue-domains` and
//!   `/api/v1/orgs/:orgId/issues`: org membership is the read grant for issues
//!   in a configured domain.
//! - Workspace associations at `/api/v1/workspaces/:workspaceId/issue-links`.
//!
//! - Native issue page actions at `/api/v1/orgs/:orgId/issues/g5/:number`
//!   (`g5.issue.native.page`).
//!
//! Tracker failures are typed (`error.details.issueError`) and never rendered
//! as empty results. Handlers commit their tenant transaction before any
//! GitHub request and open a new one afterwards; native reads open their own
//! short transactions under the caller's tenant scope.

use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    routing::{get, patch, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use uuid::Uuid;

use crate::db::scoped::{authorize_org_scopes, scoped_db_for_org, scoped_db_for_workspace};
use crate::errors::AppError;
use crate::middleware::AuthContext;
use crate::routes::audited::AuditedRouter;
use crate::routes::github_app::{mask_foreign_org_access, org_admin_db};
use crate::routes::issue_labels;
use crate::routes::orgs::OrgPathParams;
use crate::routes::ResponseEnvelope;
use crate::services::external_issues::{
    attach_primary_owners, create_github_domain, create_native_domain, create_workspace_link,
    delete_domain, domain_assignable_users, domain_for_repository, domain_issue,
    domain_labels_named, domain_login_assignable, domain_parent_issue, domain_repository_audience,
    ensure_external_issues_enabled, fetch_issue_detail, fetch_link_summaries,
    fetch_linked_issue_timeline, find_marked_comment, find_marked_issue, issue_link_refs,
    link_domain, list_domain_access, list_workspace_links, native_domain,
    org_external_issues_enabled, resolve_github_reference, resolve_reference, search_domains,
    summary, unlink_workspace_link, update_domain_settings, workspace_org_id,
    ExternalIssueCommentDTO, ExternalIssueDetailDTO, ExternalIssueSearchResultsDTO,
    ExternalIssueSummaryDTO, ExternalIssueUserDTO, IssueBackends, IssueDomainAccess,
    IssueDomainDTO, IssueErrorDTO, LinkRole, LinkedLocation, NativeBackend, NewDomainProvider,
    SearchAssignee, TenantDbOpener, WorkspaceIssueLinkDTO, WorkspaceIssueLinkRecord,
};
use crate::services::github_app::GithubAppClient;
use crate::services::github_issues::{
    add_sub_issue, create_issue, fetch_issue_comment, post_issue_comment, update_issue,
    update_issue_comment, update_issue_state, CommentPostOutcome, GithubIssue, GithubIssueComment,
    IssueCreateOutcome, IssueEdit, IssueStateChange, IssueUpdateOutcome, RepositoryAudience,
    SubIssueOutcome,
};
use crate::services::github_issues::{parse_canonical_github_id, IssueError, IssueStateFilter};
use crate::services::github_user_grants::{
    acting_user, connected_account, grant_status, ActingGithubUser, UserTokenError,
};
use crate::services::issue_comments::{
    comment_conflict, existing_marker, keyed_marker_id, marker, render_edit_payload,
    render_payload, validate_body, COMMENT_MARKER_WINDOW_DAYS,
};
use crate::services::issue_creations::{
    creation_marker, description_without_marker, edited_description,
    render_payload as render_issue_payload, resolve_assignees, target_reference_for,
    validate_draft, validate_edit, validate_key, CreatedIssueDTO, CreationDestinationDTO,
    CreationParentDTO, IssueCreationDTO, IssueDraft, MARKER_WINDOW_HOURS, MAX_ASSIGNEES,
};
use crate::services::issue_labels::{unknown_labels, validate_selection};
use crate::services::native_issues::{
    self, NativeComment, NativeIssueChange, NativeIssueEdit, NativeReader, NewNativeComment,
    NewNativeIssue, PROVIDER_G5,
};
use crate::services::workspace_activity::{
    commit_entries, compose_page, current_entries, issue_entries, workspace_entries,
    ActivityCursor, ActivityEntryDTO, ACTIVITY_COMMITS_PER_BRANCH, ACTIVITY_PAGE_SIZE,
};
use crate::services::AppState;
use g5_auth::Scope;
use g5_contracts::error::{ExternalIssueErrorKind, G5Error};

const MEMBER_READ_SCOPE: &[&str] = &["workspaces:read"];
const WORKSPACE_READ_SCOPE: &[&str] = &["workspaces:read"];
const WORKSPACE_WRITE_SCOPE: &[&str] = &["workspaces:write"];
const MEMBER_WRITE_SCOPE: &[&str] = &["workspaces:write"];

// ─── Request / response types ────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct IssueDomainPathParams {
    #[serde(rename = "orgId")]
    pub org_id: Uuid,
    #[serde(rename = "domainId")]
    pub domain_id: Uuid,
}

#[derive(Debug, Deserialize)]
pub struct GithubIssuePathParams {
    #[serde(rename = "orgId")]
    pub org_id: Uuid,
    /// GitHub numeric repository id (decimal string).
    #[serde(rename = "repositoryId")]
    pub repository_id: String,
    pub number: String,
}

#[derive(Debug, Deserialize)]
pub struct WorkspaceIssueLinksPathParams {
    #[serde(rename = "workspaceId")]
    pub workspace_id: Uuid,
}

#[derive(Debug, Deserialize)]
pub struct WorkspaceIssueLinkPathParams {
    #[serde(rename = "workspaceId")]
    pub workspace_id: Uuid,
    #[serde(rename = "linkId")]
    pub link_id: Uuid,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreateIssueDomainRequest {
    /// `g5` adds the organization's native domain; absent (or `github`) a
    /// GitHub repository.
    #[serde(default)]
    pub provider: Option<String>,
    /// GitHub numeric repository id as a decimal string.
    #[serde(default)]
    pub github_repo_id: Option<String>,
    /// The native domain's issue key (`ACME`).
    #[serde(default)]
    pub key: Option<String>,
    /// Default: untrusted for GitHub, trusted for G5 issues (only members
    /// and their agents write them).
    #[serde(default)]
    pub content_trusted: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UpdateIssueDomainRequest {
    #[serde(default)]
    pub content_trusted: Option<bool>,
    /// Allow G5 comments on this domain's issues.
    #[serde(default)]
    pub comments_enabled: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct IssueSearchQuery {
    pub q: Option<String>,
    pub state: Option<String>,
    /// `me`: only issues assigned to the caller's connected GitHub account;
    /// `none`: only unassigned issues.
    pub assignee: Option<String>,
}

/// `assignee` of an issue search.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AssigneeParam {
    Any,
    Me,
    Unassigned,
}

#[derive(Debug, Deserialize)]
pub struct IssueResolveQuery {
    #[serde(rename = "ref")]
    pub reference: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreateWorkspaceIssueLinkRequest {
    #[serde(rename = "ref")]
    pub reference: String,
    pub role: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IssueDomainListDTO {
    pub domains: Vec<IssueDomainDTO>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceIssueLinkListDTO {
    pub links: Vec<WorkspaceIssueLinkDTO>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UnlinkWorkspaceIssueLinkDTO {
    pub unlinked: bool,
}

// ─── Routers ─────────────────────────────────────────────────────────────────

/// Nested at `/api/v1/orgs/:orgId/issue-domains`.
pub fn domain_router() -> Router<Arc<AppState>> {
    AuditedRouter::new()
        .audited_route("/", get(list_issue_domains), &[Scope::WorkspacesRead])
        .audited_route("/", post(create_issue_domain), &[Scope::OrgAdmin])
        .audited_route(
            "/:domainId",
            patch(update_issue_domain).delete(delete_issue_domain),
            &[Scope::OrgAdmin],
        )
        .audited_route(
            "/:domainId/export",
            get(export_issue_domain),
            &[Scope::OrgAdmin],
        )
        .into_router()
        .merge(issue_labels::domain_label_router())
}

/// Nested at `/api/v1/orgs/:orgId/issues`.
pub fn org_issue_router() -> Router<Arc<AppState>> {
    AuditedRouter::new()
        .audited_route("/", post(create_org_issue), &[Scope::WorkspacesWrite])
        .audited_route(
            "/create-target",
            get(get_create_target),
            &[Scope::WorkspacesRead],
        )
        .audited_route("/search", get(search_issues), &[Scope::WorkspacesRead])
        .audited_route("/resolve", get(resolve_issue), &[Scope::WorkspacesRead])
        .audited_route(
            "/github/:repositoryId/:number",
            get(get_github_issue),
            &[Scope::WorkspacesRead],
        )
        .audited_route(
            "/github/:repositoryId/:number",
            patch(edit_github_page_issue),
            &[Scope::WorkspacesWrite],
        )
        .audited_route(
            "/github/:repositoryId/:number/write-target",
            get(get_github_write_target),
            &[Scope::WorkspacesRead],
        )
        .audited_route(
            "/github/:repositoryId/:number/state",
            post(change_github_page_issue_state),
            &[Scope::WorkspacesWrite],
        )
        .audited_route(
            "/github/:repositoryId/:number/comments",
            post(post_github_page_comment),
            &[Scope::WorkspacesWrite],
        )
        .audited_route(
            "/github/:repositoryId/:number/comments/:commentId",
            patch(edit_github_page_comment),
            &[Scope::WorkspacesWrite],
        )
        .audited_route(
            "/g5/:number",
            get(get_native_issue),
            &[Scope::WorkspacesRead],
        )
        .audited_route(
            "/g5/:number",
            patch(edit_native_issue),
            &[Scope::WorkspacesWrite],
        )
        .audited_route(
            "/g5/:number/state",
            post(change_native_issue_state),
            &[Scope::WorkspacesWrite],
        )
        .audited_route(
            "/g5/:number/comments",
            post(post_native_issue_comment),
            &[Scope::WorkspacesWrite],
        )
        .audited_route(
            "/g5/:number/comments/:commentId",
            patch(edit_native_issue_comment),
            &[Scope::WorkspacesWrite],
        )
        .audited_route(
            "/g5/:number/attachments",
            get(list_native_issue_attachments),
            &[Scope::WorkspacesRead],
        )
        .audited_route(
            "/g5/:number/attachments",
            post(upload_native_issue_attachment).layer(axum::extract::DefaultBodyLimit::max(
                MAX_ATTACHMENT_UPLOAD_BODY,
            )),
            &[Scope::WorkspacesWrite],
        )
        .audited_route(
            "/g5/:number/attachments/:attachmentId",
            get(get_native_issue_attachment),
            &[Scope::WorkspacesRead],
        )
        .audited_route(
            "/g5/:number/attachments/:attachmentId",
            axum::routing::delete(remove_native_issue_attachment),
            &[Scope::WorkspacesWrite],
        )
        .into_router()
        .merge(issue_labels::issue_label_router())
}

/// Nested at `/api/v1/workspaces/:workspaceId/issue-links`.
pub fn workspace_router() -> Router<Arc<AppState>> {
    AuditedRouter::new()
        .audited_route(
            "/",
            get(list_workspace_issue_links),
            &[Scope::WorkspacesRead],
        )
        .audited_route(
            "/",
            post(create_workspace_issue_link),
            &[Scope::WorkspacesWrite],
        )
        .audited_route(
            "/:linkId/unlink",
            post(unlink_workspace_issue_link),
            &[Scope::WorkspacesWrite],
        )
        .audited_route(
            "/:linkId/state",
            post(change_linked_issue_state),
            &[Scope::WorkspacesWrite],
        )
        .into_router()
}

/// `/api/v1/workspaces/:workspaceId/activity`.
pub fn activity_router() -> Router<Arc<AppState>> {
    AuditedRouter::new()
        .audited_route("/", get(get_workspace_activity), &[Scope::WorkspacesRead])
        .audited_route(
            "/current",
            get(get_workspace_activity_current),
            &[Scope::WorkspacesRead],
        )
        .audited_route(
            "/comments",
            post(post_workspace_comment),
            &[Scope::WorkspacesWrite],
        )
        .audited_route(
            "/comments/github/:commentId",
            patch(edit_workspace_comment),
            &[Scope::WorkspacesWrite],
        )
        .audited_route(
            "/comments/g5/:commentId",
            patch(edit_native_workspace_comment),
            &[Scope::WorkspacesWrite],
        )
        .into_router()
}

// ─── Shared helpers ──────────────────────────────────────────────────────────

pub(crate) fn github_client(state: &AppState) -> Result<&GithubAppClient, AppError> {
    state.github_app.as_deref().ok_or_else(|| {
        AppError::from_error(G5Error::ServiceUnavailable {
            message: "external issue integration is not configured".to_string(),
        })
    })
}

/// Opens the caller's tenant scope for a native read or write.
pub(crate) struct OpenDb<F>(pub F);

#[async_trait::async_trait]
impl<F, Fut> TenantDbOpener for OpenDb<F>
where
    F: Fn() -> Fut + Send + Sync,
    Fut: std::future::Future<Output = Result<g5_authz::ScopedDb, AppError>> + Send,
{
    async fn open(&self) -> Result<g5_authz::ScopedDb, G5Error> {
        (self.0)().await.map_err(|error| error.inner)
    }
}

/// The adapters for an organization's `domains` (`g5.issue.native.adapter`):
/// GitHub when the deployment has the App, G5's store when the organization
/// has a native domain, read through `opener`.
pub(crate) async fn issue_backends<'a>(
    state: &'a AppState,
    org_id: Uuid,
    domains: &[IssueDomainAccess],
    opener: &'a dyn TenantDbOpener,
) -> Result<IssueBackends<'a>, AppError> {
    let native = if domains.iter().any(IssueDomainAccess::is_native) {
        Some(NativeBackend {
            opener,
            system_pool: &state.system_pool,
            site: native_issues::native_site(&state.system_pool, &state.external_url, org_id)
                .await?,
            attachments: &state.issue_attachments,
        })
    } else {
        None
    };
    Ok(IssueBackends {
        github: state.github_app.as_deref(),
        native,
    })
}

pub(crate) fn issue_error(error: IssueError) -> AppError {
    AppError::from_error(G5Error::from(error))
}

fn invalid_reference(message: &str) -> AppError {
    issue_error(IssueError::new(
        ExternalIssueErrorKind::InvalidReference,
        message,
    ))
}

// ─── Organization-admin handlers ─────────────────────────────────────────────

/// `POST /api/v1/orgs/:orgId/issue-domains`
///
/// Configure a repository covered by the org's active GitHub App binding
/// (with Issues read) as an issue domain. Database-only: the display name is
/// the binding's recorded coverage name.
async fn create_issue_domain(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(params): Path<OrgPathParams>,
    Json(request): Json<CreateIssueDomainRequest>,
) -> Result<(StatusCode, Json<ResponseEnvelope<IssueDomainDTO>>), AppError> {
    let provider = NewDomainProvider::parse(request.provider.as_deref()).ok_or_else(|| {
        AppError::from_error(G5Error::ValidationError {
            details: json!({"field": "provider", "error": "must be github or g5"}),
        })
    })?;
    let domain = match provider {
        NewDomainProvider::Native => {
            if request.github_repo_id.is_some() {
                return Err(AppError::from_error(G5Error::ValidationError {
                    details: json!({"field": "githubRepoId", "error": "G5 issues name no repository"}),
                }));
            }
            let key = request.key.as_deref().ok_or_else(|| {
                AppError::from_error(G5Error::ValidationError {
                    details: json!({"field": "key", "error": "name the issue key, e.g. ACME"}),
                })
            })?;
            let (mut db, actor) = org_admin_db(&state, &auth, params.org_id).await?;
            ensure_external_issues_enabled(&mut db, params.org_id).await?;
            let domain = create_native_domain(
                &mut db,
                params.org_id,
                actor,
                key,
                request.content_trusted.unwrap_or(true),
            )
            .await?;
            (db, domain)
        }
        NewDomainProvider::Github => {
            let client = github_client(&state)?;
            let github_repo_id = request
                .github_repo_id
                .as_deref()
                .and_then(|id| parse_canonical_github_id(id.trim()))
                .ok_or_else(|| {
                    AppError::from_error(G5Error::ValidationError {
                        details: json!({
                            "field": "githubRepoId",
                            "error": "must be a GitHub numeric repository id",
                        }),
                    })
                })?;
            let (mut db, actor) = org_admin_db(&state, &auth, params.org_id).await?;
            ensure_external_issues_enabled(&mut db, params.org_id).await?;
            let domain = create_github_domain(
                &mut db,
                client.config(),
                params.org_id,
                actor,
                github_repo_id,
                request.content_trusted.unwrap_or(false),
            )
            .await?;
            (db, domain)
        }
    };
    let (db, domain) = domain;
    db.commit().await.map_err(AppError::from)?;
    tracing::info!(org_id = %params.org_id, domain_id = %domain.domain.id, "issue domain configured");
    Ok((
        StatusCode::CREATED,
        Json(ResponseEnvelope::ok(domain.to_dto())),
    ))
}

/// `PATCH /api/v1/orgs/:orgId/issue-domains/:domainId`
async fn update_issue_domain(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(params): Path<IssueDomainPathParams>,
    Json(request): Json<UpdateIssueDomainRequest>,
) -> Result<Json<ResponseEnvelope<IssueDomainDTO>>, AppError> {
    if request.content_trusted.is_none() && request.comments_enabled.is_none() {
        return Err(AppError::from_error(G5Error::ValidationError {
            details: json!({"error": "provide contentTrusted or commentsEnabled"}),
        }));
    }
    let (mut db, actor) = org_admin_db(&state, &auth, params.org_id).await?;
    ensure_external_issues_enabled(&mut db, params.org_id).await?;
    let domain = update_domain_settings(
        &mut db,
        params.org_id,
        params.domain_id,
        actor,
        request.content_trusted,
        request.comments_enabled,
    )
    .await?;
    db.commit().await.map_err(AppError::from)?;
    Ok(Json(ResponseEnvelope::ok(domain.to_dto())))
}

/// `DELETE /api/v1/orgs/:orgId/issue-domains/:domainId`
///
/// Existing workspace links are unaffected.
async fn delete_issue_domain(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(params): Path<IssueDomainPathParams>,
) -> Result<StatusCode, AppError> {
    let (mut db, _) = org_admin_db(&state, &auth, params.org_id).await?;
    delete_domain(&mut db, params.org_id, params.domain_id).await?;
    db.commit().await.map_err(AppError::from)?;
    tracing::info!(org_id = %params.org_id, domain_id = %params.domain_id, "issue domain removed");
    Ok(StatusCode::NO_CONTENT)
}

/// `GET /api/v1/orgs/:orgId/issue-domains/:domainId/export`
///
/// Every issue of the native domain with its comments, events and
/// attachments, as JSON an administrator takes elsewhere
/// (`g5.issue.native.migration`). Streamed, reading one attachment's bytes at
/// a time; a failure after the first byte ends the body early.
async fn export_issue_domain(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(params): Path<IssueDomainPathParams>,
) -> Result<axum::response::Response, AppError> {
    let (mut db, _) = org_admin_db(&state, &auth, params.org_id).await?;
    let access = list_domain_access(&mut db, params.org_id)
        .await?
        .into_iter()
        .find(|access| access.domain.id == params.domain_id)
        .ok_or_else(|| {
            AppError::from_error(G5Error::NotFound {
                resource: format!("issue domain {}", params.domain_id),
            })
        })?;
    if !access.is_native() {
        return Err(AppError::from_error(G5Error::ValidationError {
            details: json!({"error": "only G5 issues can be exported; GitHub keeps its own issues"}),
        }));
    }
    let export = native_issues::export(&mut db, &state.system_pool, &access.domain).await?;
    if export.has_attachments() {
        // Answer unavailable now rather than ending a started export.
        state.issue_attachments.ensure_available()?;
    }
    let (out, pieces) = tokio::sync::mpsc::channel::<Vec<u8>>(4);
    let failed = out.clone();
    let store = state.issue_attachments.clone();
    tokio::spawn(async move {
        let written = match export.write(&mut db, &store, &out).await {
            Ok(()) => db.commit().await.map_err(|error| error.to_string()),
            Err(error) => Err(error.to_string()),
        };
        if let Err(error) = written {
            tracing::warn!(%error, "native issue export ended early");
            // An empty piece marks the failure for the body below.
            let _ = failed.send(Vec::new()).await;
        }
    });
    use axum::response::IntoResponse as _;
    let body = futures_util::stream::unfold(pieces, |mut pieces| async move {
        let piece = pieces.recv().await?;
        let item = if piece.is_empty() {
            Err(std::io::Error::other("the export failed"))
        } else {
            Ok(bytes::Bytes::from(piece))
        };
        Some((item, pieces))
    });
    Ok((
        [(axum::http::header::CONTENT_TYPE, "application/json")],
        axum::body::Body::from_stream(body),
    )
        .into_response())
}

// ─── Organization-member handlers ────────────────────────────────────────────

/// `GET /api/v1/orgs/:orgId/issue-domains`
///
/// Domains with a database-only availability check against the latest
/// GitHub App binding.
async fn list_issue_domains(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(params): Path<OrgPathParams>,
) -> Result<Json<ResponseEnvelope<IssueDomainListDTO>>, AppError> {
    let mut db = scoped_db_for_org(&state, &auth, params.org_id, MEMBER_READ_SCOPE)
        .await
        .map_err(mask_foreign_org_access)?;
    // Listed while external issues are off too (G5's own configuration, no
    // GitHub call), so an admin can still remove a domain.
    let domains = list_domain_access(&mut db, params.org_id).await?;
    db.commit().await.map_err(AppError::from)?;
    Ok(Json(ResponseEnvelope::ok(IssueDomainListDTO {
        domains: domains.iter().map(|access| access.to_dto()).collect(),
    })))
}

/// `GET /api/v1/orgs/:orgId/issues/search?q=&state=open|closed|all&assignee=me|none`
///
/// Federated GitHub search over every domain. Each domain settles
/// independently; failures are listed in `domainErrors`. `assignee=me`
/// limits results to issues assigned to the caller's connected GitHub
/// account (refused as `github_connection_required` without one);
/// `assignee=none` limits them to unassigned issues.
async fn search_issues(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(params): Path<OrgPathParams>,
    Query(query): Query<IssueSearchQuery>,
) -> Result<Json<ResponseEnvelope<ExternalIssueSearchResultsDTO>>, AppError> {
    let filters = parse_search_filters(query.state.as_deref(), query.assignee.as_deref())?;
    let viewer = auth.require_user_id().ok();
    let mut db = scoped_db_for_org(&state, &auth, params.org_id, MEMBER_READ_SCOPE)
        .await
        .map_err(mask_foreign_org_access)?;
    ensure_external_issues_enabled(&mut db, params.org_id).await?;
    let domains = list_domain_access(&mut db, params.org_id).await?;
    db.commit().await.map_err(AppError::from)?;

    let response = run_issue_search(
        &state,
        &IssueSearch {
            org_id: params.org_id,
            viewer,
            domains: &domains,
            query: query.q.as_deref().unwrap_or_default(),
            filters,
        },
        || async {
            scoped_db_for_org(&state, &auth, params.org_id, MEMBER_READ_SCOPE)
                .await
                .map_err(mask_foreign_org_access)
        },
    )
    .await?;
    Ok(Json(ResponseEnvelope::ok(response)))
}

/// The `state` and `assignee` of an issue search, as people and agents
/// send them: `state` is `open`, `closed` or `all` (default open), and
/// `assignee` is `me` or `none` (default anyone).
pub(crate) fn parse_search_filters(
    state: Option<&str>,
    assignee: Option<&str>,
) -> Result<(IssueStateFilter, AssigneeParam), AppError> {
    let state_filter = IssueStateFilter::parse(state).ok_or_else(|| {
        AppError::from_error(G5Error::ValidationError {
            details: json!({"field": "state", "error": "must be open, closed or all"}),
        })
    })?;
    let assignee = match assignee.map(str::trim) {
        None | Some("") => AssigneeParam::Any,
        Some("me") => AssigneeParam::Me,
        Some("none") => AssigneeParam::Unassigned,
        Some(_) => {
            return Err(AppError::from_error(G5Error::ValidationError {
                details: json!({"field": "assignee", "error": "must be me or none"}),
            }))
        }
    };
    Ok((state_filter, assignee))
}

/// A federated search: `viewer` is the person searching, or the owner of
/// the searching agent's workspace. `assignee=me` is the viewer's account.
pub(crate) struct IssueSearch<'a> {
    pub org_id: Uuid,
    pub viewer: Option<Uuid>,
    pub domains: &'a [IssueDomainAccess],
    pub query: &'a str,
    pub filters: (IssueStateFilter, AssigneeParam),
}

/// Search `search.domains` and fill each result's primary workspace as the
/// viewer sees it. `open_db` opens the organization for that viewer; native
/// domains are read through it too.
pub(crate) async fn run_issue_search<F, Fut>(
    state: &AppState,
    search: &IssueSearch<'_>,
    open_db: F,
) -> Result<ExternalIssueSearchResultsDTO, AppError>
where
    F: Fn() -> Fut + Send + Sync,
    Fut: std::future::Future<Output = Result<g5_authz::ScopedDb, AppError>> + Send,
{
    let (state_filter, assignee_param) = search.filters;
    let has_github = search.domains.iter().any(|access| !access.is_native());
    let has_native = search.domains.iter().any(IssueDomainAccess::is_native);
    // "Me" needs a person; on GitHub domains also their connected account.
    // Without one, a GitHub-only search is refused as before; beside native
    // domains each GitHub domain reports it and native results still return.
    let account = if assignee_param == AssigneeParam::Me && !search.domains.is_empty() {
        let user_id = search.viewer.ok_or_else(|| {
            AppError::from_error(G5Error::ValidationError {
                details: json!({"field": "assignee", "error": "me needs a person"}),
            })
        })?;
        let account = if has_github {
            connected_account(&state.system_pool, user_id).await?
        } else {
            None
        };
        if has_github && !has_native && account.is_none() {
            return Err(AppError::from_error(G5Error::ConflictDetails {
                reason: "connect your GitHub account to see issues assigned to you".to_string(),
                details: json!({"code": "github_connection_required"}),
            }));
        }
        account
    } else {
        None
    };
    let assignee = match assignee_param {
        AssigneeParam::Any => SearchAssignee::Any,
        AssigneeParam::Unassigned => SearchAssignee::Unassigned,
        AssigneeParam::Me => SearchAssignee::Me {
            user_id: search.viewer,
            github: account.as_ref(),
        },
    };
    let opener = OpenDb(&open_db);
    let backends = issue_backends(state, search.org_id, search.domains, &opener).await?;
    let mut response = search_domains(
        &backends,
        search.domains,
        search.query,
        state_filter,
        assignee,
        search.viewer,
    )
    .await;
    if !response.results.is_empty() {
        let mut db = open_db().await?;
        let mut summaries: Vec<&mut ExternalIssueSummaryDTO> =
            response.results.iter_mut().collect();
        attach_primary_owners(
            &mut db,
            &state.system_pool,
            search.viewer,
            search.org_id,
            &mut summaries,
        )
        .await?;
        db.commit().await.map_err(AppError::from)?;
    }
    Ok(response)
}

/// `GET /api/v1/orgs/:orgId/issues/resolve?ref=<url | owner/name#N>`
async fn resolve_issue(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(params): Path<OrgPathParams>,
    Query(query): Query<IssueResolveQuery>,
) -> Result<Json<ResponseEnvelope<ExternalIssueSummaryDTO>>, AppError> {
    let reference = query.reference.unwrap_or_default();
    let viewer = auth.require_user_id().ok();
    let mut db = scoped_db_for_org(&state, &auth, params.org_id, MEMBER_READ_SCOPE)
        .await
        .map_err(mask_foreign_org_access)?;
    ensure_external_issues_enabled(&mut db, params.org_id).await?;
    let domains = list_domain_access(&mut db, params.org_id).await?;
    db.commit().await.map_err(AppError::from)?;

    let open_db = || async {
        scoped_db_for_org(&state, &auth, params.org_id, MEMBER_READ_SCOPE)
            .await
            .map_err(mask_foreign_org_access)
    };
    let opener = OpenDb(&open_db);
    let backends = issue_backends(&state, params.org_id, &domains, &opener).await?;
    let mut summary = resolve_reference(&backends, &domains, &reference)
        .await
        .map_err(issue_error)?
        .summary;

    let mut db = open_db().await?;
    attach_primary_owners(
        &mut db,
        &state.system_pool,
        viewer,
        params.org_id,
        &mut [&mut summary],
    )
    .await?;
    db.commit().await.map_err(AppError::from)?;
    Ok(Json(ResponseEnvelope::ok(summary)))
}

/// `GET /api/v1/orgs/:orgId/issues/github/:repositoryId/:number`
///
/// Issue, first 100 comments (oldest first) and active workspace links. The
/// repository must be a configured domain.
async fn get_github_issue(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(params): Path<GithubIssuePathParams>,
) -> Result<Json<ResponseEnvelope<ExternalIssueDetailDTO>>, AppError> {
    let client = github_client(&state)?;
    let (repository_id, number) = github_issue_path(&params)?;
    let viewer = auth.require_user_id().ok();
    let mut db = scoped_db_for_org(&state, &auth, params.org_id, MEMBER_READ_SCOPE)
        .await
        .map_err(mask_foreign_org_access)?;
    ensure_external_issues_enabled(&mut db, params.org_id).await?;
    let domains = list_domain_access(&mut db, params.org_id).await?;
    db.commit().await.map_err(AppError::from)?;

    let index = domain_for_repository(&domains, repository_id).map_err(issue_error)?;
    let backends = IssueBackends {
        github: Some(client),
        native: None,
    };
    let detail = fetch_issue_detail(&backends, &domains[index], number, true)
        .await
        .map_err(issue_error)?;
    let detail = with_links(&state, &auth, params.org_id, viewer, detail).await?;
    Ok(Json(ResponseEnvelope::ok(detail)))
}

/// The repository id and issue number an issue page's path names.
fn github_issue_path(params: &GithubIssuePathParams) -> Result<(u64, u64), AppError> {
    let repository_id = parse_canonical_github_id(&params.repository_id)
        .ok_or_else(|| invalid_reference("the repository id is not valid"))?;
    let number = params
        .number
        .parse::<u64>()
        .ok()
        .filter(|number| *number > 0 && !params.number.starts_with('0'))
        .ok_or_else(|| invalid_reference("the issue number is not valid"))?;
    Ok((repository_id, number))
}

/// Fill an issue page's active links and primary owner as the viewer sees
/// them.
async fn with_links(
    state: &Arc<AppState>,
    auth: &AuthContext,
    org_id: Uuid,
    viewer: Option<Uuid>,
    mut detail: ExternalIssueDetailDTO,
) -> Result<ExternalIssueDetailDTO, AppError> {
    let mut db = scoped_db_for_org(state, auth, org_id, MEMBER_READ_SCOPE)
        .await
        .map_err(mask_foreign_org_access)?;
    detail.links = issue_link_refs(
        &mut db,
        &state.system_pool,
        viewer,
        org_id,
        detail.summary.tracker_id,
        &detail.summary.external_issue_id,
    )
    .await?;
    attach_primary_owners(
        &mut db,
        &state.system_pool,
        viewer,
        org_id,
        &mut [&mut detail.summary],
    )
    .await?;
    db.commit().await.map_err(AppError::from)?;
    Ok(detail)
}

#[derive(Debug, Deserialize)]
pub struct NativeIssuePathParams {
    #[serde(rename = "orgId")]
    pub org_id: Uuid,
    pub number: String,
}

fn issue_number(raw: &str) -> Result<u64, AppError> {
    raw.parse::<u64>()
        .ok()
        .filter(|number| *number > 0 && !raw.starts_with('0') && *number <= i64::MAX as u64)
        .ok_or_else(|| invalid_reference("the issue number is not valid"))
}

/// `GET /api/v1/orgs/:orgId/issues/g5/:number`
///
/// A native issue with its comments (oldest first) and active workspace
/// links, read from G5's own store.
async fn get_native_issue(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(params): Path<NativeIssuePathParams>,
) -> Result<Json<ResponseEnvelope<ExternalIssueDetailDTO>>, AppError> {
    let number = issue_number(&params.number)?;
    let viewer = auth.require_user_id().ok();
    let mut db = scoped_db_for_org(&state, &auth, params.org_id, MEMBER_READ_SCOPE)
        .await
        .map_err(mask_foreign_org_access)?;
    ensure_external_issues_enabled(&mut db, params.org_id).await?;
    let domains = list_domain_access(&mut db, params.org_id).await?;
    let access = native_domain(&domains).map_err(issue_error)?;
    let site =
        native_issues::native_site(&state.system_pool, &state.external_url, params.org_id).await?;
    let reader = NativeReader {
        domain: &access.domain,
        site: &site,
        system_pool: &state.system_pool,
    };
    let detail = native_issues::detail_by_number(&mut db, &reader, number, true)
        .await
        .map_err(issue_error)?;
    db.commit().await.map_err(AppError::from)?;
    let detail = with_links(&state, &auth, params.org_id, viewer, detail).await?;
    Ok(Json(ResponseEnvelope::ok(detail)))
}

// ─── Issue creation ──────────────────────────────────────────────────────────

/// The organization's default creation target, if one is configured.
pub(crate) fn creation_target(domains: &[IssueDomainAccess]) -> Option<&IssueDomainAccess> {
    domains
        .iter()
        .find(|access| access.domain.is_default_create_target)
}

fn creation_destination(access: &IssueDomainAccess) -> CreationDestinationDTO {
    CreationDestinationDTO {
        domain_id: access.domain.id,
        tracker_id: access.domain.tracker_id,
        provider: access.domain.provider.clone(),
        repository_id: if access.is_native() {
            native_issues::NATIVE_LOCATOR_REPOSITORY.to_string()
        } else {
            access.domain.external_container_id.clone()
        },
        display_key: access.domain.display_key.clone(),
        content_trusted: access.domain.content_trusted,
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IssueCreateTargetDTO {
    /// `ready`, `no_target`, `comments_disabled`, `public_destination`,
    /// `visibility_unknown`, `connection_required` or `unavailable`.
    pub state: &'static str,
    pub destination: Option<CreationDestinationDTO>,
    /// The GitHub account issues are created as, once connected.
    pub github_login: Option<String>,
    /// Accounts GitHub lets the repository's issues be assigned to (first
    /// page); empty when they could not be read.
    pub assignable_users: Vec<ExternalIssueUserDTO>,
    /// Why the target could not be read, for `unavailable`.
    pub error: Option<IssueErrorDTO>,
}

/// `GET /api/v1/orgs/:orgId/issues/create-target`
///
/// Whether the caller can create an issue now, where it would go and as
/// whom: what the Create button and dialog show. The checks are those
/// creation repeats.
async fn get_create_target(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(params): Path<OrgPathParams>,
) -> Result<Json<ResponseEnvelope<IssueCreateTargetDTO>>, AppError> {
    let viewer = auth.require_user_id().map_err(AppError::from_error)?;
    let mut db = scoped_db_for_org(&state, &auth, params.org_id, MEMBER_READ_SCOPE)
        .await
        .map_err(mask_foreign_org_access)?;
    ensure_external_issues_enabled(&mut db, params.org_id).await?;
    let domains = list_domain_access(&mut db, params.org_id).await?;
    db.commit().await.map_err(AppError::from)?;

    let mut target = IssueCreateTargetDTO {
        state: "no_target",
        destination: None,
        github_login: None,
        assignable_users: Vec::new(),
        error: None,
    };
    let Some(access) = creation_target(&domains) else {
        return Ok(Json(ResponseEnvelope::ok(target)));
    };
    target.destination = Some(creation_destination(access));
    // G5 issues are created as the person's G5 account and publish nothing
    // outside G5: no connection, switch or audience applies.
    if access.is_native() {
        target.state = "ready";
        target.assignable_users =
            native_issues::assignable_users(&state.system_pool, params.org_id).await?;
        return Ok(Json(ResponseEnvelope::ok(target)));
    }
    let client = github_client(&state)?;
    target.github_login = grant_status(&state.system_pool, viewer).await?.github_login;
    if !access.domain.comments_enabled {
        target.state = "comments_disabled";
        return Ok(Json(ResponseEnvelope::ok(target)));
    }
    let (audience, users) = tokio::join!(
        domain_repository_audience(client, access),
        domain_assignable_users(client, access)
    );
    target.assignable_users = users.unwrap_or_default();
    target.state = match audience {
        Ok(RepositoryAudience::NonPublic) if target.github_login.is_none() => "connection_required",
        Ok(RepositoryAudience::NonPublic) => "ready",
        Ok(RepositoryAudience::Public) => "public_destination",
        Ok(RepositoryAudience::Unknown) => "visibility_unknown",
        Err(error) => {
            target.error = Some(IssueErrorDTO::from(&error));
            "unavailable"
        }
    };
    Ok(Json(ResponseEnvelope::ok(target)))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreateIssueRequest {
    pub title: String,
    #[serde(default)]
    pub body: String,
    /// GitHub logins; `@me` is the caller's connected account.
    #[serde(default)]
    pub assignees: Vec<String>,
    /// Stable across retries and checks of the same submission.
    pub idempotency_key: String,
    /// The destination the person saw; a changed default target is refused.
    pub domain_id: Uuid,
    /// Only look for an issue this submission already created; never send.
    #[serde(default)]
    pub check_only: bool,
    /// An issue in the creation target to create the issue under: its URL,
    /// `owner/name#N` or number.
    #[serde(default)]
    pub parent: Option<String>,
    /// Labels from the destination's catalog (`g5.issue.labels`).
    #[serde(default)]
    pub labels: Vec<String>,
}

/// `POST /api/v1/orgs/:orgId/issues`
///
/// Create an issue in the organization's default creation target as the
/// caller's GitHub account, or with `checkOnly` look for one an earlier
/// attempt with the same key created. Any member with write authority may
/// create; their GitHub permissions still apply.
async fn create_org_issue(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(params): Path<OrgPathParams>,
    Json(request): Json<CreateIssueRequest>,
) -> Result<(StatusCode, Json<ResponseEnvelope<IssueCreationDTO>>), AppError> {
    let author = auth.require_user_id().map_err(AppError::from_error)?;
    validate_key(&request.idempotency_key, 200)?;
    let open_db = || async {
        scoped_db_for_org(&state, &auth, params.org_id, MEMBER_WRITE_SCOPE)
            .await
            .map_err(mask_foreign_org_access)
    };
    let mut db = open_db().await?;
    ensure_external_issues_enabled(&mut db, params.org_id).await?;
    let domains = list_domain_access(&mut db, params.org_id).await?;
    db.commit().await.map_err(AppError::from)?;

    let result = create_tracker_issue(
        &state,
        &TrackerIssueCreation {
            org_id: params.org_id,
            domains: &domains,
            expected_domain: Some(request.domain_id),
            author,
            idempotency_key: &request.idempotency_key,
            marker_key: &request.idempotency_key,
            title: &request.title,
            body: &request.body,
            assignees: &request.assignees,
            labels: &request.labels,
            parent: request.parent.as_deref(),
            check_only: request.check_only,
            origin: "person",
        },
        &OpenDb(&open_db),
    )
    .await?;
    let status = if result.sent && result.state == "created" {
        StatusCode::CREATED
    } else {
        StatusCode::OK
    };
    Ok((status, Json(ResponseEnvelope::ok(result))))
}

/// A new issue in the organization's default creation target, from a person
/// or from a workspace's agent creating as its owner.
pub(crate) struct TrackerIssueCreation<'a> {
    pub org_id: Uuid,
    pub domains: &'a [IssueDomainAccess],
    /// The target the person saw; `None` takes the current one.
    pub expected_domain: Option<Uuid>,
    /// The G5 user whose GitHub identity creates the issue.
    pub author: Uuid,
    /// The key as the caller gave it, returned for retries and checks.
    pub idempotency_key: &'a str,
    /// The key the marker (GitHub) or the stored key (G5 issues) derives
    /// from; agent keys are namespaced.
    pub marker_key: &'a str,
    /// As submitted; validated by the destination's rules.
    pub title: &'a str,
    pub body: &'a str,
    pub assignees: &'a [String],
    /// Catalog label names, as given; validated here.
    pub labels: &'a [String],
    /// The requested parent reference, as given.
    pub parent: Option<&'a str>,
    pub check_only: bool,
    pub origin: &'static str,
}

/// Create an issue in the organization's default creation domain: a G5
/// issue under the native write rules, or a GitHub issue through the
/// publication guards (`g5.external-issues.creation`).
pub(crate) async fn create_tracker_issue(
    state: &AppState,
    creation: &TrackerIssueCreation<'_>,
    opener: &dyn TenantDbOpener,
) -> Result<IssueCreationDTO, AppError> {
    let access = creation_target(creation.domains).ok_or_else(|| {
        AppError::from_error(comment_conflict(
            "no_creation_target",
            "an organization admin has not set up where new issues go",
        ))
    })?;
    if creation
        .expected_domain
        .is_some_and(|expected| expected != access.domain.id)
    {
        return Err(AppError::from_error(comment_conflict(
            "destination_changed",
            "the organization's destination for new issues changed; review it and submit again",
        )));
    }
    let labels = validate_selection(creation.labels)?;
    if access.is_native() {
        return create_native_issue(state, access, creation, &labels, opener).await;
    }
    let draft = validate_draft(creation.title, creation.body, creation.assignees)?;
    create_github_issue(
        github_client(state)?,
        comment_token_manager(state)?,
        &state.system_pool,
        creation,
        &draft,
        &labels,
    )
    .await
}

/// Create a G5 issue (`g5.issue.native.writes`): one transaction, keyed by
/// the author's idempotency key, so a retry or a check returns the first
/// issue; the parent is set in the same transaction.
async fn create_native_issue(
    state: &AppState,
    access: &IssueDomainAccess,
    creation: &TrackerIssueCreation<'_>,
    labels: &[String],
    opener: &dyn TenantDbOpener,
) -> Result<IssueCreationDTO, AppError> {
    let title = native_issues::validate_title(creation.title)?;
    let body = native_issues::validate_text("body", creation.body, false)?;
    let assignees = native_issues::resolve_assignees(
        &state.system_pool,
        creation.org_id,
        creation.author,
        creation.assignees,
    )
    .await?;
    let site = native_issues::native_site(&state.system_pool, &state.external_url, creation.org_id)
        .await?;
    let reader = NativeReader {
        domain: &access.domain,
        site: &site,
        system_pool: &state.system_pool,
    };
    let parent = match target_reference_for(creation.parent, &access.domain) {
        Some(reference) => Some(
            native_issues::parse_reference(&reference, &access.domain.display_key, &site)
                .ok_or_else(|| {
                    AppError::from_error(G5Error::ValidationError {
                        details: json!({
                            "field": "parent",
                            "code": "parent_outside_destination",
                            "error": format!(
                                "the parent must be an issue in {}, where the new issue is created",
                                access.domain.display_key
                            ),
                        }),
                    })
                })?,
        ),
        None => None,
    };
    let mut db = opener.open().await?;
    let created = if creation.check_only {
        native_issues::find_created(
            &mut db,
            &reader,
            creation.org_id,
            creation.author,
            creation.marker_key,
        )
        .await?
    } else {
        Some(
            native_issues::create_issue(
                &mut db,
                &reader,
                &NewNativeIssue {
                    org_id: creation.org_id,
                    author: creation.author,
                    origin: creation.origin,
                    idempotency_key: creation.marker_key,
                    title: &title,
                    body: &body,
                    assignees: &assignees,
                    labels,
                    parent,
                },
            )
            .await?,
        )
    };
    db.commit().await.map_err(AppError::from)?;
    let destination = creation_destination(access);
    let profiles = native_issues::load_profiles(&state.system_pool, assignees.clone()).await?;
    let result = IssueCreationDTO {
        state: if created.is_some() {
            "created"
        } else {
            "not_found"
        },
        sent: created.as_ref().is_some_and(|created| created.created),
        issue: created
            .as_ref()
            .map(|created| CreatedIssueDTO::from_summary(&created.issue)),
        idempotency_key: creation.idempotency_key.to_string(),
        destination,
        github_login: None,
        title,
        payload: body,
        assignees: assignees
            .iter()
            .filter_map(|id| profiles.get(id).map(|user| user.login.clone()))
            .collect(),
        labels: created
            .as_ref()
            .map(|created| {
                created
                    .issue
                    .labels
                    .iter()
                    .map(|label| label.name.clone())
                    .collect()
            })
            .unwrap_or_else(|| labels.to_vec()),
        labels_not_applied: Vec::new(),
        message: None,
        parent: created.as_ref().and_then(|created| {
            created
                .parent
                .as_ref()
                .map(|parent| CreationParentDTO::from_summary(parent, "attached"))
        }),
    };
    tracing::info!(
        org_id = %creation.org_id,
        author = %creation.author,
        origin = creation.origin,
        state = result.state,
        sent = result.sent,
        issue = result.issue.as_ref().map(|issue| issue.number),
        "native issue creation"
    );
    Ok(result)
}

/// Check the destination policy, look for this submission's marker and,
/// when no issue carries it, create the issue; then put the issue under the
/// requested parent unless it already is.
///
/// The target must exist (and be the one the person saw), allow G5 comments
/// and be reported non-public by GitHub now, and the author must have
/// connected GitHub. G5 records nothing: an issue an earlier attempt with the
/// same key created is found by its marker and returned instead of creating
/// another (`sent: false`). A failed lookup refuses rather than risk a
/// duplicate. No transaction is open here.
async fn create_github_issue(
    client: &GithubAppClient,
    token_manager: &crate::auth::ScmTokenManager,
    system_pool: &sqlx::PgPool,
    creation: &TrackerIssueCreation<'_>,
    draft: &IssueDraft,
    labels: &[String],
) -> Result<IssueCreationDTO, AppError> {
    let access = creation_target(creation.domains).ok_or_else(|| {
        AppError::from_error(comment_conflict(
            "no_creation_target",
            "an organization admin has not set up a repository for new issues",
        ))
    })?;
    let repository_id = access.github_repository_id().map_err(issue_error)?;
    let acting = admit_person_write(
        client,
        token_manager,
        system_pool,
        access,
        creation.author,
        PersonWrite::Creation,
    )
    .await?;

    let parent = match target_reference_for(creation.parent, &access.domain) {
        Some(reference) => Some(resolve_creation_parent(client, access, &reference).await?),
        None => None,
    };

    let marker = creation_marker(creation.org_id, creation.author, creation.marker_key);
    let destination = creation_destination(access);
    let mut result = IssueCreationDTO {
        state: "not_found",
        sent: false,
        issue: None,
        idempotency_key: creation.idempotency_key.to_string(),
        github_login: Some(acting.github_login.clone()),
        title: draft.title.clone(),
        payload: render_issue_payload(&draft.body, &marker),
        assignees: resolve_assignees(&draft.assignees, &acting.github_login),
        labels: labels.to_vec(),
        labels_not_applied: Vec::new(),
        message: None,
        parent: parent
            .as_ref()
            .map(|parent| CreationParentDTO::new(&destination, parent)),
        destination,
    };
    let since = (chrono::Utc::now() - chrono::Duration::hours(MARKER_WINDOW_HOURS)).to_rfc3339();
    let found = find_marked_issue(client, access, &acting.github_login, &since, &marker)
        .await
        .map_err(issue_error)?;
    let mut issue = found;
    if issue.is_none() && !creation.check_only {
        // GitHub creates a label it does not know: send only catalog labels,
        // named as the catalog has them (`g5.issue.labels.github`).
        let mut missing = Vec::new();
        let mut known = Vec::new();
        for (name, found) in labels.iter().zip(
            domain_labels_named(client, access, labels)
                .await
                .map_err(issue_error)?,
        ) {
            match found {
                Some(label) => known.push(label.name),
                None => missing.push(name.clone()),
            }
        }
        if !missing.is_empty() {
            return Err(AppError::from_error(unknown_labels(
                "labels",
                &missing,
                &access.domain.display_key,
            )));
        }
        result.labels = known;
        result.sent = true;
        match create_issue(
            client,
            &acting.token,
            repository_id,
            &result.title,
            &result.payload,
            &result.assignees,
            &result.labels,
        )
        .await
        {
            IssueCreateOutcome::Created(created) => {
                // GitHub silently drops labels the author may not set; a
                // replay found by its marker is never compared, so labels
                // edited since are left alone.
                result.labels_not_applied = result
                    .labels
                    .iter()
                    .filter(|name| {
                        !created
                            .labels
                            .iter()
                            .any(|label| label.name.eq_ignore_ascii_case(name))
                    })
                    .cloned()
                    .collect();
                issue = Some(*created);
            }
            IssueCreateOutcome::Rejected(error) => return Err(issue_error(error)),
            IssueCreateOutcome::Unknown(message) => {
                result.state = "outcome_unknown";
                result.message = Some(message);
            }
        }
    }
    if let Some(issue) = issue {
        result.state = "created";
        result.issue = Some(CreatedIssueDTO::new(&result.destination, &issue));
        if let Some(parent) = result.parent.as_mut() {
            settle_parent(
                client,
                access,
                &acting.token,
                &issue,
                result.sent,
                creation.check_only,
                parent,
            )
            .await;
        }
    }
    tracing::info!(
        org_id = %creation.org_id,
        author = %creation.author,
        origin = creation.origin,
        state = result.state,
        sent = result.sent,
        issue = result.issue.as_ref().map(|issue| issue.number),
        parent = result.parent.as_ref().map(|parent| parent.number),
        parent_state = result.parent.as_ref().map(|parent| parent.state),
        "tracker issue creation"
    );
    Ok(result)
}

/// The parent a creation names: an issue in the creation target, so the
/// sub-issue write goes to the destination already admitted. A reference
/// that does not name one refuses the creation before anything is sent.
async fn resolve_creation_parent(
    client: &GithubAppClient,
    access: &IssueDomainAccess,
    reference: &str,
) -> Result<GithubIssue, AppError> {
    let error =
        match resolve_github_reference(client, std::slice::from_ref(access), reference).await {
            Ok((_, _, issue)) => return Ok(issue),
            Err(error) => error,
        };
    let (code, message) = match error.kind {
        ExternalIssueErrorKind::OutsideDomain => (
            "parent_outside_destination",
            format!(
                "the parent must be an issue in {}, where the new issue is created",
                access.domain.display_key
            ),
        ),
        ExternalIssueErrorKind::NotFound => ("parent_not_found", error.message),
        ExternalIssueErrorKind::NotAnIssue => ("parent_not_an_issue", error.message),
        ExternalIssueErrorKind::InvalidReference => ("invalid_parent", error.message),
        _ => return Err(issue_error(error)),
    };
    Err(AppError::from_error(G5Error::ValidationError {
        details: json!({"field": "parent", "code": code, "error": message}),
    }))
}

/// Put the issue under `parent` unless it already is. An issue this request
/// sent (`fresh`) has no parent yet; one an earlier attempt created is read
/// first, so a retry neither attaches twice nor moves the issue from another
/// parent. A check only reads.
async fn settle_parent(
    client: &GithubAppClient,
    access: &IssueDomainAccess,
    user_token: &str,
    issue: &GithubIssue,
    fresh: bool,
    check_only: bool,
    parent: &mut CreationParentDTO,
) {
    if !fresh {
        match domain_parent_issue(client, access, issue.number).await {
            Ok(Some(current)) if current.id == parent.external_issue_id => {
                parent.state = "attached";
                return;
            }
            Ok(Some(current)) => {
                let repository = current
                    .repository_full_name
                    .as_deref()
                    .unwrap_or(&access.domain.display_key);
                parent.state = "other_parent";
                parent.message = Some(format!(
                    "the issue is already a sub-issue of {repository}#{}",
                    current.number
                ));
                return;
            }
            Ok(None) if check_only => {
                parent.state = "not_attached";
                return;
            }
            Ok(None) => {}
            Err(error) => {
                parent.state = "outcome_unknown";
                parent.message = Some(format!("the issue's parent could not be read: {error}"));
                return;
            }
        }
    }
    let attached = match (access.github_repository_id(), issue.id.parse::<u64>()) {
        (Ok(repository_id), Ok(sub_issue_id)) => {
            add_sub_issue(
                client,
                user_token,
                repository_id,
                parent.number,
                sub_issue_id,
            )
            .await
        }
        (Err(error), _) => SubIssueOutcome::Rejected(error),
        (_, Err(_)) => SubIssueOutcome::Rejected(IssueError::unavailable(
            "GitHub returned an invalid issue id",
        )),
    };
    (parent.state, parent.message) = match attached {
        SubIssueOutcome::Attached => ("attached", None),
        SubIssueOutcome::Rejected(error) => ("not_attached", Some(error.message)),
        SubIssueOutcome::Unknown(message) => ("outcome_unknown", Some(message)),
    };
}

// ─── Workspace handlers ──────────────────────────────────────────────────────

/// `GET /api/v1/workspaces/:workspaceId/issue-links`
///
/// Active links, primary first, each with a live GitHub summary or a typed
/// error. Summaries are fetched concurrently after the transaction commits.
/// While the organization has external issues off, the links G5 holds are
/// listed without any GitHub content (no `issue`, no `issueError`) so they
/// can still be unlinked.
async fn list_workspace_issue_links(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(params): Path<WorkspaceIssueLinksPathParams>,
) -> Result<Json<ResponseEnvelope<WorkspaceIssueLinkListDTO>>, AppError> {
    let viewer = auth.require_user_id().ok();
    let mut db =
        scoped_db_for_workspace(&state, &auth, params.workspace_id, WORKSPACE_READ_SCOPE).await?;
    let org_id = workspace_org_id(&mut db, params.workspace_id).await?;
    let enabled = org_external_issues_enabled(&mut db, org_id).await?;
    let links = list_workspace_links(&mut db, org_id, params.workspace_id).await?;
    if !enabled {
        db.commit().await.map_err(AppError::from)?;
        return Ok(Json(ResponseEnvelope::ok(WorkspaceIssueLinkListDTO {
            links: links.iter().map(|link| link.to_dto(None, None)).collect(),
        })));
    }
    let domains = if links.is_empty() {
        Vec::new()
    } else {
        list_domain_access(&mut db, org_id).await?
    };
    db.commit().await.map_err(AppError::from)?;
    if links.is_empty() {
        return Ok(Json(ResponseEnvelope::ok(WorkspaceIssueLinkListDTO {
            links: Vec::new(),
        })));
    }

    let open_db =
        || scoped_db_for_workspace(&state, &auth, params.workspace_id, WORKSPACE_READ_SCOPE);
    let opener = OpenDb(&open_db);
    let backends = issue_backends(&state, org_id, &domains, &opener).await?;
    let mut outcomes = fetch_link_summaries(&backends, &domains, &links).await;
    let mut summaries: Vec<&mut ExternalIssueSummaryDTO> = outcomes
        .iter_mut()
        .filter_map(|outcome| outcome.as_mut().ok())
        .collect();
    if !summaries.is_empty() {
        let mut db =
            scoped_db_for_workspace(&state, &auth, params.workspace_id, WORKSPACE_READ_SCOPE)
                .await?;
        attach_primary_owners(&mut db, &state.system_pool, viewer, org_id, &mut summaries).await?;
        db.commit().await.map_err(AppError::from)?;
    }
    let links = links
        .iter()
        .zip(outcomes)
        .map(|(link, outcome)| match outcome {
            Ok(summary) => link.to_dto(Some(summary), None),
            Err(error) => link.to_dto(None, Some(IssueErrorDTO::from(&error))),
        })
        .collect();
    Ok(Json(ResponseEnvelope::ok(WorkspaceIssueLinkListDTO {
        links,
    })))
}

#[derive(Debug, Deserialize)]
pub struct WorkspaceActivityQuery {
    /// Cursor from a previous page's `earlierCursor`.
    #[serde(default)]
    pub before: Option<String>,
}

/// The primary issue Activity follows.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActivityIssueDTO {
    pub link_id: Uuid,
    /// `github` or `g5`.
    pub provider: &'static str,
    pub display_key: String,
    pub canonical_url: String,
    pub locator: crate::services::external_issues::ExternalIssueLocatorDTO,
    /// From the issue's domain; `false` while the issue could not be read.
    pub content_trusted: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceActivityDTO {
    /// `null` when the workspace has no primary issue.
    pub primary_issue: Option<ActivityIssueDTO>,
    /// Why the issue's discussion is missing from this page, if it is.
    pub issue_error: Option<IssueErrorDTO>,
    /// GitHub returned only the first page of the issue's timeline.
    pub issue_history_truncated: bool,
    /// Newest first, by last activity.
    pub entries: Vec<ActivityEntryDTO>,
    /// Pass as `before` to load older entries; `null` at the beginning.
    pub earlier_cursor: Option<String>,
    /// Whether the viewer can comment on the primary issue from G5; `null`
    /// without a primary issue.
    pub composer: Option<CommentComposerDTO>,
}

/// The destination a comment is addressed to; submitted back unchanged.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommentDestinationDTO {
    pub tracker_id: Uuid,
    pub external_issue_id: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommentComposerDTO {
    /// `ready`, `connection_required`, `comments_disabled`,
    /// `public_destination`, `visibility_unknown` or `unavailable`.
    pub state: &'static str,
    /// `github`, or `g5`: comments are posted as the viewer's G5 account.
    pub provider: &'static str,
    /// The GitHub account comments are posted as, once connected.
    pub github_login: Option<String>,
    pub destination: CommentDestinationDTO,
    pub display_key: String,
}

/// `GET /api/v1/workspaces/:workspaceId/activity?before=`
///
/// The primary issue's discussion (live GitHub read, no transaction open)
/// interleaved with G5 workspace context. A failed issue read is reported in
/// `issueError` and the page still carries the workspace context.
async fn get_workspace_activity(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(params): Path<WorkspaceIssueLinksPathParams>,
    Query(query): Query<WorkspaceActivityQuery>,
) -> Result<Json<ResponseEnvelope<WorkspaceActivityDTO>>, AppError> {
    let before = match query.before.as_deref() {
        Some(value) => Some(ActivityCursor::parse(value).ok_or_else(|| {
            AppError::from_error(G5Error::ValidationError {
                details: json!({"field": "before", "error": "invalid activity cursor"}),
            })
        })?),
        None => None,
    };
    let mut db =
        scoped_db_for_workspace(&state, &auth, params.workspace_id, WORKSPACE_READ_SCOPE).await?;
    let org_id = workspace_org_id(&mut db, params.workspace_id).await?;
    // Without external issues, Activity is the workspace's own context only.
    let primary = if org_external_issues_enabled(&mut db, org_id).await? {
        list_workspace_links(&mut db, org_id, params.workspace_id)
            .await?
            .into_iter()
            .find(|link| link.is_primary)
    } else {
        None
    };
    let domains = if primary.is_some() {
        list_domain_access(&mut db, org_id).await?
    } else {
        Vec::new()
    };
    // One more than a page per source tells whether older entries exist.
    let mut entries = workspace_entries(
        &mut db,
        &state.system_pool,
        params.workspace_id,
        before.as_ref(),
        ACTIVITY_PAGE_SIZE + 1,
    )
    .await?;
    entries.extend(
        commit_entries(
            &state.workspace_service,
            &mut db,
            params.workspace_id,
            ACTIVITY_COMMITS_PER_BRANCH,
        )
        .await?,
    );
    let viewer = auth.require_user_id().ok();
    db.commit().await.map_err(AppError::from)?;

    let mut issue_error = None;
    let mut issue_history_truncated = false;
    let mut content_trusted = false;
    // Only the first page carries the composer.
    let mut composer_state = None;
    let primary_native = primary
        .as_ref()
        .is_some_and(|link| link.provider() == PROVIDER_G5);
    if let Some(link) = &primary {
        let open_db =
            || scoped_db_for_workspace(&state, &auth, params.workspace_id, WORKSPACE_READ_SCOPE);
        let opener = OpenDb(&open_db);
        let backends = issue_backends(&state, org_id, &domains, &opener).await?;
        match fetch_linked_issue_timeline(&backends, &domains, link).await {
            Ok(timeline) => {
                issue_history_truncated = timeline.truncated;
                content_trusted = timeline.content_trusted;
                composer_state = Some(timeline.composer_state());
                entries.extend(issue_entries(&timeline));
            }
            Err(error) => {
                composer_state = Some("unavailable");
                issue_error = Some(IssueErrorDTO::from(&error));
            }
        }
    }
    // G5 issues take the viewer's G5 account; GitHub ones their connection.
    let github_login = match (viewer, composer_state) {
        (Some(viewer), Some(_)) if !primary_native => {
            grant_status(&state.system_pool, viewer).await?.github_login
        }
        _ => None,
    };
    let provider = if primary_native {
        PROVIDER_G5
    } else {
        "github"
    };
    let composer = match (&primary, composer_state) {
        (Some(link), Some(state)) if before.is_none() => Some(CommentComposerDTO {
            state: if state == "ready" && github_login.is_none() && !primary_native {
                "connection_required"
            } else {
                state
            },
            provider,
            github_login,
            destination: CommentDestinationDTO {
                tracker_id: link.tracker_id,
                external_issue_id: link.external_issue_id.clone(),
            },
            display_key: link.display_key.clone(),
        }),
        _ => None,
    };

    let (entries, earlier) = compose_page(entries, before.as_ref(), ACTIVITY_PAGE_SIZE);
    Ok(Json(ResponseEnvelope::ok(WorkspaceActivityDTO {
        primary_issue: primary.map(|link| {
            let dto = link.to_dto(None, None);
            ActivityIssueDTO {
                link_id: dto.id,
                provider,
                display_key: dto.display_key,
                canonical_url: dto.canonical_url,
                locator: dto.locator,
                content_trusted,
            }
        }),
        issue_error,
        issue_history_truncated,
        entries,
        earlier_cursor: earlier.map(|cursor| cursor.encode()),
        composer,
    })))
}

/// Ongoing work on the Activity surface (ui `g5.ui.activity-current`).
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceActivityCurrentDTO {
    /// Newest last activity first; empty when nothing is running.
    pub entries: Vec<ActivityEntryDTO>,
}

/// `GET /api/v1/workspaces/:workspaceId/activity/current`
///
/// The workspace's ongoing conversations and tasks, from G5 records only (no
/// GitHub read), so it can be polled while the Activity surface is shown.
async fn get_workspace_activity_current(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(params): Path<WorkspaceIssueLinksPathParams>,
) -> Result<Json<ResponseEnvelope<WorkspaceActivityCurrentDTO>>, AppError> {
    let mut db =
        scoped_db_for_workspace(&state, &auth, params.workspace_id, WORKSPACE_READ_SCOPE).await?;
    let entries = current_entries(&mut db, params.workspace_id).await?;
    db.commit().await.map_err(AppError::from)?;
    Ok(Json(ResponseEnvelope::ok(WorkspaceActivityCurrentDTO {
        entries,
    })))
}

// ─── Comments on the primary issue ───────────────────────────────────────────

/// Commenting is collaborative: require workspace access and current target-org
/// write authority in the admission transaction without imposing workspace ownership.
/// Completion of an admitted publication retains its bounded database context.
pub(crate) async fn comment_db(
    state: &Arc<AppState>,
    auth: &AuthContext,
    workspace_id: Uuid,
) -> Result<g5_authz::ScopedDb, AppError> {
    let mut db = scoped_db_for_workspace(state, auth, workspace_id, WORKSPACE_READ_SCOPE).await?;
    let org_id = workspace_org_id(&mut db, workspace_id).await?;
    authorize_org_scopes(
        &mut db,
        auth,
        org_id,
        Some(workspace_id),
        WORKSPACE_WRITE_SCOPE,
    )
    .await?;
    Ok(db)
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PostCommentRequest {
    pub body: String,
    /// Stable across retries of the same submission.
    pub idempotency_key: String,
    /// The destination the author saw; a changed primary issue is refused.
    pub destination: CommentDestinationDTO,
}

pub(crate) fn comment_token_manager(
    state: &AppState,
) -> Result<&crate::auth::ScmTokenManager, AppError> {
    state.scm_token_manager.as_deref().ok_or_else(|| {
        AppError::from_error(G5Error::ServiceUnavailable {
            message: "GitHub connections are not configured on this deployment".to_string(),
        })
    })
}

/// The domain a publication or link routes through, by its GitHub locator.
fn comment_domain(
    domains: &[IssueDomainAccess],
    tracker_id: Uuid,
    repository_id: u64,
) -> Result<&IssueDomainAccess, AppError> {
    let index = domain_for_repository(domains, repository_id).map_err(issue_error)?;
    let access = &domains[index];
    if access.domain.tracker_id != tracker_id {
        return Err(issue_error(IssueError::new(
            ExternalIssueErrorKind::OutsideDomain,
            "the issue belongs to another tracker",
        )));
    }
    Ok(access)
}

/// `POST /api/v1/workspaces/:workspaceId/activity/comments`
///
/// Post a comment on the workspace's primary issue as the caller's GitHub
/// account. The destination must still be the primary issue, the domain must
/// allow comments, and GitHub must report the repository as non-public now.
/// A replay with the same key finds the earlier send on the issue and
/// returns it without sending again (`g5.issue.comment-recovery`).
async fn post_workspace_comment(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(params): Path<WorkspaceIssueLinksPathParams>,
    Json(request): Json<PostCommentRequest>,
) -> Result<(StatusCode, Json<ResponseEnvelope<CommentResultDTO>>), AppError> {
    let author = auth.require_user_id().map_err(AppError::from_error)?;
    let key = request.idempotency_key.clone();
    if key.is_empty() || key.trim() != key || key.len() > 200 {
        return Err(AppError::from_error(G5Error::ValidationError {
            details: json!({
                "field": "idempotencyKey",
                "error": "must be 1-200 characters without surrounding whitespace",
            }),
        }));
    }

    let mut db = comment_db(&state, &auth, params.workspace_id).await?;
    let org_id = workspace_org_id(&mut db, params.workspace_id).await?;
    ensure_external_issues_enabled(&mut db, org_id).await?;
    let primary = list_workspace_links(&mut db, org_id, params.workspace_id)
        .await?
        .into_iter()
        .find(|link| link.is_primary)
        .ok_or_else(|| {
            AppError::from_error(comment_conflict(
                "no_primary_issue",
                "the workspace has no primary issue",
            ))
        })?;
    if primary.tracker_id != request.destination.tracker_id
        || primary.external_issue_id != request.destination.external_issue_id
    {
        return Err(AppError::from_error(comment_conflict(
            "destination_changed",
            "the workspace's primary issue changed; review the destination and submit again",
        )));
    }
    let domains = list_domain_access(&mut db, org_id).await?;
    db.commit().await.map_err(AppError::from)?;

    let outcome = publish_issue_comment(
        &state,
        &IssueComment {
            org_id,
            target: &CommentTarget::from_link(&primary),
            domains: &domains,
            author,
            idempotency_key: &key,
            body: &request.body,
            origin: "person",
        },
        || comment_db(&state, &auth, params.workspace_id),
    )
    .await?;
    tracing::info!(
        workspace_id = %params.workspace_id,
        comment = %outcome.id(),
        "workspace issue comment submitted"
    );
    let status = if outcome.fresh() {
        StatusCode::CREATED
    } else {
        StatusCode::OK
    };
    Ok((status, Json(ResponseEnvelope::ok(outcome.into_dto()))))
}

/// The person's write a domain admits: the operation only changes the
/// refusal wording.
#[derive(Debug, Clone, Copy)]
pub(crate) enum PersonWrite {
    Comment,
    Creation,
    Transition,
    Edit,
    /// A label change on an issue, or a change to the label catalog.
    Labels,
}

impl PersonWrite {
    fn comments_disabled(self) -> &'static str {
        match self {
            Self::Comment | Self::Transition | Self::Edit | Self::Labels => {
                "an organization admin has not enabled G5 comments for this issue domain"
            }
            Self::Creation => {
                "an organization admin has not enabled G5 comments and issue creation for this repository"
            }
        }
    }
    fn public_destination(self) -> &'static str {
        match self {
            Self::Comment => "G5 does not post to public repositories",
            Self::Creation => "G5 does not create issues in public repositories",
            Self::Transition => "G5 does not change issues in public repositories",
            Self::Edit => "G5 does not edit issues in public repositories",
            Self::Labels => "G5 does not change labels in public repositories",
        }
    }
    fn connection_required(self) -> &'static str {
        match self {
            Self::Comment => "connect your GitHub account to comment",
            Self::Creation => "connect your GitHub account to create issues",
            Self::Transition => "connect your GitHub account to close or reopen issues",
            Self::Edit => "connect your GitHub account to edit issues",
            Self::Labels => "connect your GitHub account to change labels",
        }
    }
}

/// The one admission every person's write shares (`g5.external-issues.broker`,
/// first step): the domain's "G5 comments" switch, the repository reported
/// non-public by GitHub now, and the person's own connected account, whose
/// token the write then uses. Nothing is sent before all three pass.
pub(crate) async fn admit_person_write(
    client: &GithubAppClient,
    token_manager: &crate::auth::ScmTokenManager,
    system_pool: &sqlx::PgPool,
    access: &IssueDomainAccess,
    author: Uuid,
    write: PersonWrite,
) -> Result<ActingGithubUser, AppError> {
    if !access.domain.comments_enabled {
        return Err(AppError::from_error(comment_conflict(
            "comments_disabled",
            write.comments_disabled(),
        )));
    }
    match domain_repository_audience(client, access)
        .await
        .map_err(issue_error)?
    {
        RepositoryAudience::NonPublic => {}
        RepositoryAudience::Public => {
            return Err(AppError::from_error(comment_conflict(
                "public_destination",
                write.public_destination(),
            )))
        }
        RepositoryAudience::Unknown => {
            return Err(AppError::from_error(comment_conflict(
                "visibility_unknown",
                "GitHub did not report the repository's visibility",
            )))
        }
    }
    match acting_user(system_pool, token_manager, client, author).await {
        Ok(acting) => Ok(acting),
        Err(UserTokenError::ConnectionRequired) => Err(AppError::from_error(comment_conflict(
            "github_connection_required",
            write.connection_required(),
        ))),
        Err(UserTokenError::Failed(error)) => Err(AppError::from_error(error)),
    }
}

/// The issue a comment is addressed to: a workspace's link, or an issue an
/// agent named (`g5.external-issues.cli`).
#[derive(Debug, Clone)]
pub(crate) struct CommentTarget {
    pub tracker_id: Uuid,
    pub external_issue_id: String,
    pub display_key: String,
    /// GitHub repository id, or `g5`.
    pub repository_id: String,
    pub number: u64,
}

impl CommentTarget {
    pub(crate) fn from_link(link: &WorkspaceIssueLinkRecord) -> Self {
        let locator = link.to_dto(None, None).locator;
        Self {
            tracker_id: link.tracker_id,
            external_issue_id: link.external_issue_id.clone(),
            display_key: link.display_key.clone(),
            repository_id: locator.repository_id,
            number: locator.number,
        }
    }

    pub(crate) fn from_summary(summary: &ExternalIssueSummaryDTO) -> Self {
        Self {
            tracker_id: summary.tracker_id,
            external_issue_id: summary.external_issue_id.clone(),
            display_key: summary.display_key.clone(),
            repository_id: summary.locator.repository_id.clone(),
            number: summary.locator.number,
        }
    }

    pub(crate) fn is_native(&self) -> bool {
        self.repository_id == native_issues::NATIVE_LOCATOR_REPOSITORY
    }
}

/// A new comment, from a person (in a workspace or on the issue page) or
/// from a workspace's agent posting as its owner.
pub(crate) struct IssueComment<'a> {
    pub org_id: Uuid,
    pub target: &'a CommentTarget,
    pub domains: &'a [IssueDomainAccess],
    /// The G5 user who writes the comment (on GitHub, as their account).
    pub author: Uuid,
    pub idempotency_key: &'a str,
    pub body: &'a str,
    pub origin: &'static str,
}

/// A comment on a G5 issue, in the shape of a comment publication: it is
/// published once written (`g5.issue.native.writes`).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NativeCommentDTO {
    /// `g5:<comment id>`.
    pub id: String,
    pub provider: &'static str,
    pub state: &'static str,
    /// `comment` or `edit`.
    pub kind: &'static str,
    pub origin: String,
    pub destination: &'static str,
    pub display_key: String,
    pub github_login: Option<String>,
    pub body: String,
    pub payload: String,
    pub external_comment_url: Option<String>,
    pub error: Option<IssueErrorDTO>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub published_at: Option<chrono::DateTime<chrono::Utc>>,
    pub comment: ExternalIssueCommentDTO,
}

impl NativeCommentDTO {
    fn new(comment: &NativeComment, display_key: &str, kind: &'static str) -> Self {
        Self {
            id: format!("g5:{}", comment.id),
            provider: PROVIDER_G5,
            state: "published",
            kind,
            origin: comment.origin.clone(),
            destination: "issue",
            display_key: display_key.to_string(),
            github_login: None,
            body: comment.body.clone(),
            payload: comment.body.clone(),
            external_comment_url: Some(comment.url.clone()),
            error: None,
            created_at: comment.created_at,
            published_at: Some(comment.updated_at),
            comment: comment.to_dto(),
        }
    }
}

/// A GitHub issue comment G5 posted or edited, built from GitHub's answer:
/// nothing is recorded (`g5.issue.comment-recovery`). A refusal is an error
/// response, never a result.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GithubCommentDTO {
    /// GitHub's comment id; for a new comment whose outcome is unknown,
    /// `marker:<id>`.
    pub id: String,
    pub provider: &'static str,
    /// `published`, or `outcome_unknown`: GitHub gave no definite answer.
    /// Posting again with the same key is safe: G5 looks for the comment
    /// first. Saving an edit again is harmless.
    pub state: &'static str,
    /// `comment` or `edit`.
    pub kind: &'static str,
    /// `person`, or `agent` for the workspace's agent posting as its owner.
    pub origin: String,
    pub destination: &'static str,
    pub display_key: String,
    pub github_login: String,
    /// The comment as written, without the G5 marker.
    pub body: String,
    /// The exact text sent to GitHub.
    pub payload: String,
    pub external_comment_url: Option<String>,
    /// Always `None`: kept for the shape shared with G5 comments.
    pub error: Option<IssueErrorDTO>,
    pub created_at: Option<String>,
    pub published_at: Option<String>,
    pub message: Option<String>,
}

impl GithubCommentDTO {
    fn new(
        target: &CommentTarget,
        kind: &'static str,
        origin: &str,
        github_login: &str,
        body: &str,
        payload: &str,
    ) -> Self {
        Self {
            id: existing_marker(payload)
                .map(|marker| format!("marker:{marker}"))
                .unwrap_or_default(),
            provider: "github",
            state: "outcome_unknown",
            kind,
            origin: origin.to_string(),
            destination: "issue",
            display_key: target.display_key.clone(),
            github_login: github_login.to_string(),
            body: body.trim_end().to_string(),
            payload: payload.to_string(),
            external_comment_url: None,
            error: None,
            created_at: None,
            published_at: None,
            message: None,
        }
    }

    fn published(mut self, comment: &GithubIssueComment) -> Self {
        self.id = comment.id.clone();
        self.state = "published";
        self.external_comment_url = Some(comment.html_url.clone());
        self.created_at = Some(comment.created_at.clone());
        self.published_at = Some(comment.updated_at.clone());
        self
    }

    fn unknown(mut self, message: String) -> Self {
        self.message = Some(message);
        self
    }
}

/// A comment write's result: a GitHub comment, or a G5 comment.
#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
pub enum CommentResultDTO {
    Github(GithubCommentDTO),
    Native(NativeCommentDTO),
}

/// What [`publish_issue_comment`] established, and whether this request
/// wrote it (`false` for a replay of the key).
pub(crate) enum CommentOutcome {
    Github(GithubCommentDTO, bool),
    Native(NativeCommentDTO, bool),
}

impl CommentOutcome {
    pub(crate) fn fresh(&self) -> bool {
        match self {
            Self::Github(_, fresh) | Self::Native(_, fresh) => *fresh,
        }
    }

    pub(crate) fn id(&self) -> String {
        match self {
            Self::Github(comment, _) => comment.id.clone(),
            Self::Native(comment, _) => comment.id.clone(),
        }
    }

    pub(crate) fn into_dto(self) -> CommentResultDTO {
        match self {
            Self::Github(comment, _) => CommentResultDTO::Github(comment),
            Self::Native(comment, _) => CommentResultDTO::Native(comment),
        }
    }
}

/// Comment on an issue of a configured domain: on a G5 issue, written in
/// one transaction keyed by the author's idempotency key; on a GitHub issue,
/// through the publication guards with a key-derived marker. `open_db`
/// opens the caller's tenant scope (with write authority) for G5 issues.
pub(crate) async fn publish_issue_comment<F, Fut>(
    state: &AppState,
    comment: &IssueComment<'_>,
    open_db: F,
) -> Result<CommentOutcome, AppError>
where
    F: Fn() -> Fut + Send + Sync,
    Fut: std::future::Future<Output = Result<g5_authz::ScopedDb, AppError>> + Send,
{
    if !comment.target.is_native() {
        let (posted, fresh) = publish_github_comment(
            github_client(state)?,
            comment_token_manager(state)?,
            &state.system_pool,
            comment,
        )
        .await?;
        return Ok(CommentOutcome::Github(posted, fresh));
    }
    let access = comment
        .domains
        .iter()
        .find(|access| access.is_native() && access.domain.tracker_id == comment.target.tracker_id)
        .ok_or_else(|| {
            issue_error(IssueError::new(
                ExternalIssueErrorKind::OutsideDomain,
                "the organization no longer uses G5 issues",
            ))
        })?;
    let issue_id =
        native_issues::parse_issue_id(&comment.target.external_issue_id).map_err(issue_error)?;
    let site =
        native_issues::native_site(&state.system_pool, &state.external_url, comment.org_id).await?;
    let reader = NativeReader {
        domain: &access.domain,
        site: &site,
        system_pool: &state.system_pool,
    };
    let mut db = open_db().await?;
    let (written, fresh) = native_issues::add_comment(
        &mut db,
        &reader,
        &NewNativeComment {
            org_id: comment.org_id,
            issue_id,
            author: comment.author,
            origin: comment.origin,
            idempotency_key: comment.idempotency_key,
            body: comment.body,
        },
    )
    .await?;
    db.commit().await.map_err(AppError::from)?;
    Ok(CommentOutcome::Native(
        NativeCommentDTO::new(&written, &comment.target.display_key, "comment"),
        fresh,
    ))
}

/// Look for this submission on the issue, then check the destination
/// policy and post it (`g5.issue.comment-recovery`).
///
/// The marker is derived from the organization, author and idempotency key,
/// so an earlier send with the same key is found among the issue's recent
/// comments (with the installation token, before any guard is checked
/// again) and returned without posting (`false`); the same key with other
/// text is refused. Otherwise the domain must allow G5 comments, GitHub must
/// report the repository as non-public now, the body must pass the secret
/// screen and the author must have connected GitHub. Nothing is recorded:
/// an unclear answer is `outcome_unknown`, and posting again with the same
/// key looks first.
async fn publish_github_comment(
    client: &GithubAppClient,
    token_manager: &crate::auth::ScmTokenManager,
    system_pool: &sqlx::PgPool,
    comment: &IssueComment<'_>,
) -> Result<(GithubCommentDTO, bool), AppError> {
    validate_body(comment.body)?;
    let target = comment.target;
    let repository_id = parse_canonical_github_id(&target.repository_id).ok_or_else(|| {
        issue_error(IssueError::new(
            ExternalIssueErrorKind::NotFound,
            "the issue has no GitHub repository locator",
        ))
    })?;
    let access = comment_domain(comment.domains, target.tracker_id, repository_id)?;
    let marker_id = keyed_marker_id(
        comment.org_id,
        comment.author,
        comment.origin,
        comment.idempotency_key,
    );
    let payload = render_payload(comment.body, marker_id);
    let since =
        (chrono::Utc::now() - chrono::Duration::days(COMMENT_MARKER_WINDOW_DAYS)).to_rfc3339();
    let found = find_marked_comment(client, access, target.number, &since, &marker(marker_id))
        .await
        .map_err(issue_error)?;
    if let Some(found) = found {
        if found.body.trim_end() != payload.trim_end() {
            return Err(AppError::from_error(G5Error::ValidationError {
                details: json!({
                    "field": "idempotencyKey",
                    "code": "idempotency_key_reused",
                    "error": "already used for a different comment",
                }),
            }));
        }
        let login = found
            .author
            .as_ref()
            .map(|user| user.login.clone())
            .unwrap_or_default();
        let posted = GithubCommentDTO::new(
            target,
            "comment",
            comment.origin,
            &login,
            comment.body,
            &payload,
        )
        .published(&found);
        return Ok((posted, false));
    }

    let acting = admit_person_write(
        client,
        token_manager,
        system_pool,
        access,
        comment.author,
        PersonWrite::Comment,
    )
    .await?;
    let posted = GithubCommentDTO::new(
        target,
        "comment",
        comment.origin,
        &acting.github_login,
        comment.body,
        &payload,
    );
    let posted = match post_issue_comment(
        client,
        &acting.token,
        repository_id,
        target.number,
        &payload,
    )
    .await
    {
        CommentPostOutcome::Created(created) => posted.published(&created),
        CommentPostOutcome::Rejected(error) => return Err(issue_error(error)),
        CommentPostOutcome::Unknown(message) => posted.unknown(message),
    };
    tracing::info!(
        org_id = %comment.org_id,
        author = %comment.author,
        origin = comment.origin,
        issue = %target.display_key,
        state = posted.state,
        "github issue comment"
    );
    Ok((posted, true))
}

/// Replace the text of `actor`'s own comment `comment_id` on `target`, as
/// their GitHub account, through the comment guards
/// (`g5.issue.comment-edit-observation`): the comment must be theirs (by
/// account id), on that issue, and unchanged on GitHub since the editor
/// opened, unless it already has the new text. That last check is best
/// effort: GitHub's PATCH takes no precondition, so an edit landing between
/// G5's read and its write is overwritten. G5's reference line is kept.
/// Nothing is recorded: repeating an edit is harmless, so an unclear answer
/// is `outcome_unknown` and the author saves again.
pub(crate) async fn edit_github_comment(
    state: &AppState,
    domains: &[IssueDomainAccess],
    target: &CommentTarget,
    actor: Uuid,
    comment_id: &str,
    body: &str,
    expected_updated_at: &str,
) -> Result<GithubCommentDTO, AppError> {
    let client = github_client(state)?;
    let token_manager = comment_token_manager(state)?;
    let comment_id = parse_canonical_github_id(comment_id)
        .ok_or_else(|| {
            AppError::from_error(G5Error::ValidationError {
                details: json!({"field": "commentId", "error": "not a GitHub comment id"}),
            })
        })?
        .to_string();
    validate_body(body)?;
    let repository_id = parse_canonical_github_id(&target.repository_id).ok_or_else(|| {
        issue_error(IssueError::new(
            ExternalIssueErrorKind::NotFound,
            "the issue has no GitHub repository locator",
        ))
    })?;
    let access = comment_domain(domains, target.tracker_id, repository_id)?;
    let acting = admit_person_write(
        client,
        token_manager,
        &state.system_pool,
        access,
        actor,
        PersonWrite::Comment,
    )
    .await?;

    let current = fetch_issue_comment(client, &acting.token, repository_id, &comment_id)
        .await
        .map_err(issue_error)?;
    if current.issue_number != Some(target.number) {
        return Err(AppError::from_error(G5Error::NotFound {
            resource: format!("comment {comment_id} on {}", target.display_key),
        }));
    }
    // By account id: a login can be renamed and then claimed by someone else.
    let own = current
        .author
        .as_ref()
        .is_some_and(|user| user.id == Some(acting.github_user_id));
    if !own {
        return Err(AppError::from_error(comment_conflict(
            "not_your_comment",
            "only your own comments can be edited",
        )));
    }
    let payload = render_edit_payload(body, existing_marker(&current.body).as_deref());
    let mut edited = GithubCommentDTO::new(
        target,
        "edit",
        "person",
        &acting.github_login,
        body,
        &payload,
    );
    // An edit names its comment even before GitHub answers.
    edited.id.clone_from(&comment_id);
    // GitHub may have applied an earlier attempt whose answer was lost; its
    // text is then already this edit's.
    if current.body == payload {
        return Ok(edited.published(&current));
    }
    // GitHub's PATCH is unconditional, so this check narrows but cannot
    // close the window for a concurrent edit on GitHub.
    if current.updated_at != expected_updated_at {
        return Err(AppError::from_error(comment_conflict(
            "comment_changed",
            "the comment changed on GitHub since you started editing",
        )));
    }
    Ok(
        match update_issue_comment(client, &acting.token, repository_id, &comment_id, &payload)
            .await
        {
            CommentPostOutcome::Created(comment) => edited.published(&comment),
            CommentPostOutcome::Rejected(error) => return Err(issue_error(error)),
            CommentPostOutcome::Unknown(message) => {
                edited.unknown(format!("{message}; saving again is safe"))
            }
        },
    )
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EditCommentRequest {
    pub body: String,
    /// Accepted from older clients and ignored: repeating an edit is
    /// harmless.
    #[serde(default)]
    pub idempotency_key: Option<String>,
    /// The destination the author saw; a changed primary issue is refused.
    pub destination: CommentDestinationDTO,
    /// The comment's `updatedAt` when the editor opened. A comment GitHub
    /// shows changed since then is refused; best effort, since GitHub's write
    /// is unconditional (see [`edit_github_comment`]).
    pub expected_updated_at: String,
}

#[derive(Debug, Deserialize)]
pub struct GithubCommentPathParams {
    #[serde(rename = "workspaceId")]
    pub workspace_id: Uuid,
    #[serde(rename = "commentId")]
    pub comment_id: String,
}

/// `PATCH /api/v1/workspaces/:workspaceId/activity/comments/github/:commentId`
///
/// Replace the text of the caller's own comment on the primary issue, as
/// their GitHub account ([`edit_github_comment`]). The destination must
/// still be the primary issue.
async fn edit_workspace_comment(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(params): Path<GithubCommentPathParams>,
    Json(request): Json<EditCommentRequest>,
) -> Result<Json<ResponseEnvelope<GithubCommentDTO>>, AppError> {
    let author = auth.require_user_id().map_err(AppError::from_error)?;
    let mut db = comment_db(&state, &auth, params.workspace_id).await?;
    let org_id = workspace_org_id(&mut db, params.workspace_id).await?;
    ensure_external_issues_enabled(&mut db, org_id).await?;
    let primary = list_workspace_links(&mut db, org_id, params.workspace_id)
        .await?
        .into_iter()
        .find(|link| link.is_primary)
        .ok_or_else(|| {
            AppError::from_error(comment_conflict(
                "no_primary_issue",
                "the workspace has no primary issue",
            ))
        })?;
    if primary.tracker_id != request.destination.tracker_id
        || primary.external_issue_id != request.destination.external_issue_id
    {
        return Err(AppError::from_error(comment_conflict(
            "destination_changed",
            "the workspace's primary issue changed; review the destination and submit again",
        )));
    }
    let domains = list_domain_access(&mut db, org_id).await?;
    db.commit().await.map_err(AppError::from)?;
    let edited = edit_github_comment(
        &state,
        &domains,
        &CommentTarget::from_link(&primary),
        author,
        &params.comment_id,
        &request.body,
        &request.expected_updated_at,
    )
    .await?;
    Ok(Json(ResponseEnvelope::ok(edited)))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EditNativeCommentRequest {
    pub body: String,
    /// Accepted for the same request shape as GitHub edits; a native edit is
    /// conditional on `expectedUpdatedAt` and repeating it is harmless.
    #[serde(default)]
    pub idempotency_key: Option<String>,
    /// The destination the author saw (workspace edits only).
    #[serde(default)]
    pub destination: Option<CommentDestinationDTO>,
    /// The comment's `updatedAt` when the editor opened.
    pub expected_updated_at: String,
}

#[derive(Debug, Deserialize)]
pub struct NativeCommentPathParams {
    #[serde(rename = "workspaceId")]
    pub workspace_id: Uuid,
    #[serde(rename = "commentId")]
    pub comment_id: String,
}

fn comment_number(raw: &str) -> Result<i64, AppError> {
    raw.parse::<i64>()
        .ok()
        .filter(|id| *id > 0 && !raw.starts_with('0'))
        .ok_or_else(|| {
            AppError::from_error(G5Error::ValidationError {
                details: json!({"field": "commentId", "error": "not a G5 comment id"}),
            })
        })
}

/// `PATCH /api/v1/workspaces/:workspaceId/activity/comments/g5/:commentId`
///
/// Replace the text of the caller's own comment on the workspace's primary
/// G5 issue, only if it is unchanged since the editor opened.
async fn edit_native_workspace_comment(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(params): Path<NativeCommentPathParams>,
    Json(request): Json<EditNativeCommentRequest>,
) -> Result<Json<ResponseEnvelope<CommentResultDTO>>, AppError> {
    let author = auth.require_user_id().map_err(AppError::from_error)?;
    let comment_id = comment_number(&params.comment_id)?;
    let mut db = comment_db(&state, &auth, params.workspace_id).await?;
    let org_id = workspace_org_id(&mut db, params.workspace_id).await?;
    ensure_external_issues_enabled(&mut db, org_id).await?;
    let primary = list_workspace_links(&mut db, org_id, params.workspace_id)
        .await?
        .into_iter()
        .find(|link| link.is_primary)
        .ok_or_else(|| {
            AppError::from_error(comment_conflict(
                "no_primary_issue",
                "the workspace has no primary issue",
            ))
        })?;
    if request.destination.as_ref().is_some_and(|destination| {
        primary.tracker_id != destination.tracker_id
            || primary.external_issue_id != destination.external_issue_id
    }) {
        return Err(AppError::from_error(comment_conflict(
            "destination_changed",
            "the workspace's primary issue changed; review the destination and submit again",
        )));
    }
    let domains = list_domain_access(&mut db, org_id).await?;
    let linked = link_domain(&domains, &primary).map_err(issue_error)?;
    let LinkedLocation::Native { issue_id } = linked.location else {
        return Err(AppError::from_error(G5Error::ValidationError {
            details: json!({"field": "commentId", "error": "the primary issue is not a G5 issue"}),
        }));
    };
    let site = native_issues::native_site(&state.system_pool, &state.external_url, org_id).await?;
    let reader = NativeReader {
        domain: &linked.access.domain,
        site: &site,
        system_pool: &state.system_pool,
    };
    let comment = native_issues::edit_comment(
        &mut db,
        &reader,
        issue_id,
        comment_id,
        author,
        &request.body,
        &request.expected_updated_at,
    )
    .await?;
    db.commit().await.map_err(AppError::from)?;
    Ok(Json(ResponseEnvelope::ok(CommentResultDTO::Native(
        NativeCommentDTO::new(&comment, &primary.display_key, "edit"),
    ))))
}

/// `POST /api/v1/workspaces/:workspaceId/issue-links`
///
/// Resolve the reference inside a configured domain (GitHub, no transaction
/// open), then create the association in a transaction that locks the live
/// workspace row. Conflicts are 409 with `details.code`.
async fn create_workspace_issue_link(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(params): Path<WorkspaceIssueLinksPathParams>,
    Json(request): Json<CreateWorkspaceIssueLinkRequest>,
) -> Result<(StatusCode, Json<ResponseEnvelope<WorkspaceIssueLinkDTO>>), AppError> {
    let role = LinkRole::parse(request.role.trim()).ok_or_else(|| {
        AppError::from_error(G5Error::ValidationError {
            details: json!({"field": "role", "error": "must be primary or related"}),
        })
    })?;
    let actor = auth.require_user_id().map_err(AppError::from_error)?;
    let mut db =
        scoped_db_for_workspace(&state, &auth, params.workspace_id, WORKSPACE_WRITE_SCOPE).await?;
    let org_id = workspace_org_id(&mut db, params.workspace_id).await?;
    ensure_external_issues_enabled(&mut db, org_id).await?;
    let domains = list_domain_access(&mut db, org_id).await?;
    db.commit().await.map_err(AppError::from)?;

    let open_db =
        || scoped_db_for_workspace(&state, &auth, params.workspace_id, WORKSPACE_WRITE_SCOPE);
    let opener = OpenDb(&open_db);
    let backends = issue_backends(&state, org_id, &domains, &opener).await?;
    let mut summary = resolve_reference(&backends, &domains, &request.reference)
        .await
        .map_err(issue_error)?
        .summary;

    let mut db = open_db().await?;
    let link = create_workspace_link(
        &mut db,
        &state.system_pool,
        Some(actor),
        org_id,
        params.workspace_id,
        actor,
        &summary,
        role,
    )
    .await?;
    attach_primary_owners(
        &mut db,
        &state.system_pool,
        Some(actor),
        org_id,
        &mut [&mut summary],
    )
    .await?;
    db.commit().await.map_err(AppError::from)?;
    tracing::info!(
        workspace_id = %params.workspace_id,
        link_id = %link.id,
        primary = link.is_primary,
        "workspace issue linked"
    );
    Ok((
        StatusCode::CREATED,
        Json(ResponseEnvelope::ok(link.to_dto(Some(summary), None))),
    ))
}

/// `POST /api/v1/workspaces/:workspaceId/issue-links/:linkId/unlink`
///
/// Ends the association (and any primary claim). The external issue is not
/// changed.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ChangeIssueStateRequest {
    /// `closed` (as completed) or `open`.
    pub state: String,
}

/// The result of closing or reopening a linked issue by hand.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IssueStateChangeDTO {
    /// The state that was asked for: `closed` or `open`.
    pub requested: &'static str,
    /// `applied` (GitHub answered with the issue) or `unknown` (no definite
    /// answer; the write is idempotent, so read the issue or send again).
    pub outcome: &'static str,
    /// The issue as the tracker answered, for `applied`.
    pub issue: Option<ExternalIssueSummaryDTO>,
    /// The GitHub account the change was made as; `None` for G5 issues.
    pub github_login: Option<String>,
    pub message: Option<String>,
}

/// `POST /api/v1/workspaces/:workspaceId/issue-links/:linkId/state`
///
/// Close (as completed) or reopen one of the workspace's linked issues as
/// the caller's own GitHub account (`g5.external-issues.transition`). The
/// same destination rules as comments apply: the issue's domain must allow
/// G5 comments and GitHub must report the repository as non-public now.
/// Nothing is recorded: the write is idempotent, so a retry sends the same
/// state again, and an unclear answer asks the person to look at the issue.
async fn change_linked_issue_state(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(params): Path<WorkspaceIssueLinkPathParams>,
    Json(request): Json<ChangeIssueStateRequest>,
) -> Result<Json<ResponseEnvelope<IssueStateChangeDTO>>, AppError> {
    let actor = auth.require_user_id().map_err(AppError::from_error)?;
    let db = comment_db(&state, &auth, params.workspace_id).await?;
    let changed = change_linked_issue_state_as(
        &state,
        db,
        actor,
        params.workspace_id,
        params.link_id,
        &request.state,
    )
    .await?;
    Ok(Json(ResponseEnvelope::ok(changed)))
}

/// Close or reopen one of `workspace_id`'s linked issues as `actor`, under
/// their connected GitHub account or in G5's own tracker
/// (`g5.external-issues.transition`), on a scope that already admitted
/// `actor` to write the workspace: the person's route from that workspace.
pub(crate) async fn change_linked_issue_state_as(
    state: &Arc<AppState>,
    mut db: g5_authz::ScopedDb,
    actor: Uuid,
    workspace_id: Uuid,
    link_id: Uuid,
    requested_state: &str,
) -> Result<IssueStateChangeDTO, AppError> {
    let change = parse_state_change(requested_state)?;
    let org_id = workspace_org_id(&mut db, workspace_id).await?;
    ensure_external_issues_enabled(&mut db, org_id).await?;
    ensure_writable_issues(&mut db, org_id, Some(workspace_id), "Mark as done").await?;
    let link = list_workspace_links(&mut db, org_id, workspace_id)
        .await?
        .into_iter()
        .find(|link| link.id == link_id)
        .ok_or_else(|| {
            issue_error(IssueError::new(
                ExternalIssueErrorKind::NotFound,
                "the issue link is not in this workspace",
            ))
        })?;
    let domains = list_domain_access(&mut db, org_id).await?;
    let target = CommentTarget::from_link(&link);
    transition(
        state,
        db,
        &IssueWriter {
            org_id,
            workspace_id: Some(workspace_id),
            actor,
            domains: &domains,
        },
        &target,
        change,
    )
    .await
}

/// A review copy follows its source and changes no issue, as its controls
/// say.
pub(crate) async fn ensure_writable_issues(
    db: &mut g5_authz::ScopedDb,
    org_id: Uuid,
    workspace_id: Option<Uuid>,
    action: &'static str,
) -> Result<(), AppError> {
    let Some(workspace_id) = workspace_id else {
        return Ok(());
    };
    crate::services::workspace_review_service::ensure_not_review_workspace(
        db,
        org_id,
        workspace_id,
        action,
    )
    .await
    .map_err(Into::into)
}

/// Who writes an issue, from which workspace if any, through which
/// configured domains: a person from their workspace or the issue page, or a
/// workspace's agent as its owner. `db` scopes passed alongside already
/// admitted `actor` to write that workspace, or the organization.
pub(crate) struct IssueWriter<'a> {
    pub org_id: Uuid,
    pub workspace_id: Option<Uuid>,
    pub actor: Uuid,
    pub domains: &'a [IssueDomainAccess],
}

/// Where a write to `target` goes: G5's own tracker or a GitHub repository,
/// in one of the configured domains.
pub(crate) fn target_location<'a>(
    domains: &'a [IssueDomainAccess],
    target: &CommentTarget,
) -> Result<(&'a IssueDomainAccess, LinkedLocation), AppError> {
    if target.is_native() {
        let access = domains
            .iter()
            .find(|access| access.is_native() && access.domain.tracker_id == target.tracker_id)
            .ok_or_else(|| {
                issue_error(IssueError::new(
                    ExternalIssueErrorKind::OutsideDomain,
                    "the organization no longer uses G5 issues",
                ))
            })?;
        let issue_id =
            native_issues::parse_issue_id(&target.external_issue_id).map_err(issue_error)?;
        return Ok((access, LinkedLocation::Native { issue_id }));
    }
    let repository_id = parse_canonical_github_id(&target.repository_id).ok_or_else(|| {
        issue_error(IssueError::new(
            ExternalIssueErrorKind::NotFound,
            "the issue has no GitHub repository locator",
        ))
    })?;
    let access = comment_domain(domains, target.tracker_id, repository_id)?;
    Ok((access, LinkedLocation::Github { repository_id }))
}

/// The GitHub issue a write answered with must be the one admitted.
pub(crate) fn ensure_same_issue(
    target: &CommentTarget,
    issue: &GithubIssue,
) -> Result<(), AppError> {
    if issue.id == target.external_issue_id {
        Ok(())
    } else {
        Err(issue_error(IssueError::new(
            ExternalIssueErrorKind::NotFound,
            "the GitHub issue has moved",
        )))
    }
}

/// Close (as completed) or reopen `target` as the writer's actor
/// (`g5.external-issues.transition`): in this transaction for a G5 issue,
/// or under their connected GitHub account through the comment guards. A
/// review copy changes nothing.
pub(crate) async fn change_issue_state_as(
    state: &Arc<AppState>,
    mut db: g5_authz::ScopedDb,
    writer: &IssueWriter<'_>,
    target: &CommentTarget,
    requested_state: &str,
) -> Result<IssueStateChangeDTO, AppError> {
    let change = parse_state_change(requested_state)?;
    ensure_writable_issues(&mut db, writer.org_id, writer.workspace_id, "Mark as done").await?;
    transition(state, db, writer, target, change).await
}

async fn transition(
    state: &Arc<AppState>,
    mut db: g5_authz::ScopedDb,
    writer: &IssueWriter<'_>,
    target: &CommentTarget,
    change: IssueStateChange,
) -> Result<IssueStateChangeDTO, AppError> {
    let IssueWriter {
        org_id,
        workspace_id,
        actor,
        domains,
    } = *writer;
    let (access, location) = target_location(domains, target)?;
    let repository_id = match location {
        LinkedLocation::Native { issue_id } => {
            // A G5 issue changes in this transaction (`g5.issue.native.writes`).
            let site =
                native_issues::native_site(&state.system_pool, &state.external_url, org_id).await?;
            let reader = NativeReader {
                domain: &access.domain,
                site: &site,
                system_pool: &state.system_pool,
            };
            let issue = native_issues::set_state(
                &mut db,
                &reader,
                org_id,
                issue_id,
                actor,
                change == IssueStateChange::Close,
            )
            .await?;
            db.commit().await.map_err(AppError::from)?;
            tracing::info!(
                workspace_id = ?workspace_id,
                issue = %target.display_key,
                actor = %actor,
                requested = change.state(),
                "native issue state change"
            );
            return Ok(IssueStateChangeDTO {
                requested: change.state(),
                outcome: "applied",
                issue: Some(issue),
                github_login: None,
                message: None,
            });
        }
        LinkedLocation::Github { repository_id } => repository_id,
    };
    db.commit().await.map_err(AppError::from)?;
    let client = github_client(state)?;
    let token_manager = comment_token_manager(state)?;
    let acting = admit_person_write(
        client,
        token_manager,
        &state.system_pool,
        access,
        actor,
        PersonWrite::Transition,
    )
    .await?;

    let outcome =
        update_issue_state(client, &acting.token, repository_id, target.number, change).await;
    let (outcome_name, issue, message) = match outcome {
        IssueUpdateOutcome::Updated(issue) => {
            ensure_same_issue(target, &issue)?;
            (
                "applied",
                Some(summary(access, repository_id, &issue)),
                None,
            )
        }
        IssueUpdateOutcome::Rejected(error) => return Err(issue_error(error)),
        IssueUpdateOutcome::Unknown(message) => ("unknown", None, Some(message)),
    };
    tracing::info!(
        workspace_id = ?workspace_id,
        issue = %target.display_key,
        actor = %actor,
        requested = change.state(),
        outcome = outcome_name,
        "issue state change"
    );
    Ok(IssueStateChangeDTO {
        requested: change.state(),
        outcome: outcome_name,
        issue,
        github_login: Some(acting.github_login),
        message,
    })
}

/// An edit of an issue's title, description or assignees, naming the people
/// to add and remove (`g5.external-issues.edit`): GitHub logins on GitHub,
/// member emails on G5 issues; `@me` is the actor.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct IssueChange<'a> {
    pub title: Option<&'a str>,
    pub body: Option<&'a str>,
    pub add: &'a [String],
    pub remove: &'a [String],
    /// What a person's form showed for each field it changes: the edit is
    /// refused when the issue no longer has it. `None` (the agent) applies
    /// the change to the issue as read, like `gh issue edit`.
    pub seen: Option<&'a IssueSeen>,
    /// The agent's conditional description edit (`g5d issue create
    /// --attach`): G5 issues only.
    pub expected_body: Option<&'a str>,
}

/// The values an edit form started from (`g5.external-issues.edit`).
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct IssueSeen {
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub body: Option<String>,
    /// GitHub logins.
    #[serde(default)]
    pub assignees: Option<Vec<String>>,
}

/// Whether `current` still holds what `seen` shows for each field `draft`
/// changes. GitHub bumps an issue's update time on every comment, so the
/// fields are compared rather than the version: assignees as a set without
/// regard to case, the description without G5's creation marker.
fn still_as_seen(
    current: &GithubIssue,
    draft: &crate::services::issue_creations::IssueEditDraft,
    seen: &IssueSeen,
) -> Result<bool, AppError> {
    let missing = |field: &str| {
        AppError::from_error(G5Error::ValidationError {
            details: json!({
                "field": format!("seen.{field}"),
                "error": "send the value the form showed for each field it changes",
            }),
        })
    };
    if draft.title.is_some() {
        let shown = seen.title.as_deref().ok_or_else(|| missing("title"))?;
        if shown.trim() != current.title.trim() {
            return Ok(false);
        }
    }
    if draft.body.is_some() {
        let shown = seen.body.as_deref().ok_or_else(|| missing("body"))?;
        let held = description_without_marker(current.body.as_deref().unwrap_or_default());
        if shown.trim_end() != held.trim_end() {
            return Ok(false);
        }
    }
    if !draft.add.is_empty() || !draft.remove.is_empty() {
        let shown = seen
            .assignees
            .as_ref()
            .ok_or_else(|| missing("assignees"))?;
        let lower = |logins: &mut dyn Iterator<Item = &str>| {
            logins
                .map(str::to_ascii_lowercase)
                .collect::<std::collections::BTreeSet<_>>()
        };
        let held = lower(&mut current.assignees.iter().map(|user| user.login.as_str()));
        if lower(&mut shown.iter().map(String::as_str)) != held {
            return Ok(false);
        }
    }
    Ok(true)
}

/// The result of an issue edit.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IssueEditDTO {
    /// `applied`; `partial` (GitHub applied the write but its answer lacks a
    /// requested assignee change, listed in `notAssigned`/`notUnassigned`);
    /// or `unknown` (no definite answer: setting the same values again is
    /// harmless, so run the edit again).
    pub outcome: &'static str,
    /// The issue as the tracker answered, unless `unknown`.
    pub issue: Option<ExternalIssueSummaryDTO>,
    /// The GitHub account the edit was made as; `None` for G5 issues.
    pub github_login: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub not_assigned: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub not_unassigned: Vec<String>,
    pub message: Option<String>,
    /// The label change the same request made (`g5.issue.labels.contract`),
    /// with its own per-label outcomes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub labels: Option<crate::services::issue_labels::IssueLabelChangeDTO>,
}

fn invalid_assignees(code: &str, error: String) -> AppError {
    AppError::from_error(G5Error::ValidationError {
        details: json!({"field": "assignees", "code": code, "error": error}),
    })
}

/// Edit `target` as the writer's actor (`g5.external-issues.edit`). A G5
/// issue changes in this transaction, against the version it reads. On
/// GitHub the comment guards admit the actor's connected account, title and
/// description are screened, every added login must be assignable, and one
/// write sets the new title, description (keeping a creation marker) and
/// assignee set computed from the issue as read just before; GitHub's answer
/// decides `applied` or `partial`. A review copy changes nothing.
pub(crate) async fn edit_issue_as(
    state: &Arc<AppState>,
    mut db: g5_authz::ScopedDb,
    writer: &IssueWriter<'_>,
    target: &CommentTarget,
    change: &IssueChange<'_>,
) -> Result<IssueEditDTO, AppError> {
    let IssueWriter {
        org_id,
        workspace_id,
        actor,
        domains,
    } = *writer;
    ensure_writable_issues(&mut db, org_id, workspace_id, "Edit issue").await?;
    let (access, location) = target_location(domains, target)?;
    let repository_id = match location {
        LinkedLocation::Native { issue_id } => {
            let site =
                native_issues::native_site(&state.system_pool, &state.external_url, org_id).await?;
            let reader = NativeReader {
                domain: &access.domain,
                site: &site,
                system_pool: &state.system_pool,
            };
            let issue = native_issues::change_issue(
                &mut db,
                &reader,
                org_id,
                issue_id,
                actor,
                &NativeIssueChange {
                    title: change.title,
                    body: change.body,
                    add: change.add,
                    remove: change.remove,
                    expected_body: change.expected_body,
                },
            )
            .await?;
            db.commit().await.map_err(AppError::from)?;
            tracing::info!(
                workspace_id = ?workspace_id,
                issue = %target.display_key,
                actor = %actor,
                "native issue edit"
            );
            return Ok(IssueEditDTO {
                outcome: "applied",
                issue: Some(issue),
                github_login: None,
                not_assigned: Vec::new(),
                not_unassigned: Vec::new(),
                message: None,
                labels: None,
            });
        }
        LinkedLocation::Github { repository_id } => repository_id,
    };
    db.commit().await.map_err(AppError::from)?;
    if change.expected_body.is_some() {
        return Err(AppError::from_error(G5Error::ValidationError {
            details: json!({
                "field": "expectedBody",
                "error": "a conditional description edit is for G5 issues only",
            }),
        }));
    }
    let draft = validate_edit(change.title, change.body, change.add, change.remove)?;
    let client = github_client(state)?;
    let token_manager = comment_token_manager(state)?;
    let acting = admit_person_write(
        client,
        token_manager,
        &state.system_pool,
        access,
        actor,
        PersonWrite::Edit,
    )
    .await?;
    let add = resolve_assignees(&draft.add, &acting.github_login);
    let remove = resolve_assignees(&draft.remove, &acting.github_login);
    let named =
        |logins: &[String], login: &str| logins.iter().any(|l| l.eq_ignore_ascii_case(login));
    if let Some(both) = add.iter().find(|login| named(&remove, login)) {
        return Err(invalid_assignees(
            "assignee_conflict",
            format!("{both} is both added and removed"),
        ));
    }
    for login in &add {
        if !domain_login_assignable(client, access, login)
            .await
            .map_err(issue_error)?
        {
            return Err(invalid_assignees(
                "invalid_assignee",
                format!(
                    "{login} cannot be assigned issues in {}",
                    access.domain.display_key
                ),
            ));
        }
    }
    let current = domain_issue(client, access, target.number)
        .await
        .map_err(issue_error)?;
    ensure_same_issue(target, &current)?;
    if let Some(seen) = change.seen {
        if !still_as_seen(&current, &draft, seen)? {
            return Err(AppError::from_error(comment_conflict(
                "issue_changed",
                "the issue changed on GitHub since you started editing; reload it and try again",
            )));
        }
    }
    let assignees = (!add.is_empty() || !remove.is_empty()).then(|| {
        let mut logins: Vec<String> = current
            .assignees
            .iter()
            .map(|user| user.login.clone())
            .filter(|login| !named(&remove, login))
            .collect();
        for login in &add {
            if !named(&logins, login) {
                logins.push(login.clone());
            }
        }
        logins
    });
    if assignees
        .as_ref()
        .is_some_and(|logins| logins.len() > MAX_ASSIGNEES)
    {
        return Err(invalid_assignees(
            "too_many_assignees",
            format!("an issue can have at most {MAX_ASSIGNEES} assignees"),
        ));
    }
    let edit = IssueEdit {
        title: draft.title.clone(),
        body: draft
            .body
            .as_deref()
            .map(|body| edited_description(current.body.as_deref(), body)),
        assignees,
    };
    let outcome = update_issue(client, &acting.token, repository_id, target.number, &edit).await;
    let mut result = IssueEditDTO {
        outcome: "applied",
        issue: None,
        github_login: Some(acting.github_login),
        not_assigned: Vec::new(),
        not_unassigned: Vec::new(),
        message: None,
        labels: None,
    };
    match outcome {
        IssueUpdateOutcome::Updated(issue) => {
            ensure_same_issue(target, &issue)?;
            let answered: Vec<String> = issue
                .assignees
                .iter()
                .map(|user| user.login.clone())
                .collect();
            result.not_assigned = add
                .into_iter()
                .filter(|login| !named(&answered, login))
                .collect();
            result.not_unassigned = remove
                .into_iter()
                .filter(|login| named(&answered, login))
                .collect();
            if !result.not_assigned.is_empty() || !result.not_unassigned.is_empty() {
                result.outcome = "partial";
                result.message = Some(
                    "GitHub applied the edit but not every assignee change; the owner's account may not be allowed to assign issues here"
                        .to_string(),
                );
            }
            result.issue = Some(summary(access, repository_id, &issue));
        }
        IssueUpdateOutcome::Rejected(error) => return Err(issue_error(error)),
        IssueUpdateOutcome::Unknown(message) => {
            result.outcome = "unknown";
            result.message = Some(message);
        }
    }
    tracing::info!(
        workspace_id = ?workspace_id,
        issue = %target.display_key,
        actor = %actor,
        outcome = result.outcome,
        "issue edit"
    );
    Ok(result)
}

fn parse_state_change(state: &str) -> Result<IssueStateChange, AppError> {
    match state {
        "closed" => Ok(IssueStateChange::Close),
        "open" => Ok(IssueStateChange::Reopen),
        _ => Err(AppError::from_error(G5Error::ValidationError {
            details: json!({
                "field": "state",
                "error": "must be `closed` or `open`",
            }),
        })),
    }
}

async fn unlink_workspace_issue_link(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(params): Path<WorkspaceIssueLinkPathParams>,
) -> Result<Json<ResponseEnvelope<UnlinkWorkspaceIssueLinkDTO>>, AppError> {
    let actor = auth.require_user_id().map_err(AppError::from_error)?;
    let mut db =
        scoped_db_for_workspace(&state, &auth, params.workspace_id, WORKSPACE_WRITE_SCOPE).await?;
    let org_id = workspace_org_id(&mut db, params.workspace_id).await?;
    unlink_workspace_link(&mut db, org_id, params.workspace_id, params.link_id, actor).await?;
    db.commit().await.map_err(AppError::from)?;
    Ok(Json(ResponseEnvelope::ok(UnlinkWorkspaceIssueLinkDTO {
        unlinked: true,
    })))
}

// ─── GitHub issue page (g5.issue.native.page) ────────────────────────────────
//
// A GitHub issue's G5 page offers the adapter's write operations as a G5
// issue's page does, through the same operations as the workspace and the
// agent, with no workspace: the person's own GitHub account and the comment
// guards apply to every write.

#[derive(Debug, Deserialize)]
pub struct GithubIssueCommentPathParams {
    #[serde(rename = "orgId")]
    pub org_id: Uuid,
    #[serde(rename = "repositoryId")]
    pub repository_id: String,
    pub number: String,
    #[serde(rename = "commentId")]
    pub comment_id: String,
}

/// The GitHub issue an issue page names, read in one of the organization's
/// domains, and an organization scope admitting the caller to write. No
/// transaction is held while GitHub is read.
pub(crate) async fn github_page_target(
    state: &Arc<AppState>,
    auth: &AuthContext,
    org_id: Uuid,
    repository_id: &str,
    number: &str,
) -> Result<(g5_authz::ScopedDb, Vec<IssueDomainAccess>, CommentTarget), AppError> {
    let client = github_client(state)?;
    let (repository_id, number) = github_issue_path(&GithubIssuePathParams {
        org_id,
        repository_id: repository_id.to_string(),
        number: number.to_string(),
    })?;
    let mut db = scoped_db_for_org(state, auth, org_id, MEMBER_WRITE_SCOPE)
        .await
        .map_err(mask_foreign_org_access)?;
    ensure_external_issues_enabled(&mut db, org_id).await?;
    let domains = list_domain_access(&mut db, org_id).await?;
    db.commit().await.map_err(AppError::from)?;
    let index = domain_for_repository(&domains, repository_id).map_err(issue_error)?;
    let issue = domain_issue(client, &domains[index], number)
        .await
        .map_err(issue_error)?;
    let target = CommentTarget::from_summary(&summary(&domains[index], repository_id, &issue));
    let db = scoped_db_for_org(state, auth, org_id, MEMBER_WRITE_SCOPE)
        .await
        .map_err(mask_foreign_org_access)?;
    Ok((db, domains, target))
}

/// What a GitHub issue's page needs to offer its writes.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IssueWriteTargetDTO {
    /// `ready`, `connection_required`, `comments_disabled`,
    /// `public_destination`, `visibility_unknown` or `unavailable`: the
    /// comment composer's states, for every write on the page.
    pub state: &'static str,
    /// The GitHub account writes are made as, once connected.
    pub github_login: Option<String>,
    /// The domain is marked trusted: no review step before sending.
    pub content_trusted: bool,
    pub display_key: String,
    pub destination: CommentDestinationDTO,
    /// Accounts GitHub lets the repository's issues be assigned to (first
    /// page); empty when they could not be read.
    pub assignable_users: Vec<ExternalIssueUserDTO>,
    /// Why the target could not be read, for `unavailable`.
    pub error: Option<IssueErrorDTO>,
}

/// `GET /api/v1/orgs/:orgId/issues/github/:repositoryId/:number/write-target`
///
/// Whether the caller can write this GitHub issue from G5 now, as whom, and
/// whom it can be assigned to. The checks are those each write repeats.
async fn get_github_write_target(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(params): Path<GithubIssuePathParams>,
) -> Result<Json<ResponseEnvelope<IssueWriteTargetDTO>>, AppError> {
    let viewer = auth.require_user_id().map_err(AppError::from_error)?;
    let client = github_client(&state)?;
    let (repository_id, number) = github_issue_path(&params)?;
    let mut db = scoped_db_for_org(&state, &auth, params.org_id, MEMBER_READ_SCOPE)
        .await
        .map_err(mask_foreign_org_access)?;
    ensure_external_issues_enabled(&mut db, params.org_id).await?;
    let domains = list_domain_access(&mut db, params.org_id).await?;
    db.commit().await.map_err(AppError::from)?;
    let access = &domains[domain_for_repository(&domains, repository_id).map_err(issue_error)?];
    let issue = domain_issue(client, access, number)
        .await
        .map_err(issue_error)?;
    let issue = summary(access, repository_id, &issue);
    let mut target = IssueWriteTargetDTO {
        state: "comments_disabled",
        github_login: grant_status(&state.system_pool, viewer).await?.github_login,
        content_trusted: access.domain.content_trusted,
        display_key: issue.display_key.clone(),
        destination: CommentDestinationDTO {
            tracker_id: issue.tracker_id,
            external_issue_id: issue.external_issue_id.clone(),
        },
        assignable_users: Vec::new(),
        error: None,
    };
    if !access.domain.comments_enabled {
        return Ok(Json(ResponseEnvelope::ok(target)));
    }
    let (audience, users) = tokio::join!(
        domain_repository_audience(client, access),
        domain_assignable_users(client, access)
    );
    target.assignable_users = users.unwrap_or_default();
    target.state = match audience {
        Ok(RepositoryAudience::NonPublic) if target.github_login.is_none() => "connection_required",
        Ok(RepositoryAudience::NonPublic) => "ready",
        Ok(RepositoryAudience::Public) => "public_destination",
        Ok(RepositoryAudience::Unknown) => "visibility_unknown",
        Err(error) => {
            target.error = Some(IssueErrorDTO::from(&error));
            "unavailable"
        }
    };
    Ok(Json(ResponseEnvelope::ok(target)))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EditGithubIssueRequest {
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub body: Option<String>,
    /// The complete new set of assignees' logins.
    #[serde(default)]
    pub assignees: Option<Vec<String>>,
    /// What the form showed for each field it changes.
    pub seen: IssueSeen,
}

/// `PATCH /api/v1/orgs/:orgId/issues/github/:repositoryId/:number`
///
/// Edit a GitHub issue's title, description or assignees as the caller's
/// GitHub account (`g5.external-issues.edit`), only if each changed field
/// still holds what the form showed (`issue_changed` otherwise).
async fn edit_github_page_issue(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(params): Path<GithubIssuePathParams>,
    Json(request): Json<EditGithubIssueRequest>,
) -> Result<Json<ResponseEnvelope<IssueEditDTO>>, AppError> {
    let actor = auth.require_user_id().map_err(AppError::from_error)?;
    let (add, remove) = match &request.assignees {
        Some(wanted) => {
            let shown = request.seen.assignees.as_deref().ok_or_else(|| {
                AppError::from_error(G5Error::ValidationError {
                    details: json!({
                        "field": "seen.assignees",
                        "error": "send the assignees the form showed",
                    }),
                })
            })?;
            let missing_from = |logins: &[String], login: &String| {
                !logins.iter().any(|other| other.eq_ignore_ascii_case(login))
            };
            (
                wanted
                    .iter()
                    .filter(|login| missing_from(shown, login))
                    .cloned()
                    .collect::<Vec<_>>(),
                shown
                    .iter()
                    .filter(|login| missing_from(wanted, login))
                    .cloned()
                    .collect::<Vec<_>>(),
            )
        }
        None => (Vec::new(), Vec::new()),
    };
    let (db, domains, target) = github_page_target(
        &state,
        &auth,
        params.org_id,
        &params.repository_id,
        &params.number,
    )
    .await?;
    let edited = edit_issue_as(
        &state,
        db,
        &IssueWriter {
            org_id: params.org_id,
            workspace_id: None,
            actor,
            domains: &domains,
        },
        &target,
        &IssueChange {
            title: request.title.as_deref(),
            body: request.body.as_deref(),
            add: &add,
            remove: &remove,
            seen: Some(&request.seen),
            expected_body: None,
        },
    )
    .await?;
    Ok(Json(ResponseEnvelope::ok(edited)))
}

/// `POST /api/v1/orgs/:orgId/issues/github/:repositoryId/:number/state`
///
/// Close (as completed) or reopen a GitHub issue as the caller's GitHub
/// account (`g5.external-issues.transition`).
async fn change_github_page_issue_state(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(params): Path<GithubIssuePathParams>,
    Json(request): Json<ChangeIssueStateRequest>,
) -> Result<Json<ResponseEnvelope<IssueStateChangeDTO>>, AppError> {
    let actor = auth.require_user_id().map_err(AppError::from_error)?;
    parse_state_change(&request.state)?;
    let (db, domains, target) = github_page_target(
        &state,
        &auth,
        params.org_id,
        &params.repository_id,
        &params.number,
    )
    .await?;
    let changed = change_issue_state_as(
        &state,
        db,
        &IssueWriter {
            org_id: params.org_id,
            workspace_id: None,
            actor,
            domains: &domains,
        },
        &target,
        &request.state,
    )
    .await?;
    Ok(Json(ResponseEnvelope::ok(changed)))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PostPageCommentRequest {
    pub body: String,
    /// Stable across retries of the same comment.
    pub idempotency_key: String,
}

/// `POST /api/v1/orgs/:orgId/issues/github/:repositoryId/:number/comments`
///
/// Comment on a GitHub issue as the caller's GitHub account through the
/// comment guards; a retry with the same key finds the earlier send
/// (`g5.issue.comment-recovery`).
async fn post_github_page_comment(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(params): Path<GithubIssuePathParams>,
    Json(request): Json<PostPageCommentRequest>,
) -> Result<(StatusCode, Json<ResponseEnvelope<CommentResultDTO>>), AppError> {
    let author = auth.require_user_id().map_err(AppError::from_error)?;
    validate_key(&request.idempotency_key, 200).map_err(AppError::from_error)?;
    let (db, domains, target) = github_page_target(
        &state,
        &auth,
        params.org_id,
        &params.repository_id,
        &params.number,
    )
    .await?;
    db.commit().await.map_err(AppError::from)?;
    let outcome = publish_issue_comment(
        &state,
        &IssueComment {
            org_id: params.org_id,
            target: &target,
            domains: &domains,
            author,
            idempotency_key: &request.idempotency_key,
            body: &request.body,
            origin: "person",
        },
        || scoped_db_for_org(&state, &auth, params.org_id, MEMBER_WRITE_SCOPE),
    )
    .await?;
    let status = if outcome.fresh() {
        StatusCode::CREATED
    } else {
        StatusCode::OK
    };
    Ok((status, Json(ResponseEnvelope::ok(outcome.into_dto()))))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EditPageCommentRequest {
    pub body: String,
    /// The comment's `updatedAt` when the editor opened; checked as for a
    /// workspace comment edit, best effort.
    pub expected_updated_at: String,
}

/// `PATCH /api/v1/orgs/:orgId/issues/github/:repositoryId/:number/comments/:commentId`
///
/// Replace the text of the caller's own comment on a GitHub issue
/// ([`edit_github_comment`]).
async fn edit_github_page_comment(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(params): Path<GithubIssueCommentPathParams>,
    Json(request): Json<EditPageCommentRequest>,
) -> Result<Json<ResponseEnvelope<GithubCommentDTO>>, AppError> {
    let actor = auth.require_user_id().map_err(AppError::from_error)?;
    let (db, domains, target) = github_page_target(
        &state,
        &auth,
        params.org_id,
        &params.repository_id,
        &params.number,
    )
    .await?;
    db.commit().await.map_err(AppError::from)?;
    let edited = edit_github_comment(
        &state,
        &domains,
        &target,
        actor,
        &params.comment_id,
        &request.body,
        &request.expected_updated_at,
    )
    .await?;
    Ok(Json(ResponseEnvelope::ok(edited)))
}

// ─── Native issue page (g5.issue.native.page) ────────────────────────────────

/// The org's native domain and an org-scoped transaction with write
/// authority, for an issue page action.
pub(crate) async fn native_page_db(
    state: &Arc<AppState>,
    auth: &AuthContext,
    org_id: Uuid,
) -> Result<
    (
        g5_authz::ScopedDb,
        IssueDomainAccess,
        native_issues::NativeSite,
    ),
    AppError,
> {
    let mut db = scoped_db_for_org(state, auth, org_id, MEMBER_WRITE_SCOPE)
        .await
        .map_err(mask_foreign_org_access)?;
    ensure_external_issues_enabled(&mut db, org_id).await?;
    let domains = list_domain_access(&mut db, org_id).await?;
    let access = native_domain(&domains).map_err(issue_error)?.clone();
    let site = native_issues::native_site(&state.system_pool, &state.external_url, org_id).await?;
    Ok((db, access, site))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NativeIssueCommentRequest {
    pub body: String,
    /// Stable across retries of the same comment.
    pub idempotency_key: String,
}

/// `POST /api/v1/orgs/:orgId/issues/g5/:number/comments`
///
/// Comment on any G5 issue as the caller's G5 account. A retry with the
/// same key returns the first comment.
async fn post_native_issue_comment(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(params): Path<NativeIssuePathParams>,
    Json(request): Json<NativeIssueCommentRequest>,
) -> Result<(StatusCode, Json<ResponseEnvelope<CommentResultDTO>>), AppError> {
    let author = auth.require_user_id().map_err(AppError::from_error)?;
    let number = issue_number(&params.number)?;
    validate_key(&request.idempotency_key, 200)?;
    let (mut db, access, site) = native_page_db(&state, &auth, params.org_id).await?;
    let reader = NativeReader {
        domain: &access.domain,
        site: &site,
        system_pool: &state.system_pool,
    };
    let issue_id = native_issues::issue_id_by_number(&mut db, &reader, number).await?;
    let (comment, fresh) = native_issues::add_comment(
        &mut db,
        &reader,
        &NewNativeComment {
            org_id: params.org_id,
            issue_id,
            author,
            origin: "person",
            idempotency_key: &request.idempotency_key,
            body: &request.body,
        },
    )
    .await?;
    db.commit().await.map_err(AppError::from)?;
    let display_key = format!("{}-{number}", access.domain.display_key);
    let status = if fresh {
        StatusCode::CREATED
    } else {
        StatusCode::OK
    };
    Ok((
        status,
        Json(ResponseEnvelope::ok(CommentResultDTO::Native(
            NativeCommentDTO::new(&comment, &display_key, "comment"),
        ))),
    ))
}

#[derive(Debug, Deserialize)]
pub struct NativeIssueCommentPathParams {
    #[serde(rename = "orgId")]
    pub org_id: Uuid,
    pub number: String,
    #[serde(rename = "commentId")]
    pub comment_id: String,
}

/// `PATCH /api/v1/orgs/:orgId/issues/g5/:number/comments/:commentId`
///
/// Replace the text of the caller's own comment, only if it is unchanged
/// since the editor opened.
async fn edit_native_issue_comment(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(params): Path<NativeIssueCommentPathParams>,
    Json(request): Json<EditNativeCommentRequest>,
) -> Result<Json<ResponseEnvelope<CommentResultDTO>>, AppError> {
    let author = auth.require_user_id().map_err(AppError::from_error)?;
    let number = issue_number(&params.number)?;
    let comment_id = comment_number(&params.comment_id)?;
    let (mut db, access, site) = native_page_db(&state, &auth, params.org_id).await?;
    let reader = NativeReader {
        domain: &access.domain,
        site: &site,
        system_pool: &state.system_pool,
    };
    let issue_id = native_issues::issue_id_by_number(&mut db, &reader, number).await?;
    let comment = native_issues::edit_comment(
        &mut db,
        &reader,
        issue_id,
        comment_id,
        author,
        &request.body,
        &request.expected_updated_at,
    )
    .await?;
    db.commit().await.map_err(AppError::from)?;
    let display_key = format!("{}-{number}", access.domain.display_key);
    Ok(Json(ResponseEnvelope::ok(CommentResultDTO::Native(
        NativeCommentDTO::new(&comment, &display_key, "edit"),
    ))))
}

/// `POST /api/v1/orgs/:orgId/issues/g5/:number/state`
///
/// Close (as completed) or reopen a G5 issue as the caller.
async fn change_native_issue_state(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(params): Path<NativeIssuePathParams>,
    Json(request): Json<ChangeIssueStateRequest>,
) -> Result<Json<ResponseEnvelope<IssueStateChangeDTO>>, AppError> {
    let actor = auth.require_user_id().map_err(AppError::from_error)?;
    let number = issue_number(&params.number)?;
    let change = parse_state_change(&request.state)?;
    let (mut db, access, site) = native_page_db(&state, &auth, params.org_id).await?;
    let reader = NativeReader {
        domain: &access.domain,
        site: &site,
        system_pool: &state.system_pool,
    };
    let issue_id = native_issues::issue_id_by_number(&mut db, &reader, number).await?;
    let issue = native_issues::set_state(
        &mut db,
        &reader,
        params.org_id,
        issue_id,
        actor,
        change == IssueStateChange::Close,
    )
    .await?;
    db.commit().await.map_err(AppError::from)?;
    Ok(Json(ResponseEnvelope::ok(IssueStateChangeDTO {
        requested: change.state(),
        outcome: "applied",
        issue: Some(issue),
        github_login: None,
        message: None,
    })))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EditNativeIssueRequest {
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub body: Option<String>,
    /// Member emails, user ids or `@me`; the complete new set.
    #[serde(default)]
    pub assignees: Option<Vec<String>>,
    /// The issue's `updatedAt` when the editor opened.
    pub expected_updated_at: String,
}

/// `PATCH /api/v1/orgs/:orgId/issues/g5/:number`
///
/// Edit a G5 issue's title, description or assignees, only if it is
/// unchanged since the editor opened (`issue_changed` otherwise).
async fn edit_native_issue(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(params): Path<NativeIssuePathParams>,
    Json(request): Json<EditNativeIssueRequest>,
) -> Result<Json<ResponseEnvelope<ExternalIssueSummaryDTO>>, AppError> {
    let actor = auth.require_user_id().map_err(AppError::from_error)?;
    let number = issue_number(&params.number)?;
    // Authorize the path org first: resolving assignees reads its members.
    let (mut db, access, site) = native_page_db(&state, &auth, params.org_id).await?;
    let assignees = match &request.assignees {
        Some(requested) => Some(
            native_issues::resolve_assignees(&state.system_pool, params.org_id, actor, requested)
                .await?,
        ),
        None => None,
    };
    let reader = NativeReader {
        domain: &access.domain,
        site: &site,
        system_pool: &state.system_pool,
    };
    let issue = native_issues::edit_issue(
        &mut db,
        &reader,
        params.org_id,
        number,
        actor,
        &NativeIssueEdit {
            title: request.title.as_deref(),
            body: request.body.as_deref(),
            assignees: assignees.as_deref(),
        },
        &request.expected_updated_at,
    )
    .await?;
    db.commit().await.map_err(AppError::from)?;
    Ok(Json(ResponseEnvelope::ok(issue)))
}

// ─── Attachments (g5.attachments, g5.issue.native.attachments) ───────────────

/// An upload's JSON body: a 10 MiB image in base64 with room for its name
/// and key.
const MAX_ATTACHMENT_UPLOAD_BODY: usize = 15 * 1024 * 1024;

/// A file name safe inside a quoted `Content-Disposition` value.
fn disposition_name(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_ascii_graphic() && c != '"' && c != '\\' || c == ' ' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

/// The headers every attachment read sends: the recognized type, never
/// sniffed, a download for anything but an image or video, and no caching
/// outside the reader's browser.
fn attachment_headers(
    kind: g5_contracts::attachments::AttachmentKind,
    content_type: &str,
    name: Option<&str>,
    length: Option<u64>,
) -> axum::http::HeaderMap {
    use axum::http::{header, HeaderValue};
    use g5_contracts::attachments::AttachmentKind;
    let mut headers = axum::http::HeaderMap::new();
    let content_type = HeaderValue::from_str(content_type)
        .unwrap_or_else(|_| HeaderValue::from_static("application/octet-stream"));
    headers.insert(header::CONTENT_TYPE, content_type);
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    headers.insert(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_static("default-src 'none'; sandbox"),
    );
    headers.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, no-store"),
    );
    let disposition = match kind {
        AttachmentKind::Image | AttachmentKind::Video => "inline",
        AttachmentKind::File => "attachment",
    };
    let disposition = match name {
        Some(name) => format!("{disposition}; filename=\"{}\"", disposition_name(name)),
        None => disposition.to_string(),
    };
    if let Ok(value) = HeaderValue::from_str(&disposition) {
        headers.insert(header::CONTENT_DISPOSITION, value);
    }
    if let Some(length) = length {
        headers.insert(header::CONTENT_LENGTH, HeaderValue::from(length));
    }
    if let Ok(value) = HeaderValue::from_str(kind.as_str()) {
        headers.insert("x-g5-attachment-kind", value);
    }
    headers
}

/// Stream an attachment read through its adapter. A failure after the
/// first bytes ends the body early, which the reader sees as an error.
pub(crate) fn attachment_stream_response(
    stream: crate::services::external_issues::IssueAttachmentStream,
) -> axum::response::Response {
    use axum::response::IntoResponse;
    let headers = attachment_headers(
        stream.kind,
        &stream.content_type,
        stream.name.as_deref(),
        stream.length,
    );
    let body = futures_util::stream::unfold(Some(stream), |state| async move {
        let mut stream = state?;
        match stream.chunk().await {
            Ok(Some(chunk)) => Some((Ok::<_, std::io::Error>(chunk), Some(stream))),
            Ok(None) => None,
            Err(error) => {
                tracing::warn!(%error, "an attachment read ended early");
                Some((Err(std::io::Error::other(error.message)), None))
            }
        }
    });
    (headers, axum::body::Body::from_stream(body)).into_response()
}

#[derive(Debug, Deserialize)]
pub struct NativeAttachmentPathParams {
    #[serde(rename = "orgId")]
    pub org_id: Uuid,
    pub number: String,
    #[serde(rename = "attachmentId")]
    pub attachment_id: String,
}

fn parse_attachment_id(raw: &str) -> Result<i64, AppError> {
    raw.parse::<i64>()
        .ok()
        .filter(|id| *id > 0 && !raw.starts_with('0'))
        .ok_or_else(|| {
            AppError::from(G5Error::NotFound {
                resource: "the attachment".to_string(),
            })
        })
}

/// The org's native domain and site in a transaction with `scopes`.
async fn native_db_with(
    state: &Arc<AppState>,
    auth: &AuthContext,
    org_id: Uuid,
    scopes: &[&str],
) -> Result<
    (
        g5_authz::ScopedDb,
        IssueDomainAccess,
        native_issues::NativeSite,
    ),
    AppError,
> {
    let mut db = scoped_db_for_org(state, auth, org_id, scopes)
        .await
        .map_err(mask_foreign_org_access)?;
    ensure_external_issues_enabled(&mut db, org_id).await?;
    let domains = list_domain_access(&mut db, org_id).await?;
    let access = native_domain(&domains).map_err(issue_error)?.clone();
    let site = native_issues::native_site(&state.system_pool, &state.external_url, org_id).await?;
    Ok((db, access, site))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NativeAttachmentUpload {
    pub file_name: String,
    pub content_base64: String,
    /// Stable across retries of the same upload.
    pub idempotency_key: String,
}

/// `POST /api/v1/orgs/:orgId/issues/g5/:number/attachments`
///
/// Upload an image to a G5 issue as the caller. A retry with the same key
/// returns the first upload. The issue's text is not changed: the composer
/// inserts the returned Markdown.
async fn upload_native_issue_attachment(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(params): Path<NativeIssuePathParams>,
    Json(request): Json<NativeAttachmentUpload>,
) -> Result<
    (
        StatusCode,
        Json<ResponseEnvelope<native_issues::NativeAttachmentDTO>>,
    ),
    AppError,
> {
    use base64::Engine as _;
    let uploader = auth.require_user_id().map_err(AppError::from_error)?;
    let number = issue_number(&params.number)?;
    validate_key(&request.idempotency_key, 200)?;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(request.content_base64.trim())
        .map_err(|_| {
            AppError::from(G5Error::ValidationError {
                details: json!({"field": "contentBase64", "error": "not valid base64"}),
            })
        })?;
    let (mut db, access, site) = native_page_db(&state, &auth, params.org_id).await?;
    let reader = NativeReader {
        domain: &access.domain,
        site: &site,
        system_pool: &state.system_pool,
    };
    let issue_id = native_issues::issue_id_by_number(&mut db, &reader, number).await?;
    let (attachment, fresh) = native_issues::add_attachment(
        &mut db,
        &reader,
        &state.issue_attachments,
        &native_issues::NewNativeAttachment {
            org_id: params.org_id,
            issue_id,
            uploader,
            origin: "person",
            idempotency_key: &request.idempotency_key,
            file_name: &request.file_name,
            bytes: &bytes,
        },
    )
    .await?;
    db.commit().await.map_err(AppError::from)?;
    let status = if fresh {
        StatusCode::CREATED
    } else {
        StatusCode::OK
    };
    Ok((status, Json(ResponseEnvelope::ok(attachment.to_dto()))))
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NativeAttachmentListItem {
    #[serde(flatten)]
    pub attachment: native_issues::NativeAttachmentDTO,
    /// `description`, `comment` or `issue` (shown by no text).
    pub place: &'static str,
    pub comment_id: Option<String>,
}

/// `GET /api/v1/orgs/:orgId/issues/g5/:number/attachments`
///
/// The issue's attachments, oldest first, with where each first appears.
async fn list_native_issue_attachments(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(params): Path<NativeIssuePathParams>,
) -> Result<Json<ResponseEnvelope<Vec<NativeAttachmentListItem>>>, AppError> {
    let number = issue_number(&params.number)?;
    let (mut db, access, site) =
        native_db_with(&state, &auth, params.org_id, MEMBER_READ_SCOPE).await?;
    let reader = NativeReader {
        domain: &access.domain,
        site: &site,
        system_pool: &state.system_pool,
    };
    let issue_id = native_issues::issue_id_by_number(&mut db, &reader, number).await?;
    let attachments = native_issues::list_attachments(&mut db, &reader, issue_id).await?;
    let places = native_issues::attachment_places(&mut db, &reader, issue_id, &attachments).await?;
    db.commit().await.map_err(AppError::from)?;
    let items = attachments
        .iter()
        .zip(places)
        .map(|(attachment, place)| {
            let (place, comment_id) = match place {
                native_issues::NativeAttachmentPlace::Description => ("description", None),
                native_issues::NativeAttachmentPlace::Comment { id, .. } => {
                    ("comment", Some(id.to_string()))
                }
                native_issues::NativeAttachmentPlace::Issue => ("issue", None),
            };
            NativeAttachmentListItem {
                attachment: attachment.to_dto(),
                place,
                comment_id,
            }
        })
        .collect();
    Ok(Json(ResponseEnvelope::ok(items)))
}

/// `GET /api/v1/orgs/:orgId/issues/g5/:number/attachments/:attachmentId`
///
/// The image itself, for a reader of the issue: its recognized type,
/// `nosniff`, inline, never cached outside the reader.
async fn get_native_issue_attachment(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(params): Path<NativeAttachmentPathParams>,
) -> Result<axum::response::Response, AppError> {
    use axum::response::IntoResponse;
    let number = issue_number(&params.number)?;
    let attachment_id = parse_attachment_id(&params.attachment_id)?;
    let (mut db, access, site) =
        native_db_with(&state, &auth, params.org_id, MEMBER_READ_SCOPE).await?;
    let reader = NativeReader {
        domain: &access.domain,
        site: &site,
        system_pool: &state.system_pool,
    };
    let issue_id = native_issues::issue_id_by_number(&mut db, &reader, number).await?;
    let (attachment, content) = native_issues::attachment_content(
        &mut db,
        &reader,
        &state.issue_attachments,
        issue_id,
        attachment_id,
    )
    .await?;
    db.commit().await.map_err(AppError::from)?;
    let headers = attachment_headers(
        g5_contracts::attachments::AttachmentKind::Image,
        &attachment.content_type,
        Some(&attachment.file_name),
        Some(content.length),
    );
    Ok((headers, axum::body::Body::from_stream(content.body)).into_response())
}

/// `DELETE /api/v1/orgs/:orgId/issues/g5/:number/attachments/:attachmentId`
///
/// Remove an attachment and its bytes: the uploader's own, or any for an
/// organization administrator. Text that showed it then shows it as
/// removed.
async fn remove_native_issue_attachment(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(params): Path<NativeAttachmentPathParams>,
) -> Result<StatusCode, AppError> {
    let actor = auth.require_user_id().map_err(AppError::from_error)?;
    let number = issue_number(&params.number)?;
    let attachment_id = parse_attachment_id(&params.attachment_id)?;
    // Without a store the bytes could not be deleted: refuse before the
    // record goes.
    state.issue_attachments.ensure_available()?;
    let (mut db, access, site) = native_page_db(&state, &auth, params.org_id).await?;
    let reader = NativeReader {
        domain: &access.domain,
        site: &site,
        system_pool: &state.system_pool,
    };
    let issue_id = native_issues::issue_id_by_number(&mut db, &reader, number).await?;
    // Someone else's attachment needs administrator authority, checked only
    // then.
    let removed = match native_issues::remove_attachment(
        &mut db,
        issue_id,
        attachment_id,
        actor,
        false,
    )
    .await
    {
        Err(G5Error::Forbidden { missing_scopes }) => {
            let admin = crate::services::github_app_binding::is_org_admin(
                &state.system_pool,
                params.org_id,
                actor,
            )
            .await?;
            if !admin {
                return Err(AppError::from(G5Error::Forbidden { missing_scopes }));
            }
            native_issues::remove_attachment(&mut db, issue_id, attachment_id, actor, true).await?
        }
        other => other?,
    };
    db.commit().await.map_err(AppError::from)?;
    // The bytes go only once the record's removal has committed.
    state.issue_attachments.discard(removed).await;
    Ok(StatusCode::NO_CONTENT)
}
