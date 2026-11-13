//! Workspace approvals route handlers (B6 governance UI PR-2).
//!
//! Mounted at `/api/v1/workspaces/:workspaceId/approvals`. Two subjects on
//! one surface (plan-doc D3 mapping): attach/retire of the approval
//! definitions required for merges INTO the viewed workspace
//! (`approval_configs`), and human Approve/Reject decisions for the viewed
//! workspace's own outgoing merge (`approval_statuses`,
//! `source_workspace_id` = this workspace). No enforcement — the finalizer
//! gate is a later PR. Handlers are thin: extract, delegate to
//! `ApprovalConfigService`, wrap in `ResponseEnvelope`.

use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Deserializer, Serialize};
use uuid::Uuid;

use crate::db::scoped::scoped_db_for_workspace;
use crate::errors::AppError;
use crate::middleware::AuthContext;
use crate::routes::audited::AuditedRouter;
use crate::routes::org_approvals::OrgApprovalDefinitionDTO;
use crate::routes::ResponseEnvelope;
use crate::services::approval_config_service::{
    ActiveRequirementRecord, AgentApprovalReviewRecord, AgentEscalation, ApprovalPath,
    ApprovalRequestRecord, ApproverStateRecord, AttachmentEventRecord, CreateHumanApprovalRequests,
    Currentness, DecisionRecord, DecisionRequest, HumanApprovalRequestAdmission,
    MergeAdmissionEvidence, OutgoingHistoryRecord, OutgoingRequirementRecord, OutgoingSection,
    RecipientNeeds, RequestNeeds, ReviewerRef, WorkspaceApprovalsState,
};
use crate::services::approval_review_service::CreateApprovalReviewRequest;
use crate::services::AppState;
use g5_auth::Scope;
use g5_contracts::types::{AgentLaunchSelectionV1, ApprovalPolicy};

// ─── Path Parameter Types ────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct WorkspacePathParams {
    #[serde(rename = "workspaceId")]
    pub workspace_id: Uuid,
}

#[derive(Debug, Deserialize)]
pub struct WorkspaceRequirementPathParams {
    #[serde(rename = "workspaceId")]
    pub workspace_id: Uuid,
    #[serde(rename = "definitionId")]
    pub definition_id: Uuid,
}

#[derive(Debug, Deserialize)]
pub struct WorkspaceApprovalRequestPathParams {
    #[serde(rename = "workspaceId")]
    pub workspace_id: Uuid,
    #[serde(rename = "requestId")]
    pub request_id: Uuid,
}

// ─── Request DTOs ────────────────────────────────────────────────────────────

/// Body for `POST /api/v1/workspaces/:workspaceId/approvals/requirements`.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AttachRequirementRequestDTO {
    pub definition_id: Uuid,
}

/// Body for `POST /api/v1/workspaces/:workspaceId/approvals/decisions`.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RecordDecisionRequestDTO {
    pub reviewed_git_heads: Vec<g5_contracts::types::MergeSourceHead>,
    pub definition_id: Uuid,
    #[serde(default)]
    pub request_id: Option<Uuid>,
    /// `approved` or `rejected`; `deferred` is deliberately unexposed.
    pub outcome: String,
    #[serde(default)]
    pub comment: Option<String>,
    /// Phase this decision settles (`be` | `se` | `ie`, g5.workbench.gates);
    /// legacy phase vocabulary is validated; the addressed request retains its historical phase.
    #[serde(default = "workspace_approval_phase_default")]
    pub phase: String,
}

/// Body for `POST /api/v1/workspaces/:workspaceId/approvals/requests`.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreateHumanApprovalRequestsDTO {
    /// Stable identity for one owner submission, retained across retries.
    #[serde(default)]
    pub client_request_id: Option<Uuid>,
    pub definition_id: Uuid,
    pub definition_version: i32,
    /// Compatibility phase input (one workspace-wide chain per definition version
    /// and actor).
    #[serde(default = "workspace_approval_phase_default")]
    pub phase: String,
    #[serde(default, deserialize_with = "deserialize_optional_actor_user_ids")]
    pub actor_user_ids: Option<Vec<Uuid>>,
    #[serde(default)]
    pub note: Option<String>,
}

fn workspace_approval_phase_default() -> String {
    g5_contracts::types::WORKSPACE_APPROVAL_COMPAT_PHASE.to_string()
}

fn deserialize_optional_actor_user_ids<'de, D>(
    deserializer: D,
) -> Result<Option<Vec<Uuid>>, D::Error>
where
    D: Deserializer<'de>,
{
    Option::<Vec<Uuid>>::deserialize(deserializer)?
        .map(Some)
        .ok_or_else(|| serde::de::Error::custom("actorUserIds must not be null"))
}

/// Body for `POST /api/v1/workspaces/:workspaceId/approvals/reviews`.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreateApprovalReviewRequestDTO {
    #[serde(default)]
    pub client_request_id: Option<Uuid>,
    #[serde(default)]
    pub retry_review_id: Option<Uuid>,
    pub definition_id: Uuid,
    pub definition_version: i32,
    #[serde(default)]
    pub agent_launch: Option<AgentLaunchSelectionV1>,
    /// Compatibility phase input; recommendation associations do not limit admission.
    #[serde(default = "workspace_approval_phase_default")]
    pub phase: String,
}

// ─── Response DTOs ───────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApprovalUserRefDTO {
    pub user_id: String,
    pub display_name: String,
}

/// An active requirement of the viewed workspace (target side).
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RequiredIntoDTO {
    pub definition: OrgApprovalDefinitionDTO,
    pub attached_by: ApprovalUserRefDTO,
    pub attached_at: String,
}

/// One attach/retire event (bottom history).
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AttachmentEventDTO {
    pub definition_id: String,
    pub definition_name: String,
    pub retired: bool,
    pub actor: ApprovalUserRefDTO,
    pub created_at: String,
}

