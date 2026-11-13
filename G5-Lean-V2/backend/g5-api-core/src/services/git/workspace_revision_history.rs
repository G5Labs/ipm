//! Complete published-vector observations. Git remains head authority.
use super::workspace_reader::resolve_published_snapshot;
use crate::services::git::repo_store::{ScopedRepoStorage, TenantRepoStorage};
use chrono::{DateTime, Utc};
use g5_contracts::{
    error::G5Error,
    types::{AuthorityTarget, MergeSourceHead},
};
use serde::{Deserialize, Serialize};
use sqlx::{Connection, PgConnection};
use std::collections::BTreeMap;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct Revision {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub observed_at: DateTime<Utc>,
    pub heads: serde_json::Value,
    pub pushed_at: serde_json::Value,
}

pub async fn published_heads(
    db: &mut g5_authz::ScopedDb,
    storage: &TenantRepoStorage,
    workspace_id: Uuid,
) -> Result<Vec<MergeSourceHead>, G5Error> {
    let snapshot =
        resolve_published_snapshot(db, storage, AuthorityTarget::Workspace { workspace_id })
            .await?;
    Ok(snapshot
        .repos
        .into_iter()
        .map(|r| MergeSourceHead {
            repo_id: r.repo_id.to_string(),
            workspace_id: workspace_id.to_string(),
            branch_name: r.branch_name,
            head_commit: r.head_commit,
        })
        .collect())
}

/// Observation failures never gate an accepted publication or completed fork.
/// A savepoint also isolates SQL errors from the caller's domain transaction.
pub async fn observe(
    conn: &mut PgConnection,
    storage: &ScopedRepoStorage,
    org_id: Uuid,
    workspace_id: Uuid,
    heads: Vec<MergeSourceHead>,
    observed_at: DateTime<Utc>,
    changed: &[String],
) {
    let result: Result<(), G5Error> = async {
        let mut evidence = conn.begin().await?;
        match record(
            &mut evidence,
            storage,
            org_id,
            workspace_id,
            heads,
            observed_at,
            changed,
        )
        .await
        {
            Ok(()) => {
                evidence.commit().await?;
                Ok(())
            }
            Err(error) => {
                evidence.rollback().await?;
                Err(error)
            }
        }
    }
    .await;
    if let Err(error) = result {
        tracing::warn!(%error, %workspace_id, "Published workspace cut has no retained revision observation");
    }
}

/// The caller holds the workspace fence and supplies the full accepted cut.
/// Only changed members get a new push time. Equal retries do not add rows.
pub async fn record(
    conn: &mut PgConnection,
    storage: &ScopedRepoStorage,
    org_id: Uuid,
    workspace_id: Uuid,
    mut heads: Vec<MergeSourceHead>,
    observed_at: DateTime<Utc>,
    changed: &[String],
) -> Result<(), G5Error> {
    heads.sort_by(|a, b| a.repo_id.cmp(&b.repo_id));
    let value = serde_json::to_value(&heads).map_err(|e| G5Error::Internal {
        message: e.to_string(),
    })?;
    let previous: Option<Revision> = sqlx::query_as("SELECT id, workspace_id, observed_at, heads, pushed_at FROM workspace_revision_events WHERE workspace_id=$1 ORDER BY observed_at DESC,id DESC LIMIT 1")
        .bind(workspace_id).fetch_optional(&mut *conn).await?;
    if previous.as_ref().is_some_and(|p| p.heads == value) {
        return Ok(());
    }
    for head in &heads {
        let repo_id = Uuid::parse_str(&head.repo_id).map_err(|_| G5Error::Internal {
            message: "Review repository id is not a UUID".into(),
        })?;
        let path = storage.repo_path(org_id, repo_id)?;
        let sha = head.head_commit.clone();
        tokio::task::spawn_blocking(move || {
            g5_scm::GitRepo::open(&path)?.retain_review_commit(workspace_id, &sha)
        })
        .await
        .map_err(|e| G5Error::Internal {
            message: format!("Retain review evidence: {e}"),
        })??;
    }
    let mut times = BTreeMap::<String, DateTime<Utc>>::new();
    if let Some(p) = previous {
        let old: Vec<MergeSourceHead> =
            serde_json::from_value(p.heads).map_err(|error| G5Error::Internal {
                message: format!("Invalid recorded revision heads: {error}"),
            })?;
        let prior: BTreeMap<String, DateTime<Utc>> =
            serde_json::from_value(p.pushed_at).map_err(|error| G5Error::Internal {
                message: format!("Invalid recorded push times: {error}"),
            })?;
        for h in &heads {
            if old
                .iter()
                .any(|o| o.repo_id == h.repo_id && o.head_commit == h.head_commit)
            {
                if let Some(t) = prior.get(&h.repo_id) {
                    times.insert(h.repo_id.clone(), *t);
                }
            }
        }
    }
    for id in changed {
        times.insert(id.clone(), observed_at);
    }
    sqlx::query("INSERT INTO workspace_revision_events(org_id,workspace_id,observed_at,heads,pushed_at) VALUES($1,$2,$3,$4,$5)")
        .bind(org_id).bind(workspace_id).bind(observed_at).bind(value).bind(serde_json::json!(times)).execute(conn).await?;
    Ok(())
}

