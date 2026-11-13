//! How current a stored G5 PR's merge evidence is (g5.governance.pr.read.freshness).
//!
//! Reads compare the target heads the preflight was computed against with the
//! target branches' current heads and never recompute the preflight. Only the
//! recorded branches are read, from the repository mirrors, so the comparison
//! reveals nothing about the target beyond whether it moved.

use crate::services::git::repo_store::ScopedRepoStorage;
use serde_json::{json, Value};
use uuid::Uuid;

use super::coordinated_repo_publication::WorkspacePullRequestSummary;

/// The `mergeEvidence` projection for an open PR, `None` otherwise. `stale` is
/// true when any recorded target branch has moved, false when none has, and
/// null when a target branch could not be read.
pub async fn merge_evidence(
    storage: ScopedRepoStorage,
    pull: &WorkspacePullRequestSummary,
) -> Option<Value> {
    if pull.status != "open" {
        return None;
    }
    let results = pull.preflight["repoResults"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let recorded: Vec<(Uuid, String, String)> = pull
        .target_heads
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|head| {
            Some((
                head["repoId"].as_str()?.parse().ok()?,
                head["branchName"].as_str()?.to_owned(),
                head["headCommit"].as_str()?.to_owned(),
            ))
        })
        .collect();
    let org_id = pull.org_id;
    let current = tokio::task::spawn_blocking(move || {
        recorded
            .into_iter()
            .map(|(repo_id, branch, head)| {
                let moved = storage
                    .repo_path(org_id, repo_id)
                    .ok()
                    .and_then(|path| g5_scm::GitRepo::open(&path).ok())
                    .and_then(|repo| repo.resolve_branch_head(&branch).ok())
                    .map(|now| now != head);
                (repo_id, branch, moved)
            })
            .collect::<Vec<_>>()
    })
    .await
    .unwrap_or_default();
    let stale = if current.is_empty() || current.iter().any(|(_, _, moved)| moved.is_none()) {
        Value::Null
    } else {
        json!(current.iter().any(|(_, _, moved)| *moved == Some(true)))
    };
    let repos: Vec<Value> = current
        .into_iter()
        .map(|(repo_id, branch, moved)| {
            let result = results
                .iter()
                .find(|r| r["repoId"].as_str() == Some(&repo_id.to_string()));
            json!({
                "repoId": repo_id,
                "targetBranch": branch,
                "targetMoved": moved,
                "outcome": result.map_or(Value::Null, |r| r["outcome"].clone()),
                "conflictPaths": result
                    .and_then(|r| r["conflictPaths"].as_array().cloned())
                    .unwrap_or_default(),
            })
        })
        .collect();
    Some(json!({
        "localMergeStatus": pull.local_merge_status,
        "stale": stale,
        "repos": repos,
    }))
}
