//! Queue admission for repository-scoped remote integration.

use chrono::{DateTime, Utc};
use g5_contracts::types::WorkspaceRepoKey;
use sqlx::PgConnection;
use uuid::Uuid;

use crate::models::branch_integration_queue::trigger_kind;

/// Authority for one explicit provider publication, captured before asynchronous work.
#[derive(Debug, Clone, Copy, PartialEq, Eq, sqlx::FromRow)]
pub struct PushAuthority {
    pub requested_by_user_id: Uuid,
    pub lifecycle_generation: i64,
}

impl PushAuthority {
    /// The workspace row lock excludes transfer admission until the caller commits.
    pub async fn capture(
        conn: &mut PgConnection,
        workspace_id: Uuid,
        actor: Uuid,
    ) -> Result<Self, sqlx::Error> {
        sqlx::query_as(
            "SELECT w.owner_user_id AS requested_by_user_id, w.lifecycle_generation
             FROM workspaces w WHERE w.id = $1 AND w.owner_user_id = $2
               AND w.deleted_at IS NULL AND NOT EXISTS (
                 SELECT 1 FROM workspace_ownership_transfers t
                 WHERE t.workspace_id = w.id AND t.status = 'pending')
             FOR SHARE OF w",
        )
        .bind(workspace_id)
        .bind(actor)
        .fetch_optional(conn)
        .await?
        .ok_or_else(|| {
            sqlx::Error::Protocol(
                "Explicit push authority changed; request publication again".into(),
            )
        })
    }
}

fn explicit(trigger: &str) -> bool {
    matches!(
        trigger,
        trigger_kind::PUSH | trigger_kind::APPROVED_REBASE_PUBLISH
    )
}

