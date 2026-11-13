//! Org management route handlers under multiple path prefixes.
//!
//! This module implements the HTTP route handlers for the Org Management API
//! (API_AND_STORAGE.md §3.12). Endpoints span 2 path prefixes:
//!
//! - `/api/v1/orgs` — Org CRUD, member management, invitation management, repo listing
//! - `/api/v1/invitations` — User-facing invitation accept/decline
//!
//! All handlers follow the thin handler pattern: extract request parameters,
//! delegate to `OrgService`, and format the response using `ResponseEnvelope`.
//! No business logic resides in the handlers.

use g5_api_core::services::git::repo_store::{RepoKey, TenantRepoStorage};
use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{delete, get, patch, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::db::scoped::{
    ensure_org_invitation_id, ensure_org_user_id, scoped_db_for_actor_unscoped, scoped_db_for_org,
    scoped_db_for_system_org, scoped_system_db_for_org,
};
use crate::errors::AppError;
use crate::middleware::{extract_session_user_id, AuthContext};
use crate::models::{Org, OrgInvitation, Repo};
use crate::routes::audited::AuditedRouter;
use crate::routes::ResponseEnvelope;
use crate::services::branch_commit::{
    map_branch_commit_app_error, prepare_initial_branch_commit_in_tx, InitialWorkspaceHeadInput,
};
use crate::services::org_admin_settings::{self, OrgAdminSettings, UpdateOrgAdminSettings};
use crate::services::org_service;
use crate::services::AppState;
use g5_auth::Scope;
use g5_authz::ScopedDb;
use g5_contracts::{error::G5Error, types::AuthorityTarget};
use g5_scm::workspace as ops;
use g5_scm::{GitAuthor, GitRepo};

// ─── Path Parameter Types ────────────────────────────────────────────────────

/// Path parameters for org-scoped endpoints (`/api/v1/orgs/:orgId`).
#[derive(Debug, Deserialize)]
pub struct OrgPathParams {
    #[serde(rename = "orgId")]
    pub org_id: Uuid,
}

/// Path parameters for org member endpoints (`/api/v1/orgs/:orgId/members/:userId`).
#[derive(Debug, Deserialize)]
pub struct OrgMemberPathParams {
    #[serde(rename = "orgId")]
    pub org_id: Uuid,
    #[serde(rename = "userId")]
    pub user_id: Uuid,
}

/// Path parameters for org invitation endpoints (`/api/v1/orgs/:orgId/invitations/:invitationId`).
#[derive(Debug, Deserialize)]
pub struct OrgInvitationPathParams {
    #[serde(rename = "orgId")]
    pub org_id: Uuid,
    #[serde(rename = "invitationId")]
    pub invitation_id: Uuid,
}

/// Path parameters for user-facing invitation endpoints (`/api/v1/invitations/:invitationId`).
#[derive(Debug, Deserialize)]
pub struct InvitationPathParams {
    #[serde(rename = "invitationId")]
    pub invitation_id: Uuid,
}

// ─── Request DTOs ────────────────────────────────────────────────────────────

/// Request body for `PATCH /api/v1/orgs/:orgId`.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateOrgRequestDTO {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub slug: Option<String>,
}

/// Request body for `POST /api/v1/orgs/:orgId/members`.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InviteMemberRequestDTO {
    pub email: String,
    pub role: String,
}

/// Request body for `PATCH /api/v1/orgs/:orgId/members/:userId`.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateMemberRoleRequestDTO {
    pub role: String,
}

// ─── Response DTOs ───────────────────────────────────────────────────────────

/// Org list item including the user's role in the org.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OrgListItemDTO {
    pub org_id: String,
    pub org_name: String,
    pub org_slug: String,
    pub role: String,
    pub created_at: String,
}

/// Full org detail response.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OrgDTO {
    pub id: String,
    pub name: String,
    pub slug: String,
    pub created_at: String,
}

/// Response from org creation cascade.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateOrgResponseDTO {
    pub org: OrgSummaryDTO,
    pub base_workspace: WorkspaceSummaryDTO,
    pub docs_repo: RepoSummaryDTO,
    pub code_repo: RepoSummaryDTO,
}

/// Org summary within create response.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OrgSummaryDTO {
    pub id: String,
    pub name: String,
    pub slug: String,
    pub created_at: String,
}

/// Workspace summary within create response.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceSummaryDTO {
    pub id: String,
    pub name: String,
}

/// Repo summary within create response.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RepoSummaryDTO {
    pub id: String,
    pub name: String,
    pub git_url: String,
}

/// Member with profile information.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OrgMemberResponseDTO {
    pub user_id: String,
    pub email: String,
    pub display_name: String,
    pub avatar_url: Option<String>,
    pub role: String,
    pub joined_at: String,
    pub deactivated_at: Option<String>,
    pub activity_revision: i64,
    pub shutdown_pending: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MemberActivityRequestDTO {
    pub activity_revision: i64,
}

/// Response from invite member — either immediate membership or pending invitation.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InviteMemberResponseDTO {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub membership: Option<OrgMemberResponseDTO>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub invitation: Option<OrgInvitationDTO>,
}

