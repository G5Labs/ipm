//! Merge when ready (`g5.git.pr-auto-merge`): a person's per-pull-request
//! decision that G5 merges the pull request as them once it is ready, stored
//! on the pull request with the background pass's progress
//! (`g5.git.pr-auto-merge.readiness`). Callers admit the actor first; every
//! write here touches an open pull request only.
use std::sync::Arc;

use chrono::{DateTime, Utc};
use g5_authz::ScopedDb;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::errors::G5Error;
use crate::services::AppState;

/// The decision as pull-request readers see it, after gh's `autoMergeRequest`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AutoMergeRequest {
    pub enabled_by: Uuid,
    pub enabled_at: DateTime<Utc>,
    /// G5 merges by rebase.
    pub merge_method: String,
    /// The latest reason it is still waiting.
    pub reason: Option<String>,
    pub checked_at: Option<DateTime<Utc>>,
}

/// An open pull request with merge when ready on, and the pass's progress.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ArmedPullRequest {
    pub id: Uuid,
    pub org_id: Uuid,
    pub source_workspace_id: Uuid,
    pub target_workspace_id: Uuid,
    pub enabled_by: Uuid,
    pub enabled_at: DateTime<Utc>,
    pub reason: Option<String>,
    pub observed_heads: Option<serde_json::Value>,
    pub observed_since: Option<DateTime<Utc>>,
    pub attempt_key: Option<String>,
    pub attempted_at: Option<DateTime<Utc>>,
}

/// What one pass learned about an armed pull request.
#[derive(Debug, Clone, Default)]
pub struct Progress {
    pub reason: Option<String>,
    pub observed_heads: Option<serde_json::Value>,
    pub observed_since: Option<DateTime<Utc>>,
    pub attempt_key: Option<String>,
    pub attempted_at: Option<DateTime<Utc>>,
}

/// The source owner changes this choice independently of enabling merge or
/// starting an agent. Hold ownership through commit, as transfer clears it.
pub async fn set_stop_vm_after_merge(
    db: &mut ScopedDb,
    target: Uuid,
    id: Uuid,
    actor: Uuid,
    selected: bool,
) -> Result<bool, G5Error> {
    let source = source_into(db, id, target)
        .await?
        .ok_or_else(|| G5Error::NotFound {
            resource: format!("workspace pull request {id}"),
        })?;
    crate::services::workspace_mutation_policy::ensure_workspace_owner_is(db, source, actor)
        .await?;
    let (status, current): (String, Option<bool>) = sqlx::query_as(
        "SELECT status, stop_vm_after_merge FROM workspace_pull_requests
         WHERE id=$1 AND target_workspace_id=$2 FOR UPDATE",
    )
    .bind(id)
    .bind(target)
    .fetch_one(db.conn())
    .await?;
    if status != "open" {
        return Err(G5Error::Conflict {
            reason: "VM shutdown settings can only change on an open pull request".into(),
        });
    }
    // An explicit choice also overrides a matching inherited default.
    if current == Some(selected) {
        return Ok(false);
    }
    sqlx::query(
        "UPDATE workspace_pull_requests SET stop_vm_after_merge=$2, updated_at=now()
         WHERE id=$1",
    )
    .bind(id)
    .bind(selected)
    .execute(db.conn())
    .await?;
    Ok(true)
}

/// The current source owner whose Stop choice survived a successful merge.
/// Transfer clears that choice; ordinary Stop must recheck ownership at admission.
pub async fn merged_stop_owner(
    db: &mut ScopedDb,
    pull_request_id: Uuid,
    source_workspace_id: Uuid,
) -> Result<Option<Uuid>, G5Error> {
    Ok(sqlx::query_scalar(
        "SELECT w.owner_user_id FROM workspace_pull_requests pr
         JOIN workspaces w ON w.id=pr.source_workspace_id
         WHERE pr.id=$1 AND pr.source_workspace_id=$2 AND pr.status='merged'
           AND pr.stop_vm_after_merge AND w.deleted_at IS NULL
           AND w.deletion_operation_id IS NULL AND w.owner_user_id IS NOT NULL",
    )
    .bind(pull_request_id)
    .bind(source_workspace_id)
    .fetch_optional(db.conn())
    .await?)
}

