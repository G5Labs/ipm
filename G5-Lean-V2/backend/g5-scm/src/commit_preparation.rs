//! Unattached commit preparation with immutable inputs and private indexes.

use std::collections::BTreeSet;
use std::path::{Component, Path, PathBuf};

use crate::{GitAuthor, GitRepo};
use g5_contracts::{g5_paths::is_g5_runtime_metadata_path, G5Error};

/// Request for an exact-files commit object prepared from one immutable base.
///
/// This request never names or advances a
/// canonical branch. The caller may attach the returned object only to an
/// operation-scoped staging ref before authority finalization.
pub struct ExactBaseFilesCommit<'a> {
    pub base_commit: &'a str,
    pub files: &'a [(String, Vec<u8>)],
    pub message: &'a str,
    pub author: &'a GitAuthor,
}

/// One exact path change for immutable tree preparation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExactTreeChange {
    /// Store `content` as the exact blob bytes at `path`.
    Write { path: String, content: Vec<u8> },
    /// Remove the tracked entry at `path`.
    Delete { path: String },
}

/// Request for an unattached commit constructed directly from an exact base tree.
///
/// This operation never creates a branch, worktree, checkout, or mutable index
/// file. It is suitable for authority preparation where the caller will protect
/// the returned object with an operation-scoped ref before finalization.
pub struct ExactBaseTreeCommit<'a> {
    pub base_commit: &'a str,
    pub changes: &'a [ExactTreeChange],
    pub message: &'a str,
    pub author: &'a GitAuthor,
}

/// Request for an unattached parentless commit containing exact files.
///
/// This is reserved for first materialization where no database-authoritative
/// Git parent exists. Ambient refs and checkout state are deliberately absent
/// from the request.
pub struct InitialFilesCommit<'a> {
    pub files: &'a [(String, Vec<u8>)],
    pub message: &'a str,
    pub author: &'a GitAuthor,
}

type ValidatedIsolatedFile<'a> = (String, PathBuf, &'a [u8]);

fn validate_repo_relative_path(file_path: &str) -> Result<&Path, G5Error> {
    let trimmed = file_path.trim();
    if trimmed.is_empty() {
        return Err(G5Error::ValidationError {
            details: serde_json::json!({
                "path": "file path must not be empty"
            }),
        });
    }

    let path = Path::new(trimmed);
    if path.is_absolute()
        || path.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return Err(G5Error::ValidationError {
            details: serde_json::json!({
                "path": "file path must be relative and must not contain parent traversal"
            }),
        });
    }

    Ok(path)
}

fn validate_exact_tree_path(file_path: &str) -> Result<PathBuf, G5Error> {
    let path = validate_repo_relative_path(file_path)?;
    if file_path != file_path.trim()
        || file_path.contains('\0')
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(G5Error::ValidationError {
            details: serde_json::json!({
                "path": file_path,
                "reason": "exact tree path must be canonical repository-relative UTF-8"
            }),
        });
    }
    let canonical = path
        .components()
        .map(|component| component.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/");
    if canonical != file_path
        || path.components().any(|component| {
            component
                .as_os_str()
                .to_str()
                .is_some_and(|part| part.eq_ignore_ascii_case(".git"))
        })
    {
        return Err(G5Error::ValidationError {
            details: serde_json::json!({
                "path": file_path,
                "reason": "exact tree path is noncanonical or targets .git metadata"
            }),
        });
    }
    if is_g5_runtime_metadata_path(path) {
        return Err(G5Error::ValidationError {
            details: serde_json::json!({
                "path": file_path,
                "reason": "exact tree path targets non-durable .g5 runtime metadata"
            }),
        });
    }
    Ok(path.to_path_buf())
}

fn validate_exact_tree_changes(
    changes: &[ExactTreeChange],
) -> Result<Vec<(PathBuf, &ExactTreeChange)>, G5Error> {
    if changes.is_empty() {
        return Err(G5Error::ValidationError {
            details: serde_json::json!({
                "changes": "exact tree commit request must include at least one change"
            }),
        });
    }

    let paths = validate_exact_tree_paths(changes.iter().map(|change| match change {
        ExactTreeChange::Write { path, .. } | ExactTreeChange::Delete { path } => path.as_str(),
    }))?;
    Ok(paths.into_iter().zip(changes).collect())
}

