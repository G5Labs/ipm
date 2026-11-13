//! Saved workspace content for Diff Graph's All changes (`g5.diff-graph.modes`).
//!
//! The owner of a running workspace VM sees saved, uncommitted edits: every
//! live ontology page, and each `git status` path when the VM's HEAD is the
//! published branch head. A repository whose HEAD differs (unpublished local
//! commits) or whose status is unavailable shows committed content for other
//! files, and the graph says so. Readers and stopped VMs see committed heads
//! (`g5.workspace.durable-read`).
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

use base64::Engine;
use g5_contracts::{error::G5Error, g5_paths::is_ontology_path, types::HistoricalReadReason};
use uuid::Uuid;

use super::{
    ontology_files::current_ontology_files, workspaces::read_workspace_file_for_actor_bounded,
};
use crate::{
    db::scoped::scoped_db_for_workspace,
    errors::AppError,
    middleware::{AuthContext, AuthenticatedUser},
    services::{
        source_control_change_graph_service::{
            OntologyComparisonRepository, RepositoryLiveOverlay, WorkspaceLiveContent,
        },
        workspace_reader::{CommittedRepoReadView, FILE_TREE_LIMITS},
        AppState,
    },
};

/// Live reads per graph, bounded like the All changes overview (500 ontology
/// files) and the graph's comparison input (500 changed files, 100 MiB), and
/// checked from the listings before any file is read.
const MAX_LIVE_ONTOLOGY_FILES: usize = 500;
const MAX_LIVE_CHANGED_FILES: usize = 500;
const MAX_LIVE_BYTES: usize = 100 * 1024 * 1024;
const MAX_LIVE_FILE_BYTES: usize = 10 * 1024 * 1024;

pub(crate) struct LiveCapture {
    pub content: WorkspaceLiveContent,
    identity: Option<(Uuid, u64)>,
    repositories: Vec<OntologyComparisonRepository>,
}

fn changed_during_read() -> AppError {
    AppError::from_error(G5Error::Conflict {
        reason: "Workspace files changed while the Diff Graph was read. Retry.".into(),
    })
}

fn too_large(files: usize, max_files: usize, bytes: usize) -> AppError {
    AppError::from_error(G5Error::DiffGraphTooLarge {
        observed_roots: files,
        max_roots: max_files,
        observed_bytes: bytes,
        max_bytes: MAX_LIVE_BYTES,
    })
}

fn committed(reason: &str) -> LiveCapture {
    LiveCapture {
        content: WorkspaceLiveContent {
            repositories: Vec::new(),
            committed_reason: Some(reason.to_string()),
        },
        identity: None,
        repositories: Vec::new(),
    }
}

/// Preflight from the listings: refuse before reading rather than truncate.
fn preflight(ontology_pages: usize, changed_paths: usize) -> Result<(), AppError> {
    if ontology_pages > MAX_LIVE_ONTOLOGY_FILES {
        return Err(too_large(ontology_pages, MAX_LIVE_ONTOLOGY_FILES, 0));
    }
    if changed_paths > MAX_LIVE_CHANGED_FILES {
        return Err(too_large(changed_paths, MAX_LIVE_CHANGED_FILES, 0));
    }
    Ok(())
}

struct ByteBudget(usize);

impl ByteBudget {
    fn add(&mut self, bytes: usize) -> Result<(), AppError> {
        self.0 = self.0.saturating_add(bytes);
        if self.0 > MAX_LIVE_BYTES {
            return Err(too_large(0, MAX_LIVE_CHANGED_FILES, self.0));
        }
        Ok(())
    }
}

/// Why non-ontology files show committed content when the VM's HEAD is not the
/// published head: behind (being synchronized) or ahead/diverged (local
/// commits not yet published).
fn head_mismatch_reason(member: &serde_json::Value) -> &'static str {
    let ahead = member["against_internal"]["ahead"].as_u64();
    let behind = member["against_internal"]["behind"].as_u64();
    match (ahead, behind) {
        (Some(0), Some(behind)) if behind > 0 => "workspace_synchronizing",
        _ => "unpublished_commits",
    }
}

enum LiveRead {
    Content(Vec<u8>),
    /// The path vanished between the status listing and the read.
    Missing,
    Failed,
}

