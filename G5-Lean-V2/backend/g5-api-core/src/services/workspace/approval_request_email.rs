//! The reviewer-notification email for a requested approval
//! (`g5.governance.approval.request-email`) and the owner-notification email
//! for a human decision on one (`g5.governance.approval.decision-email`), both
//! requested through the shared email module.
//!
//! Approval requests are written under tenant authority, but the email queue is
//! system-owned, so the two cannot share a transaction (`g5.email.handoff`).
//! The request transaction commits first; this consumer then queues one email
//! per *newly admitted* request on a system-pool transaction. The purpose key
//! names the request (with its organization and reviewer), so a replay
//! coalesces instead of sending twice, and the requester is never emailed for
//! their own request. Email is best-effort: a
//! disabled deployment, a volume limit or any queue failure leaves the already
//! committed request untouched, with the Home attention list as the durable
//! signal. A decision email follows the same post-commit, best-effort handoff
//! after the decision commits, keyed on the request it settled.

use g5_authz::ScopedDb;
use g5_contracts::error::G5Error;
use sqlx::{Connection, PgConnection, PgPool};
use uuid::Uuid;

use super::approval_config_service::HumanApprovalRequestAdmission;
use super::workspace_owner_email::organization_repository_scopes;
use crate::services::email::{
    link_path, ApprovalDecisionEmail, ApprovalDecisionOutcome, ApprovalRequestEmail, DeliveryGate,
    Email, EmailRequest, Message, RequestOutcome, Sender, Validity,
};

/// `G5 Approvals <approvals@{sending domain}>` (`g5.email.sender`).
const APPROVALS_SENDER: Sender = Sender::new("approvals").with_display_name("G5 Approvals");

const LOG_TARGET: &str = "g5_api::approval_request_email";

#[derive(sqlx::FromRow)]
struct RequestEmailContext {
    workspace_name: String,
    org_name: String,
    org_slug: String,
    definition_name: String,
    requester_name: String,
    recipient_email: String,
    setting_on: bool,
    state: String,
    note: Option<String>,
}

/// Queue a reviewer-notification email for each newly admitted request in
/// `admissions`, on `db`, a system-pool transaction scoped to `org_id` that the
/// caller commits. A reused admission, the requester's own request, a recipient
/// whose account preference is off and a request that is no longer pending are
/// all skipped. The committed approval request is never undone by an email
/// failure: a disabled deployment or a volume limit is logged and skipped.
/// `pull_request_description` is the canonical description of the requests'
/// PR (one create call shares one PR), quoted in each email when present.
pub async fn request_approval_request_emails(
    email: &Email,
    db: &mut ScopedDb,
    org_id: Uuid,
    source_workspace_id: Uuid,
    requested_by: Uuid,
    admissions: &[HumanApprovalRequestAdmission],
    pull_request_description: Option<&str>,
) -> Result<usize, G5Error> {
    if !email.is_enabled() {
        return Ok(0);
    }
    let link_scopes = match pull_request_description {
        Some(_) => description_link_scopes(db, org_id).await,
        None => Vec::new(),
    };
    let mut queued = 0;
    for admission in admissions {
        if admission.reused || admission.actor_user_id == requested_by {
            continue;
        }
        // Each admission runs in its own savepoint on the shared transaction, so
        // one failed context read or queue (for example a poisoning SQL error)
        // rolls back only that admission and never discards the emails already
        // queued for the rest of the batch
        // (`g5.governance.approval.request-email`, best-effort).
        let mut savepoint = db.conn().begin().await.map_err(|error| G5Error::Internal {
            message: format!("Failed to open approval email savepoint: {error}"),
        })?;
        match queue_one_admission(
            email,
            &mut savepoint,
            org_id,
            source_workspace_id,
            admission,
            pull_request_description,
            &link_scopes,
        )
        .await
        {
            Ok(sent) => {
                savepoint
                    .commit()
                    .await
                    .map_err(|error| G5Error::Internal {
                        message: format!("Failed to release approval email savepoint: {error}"),
                    })?;
                if sent {
                    queued += 1;
                }
            }
            // Best-effort: a volume limit, invalid recipient or query error never
            // undoes the committed request; roll the savepoint back and continue.
            Err(error) => {
                let _ = savepoint.rollback().await;
                tracing::warn!(
                    target: LOG_TARGET,
                    request_id = %admission.request_id,
                    %error,
                    "Approval request email was not queued"
                );
            }
        }
    }
    Ok(queued)
}

