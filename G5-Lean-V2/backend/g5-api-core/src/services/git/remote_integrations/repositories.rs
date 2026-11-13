use super::*;
use crate::services::git::repo_store::ScopedRepoStorage;
use g5_scm::GitRepo;

#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub(crate) struct CoordinatedWorkspaceRepoRow {
    pub(crate) org_id: Uuid,
    pub(crate) is_base: bool,
    pub(crate) owner_user_id: Option<Uuid>,
    pub(crate) docs_repo_id: Option<Uuid>,
    pub(crate) workspace_id: Uuid,
    pub(crate) repo_id: Uuid,
    pub(crate) repo_name: String,
    pub(crate) git_url: String,
    pub(crate) provider: Option<String>,
    pub(crate) provider_repo_id: Option<String>,
    pub(crate) provider_full_name: Option<String>,
    pub(crate) branch_name: String,
    pub(crate) default_branch: String,
    #[sqlx(skip)]
    pub(crate) head_commit: String,
    pub(crate) tracking_mode: String,
    #[sqlx(skip)]
    pub(crate) materialization_status: String,
    pub(crate) remote_branch: Option<String>,
    pub(crate) sync_paused_reason: Option<String>,
    #[sqlx(skip)]
    pub(crate) baseline_commit: Option<String>,
    #[sqlx(skip)]
    pub(crate) remote_tip_advisory_sha: Option<String>,
    pub(crate) dirty_count: i32,
}

impl CoordinatedWorkspaceRepoRow {
    pub(crate) fn key(&self) -> WorkspaceRepoKey {
        WorkspaceRepoKey {
            org_id: self.org_id,
            workspace_id: self.workspace_id,
            repo_id: self.repo_id,
        }
    }
}