fn validate_exact_tree_paths<'a>(
    selected: impl IntoIterator<Item = &'a str>,
) -> Result<Vec<PathBuf>, G5Error> {
    let mut validated = Vec::new();
    let mut paths = BTreeSet::new();
    for raw_path in selected {
        let path = validate_exact_tree_path(raw_path)?;
        if !paths.insert(raw_path) {
            return Err(G5Error::ValidationError {
                details: serde_json::json!({
                    "path": raw_path,
                    "reason": "duplicate exact tree path"
                }),
            });
        }
        validated.push(path);
    }

    let sorted_paths: Vec<&str> = paths.into_iter().collect();
    for (index, ancestor) in sorted_paths.iter().enumerate() {
        for descendant in sorted_paths.iter().skip(index + 1) {
            if descendant
                .strip_prefix(*ancestor)
                .is_some_and(|suffix| suffix.starts_with('/'))
            {
                return Err(G5Error::ValidationError {
                    details: serde_json::json!({
                        "path": descendant,
                        "ancestor": ancestor,
                        "reason": "exact tree changes must not contain ancestor collisions"
                    }),
                });
            }
        }
    }
    Ok(validated)
}

fn supported_blob_mode(path: &str, entry: &git2::IndexEntry) -> Result<u32, G5Error> {
    match entry.mode {
        0o100644 | 0o100755 | 0o120000 => Ok(entry.mode),
        mode => Err(G5Error::ValidationError {
            details: serde_json::json!({
                "path": path,
                "mode": format!("{mode:o}"),
                "reason": "selected exact tree entry has an unsupported git mode"
            }),
        }),
    }
}

fn ensure_no_base_tree_collision(base_tree: &git2::Tree<'_>, path: &Path) -> Result<(), G5Error> {
    if let Ok(entry) = base_tree.get_path(path) {
        if entry.kind() == Some(git2::ObjectType::Tree) {
            return Err(G5Error::ValidationError {
                details: serde_json::json!({
                    "path": path.to_string_lossy(),
                    "reason": "selected exact tree path resolves to a tree"
                }),
            });
        }
    }

    let components: Vec<_> = path.components().collect();
    let mut ancestor = PathBuf::new();
    for component in components.iter().take(components.len().saturating_sub(1)) {
        ancestor.push(component.as_os_str());
        if let Ok(entry) = base_tree.get_path(&ancestor) {
            if entry.kind() != Some(git2::ObjectType::Tree) {
                return Err(G5Error::ValidationError {
                    details: serde_json::json!({
                        "path": path.to_string_lossy(),
                        "ancestor": ancestor.to_string_lossy(),
                        "reason": "selected exact tree path descends from a non-tree entry"
                    }),
                });
            }
        }
    }
    Ok(())
}

fn validate_isolated_files(
    files: &[(String, Vec<u8>)],
) -> Result<Vec<ValidatedIsolatedFile<'_>>, G5Error> {
    let mut seen_paths = BTreeSet::new();
    let mut validated_files = Vec::with_capacity(files.len());
    if files.is_empty() {
        return Err(G5Error::ValidationError {
            details: serde_json::json!({
                "files": "isolated commit request must include at least one file"
            }),
        });
    }
    for (file_path, content) in files {
        let trimmed = file_path.trim();
        if trimmed == ".g5" || trimmed.starts_with(".g5/") {
            return Err(G5Error::ValidationError {
                details: serde_json::json!({
                    "path": trimmed,
                    "reason": "isolated commit request cannot write internal .g5 metadata"
                }),
            });
        }
        if !seen_paths.insert(trimmed.to_string()) {
            return Err(G5Error::ValidationError {
                details: serde_json::json!({
                    "path": trimmed,
                    "reason": "duplicate file path in isolated commit request"
                }),
            });
        }
        let relative_path = validate_repo_relative_path(trimmed)?.to_path_buf();
        validated_files.push((trimmed.to_string(), relative_path, content.as_slice()));
    }
    Ok(validated_files)
}

/// Prepare one exact-files commit object from an immutable base without moving
/// a canonical branch ref.
pub fn prepare_files_commit_from_exact_base(
    repo: &GitRepo,
    request: ExactBaseFilesCommit<'_>,
) -> Result<String, G5Error> {
    let ExactBaseFilesCommit {
        base_commit,
        files,
        message,
        author,
    } = request;
    validate_isolated_files(files)?;
    let changes: Vec<ExactTreeChange> = files
        .iter()
        .map(|(path, content)| ExactTreeChange::Write {
            path: path.clone(),
            content: content.clone(),
        })
        .collect();
    prepare_tree_commit_from_exact_base(
        repo,
        ExactBaseTreeCommit {
            base_commit,
            changes: &changes,
            message,
            author,
        },
    )
}

