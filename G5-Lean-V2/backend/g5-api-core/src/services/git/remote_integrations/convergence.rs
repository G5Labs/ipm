//! Remote policy and synchronization effects applied with a complete Git cut.
use super::SyncWatermarkUpdate;
use crate::errors::AppError;
use crate::services::advance_consumer::{
    consumer_receipt_exists, load_consumer_input, record_consumer_receipt, register_consumer_input,
};
use crate::services::branch_commit::{BranchCommitMember, PreparedBranchCommit, ZERO_GIT_SHA};
use crate::services::git::repo_store::ScopedRepoStorage;
use g5_contracts::types::AuthorityTarget;
use serde::{Deserialize, Serialize};
use sqlx::PgConnection;
use std::collections::BTreeSet;
use uuid::Uuid;

pub(crate) const INITIAL_REMOTE_TRACKING_CONSUMER: &str = "initial_remote_tracking";
pub(crate) const REMOTE_TRACKING_CONVERGENCE_CONSUMER: &str = "remote_tracking_convergence";

fn is_git_sha(value: &str) -> bool {
    value.len() == 40
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InitialRemoteTrackingInput {
    pub workspace_id: Uuid,
    pub repo_id: Uuid,
    pub remote_branch: String,
    pub baseline_commit: String,
    pub remote_tip_advisory_sha: String,
    pub acting_user_id: Uuid,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RemoteTrackingConvergenceStatus {
    Imported,
    Noop,
}

impl RemoteTrackingConvergenceStatus {
    fn as_str(self) -> &'static str {
        match self {
            Self::Imported => crate::models::branch_sync_run::status::IMPORTED,
            Self::Noop => crate::models::branch_sync_run::status::NOOP,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreparedRemoteTrackingConvergence {
    pub workspace_id: Uuid,
    pub repo_id: Uuid,
    pub remote_branch: String,
    pub expected_tracking_mode: String,
    pub expected_remote_branch: Option<String>,
    pub expected_old_head: String,
    pub proposed_new_head: String,
    pub expected_baseline_commit: Option<String>,
    pub expected_git_url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_sync_paused_reason: Option<String>,
    pub acting_user_id: Uuid,
    pub trigger_kind: String,
    pub status: RemoteTrackingConvergenceStatus,
}

/// Validate initialization evidence against the exact prepared members.
pub fn validate_initial_remote_tracking_inputs(
    members: &[BranchCommitMember],
    inputs: &[InitialRemoteTrackingInput],
) -> Result<(), sqlx::Error> {
    if inputs.is_empty() {
        return Ok(());
    }
    let unique_members = inputs
        .iter()
        .map(|input| input.repo_id)
        .collect::<BTreeSet<_>>();
    if unique_members.len() != inputs.len()
        || inputs.iter().any(|input| {
            input.remote_branch.trim().is_empty()
                || !is_git_sha(&input.baseline_commit)
                || !is_git_sha(&input.remote_tip_advisory_sha)
        })
    {
        return Err(protocol_error(
            "Remote-tracking initialization must name each repository member once with exact SHAs",
        ));
    }
    for input in inputs {
        let member = members
            .iter()
            .find(|member| {
                member.workspace_id == input.workspace_id && member.repo_id == input.repo_id
            })
            .ok_or_else(|| {
                protocol_error(
                    "Remote-tracking initialization names a member outside the prepared cut",
                )
            })?;
        if member.expected_old_head != ZERO_GIT_SHA {
            return Err(protocol_error(
                "Remote-tracking initialization requires a typed initialize transition",
            ));
        }
    }
    Ok(())
}

/// Register remote-tracking initialization inputs against the cut being
/// landed; the landing applies them in its transaction.
pub async fn register_initial_remote_tracking_consumer(
    conn: &mut PgConnection,
    cut: &PreparedBranchCommit,
    inputs: &[InitialRemoteTrackingInput],
) -> Result<(), sqlx::Error> {
    if inputs.is_empty() {
        return Ok(());
    }
    if !matches!(cut.target(), AuthorityTarget::Workspace { .. }) {
        return Err(protocol_error(
            "Remote-tracking initialization requires a workspace cut",
        ));
    }
    validate_initial_remote_tracking_inputs(cut.members(), inputs)?;
    register_consumer_input(
        conn,
        cut.advance_id(),
        INITIAL_REMOTE_TRACKING_CONSUMER,
        &inputs,
    )
    .await
}

/// Register remote-tracking convergence inputs against the cut being landed;
/// the landing applies them in its transaction.
pub(crate) async fn register_remote_tracking_convergence_consumer(
    conn: &mut PgConnection,
    cut: &PreparedBranchCommit,
    inputs: &[PreparedRemoteTrackingConvergence],
) -> Result<(), sqlx::Error> {
    if inputs.is_empty() {
        return Ok(());
    }
    if !matches!(cut.target(), AuthorityTarget::Workspace { .. }) {
        return Err(protocol_error(
            "Remote-tracking convergence requires a workspace cut",
        ));
    }
    validate_remote_tracking_convergence_inputs(cut.members(), inputs)?;
    register_consumer_input(
        conn,
        cut.advance_id(),
        REMOTE_TRACKING_CONVERGENCE_CONSUMER,
        &inputs,
    )
    .await
}

/// A provider import already synchronizes these members in the landing
/// transaction. Its generic commit signal needs no additional remote cycle.
/// Require the receipt as well as the exact member input: preparation alone
/// is not evidence that synchronization was applied.
pub(super) async fn member_synchronized_during_landing(
    conn: &mut PgConnection,
    advance_id: Uuid,
    workspace_id: Uuid,
    repo_id: Uuid,
) -> Result<bool, sqlx::Error> {
    if !consumer_receipt_exists(conn, advance_id, REMOTE_TRACKING_CONVERGENCE_CONSUMER).await? {
        return Ok(false);
    }
    let inputs = load_consumer_input::<Vec<PreparedRemoteTrackingConvergence>>(
        conn,
        advance_id,
        REMOTE_TRACKING_CONVERGENCE_CONSUMER,
    )
    .await?;
    Ok(inputs.is_some_and(|inputs| {
        inputs
            .iter()
            .any(|input| input.workspace_id == workspace_id && input.repo_id == repo_id)
    }))
}

/// Apply every consumer input registered against a landing, inside its
/// transaction, after the member heads were refreshed. Each consumer records
/// a receipt so a replay is a no-op.
pub(crate) async fn apply_registered_landing_consumers(
    conn: &mut PgConnection,
    advance_id: Uuid,
    target: &AuthorityTarget,
    members: &[BranchCommitMember],
    storage: &ScopedRepoStorage,
    org_id: Uuid,
) -> Result<Vec<g5_scm::RefCommit>, AppError> {
    let mut watermarks = Vec::new();
    if !consumer_receipt_exists(conn, advance_id, INITIAL_REMOTE_TRACKING_CONSUMER).await? {
        if let Some(inputs) = load_consumer_input::<Vec<InitialRemoteTrackingInput>>(
            conn,
            advance_id,
            INITIAL_REMOTE_TRACKING_CONSUMER,
        )
        .await?
        {
            watermarks.extend(inputs.iter().map(|input| SyncWatermarkUpdate {
                workspace_id: input.workspace_id,
                repo_id: input.repo_id,
                expected: None,
                new: (input.baseline_commit != ZERO_GIT_SHA).then(|| input.baseline_commit.clone()),
            }));
            apply_initial_remote_tracking_during_initialization(conn, members, &inputs).await?;
            record_consumer_receipt::<serde_json::Value>(
                conn,
                advance_id,
                INITIAL_REMOTE_TRACKING_CONSUMER,
                None,
            )
            .await?;
        }
    }
    if !consumer_receipt_exists(conn, advance_id, REMOTE_TRACKING_CONVERGENCE_CONSUMER).await? {
        if let Some(inputs) = load_consumer_input::<Vec<PreparedRemoteTrackingConvergence>>(
            conn,
            advance_id,
            REMOTE_TRACKING_CONVERGENCE_CONSUMER,
        )
        .await?
        {
            watermarks.extend(inputs.iter().map(|input| {
                SyncWatermarkUpdate {
                    workspace_id: input.workspace_id,
                    repo_id: input.repo_id,
                    expected: input
                        .expected_baseline_commit
                        .clone()
                        .filter(|oid| oid != ZERO_GIT_SHA),
                    new: Some(input.proposed_new_head.clone()),
                }
            }));
            apply_remote_tracking_convergence_metadata(conn, members, &inputs).await?;
            record_consumer_receipt::<serde_json::Value>(
                conn,
                advance_id,
                REMOTE_TRACKING_CONVERGENCE_CONSUMER,
                None,
            )
            .await?;
        }
    }
    if !consumer_receipt_exists(
        conn,
        advance_id,
        crate::services::remote_tracking_merge::REMOTE_TRACKING_MERGE_COMPLETION_CONSUMER,
    )
    .await?
    {
        if let Some(input) = load_consumer_input::<
            crate::services::remote_tracking_merge::RemoteTrackingMergeCompletionConsumerInput,
        >(
            conn,
            advance_id,
            crate::services::remote_tracking_merge::REMOTE_TRACKING_MERGE_COMPLETION_CONSUMER,
        )
        .await?
        {
            watermarks.push(SyncWatermarkUpdate {
                workspace_id: input.workspace_id,
                repo_id: input.repo_id,
                expected: input.expected_watermark,
                new: Some(input.synchronized_remote_tip),
            });
        }
    }
    crate::services::remote_tracking_merge::apply_remote_tracking_merge_completion_consumer(
        conn, advance_id, target, members,
    )
    .await?;
    super::watermark_effects(storage, org_id, members, &watermarks).map_err(AppError::from)
}

/// Bind initialized members to their remote branch in the landing transaction.
pub(crate) async fn apply_initial_remote_tracking_during_initialization(
    conn: &mut PgConnection,
    members: &[BranchCommitMember],
    inputs: &[InitialRemoteTrackingInput],
) -> Result<(), sqlx::Error> {
    for input in inputs {
        let member = members
            .iter()
            .find(|member| {
                member.workspace_id == input.workspace_id && member.repo_id == input.repo_id
            })
            .ok_or_else(|| {
                protocol_error("Remote-tracking initialization member changed after preparation")
            })?;
        let updated = sqlx::query(&format!(
            r#"
            INSERT INTO remote_integrations (org_id, workspace_id, repo_id, remote_branch, last_sync_at, auto_pull_enabled)
            SELECT org_id, workspace_id, repo_id, $3, now(), {}
            FROM workspace_repos WHERE workspace_id = $1 AND repo_id = $2
            ON CONFLICT (workspace_id, repo_id) DO NOTHING
            "#,
            super::policy::initial_auto_pull_sql("$1")
        ))
        .bind(input.workspace_id)
        .bind(input.repo_id)
        .bind(input.remote_branch.trim())
        .execute(&mut *conn)
        .await?;
        if updated.rows_affected() != 1 {
            return Err(protocol_error(
                "Remote-tracking initialization shell is no longer the exact initialized member",
            ));
        }
        insert_initial_remote_sync_run(conn, member, input).await?;
    }
    Ok(())
}

async fn insert_initial_remote_sync_run(
    conn: &mut PgConnection,
    member: &BranchCommitMember,
    input: &InitialRemoteTrackingInput,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        INSERT INTO remote_sync_runs (
            org_id, workspace_id, repo_id, acting_user_id,
            kind, trigger_kind, status, new_workspace_tip, new_mirror_tip, new_baseline, completed_at
        )
        SELECT org_id, workspace_id, repo_id, $3, 'integration', 'manual', 'noop', $4, $5, $6, now()
        FROM remote_integrations WHERE workspace_id = $1 AND repo_id = $2
        "#,
    )
    .bind(input.workspace_id)
    .bind(input.repo_id)
    .bind(input.acting_user_id)
    .bind(&member.proposed_new_head)
    .bind(&input.remote_tip_advisory_sha)
    .bind(&input.baseline_commit)
    .execute(&mut *conn)
    .await?;
    Ok(())
}

/// Validate convergence inputs against the members of a prepared cut.
pub(crate) fn validate_remote_tracking_convergence_inputs(
    members: &[BranchCommitMember],
    inputs: &[PreparedRemoteTrackingConvergence],
) -> Result<(), sqlx::Error> {
    if inputs.is_empty() {
        return Ok(());
    }
    let unique_members = inputs
        .iter()
        .map(|input| input.repo_id)
        .collect::<BTreeSet<_>>();
    let binding_inputs = inputs
        .iter()
        .filter(|input| input.expected_tracking_mode != "remote_tracking")
        .count();
    // Synchronization effects belong only to verified provider imports. A
    // complete workspace cut can also contain local changes or unchanged
    // remote members whose watermarks must stay untouched. The producer owns
    // admission and coverage; this boundary verifies each exact member tuple.
    // Binding a previously local member remains a separate single-member action.
    let valid_shape = binding_inputs == 0 || (binding_inputs == inputs.len() && inputs.len() == 1);
    if !valid_shape || unique_members.len() != inputs.len() {
        return Err(protocol_error(
            "Remote-tracking convergence must name unique tracked members or one exact binding member",
        ));
    }
    for input in inputs {
        let member = members
            .iter()
            .find(|member| {
                member.workspace_id == input.workspace_id && member.repo_id == input.repo_id
            })
            .ok_or_else(|| {
                protocol_error(
                    "Remote-tracking convergence names a member outside the prepared cut",
                )
            })?;
        if input.remote_branch.trim().is_empty()
            || input.expected_tracking_mode.trim().is_empty()
            || input.trigger_kind.trim().is_empty()
            || !is_git_sha(&input.expected_old_head)
            || !is_git_sha(&input.proposed_new_head)
            || member.expected_old_head != input.expected_old_head
            || member.proposed_new_head != input.proposed_new_head
        {
            return Err(protocol_error(
                "Remote-tracking convergence does not match the prepared exact vector",
            ));
        }
    }
    Ok(())
}

/// Apply remote-tracking convergence metadata in the landing transaction,
/// after the member heads were refreshed.
pub(crate) async fn apply_remote_tracking_convergence_metadata(
    conn: &mut PgConnection,
    members: &[BranchCommitMember],
    inputs: &[PreparedRemoteTrackingConvergence],
) -> Result<(), sqlx::Error> {
    for input in inputs {
        let member = members
            .iter()
            .find(|member| member.repo_id == input.repo_id)
            .ok_or_else(|| {
                protocol_error("Remote-tracking convergence member changed after preparation")
            })?;
        let git_url: Option<String> = sqlx::query_scalar(
            "SELECT r.git_url FROM repos r JOIN workspace_repos m ON m.repo_id = r.id AND m.org_id = r.org_id
             WHERE m.workspace_id = $1 AND m.repo_id = $2 FOR UPDATE OF r, m",
        )
        .bind(input.workspace_id).bind(input.repo_id)
        .fetch_optional(&mut *conn).await?;
        if git_url.as_deref() != Some(input.expected_git_url.as_str()) {
            return Err(protocol_error(
                "Remote URL or membership changed before pull publication",
            ));
        }
        let updated = if input.expected_tracking_mode == "remote_tracking" {
            sqlx::query(
                "UPDATE remote_integrations SET remote_branch = $3, sync_paused_reason = NULL,
                     last_sync_at = now(), updated_at = now()
                 WHERE workspace_id = $1 AND repo_id = $2
                   AND remote_branch IS NOT DISTINCT FROM $4
                   AND sync_paused_reason IS NOT DISTINCT FROM $5",
            )
            .bind(input.workspace_id)
            .bind(input.repo_id)
            .bind(input.remote_branch.trim())
            .bind(&input.expected_remote_branch)
            .bind(&input.expected_sync_paused_reason)
            .execute(&mut *conn)
            .await?
        } else {
            sqlx::query(&format!(
                "INSERT INTO remote_integrations (org_id, workspace_id, repo_id, remote_branch, last_sync_at, auto_pull_enabled)
                 SELECT org_id, workspace_id, repo_id, $3, now(), {} FROM workspace_repos
                 WHERE workspace_id = $1 AND repo_id = $2
                 ON CONFLICT (workspace_id, repo_id) DO NOTHING",
                super::policy::initial_auto_pull_sql("$1")
            ))
            .bind(input.workspace_id).bind(input.repo_id).bind(input.remote_branch.trim())
            .execute(&mut *conn).await?
        };
        if updated.rows_affected() != 1 {
            return Err(protocol_error(
                "Remote-tracking metadata changed before convergence reconciliation",
            ));
        }
        sqlx::query(
            r#"
            INSERT INTO remote_sync_runs (
                org_id, workspace_id, repo_id, acting_user_id, kind, trigger_kind, status,
                old_workspace_tip, new_workspace_tip, old_mirror_tip, new_mirror_tip,
                new_baseline, started_at, completed_at
            )
            SELECT org_id, workspace_id, repo_id, $3, 'integration', $4, $5,
                   $6, $7, $8, $7, $7, now(), now()
            FROM remote_integrations WHERE workspace_id = $1 AND repo_id = $2
            "#,
        )
        .bind(input.workspace_id)
        .bind(input.repo_id)
        .bind(input.acting_user_id)
        .bind(input.trigger_kind.trim())
        .bind(input.status.as_str())
        .bind(&input.expected_old_head)
        .bind(&member.proposed_new_head)
        .bind(input.expected_baseline_commit.as_deref())
        .execute(&mut *conn)
        .await?;
    }
    Ok(())
}

fn protocol_error(message: impl Into<String>) -> sqlx::Error {
    sqlx::Error::Protocol(message.into())
}
