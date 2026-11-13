//! Read-only ontology overview. Source authority stays with Files and All changes.
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

use axum::{
    extract::{Path, State},
    routing::get,
    Json, Router,
};
use serde::Serialize;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use super::{
    audited::AuditedRouter, ontology_files::current_ontology_files,
    source_control_change_graph::WorkspacePathParams,
    workspaces::read_workspace_file_for_actor_bounded, ResponseEnvelope,
};
use crate::{
    db::scoped::scoped_db_for_workspace,
    errors::AppError,
    middleware::{AuthContext, AuthenticatedUser},
    services::{
        source_control_change_graph_service::OntologyComparisonRepository,
        workspace_reader::{CommittedRepoReadView, FILE_TREE_LIMITS},
        AppState,
    },
};
use g5_auth::Scope;
use g5_contracts::{error::G5Error, types::HistoricalReadReason};

const MAX_FILES: usize = 500;
const MAX_BYTES: usize = 100 * 1024 * 1024;
const MAX_FILE_BYTES: usize = 10 * 1024 * 1024;

pub(super) fn router() -> Router<Arc<AppState>> {
    AuditedRouter::new()
        .audited_route(
            "/ontology-changes",
            get(get_ontology_changes),
            &[Scope::WorkspacesRead],
        )
        .into_router()
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct OntologyChanges {
    revision: String,
    repositories: Vec<OntologyComparisonRepository>,
    files: Vec<OntologyChangedFile>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct OntologyChangedFile {
    repo_id: Uuid,
    path: String,
    source: String,
    current: Option<String>,
    baseline: Option<String>,
}

fn unavailable(message: &str) -> AppError {
    AppError::from_error(G5Error::ValidationError {
        details: serde_json::json!({"message": message}),
    })
}

fn check_budget(files: usize, bytes: usize) -> Result<(), AppError> {
    if files > MAX_FILES || bytes > MAX_BYTES {
        return Err(unavailable("Ontology comparison exceeds 500 files or 100 MiB of input. No complete total is available."));
    }
    Ok(())
}

async fn get_ontology_changes(
    State(state): State<Arc<AppState>>,
    user: AuthenticatedUser,
    auth: AuthContext,
    Path(params): Path<WorkspacePathParams>,
) -> Result<Json<ResponseEnvelope<OntologyChanges>>, AppError> {
    let workspace_id = params.workspace_id;
    let mut db = scoped_db_for_workspace(&state, &auth, workspace_id, &["workspaces:read"]).await?;
    let repositories = state
        .source_control_change_graph_service
        .ontology_comparison_repositories(&mut db, workspace_id)
        .await?;
    db.commit().await.map_err(AppError::from)?;
    let runtime = state.workspace_runtime.get(workspace_id).await?;
    let runtime_identity = runtime
        .as_ref()
        .map(|r| (r.vm_id, r.owner_user_id, r.status.clone()));
    let (nodes, discovery) = current_ontology_files(&state, &auth, workspace_id).await?;
    if discovery.iter().any(|r| r.incomplete || r.notice.is_some()) {
        return Err(unavailable("Ontology discovery is incomplete or using a fallback. Retry when all workspace files are available."));
    }
    // A missing live ontology directory is an authoritative empty listing, but
    // Files cannot attach a HEAD to that response. There are no current bodies
    // to read in that repository; retain the inventory/runtime/ref rechecks below.
    for repo in &repositories {
        let listed = discovery
            .iter()
            .find(|d| d.repo_id == repo.repo_id.to_string())
            .ok_or_else(|| unavailable("Workspace repository membership changed. Retry."))?;
        let empty_live_ontology = listed.source.as_deref() == Some("live")
            && !nodes.iter().any(|node| node.repo_id == repo.repo_id);
        if (listed.repo_oid.is_none() && !empty_live_ontology)
            || (listed.source.as_deref() != Some("live")
                && listed.repo_oid.as_deref() != Some(repo.source_commit.as_str()))
        {
            return Err(unavailable(
                "Workspace revisions are updating. Retry after the workspace synchronizes.",
            ));
        }
    }
    let current_paths = nodes
        .iter()
        .map(|file| (file.repo_id, file.path.clone()))
        .collect::<BTreeSet<_>>();
    let mut files = Vec::new();
    let mut qualifying_repos = BTreeSet::new();
    let mut observed_files = 0;
    let mut observed_bytes = 0;
    let mut hash = Sha256::new();
    hash.update(
        serde_json::to_vec(&repositories)
            .map_err(|_| unavailable("Cannot encode comparison revisions"))?,
    );
    let mut historical_budget = FILE_TREE_LIMITS;
    for repo in &repositories {
        let baseline_view = CommittedRepoReadView::from_key(
            repo.key,
            &repo.baseline_commit,
            HistoricalReadReason::Diff,
        )
        .map_err(AppError::from_error)?;
        let listed = match baseline_view
            .list_tree_bounded(
                &state.repo_storage,
                ".g5/ontology",
                u32::MAX,
                historical_budget,
            )
            .await
        {
            Ok(listed) => listed,
            // The read owner distinguishes an absent subtree from missing Git history.
            Err(G5Error::NotFound { resource })
                if resource
                    == format!("Tree '.g5/ontology' at commit '{}'", repo.baseline_commit) =>
            {
                g5_scm::TreeListing {
                    entries: Vec::new(),
                    truncated: false,
                    bytes: 0,
                }
            }
            Err(error) => return Err(AppError::from_error(error)),
        };
        if listed.truncated {
            return Err(AppError::from_error(G5Error::TreeTooLarge {
                path: ".g5/ontology (historical workspace)".into(),
                max_entries: FILE_TREE_LIMITS.max_entries,
                max_bytes: FILE_TREE_LIMITS.max_bytes,
            }));
        }
        historical_budget.max_entries = historical_budget
            .max_entries
            .saturating_sub(listed.entries.len());
        historical_budget.max_bytes = historical_budget.max_bytes.saturating_sub(listed.bytes);
        let baseline_paths = listed
            .entries
            .into_iter()
            .filter(|entry| {
                matches!(entry.entry_type, g5_scm::TreeEntryType::File)
                    && g5_contracts::g5_paths::is_ontology_path(&entry.path)
            })
            .map(|entry| entry.path)
            .collect::<Vec<_>>();
        let metadata = baseline_view
            .file_metadata(&state.repo_storage, &baseline_paths)
            .await?;
        let mut paths = metadata
            .iter()
            .filter(|(_, entry)| entry.is_some())
            .map(|(path, _)| (path.clone(), false))
            .collect::<BTreeMap<_, _>>();
        for (repo_id, path) in &current_paths {
            if *repo_id == repo.repo_id {
                paths.insert(path.clone(), true);
            }
        }
        if !paths.is_empty() {
            qualifying_repos.insert(repo.repo_id);
        }
        observed_files += paths.len();
        check_budget(observed_files, observed_bytes)?;
        for (path, current_file) in paths {
            let baseline_size = metadata
                .get(&path)
                .and_then(|m| m.as_ref())
                .map(|(_, size)| *size);
            if baseline_size.unwrap_or_default() > MAX_FILE_BYTES {
                return Err(unavailable(&format!(
                    "{path}: comparison input exceeds 10 MiB."
                )));
            }
            check_budget(
                observed_files,
                observed_bytes + baseline_size.unwrap_or_default(),
            )?;
            let baseline = if baseline_size.is_some() {
                Some(
                    String::from_utf8(
                        baseline_view
                            .read_file_bounded(&state.repo_storage, &path, MAX_FILE_BYTES)
                            .await?,
                    )
                    .map_err(|_| {
                        unavailable(&format!("{path}: historical content is not UTF-8."))
                    })?,
                )
            } else {
                None
            };
            let source = discovery
                .iter()
                .find(|r| r.repo_id == repo.repo_id.to_string())
                .and_then(|r| r.source.clone())
                .unwrap_or_default();
            let current = if current_file {
                let file = read_workspace_file_for_actor_bounded(
                    &state,
                    &user,
                    &auth,
                    workspace_id,
                    repo.repo_id,
                    &path,
                    MAX_FILE_BYTES
                        .saturating_sub(baseline_size.unwrap_or_default())
                        .min(
                            MAX_BYTES
                                .saturating_sub(observed_bytes + baseline_size.unwrap_or_default()),
                        ),
                )
                .await?;
                if file.source != source
                    || (source != "live"
                        && file.repo_oid.as_deref() != Some(repo.source_commit.as_str()))
                    || (source == "live" && file.vm_id != runtime.as_ref().map(|r| r.vm_id))
                {
                    return Err(unavailable(
                        "Workspace source changed during comparison. Retry.",
                    ));
                }
                if file.encoding != "utf-8" {
                    return Err(unavailable(&format!(
                        "{path}: current content is not UTF-8."
                    )));
                }
                Some(file.content)
            } else {
                None
            };
            let bytes =
                baseline.as_ref().map_or(0, String::len) + current.as_ref().map_or(0, String::len);
            if bytes > MAX_FILE_BYTES {
                return Err(unavailable(&format!(
                    "{path}: combined comparison input exceeds 10 MiB."
                )));
            }
            observed_bytes += bytes;
            check_budget(observed_files, observed_bytes)?;
            hash.update(repo.repo_id.as_bytes());
            hash.update((path.len() as u64).to_le_bytes());
            hash.update(path.as_bytes());
            hash.update([u8::from(current.is_some())]);
            hash.update((current.as_ref().map_or(0, String::len) as u64).to_le_bytes());
            hash.update(current.as_deref().unwrap_or_default().as_bytes());
            if current != baseline {
                files.push(OntologyChangedFile {
                    repo_id: repo.repo_id,
                    path,
                    source,
                    current,
                    baseline,
                });
            }
        }
    }
    // Do not publish a result assembled across repository/cut/incarnation changes.
    let (latest_nodes, latest_discovery) =
        current_ontology_files(&state, &auth, workspace_id).await?;
    let latest_paths = latest_nodes
        .iter()
        .map(|file| (file.repo_id, file.path.clone()))
        .collect::<BTreeSet<_>>();
    let latest_runtime = state.workspace_runtime.get(workspace_id).await?;
    if current_paths != latest_paths
        || latest_discovery
            .iter()
            .any(|d| d.incomplete || d.notice.is_some())
        || discovery
            .iter()
            .map(|d| (&d.repo_id, &d.repo_oid, &d.source))
            .collect::<Vec<_>>()
            != latest_discovery
                .iter()
                .map(|d| (&d.repo_id, &d.repo_oid, &d.source))
                .collect::<Vec<_>>()
        || runtime_identity != latest_runtime.map(|r| (r.vm_id, r.owner_user_id, r.status))
    {
        return Err(unavailable(
            "Workspace files changed during discovery. Retry.",
        ));
    }
    let mut db = scoped_db_for_workspace(&state, &auth, workspace_id, &["workspaces:read"]).await?;
    let latest = state
        .source_control_change_graph_service
        .ontology_comparison_repositories(&mut db, workspace_id)
        .await?;
    db.commit().await.map_err(AppError::from)?;
    if latest != repositories {
        return Err(unavailable("Comparison revisions changed. Retry."));
    }
    Ok(Json(ResponseEnvelope::ok(OntologyChanges {
        revision: format!("{:x}", hash.finalize()),
        repositories: repositories
            .into_iter()
            .filter(|repo| qualifying_repos.contains(&repo.repo_id))
            .collect(),
        files,
    })))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn refuses_incomplete_totals_past_each_independent_budget() {
        assert!(check_budget(MAX_FILES, MAX_BYTES).is_ok());
        assert!(check_budget(MAX_FILES + 1, 1).is_err());
        assert!(check_budget(1, MAX_BYTES + 1).is_err());
    }
}