/// Capture saved content for every workspace repository, or the reason the
/// graph shows committed content only. Ontology pages are read in full whether
/// or not the Changes report is available, matching All changes.
pub(crate) async fn capture(
    state: &Arc<AppState>,
    user: &AuthenticatedUser,
    auth: &AuthContext,
    workspace_id: Uuid,
) -> Result<LiveCapture, AppError> {
    let Some(handle) = state
        .workspace_runtime
        .get(workspace_id)
        .await?
        .filter(|handle| handle.status == "running")
    else {
        return Ok(committed("vm_not_running"));
    };
    if handle.owner_user_id != user.user_uuid() {
        return Ok(committed("not_owner"));
    }
    let mut db = scoped_db_for_workspace(state, auth, workspace_id, &["workspaces:read"]).await?;
    let repositories = state
        .source_control_change_graph_service
        .ontology_comparison_repositories(&mut db, workspace_id)
        .await;
    let scope = db.reopener();
    db.commit().await.map_err(AppError::from)?;
    // Without a comparison baseline the projection reports its own refusal.
    let Ok(repositories) = repositories else {
        return Ok(committed("report_unavailable"));
    };
    let report = state
        .workspace_runtime
        .workspace_changes_report(&scope, workspace_id, user.user_uuid())
        .await
        .ok()
        .filter(|report| {
            report["source"] == "live"
                && report["vm_id"].as_str() == Some(handle.vm_id.to_string().as_str())
        });
    let member_of = |repo: &OntologyComparisonRepository| {
        report.as_ref().and_then(|report| {
            report["members"]
                .as_array()
                .into_iter()
                .flatten()
                .find(|member| {
                    member["repo_id"].as_str() == Some(repo.repo_id.to_string().as_str())
                })
        })
    };
    let (ontology_pages, discovery) = current_ontology_files(state, auth, workspace_id).await?;
    let changed_paths = repositories
        .iter()
        .filter_map(|repo| {
            member_of(repo).filter(|m| m["head_oid"].as_str() == Some(repo.source_commit.as_str()))
        })
        .filter_map(|member| member["dirty"].as_array())
        .map(Vec::len)
        .sum();
    preflight(ontology_pages.len(), changed_paths)?;

    let read = |repo_id: Uuid, path: String| {
        let state = state.clone();
        async move {
            let file = match read_workspace_file_for_actor_bounded(
                &state,
                user,
                auth,
                workspace_id,
                repo_id,
                &path,
                MAX_LIVE_FILE_BYTES,
            )
            .await
            {
                Ok(file) => file,
                Err(error) if matches!(error.inner, G5Error::NotFound { .. }) => {
                    return Ok(LiveRead::Missing)
                }
                Err(_) => return Ok(LiveRead::Failed),
            };
            if file.source != "live" || file.vm_id != Some(handle.vm_id) {
                return Err(changed_during_read());
            }
            Ok(match file.encoding.as_str() {
                "base64" => base64::engine::general_purpose::STANDARD
                    .decode(file.content.as_bytes())
                    .map_or(LiveRead::Failed, LiveRead::Content),
                _ => LiveRead::Content(file.content.into_bytes()),
            })
        }
    };

    let mut bytes = ByteBudget(0);
    let mut overlays = Vec::with_capacity(repositories.len());
    for repo in &repositories {
        let ontology_listed = discovery.iter().any(|listed| {
            listed.repo_id == repo.repo_id.to_string()
                && !listed.incomplete
                && listed.notice.is_none()
                && listed.source.as_deref() == Some("live")
        });
        if !ontology_listed {
            overlays.push(RepositoryLiveOverlay {
                repo_id: repo.repo_id,
                expected_head: repo.source_commit.clone(),
                files: BTreeMap::new(),
                includes_uncommitted: false,
                ontology_live: false,
                reason: Some("report_unavailable".into()),
            });
            continue;
        }

        // Ontology: every live page, and pages at the published head that are
        // gone from the working tree. A page that cannot be read refuses the
        // graph, as All changes does, rather than silently showing its commit.
        let mut files = BTreeMap::new();
        let live_pages: BTreeSet<String> = ontology_pages
            .iter()
            .filter(|page| page.repo_id == repo.repo_id)
            .map(|page| page.path.clone())
            .collect();
        for path in &live_pages {
            let content = match read(repo.repo_id, path.clone()).await? {
                LiveRead::Content(content) => Some(content),
                LiveRead::Missing => None,
                LiveRead::Failed => {
                    return Err(AppError::from_error(G5Error::Conflict {
                        reason: format!("{path} could not be read from the workspace. Retry."),
                    }))
                }
            };
            bytes.add(content.as_ref().map_or(0, Vec::len))?;
            files.insert(path.clone(), content);
        }
        for path in head_ontology_pages(state, repo).await? {
            files.entry(path).or_insert(None);
        }

        let member = member_of(repo);
        let head_matches =
            member.is_some_and(|m| m["head_oid"].as_str() == Some(repo.source_commit.as_str()));
        let reason = match (member, head_matches) {
            (None, _) => Some("report_unavailable"),
            (Some(member), false) => Some(head_mismatch_reason(member)),
            (Some(member), true) => match member["dirty"].as_array() {
                None => Some("status_unavailable"),
                Some(entries) => {
                    let mut changed = BTreeMap::new();
                    let mut failed = false;
                    for entry in entries {
                        let Some(path) = entry["path"].as_str() else {
                            continue;
                        };
                        if let Some(from) = entry["from"].as_str() {
                            if !is_ontology_path(from) {
                                changed.entry(from.to_string()).or_insert(None);
                            }
                        }
                        if is_ontology_path(path) || changed.contains_key(path) {
                            continue;
                        }
                        let deleted = entry["status"]
                            .as_str()
                            .is_some_and(|status| status.contains('D'));
                        let content = if deleted {
                            None
                        } else {
                            match read(repo.repo_id, path.to_string()).await? {
                                LiveRead::Content(content) => Some(content),
                                LiveRead::Missing => None,
                                LiveRead::Failed => {
                                    failed = true;
                                    break;
                                }
                            }
                        };
                        bytes.add(content.as_ref().map_or(0, Vec::len))?;
                        changed.insert(path.to_string(), content);
                    }
                    if failed {
                        // One unreadable saved file degrades this repository's
                        // other files to committed content, said so.
                        Some("report_unavailable")
                    } else {
                        files.extend(changed);
                        None
                    }
                }
            },
        };
        overlays.push(RepositoryLiveOverlay {
            repo_id: repo.repo_id,
            expected_head: repo.source_commit.clone(),
            files,
            includes_uncommitted: reason.is_none(),
            ontology_live: true,
            reason: reason.map(str::to_string),
        });
    }
    Ok(LiveCapture {
        content: WorkspaceLiveContent {
            repositories: overlays,
            committed_reason: None,
        },
        identity: Some((handle.vm_id, handle.epoch)),
        repositories,
    })
}

