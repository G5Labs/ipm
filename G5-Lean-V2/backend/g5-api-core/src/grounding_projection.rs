//! Route-neutral overlay and JSONL projection helpers shared by API layers.

use crate::services::git::repo_store::TenantRepoStorage;
use std::collections::{BTreeMap, HashSet};

use g5_contracts::error::G5Error;
use g5_scm::TreeEntryType;
use tracing::warn;

use crate::services::workspace_reader::{RepoFileRead, RepoReadView};

/// List the effective grounding JSONL corpus from one captured repository
/// view. The committed tree and dirty membership belong to the same request
/// snapshot, so multi-file scans cannot mix authority cuts or persistence
/// generations.
pub async fn list_grounding_jsonl_files_from_view(
    repo_storage: &TenantRepoStorage,
    repo_view: &RepoReadView,
) -> Vec<String> {
    let mut paths = match repo_view
        .list_committed_tree(repo_storage, "intent_grounding", 32)
        .await
    {
        Ok(entries) => entries
            .into_iter()
            .filter(|entry| {
                matches!(entry.entry_type, TreeEntryType::File)
                    && entry.path.starts_with("intent_grounding/")
                    && entry.path.ends_with(".jsonl")
            })
            .map(|entry| entry.path)
            .collect::<HashSet<_>>(),
        Err(G5Error::NotFound { .. }) => HashSet::new(),
        Err(error) => {
            warn!(
                repo_id = %repo_view.repo.repo_id,
                commit = %repo_view.repo.head_commit,
                error = %error,
                "Unable to list committed grounding corpus"
            );
            HashSet::new()
        }
    };

    let dirty_paths = repo_view
        .dirty_files()
        .iter()
        .filter(|path| path.starts_with("intent_grounding/") && path.ends_with(".jsonl"))
        .cloned()
        .collect::<Vec<_>>();
    match repo_view.read_files(repo_storage, &dirty_paths).await {
        Ok(effective) => {
            for path in dirty_paths {
                match effective.get(&path) {
                    Some(RepoFileRead::Content { .. }) => {
                        paths.insert(path);
                    }
                    Some(RepoFileRead::Deleted | RepoFileRead::Missing) => {
                        paths.remove(&path);
                    }
                    None => {}
                }
            }
        }
        Err(error) => {
            warn!(
                repo_id = %repo_view.repo.repo_id,
                commit = %repo_view.repo.head_commit,
                error = %error,
                "Unable to read captured grounding overlay corpus"
            );
        }
    }

    let mut paths = paths.into_iter().collect::<Vec<_>>();
    paths.sort();
    paths
}

/// Read and parse a JSONL corpus through one captured repository view.
pub async fn read_jsonl_files_from_view<T: serde::de::DeserializeOwned>(
    repo_storage: &TenantRepoStorage,
    repo_view: &RepoReadView,
    paths: &[String],
) -> Result<BTreeMap<String, Vec<T>>, G5Error> {
    let mut reads = repo_view.read_files(repo_storage, paths).await?;
    Ok(paths
        .iter()
        .cloned()
        .map(|path| {
            let records = match reads.remove(&path) {
                Some(RepoFileRead::Content { bytes, .. }) => String::from_utf8(bytes)
                    .ok()
                    .map(|content| parse_jsonl(&content, &path))
                    .unwrap_or_default(),
                Some(RepoFileRead::Deleted | RepoFileRead::Missing) | None => Vec::new(),
            };
            (path, records)
        })
        .collect())
}

pub async fn read_jsonl_from_view<T: serde::de::DeserializeOwned>(
    repo_storage: &TenantRepoStorage,
    repo_view: &RepoReadView,
    path: &str,
) -> Vec<T> {
    read_jsonl_files_from_view(repo_storage, repo_view, &[path.to_string()])
        .await
        .ok()
        .and_then(|mut files| files.remove(path))
        .unwrap_or_default()
}

fn parse_jsonl<T: serde::de::DeserializeOwned>(content: &str, path: &str) -> Vec<T> {
    let mut records = Vec::new();
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        match serde_json::from_str::<T>(trimmed) {
            Ok(record) => records.push(record),
            Err(error) => {
                warn!(path, error = %error, "Skipping malformed JSONL line in grounding file");
            }
        }
    }
    records
}

pub fn format_chat_content(chat_context: Option<&str>, content: Option<&str>) -> Option<String> {
    let content = content.unwrap_or_default();
    if content.is_empty() {
        return None;
    }
    match chat_context {
        Some(context) if !context.is_empty() => Some(format!(
            "Chat context: {context}\n---------\nUser input: {content}"
        )),
        _ => Some(format!("User input: {content}")),
    }
}