/// Invitation detail.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OrgInvitationDTO {
    pub id: String,
    pub org_id: String,
    pub email: String,
    pub role: String,
    pub invited_by: UserRefDTO,
    pub status: String,
    pub expires_at: String,
    pub created_at: String,
    /// Latest invitation email status, listed for administrators
    /// (`g5.organization.invitation-email-status`); absent when no email
    /// was requested.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub email_delivery: Option<InvitationEmailDeliveryDTO>,
}

/// The latest email requested for an invitation.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InvitationEmailDeliveryDTO {
    /// `queued`, `sending`, `sent`, `failed` or `cancelled`.
    pub status: crate::services::email::DeliveryState,
    /// An attempt may have reached the provider without a definite answer.
    pub outcome_uncertain: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    pub updated_at: String,
}

/// Minimal user reference (for invitation inviter).
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UserRefDTO {
    pub user_id: String,
    pub display_name: String,
}

/// Repo listing item.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RepoDTO {
    pub id: String,
    pub name: String,
    pub slug: String,
    pub git_url: String,
    pub default_branch: String,
    pub provider: Option<String>,
    pub provider_full_name: Option<String>,
    pub created_at: String,
}

async fn prepare_docs_repo_bootstrap_commit(
    repo_storage: &TenantRepoStorage,
    key: &RepoKey,
) -> Result<String, G5Error> {
    let repo_id = key.repo_id();
    let repo_path = repo_storage.path(key)?;

    tokio::task::spawn_blocking(move || -> Result<String, G5Error> {
        let repo = match GitRepo::open(&repo_path) {
            Ok(repo) => repo,
            Err(_) => {
                if let Some(parent) = repo_path.parent() {
                    std::fs::create_dir_all(parent).map_err(|e| G5Error::GitError {
                        message: format!(
                            "Failed to create docs repo parent '{}': {e}",
                            parent.display()
                        ),
                    })?;
                }
                tracing::warn!(
                    repo_id = %repo_id,
                    repo_path = %repo_path.display(),
                    "Docs repo mirror missing or invalid; initializing in place"
                );
                ops::clone::init(&repo_path, false)?
            }
        };

        let author = GitAuthor {
            name: "G5 System".to_string(),
            email: "g5@system.local".to_string(),
        };
        g5_scm::prepare_initial_empty_commit(&repo, "Initial docs repo bootstrap", &author)
    })
    .await
    .map_err(|e| G5Error::Internal {
        message: format!("Docs repo bootstrap task failed: {e}"),
    })?
}

/// Post-commit Base **docs** repo bootstrap shared by normal org creation and
/// dev/E2E test-token provisioning. The repository constructor prepares an
/// unattached commit that lands on the base branch ref by compare-and-swap.
pub(crate) async fn bootstrap_base_docs_repo(
    state: &Arc<AppState>,
    org_id: Uuid,
    base_workspace_id: Uuid,
    docs_repo_id: Uuid,
) -> Result<String, AppError> {
    let scope = scoped_db_for_system_org(state, org_id, &["workspaces:write"])
        .await?
        .reopener();
    let key = TenantRepoStorage::key(&scope, org_id, docs_repo_id)?;
    let docs_head_commit = prepare_docs_repo_bootstrap_commit(&state.repo_storage, &key).await?;

    let mut db = scope.open().await.map_err(AppError::from)?;
    let authority_advance = prepare_initial_branch_commit_in_tx(
        &mut db,
        &state.repo_storage,
        AuthorityTarget::Workspace {
            workspace_id: base_workspace_id,
        },
        vec![InitialWorkspaceHeadInput {
            repo_id: docs_repo_id,
            proposed_new_head: docs_head_commit.clone(),
        }],
        "workspace_initializer",
    )
    .await
    .map_err(map_branch_commit_app_error)?;
    authority_advance
        .land(db, &state.event_bus, &state.repo_storage)
        .await
        .map_err(AppError::from)?;

    Ok(docs_head_commit)
}

/// Post-commit Base **code** repo bootstrap shared by normal org creation and
/// dev/E2E test-token provisioning. The repository constructor prepares an
/// unattached commit that lands on the base branch ref by compare-and-swap.
pub(crate) async fn bootstrap_base_code_repo(
    state: &Arc<AppState>,
    org_id: Uuid,
    base_workspace_id: Uuid,
    code_repo_id: Uuid,
    code_repo_name: &str,
) -> Result<String, AppError> {
    let scope = scoped_db_for_system_org(state, org_id, &["workspaces:write"])
        .await?
        .reopener();
    let key = TenantRepoStorage::key(&scope, org_id, code_repo_id)?;
    let code_head_commit = state
        .workspace_service
        .initialize_local_workspace_repo(&key, code_repo_name, "main")
        .await?;

    let mut db = scope.open().await.map_err(AppError::from)?;
    let authority_advance = prepare_initial_branch_commit_in_tx(
        &mut db,
        &state.repo_storage,
        AuthorityTarget::Workspace {
            workspace_id: base_workspace_id,
        },
        vec![InitialWorkspaceHeadInput {
            repo_id: code_repo_id,
            proposed_new_head: code_head_commit.clone(),
        }],
        "workspace_initializer",
    )
    .await
    .map_err(map_branch_commit_app_error)?;
    authority_advance
        .land(db, &state.event_bus, &state.repo_storage)
        .await
        .map_err(AppError::from)?;

    Ok(code_head_commit)
}

