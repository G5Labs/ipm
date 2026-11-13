//! Org approval-definition route handlers (B6 governance UI PR-1).
//!
//! Mounted at `/api/v1/orgs/:orgId/approval-definitions`. This surface is the
//! org-wide definition library backed by `approval_definitions`
//! (migration 186): list latest versions, create v1, publish the next
//! version. Nothing here evaluates, attaches, or enforces approvals —
//! `approval_configs` and the finalizer gate are later PRs.
//!
//! Handlers follow the thin handler pattern of `orgs.rs`: extract, delegate
//! to `ApprovalDefinitionService`, wrap in `ResponseEnvelope`.

use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::db::scoped::scoped_db_for_org;
use crate::errors::AppError;
use crate::middleware::AuthContext;
use crate::routes::audited::AuditedRouter;
use crate::routes::orgs::OrgPathParams;
use crate::routes::ResponseEnvelope;
use crate::services::approval_definition_service::{
    ApprovalDefinitionRecord, ApprovalHumanRecord, CreateApprovalDefinitionRequest,
    PublishApprovalDefinitionVersionRequest,
};
use crate::services::AppState;
use g5_auth::Scope;

// ─── Path Parameter Types ────────────────────────────────────────────────────

/// Path parameters for definition-scoped endpoints
/// (`/api/v1/orgs/:orgId/approval-definitions/:definitionId/...`).
#[derive(Debug, Deserialize)]
pub struct OrgApprovalDefinitionPathParams {
    #[serde(rename = "orgId")]
    pub org_id: Uuid,
    #[serde(rename = "definitionId")]
    pub definition_id: Uuid,
}

// ─── Request DTOs ────────────────────────────────────────────────────────────

/// Request body for `POST /api/v1/orgs/:orgId/approval-definitions`.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreateOrgApprovalDefinitionRequestDTO {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub human_user_ids: Vec<Uuid>,
    #[serde(default)]
    pub agent_type: Option<String>,
    #[serde(default)]
    pub agent_prompt: Option<String>,
    #[serde(default)]
    pub agent_harness: Option<String>,
    #[serde(default)]
    pub agent_model: Option<String>,
    #[serde(default)]
    pub agent_effort: Option<String>,
    /// Workbench phases this version binds (`be` | `se` | `ie`,
    /// g5.workbench.gates); omitted resolves to `["ie"]`, an empty list is
    /// attached but inert.
    #[serde(default)]
    pub phases: Option<Vec<String>>,
    #[serde(default)]
    pub require_all_reviewers: bool,
    #[serde(default)]
    pub allow_self_approval: bool,
    #[serde(default = "default_human_override")]
    pub human_approvals_override_agent: bool,
}

/// Request body for
/// `POST /api/v1/orgs/:orgId/approval-definitions/:definitionId/versions`.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PublishOrgApprovalDefinitionVersionRequestDTO {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub human_user_ids: Vec<Uuid>,
    #[serde(default)]
    pub agent_type: Option<String>,
    #[serde(default)]
    pub agent_prompt: Option<String>,
    #[serde(default)]
    pub agent_harness: Option<String>,
    #[serde(default)]
    pub agent_model: Option<String>,
    #[serde(default)]
    pub agent_effort: Option<String>,
    #[serde(default)]
    pub invalidates_previous: bool,
    /// Workbench phases this version binds; omitted resolves to `["ie"]`.
    #[serde(default)]
    pub phases: Option<Vec<String>>,
    #[serde(default)]
    pub require_all_reviewers: bool,
    #[serde(default)]
    pub allow_self_approval: bool,
    #[serde(default = "default_human_override")]
    pub human_approvals_override_agent: bool,
}

fn default_human_override() -> bool {
    true
}

// ─── Response DTOs ───────────────────────────────────────────────────────────

/// One human approver, profile-enriched. `stillMember=false` marks a departed
/// member who remains in the version's approver set (historical fact — never
/// silently dropped).
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApprovalHumanDTO {
    pub user_id: String,
    pub display_name: String,
    pub email: String,
    pub avatar_url: Option<String>,
    pub still_member: bool,
    pub deactivated_at: Option<String>,
}

/// A user reference for provenance lines.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApprovalUserRefDTO {
    pub user_id: String,
    pub display_name: String,
}