/// Prepare an unattached commit by applying exact changes to an immutable base tree.
///
/// The complete change set is validated before any object is written. The
/// operation uses an in-memory index and never creates or mutates a branch,
/// worktree, checkout, or on-disk index. If the resulting tree is identical to
/// the base tree, the base SHA is returned without creating an empty commit.
pub fn prepare_tree_commit_from_exact_base(
    repo: &GitRepo,
    request: ExactBaseTreeCommit<'_>,
) -> Result<String, G5Error> {
    let ExactBaseTreeCommit {
        base_commit,
        changes,
        message,
        author,
    } = request;
    let validated_changes = validate_exact_tree_changes(changes)?;

    let repo = repo.repo()?;
    let base_oid = git2::Oid::from_str(base_commit).map_err(|error| G5Error::GitError {
        message: format!("Invalid exact base commit '{base_commit}': {error}"),
    })?;
    let base = repo
        .find_commit(base_oid)
        .map_err(|error| G5Error::NotFound {
            resource: format!("exact base commit '{base_commit}': {error}"),
        })?;
    let base_tree = base.tree().map_err(crate::backend_error)?;
    let mut index = git2::Index::new().map_err(|error| G5Error::GitError {
        message: format!("Failed to create in-memory exact-base index: {error}"),
    })?;
    index
        .read_tree(&base_tree)
        .map_err(|error| G5Error::GitError {
            message: format!("Failed to read exact base tree '{base_commit}': {error}"),
        })?;

    for (path, change) in &validated_changes {
        ensure_no_base_tree_collision(&base_tree, path)?;
        let path_label = path.to_string_lossy();
        match change {
            ExactTreeChange::Write { .. } => {
                if let Some(entry) = index.get_path(path, 0) {
                    supported_blob_mode(&path_label, &entry)?;
                }
            }
            ExactTreeChange::Delete { .. } => {
                let entry = index
                    .get_path(path, 0)
                    .ok_or_else(|| G5Error::ValidationError {
                        details: serde_json::json!({
                            "path": path_label,
                            "reason": "exact tree deletion target is not tracked at the base"
                        }),
                    })?;
                supported_blob_mode(&path_label, &entry)?;
            }
        }
    }

    for (path, change) in validated_changes {
        let path_label = path.to_string_lossy();
        match change {
            ExactTreeChange::Write { content, .. } => {
                let mode = index.get_path(&path, 0).map_or(Ok(0o100644), |entry| {
                    supported_blob_mode(&path_label, &entry)
                })?;
                let blob_oid = repo.blob(content).map_err(|error| G5Error::GitError {
                    message: format!("Failed to store exact blob '{path_label}': {error}"),
                })?;
                let entry = git2::IndexEntry {
                    ctime: git2::IndexTime::new(0, 0),
                    mtime: git2::IndexTime::new(0, 0),
                    dev: 0,
                    ino: 0,
                    mode,
                    uid: 0,
                    gid: 0,
                    file_size: content.len().try_into().unwrap_or(u32::MAX),
                    id: blob_oid,
                    flags: 0,
                    flags_extended: 0,
                    path: path_label.as_bytes().to_vec(),
                };
                index.add(&entry).map_err(|error| G5Error::GitError {
                    message: format!("Failed to add exact tree entry '{path_label}': {error}"),
                })?;
            }
            ExactTreeChange::Delete { .. } => {
                index
                    .remove_path(&path)
                    .map_err(|error| G5Error::GitError {
                        message: format!(
                            "Failed to remove exact tree entry '{path_label}': {error}"
                        ),
                    })?;
            }
        }
    }

    let tree_oid = index
        .write_tree_to(&repo)
        .map_err(|error| G5Error::GitError {
            message: format!("Failed to write exact-base commit tree: {error}"),
        })?;
    if tree_oid == base.tree_id() {
        return Ok(base_commit.to_string());
    }

    let tree = repo.find_tree(tree_oid).map_err(crate::backend_error)?;
    let signature =
        git2::Signature::now(&author.name, &author.email).map_err(crate::backend_error)?;
    let commit_oid = repo
        .commit(None, &signature, &signature, message, &tree, &[&base])
        .map_err(|error| G5Error::GitError {
            message: format!("Failed to create exact-base commit: {error}"),
        })?;
    Ok(commit_oid.to_string())
}

