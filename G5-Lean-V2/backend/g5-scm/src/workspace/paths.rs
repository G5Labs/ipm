//! Backend-free workspace path routing.

use g5_contracts::G5Error;
use std::cmp::Reverse;
use uuid::Uuid;

/// Repo-local entry under a product-level workspace overlay.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceOverlayRepoEntry {
    pub repo_id: Uuid,
    pub root_path: String,
}

/// Result of resolving a workspace-relative path to a repo entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedWorkspacePath<'a> {
    pub owner: &'a WorkspaceOverlayRepoEntry,
    pub repo_relative_path: String,
}

fn normalize_workspace_path(path: &str) -> Result<String, G5Error> {
    let normalized = path.replace('\\', "/");
    let mut parts = Vec::new();

    for part in normalized.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                if parts.pop().is_none() {
                    return Err(G5Error::ValidationError {
                        details: serde_json::json!({
                            "message": "workspace path escapes the workspace root",
                            "path": path,
                        }),
                    });
                }
            }
            segment => parts.push(segment),
        }
    }

    Ok(parts.join("/"))
}

fn normalize_root_path(path: &str) -> Result<String, G5Error> {
    Ok(normalize_workspace_path(path)?
        .trim_end_matches('/')
        .to_string())
}

fn path_matches_root(path: &str, root: &str) -> bool {
    root.is_empty()
        || path == root
        || path
            .strip_prefix(root)
            .is_some_and(|rest| rest.starts_with('/'))
}

fn path_without_root(path: &str, root: &str) -> String {
    if root.is_empty() {
        path.to_string()
    } else if path == root {
        String::new()
    } else {
        path.strip_prefix(root)
            .and_then(|rest| rest.strip_prefix('/'))
            .unwrap_or(path)
            .to_string()
    }
}

/// Resolves a workspace-relative file path to its owning repo entry.
///
/// The longest matching `root_path` wins. Equal-length matches are ambiguous
/// and fail rather than silently choosing one repo entry.
pub fn resolve_path_owner<'a>(
    entries: &'a [WorkspaceOverlayRepoEntry],
    path: &str,
) -> Result<ResolvedWorkspacePath<'a>, G5Error> {
    let normalized_path = normalize_workspace_path(path)?;
    let mut matches: Vec<(&WorkspaceOverlayRepoEntry, String)> = entries
        .iter()
        .map(|entry| normalize_root_path(&entry.root_path).map(|root| (entry, root)))
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .filter_map(|(entry, root)| {
            path_matches_root(&normalized_path, &root).then_some((entry, root))
        })
        .collect();

    matches.sort_by_key(|(_, root)| Reverse(root.len()));

    let Some((owner, root)) = matches.first() else {
        return Err(G5Error::NotFound {
            resource: format!("workspace path owner for '{normalized_path}'"),
        });
    };

    if matches
        .get(1)
        .is_some_and(|(_, other_root)| other_root.len() == root.len())
    {
        return Err(G5Error::Conflict {
            reason: format!("ambiguous workspace path owner for '{normalized_path}'"),
        });
    }

    Ok(ResolvedWorkspacePath {
        owner,
        repo_relative_path: path_without_root(&normalized_path, root),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn overlay_entry(repo_id: Uuid, root_path: &str) -> WorkspaceOverlayRepoEntry {
        WorkspaceOverlayRepoEntry {
            repo_id,
            root_path: root_path.to_string(),
        }
    }

    #[test]
    fn resolve_path_owner_prefers_longest_folder_prefix() {
        let docs_repo_id = Uuid::new_v4();
        let code_repo_id = Uuid::new_v4();
        let entries = vec![
            overlay_entry(code_repo_id, ""),
            overlay_entry(docs_repo_id, "docs"),
        ];

        let resolved = resolve_path_owner(&entries, "docs/plan.md").unwrap();

        assert_eq!(resolved.owner.repo_id, docs_repo_id);
        assert_eq!(resolved.repo_relative_path, "plan.md");
    }

    #[test]
    fn resolve_path_owner_rejects_equal_length_ambiguity() {
        let entries = vec![
            overlay_entry(Uuid::new_v4(), "docs"),
            overlay_entry(Uuid::new_v4(), "docs/"),
        ];

        let err = resolve_path_owner(&entries, "docs/plan.md").unwrap_err();

        assert!(matches!(err, G5Error::Conflict { .. }));
    }

    #[test]
    fn resolve_path_owner_rejects_workspace_escape() {
        let entries = vec![overlay_entry(Uuid::new_v4(), "")];

        let err = resolve_path_owner(&entries, "../docs/plan.md").unwrap_err();

        assert!(matches!(err, G5Error::ValidationError { .. }));
    }
}