/// The organization's repository web roots, which the description's links may
/// lead into (`g5.email.link-policy`). Read in its own savepoint and
/// best-effort: on failure no repository link is clickable, and the emails are
/// still queued.
async fn description_link_scopes(db: &mut ScopedDb, org_id: Uuid) -> Vec<String> {
    let result = async {
        let mut savepoint = db.conn().begin().await.map_err(|error| G5Error::Internal {
            message: format!("Failed to open approval email savepoint: {error}"),
        })?;
        match organization_repository_scopes(&mut savepoint, org_id).await {
            Ok(scopes) => {
                savepoint
                    .commit()
                    .await
                    .map_err(|error| G5Error::Internal {
                        message: format!("Failed to release approval email savepoint: {error}"),
                    })?;
                Ok(scopes)
            }
            Err(error) => {
                let _ = savepoint.rollback().await;
                Err(error)
            }
        }
    }
    .await;
    result.unwrap_or_else(|error: G5Error| {
        tracing::warn!(
            target: LOG_TARGET,
            %error,
            "Approval request email links were not resolved"
        );
        Vec::new()
    })
}

/// Read one request's context and queue its reviewer email on `conn` (a
/// savepoint). Returns whether a new email was queued; a reused/coalesced
/// request, a disabled deployment, a request no longer pending and a recipient
/// whose preference is off all return `Ok(false)`. Any error leaves the
/// savepoint to be rolled back by the caller.
async fn queue_one_admission(
    email: &Email,
    conn: &mut PgConnection,
    org_id: Uuid,
    source_workspace_id: Uuid,
    admission: &HumanApprovalRequestAdmission,
    pull_request_description: Option<&str>,
    link_scopes: &[String],
) -> Result<bool, G5Error> {
    let context: Option<RequestEmailContext> = sqlx::query_as(
        r#"
        SELECT w.name AS workspace_name,
               o.name AS org_name,
               o.slug AS org_slug,
               d.name AS definition_name,
               requester.display_name AS requester_name,
               reviewer.email AS recipient_email,
               reviewer.email_on_approval_request AS setting_on,
               r.state AS state,
               r.note AS note
          FROM approval_requests r
          JOIN workspaces w ON w.id = r.source_workspace_id AND w.org_id = r.org_id
          JOIN orgs o ON o.id = r.org_id
          JOIN approval_definitions d
            ON d.org_id = r.org_id AND d.id = r.approval_definition_id
           AND d.version = r.approval_definition_version
          JOIN users requester ON requester.id = r.requested_by
          JOIN users reviewer ON reviewer.id = r.actor_user_id
         WHERE r.org_id = $1 AND r.id = $2 AND r.source_workspace_id = $3
           AND r.actor_user_id = $4
        "#,
    )
    .bind(org_id)
    .bind(admission.request_id)
    .bind(source_workspace_id)
    .bind(admission.actor_user_id)
    .fetch_optional(&mut *conn)
    .await
    .map_err(|error| G5Error::Internal {
        message: format!("Failed to load approval request email context: {error}"),
    })?;
    let Some(context) = context else {
        return Ok(false);
    };
    if context.state != "pending" || !context.setting_on {
        return Ok(false);
    }

    let outcome = email
        .request(
            &mut *conn,
            EmailRequest {
                org_id,
                sender: APPROVALS_SENDER,
                recipient: context.recipient_email,
                message: Message::ApprovalRequest(ApprovalRequestEmail {
                    request_id: admission.request_id,
                    org_id,
                    reviewer_user_id: admission.actor_user_id,
                    link_path: link_path(&[&context.org_slug, &context.workspace_name]),
                    settings_link_path: link_path(&[&context.org_slug, "settings"]),
                    note: context.note,
                    pull_request_description: pull_request_description.map(str::to_owned),
                    link_scopes: link_scopes.to_vec(),
                    workspace_name: context.workspace_name,
                    org_name: context.org_name,
                    definition_name: context.definition_name,
                    requester_name: context.requester_name,
                }),
            },
        )
        .await?;
    Ok(matches!(outcome, RequestOutcome::Queued(_)))
}

