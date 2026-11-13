//! Bounded current ontology file inventory for comparison. No node identities or storage.
use crate::{
    db::scoped::scoped_db_for_workspace, errors::AppError, middleware::AuthContext,
    services::AppState,
};
use g5_contracts::error::G5Error;
use serde::Serialize;
use std::sync::Arc;
use uuid::Uuid;

pub(crate) struct OntologyFile {
    pub repo_id: Uuid,
    pub path: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct OntologyRepositoryDiscovery {
    pub repo_id: String,
    pub repo_name: String,
    pub source: Option<String>,
    pub repo_oid: Option<String>,
    pub incomplete: bool,
    pub notice: Option<String>,
}

/// Enumerate through the same actor-aware boundary as Files. No document bytes
/// or legacy projection rows are loaded to discover current ontology.
pub(crate) async fn current_ontology_files(
    state: &Arc<AppState>,
    auth: &AuthContext,
    workspace_id: Uuid,
) -> Result<(Vec<OntologyFile>, Vec<OntologyRepositoryDiscovery>), AppError> {
    use crate::routes::files::{
        read_file_tree_for_actor, BranchPathParams, FileTreeEntry, FileTreeQuery,
    };
    use crate::services::workspace_reader::{
        resolve_committed_snapshot, FILE_TREE_BYTE_CAP, FILE_TREE_ENTRY_CAP,
    };
    use g5_contracts::types::AuthorityTarget;
    let mut db = scoped_db_for_workspace(state, auth, workspace_id, &["workspaces:read"]).await?;
    let snapshot = resolve_committed_snapshot(
        &mut db,
        &state.repo_storage,
        AuthorityTarget::Workspace { workspace_id },
    )
    .await?;
    db.commit().await.map_err(AppError::from)?;
    let mut nodes = Vec::new();
    let mut discovery = Vec::new();
    let mut remaining_entries = FILE_TREE_ENTRY_CAP;
    let mut remaining_bytes = FILE_TREE_BYTE_CAP;
    fn collect(
        entries: &[FileTreeEntry],
        paths: &mut Vec<String>,
        count: &mut usize,
        bytes: &mut usize,
    ) -> bool {
        for entry in entries {
            let cost = g5_scm::entry_bytes(&entry.path);
            if *count == 0 || cost > *bytes {
                return false;
            }
            *count -= 1;
            *bytes -= cost;
            if entry.entry_type == "file" && g5_contracts::g5_paths::is_ontology_path(&entry.path) {
                paths.push(entry.path.clone());
            }
            if let Some(children) = &entry.children {
                if !collect(children, paths, count, bytes) {
                    return false;
                }
            }
        }
        true
    }
    for repo in snapshot.repos {
        let mut status = OntologyRepositoryDiscovery {
            repo_id: repo.repo_id.to_string(),
            repo_name: repo.repo_name.clone(),
            source: None,
            repo_oid: None,
            incomplete: false,
            notice: None,
        };
        if remaining_entries == 0 || remaining_bytes == 0 {
            status.incomplete = true;
            status.notice = Some("Ontology discovery reached the workspace listing limit".into());
            discovery.push(status);
            continue;
        }
        let listed = read_file_tree_for_actor(
            state,
            auth,
            BranchPathParams {
                workspace_id,
                repo_id: repo.repo_id,
            },
            FileTreeQuery {
                path: ".g5/ontology".into(),
                depth: -1,
                task_id: None,
            },
            true,
        )
        .await;
        match listed {
            Ok(listed) => {
                let listed = listed.0.data;
                status.source = Some(listed.source);
                status.repo_oid = listed.repo_oid;
                let mut paths = Vec::new();
                let complete = collect(
                    &listed.entries,
                    &mut paths,
                    &mut remaining_entries,
                    &mut remaining_bytes,
                );
                status.incomplete = listed.truncated || !complete;
                if !complete {
                    remaining_entries = 0;
                }
                status.notice = if status.incomplete {
                    Some("Ontology listing is incomplete".into())
                } else {
                    listed
                        .live_fallback
                        .map(|why| format!("Showing committed ontology ({why})"))
                };
                nodes.extend(paths.into_iter().map(|path| OntologyFile {
                    repo_id: repo.repo_id,
                    path,
                }));
            }
            Err(error) => {
                // Authorization failures must invalidate the old view, not become empty members.
                if matches!(
                    error.inner,
                    G5Error::Unauthorized | G5Error::Forbidden { .. }
                ) {
                    return Err(error);
                }
                status.incomplete = true;
                status.notice = Some("Could not read this repository's ontology".into());
            }
        }
        discovery.push(status);
    }
    Ok((nodes, discovery))
}
