//! Workspace membership and Git ref selection. No Git versions live in these rows.

use crate::services::git::repo_store::ScopedRepoStorage;
use g5_contracts::G5Error;
use sqlx::PgConnection;
use uuid::Uuid;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct WorkspaceRepository {
    pub org_id: Uuid,
    pub workspace_id: Uuid,
    pub repo_id: Uuid,
    pub repo_name: String,
    pub repo_slug: String,
    pub root_path: String,
    pub role: String,
    pub branch_name: String,
    pub is_base: bool,
}

/// Load the entire member vector in deterministic lock order. Callers must
/// already hold permission to read the workspace (tenant scope is not enough).
pub async fn load_workspace_repositories(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    for_update: bool,
) -> Result<Vec<WorkspaceRepository>, G5Error> {
    let lock = if for_update {
        "FOR UPDATE OF w, m, r"
    } else {
        ""
    };
    let query = format!(
        "SELECT m.org_id, m.workspace_id, m.repo_id,
                r.name AS repo_name, r.slug AS repo_slug, m.root_path, m.role,
                CASE WHEN w.is_base THEN r.default_branch ELSE w.branch_name END AS branch_name,
                w.is_base
         FROM workspace_repos m
         JOIN workspaces w ON w.id = m.workspace_id AND w.org_id = m.org_id
         JOIN repos r ON r.id = m.repo_id AND r.org_id = m.org_id
         WHERE w.id = $1 AND w.deleted_at IS NULL
         ORDER BY m.repo_id {lock}"
    );
    let repos = sqlx::query_as::<_, WorkspaceRepository>(&query)
        .bind(workspace_id)
        .fetch_all(conn)
        .await?;
    if repos.is_empty() {
        return Err(G5Error::NotFound {
            resource: format!("workspace repository membership {workspace_id}"),
        });
    }
    Ok(repos)
}

/// A parent relationship never grants read permission to the parent. Resolve
/// it inside the source organization and recheck the current actor's access.
pub async fn readable_parent_workspace(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    actor_id: Uuid,
) -> Result<Uuid, G5Error> {
    sqlx::query_scalar::<_, Option<Uuid>>("SELECT authz.readable_parent_workspace($1, $2)")
        .bind(workspace_id)
        .bind(actor_id)
        .fetch_one(conn)
        .await?
        .ok_or_else(|| G5Error::DiffGraphUnavailable {
            reason: "parent_unavailable".to_string(),
            missing_repo_count: 0,
        })
}

/// Resolve comparison bases from two already-selected immutable repository
/// vectors. Both vectors must contain exactly the same repository members.
pub async fn comparison_bases(
    storage: &ScopedRepoStorage,
    org_id: Uuid,
    source: &std::collections::BTreeMap<Uuid, String>,
    parent: &std::collections::BTreeMap<Uuid, String>,
) -> Result<std::collections::BTreeMap<Uuid, String>, G5Error> {
    use std::collections::{BTreeMap, BTreeSet};
    let source_ids = source.keys().copied().collect::<BTreeSet<_>>();
    let parent_ids = parent.keys().copied().collect::<BTreeSet<_>>();
    if source_ids != parent_ids {
        return Err(G5Error::DiffGraphUnavailable {
            reason: "repository_set_mismatch".into(),
            missing_repo_count: source_ids.symmetric_difference(&parent_ids).count(),
        });
    }
    let inputs = source
        .iter()
        .map(|(id, head)| {
            let path = storage.repo_path(org_id, *id)?;
            Ok((*id, path, head.clone(), parent[id].clone()))
        })
        .collect::<Result<Vec<_>, G5Error>>()?;
    tokio::task::spawn_blocking(move || {
        inputs
            .into_iter()
            .map(|(id, path, head, parent)| {
                let bases = g5_scm::GitRepo::open(&path)?.merge_bases(&parent, &head)?;
                match bases.as_slice() {
                    [base] => Ok((id, base.clone())),
                    [] => Err(G5Error::DiffGraphUnavailable {
                        reason: "no_common_ancestor".into(),
                        missing_repo_count: 1,
                    }),
                    _ => Err(G5Error::DiffGraphUnavailable {
                        reason: "multiple_merge_bases".into(),
                        missing_repo_count: 1,
                    }),
                }
            })
            .collect::<Result<BTreeMap<_, _>, G5Error>>()
    })
    .await
    .map_err(|error| G5Error::Internal {
        message: format!("Comparison task failed: {error}"),
    })?
}