/// A definition at its latest version. `firstCreatedBy`/`firstCreatedAt` are
/// v1 provenance — the UI card's "Created by" line; `createdBy`/`createdAt`
/// describe the latest version.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OrgApprovalDefinitionDTO {
    pub id: String,
    pub latest_version: i32,
    pub name: String,
    pub description: Option<String>,
    pub humans: Vec<ApprovalHumanDTO>,
    pub agent_type: Option<String>,
    pub agent_prompt: Option<String>,
    pub agent_harness: Option<String>,
    pub agent_model: Option<String>,
    pub agent_effort: Option<String>,
    pub invalidates_previous: bool,
    /// Phases the latest version binds, surfaced to every embedding surface
    /// (org library, workspace approvals state, fork modal, chevron meters).
    pub phases: Vec<String>,
    pub require_all_reviewers: bool,
    pub allow_self_approval: bool,
    pub human_approvals_override_agent: bool,
    pub created_by: ApprovalUserRefDTO,
    pub created_at: String,
    pub first_created_by: ApprovalUserRefDTO,
    pub first_created_at: String,
}

impl From<ApprovalDefinitionRecord> for OrgApprovalDefinitionDTO {
    fn from(r: ApprovalDefinitionRecord) -> Self {
        Self {
            id: r.id.to_string(),
            latest_version: r.latest_version,
            name: r.name,
            description: r.description,
            humans: r.humans.into_iter().map(ApprovalHumanDTO::from).collect(),
            agent_type: r.agent_type,
            agent_prompt: r.agent_prompt,
            agent_harness: r.agent_harness,
            agent_model: r.agent_model,
            agent_effort: r.agent_effort,
            invalidates_previous: r.invalidates_previous,
            phases: r.phases,
            require_all_reviewers: r.require_all_reviewers,
            allow_self_approval: r.allow_self_approval,
            human_approvals_override_agent: r.human_approvals_override_agent,
            created_by: ApprovalUserRefDTO {
                user_id: r.created_by.to_string(),
                display_name: r.created_by_display_name,
            },
            created_at: r.created_at.to_rfc3339(),
            first_created_by: ApprovalUserRefDTO {
                user_id: r.first_created_by.to_string(),
                display_name: r.first_created_by_display_name,
            },
            first_created_at: r.first_created_at.to_rfc3339(),
        }
    }
}

impl From<ApprovalHumanRecord> for ApprovalHumanDTO {
    fn from(h: ApprovalHumanRecord) -> Self {
        Self {
            user_id: h.user_id.to_string(),
            display_name: h.display_name,
            email: h.email,
            avatar_url: h.avatar_url,
            still_member: h.still_member,
            deactivated_at: h.deactivated_at.map(|t| t.to_rfc3339()),
        }
    }
}

// ─── Router ──────────────────────────────────────────────────────────────────

/// Approval-definition routes nested under
/// `/api/v1/orgs/:orgId/approval-definitions`.
///
/// Endpoints:
/// - `GET /` — list latest definitions (any org member)
/// - `POST /` — create definition v1 (org admin)
/// - `POST /:definitionId/versions` — publish next version (org admin)
///
/// Reads are member-visible because members must see the org's sign-off
/// requirements; writes are Admin, matching the member-management
/// surface.
pub fn org_approvals_router() -> Router<Arc<AppState>> {
    AuditedRouter::new()
        .exempt_route(
            "/",
            get(list_definitions),
            "Authenticated only — any org member can read the definition library",
        )
        .audited_route("/", post(create_definition), &[Scope::OrgAdmin])
        .audited_route(
            "/:definitionId/versions",
            post(publish_definition_version),
            &[Scope::OrgAdmin],
        )
        .into_router()
}

// ─── Handlers ────────────────────────────────────────────────────────────────