// ─── From Implementations ────────────────────────────────────────────────────

impl From<org_service::OrgWithRole> for OrgListItemDTO {
    fn from(o: org_service::OrgWithRole) -> Self {
        OrgListItemDTO {
            org_id: o.id.to_string(),
            org_name: o.name,
            org_slug: o.slug,
            role: o.role,
            created_at: o.created_at.to_rfc3339(),
        }
    }
}

impl From<Org> for OrgDTO {
    fn from(o: Org) -> Self {
        OrgDTO {
            id: o.id.to_string(),
            name: o.name,
            slug: o.slug,
            created_at: o.created_at.to_rfc3339(),
        }
    }
}

impl From<org_service::CreateOrgResponse> for CreateOrgResponseDTO {
    fn from(r: org_service::CreateOrgResponse) -> Self {
        CreateOrgResponseDTO {
            org: OrgSummaryDTO {
                id: r.org.id.to_string(),
                name: r.org.name,
                slug: r.org.slug,
                created_at: r.org.created_at.to_rfc3339(),
            },
            base_workspace: WorkspaceSummaryDTO {
                id: r.base_workspace.id.to_string(),
                name: r.base_workspace.name,
            },
            docs_repo: RepoSummaryDTO {
                id: r.docs_repo.id.to_string(),
                name: r.docs_repo.name,
                git_url: r.docs_repo.git_url,
            },
            code_repo: RepoSummaryDTO {
                id: r.code_repo.id.to_string(),
                name: r.code_repo.name,
                git_url: r.code_repo.git_url,
            },
        }
    }
}

impl From<org_service::OrgMemberDTO> for OrgMemberResponseDTO {
    fn from(m: org_service::OrgMemberDTO) -> Self {
        OrgMemberResponseDTO {
            user_id: m.user_id.to_string(),
            email: m.email,
            display_name: m.display_name,
            avatar_url: m.avatar_url,
            role: m.role,
            joined_at: m.joined_at.to_rfc3339(),
            deactivated_at: m.deactivated_at.map(|at| at.to_rfc3339()),
            activity_revision: m.activity_revision,
            shutdown_pending: m.shutdown_pending,
        }
    }
}

impl From<OrgInvitation> for OrgInvitationDTO {
    fn from(inv: OrgInvitation) -> Self {
        // Note: invited_by UserRefDTO requires the inviter's display_name.
        // The raw OrgInvitation model only stores `invited_by: Uuid`.
        // The service layer should ideally return an enriched DTO.
        // As a fallback, we populate with just the UUID and an empty display name.
        OrgInvitationDTO {
            id: inv.id.to_string(),
            org_id: inv.org_id.to_string(),
            email: inv.email,
            role: inv.role,
            invited_by: UserRefDTO {
                user_id: inv.invited_by.to_string(),
                display_name: String::new(),
            },
            status: inv.status,
            expires_at: inv.expires_at.to_rfc3339(),
            created_at: inv.created_at.to_rfc3339(),
            email_delivery: None,
        }
    }
}

impl From<Repo> for RepoDTO {
    fn from(r: Repo) -> Self {
        RepoDTO {
            id: r.id.to_string(),
            name: r.name,
            slug: r.slug,
            git_url: r.git_url,
            default_branch: r.default_branch,
            provider: r.provider,
            provider_full_name: r.provider_full_name,
            created_at: r.created_at.to_rfc3339(),
        }
    }
}

// ─── Routers ─────────────────────────────────────────────────────────────────