/// The PR the newly admitted requests in `admissions` are for, read on `db`
/// under the requester's scope; `None` when none is new or the requests carry
/// no PR. One create call admits every request against the same PR.
pub async fn requested_pull_request(
    db: &mut ScopedDb,
    admissions: &[HumanApprovalRequestAdmission],
) -> Result<Option<Uuid>, G5Error> {
    let Some(admission) = admissions.iter().find(|admission| !admission.reused) else {
        return Ok(None);
    };
    let pull_request_id: Option<Option<Uuid>> =
        sqlx::query_scalar("SELECT pull_request_id FROM approval_requests WHERE id = $1")
            .bind(admission.request_id)
            .fetch_optional(db.conn())
            .await
            .map_err(|error| G5Error::Internal {
                message: format!("Failed to load the approval request's pull request: {error}"),
            })?;
    Ok(pull_request_id.flatten())
}

#[derive(sqlx::FromRow)]
struct DecisionEmailContext {
    workspace_name: String,
    org_name: String,
    org_slug: String,
    definition_name: String,
    reviewer_id: Uuid,
    reviewer_name: String,
    owner_id: Uuid,
    owner_email: String,
    setting_on: bool,
    outcome: String,
    comment: Option<String>,
}

/// Queue the owner-notification email for the human decision that settled
/// `request_id` in `source_workspace_id`, on `db`, a system-pool transaction
/// scoped to `org_id` that the caller commits
/// (`g5.governance.approval.decision-email`). The decision is read back here,
/// so a caller passes only the request it settled. A request that is not
/// settled by its reviewer's approval or request for changes, a decision by
/// the workspace's own owner and an owner whose preference is off are all
/// skipped. The key is single-use, so a replayed decision returns the email
/// already queued or sent instead of sending again. Returns whether a new
/// email was queued; the caller treats any error as best-effort.
pub async fn request_approval_decision_email(
    email: &Email,
    db: &mut ScopedDb,
    org_id: Uuid,
    source_workspace_id: Uuid,
    request_id: Uuid,
) -> Result<bool, G5Error> {
    if !email.is_enabled() {
        return Ok(false);
    }
    // The settling decision is the reviewer's own row on the request; a request
    // settles once, so there is at most one.
    let context: Option<DecisionEmailContext> = sqlx::query_as(
        r#"
        SELECT w.name AS workspace_name,
               o.name AS org_name,
               o.slug AS org_slug,
               d.name AS definition_name,
               reviewer.id AS reviewer_id,
               reviewer.display_name AS reviewer_name,
               owner.id AS owner_id,
               owner.email AS owner_email,
               owner.email_on_approval_request AS setting_on,
               s.outcome AS outcome,
               s.comment AS comment
          FROM approval_requests r
          JOIN approval_statuses s
            ON s.org_id = r.org_id AND s.request_id = r.id
           AND s.actor_type = 'human' AND s.actor_user_id = r.actor_user_id
          JOIN workspaces w ON w.id = r.source_workspace_id AND w.org_id = r.org_id
          JOIN orgs o ON o.id = r.org_id
          JOIN approval_definitions d
            ON d.org_id = r.org_id AND d.id = r.approval_definition_id
           AND d.version = r.approval_definition_version
          JOIN users reviewer ON reviewer.id = r.actor_user_id
          JOIN users owner ON owner.id = w.owner_user_id
         WHERE r.org_id = $1 AND r.id = $2 AND r.source_workspace_id = $3
           AND r.actor_type = 'human' AND r.state = 'settled'
         ORDER BY s.seq DESC
         LIMIT 1
        "#,
    )
    .bind(org_id)
    .bind(request_id)
    .bind(source_workspace_id)
    .fetch_optional(db.conn())
    .await
    .map_err(|error| G5Error::Internal {
        message: format!("Failed to load approval decision email context: {error}"),
    })?;
    let Some(context) = context else {
        return Ok(false);
    };
    let Some(outcome) = ApprovalDecisionOutcome::from_recorded(&context.outcome) else {
        return Ok(false);
    };
    if context.owner_id == context.reviewer_id || !context.setting_on {
        return Ok(false);
    }
    let outcome = email
        .request(
            db.conn(),
            EmailRequest {
                org_id,
                sender: APPROVALS_SENDER,
                recipient: context.owner_email,
                message: Message::ApprovalDecision(ApprovalDecisionEmail {
                    request_id,
                    org_id,
                    owner_user_id: context.owner_id,
                    link_path: link_path(&[&context.org_slug, &context.workspace_name]),
                    settings_link_path: link_path(&[&context.org_slug, "settings"]),
                    workspace_name: context.workspace_name,
                    org_name: context.org_name,
                    definition_name: context.definition_name,
                    reviewer_name: context.reviewer_name,
                    outcome,
                    comment: context.comment,
                }),
            },
        )
        .await?;
    Ok(matches!(outcome, RequestOutcome::Queued(_)))
}