/// Per-approver current state on an outgoing requirement.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApproverStateDTO {
    pub user_id: String,
    pub display_name: String,
    pub email: String,
    pub still_member: bool,
    pub deactivated_at: Option<String>,
    pub state: String,
    pub comment: Option<String>,
    pub decided_at: Option<String>,
    pub request_id: Option<String>,
    pub request_state: Option<String>,
    pub requested_at: Option<String>,
    pub closed_at: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApprovalRequestDTO {
    pub request_id: String,
    pub actor_type: String,
    pub actor_user_id: Option<String>,
    pub actor: Option<ApprovalUserRefDTO>,
    pub requested_by: Option<ApprovalUserRefDTO>,
    pub state: String,
    pub definition_version: i32,
    pub definition_name: String,
    /// Phase of the request chain; `null` for a request admitted under the
    /// stage model before the cutover (terminal since then).
    pub phase: Option<String>,
    pub requested_at: String,
    pub closed_at: Option<String>,
    pub agent_reviews: Vec<AgentApprovalReviewDTO>,
    pub note: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentApprovalReviewDTO {
    pub review_id: Option<String>,
    pub stage: Option<String>,
    pub cleanup_pending: bool,
    pub cleanup_error: Option<String>,
    pub cleanup_overdue: bool,
    pub report_available: bool,
    pub retry_available: bool,
    pub content_sha256: Option<String>,
    pub task_id: Option<String>,
    pub session_id: Option<String>,
    pub state: String,
    pub terminal_reason: Option<String>,
    pub created_at: String,
    pub started_at: Option<String>,
    pub ended_at: Option<String>,
    pub updated_at: String,
}

/// One immutable decision-log entry.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DecisionLogEntryDTO {
    pub agent_review_id: Option<String>,
    pub request_id: Option<String>,
    pub outcome: String,
    pub comment: Option<String>,
    pub summary: Option<String>,
    pub actor_type: String,
    pub actor: Option<ApprovalUserRefDTO>,
    pub definition_version: i32,
    pub superseded: bool,
    pub reviewed_git_heads: serde_json::Value,
    pub created_at: String,
}