/// Turn merge when ready on for `actor`, who the caller admitted with merge
/// authority over `target_workspace_id`. Returns whether anything changed:
/// turning it on again by the same person is a no-op, and so is any decision
/// already on when `keep_existing` (starting another agent for it never
/// changes whom G5 merges as).
pub async fn enable(
    db: &mut ScopedDb,
    target_workspace_id: Uuid,
    pull_request_id: Uuid,
    actor: Uuid,
    keep_existing: bool,
) -> Result<bool, G5Error> {
    let (org_id, status, enabled_by, _) =
        locked(db, pull_request_id, Some(Side::Target(target_workspace_id))).await?;
    if status != "open" {
        return Err(G5Error::Conflict {
            reason: format!(
                "Workspace pull request is {status}; merge when ready applies to open pull requests"
            ),
        });
    }
    if enabled_by == Some(actor) || (keep_existing && enabled_by.is_some()) {
        return Ok(false);
    }
    sqlx::query(
        "UPDATE workspace_pull_requests
            SET auto_merge_enabled_by = $2, auto_merge_enabled_at = now(),
                auto_merge_reason = NULL, auto_merge_checked_at = NULL,
                auto_merge_observed_heads = NULL, auto_merge_observed_since = NULL,
                auto_merge_attempt_key = NULL, auto_merge_attempted_at = NULL
          WHERE id = $1",
    )
    .bind(pull_request_id)
    .bind(actor)
    .execute(db.conn())
    .await?;
    insert_event(
        db,
        org_id,
        pull_request_id,
        "auto_merge_enabled",
        actor,
        None,
    )
    .await?;
    Ok(true)
}

/// The workspace a caller was admitted on, which the pull request must have
/// on that side.
#[derive(Debug, Clone, Copy)]
pub enum Side {
    Target(Uuid),
    Source(Uuid),
}

/// Turn merge when ready off, recording `actor` and an optional reason in
/// Activity. Returns the person who had turned it on, or `None` when it was
/// already off or the pull request is closed (a no-op).
pub async fn disable(
    db: &mut ScopedDb,
    side: Side,
    pull_request_id: Uuid,
    actor: Uuid,
    message: Option<String>,
) -> Result<Option<Uuid>, G5Error> {
    let (org_id, status, enabled_by, _) = locked(db, pull_request_id, Some(side)).await?;
    let Some(enabled_by) = enabled_by.filter(|_| status == "open") else {
        return Ok(None);
    };
    clear(db, pull_request_id).await?;
    insert_event(
        db,
        org_id,
        pull_request_id,
        "auto_merge_disabled",
        actor,
        message,
    )
    .await?;
    Ok(Some(enabled_by))
}

/// Turn off the decision the pass evaluated, unless it changed meanwhile.
/// The event names its author, whose authority lapsed.
pub async fn disable_decision(
    db: &mut ScopedDb,
    armed: &ArmedPullRequest,
    message: String,
) -> Result<bool, G5Error> {
    let (_, status, enabled_by, enabled_at) = locked(db, armed.id, None).await?;
    if status != "open"
        || enabled_by != Some(armed.enabled_by)
        || enabled_at != Some(armed.enabled_at)
    {
        return Ok(false);
    }
    clear(db, armed.id).await?;
    insert_event(
        db,
        armed.org_id,
        armed.id,
        "auto_merge_disabled",
        armed.enabled_by,
        Some(message),
    )
    .await?;
    Ok(true)
}