/// `GET /api/v1/orgs/:orgId/approval-definitions`
///
/// Lists every definition in the org at its latest version, name ASC, with
/// v1 provenance and profile-enriched approvers.
async fn list_definitions(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(params): Path<OrgPathParams>,
) -> Result<Json<ResponseEnvelope<Vec<OrgApprovalDefinitionDTO>>>, AppError> {
    let mut db = scoped_db_for_org(&state, &auth, params.org_id, &["workspaces:read"]).await?;
    tracing::debug!(org_id = %params.org_id, "Org approvals endpoint called: list_definitions");
    let records = state
        .approval_definition_service
        .list_latest(&mut db, &state.system_pool, params.org_id)
        .await?;
    let dtos: Vec<OrgApprovalDefinitionDTO> = records
        .into_iter()
        .map(OrgApprovalDefinitionDTO::from)
        .collect();
    db.commit().await.map_err(AppError::from)?;

    Ok(Json(ResponseEnvelope::ok(dtos)))
}

/// `POST /api/v1/orgs/:orgId/approval-definitions`
///
/// Creates version 1 of a new definition. Returns 201. Validation failures
/// surface as typed 422s with no row written; `invalidates_previous` is
/// always false at v1 and the request body has no field for it.
async fn create_definition(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(params): Path<OrgPathParams>,
    Json(body): Json<CreateOrgApprovalDefinitionRequestDTO>,
) -> Result<(StatusCode, Json<ResponseEnvelope<OrgApprovalDefinitionDTO>>), AppError> {
    let mut db = scoped_db_for_org(&state, &auth, params.org_id, &["org:admin"]).await?;
    tracing::debug!(org_id = %params.org_id, "Org approvals endpoint called: create_definition");
    let actor = auth.require_user_id()?;

    let record = state
        .approval_definition_service
        .create(
            &mut db,
            &state.system_pool,
            params.org_id,
            actor,
            CreateApprovalDefinitionRequest {
                name: body.name,
                description: body.description,
                human_user_ids: body.human_user_ids,
                agent_type: body.agent_type,
                agent_prompt: body.agent_prompt,
                agent_harness: body.agent_harness,
                agent_model: body.agent_model,
                agent_effort: body.agent_effort,
                phases: body.phases,
                require_all_reviewers: body.require_all_reviewers,
                allow_self_approval: body.allow_self_approval,
                human_approvals_override_agent: body.human_approvals_override_agent,
            },
        )
        .await?;
    tracing::info!(org_id = %params.org_id, definition_id = %record.id,
        "Org approvals endpoint succeeded: create_definition");
    db.commit().await.map_err(AppError::from)?;

    Ok((
        StatusCode::CREATED,
        Json(ResponseEnvelope::ok(OrgApprovalDefinitionDTO::from(record))),
    ))
}

/// `POST /api/v1/orgs/:orgId/approval-definitions/:definitionId/versions`
///
/// Appends the next version of an existing definition. 404 when the
/// definition does not exist in this org; 409 when a concurrent publish won
/// the race (nothing durable written — the client reloads and re-edits).
async fn publish_definition_version(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(params): Path<OrgApprovalDefinitionPathParams>,
    Json(body): Json<PublishOrgApprovalDefinitionVersionRequestDTO>,
) -> Result<Json<ResponseEnvelope<OrgApprovalDefinitionDTO>>, AppError> {
    let mut db = scoped_db_for_org(&state, &auth, params.org_id, &["org:admin"]).await?;
    tracing::debug!(org_id = %params.org_id, definition_id = %params.definition_id,
        "Org approvals endpoint called: publish_definition_version");
    let actor = auth.require_user_id()?;

    let record = state
        .approval_definition_service
        .publish_version(
            &mut db,
            &state.system_pool,
            params.org_id,
            params.definition_id,
            actor,
            PublishApprovalDefinitionVersionRequest {
                name: body.name,
                description: body.description,
                human_user_ids: body.human_user_ids,
                agent_type: body.agent_type,
                agent_prompt: body.agent_prompt,
                agent_harness: body.agent_harness,
                agent_model: body.agent_model,
                agent_effort: body.agent_effort,
                invalidates_previous: body.invalidates_previous,
                phases: body.phases,
                require_all_reviewers: body.require_all_reviewers,
                allow_self_approval: body.allow_self_approval,
                human_approvals_override_agent: body.human_approvals_override_agent,
            },
        )
        .await?;
    tracing::info!(org_id = %params.org_id, definition_id = %record.id,
        version = record.latest_version,
        "Org approvals endpoint succeeded: publish_definition_version");
    db.commit().await.map_err(AppError::from)?;

    Ok(Json(ResponseEnvelope::ok(OrgApprovalDefinitionDTO::from(
        record,
    ))))
}