/// Exact fork evidence, not a merge base that moves when the parent advances.
pub async fn saved_base(
    conn: &mut PgConnection,
    workspace_id: Uuid,
) -> Result<Option<Vec<MergeSourceHead>>, G5Error> {
    let manifest: Option<serde_json::Value> = sqlx::query_scalar("SELECT repo_manifest FROM workspace_forks WHERE target_workspace_id=$1 AND status='completed' ORDER BY created_at,id LIMIT 1")
        .bind(workspace_id).fetch_optional(conn).await?;
    manifest
        .map(|v| {
            v.as_array()
                .ok_or_else(|| G5Error::Internal {
                    message: "Invalid fork manifest".into(),
                })?
                .iter()
                .map(|m| {
                    let field = |k: &str| {
                        m[k].as_str()
                            .map(str::to_owned)
                            .ok_or_else(|| G5Error::Internal {
                                message: format!("Missing fork {k}"),
                            })
                    };
                    Ok(MergeSourceHead {
                        repo_id: field("repo_id")?,
                        workspace_id: workspace_id.to_string(),
                        branch_name: field("target_branch")?,
                        head_commit: field("source_oid")?,
                    })
                })
                .collect()
        })
        .transpose()
}

pub fn same_heads(a: &[MergeSourceHead], b: &[MergeSourceHead]) -> bool {
    let map = |hs: &[MergeSourceHead]| {
        hs.iter()
            .map(|h| (h.repo_id.clone(), h.head_commit.clone()))
            .collect::<BTreeMap<_, _>>()
    };
    // Include membership cardinality so duplicate repository entries cannot pass.
    a.len() == b.len()
        && a.iter()
            .map(|h| &h.repo_id)
            .collect::<std::collections::BTreeSet<_>>()
            .len()
            == a.len()
        && map(a) == map(b)
}

/// Pushed source/parent cuts for the shared comparison. Parent authorization is
/// required even for retained historical cuts. No post-merge target is a base.
pub async fn comparison_cuts(
    db: &mut g5_authz::ScopedDb,
    system_pool: &sqlx::PgPool,
    storage: &TenantRepoStorage,
    workspace_id: Uuid,
    actor_id: Uuid,
    pull_request_id: Option<Uuid>,
) -> Result<(Vec<MergeSourceHead>, Option<Vec<MergeSourceHead>>), G5Error> {
    let storage = &storage.scoped(&*db);
    let initial = available_published_heads(db.conn(), storage, workspace_id).await?;
    let recorded: Option<(String, Uuid, serde_json::Value, serde_json::Value)> = sqlx::query_as(
        "SELECT status, target_workspace_id, source_heads, preflight FROM workspace_pull_requests WHERE source_workspace_id=$1 AND ($2::uuid IS NULL OR id=$2) ORDER BY created_at DESC,id DESC LIMIT 1")
        .bind(workspace_id).bind(pull_request_id).fetch_optional(db.conn()).await?;
    let mut historical = None;
    if let Some((status, target, source, preflight)) = recorded {
        let source: Vec<MergeSourceHead> =
            serde_json::from_value(source).map_err(|e| G5Error::Internal {
                message: format!("Recorded source cut unavailable: {e}"),
            })?;
        if status == "merged" && (pull_request_id.is_some() || same_heads(&source, &initial)) {
            historical = Some((target, source, preflight));
        } else if pull_request_id.is_some() && status != "open" {
            return Ok((source, None));
        }
    } else if pull_request_id.is_some() {
        return Ok((initial, None));
    }
    let parent = if let Some((target, _, _)) = &historical {
        let org = db
            .primary_org_id()
            .map_err(|e| G5Error::Internal {
                message: e.to_string(),
            })?
            .into_uuid();
        if crate::db::scoped::user_can_access_workspace(system_pool, db, actor_id, org, *target)
            .await
            .map_err(|e| e.inner)?
        {
            Some(*target)
        } else {
            None
        }
    } else {
        readable_comparison_parent(db, system_pool, workspace_id, actor_id).await?
    };
    let mut locks = vec![workspace_id];
    locks.extend(parent);
    locks.sort();
    locks.dedup();
    for id in locks {
        crate::services::branch_commit::acquire_workspace_read_fence(db.conn(), id).await?;
    }
    let current = available_published_heads(db.conn(), storage, workspace_id).await?;
    if !same_heads(&initial, &current) {
        return Err(G5Error::Conflict {
            reason: "Pushed heads changed during comparison capture; retry".into(),
        });
    }
    let Some(parent) = parent else {
        return Ok((historical.map(|(_, s, _)| s).unwrap_or(current), None));
    };
    if let Some((target, source, preflight)) = historical {
        let Ok(preflight) = serde_json::from_value::<
            crate::services::workspace_merge_service::MergePreflightResponse,
        >(preflight) else {
            return Ok((source, None));
        };
        if preflight
            .repo_results
            .iter()
            .any(|r| r.target_workspace_id != target)
        {
            return Ok((source, None));
        }
        let parents = preflight
            .repo_results
            .into_iter()
            .map(|r| MergeSourceHead {
                repo_id: r.repo_id.to_string(),
                workspace_id: r.target_workspace_id.to_string(),
                branch_name: r.target_branch_name,
                head_commit: r.target_head,
            })
            .collect();
        return Ok((source, Some(parents)));
    }
    Ok((
        current,
        Some(available_published_heads(db.conn(), storage, parent).await?),
    ))
}