/// Org-scoped routes nested under `/api/v1/orgs`.
///
/// Endpoints:
/// - `GET /` — list orgs for current user (organizations are created only by
///   platform operators, `g5.organization.creation`)
/// - `GET /:orgId` — get org detail
/// - `PATCH /:orgId` — update org
/// - `GET /:orgId/members` — list members
/// - `POST /:orgId/members` — invite member
/// - `PATCH /:orgId/members/:userId` — update member role
/// - `DELETE /:orgId/members/:userId` — remove member
/// - `GET /:orgId/invitations` — list invitations
/// - `POST /:orgId/invitations/:invitationId/resend` — resend invitation
/// - `DELETE /:orgId/invitations/:invitationId` — revoke invitation
/// - `GET /:orgId/repos` — list repos
/// - `GET/PUT /:orgId/admin-settings` — Admin page organization settings
pub fn org_router() -> Router<Arc<AppState>> {
    AuditedRouter::new()
        // List — account-wide person session (no org scope)
        .exempt_route(
            "/",
            get(list_orgs),
            "Account organizations — person session required in handler",
        )
        // Org detail — GET is authenticated
        .exempt_route(
            "/:orgId",
            get(get_org),
            "Authenticated only — any org member can access",
        )
        .audited_route("/:orgId", patch(update_org), &[Scope::OrgAdmin])
        // Members — GET is authenticated, POST (invite) requires org:admin
        .exempt_route(
            "/:orgId/members",
            get(list_members),
            "Authenticated only — any org member can access",
        )
        .audited_route("/:orgId/members", post(invite_member), &[Scope::OrgAdmin])
        .audited_route(
            "/:orgId/members/:userId",
            patch(update_member_role).delete(remove_member),
            &[Scope::OrgAdmin],
        )
        .audited_route(
            "/:orgId/invitations",
            get(list_invitations),
            &[Scope::OrgAdmin],
        )
        // Stop workspaces by user (`g5.lifecycle.stop.by-user`): the member
        // themself or an org admin; the service re-checks which one applies.
        .audited_route(
            "/:orgId/members/:userId/shutdown",
            get(member_shutdown),
            &[Scope::WorkspacesRead],
        )
        .audited_route(
            "/:orgId/members/:userId/stop-workspaces",
            post(stop_member_workspaces),
            &[Scope::WorkspacesWrite],
        )
        .audited_route(
            "/:orgId/members/:userId/deactivate",
            post(deactivate_member),
            &[Scope::OrgAdmin],
        )
        .audited_route(
            "/:orgId/members/:userId/reactivate",
            post(reactivate_member),
            &[Scope::OrgAdmin],
        )
        .audited_route(
            "/:orgId/invitations/:invitationId/resend",
            post(resend_invitation),
            &[Scope::OrgAdmin],
        )
        .audited_route(
            "/:orgId/invitations/:invitationId",
            delete(revoke_invitation),
            &[Scope::OrgAdmin],
        )
        .audited_route("/:orgId/repos", get(list_repos), &[Scope::WorkspacesRead])
        // Admin page settings: reads and writes both require current target-org
        // administrator authority (g5.security.org-admin-settings).
        .audited_route(
            "/:orgId/admin-settings",
            get(get_admin_settings).put(update_admin_settings),
            &[Scope::OrgAdmin],
        )
        .into_router()
}

/// Invitation routes nested under `/api/v1/invitations`.
///
/// These are separate because accept/decline are user-facing, not org-scoped.
/// The user accepting an invitation may not yet be an org member.
///
/// Endpoints:
/// - `POST /:invitationId/accept` — accept invitation
/// - `POST /:invitationId/decline` — decline invitation
pub fn invitation_router() -> Router<Arc<AppState>> {
    AuditedRouter::new()
        .exempt_route(
            "/:invitationId/accept",
            post(accept_invitation),
            "Person session required to accept/decline account invitations",
        )
        .exempt_route(
            "/:invitationId/decline",
            post(decline_invitation),
            "Person session required to accept/decline account invitations",
        )
        .into_router()
}

// ─── Handlers ────────────────────────────────────────────────────────────────

/// `GET /api/v1/orgs`
///
/// Lists all organizations the current user belongs to, including the
/// user's role in each org. Returns an empty array if the user belongs
/// to no orgs (not an error).
async fn list_orgs(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
) -> Result<Json<ResponseEnvelope<Vec<OrgListItemDTO>>>, AppError> {
    tracing::debug!("Org endpoint called: list_orgs");
    let user_id = extract_session_user_id(&auth)?;
    let orgs = state
        .org_service
        .list_orgs(&state.system_pool, user_id)
        .await?;
    let dtos: Vec<OrgListItemDTO> = orgs.into_iter().map(OrgListItemDTO::from).collect();

    Ok(Json(ResponseEnvelope::ok(dtos)))
}

/// Run the organization creation cascade with `owner_id` as the protected
/// owner on `db` (which the caller opened and may already have used inside
/// the same transaction), commit it, then finish the post-commit repository
/// bootstrap and the owner's personal-workspace hook. Shared by operator
/// creation (`g5.organization.platform-operator.create-org`) and the dev/E2E
/// operator fixture.
pub(crate) async fn create_org_with_owner(
    state: &Arc<AppState>,
    mut db: ScopedDb,
    owner_id: Uuid,
    req: org_service::CreateOrgRequest,
) -> Result<CreateOrgResponseDTO, AppError> {
    let result = state
        .org_service
        .create_org(state.authz.as_ref(), &mut db, owner_id, req)
        .await?;
    let org_id = result.org.id;
    let docs_repo_id = result.docs_repo.id;
    let code_repo_id = result.code_repo.id;
    let code_repo_name = result.code_repo.name.clone();
    let base_workspace_id = result.base_workspace.id;

    let dto: CreateOrgResponseDTO = result.into();

    // §38.215 — Commit database transaction BEFORE git side effects.
    // If git bootstrap fails after commit, the org exists with no materialized
    // docs repo — a recoverable state. The reverse (git succeeds, DB rolls
    // back) would leave orphaned repos on disk.
    db.commit().await.map_err(AppError::from)?;

    // Prepare one unattached parentless commit. The canonical finalizer owns
    // both the stable repo row and parent cut; notifier owns ref projection.
    bootstrap_base_docs_repo(state, org_id, base_workspace_id, docs_repo_id).await?;

    // Bootstrap the Base code repo the same way (initial README commit + `main`
    // ref), then materialize its base branch head. Shared with the dev/E2E
    // test-token provisioning path so every Base is born docs+code. Filesystem
    // side effects stay post-commit so a bootstrap failure leaves a recoverable
    // org.
    bootstrap_base_code_repo(
        state,
        org_id,
        base_workspace_id,
        code_repo_id,
        &code_repo_name,
    )
    .await?;

    // The owner is the organization's first eligible owner (g5.personal.lifecycle).
    crate::routes::personal::after_membership_gain(state, org_id, owner_id).await;

    Ok(dto)
}