/// One requirement of the merge target with decision state (source side).
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OutgoingRequirementDTO {
    pub definition: OrgApprovalDefinitionDTO,
    /// Compatibility field; one entry per attached workspace-wide definition.
    pub phase: String,
    pub state: String,
    pub approvers: Vec<ApproverStateDTO>,
    pub caller_can_decide: bool,
    pub requests: Vec<ApprovalRequestDTO>,
    pub decision_log: Vec<DecisionLogEntryDTO>,
    /// Aggregate commit currentness; never changes `state`.
    pub currentness: CurrentnessDTO,
    /// Remaining path for a Rejected requirement or a responded Pending one.
    pub approval_path: Option<ApprovalPathDTO>,
    pub request_needs: RequestNeedsDTO,
    /// How an overridable agent rejection reads; display only, never a status.
    pub agent_escalation: Option<AgentEscalationDTO>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentEscalationDTO {
    /// `needsHumanEscalation` or `overriddenByHuman`.
    pub state: &'static str,
    /// The counted agent decision this display describes.
    pub agent_review_id: Option<String>,
}

impl From<AgentEscalation> for AgentEscalationDTO {
    fn from(escalation: AgentEscalation) -> Self {
        Self {
            state: escalation.state.as_str(),
            agent_review_id: escalation.agent_review_id.map(|id| id.to_string()),
        }
    }
}

/// One configured approver named by the requirement evaluator.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewerRefDTO {
    pub actor_type: &'static str,
    pub user_id: Option<String>,
    pub display_name: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CurrentnessDTO {
    pub state: &'static str,
    pub not_current: Vec<ReviewerRefDTO>,
}

#[derive(Debug, Serialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum ApprovalPathDTO {
    HumansRejected {
        humans: Vec<ReviewerRefDTO>,
        others_required: bool,
    },
    AgentRejected {
        agent_name: String,
        human_override: bool,
    },
    Needed {
        approvers: Vec<ReviewerRefDTO>,
        require_all: bool,
        all_humans_required: bool,
    },
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecipientNeedsDTO {
    pub agent: bool,
    pub humans: Vec<String>,
    pub random_human_from: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RequestNeedsDTO {
    pub outstanding: RecipientNeedsDTO,
    pub repeat: RecipientNeedsDTO,
    pub agent_requestable: bool,
    pub agent_approved: bool,
}

impl From<ReviewerRef> for ReviewerRefDTO {
    fn from(reviewer: ReviewerRef) -> Self {
        Self {
            actor_type: reviewer.actor_type,
            user_id: reviewer.user_id.map(|id| id.to_string()),
            display_name: reviewer.display_name,
        }
    }
}

impl From<Currentness> for CurrentnessDTO {
    fn from(currentness: Currentness) -> Self {
        Self {
            state: currentness.state.as_str(),
            not_current: currentness
                .not_current
                .into_iter()
                .map(Into::into)
                .collect(),
        }
    }
}

impl From<ApprovalPath> for ApprovalPathDTO {
    fn from(path: ApprovalPath) -> Self {
        let refs = |reviewers: Vec<ReviewerRef>| reviewers.into_iter().map(Into::into).collect();
        match path {
            ApprovalPath::HumansRejected {
                humans,
                others_required,
            } => Self::HumansRejected {
                humans: refs(humans),
                others_required,
            },
            ApprovalPath::AgentRejected {
                agent_name,
                human_override,
            } => Self::AgentRejected {
                agent_name,
                human_override,
            },
            ApprovalPath::Needed {
                approvers,
                require_all,
                all_humans_required,
            } => Self::Needed {
                approvers: refs(approvers),
                require_all,
                all_humans_required,
            },
        }
    }
}

impl From<RecipientNeeds> for RecipientNeedsDTO {
    fn from(needs: RecipientNeeds) -> Self {
        let ids = |ids: Vec<Uuid>| ids.into_iter().map(|id| id.to_string()).collect();
        Self {
            agent: needs.agent,
            humans: ids(needs.humans),
            random_human_from: ids(needs.random_human_from),
        }
    }
}

impl From<RequestNeeds> for RequestNeedsDTO {
    fn from(needs: RequestNeeds) -> Self {
        Self {
            outstanding: needs.outstanding.into(),
            repeat: needs.repeat.into(),
            agent_requestable: needs.agent_requestable,
            agent_approved: needs.agent_approved,
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OutgoingHistoryDTO {
    pub definition_id: String,
    pub definition_name: String,
    pub requests: Vec<ApprovalRequestDTO>,
    pub decision_log: Vec<DecisionLogEntryDTO>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OutgoingSectionDTO {
    pub target_workspace_id: String,
    pub target_workspace_name: String,
    pub requirements: Vec<OutgoingRequirementDTO>,
    pub history: Vec<OutgoingHistoryDTO>,
    /// Server-evaluated merge admission against the current source cut:
    /// only Integration Engineering requirements gate the merge
    /// (g5.governance.approval.requirements); every shipping surface reads
    /// this rather than recomputing it.
    pub merge_admission: MergeAdmissionDTO,
    /// The request rule and source-commit policy the server enforces.
    pub request_policy: ApprovalPolicy,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MergeAdmissionRequirementDTO {
    pub definition_id: String,
    pub definition_name: String,
    pub definition_version: i32,
    pub state: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MergeAdmissionDTO {
    pub admitted: bool,
    pub reason: Option<String>,
    pub requirements: Vec<MergeAdmissionRequirementDTO>,
}

impl From<MergeAdmissionEvidence> for MergeAdmissionDTO {
    fn from(evidence: MergeAdmissionEvidence) -> Self {
        Self {
            admitted: evidence.admitted,
            reason: evidence.reason,
            requirements: evidence
                .requirements
                .into_iter()
                .map(|requirement| MergeAdmissionRequirementDTO {
                    definition_id: requirement.definition_id.to_string(),
                    definition_name: requirement.definition_name,
                    definition_version: requirement.definition_version,
                    state: requirement.state,
                })
                .collect(),
        }
    }
}

/// Combined tab state.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceApprovalsStateDTO {
    pub required_into: Vec<RequiredIntoDTO>,
    pub attachment_history: Vec<AttachmentEventDTO>,
    pub outgoing: Option<OutgoingSectionDTO>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApprovalReviewAdmissionDTO {
    pub request_id: String,
    pub review_id: String,
    pub status: String,
    pub reused: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HumanApprovalRequestAdmissionDTO {
    pub request_id: String,
    pub actor_user_id: String,
    pub status: String,
    pub reused: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HumanApprovalRequestsAdmissionDTO {
    pub requests: Vec<HumanApprovalRequestAdmissionDTO>,
}

impl From<WorkspaceApprovalsState> for WorkspaceApprovalsStateDTO {
    fn from(s: WorkspaceApprovalsState) -> Self {
        Self {
            required_into: s.required_into.into_iter().map(Into::into).collect(),
            attachment_history: s.attachment_history.into_iter().map(Into::into).collect(),
            outgoing: s.outgoing.map(Into::into),
        }
    }
}

impl From<ActiveRequirementRecord> for RequiredIntoDTO {
    fn from(r: ActiveRequirementRecord) -> Self {
        Self {
            definition: r.definition.into(),
            attached_by: ApprovalUserRefDTO {
                user_id: r.attached_by.to_string(),
                display_name: r.attached_by_display_name,
            },
            attached_at: r.attached_at.to_rfc3339(),
        }
    }
}

impl From<AttachmentEventRecord> for AttachmentEventDTO {
    fn from(e: AttachmentEventRecord) -> Self {
        Self {
            definition_id: e.definition_id.to_string(),
            definition_name: e.definition_name,
            retired: e.retired,
            actor: ApprovalUserRefDTO {
                user_id: e.actor_user_id.to_string(),
                display_name: e.actor_display_name,
            },
            created_at: e.created_at.to_rfc3339(),
        }
    }
}

impl From<ApproverStateRecord> for ApproverStateDTO {
    fn from(a: ApproverStateRecord) -> Self {
        Self {
            user_id: a.user_id.to_string(),
            display_name: a.display_name,
            email: a.email,
            still_member: a.still_member,
            deactivated_at: a.deactivated_at.map(|t| t.to_rfc3339()),
            state: a.state,
            comment: a.comment,
            decided_at: a.decided_at.map(|t| t.to_rfc3339()),
            request_id: a.request_id.map(|id| id.to_string()),
            request_state: a.request_state,
            requested_at: a.requested_at.map(|time| time.to_rfc3339()),
            closed_at: a.closed_at.map(|time| time.to_rfc3339()),
        }
    }
}

impl From<ApprovalRequestRecord> for ApprovalRequestDTO {
    fn from(request: ApprovalRequestRecord) -> Self {
        Self {
            note: request.note,
            request_id: request.request_id.to_string(),
            actor_type: request.actor_type,
            actor_user_id: request.actor_user_id.map(|id| id.to_string()),
            actor: request.actor_user_id.map(|user_id| ApprovalUserRefDTO {
                user_id: user_id.to_string(),
                display_name: request
                    .actor_display_name
                    .unwrap_or_else(|| "Unknown user".to_string()),
            }),
            requested_by: request.requested_by.map(|user_id| ApprovalUserRefDTO {
                user_id: user_id.to_string(),
                display_name: request
                    .requested_by_display_name
                    .unwrap_or_else(|| "Unknown user".to_string()),
            }),
            state: request.state,
            definition_version: request.definition_version,
            definition_name: request.definition_name,
            phase: request.phase,
            requested_at: request.requested_at.to_rfc3339(),
            closed_at: request.closed_at.map(|time| time.to_rfc3339()),
            agent_reviews: request.agent_reviews.into_iter().map(Into::into).collect(),
        }
    }
}

impl From<AgentApprovalReviewRecord> for AgentApprovalReviewDTO {
    fn from(review: AgentApprovalReviewRecord) -> Self {
        Self {
            review_id: review.review_id.map(|id| id.to_string()),
            stage: review.stage,
            cleanup_pending: review.cleanup_pending,
            cleanup_error: review.cleanup_error,
            cleanup_overdue: review.cleanup_overdue,
            report_available: review.report_available,
            retry_available: review.retry_available,
            content_sha256: review.content_sha256,
            task_id: review.task_id.map(|id| id.to_string()),
            session_id: review.session_id.map(|id| id.to_string()),
            state: review.state,
            terminal_reason: review.terminal_reason,
            created_at: review.created_at.to_rfc3339(),
            started_at: review.started_at.map(|time| time.to_rfc3339()),
            ended_at: review.ended_at.map(|time| time.to_rfc3339()),
            updated_at: review.updated_at.to_rfc3339(),
        }
    }
}

impl From<HumanApprovalRequestAdmission> for HumanApprovalRequestAdmissionDTO {
    fn from(admission: HumanApprovalRequestAdmission) -> Self {
        Self {
            request_id: admission.request_id.to_string(),
            actor_user_id: admission.actor_user_id.to_string(),
            status: "pending".to_string(),
            reused: admission.reused,
        }
    }
}

impl From<DecisionRecord> for DecisionLogEntryDTO {
    fn from(d: DecisionRecord) -> Self {
        Self {
            agent_review_id: d.agent_review_id.map(|id| id.to_string()),
            request_id: d.request_id.map(|id| id.to_string()),
            outcome: d.outcome,
            comment: d.comment,
            summary: d.summary,
            actor_type: d.actor_type,
            actor: d.actor_user_id.map(|user_id| ApprovalUserRefDTO {
                user_id: user_id.to_string(),
                display_name: d
                    .actor_display_name
                    .unwrap_or_else(|| "Unknown user".to_string()),
            }),
            definition_version: d.definition_version,
            superseded: d.superseded,
            reviewed_git_heads: d.reviewed_git_heads,
            created_at: d.created_at.to_rfc3339(),
        }
    }
}

impl From<OutgoingRequirementRecord> for OutgoingRequirementDTO {
    fn from(r: OutgoingRequirementRecord) -> Self {
        Self {
            definition: r.definition.into(),
            phase: r.phase,
            state: r.state,
            approvers: r.approvers.into_iter().map(Into::into).collect(),
            caller_can_decide: r.caller_can_decide,
            requests: r.requests.into_iter().map(Into::into).collect(),
            decision_log: r.decision_log.into_iter().map(Into::into).collect(),
            currentness: r.evaluation.currentness.into(),
            approval_path: r.evaluation.path.map(Into::into),
            request_needs: r.evaluation.needs.into(),
            agent_escalation: r.evaluation.agent_escalation.map(Into::into),
        }
    }
}

impl From<OutgoingHistoryRecord> for OutgoingHistoryDTO {
    fn from(record: OutgoingHistoryRecord) -> Self {
        Self {
            definition_id: record.definition_id.to_string(),
            definition_name: record.definition_name,
            requests: record.requests.into_iter().map(Into::into).collect(),
            decision_log: record.decision_log.into_iter().map(Into::into).collect(),
        }
    }
}

impl From<OutgoingSection> for OutgoingSectionDTO {
    fn from(s: OutgoingSection) -> Self {
        Self {
            target_workspace_id: s.target_workspace_id.to_string(),
            target_workspace_name: s.target_workspace_name,
            requirements: s.requirements.into_iter().map(Into::into).collect(),
            history: s.history.into_iter().map(Into::into).collect(),
            merge_admission: s.merge_admission.into(),
            request_policy: s.policy,
        }
    }
}

// ─── Router ──────────────────────────────────────────────────────────────────

/// Workspace approvals routes nested under
/// `/api/v1/workspaces/:workspaceId/approvals`.
///
/// Endpoints:
/// - `GET /` — combined state (any workspace member)
/// - `POST /requirements` — attach requirement (org admin)
/// - `POST /requirements/:definitionId/retire` — retire requirement (org admin)
/// - `POST /decisions` — record an addressed reviewer's decision after PR
///   submission; the service locks the exact addressed request and retains
///   the complete pushed repository vector that the reviewer inspected.
/// - `POST /requests/:requestId/cancel` — cancel a pending human request
///   (exact stored requester only)
pub fn workspace_approvals_router() -> Router<Arc<AppState>> {
    AuditedRouter::new()
        .exempt_route(
            "/",
            get(get_approvals_state),
            "Authenticated only — any workspace member can read approval state",
        )
        .audited_route(
            "/requirements",
            post(attach_requirement),
            &[Scope::OrgAdmin],
        )
        .audited_route(
            "/requirements/:definitionId/retire",
            post(retire_requirement),
            &[Scope::OrgAdmin],
        )
        .audited_route(
            "/decisions",
            post(record_decision),
            &[Scope::WorkspacesRead],
        )
        .audited_route(
            "/requests",
            post(create_human_requests),
            &[Scope::WorkspacesWrite],
        )
        .exempt_route(
            "/requests/:requestId/cancel",
            post(cancel_human_request),
            "Exact requester identity is the authorization: the service locks the current-org, \
             source-workspace review request by requested_by before cancellation",
        )
        .audited_route("/reviews", post(create_review), &[Scope::WorkspacesWrite])
        .audited_route(
            "/reviews/:reviewId/report",
            get(get_review_report),
            &[Scope::WorkspacesRead],
        )
        .audited_route(
            "/reviews/:reviewId/commits",
            get(get_review_commits),
            &[Scope::WorkspacesRead],
        )
        .audited_route(
            "/reviews/:reviewId/transcript",
            get(get_review_transcript),
            &[Scope::WorkspacesRead],
        )
        .audited_route(
            "/reviews/:reviewId/cleanup",
            post(retry_review_cleanup),
            &[Scope::WorkspacesWrite],
        )
        .into_router()
}

// ─── Handlers ────────────────────────────────────────────────────────────────

/// Resolve the owning org via the service (no-direct-DB-in-handlers guard);
/// the lookup runs on the ALREADY-SCOPED connection — `workspaces` carries
/// forced RLS, so a system-pool read without org context sees nothing.
async fn org_of_workspace(
    state: &AppState,
    db: &mut g5_authz::ScopedDb,
    workspace_id: Uuid,
) -> Result<Uuid, AppError> {
    Ok(state
        .approval_config_service
        .org_of_workspace(db, workspace_id)
        .await?)
}

/// `GET /api/v1/workspaces/:workspaceId/approvals`
async fn get_approvals_state(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(params): Path<WorkspacePathParams>,
) -> Result<Json<ResponseEnvelope<WorkspaceApprovalsStateDTO>>, AppError> {
    let db =
        scoped_db_for_workspace(&state, &auth, params.workspace_id, &["workspaces:read"]).await?;
    tracing::debug!(workspace_id = %params.workspace_id,
        "Workspace approvals endpoint called: get_approvals_state");
    let caller = auth.require_user_id()?;
    let result = approvals_state_with(&state, db, params.workspace_id, caller).await?;
    Ok(Json(ResponseEnvelope::ok(result)))
}

/// A workspace's approval state as `caller` sees it, on a scope the caller
/// already holds for the workspace. Shared with the workspace agent.
pub(crate) async fn approvals_state_with(
    state: &Arc<AppState>,
    mut db: g5_authz::ScopedDb,
    workspace_id: Uuid,
    caller: Uuid,
) -> Result<WorkspaceApprovalsStateDTO, AppError> {
    let org_id = org_of_workspace(state, &mut db, workspace_id).await?;
    let result = state
        .approval_config_service
        .workspace_state(
            &mut db,
            &state.system_pool,
            &state.approval_definition_service,
            org_id,
            workspace_id,
            caller,
        )
        .await?;
    db.commit().await.map_err(AppError::from)?;
    Ok(result.into())
}

/// `POST /api/v1/workspaces/:workspaceId/approvals/requirements`
async fn attach_requirement(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(params): Path<WorkspacePathParams>,
    Json(body): Json<AttachRequirementRequestDTO>,
) -> Result<Json<ResponseEnvelope<WorkspaceApprovalsStateDTO>>, AppError> {
    // Requirement administration is organization authority over the target
    // workspace's merge rules, not workspace ownership (issue #278). Request
    // only `org:admin`: a `workspaces:write` scope admits the current workspace
    // owner alone (`scopes_require_current_workspace_owner`).
    let mut db =
        scoped_db_for_workspace(&state, &auth, params.workspace_id, &["org:admin"]).await?;
    tracing::debug!(workspace_id = %params.workspace_id, definition_id = %body.definition_id,
        "Workspace approvals endpoint called: attach_requirement");
    let org_id = org_of_workspace(&state, &mut db, params.workspace_id).await?;
    let caller = auth.require_user_id()?;

    let result = state
        .approval_config_service
        .attach(
            &mut db,
            &state.system_pool,
            &state.approval_definition_service,
            org_id,
            params.workspace_id,
            caller,
            body.definition_id,
        )
        .await?;
    tracing::info!(workspace_id = %params.workspace_id, definition_id = %body.definition_id,
        "Workspace approvals endpoint succeeded: attach_requirement");
    db.commit().await.map_err(AppError::from)?;

    Ok(Json(ResponseEnvelope::ok(result.into())))
}

/// `POST /api/v1/workspaces/:workspaceId/approvals/requirements/:definitionId/retire`
async fn retire_requirement(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(params): Path<WorkspaceRequirementPathParams>,
) -> Result<Json<ResponseEnvelope<WorkspaceApprovalsStateDTO>>, AppError> {
    // Org-admin authority, not workspace ownership (see `attach_requirement`).
    let mut db =
        scoped_db_for_workspace(&state, &auth, params.workspace_id, &["org:admin"]).await?;
    tracing::debug!(workspace_id = %params.workspace_id, definition_id = %params.definition_id,
        "Workspace approvals endpoint called: retire_requirement");
    let org_id = org_of_workspace(&state, &mut db, params.workspace_id).await?;
    let caller = auth.require_user_id()?;

    let result = state
        .approval_config_service
        .retire(
            &mut db,
            &state.system_pool,
            &state.approval_definition_service,
            org_id,
            params.workspace_id,
            caller,
            params.definition_id,
        )
        .await?;
    tracing::info!(workspace_id = %params.workspace_id, definition_id = %params.definition_id,
        "Workspace approvals endpoint succeeded: retire_requirement");
    db.commit().await.map_err(AppError::from)?;

    Ok(Json(ResponseEnvelope::ok(result.into())))
}

/// `POST /api/v1/workspaces/:workspaceId/approvals/decisions`
async fn record_decision(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(params): Path<WorkspacePathParams>,
    Json(body): Json<RecordDecisionRequestDTO>,
) -> Result<Json<ResponseEnvelope<WorkspaceApprovalsStateDTO>>, AppError> {
    // A request-addressed decision is the addressed human's authority,
    // independent of workspace ownership: it needs only read access here, and
    // the service admits it only for the caller's own pending request.
    // Requestless decisions keep the legacy owner-only guard.
    let mut db = if body.request_id.is_some() {
        scoped_db_for_workspace(&state, &auth, params.workspace_id, &["workspaces:read"]).await?
    } else {
        let mut db =
            scoped_db_for_workspace(&state, &auth, params.workspace_id, &["workspaces:write"])
                .await?;
        crate::services::workspace_mutation_policy::ensure_workspace_owner(
            &mut db,
            params.workspace_id,
        )
        .await?;
        db
    };
    tracing::debug!(workspace_id = %params.workspace_id, definition_id = %body.definition_id,
        outcome = %body.outcome,
        "Workspace approvals endpoint called: record_decision");
    let org_id = org_of_workspace(&state, &mut db, params.workspace_id).await?;
    let caller = auth.require_user_id()?;
    let settled_request = body.request_id;

    let result = state
        .approval_config_service
        .record_human_decision(
            &mut db,
            &state.system_pool,
            &state.approval_definition_service,
            org_id,
            params.workspace_id,
            caller,
            DecisionRequest {
                reviewed_git_heads: body.reviewed_git_heads,
                definition_id: body.definition_id,
                request_id: body.request_id,
                outcome: body.outcome,
                comment: body.comment,
                phase: body.phase,
            },
        )
        .await?;
    tracing::info!(workspace_id = %params.workspace_id, definition_id = %body.definition_id,
        "Workspace approvals endpoint succeeded: record_decision");
    db.commit().await.map_err(AppError::from)?;
    // Best-effort owner notification, after the decision transaction commits
    // (`g5.governance.approval.decision-email`). A replayed decision reaches
    // here too; the email's single-use key returns the one already queued.
    if let Some(request_id) = settled_request {
        notify_workspace_owner_of_decision(&state, org_id, params.workspace_id, request_id).await;
    }

    Ok(Json(ResponseEnvelope::ok(result.into())))
}

/// Queue the owner-notification email for the decision that settled
/// `request_id`, on a system-pool transaction after the decision committed.
/// Best-effort: any failure is logged and swallowed, since the decision is
/// already recorded.
async fn notify_workspace_owner_of_decision(
    state: &Arc<AppState>,
    org_id: Uuid,
    source_workspace_id: Uuid,
    request_id: Uuid,
) {
    if !state.email.is_enabled() {
        return;
    }
    let result = async {
        let mut db = crate::db::scoped::scoped_db_for_system_org_with(
            state.authz.as_ref(),
            &state.system_pool,
            org_id,
            &["workspaces:read"],
        )
        .await?;
        g5_api_core::services::approval_request_email::request_approval_decision_email(
            &state.email,
            &mut db,
            org_id,
            source_workspace_id,
            request_id,
        )
        .await?;
        db.commit().await.map_err(AppError::from)?;
        Ok::<(), AppError>(())
    }
    .await;
    if let Err(error) = result {
        tracing::warn!(
            source_workspace_id = %source_workspace_id,
            request_id = %request_id,
            %error,
            "Failed to queue the approval decision owner email"
        );
    }
}

/// `POST /api/v1/workspaces/:workspaceId/approvals/requests`
async fn create_human_requests(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(params): Path<WorkspacePathParams>,
    Json(body): Json<CreateHumanApprovalRequestsDTO>,
) -> Result<
    (
        StatusCode,
        Json<ResponseEnvelope<HumanApprovalRequestsAdmissionDTO>>,
    ),
    AppError,
> {
    let mut db =
        scoped_db_for_workspace(&state, &auth, params.workspace_id, &["workspaces:write"]).await?;
    crate::services::workspace_mutation_policy::ensure_workspace_owner(
        &mut db,
        params.workspace_id,
    )
    .await?;
    let caller = auth.require_user_id()?;
    create_human_requests_with(&state, db, params.workspace_id, caller, body).await
}

/// Request human approvals for `workspace_id` as `caller`, its owner, on a
/// scope that already checked that ownership. Shared with the workspace agent.
pub(crate) async fn create_human_requests_with(
    state: &Arc<AppState>,
    mut db: g5_authz::ScopedDb,
    workspace_id: Uuid,
    caller: Uuid,
    body: CreateHumanApprovalRequestsDTO,
) -> Result<
    (
        StatusCode,
        Json<ResponseEnvelope<HumanApprovalRequestsAdmissionDTO>>,
    ),
    AppError,
> {
    let org_id = org_of_workspace(state, &mut db, workspace_id).await?;
    let admissions = state
        .approval_config_service
        .create_human_requests(
            &mut db,
            org_id,
            workspace_id,
            caller,
            CreateHumanApprovalRequests {
                client_request_id: body.client_request_id,
                definition_id: body.definition_id,
                definition_version: body.definition_version,
                phase: body.phase,
                actor_user_ids: body.actor_user_ids,
                note: body.note,
            },
        )
        .await?;
    let created = admissions.iter().any(|admission| !admission.reused);
    // The requester's own scope, reopened after commit to read the PR the
    // emails quote, so no transaction is held across that Git read.
    let requester_scope = db.reopener();
    db.commit().await.map_err(AppError::from)?;
    // Best-effort reviewer notification, after the request transaction commits:
    // the email queue is system-owned and cannot share the tenant transaction
    // (`g5.governance.approval.request-email`). A failure here never fails the
    // already-created request.
    if created {
        notify_requested_reviewers(
            state,
            &requester_scope,
            org_id,
            workspace_id,
            caller,
            &admissions,
        )
        .await;
    }
    tracing::info!(
        workspace_id = %workspace_id,
        definition_id = %body.definition_id,
        request_count = admissions.len(),
        created,
        "Workspace approvals endpoint succeeded: create_human_requests"
    );
    Ok((
        if created {
            StatusCode::CREATED
        } else {
            StatusCode::OK
        },
        Json(ResponseEnvelope::ok(HumanApprovalRequestsAdmissionDTO {
            requests: admissions.into_iter().map(Into::into).collect(),
        })),
    ))
}

/// Queue the reviewer-notification emails on a system-pool transaction, after
/// the request transaction has committed. Best-effort: any failure is logged
/// and swallowed, since the requests already exist.
async fn notify_requested_reviewers(
    state: &Arc<AppState>,
    requester_scope: &g5_authz::ScopedDbReopener,
    org_id: Uuid,
    source_workspace_id: Uuid,
    requested_by: Uuid,
    admissions: &[HumanApprovalRequestAdmission],
) {
    if !state.email.is_enabled() {
        return;
    }
    let description = requested_pull_request_description(state, requester_scope, admissions)
        .await
        .unwrap_or_else(|error| {
            // The email is still sent, without the description.
            tracing::warn!(
                source_workspace_id = %source_workspace_id,
                %error,
                "Failed to read the PR description for approval request emails"
            );
            None
        });
    let result = async {
        let mut db = crate::db::scoped::scoped_db_for_system_org_with(
            state.authz.as_ref(),
            &state.system_pool,
            org_id,
            &["workspaces:read"],
        )
        .await?;
        g5_api_core::services::approval_request_email::request_approval_request_emails(
            &state.email,
            &mut db,
            org_id,
            source_workspace_id,
            requested_by,
            admissions,
            description.as_deref(),
        )
        .await?;
        db.commit().await.map_err(AppError::from)?;
        Ok::<(), AppError>(())
    }
    .await;
    if let Err(error) = result {
        tracing::warn!(
            source_workspace_id = %source_workspace_id,
            %error,
            "Failed to queue approval request reviewer emails"
        );
    }
}

/// The canonical description of the PR the new requests are for, quoted in
/// their emails. Read under the requester's own scope (the system role cannot
/// read workspace PRs), on a short read-only transaction that is rolled back.
async fn requested_pull_request_description(
    state: &Arc<AppState>,
    requester_scope: &g5_authz::ScopedDbReopener,
    admissions: &[HumanApprovalRequestAdmission],
) -> Result<Option<String>, AppError> {
    let mut read = requester_scope.open().await.map_err(AppError::from)?;
    let description = async {
        let Some(pull_request_id) =
            g5_api_core::services::approval_request_email::requested_pull_request(
                &mut read, admissions,
            )
            .await?
        else {
            return Ok(None);
        };
        state
            .workspace_pull_request_provider_lifecycle_service
            .read_canonical_description(&mut read, pull_request_id)
            .await
    }
    .await;
    let _ = read.rollback().await;
    description.map_err(AppError::from)
}

/// `POST /api/v1/workspaces/:workspaceId/approvals/requests/:requestId/cancel`
async fn cancel_human_request(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(params): Path<WorkspaceApprovalRequestPathParams>,
) -> Result<Json<ResponseEnvelope<WorkspaceApprovalsStateDTO>>, AppError> {
    let mut db =
        scoped_db_for_workspace(&state, &auth, params.workspace_id, &["workspaces:write"]).await?;
    crate::services::workspace_mutation_policy::ensure_workspace_owner(
        &mut db,
        params.workspace_id,
    )
    .await?;
    let caller = auth.require_user_id()?;
    let result =
        cancel_human_request_with(&state, db, params.workspace_id, caller, params.request_id)
            .await?;
    Ok(Json(ResponseEnvelope::ok(result)))
}

/// Cancel a human approval request `caller` made for `workspace_id`, on a
/// scope that already checked ownership. Shared with the workspace agent.
pub(crate) async fn cancel_human_request_with(
    state: &Arc<AppState>,
    mut db: g5_authz::ScopedDb,
    workspace_id: Uuid,
    caller: Uuid,
    request_id: Uuid,
) -> Result<WorkspaceApprovalsStateDTO, AppError> {
    let org_id = org_of_workspace(state, &mut db, workspace_id).await?;
    let result = state
        .approval_config_service
        .cancel_human_request(
            &mut db,
            &state.system_pool,
            &state.approval_definition_service,
            org_id,
            workspace_id,
            caller,
            request_id,
        )
        .await?;
    db.commit().await.map_err(AppError::from)?;
    tracing::info!(
        workspace_id = %workspace_id,
        request_id = %request_id,
        requester_id = %caller,
        "Workspace approvals endpoint succeeded: cancel_human_request"
    );
    Ok(result.into())
}

/// `POST /api/v1/workspaces/:workspaceId/approvals/reviews`
async fn create_review(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(params): Path<WorkspacePathParams>,
    Json(body): Json<CreateApprovalReviewRequestDTO>,
) -> Result<
    (
        StatusCode,
        Json<ResponseEnvelope<ApprovalReviewAdmissionDTO>>,
    ),
    AppError,
> {
    let mut db =
        scoped_db_for_workspace(&state, &auth, params.workspace_id, &["workspaces:write"]).await?;
    crate::services::workspace_mutation_policy::ensure_workspace_owner(
        &mut db,
        params.workspace_id,
    )
    .await?;
    let caller = auth.require_user_id()?;
    create_review_with(&state, db, params.workspace_id, caller, body).await
}

/// Request a formal agent review of `workspace_id` as `caller`, its owner, on
/// a scope that already checked that ownership, reconciling an uncertain
/// commit acknowledgement. Shared with the workspace agent: requesting is not
/// the verdict, which the confined approver produces.
pub(crate) async fn create_review_with(
    state: &Arc<AppState>,
    db: g5_authz::ScopedDb,
    workspace_id: Uuid,
    caller: Uuid,
    body: CreateApprovalReviewRequestDTO,
) -> Result<
    (
        StatusCode,
        Json<ResponseEnvelope<ApprovalReviewAdmissionDTO>>,
    ),
    AppError,
> {
    let application_scope = db.reopener();
    let request = CreateApprovalReviewRequest {
        client_request_id: body.client_request_id,
        retry_review_id: body.retry_review_id,
        definition_id: body.definition_id,
        definition_version: body.definition_version,
        agent_launch: body.agent_launch,
        phase: body.phase,
    };
    tracing::debug!(
        workspace_id = %workspace_id,
        definition_id = %body.definition_id,
        definition_version = body.definition_version,
        "Workspace approvals endpoint called: create_review"
    );

    let (db, result) = state
        .approval_review_service
        .admit(db, &state.system_pool, workspace_id, caller, request)
        .await?;
    let result = match db.commit().await {
        Ok(()) => result,
        Err(commit_error) => {
            tracing::warn!(
                workspace_id = %workspace_id,
                request_id = %result.request_id,
                review_id = %result.review_id,
                error = %commit_error,
                "Approval-review admission commit acknowledgement failed; reconciling durable receipt"
            );
            let mut confirmation_db = match application_scope.open().await {
                Ok(db) => db,
                Err(confirmation_error) => {
                    tracing::error!(
                        workspace_id = %workspace_id,
                        request_id = %result.request_id,
                        commit_error = %commit_error,
                        error = %confirmation_error,
                        "Could not open a fresh scope to reconcile approval-review admission"
                    );
                    return Err(AppError::from(g5_contracts::error::G5Error::Internal {
                        message: format!(
                            "Approval-review admission commit outcome is uncertain; retry request {}",
                            result.request_id
                        ),
                    }));
                }
            };
            let confirmation = state
                .approval_review_service
                .confirm_admission_after_commit_error(&mut confirmation_db, workspace_id, &result)
                .await;
            if let Err(rollback_error) = confirmation_db.rollback().await {
                tracing::warn!(
                    workspace_id = %workspace_id,
                    error = %rollback_error,
                    "Failed to close approval-review commit-reconciliation transaction"
                );
            }
            match confirmation {
                Ok(Some(verified)) => {
                    tracing::warn!(
                        workspace_id = %workspace_id,
                        request_id = %verified.request_id,
                        review_id = %verified.review_id,
                        "Recovered approval-review admission after ambiguous commit acknowledgement"
                    );
                    verified
                }
                Ok(None) => return Err(AppError::from(commit_error)),
                Err(error @ g5_contracts::error::G5Error::Conflict { .. }) => {
                    return Err(AppError::from(error));
                }
                Err(confirmation_error) => {
                    tracing::error!(
                        workspace_id = %workspace_id,
                        request_id = %result.request_id,
                        commit_error = %commit_error,
                        error = %confirmation_error,
                        "Approval-review admission commit outcome could not be reconciled safely"
                    );
                    return Err(AppError::from(g5_contracts::error::G5Error::Internal {
                        message: format!(
                            "Approval-review admission commit outcome is uncertain; retry request {}",
                            result.request_id
                        ),
                    }));
                }
            }
        }
    };
    tracing::info!(
        workspace_id = %workspace_id,
        request_id = %result.request_id,
        reused = result.reused,
        "Workspace approvals endpoint succeeded: create_review"
    );
    let status = if result.reused {
        StatusCode::OK
    } else {
        StatusCode::CREATED
    };
    Ok((
        status,
        Json(ResponseEnvelope::ok(ApprovalReviewAdmissionDTO {
            request_id: result.request_id.to_string(),
            review_id: result.review_id.to_string(),
            status: result.status,
            reused: result.reused,
        })),
    ))
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct ApprovalReviewEvidencePath {
    workspace_id: Uuid,
    review_id: Uuid,
}

async fn get_review_report(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(path): Path<ApprovalReviewEvidencePath>,
) -> Result<Json<ResponseEnvelope<serde_json::Value>>, AppError> {
    let mut db =
        scoped_db_for_workspace(&state, &auth, path.workspace_id, &["workspaces:read"]).await?;
    let report = state
        .approval_review_service
        .report(&mut db, path.workspace_id, path.review_id)
        .await?;
    db.commit().await?;
    Ok(Json(ResponseEnvelope::ok(report)))
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ApprovalReviewCommitQuery {
    repo_id: Uuid,
    #[serde(default)]
    side: crate::services::approval_review_service::inspection::CommitSide,
    #[serde(default)]
    operation: crate::services::approval_review_service::inspection::CommitOperation,
    path: Option<String>,
}

async fn get_review_commits(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(path): Path<ApprovalReviewEvidencePath>,
    Query(query): Query<ApprovalReviewCommitQuery>,
) -> Result<Json<ResponseEnvelope<serde_json::Value>>, AppError> {
    let mut db =
        scoped_db_for_workspace(&state, &auth, path.workspace_id, &["workspaces:read"]).await?;
    let result = state
        .approval_review_service
        .read_commits(
            &state,
            &mut db,
            path.workspace_id,
            path.review_id,
            auth.require_user_id()?,
            crate::services::approval_review_service::inspection::ReviewCommitQuery {
                repo_id: query.repo_id,
                side: query.side,
                operation: query.operation,
                path: query.path,
            },
        )
        .await?;
    db.rollback().await?;
    Ok(Json(ResponseEnvelope::ok(result)))
}

async fn get_review_transcript(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(path): Path<ApprovalReviewEvidencePath>,
    Query(query): Query<
        crate::services::approval_review_service::inspection::ReviewTranscriptQuery,
    >,
) -> Result<
    Json<
        ResponseEnvelope<
            crate::services::approval_review_service::inspection::ReviewTranscriptPage,
        >,
    >,
    AppError,
> {
    let mut db =
        scoped_db_for_workspace(&state, &auth, path.workspace_id, &["workspaces:read"]).await?;
    let result = state
        .approval_review_service
        .transcript(
            &state,
            &mut db,
            path.workspace_id,
            path.review_id,
            auth.require_user_id()?,
            query,
        )
        .await?;
    db.rollback().await?;
    Ok(Json(ResponseEnvelope::ok(result)))
}

async fn retry_review_cleanup(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(path): Path<ApprovalReviewEvidencePath>,
) -> Result<Json<ResponseEnvelope<serde_json::Value>>, AppError> {
    let mut db =
        scoped_db_for_workspace(&state, &auth, path.workspace_id, &["workspaces:write"]).await?;
    crate::services::workspace_mutation_policy::ensure_workspace_owner(&mut db, path.workspace_id)
        .await?;
    state
        .approval_review_service
        .retry_cleanup(&mut db, path.workspace_id, path.review_id)
        .await?;
    db.commit().await?;
    Ok(Json(ResponseEnvelope::ok(
        serde_json::json!({"cleanupPending":true}),
    )))
}