/// A prior captured pushed head is authorized only by this workspace's retained
/// observations/PR evidence. Object existence alone grants no read authority.
pub async fn recorded_head(
    conn: &mut PgConnection,
    workspace: Uuid,
    repo: Uuid,
    sha: &str,
) -> Result<bool, G5Error> {
    let head = serde_json::json!([{"repoId":repo,"headCommit":sha}]);
    Ok(sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM workspace_revision_events WHERE workspace_id=$1 AND heads @> $2) OR EXISTS(SELECT 1 FROM workspace_pull_requests WHERE source_workspace_id=$1 AND source_heads @> $2)")
        .bind(workspace).bind(head).fetch_one(conn).await?)
}

/// Source Control and formal approvals compare with the configured merge target;
/// Diff Graph deliberately retains its separate lineage-parent selection.
pub async fn readable_comparison_parent(
    db: &mut g5_authz::ScopedDb,
    system_pool: &sqlx::PgPool,
    workspace: Uuid,
    actor: Uuid,
) -> Result<Option<Uuid>, G5Error> {
    let parent:Option<Uuid>=sqlx::query_scalar("SELECT COALESCE(merge_target_workspace_id,cloned_from_id) FROM workspaces WHERE id=$1 AND deleted_at IS NULL").bind(workspace).fetch_one(db.conn()).await?;
    let Some(parent) = parent else {
        return Ok(None);
    };
    let org = db
        .primary_org_id()
        .map_err(|e| G5Error::Internal {
            message: e.to_string(),
        })?
        .into_uuid();
    Ok(
        if crate::db::scoped::user_can_access_workspace(system_pool, db, actor, org, parent)
            .await
            .map_err(|e| e.inner)?
        {
            Some(parent)
        } else {
            None
        },
    )
}

/// UI comparisons degrade per repository. Approval capture still uses the
/// complete published snapshot and refuses missing members operationally.
async fn available_published_heads(
    conn: &mut PgConnection,
    storage: &ScopedRepoStorage,
    workspace: Uuid,
) -> Result<Vec<MergeSourceHead>, G5Error> {
    let members =
        super::workspace_repositories::load_workspace_repositories(conn, workspace, false).await?;
    let inputs = members
        .into_iter()
        .map(|member| {
            (
                storage.repo_path(member.org_id, member.repo_id).ok(),
                member,
            )
        })
        .collect::<Vec<_>>();
    tokio::task::spawn_blocking(move || {
        inputs
            .into_iter()
            .filter_map(|(path, member)| {
                let path = path?;
                let head = g5_scm::GitRepo::open(&path)
                    .ok()?
                    .read_branch_oid(&member.branch_name)
                    .ok()??;
                Some(MergeSourceHead {
                    repo_id: member.repo_id.to_string(),
                    workspace_id: workspace.to_string(),
                    branch_name: member.branch_name,
                    head_commit: head.to_string(),
                })
            })
            .collect()
    })
    .await
    .map_err(|e| G5Error::Internal {
        message: e.to_string(),
    })
}