/// `GET /api/v1/orgs/:orgId`
///
/// Returns details for a single organization.
async fn get_org(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(params): Path<OrgPathParams>,
) -> Result<Json<ResponseEnvelope<OrgDTO>>, AppError> {
    // Preserve route contract: unknown org IDs return 404 (not 403).
    // We first probe existence with an actor-scoped unscoped DB, then enforce
    // org authorization through the normal scoped path.
    let org = state
        .org_service
        .get_org(&state.system_pool, params.org_id)
        .await?;

    let db = scoped_db_for_org(&state, &auth, params.org_id, &["workspaces:read"]).await?;
    tracing::debug!(org_id = %params.org_id, "Org endpoint called: get_org");
    let dto: OrgDTO = org.into();
    db.commit().await.map_err(AppError::from)?;

    Ok(Json(ResponseEnvelope::ok(dto)))
}

/// `PATCH /api/v1/orgs/:orgId`
///
/// Updates an org's name and/or slug. Requires `org:admin` scope.
async fn update_org(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(params): Path<OrgPathParams>,
    Json(body): Json<UpdateOrgRequestDTO>,
) -> Result<Json<ResponseEnvelope<OrgDTO>>, AppError> {
    let mut db = scoped_system_db_for_org(&state, &auth, params.org_id, &["org:admin"]).await?;
    tracing::debug!(org_id = %params.org_id, "Org endpoint called: update_org");
    let req = org_service::UpdateOrgRequest {
        name: body.name,
        slug: body.slug,
    };
    let org = state
        .org_service
        .update_org(&mut db, params.org_id, auth.require_user_id()?, req)
        .await?;
    let dto: OrgDTO = org.into();
    tracing::info!(org_id = %params.org_id, "Org endpoint succeeded: update_org");
    db.commit().await.map_err(AppError::from)?;

    Ok(Json(ResponseEnvelope::ok(dto)))
}

/// `GET /api/v1/orgs/:orgId/admin-settings`
///
/// Returns the organization's effective Admin page settings: the default idle
/// timeout new workspaces start with and the VM quota.
async fn get_admin_settings(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(params): Path<OrgPathParams>,
) -> Result<Json<ResponseEnvelope<OrgAdminSettings>>, AppError> {
    let (mut db, _) = crate::routes::github_app::org_admin_db(&state, &auth, params.org_id).await?;
    let settings = org_admin_settings::read(db.conn(), params.org_id).await?;
    db.commit().await.map_err(AppError::from)?;

    Ok(Json(ResponseEnvelope::ok(settings)))
}

/// `PUT /api/v1/orgs/:orgId/admin-settings`
///
/// Updates the fields present in the body. A new idle default applies only to
/// workspaces created afterwards; a new quota only to later placements.
async fn update_admin_settings(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(params): Path<OrgPathParams>,
    Json(body): Json<UpdateOrgAdminSettings>,
) -> Result<Json<ResponseEnvelope<OrgAdminSettings>>, AppError> {
    // These settings shape every owner's new workspaces and starts. Hold
    // current target-org admin authority in the mutation transaction.
    let (mut db, actor_id) =
        crate::routes::github_app::org_admin_db(&state, &auth, params.org_id).await?;
    let settings = org_admin_settings::update(db.conn(), params.org_id, actor_id, &body).await?;
    db.commit().await.map_err(AppError::from)?;
    tracing::info!(org_id = %params.org_id, "Org admin settings updated");

    Ok(Json(ResponseEnvelope::ok(settings)))
}

/// `GET /api/v1/orgs/:orgId/members`
///
/// Lists all members of an organization with their roles and profile information.
/// Members are returned ordered by role priority (Admin first) then by display name.
async fn list_members(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(params): Path<OrgPathParams>,
) -> Result<Json<ResponseEnvelope<Vec<OrgMemberResponseDTO>>>, AppError> {
    let db = scoped_db_for_org(&state, &auth, params.org_id, &["workspaces:read"]).await?;
    tracing::debug!(org_id = %params.org_id, "Org endpoint called: list_members");
    let members = state
        .org_service
        .list_members(&state.system_pool, params.org_id)
        .await?;
    let dtos: Vec<OrgMemberResponseDTO> = members
        .into_iter()
        .map(OrgMemberResponseDTO::from)
        .collect();
    db.commit().await.map_err(AppError::from)?;

    Ok(Json(ResponseEnvelope::ok(dtos)))
}