/// Compare an authorized captured source cut with its immediate parent's
/// current refs. Base has no parent comparison. Missing or inaccessible
/// working parents never silently select a repository default.
pub async fn parent_comparison_bases(
    conn: &mut PgConnection,
    storage: &ScopedRepoStorage,
    source: &g5_contracts::types::CommittedBindingSnapshot,
    actor_id: Uuid,
) -> Result<Option<std::collections::BTreeMap<Uuid, String>>, G5Error> {
    let is_base = sqlx::query_scalar::<_, bool>(
        "SELECT is_base FROM workspaces WHERE id = $1 AND org_id = $2 AND deleted_at IS NULL",
    )
    .bind(source.workspace_id)
    .bind(source.org_id)
    .fetch_one(&mut *conn)
    .await?;
    if is_base {
        return Ok(None);
    }
    let parent_id = readable_parent_workspace(conn, source.workspace_id, actor_id).await?;
    let parent = load_workspace_repositories(conn, parent_id, false).await?;
    let selected = parent
        .iter()
        .map(|repo| (repo.repo_id, repo.branch_name.clone()))
        .collect::<Vec<_>>();
    let mut heads = crate::services::git::workspace_reader::read_branch_head_oids(
        storage,
        source.org_id,
        &selected,
    )
    .await?;
    let parent_heads = parent
        .into_iter()
        .map(|repo| {
            let head = heads
                .remove(&(repo.repo_id, repo.branch_name))
                .flatten()
                .ok_or_else(|| G5Error::DiffGraphUnavailable {
                    reason: "missing_ref".into(),
                    missing_repo_count: 1,
                })?;
            Ok((repo.repo_id, head))
        })
        .collect::<Result<std::collections::BTreeMap<_, _>, G5Error>>()?;
    let source_heads = source
        .repos
        .iter()
        .map(|repo| (repo.repo_id, repo.head_commit.clone()))
        .collect();
    comparison_bases(storage, source.org_id, &source_heads, &parent_heads)
        .await
        .map(Some)
}

/// A ref observation for one operation. Never persisted as workspace metadata.
pub struct WorkspaceRepositoryVersion {
    pub member: WorkspaceRepository,
    pub head_commit: String,
    pub committed_at: chrono::DateTime<chrono::Utc>,
}

/// Capture each Git ref once, retaining its immutable commit and timestamp.
/// Consumers must fence mutation against this vector before publishing results.
pub async fn read_repository_versions(
    storage: &ScopedRepoStorage,
    members: Vec<WorkspaceRepository>,
) -> Result<Vec<WorkspaceRepositoryVersion>, G5Error> {
    let inputs = members
        .into_iter()
        .map(|member| {
            let path = storage.repo_path(member.org_id, member.repo_id)?;
            Ok((member, path))
        })
        .collect::<Result<Vec<_>, G5Error>>()?;
    tokio::task::spawn_blocking(move || {
        inputs
            .into_iter()
            .map(|(member, path)| {
                let repo = g5_scm::GitRepo::open(&path)?;
                let head_commit = repo.resolve_branch_head(&member.branch_name)?;
                let committed_at =
                    chrono::DateTime::from_timestamp(repo.commit_committed_at(&head_commit)?, 0)
                        .ok_or_else(|| G5Error::Internal {
                            message: "Invalid Git commit timestamp".into(),
                        })?;
                Ok(WorkspaceRepositoryVersion {
                    member,
                    head_commit,
                    committed_at,
                })
            })
            .collect()
    })
    .await
    .map_err(|error| G5Error::Internal {
        message: format!("Git ref capture failed: {error}"),
    })?
}