/// Record the pass's progress for the decision it evaluated; a decision
/// turned off or replaced meanwhile keeps its own state.
pub async fn record_progress(
    db: &mut ScopedDb,
    armed: &ArmedPullRequest,
    progress: &Progress,
) -> Result<(), G5Error> {
    sqlx::query(
        "UPDATE workspace_pull_requests
            SET auto_merge_reason = $4, auto_merge_checked_at = now(),
                auto_merge_observed_heads = $5, auto_merge_observed_since = $6,
                auto_merge_attempt_key = $7, auto_merge_attempted_at = $8
          WHERE id = $1 AND status = 'open'
            AND auto_merge_enabled_by = $2 AND auto_merge_enabled_at = $3",
    )
    .bind(armed.id)
    .bind(armed.enabled_by)
    .bind(armed.enabled_at)
    .bind(&progress.reason)
    .bind(&progress.observed_heads)
    .bind(progress.observed_since)
    .bind(&progress.attempt_key)
    .bind(progress.attempted_at)
    .execute(db.conn())
    .await?;
    Ok(())
}

/// Whether the decision the pass evaluated is still the pull request's, so a
/// turn-off that arrived meanwhile stops the merge before it starts.
pub async fn still_on(db: &mut ScopedDb, armed: &ArmedPullRequest) -> Result<bool, G5Error> {
    Ok(sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM workspace_pull_requests
                        WHERE id = $1 AND status = 'open'
                          AND auto_merge_enabled_by = $2 AND auto_merge_enabled_at = $3)",
    )
    .bind(armed.id)
    .bind(armed.enabled_by)
    .bind(armed.enabled_at)
    .fetch_one(db.conn())
    .await?)
}

/// Open pull requests of the scope's organization with merge when ready on.
pub async fn armed(db: &mut ScopedDb) -> Result<Vec<ArmedPullRequest>, G5Error> {
    Ok(sqlx::query_as::<_, ArmedPullRequest>(
        "SELECT id, org_id, source_workspace_id, target_workspace_id,
                auto_merge_enabled_by AS enabled_by, auto_merge_enabled_at AS enabled_at,
                auto_merge_reason AS reason,
                auto_merge_observed_heads AS observed_heads,
                auto_merge_observed_since AS observed_since,
                auto_merge_attempt_key AS attempt_key,
                auto_merge_attempted_at AS attempted_at
           FROM workspace_pull_requests
          WHERE status = 'open' AND auto_merge_enabled_by IS NOT NULL
          ORDER BY id",
    )
    .fetch_all(db.conn())
    .await?)
}

/// The source workspace of the pull request `id` into `target`, if there is
/// one the scope can read.
pub async fn source_into(
    db: &mut ScopedDb,
    id: Uuid,
    target: Uuid,
) -> Result<Option<Uuid>, G5Error> {
    Ok(sqlx::query_scalar::<_, Uuid>(
        "SELECT source_workspace_id FROM workspace_pull_requests
          WHERE id = $1 AND target_workspace_id = $2",
    )
    .bind(id)
    .bind(target)
    .fetch_optional(db.conn())
    .await?)
}

/// What a merge announcement names: both workspaces and the source's owner.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct AnnouncedPullRequest {
    pub source_workspace_id: Uuid,
    pub target_workspace_id: Uuid,
    pub source_name: String,
    pub target_name: String,
    pub source_owner: Option<Uuid>,
    pub target_owner: Option<Uuid>,
    /// The recorded merge admission, for the override a merge landed under.
    pub merge_admission: Option<sqlx::types::Json<serde_json::Value>>,
}

impl AnnouncedPullRequest {
    /// The override the merged pull request landed under, if any
    /// (g5.workspace.merge.admin-override.formal).
    pub fn merge_override(
        &self,
    ) -> Option<crate::services::git::merge_override::MergeOverrideRecord> {
        let admission = self.merge_admission.as_ref()?;
        serde_json::from_value::<
            crate::services::git::workspace_merge_service::MergeAdmissionEvidence,
        >(admission.0.clone())
        .ok()?
        .merge_override
    }
}

/// The people whose review requests on `id` an override merge bypassed.
pub async fn bypassed_reviewers(db: &mut ScopedDb, id: Uuid) -> Result<Vec<Uuid>, G5Error> {
    Ok(sqlx::query_scalar(
        "SELECT DISTINCT actor_user_id FROM approval_requests
          WHERE (pull_request_id = $1
                 OR (without_pull_request
                     AND source_workspace_id = (SELECT source_workspace_id
                                                  FROM workspace_pull_requests WHERE id = $1)
                     -- closed by this PR's override merge, in its transaction
                     AND closed_at = (SELECT MAX(created_at) FROM workspace_pull_request_events
                                       WHERE pull_request_id = $1 AND event_type = 'merged')))
            AND state = 'bypassed' AND actor_type = 'human'
            AND actor_user_id IS NOT NULL",
    )
    .bind(id)
    .fetch_all(db.conn())
    .await?)
}