pub(crate) async fn capture_repositories(
    conn: &mut PgConnection,
    storage: ScopedRepoStorage,
    workspace_id: Uuid,
    lock_rows: bool,
) -> Result<Vec<CoordinatedWorkspaceRepoRow>, G5Error> {
    let for_update = if lock_rows {
        "FOR UPDATE OF w, m, r"
    } else {
        ""
    };
    let sql = format!(
        r#"
            SELECT w.org_id, w.is_base, w.owner_user_id, w.docs_repo_id,
                   w.id AS workspace_id, r.id AS repo_id, r.name AS repo_name,
                   r.git_url, r.provider, r.provider_repo_id, r.provider_full_name,
                   CASE WHEN w.is_base THEN r.default_branch ELSE w.branch_name END AS branch_name,
                   r.default_branch,
                   CASE WHEN i.repo_id IS NOT NULL THEN 'remote_tracking' ELSE 'pinned' END AS tracking_mode,
                   i.remote_branch, i.sync_paused_reason, 0::int4 AS dirty_count
            FROM workspaces w
            JOIN workspace_repos m ON m.workspace_id = w.id AND m.org_id = w.org_id
            JOIN repos r ON r.id = m.repo_id AND r.org_id = m.org_id
            LEFT JOIN remote_integrations i
              ON (i.org_id, i.workspace_id, i.repo_id) = (m.org_id, m.workspace_id, m.repo_id)
            WHERE w.id = $1 AND w.deleted_at IS NULL
            ORDER BY (r.id = w.docs_repo_id), r.name, r.id
            {for_update}
        "#
    );
    let mut rows = sqlx::query_as::<_, CoordinatedWorkspaceRepoRow>(&sql)
        .bind(workspace_id)
        .fetch_all(&mut *conn)
        .await?;
    if lock_rows {
        sqlx::query("SELECT repo_id FROM remote_integrations WHERE workspace_id = $1 ORDER BY repo_id FOR UPDATE")
                .bind(workspace_id).fetch_all(&mut *conn).await?;
    }
    rows = tokio::task::spawn_blocking(move || -> Result<_, G5Error> {
        for row in &mut rows {
            let path = storage.repo_path(row.org_id, row.repo_id)?;
            let repo = GitRepo::open(&path)?;
            row.head_commit = repo.resolve_branch_head(&row.branch_name)?;
            row.materialization_status =
                crate::services::workspace_ops::branch_materialization_status(&row.head_commit)
                    .to_string();
            row.baseline_commit = repo.read_sync_watermark(&row.branch_name)?;
        }
        Ok(rows)
    })
    .await
    .map_err(|error| G5Error::Internal {
        message: format!("Git capture failed: {error}"),
    })??;

    Ok(rows)
}
#[derive(Debug, Clone, sqlx::FromRow)]
pub(crate) struct WorkspaceRepositoryRow {
    pub(crate) org_id: Uuid,
    pub(crate) workspace_id: Uuid,
    pub(crate) repo_id: Uuid,
    pub(crate) branch_name: String,
    pub(crate) tracking_mode: String,
    #[sqlx(skip)]
    pub(crate) head_commit: String,
    #[sqlx(skip)]
    pub(crate) materialization_status: String,
    pub(crate) has_draft: bool,
    pub(crate) dirty_file_count: i32,
    pub(crate) remote_branch: Option<String>,
    pub(crate) co_tracking_workspace_count: Option<i64>,
    pub(crate) auto_pull_enabled: Option<bool>,
    pub(crate) remote_eligible: Option<bool>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub(crate) struct EligiblePrSourceRow {
    pub(crate) source_mode: String,
    pub(crate) target_mode: String,
    pub(crate) git_url: Option<String>,
    pub(crate) source_local_branch: String,
    pub(crate) source_remote_branch: Option<String>,
    pub(crate) source_workspace_id: Uuid,
}

pub(crate) async fn repository_summaries(
    conn: &mut PgConnection,
    storage: &ScopedRepoStorage,
    workspace_ids: &[Uuid],
) -> Result<Vec<WorkspaceRepositoryRow>, G5Error> {
    let mut rows: Vec<WorkspaceRepositoryRow> = sqlx::query_as(
            r#"SELECT m.org_id, m.workspace_id, m.repo_id,
                CASE WHEN w.is_base THEN r.default_branch ELSE w.branch_name END AS branch_name,
                CASE WHEN i.repo_id IS NOT NULL THEN 'remote_tracking' ELSE 'pinned' END AS tracking_mode,
                false AS has_draft, 0::int4 AS dirty_file_count, i.remote_branch, i.auto_pull_enabled,
                CASE WHEN i.repo_id IS NOT NULL THEN (
                    SELECT count(*) FROM remote_integrations peer JOIN workspaces pw ON pw.id = peer.workspace_id AND pw.org_id = peer.org_id
                    WHERE peer.org_id = m.org_id AND peer.repo_id = m.repo_id AND peer.remote_branch = i.remote_branch
                      AND peer.workspace_id <> m.workspace_id AND pw.deleted_at IS NULL
                ) ELSE NULL END AS co_tracking_workspace_count,
                CASE WHEN i.repo_id IS NULL AND EXISTS (
                    SELECT 1 FROM remote_integrations parent_integration JOIN workspaces parent ON parent.id = parent_integration.workspace_id AND parent.org_id = parent_integration.org_id
                    WHERE parent_integration.workspace_id = w.cloned_from_id AND parent_integration.repo_id = m.repo_id
                      AND parent_integration.org_id = m.org_id AND parent.deleted_at IS NULL
                ) THEN true ELSE NULL END AS remote_eligible
            FROM workspace_repos m JOIN workspaces w ON w.id = m.workspace_id AND w.org_id = m.org_id
            JOIN repos r ON r.id = m.repo_id AND r.org_id = m.org_id
            LEFT JOIN remote_integrations i ON (i.org_id, i.workspace_id, i.repo_id) = (m.org_id, m.workspace_id, m.repo_id)
            WHERE m.workspace_id = ANY($1) AND w.deleted_at IS NULL ORDER BY m.workspace_id, m.repo_id"#,
        ).bind(workspace_ids).fetch_all(conn).await?;
    let mut refs = std::collections::BTreeMap::<Uuid, Vec<(Uuid, String)>>::new();
    for row in &rows {
        refs.entry(row.org_id)
            .or_default()
            .push((row.repo_id, row.branch_name.clone()));
    }
    let mut heads = std::collections::BTreeMap::new();
    for (org_id, refs) in refs {
        heads.extend(
            crate::services::workspace_reader::read_branch_head_oids(storage, org_id, &refs)
                .await?,
        );
    }
    for row in &mut rows {
        row.head_commit = heads
            .get(&(row.repo_id, row.branch_name.clone()))
            .cloned()
            .flatten()
            .unwrap_or_else(|| crate::services::branch_commit::ZERO_GIT_SHA.into());
        row.materialization_status =
            crate::services::workspace_ops::branch_materialization_status(&row.head_commit).into();
    }
    Ok(rows)
}
pub(crate) async fn eligible_pr_source(
    conn: &mut PgConnection,
    source_workspace_id: Uuid,
    target_workspace_id: Uuid,
    repo_id: Uuid,
) -> Result<Option<EligiblePrSourceRow>, G5Error> {
    let eligible = sqlx::query_as(
                r#"
                    SELECT CASE WHEN si.repo_id IS NULL THEN 'pinned' ELSE 'remote_tracking' END AS source_mode,
                           'remote_tracking'::text AS target_mode,
                           r.git_url,
                           CASE WHEN sw.is_base THEN r.default_branch ELSE sw.name END AS source_local_branch,
                           si.remote_branch AS source_remote_branch, sw.id AS source_workspace_id
                    FROM repos r
                    JOIN workspace_repos sm ON sm.repo_id = r.id AND sm.org_id = r.org_id AND sm.workspace_id = $1
                    JOIN workspaces sw ON sw.id = sm.workspace_id AND sw.org_id = sm.org_id AND sw.deleted_at IS NULL
                    LEFT JOIN remote_integrations si ON (si.org_id, si.workspace_id, si.repo_id) = (sm.org_id, sm.workspace_id, sm.repo_id)
                    JOIN remote_integrations ti ON ti.repo_id = r.id AND ti.org_id = r.org_id AND ti.workspace_id = $2
                    JOIN workspaces tw ON tw.id = ti.workspace_id AND tw.org_id = ti.org_id AND tw.deleted_at IS NULL
                    WHERE r.id = $3 AND r.org_id = ANY(authz.current_org_ids())
                    "#,
            )
            .bind(source_workspace_id)
            .bind(target_workspace_id)
            .bind(repo_id)
            .fetch_optional(conn)
            .await
            .map_err(|e| G5Error::Internal {
                message: format!("Failed to load PR repo tracking modes: {e}"),
            })?;
    Ok(eligible)
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub(crate) struct ProviderCandidateRow {
    pub(crate) repo_id: Uuid,
    pub(crate) provider: Option<String>,
    pub(crate) git_url: String,
    pub(crate) source_tracking_mode: String,
    pub(crate) source_remote_branch: Option<String>,
    pub(crate) target_tracking_mode: String,
    pub(crate) target_remote_branch: Option<String>,
}
pub(crate) async fn provider_candidate(
    conn: &mut PgConnection,
    org_id: Uuid,
    source_workspace_id: Uuid,
    target_workspace_id: Uuid,
    repo_id: Uuid,
) -> Result<Option<ProviderCandidateRow>, G5Error> {
    let row = sqlx::query_as::<_, ProviderCandidateRow>(
        r#"
            SELECT r.id AS repo_id,
                   r.provider,
                   r.git_url,
                   'remote_tracking'::text AS source_tracking_mode,
                   swbr.remote_branch AS source_remote_branch,
                   'remote_tracking'::text AS target_tracking_mode,
                   twbr.remote_branch AS target_remote_branch
              FROM repos r
              JOIN remote_integrations swbr
                ON swbr.repo_id = r.id
               AND swbr.workspace_id = $1
              JOIN remote_integrations twbr
                ON twbr.repo_id = r.id
               AND twbr.workspace_id = $2
             WHERE r.id = $3
               AND r.org_id = $4
               AND swbr.org_id = $4
               AND twbr.org_id = $4
            "#,
    )
    .bind(source_workspace_id)
    .bind(target_workspace_id)
    .bind(repo_id)
    .bind(org_id)
    .fetch_optional(conn)
    .await?;
    Ok(row)
}