/// A queued approval-request or approval-decision email is wanted only while
/// its recipient is still an active member whose account email is unchanged
/// and whose account preference is still on
/// (`g5.governance.approval.request-email`,
/// `g5.governance.approval.decision-email`). Both kinds key on
/// `<request id>:<org id>:<recipient user id>`. The gate deliberately never
/// reads the request or the workspace: `users` and `org_memberships` carry no
/// RLS, so the system worker needs no authority over tenant approval records,
/// at the cost of possibly emailing a request settled, or a workspace handed
/// to a new owner, moments before delivery.
pub struct ApprovalEmailGate;

#[async_trait::async_trait]
impl DeliveryGate for ApprovalEmailGate {
    async fn still_wanted(
        &self,
        pool: &PgPool,
        purpose_key: &str,
        recipient: &str,
    ) -> Result<Validity, sqlx::Error> {
        // `<request id>:<org id>:<recipient user id>`, written by the consumer.
        let mut parts = purpose_key.split(':').map(Uuid::parse_str);
        let (Some(Ok(_request_id)), Some(Ok(org_id)), Some(Ok(recipient_user_id)), None) =
            (parts.next(), parts.next(), parts.next(), parts.next())
        else {
            return Ok(Validity::Cancelled("invalid_purpose"));
        };
        let recipient_user: Option<(String, bool, bool)> = sqlx::query_as(
            "SELECT u.email, u.email_on_approval_request,
                    EXISTS(SELECT 1 FROM org_memberships m
                            WHERE m.org_id = $1 AND m.user_id = $2
                              AND m.deactivated_at IS NULL)
               FROM users u WHERE u.id = $2",
        )
        .bind(org_id)
        .bind(recipient_user_id)
        .fetch_optional(pool)
        .await?;
        Ok(match recipient_user {
            None => Validity::Cancelled("recipient_gone"),
            Some((_, setting_on, _)) if !setting_on => Validity::Cancelled("setting_off"),
            Some((email, _, _)) if email.trim() != recipient => {
                Validity::Cancelled("recipient_changed")
            }
            Some((_, _, active)) if !active => Validity::Cancelled("ineligible"),
            Some(_) => Validity::Wanted,
        })
    }
}