/// Prepare one parentless exact-files commit object without moving any ref.
pub fn prepare_initial_files_commit(
    repo: &GitRepo,
    request: InitialFilesCommit<'_>,
) -> Result<String, G5Error> {
    let InitialFilesCommit {
        files,
        message,
        author,
    } = request;
    let validated_files = validate_isolated_files(files)?;
    // Preserve the legacy initial-file trimming and .g5 restriction, then validate
    // the complete normalized path set before writing any object.
    validate_exact_tree_paths(validated_files.iter().map(|(path, _, _)| path.as_str()))?;
    let repo = repo.repo()?;
    let mut index = git2::Index::new().map_err(|error| G5Error::GitError {
        message: format!("Failed to create initial commit index: {error}"),
    })?;
    for (file_path, _, content) in validated_files {
        let blob_oid = repo.blob(content).map_err(|error| G5Error::GitError {
            message: format!("Failed to store initial file '{file_path}': {error}"),
        })?;
        let entry = git2::IndexEntry {
            ctime: git2::IndexTime::new(0, 0),
            mtime: git2::IndexTime::new(0, 0),
            dev: 0,
            ino: 0,
            mode: 0o100644,
            uid: 0,
            gid: 0,
            file_size: content.len().try_into().unwrap_or(u32::MAX),
            id: blob_oid,
            flags: 0,
            flags_extended: 0,
            path: file_path.as_bytes().to_vec(),
        };
        index.add(&entry).map_err(|error| G5Error::GitError {
            message: format!("Failed to add initial file '{file_path}' to index: {error}"),
        })?;
    }
    commit_parentless_index(&repo, &mut index, message, author)
}

/// Prepare one parentless empty commit object without moving any ref.
pub fn prepare_initial_empty_commit(
    repo: &GitRepo,
    message: &str,
    author: &GitAuthor,
) -> Result<String, G5Error> {
    let repo = repo.repo()?;
    let mut index = git2::Index::new().map_err(|error| G5Error::GitError {
        message: format!("Failed to create empty initial commit index: {error}"),
    })?;
    commit_parentless_index(&repo, &mut index, message, author)
}

fn commit_parentless_index(
    repo: &git2::Repository,
    index: &mut git2::Index,
    message: &str,
    author: &GitAuthor,
) -> Result<String, G5Error> {
    let tree_oid = index
        .write_tree_to(repo)
        .map_err(|error| G5Error::GitError {
            message: format!("Failed to write initial commit tree: {error}"),
        })?;
    let tree = repo.find_tree(tree_oid).map_err(crate::backend_error)?;
    let signature =
        git2::Signature::now(&author.name, &author.email).map_err(crate::backend_error)?;
    let commit_oid = repo
        .commit(None, &signature, &signature, message, &tree, &[])
        .map_err(|error| G5Error::GitError {
            message: format!("Failed to create parentless initial commit: {error}"),
        })?;
    Ok(commit_oid.to_string())
}

#[cfg(test)]
mod tests {
    use super::validate_exact_tree_path;

    // Durable `.g5` content (`g5_paths::is_durable_g5_repo_content_path`)
    // is accepted by exact-tree commits; runtime metadata is refused. The
    // workbench root (workbench g5.workbench.actions) is `actions.yaml` plus
    // one level of `personas/<name>.md`.
    #[test]
    fn exact_tree_accepts_durable_g5_content_and_refuses_runtime_metadata() {
        for path in [
            ".g5/workspace.yaml",
            ".g5/pr_ontology_diff_design/topic/ontology-plan.md",
            ".g5/pr_ontology_diff_design/topic/screenshots/nested/files.png",
            ".g5/preview_profiles/browser.yaml",
            ".g5/explainer_templates/my-style.yaml",
            ".g5/workbench/actions.yaml",
            ".g5/workbench/personas/be.md",
            "docs/ws/plan.md",
        ] {
            assert!(
                validate_exact_tree_path(path).is_ok(),
                "{path} should be accepted"
            );
        }
        for path in [
            ".g5/auth_state.json",
            ".g5/pr_ontology_diff_design/topic/.g5/state.json",
            ".g5/pr_ontology_diff_design_old/plan.md",
            ".g5/workbench/other.yaml",
            ".g5/workbench/personas/nested/be.md",
            ".g5/workbench/personas/be.txt",
        ] {
            let error = validate_exact_tree_path(path).expect_err(path);
            assert!(
                error
                    .to_string()
                    .contains("non-durable .g5 runtime metadata"),
                "{path}: {error}"
            );
        }
    }
}