/// [`AnnouncedPullRequest`] for `id`, if the scope can read it.
pub async fn announced(
    db: &mut ScopedDb,
    id: Uuid,
) -> Result<Option<AnnouncedPullRequest>, G5Error> {
    Ok(sqlx::query_as::<_, AnnouncedPullRequest>(
        "SELECT pr.source_workspace_id, pr.target_workspace_id, sw.name AS source_name,
                tw.name AS target_name, sw.owner_user_id AS source_owner,
                tw.owner_user_id AS target_owner, pr.merge_admission
           FROM workspace_pull_requests pr
           JOIN workspaces sw ON sw.id = pr.source_workspace_id
           JOIN workspaces tw ON tw.id = pr.target_workspace_id
          WHERE pr.id = $1",
    )
    .bind(id)
    .fetch_optional(db.conn())
    .await?)
}

/// Tell both workspaces' readers that the pull request changed.
pub fn publish_updated(state: &Arc<AppState>, source: Uuid, target: Uuid, id: Uuid) {
    for workspace_id in [source, target] {
        state.event_bus.publish(
            &crate::ws::Channel::Workspace(workspace_id),
            &crate::ws::events::WsEvent::WorkspacePullRequestUpdated {
                workspace_id,
                pull_request_id: id,
            },
        );
    }
}

type LockedDecision = (Uuid, String, Option<Uuid>, Option<DateTime<Utc>>);

/// The pull request's organization, status and decision, locked.
async fn locked(
    db: &mut ScopedDb,
    pull_request_id: Uuid,
    side: Option<Side>,
) -> Result<LockedDecision, G5Error> {
    let (target, source) = match side {
        Some(Side::Target(id)) => (Some(id), None),
        Some(Side::Source(id)) => (None, Some(id)),
        None => (None, None),
    };
    sqlx::query_as::<_, LockedDecision>(
        "SELECT org_id, status, auto_merge_enabled_by, auto_merge_enabled_at
           FROM workspace_pull_requests
          WHERE id = $1
            AND ($2::uuid IS NULL OR target_workspace_id = $2)
            AND ($3::uuid IS NULL OR source_workspace_id = $3)
          FOR UPDATE",
    )
    .bind(pull_request_id)
    .bind(target)
    .bind(source)
    .fetch_optional(db.conn())
    .await?
    .ok_or_else(|| G5Error::NotFound {
        resource: format!("workspace pull request {pull_request_id}"),
    })
}

async fn clear(db: &mut ScopedDb, pull_request_id: Uuid) -> Result<(), G5Error> {
    sqlx::query(
        "UPDATE workspace_pull_requests
            SET auto_merge_enabled_by = NULL, auto_merge_enabled_at = NULL,
                auto_merge_reason = NULL, auto_merge_checked_at = NULL,
                auto_merge_observed_heads = NULL, auto_merge_observed_since = NULL,
                auto_merge_attempt_key = NULL, auto_merge_attempted_at = NULL
          WHERE id = $1",
    )
    .bind(pull_request_id)
    .execute(db.conn())
    .await?;
    Ok(())
}

async fn insert_event(
    db: &mut ScopedDb,
    org_id: Uuid,
    pull_request_id: Uuid,
    event_type: &str,
    actor: Uuid,
    message: Option<String>,
) -> Result<(), G5Error> {
    sqlx::query(
        "INSERT INTO workspace_pull_request_events
             (org_id, pull_request_id, event_type, actor_id, message)
         VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(org_id)
    .bind(pull_request_id)
    .bind(event_type)
    .bind(actor)
    .bind(message)
    .execute(db.conn())
    .await?;
    Ok(())
}
