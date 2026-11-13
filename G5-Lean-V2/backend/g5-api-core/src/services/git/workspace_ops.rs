//! Workspace authority bookkeeping shared by retained Git producers.

use g5_authz::ScopedDb;
use g5_contracts::error::G5Error;
use sqlx::PgConnection;
use uuid::Uuid;

const ZERO_GIT_SHA: &str = "0000000000000000000000000000000000000000";

#[must_use]
pub fn branch_materialization_status(head_commit: &str) -> &'static str {
    let trimmed = head_commit.trim();
    if trimmed.is_empty()
        || trimmed == ZERO_GIT_SHA
        || trimmed.chars().all(|c| c == '0')
        || trimmed == "HEAD"
        || trimmed == "no-changes"
    {
        "unborn"
    } else {
        "materialized"
    }
}

pub(crate) async fn terminalize_active_merge_runs_with_tombstoned_sides(
    db: &mut ScopedDb,
    workspace_ids: &[Uuid],
) -> Result<(), G5Error> {
    if workspace_ids.is_empty() {
        return Ok(());
    }

    sqlx::query(
        r#"
        UPDATE workspace_merge_runs AS run
           SET status = 'failed',
               blocking_reason = COALESCE(
                   run.blocking_reason,
                   'Workspace tombstoned while merge was active'
               ),
               completed_at = COALESCE(run.completed_at, now()),
               updated_at = now()
         WHERE run.status IN ('pending', 'running', 'waiting_for_human', 'rolling_back')
           AND (
                run.workspace_id = ANY($1)
             OR run.source_workspace_id = ANY($1)
             OR run.target_workspace_id = ANY($1)
           )
           AND (
                EXISTS (
                    SELECT 1 FROM workspaces workspace
                     WHERE workspace.id = run.workspace_id
                       AND workspace.deleted_at IS NOT NULL
                )
             OR EXISTS (
                    SELECT 1 FROM workspaces workspace
                     WHERE workspace.id = run.source_workspace_id
                       AND workspace.deleted_at IS NOT NULL
                )
             OR EXISTS (
                    SELECT 1 FROM workspaces workspace
                     WHERE workspace.id = run.target_workspace_id
                       AND workspace.deleted_at IS NOT NULL
                )
           )
        "#,
    )
    .bind(workspace_ids)
    .execute(db.conn())
    .await?;

    Ok(())
}

/// Wake the configured external integration after a successful Git landing.
pub(crate) async fn enqueue_remote_tracking_integration_for_advance(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    repo_id: Uuid,
    advance_id: Uuid,
    trigger_kind: &str,
    authority: Option<crate::models::branch_integration_queue::PushAuthority>,
) -> Result<(), sqlx::Error> {
    crate::services::git::remote_integrations::enqueue_after_landing(
        conn,
        workspace_id,
        repo_id,
        advance_id,
        trigger_kind,
        authority,
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn materialization_status_requires_an_exact_commit() {
        assert_eq!(branch_materialization_status(""), "unborn");
        assert_eq!(branch_materialization_status(ZERO_GIT_SHA), "unborn");
        assert_eq!(branch_materialization_status("HEAD"), "unborn");
        assert_eq!(
            branch_materialization_status("a".repeat(40).as_str()),
            "materialized"
        );
    }
}