/// Select the trigger and its authority together. An implicit signal may never
/// refresh an old explicit request's owner or lifecycle generation.
fn on_conflict_precedence() -> String {
    let incoming = "CASE
        WHEN EXCLUDED.requested_by_user_id IS NOT NULL AND
             (remote_integration_queue.requested_by_user_id IS DISTINCT FROM EXCLUDED.requested_by_user_id
              OR remote_integration_queue.lifecycle_generation IS DISTINCT FROM EXCLUDED.lifecycle_generation) THEN true
        WHEN remote_integration_queue.trigger_kind = 'approved_rebase_publish'
             AND EXCLUDED.trigger_kind = 'push' AND EXCLUDED.advance_id IS NOT NULL
             AND NOT EXISTS (
                SELECT 1 FROM workspace_merge_runs active_checkpoint
                WHERE active_checkpoint.workspace_id = EXCLUDED.workspace_id
                  AND active_checkpoint.repo_id = EXCLUDED.repo_id
                  AND active_checkpoint.source_kind = 'remote_tracking'
                  AND active_checkpoint.phase = 'reset_below_baseline'
                  AND active_checkpoint.status IN ('waiting_for_human', 'running')) THEN true
        WHEN remote_integration_queue.trigger_kind = 'approved_rebase_publish' THEN false
        WHEN EXCLUDED.trigger_kind = 'approved_rebase_publish' THEN true
        WHEN EXCLUDED.trigger_kind = 'push' THEN true
        WHEN remote_integration_queue.trigger_kind = 'push' THEN false
        WHEN EXCLUDED.trigger_kind = 'manual' THEN true
        WHEN remote_integration_queue.trigger_kind = 'manual' THEN false
        WHEN remote_integration_queue.trigger_kind = 'commit' THEN false
        ELSE true END";
    format!("ON CONFLICT (workspace_id, repo_id) DO UPDATE SET
        trigger_kind = CASE WHEN {incoming} THEN EXCLUDED.trigger_kind ELSE remote_integration_queue.trigger_kind END,
        requested_by_user_id = CASE WHEN {incoming} THEN EXCLUDED.requested_by_user_id ELSE remote_integration_queue.requested_by_user_id END,
        lifecycle_generation = CASE WHEN {incoming} THEN EXCLUDED.lifecycle_generation ELSE remote_integration_queue.lifecycle_generation END,
        requested_at = EXCLUDED.requested_at,
        advance_id = COALESCE(EXCLUDED.advance_id, remote_integration_queue.advance_id)")
}

pub async fn enqueue_branch_integration(
    conn: &mut PgConnection,
    member: WorkspaceRepoKey,
    trigger_kind: &str,
) -> Result<(), sqlx::Error> {
    enqueue_branch_integration_at(conn, member, trigger_kind, Utc::now()).await
}

pub async fn enqueue_branch_integration_for_advance(
    conn: &mut PgConnection,
    member: WorkspaceRepoKey,
    advance_id: Uuid,
) -> Result<(), sqlx::Error> {
    enqueue_branch_integration_for_advance_with_trigger(
        conn,
        member,
        advance_id,
        trigger_kind::COMMIT,
        None,
    )
    .await
}

pub async fn enqueue_branch_integration_for_advance_with_trigger(
    conn: &mut PgConnection,
    member: WorkspaceRepoKey,
    advance_id: Uuid,
    trigger_kind: &str,
    authority: Option<PushAuthority>,
) -> Result<(), sqlx::Error> {
    enqueue(
        conn,
        member,
        trigger_kind,
        Utc::now(),
        Some(advance_id),
        authority,
    )
    .await
}

pub async fn enqueue_branch_integration_at(
    conn: &mut PgConnection,
    member: WorkspaceRepoKey,
    trigger_kind: &str,
    requested_at: DateTime<Utc>,
) -> Result<(), sqlx::Error> {
    enqueue(conn, member, trigger_kind, requested_at, None, None).await
}

pub async fn enqueue_authorized_branch_integration(
    conn: &mut PgConnection,
    member: WorkspaceRepoKey,
    trigger_kind: &str,
    requested_at: DateTime<Utc>,
    authority: PushAuthority,
) -> Result<(), sqlx::Error> {
    enqueue(
        conn,
        member,
        trigger_kind,
        requested_at,
        None,
        Some(authority),
    )
    .await
}

async fn enqueue(
    conn: &mut PgConnection,
    member: WorkspaceRepoKey,
    trigger_kind: &str,
    requested_at: DateTime<Utc>,
    advance_id: Option<Uuid>,
    authority: Option<PushAuthority>,
) -> Result<(), sqlx::Error> {
    if explicit(trigger_kind) != authority.is_some() {
        return Err(sqlx::Error::Protocol(
            "Explicit publication requires captured push authority".into(),
        ));
    }
    // Validate before taking queue locks, in the same order as ownership transfer.
    if let Some(expected) = authority {
        if PushAuthority::capture(conn, member.workspace_id, expected.requested_by_user_id).await?
            != expected
        {
            return Err(sqlx::Error::Protocol(
                "Explicit push lifecycle changed; request publication again".into(),
            ));
        }
    }
    // Base never publishes to its provider (`g5.workspace.remote.base-parity`):
    // an explicit publication request for Base, including an owner's merge
    // confirmation, queues nothing.
    let sql = format!(
        "INSERT INTO remote_integration_queue
            (org_id, workspace_id, repo_id, trigger_kind, requested_at, advance_id, requested_by_user_id, lifecycle_generation)
         SELECT integration.org_id, integration.workspace_id, integration.repo_id, $4, $5, $6, $7, $8
         FROM remote_integrations integration
         JOIN workspaces workspace ON workspace.id = integration.workspace_id AND workspace.org_id = integration.org_id
         WHERE integration.org_id = $1 AND integration.workspace_id = $2 AND integration.repo_id = $3
           AND workspace.deleted_at IS NULL
           AND NOT (workspace.is_base AND $4 IN ('{push}', '{approved}'))
         {}", on_conflict_precedence(),
        push = trigger_kind::PUSH,
        approved = trigger_kind::APPROVED_REBASE_PUBLISH,
    );
    sqlx::query(&sql)
        .bind(member.org_id)
        .bind(member.workspace_id)
        .bind(member.repo_id)
        .bind(trigger_kind)
        .bind(requested_at)
        .bind(advance_id)
        .bind(authority.map(|a| a.requested_by_user_id))
        .bind(authority.map(|a| a.lifecycle_generation))
        .execute(conn)
        .await?;
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkspaceEnqueueConflict {
    Precedence,
    DoNothing,
}

/// The caller's tenant scope and same-org join govern the complete fan-out.
pub async fn enqueue_branch_integration_for_workspace(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    trigger_kind: &str,
    conflict: WorkspaceEnqueueConflict,
) -> Result<u64, sqlx::Error> {
    if explicit(trigger_kind) {
        return Err(sqlx::Error::Protocol(
            "Explicit publication requires captured push authority".into(),
        ));
    }
    let on_conflict = match conflict {
        WorkspaceEnqueueConflict::Precedence => on_conflict_precedence(),
        WorkspaceEnqueueConflict::DoNothing => {
            "ON CONFLICT (workspace_id, repo_id) DO NOTHING".into()
        }
    };
    let sql = format!(
        "INSERT INTO remote_integration_queue (org_id, workspace_id, repo_id, trigger_kind, requested_at)
         SELECT integration.org_id, integration.workspace_id, integration.repo_id, $2, now()
         FROM remote_integrations integration
         JOIN workspaces workspace ON workspace.id = integration.workspace_id AND workspace.org_id = integration.org_id
         WHERE integration.workspace_id = $1 AND workspace.deleted_at IS NULL
           AND integration.sync_paused_reason IS NULL
         {on_conflict}"
    );
    Ok(sqlx::query(&sql)
        .bind(workspace_id)
        .bind(trigger_kind)
        .execute(conn)
        .await?
        .rows_affected())
}