/// Ontology pages at the published head, so a page deleted only in the working
/// tree is absent from the overlay rather than silently kept.
async fn head_ontology_pages(
    state: &Arc<AppState>,
    repo: &OntologyComparisonRepository,
) -> Result<Vec<String>, AppError> {
    let view =
        CommittedRepoReadView::from_key(repo.key, &repo.source_commit, HistoricalReadReason::Diff)
            .map_err(AppError::from_error)?;
    let listed = match view
        .list_tree_bounded(
            &state.repo_storage,
            ".g5/ontology",
            u32::MAX,
            FILE_TREE_LIMITS,
        )
        .await
    {
        Ok(listed) => listed,
        Err(G5Error::NotFound { .. }) => return Ok(Vec::new()),
        Err(error) => return Err(AppError::from_error(error)),
    };
    if listed.truncated {
        return Err(AppError::from_error(G5Error::TreeTooLarge {
            path: ".g5/ontology".into(),
            max_entries: FILE_TREE_LIMITS.max_entries,
            max_bytes: FILE_TREE_LIMITS.max_bytes,
        }));
    }
    Ok(listed
        .entries
        .into_iter()
        .filter(|entry| matches!(entry.entry_type, g5_scm::TreeEntryType::File))
        .map(|entry| entry.path)
        .filter(|path| is_ontology_path(path))
        .collect())
}

impl LiveCapture {
    /// Refuse a graph assembled across a VM incarnation or published-head change.
    pub(crate) async fn recheck(
        &self,
        state: &Arc<AppState>,
        auth: &AuthContext,
        workspace_id: Uuid,
    ) -> Result<(), AppError> {
        let Some(identity) = self.identity else {
            return Ok(());
        };
        let latest = state
            .workspace_runtime
            .get(workspace_id)
            .await?
            .map(|handle| (handle.vm_id, handle.epoch));
        let mut db =
            scoped_db_for_workspace(state, auth, workspace_id, &["workspaces:read"]).await?;
        let repositories = state
            .source_control_change_graph_service
            .ontology_comparison_repositories(&mut db, workspace_id)
            .await?;
        db.commit().await.map_err(AppError::from)?;
        if latest != Some(identity) || repositories != self.repositories {
            return Err(changed_during_read());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn live_reads_refuse_past_each_budget_before_or_while_reading() {
        assert!(preflight(MAX_LIVE_ONTOLOGY_FILES, MAX_LIVE_CHANGED_FILES).is_ok());
        assert!(
            preflight(MAX_LIVE_ONTOLOGY_FILES + 1, 0).is_err(),
            "one page over"
        );
        assert!(
            preflight(0, MAX_LIVE_CHANGED_FILES + 1).is_err(),
            "one changed path over"
        );
        let mut bytes = ByteBudget(0);
        bytes.add(MAX_LIVE_BYTES).unwrap();
        assert!(bytes.add(1).is_err(), "one byte over");
    }

    #[test]
    fn a_vm_behind_the_published_head_is_synchronizing_not_unpublished() {
        let member = |ahead: u64, behind: u64| json!({"against_internal": {"ahead": ahead, "behind": behind}});
        assert_eq!(
            head_mismatch_reason(&member(0, 2)),
            "workspace_synchronizing"
        );
        assert_eq!(head_mismatch_reason(&member(1, 0)), "unpublished_commits");
        assert_eq!(head_mismatch_reason(&member(1, 1)), "unpublished_commits");
        assert_eq!(head_mismatch_reason(&json!({})), "unpublished_commits");
    }
}