/// `POST /api/v1/orgs/:orgId/members`
///
/// Invites a user to the org by email with an explicit role. If the email
/// matches an existing G5 user, membership is created immediately. Otherwise,
/// a pending invitation is recorded.
async fn invite_member(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(params): Path<OrgPathParams>,
    Json(body): Json<InviteMemberRequestDTO>,
) -> Result<Json<ResponseEnvelope<InviteMemberResponseDTO>>, AppError> {
    let mut db = scoped_system_db_for_org(&state, &auth, params.org_id, &["org:admin"]).await?;
    tracing::debug!(org_id = %params.org_id, email = %body.email, "Org endpoint called: invite_member");
    let user_id = auth.require_user_id()?;

    let req = org_service::InviteMemberRequest {
        email: body.email,
        role: body.role,
    };
    let result = state
        .org_service
        .invite_member(&mut db, params.org_id, user_id, req)
        .await?;

    let dto = InviteMemberResponseDTO {
        membership: result.membership.map(OrgMemberResponseDTO::from),
        invitation: result.invitation.map(OrgInvitationDTO::from),
    };
    tracing::info!(org_id = %params.org_id, "Org endpoint succeeded: invite_member");
    let joined = dto
        .membership
        .as_ref()
        .filter(|membership| membership.role != "viewer")
        .and_then(|membership| membership.user_id.parse::<Uuid>().ok());
    db.commit().await.map_err(AppError::from)?;
    if let Some(member_id) = joined {
        crate::routes::personal::after_membership_gain(&state, params.org_id, member_id).await;
    }

    Ok(Json(ResponseEnvelope::ok(dto)))
}

/// `PATCH /api/v1/orgs/:orgId/members/:userId`
///
/// Changes a member's role within the org. Requires `org:admin` scope.
async fn update_member_role(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(params): Path<OrgMemberPathParams>,
    Json(body): Json<UpdateMemberRoleRequestDTO>,
) -> Result<Json<ResponseEnvelope<OrgMemberResponseDTO>>, AppError> {
    let demoted = body.role == "viewer";
    let result = change_member(
        &state,
        &auth,
        &params,
        org_service::MemberChange::Role(body.role),
    )
    .await?;
    if demoted {
        crate::routes::personal::after_retirement(&state, params.org_id, result.retired);
    } else {
        crate::routes::personal::after_membership_gain(&state, params.org_id, params.user_id).await;
    }
    Ok(Json(ResponseEnvelope::ok(
        result
            .member
            .ok_or_else(|| G5Error::Internal {
                message: "Role transition did not return membership".into(),
            })?
            .into(),
    )))
}

async fn member_shutdown(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(params): Path<OrgMemberPathParams>,
) -> Result<Json<ResponseEnvelope<Vec<org_service::MemberShutdownWorkspace>>>, AppError> {
    let mut db =
        scoped_system_db_for_org(&state, &auth, params.org_id, &["workspaces:read"]).await?;
    ensure_org_user_id(&mut db, params.org_id, params.user_id, "userId").await?;
    let progress = state
        .org_service
        .member_shutdown_progress(
            &mut db,
            params.org_id,
            auth.require_user_id()?,
            params.user_id,
        )
        .await?;
    db.rollback().await.map_err(AppError::from)?;
    Ok(Json(ResponseEnvelope::ok(progress)))
}

/// Response of a stop-by-user request: the hold and the workspaces it stops.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StopMemberWorkspacesResponseDTO {
    pub hold_id: Uuid,
    /// The request joined a hold that was already pending.
    pub joined: bool,
    pub workspaces: Vec<org_service::MemberShutdownWorkspace>,
}

/// `POST /api/v1/orgs/:orgId/members/:userId/stop-workspaces`
async fn stop_member_workspaces(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(params): Path<OrgMemberPathParams>,
) -> Result<
    (
        StatusCode,
        Json<ResponseEnvelope<StopMemberWorkspacesResponseDTO>>,
    ),
    AppError,
> {
    let actor_id = auth.require_user_id()?;
    let mut db =
        scoped_system_db_for_org(&state, &auth, params.org_id, &["workspaces:write"]).await?;
    ensure_org_user_id(&mut db, params.org_id, params.user_id, "userId").await?;
    let hold = state
        .org_service
        .stop_member_workspaces(&mut db, params.org_id, actor_id, params.user_id)
        .await?;
    let workspaces = state
        .org_service
        .member_shutdown_progress(&mut db, params.org_id, actor_id, params.user_id)
        .await?;
    db.commit().await.map_err(AppError::from)?;
    let status = if hold.joined {
        StatusCode::OK
    } else {
        StatusCode::CREATED
    };
    Ok((
        status,
        Json(ResponseEnvelope::ok(StopMemberWorkspacesResponseDTO {
            hold_id: hold.hold_id,
            joined: hold.joined,
            workspaces,
        })),
    ))
}

async fn deactivate_member(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(params): Path<OrgMemberPathParams>,
    Json(body): Json<MemberActivityRequestDTO>,
) -> Result<Json<ResponseEnvelope<OrgMemberResponseDTO>>, AppError> {
    change_member_activity(state, auth, params, body, false).await
}

