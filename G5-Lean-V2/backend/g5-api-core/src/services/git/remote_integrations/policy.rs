use super::*;
use crate::{errors::AppError, models::branch_sync_run::status as run_status};

/// Change passive policy and its queued intent in the caller's transaction.
/// Explicit push/commit requests retain their precedence when passive work is disabled.
pub async fn set_auto_pull(
    conn: &mut PgConnection,
    key: WorkspaceRepoKey,
    enabled: bool,
    actor_user_id: Uuid,
) -> Result<bool, G5Error> {
    let workspace_id = key.workspace_id;
    let repo_id = key.repo_id;
    let integration = super::load(conn, key, true)
        .await?
        .ok_or_else(|| G5Error::Conflict {
            reason: "Bind or publish this repository before enabling auto-pull".into(),
        })?;
    let old_value = integration.auto_pull_enabled;
    sqlx::query("UPDATE remote_integrations SET auto_pull_enabled = $3, updated_at = now()
                     WHERE workspace_id = $1 AND repo_id = $2 AND auto_pull_enabled IS DISTINCT FROM $3")
            .bind(workspace_id).bind(repo_id).bind(enabled).execute(&mut *conn).await?;
    if enabled && !old_value {
        sqlx::query("INSERT INTO remote_integration_queue (org_id, workspace_id, repo_id, trigger_kind, requested_at)
                VALUES ($1, $2, $3, 'background_inbound', now()) ON CONFLICT (workspace_id, repo_id) DO NOTHING")
                .bind(key.org_id).bind(workspace_id).bind(repo_id).execute(&mut *conn).await?;
    } else if !enabled {
        sqlx::query(
            "DELETE FROM remote_integration_queue WHERE workspace_id = $1 AND repo_id = $2
                         AND trigger_kind = 'background_inbound'",
        )
        .bind(workspace_id)
        .bind(repo_id)
        .execute(&mut *conn)
        .await?;
    }

    // Append-only audit event (actor, old value, new value, identity,
    // result) via structured observability logging. See
    // `system_invariants/observability_audit` D3.
    tracing::info!(
        target: "g5_audit",
        workspace_id = %workspace_id,
        repo_id = %repo_id,
        actor_user_id = %actor_user_id,
        old_value = old_value,
        new_value = enabled,
        "auto_pull_enabled toggled"
    );

    Ok(enabled)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RetryOutcome {
    Reenqueued { branches: usize },
    Nothing,
    NonRetryable,
}

/// Retry transient failures without clearing divergence or watermark pauses.
/// Hold the policy lock through classification and enqueue to exclude a race
/// with conflict handling or a policy edit. Queue precedence preserves push.
/// A provider-rewrite pause is rechecked by the worker against fresh Git
/// inputs, which keeps a genuine checkpoint and closes a false one.
pub async fn retry(
    conn: &mut PgConnection,
    key: WorkspaceRepoKey,
) -> Result<RetryOutcome, AppError> {
    let Some(policy) = super::load(conn, key, true).await? else {
        return Ok(RetryOutcome::Nothing);
    };
    if policy.sync_paused_reason.as_deref() == Some("watermark_violation") {
        crate::models::branch_integration_queue::enqueue_branch_integration(
            conn,
            key,
            crate::models::branch_integration_queue::trigger_kind::MANUAL,
        )
        .await?;
        return Ok(RetryOutcome::Reenqueued { branches: 1 });
    }
    let retryable = [
        run_status::PUSH_FAILED,
        run_status::FETCH_FAILED,
        run_status::AUTH_REQUIRED,
    ];
    if policy
        .sync_paused_reason
        .as_deref()
        .is_some_and(|reason| !retryable.contains(&reason))
    {
        return Ok(RetryOutcome::NonRetryable);
    }
    let latest: Option<String> = sqlx::query_scalar(
        "SELECT status FROM remote_sync_runs WHERE org_id = $1 AND workspace_id = $2 AND repo_id = $3
           AND kind = 'integration' ORDER BY started_at DESC, id DESC LIMIT 1"
    ).bind(key.org_id).bind(key.workspace_id).bind(key.repo_id).fetch_optional(&mut *conn).await?;
    let Some(latest) = latest else {
        return Ok(RetryOutcome::Nothing);
    };
    if !retryable.contains(&latest.as_str()) {
        return Ok(RetryOutcome::NonRetryable);
    }
    sqlx::query(
        "UPDATE remote_integrations SET sync_paused_reason = NULL, updated_at = now()
         WHERE org_id = $1 AND workspace_id = $2 AND repo_id = $3",
    )
    .bind(key.org_id)
    .bind(key.workspace_id)
    .bind(key.repo_id)
    .execute(&mut *conn)
    .await?;
    crate::models::branch_integration_queue::enqueue_branch_integration(
        conn,
        key,
        crate::models::branch_integration_queue::trigger_kind::MANUAL,
    )
    .await?;
    Ok(RetryOutcome::Reenqueued { branches: 1 })
}

/// SQL for a new binding's initial auto-pull: on for Base, off for every other
/// workspace (`g5.workspace.remote.base-auto-pull-default`). Every binding
/// insert uses it so the default has one home; the owner may change it later.
pub(super) fn initial_auto_pull_sql(workspace_id: &str) -> String {
    format!("COALESCE((SELECT w.is_base FROM workspaces w WHERE w.id = {workspace_id}), false)")
}

/// Bind exactly one existing member. The caller must confirm external publication
/// first; this function owns policy defaults and initial queue admission.
pub async fn bind_published(
    conn: &mut PgConnection,
    key: WorkspaceRepoKey,
    remote_branch: &str,
) -> Result<(), G5Error> {
    let inserted = sqlx::query(&format!(
        "INSERT INTO remote_integrations (org_id, workspace_id, repo_id, remote_branch, auto_pull_enabled)
         VALUES ($1, $2, $3, $4, {}) ON CONFLICT (workspace_id, repo_id) DO NOTHING",
        initial_auto_pull_sql("$2")
    ))
    .bind(key.org_id)
    .bind(key.workspace_id)
    .bind(key.repo_id)
    .bind(remote_branch)
    .execute(&mut *conn)
    .await?;
    if inserted.rows_affected() != 1 {
        return Err(G5Error::Conflict {
            reason: "Remote binding changed during publication".into(),
        });
    }
    crate::models::branch_integration_queue::enqueue_branch_integration(
        conn,
        key,
        crate::models::branch_integration_queue::trigger_kind::MANUAL,
    )
    .await?;
    Ok(())
}

/// Reserve an unbound member's policy. Exact Git evidence is published by the
/// enclosing remote operation; this never stores the current Git head.
pub(crate) async fn bind_policy(
    conn: &mut PgConnection,
    key: WorkspaceRepoKey,
    remote_branch: &str,
    pause: Option<&str>,
    synchronized: bool,
) -> Result<bool, sqlx::Error> {
    Ok(sqlx::query(&format!("INSERT INTO remote_integrations (org_id, workspace_id, repo_id, remote_branch, sync_paused_reason, last_sync_at, auto_pull_enabled)
        VALUES ($1, $2, $3, $4, $5, CASE WHEN $6 THEN now() END, {})
        ON CONFLICT (workspace_id, repo_id) DO NOTHING", initial_auto_pull_sql("$2")))
        .bind(key.org_id).bind(key.workspace_id).bind(key.repo_id).bind(remote_branch).bind(pause).bind(synchronized)
        .execute(conn).await?.rows_affected() == 1)
}

pub(crate) async fn has_bindings(
    conn: &mut PgConnection,
    workspace_id: Uuid,
) -> Result<bool, G5Error> {
    Ok(sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM remote_integrations WHERE workspace_id=$1)",
    )
    .bind(workspace_id)
    .fetch_one(conn)
    .await?)
}

pub(crate) async fn remote_branches(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    for_update: bool,
) -> Result<BTreeMap<Uuid, String>, G5Error> {
    let lock = if for_update { "FOR UPDATE" } else { "" };
    let rows: Vec<(Uuid, String)> = sqlx::query_as(&format!("SELECT repo_id, remote_branch FROM remote_integrations WHERE workspace_id=$1 ORDER BY repo_id {lock}"))
        .bind(workspace_id).fetch_all(conn).await?;
    Ok(rows.into_iter().collect())
}

/// Native Git is a commit signal, not an explicit provider-push override.
/// The worker applies current policy and verifies the provider branch exists.
pub async fn enqueue_after_push(
    conn: &mut PgConnection,
    key: WorkspaceRepoKey,
) -> Result<(), sqlx::Error> {
    crate::models::branch_integration_queue::enqueue_branch_integration(
        conn,
        key,
        crate::models::branch_integration_queue::trigger_kind::COMMIT,
    )
    .await
}

pub(crate) async fn enqueue_after_landing(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    repo_id: Uuid,
    advance_id: Uuid,
    trigger_kind: &str,
    authority: Option<crate::models::branch_integration_queue::PushAuthority>,
) -> Result<(), sqlx::Error> {
    if super::convergence::member_synchronized_during_landing(
        conn,
        advance_id,
        workspace_id,
        repo_id,
    )
    .await?
    {
        // Suppress only this landing's redundant signal. Leave any existing
        // queue generation intact so the worker can preserve newer requests.
        return Ok(());
    }
    let org_id: Option<Uuid> = sqlx::query_scalar(
        "SELECT org_id FROM remote_integrations WHERE workspace_id = $1 AND repo_id = $2",
    )
    .bind(workspace_id)
    .bind(repo_id)
    .fetch_optional(&mut *conn)
    .await?;
    let Some(org_id) = org_id else {
        return Ok(());
    };
    crate::models::branch_integration_queue::enqueue_branch_integration_for_advance_with_trigger(
        conn,
        g5_contracts::types::WorkspaceRepoKey {
            org_id,
            workspace_id,
            repo_id,
        },
        advance_id,
        trigger_kind,
        authority,
    )
    .await
}

/// Enqueue one background request per eligible canonical membership.
/// Nonblocking policy locks avoid waiting on an active integration. The queue's
/// composite key and DO NOTHING preserve any existing higher-priority request.
/// Explicit org scoping also keeps superuser-backed development pools isolated.
pub(crate) async fn enqueue_background_inbound_rows(
    conn: &mut PgConnection,
    passive_pull_enabled: bool,
) -> Result<usize, AppError> {
    let rows = sqlx::query(
        r#"
        -- Avoid waiting on a policy row held by a commit or integration cycle.
        WITH eligible AS (
            SELECT i.workspace_id, i.repo_id, i.org_id
            FROM remote_integrations i
            JOIN workspaces w ON w.id = i.workspace_id AND w.org_id = i.org_id
            -- A provider-rewrite pause is rechecked; the worker keeps a genuine one.
            WHERE (w.push_mode = 'auto' OR ($1 AND i.auto_pull_enabled))
              AND (i.sync_paused_reason IS NULL OR i.sync_paused_reason = 'watermark_violation')
              AND w.deleted_at IS NULL
              AND i.org_id = ANY(authz.current_org_ids())
            FOR KEY SHARE OF i SKIP LOCKED
        )
        INSERT INTO remote_integration_queue (workspace_id, repo_id, org_id, trigger_kind, requested_at)
        SELECT workspace_id, repo_id, org_id, 'background_inbound', now() FROM eligible
        ON CONFLICT (workspace_id, repo_id) DO NOTHING
        "#,
    )
    .bind(passive_pull_enabled)
    .execute(&mut *conn)
    .await
    .map_err(AppError::from)?;
    Ok(rows.rows_affected() as usize)
}

#[cfg(test)]
mod tests {
    #[test]
    fn auto_pull_lookup_filters_only_live_workspaces() {
        assert!(super::super::POLICY_QUERY.contains("w.deleted_at IS NULL"));
        let source = include_str!("policy.rs");
        let helper = source.split("#[derive(Debug, Clone, Copy").next().unwrap();
        assert!(helper.contains("super::load(conn, key, true)"));
    }

    #[test]
    fn repeated_auto_pull_enable_does_not_wake_passive_work() {
        let source = include_str!("policy.rs");
        let helper = source.split("#[derive(Debug, Clone, Copy").next().unwrap();
        assert!(helper.contains("AND auto_pull_enabled IS DISTINCT FROM $3"));
        assert!(
            helper.contains("if enabled && !old_value"),
            "only a false-to-true transition may wake passive auto-pull work"
        );
    }
}