async fn reactivate_member(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(params): Path<OrgMemberPathParams>,
    Json(body): Json<MemberActivityRequestDTO>,
) -> Result<Json<ResponseEnvelope<OrgMemberResponseDTO>>, AppError> {
    change_member_activity(state, auth, params, body, true).await
}

async fn change_member_activity(
    state: Arc<AppState>,
    auth: AuthContext,
    params: OrgMemberPathParams,
    body: MemberActivityRequestDTO,
    active: bool,
) -> Result<Json<ResponseEnvelope<OrgMemberResponseDTO>>, AppError> {
    let result = change_member(
        &state,
        &auth,
        &params,
        org_service::MemberChange::Activity {
            active,
            expected_revision: body.activity_revision,
        },
    )
    .await?;
    state.event_bus.ws_hub().note_membership_change();
    Ok(Json(ResponseEnvelope::ok(
        result
            .member
            .ok_or_else(|| G5Error::Internal {
                message: "Activity transition did not return membership".into(),
            })?
            .into(),
    )))
}

/// Retry the whole transaction, including capability minting and current
/// authority. Never retain an actor verdict or retirement from a failed try.
async fn change_member(
    state: &Arc<AppState>,
    auth: &AuthContext,
    params: &OrgMemberPathParams,
    change: org_service::MemberChange,
) -> Result<org_service::MemberChangeResult, AppError> {
    for attempt in 0..3 {
        let result = async {
            let mut db = crate::db::scoped::scoped_system_db_for_membership_transition(
                state,
                auth,
                params.org_id,
                params.user_id,
            )
            .await?;
            ensure_org_user_id(&mut db, params.org_id, params.user_id, "userId").await?;
            let result = state
                .org_service
                .change_member(
                    &mut db,
                    params.org_id,
                    auth.require_user_id()?,
                    params.user_id,
                    change.clone(),
                )
                .await?;
            db.commit().await.map_err(AppError::from)?;
            Ok::<_, AppError>(result)
        }
        .await;
        match result {
            Err(error)
                if attempt < 2 && matches!(error.inner, G5Error::DatabaseRetryable { .. }) =>
            {
                tokio::time::sleep(std::time::Duration::from_millis(10 * (attempt + 1))).await;
            }
            result => return result,
        }
    }
    unreachable!()
}

/// `DELETE /api/v1/orgs/:orgId/members/:userId`
///
/// Removes a member from the org. Requires `org:admin` scope.
/// Returns 204 No Content on success.
async fn remove_member(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(params): Path<OrgMemberPathParams>,
) -> Result<StatusCode, AppError> {
    let result = change_member(&state, &auth, &params, org_service::MemberChange::Remove).await?;
    crate::routes::personal::after_retirement(&state, params.org_id, result.retired);
    state.event_bus.ws_hub().evict(
        params.user_id,
        crate::ws::RevocationScope::Organization(params.org_id),
    );
    state.event_bus.ws_hub().note_membership_change();

    Ok(StatusCode::NO_CONTENT)
}

/// `GET /api/v1/orgs/:orgId/invitations`
///
/// Lists pending and recent invitations for the org. Requires `org:admin` scope.
async fn list_invitations(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(params): Path<OrgPathParams>,
) -> Result<Json<ResponseEnvelope<Vec<OrgInvitationDTO>>>, AppError> {
    let db = scoped_db_for_org(&state, &auth, params.org_id, &["org:admin"]).await?;
    tracing::debug!(org_id = %params.org_id, "Org endpoint called: list_invitations");
    let invitations = state
        .org_service
        .list_invitations(&state.system_pool, params.org_id)
        .await?;

    // Batch-load inviter display names so the DTO doesn't ship blank metadata.
    let inviter_ids: Vec<Uuid> = invitations.iter().map(|inv| inv.invited_by).collect();
    let inviter_names = state
        .org_service
        .get_user_display_names(&state.system_pool, &inviter_ids)
        .await
        .unwrap_or_default();
    let invitation_ids: Vec<Uuid> = invitations.iter().map(|inv| inv.id).collect();
    let mut email_statuses = state
        .org_service
        .invitation_email_statuses(&state.system_pool, params.org_id, &invitation_ids)
        .await
        .unwrap_or_else(|error| {
            tracing::warn!(org_id = %params.org_id, %error, "Invitation email status unavailable");
            Default::default()
        });

    let dtos: Vec<OrgInvitationDTO> = invitations
        .into_iter()
        .map(|inv| {
            let inviter_name = inviter_names
                .get(&inv.invited_by)
                .cloned()
                .unwrap_or_default();
            let email_delivery =
                email_statuses
                    .remove(&inv.id)
                    .map(|status| InvitationEmailDeliveryDTO {
                        status: status.state,
                        outcome_uncertain: status.outcome_uncertain,
                        reason: status.terminal_reason,
                        updated_at: status.updated_at.to_rfc3339(),
                    });
            let mut dto = OrgInvitationDTO::from(inv);
            dto.invited_by.display_name = inviter_name;
            dto.email_delivery = email_delivery;
            dto
        })
        .collect();
    db.commit().await.map_err(AppError::from)?;

    Ok(Json(ResponseEnvelope::ok(dtos)))
}

/// `POST /api/v1/orgs/:orgId/invitations/:invitationId/resend`
///
/// Resends a pending invitation by resetting its expiry to now + 7 days and
/// requesting a new invitation email (`g5.organization.invitation-resend`).
/// Requires `org:admin` scope.
async fn resend_invitation(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(params): Path<OrgInvitationPathParams>,
) -> Result<Json<ResponseEnvelope<OrgInvitationDTO>>, AppError> {
    let mut db = scoped_system_db_for_org(&state, &auth, params.org_id, &["org:admin"]).await?;
    ensure_org_invitation_id(&mut db, params.org_id, params.invitation_id, "invitationId").await?;
    tracing::debug!(
        org_id = %params.org_id,
        invitation_id = %params.invitation_id,
        "Org endpoint called: resend_invitation"
    );
    let invitation = state
        .org_service
        .resend_invitation(&mut db, params.invitation_id, auth.require_user_id()?)
        .await?;
    let dto: OrgInvitationDTO = invitation.into();
    tracing::info!(
        invitation_id = %params.invitation_id,
        "Org endpoint succeeded: resend_invitation"
    );
    db.commit().await.map_err(AppError::from)?;

    Ok(Json(ResponseEnvelope::ok(dto)))
}

/// `DELETE /api/v1/orgs/:orgId/invitations/:invitationId`
///
/// Revokes (deletes) a pending invitation. Requires `org:admin` scope.
/// Returns 204 No Content on success.
async fn revoke_invitation(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(params): Path<OrgInvitationPathParams>,
) -> Result<StatusCode, AppError> {
    let mut db = scoped_system_db_for_org(&state, &auth, params.org_id, &["org:admin"]).await?;
    ensure_org_invitation_id(&mut db, params.org_id, params.invitation_id, "invitationId").await?;
    tracing::debug!(
        org_id = %params.org_id,
        invitation_id = %params.invitation_id,
        "Org endpoint called: revoke_invitation"
    );
    state
        .org_service
        .revoke_invitation(&mut db, params.invitation_id, auth.require_user_id()?)
        .await?;
    tracing::info!(
        invitation_id = %params.invitation_id,
        "Org endpoint succeeded: revoke_invitation"
    );
    db.commit().await.map_err(AppError::from)?;

    Ok(StatusCode::NO_CONTENT)
}

/// `POST /api/v1/invitations/:invitationId/accept`
///
/// Accepts a pending invitation. The authenticated user's email must match
/// the invitation email.
async fn accept_invitation(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(params): Path<InvitationPathParams>,
) -> Result<Json<ResponseEnvelope<OrgMemberResponseDTO>>, AppError> {
    tracing::debug!(
        invitation_id = %params.invitation_id,
        "Org endpoint called: accept_invitation"
    );
    let user_id = extract_session_user_id(&auth)?;
    let mut db = scoped_db_for_actor_unscoped(&state, &auth).await?;
    let member = state
        .org_service
        .accept_invitation(&mut db, params.invitation_id, user_id)
        .await?;
    let dto: OrgMemberResponseDTO = member.into();
    db.commit().await.map_err(AppError::from)?;
    if dto.role != "viewer" {
        let org_id = state
            .org_service
            .invitation_org_id(&state.system_pool, params.invitation_id)
            .await
            .ok()
            .flatten();
        if let Some(org_id) = org_id {
            crate::routes::personal::after_membership_gain(&state, org_id, user_id).await;
        }
    }
    tracing::info!(
        invitation_id = %params.invitation_id,
        user_id = %dto.user_id,
        "Org endpoint succeeded: accept_invitation"
    );
    Ok(Json(ResponseEnvelope::ok(dto)))
}

/// `POST /api/v1/invitations/:invitationId/decline`
///
/// Declines a pending invitation. Returns 204 No Content on success.
async fn decline_invitation(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(params): Path<InvitationPathParams>,
) -> Result<StatusCode, AppError> {
    let user_id = extract_session_user_id(&auth)?;
    tracing::debug!(
        invitation_id = %params.invitation_id,
        "Org endpoint called: decline_invitation"
    );
    state
        .org_service
        .decline_invitation(&state.system_pool, params.invitation_id, user_id)
        .await?;
    tracing::info!(
        invitation_id = %params.invitation_id,
        user_id = %user_id,
        "Org endpoint succeeded: decline_invitation"
    );
    Ok(StatusCode::NO_CONTENT)
}

/// `GET /api/v1/orgs/:orgId/repos`
///
/// Lists all repos belonging to the org.
async fn list_repos(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(params): Path<OrgPathParams>,
) -> Result<Json<ResponseEnvelope<Vec<RepoDTO>>>, AppError> {
    let mut db = scoped_db_for_org(&state, &auth, params.org_id, &["workspaces:read"]).await?;
    tracing::debug!(org_id = %params.org_id, "Org endpoint called: list_repos");
    let repos = state.org_service.list_repos(&mut db, params.org_id).await?;
    let dtos: Vec<RepoDTO> = repos.into_iter().map(RepoDTO::from).collect();
    db.commit().await.map_err(AppError::from)?;

    Ok(Json(ResponseEnvelope::ok(dtos)))
}
