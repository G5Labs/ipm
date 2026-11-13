//! Local filesystem implementation of [`RepoStorage`].
//!
//! Uses a base directory for bare repos and a separate worktree directory
//! (local SSD) for checked-out working trees.

use std::path::{Component, Path, PathBuf};

use async_trait::async_trait;
use bytes::Bytes;

use super::{RepoKey, RepoStorage, StorageEntry, StorageError};

/// Local filesystem repo storage provider.
///
/// Suitable for dev and single-node deployments.
pub struct LocalRepoStorage {
    /// Base directory for bare repos (e.g. `/data/repos`).
    #[allow(dead_code)]
    base_dir: PathBuf,
    /// Canonicalized base — used for path traversal checks.
    canonical_base: PathBuf,
    /// Worktree root directory (local SSD).
    worktree_dir: PathBuf,
}

impl LocalRepoStorage {
    /// Create a new `LocalRepoStorage`.
    ///
    /// Both `base_dir` and `worktree_dir` will be created if they don't
    /// already exist.
    pub fn new(base_dir: PathBuf, worktree_dir: PathBuf) -> Result<Self, StorageError> {
        std::fs::create_dir_all(&base_dir)
            .map_err(|e| StorageError::Backend(format!("base_dir cannot be created: {e}")))?;
        let canonical_base = base_dir.canonicalize().map_err(|e| {
            StorageError::Backend(format!("base_dir is inaccessible after creation: {e}"))
        })?;
        std::fs::create_dir_all(&worktree_dir)
            .map_err(|e| StorageError::Backend(format!("worktree_dir cannot be created: {e}")))?;
        Ok(Self {
            base_dir,
            canonical_base,
            worktree_dir,
        })
    }

    fn repo_dir(&self, key: &RepoKey) -> PathBuf {
        self.canonical_base.join(key.relative())
    }

    fn validate_repo_path(&self, key: &RepoKey, path: &str) -> Result<PathBuf, StorageError> {
        self.validate_path(&key.relative().join(path))
    }

    /// Lexical path traversal validation.
    ///
    /// Rejects absolute paths and `..` components. Joins against the
    /// canonicalized root (set at construction time).
    fn validate_path(&self, rel_path: &Path) -> Result<PathBuf, StorageError> {
        if rel_path.is_absolute() {
            return Err(StorageError::PathTraversal(format!(
                "absolute path rejected: {}",
                rel_path.display()
            )));
        }

        for component in rel_path.components() {
            if matches!(component, Component::ParentDir) {
                return Err(StorageError::PathTraversal(format!(
                    "parent traversal rejected: {}",
                    rel_path.display()
                )));
            }
        }

        Ok(self.canonical_base.join(rel_path))
    }

    /// Recursively walk a directory up to `depth` levels.
    async fn walk_dir(
        &self,
        _root: &Path,
        target: &Path,
        depth: u32,
        include_hidden: bool,
    ) -> Result<Vec<StorageEntry>, StorageError> {
        if depth == 0 {
            return Ok(Vec::new());
        }

        let mut entries = Vec::new();
        let mut read_dir = tokio::fs::read_dir(target).await.map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                StorageError::NotFound(target.display().to_string())
            } else {
                StorageError::Io(e)
            }
        })?;

        while let Some(entry) = read_dir.next_entry().await? {
            let metadata = entry.metadata().await?;
            let name = entry.file_name().to_string_lossy().into_owned();

            let is_dir = metadata.is_dir();

            // Skip .git directory — internal git metadata should never appear
            // in the user-facing file tree.
            if name == ".git" && is_dir {
                continue;
            }

            // Skip hidden files/directories unless this is an internal system
            // traversal. `.git` is always excluded above.
            if !include_hidden && name.starts_with('.') {
                continue;
            }

            entries.push(StorageEntry {
                path: name.clone(),
                is_dir,
                size: if is_dir { None } else { Some(metadata.len()) },
                last_modified: metadata.modified().ok().map(chrono::DateTime::from),
            });

            if is_dir && depth > 1 {
                let sub_entries =
                    Box::pin(self.walk_dir(_root, &entry.path(), depth - 1, include_hidden))
                        .await?;
                for mut sub in sub_entries {
                    sub.path = format!("{name}/{}", sub.path);
                    entries.push(sub);
                }
            }
        }

        Ok(entries)
    }
}

#[async_trait]
impl RepoStorage for LocalRepoStorage {
    async fn list_tree(
        &self,
        key: &RepoKey,
        path: &str,
        depth: u32,
    ) -> Result<Vec<StorageEntry>, StorageError> {
        let target = self.validate_repo_path(key, path)?;
        let root = self.repo_dir(key);
        self.walk_dir(&root, &target, depth, false).await
    }

    async fn list_tree_including_hidden(
        &self,
        key: &RepoKey,
        path: &str,
        depth: u32,
    ) -> Result<Vec<StorageEntry>, StorageError> {
        let target = self.validate_repo_path(key, path)?;
        let root = self.repo_dir(key);
        self.walk_dir(&root, &target, depth, true).await
    }

    async fn read_file(&self, key: &RepoKey, path: &str) -> Result<Bytes, StorageError> {
        let full_path = self.validate_repo_path(key, path)?;
        let data = tokio::fs::read(&full_path).await.map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                StorageError::NotFound(full_path.display().to_string())
            } else {
                StorageError::Io(e)
            }
        })?;
        Ok(Bytes::from(data))
    }

    async fn write_file(
        &self,
        key: &RepoKey,
        path: &str,
        content: &[u8],
    ) -> Result<(), StorageError> {
        let full_path = self.validate_repo_path(key, path)?;
        if let Some(parent) = full_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        tokio::fs::write(&full_path, content).await?;
        Ok(())
    }

    async fn exists(&self, key: &RepoKey, path: &str) -> Result<bool, StorageError> {
        let full_path = self.validate_repo_path(key, path)?;
        Ok(tokio::fs::try_exists(&full_path).await?)
    }

    async fn delete_file(&self, key: &RepoKey, path: &str) -> Result<(), StorageError> {
        let full_path = self.validate_repo_path(key, path)?;
        tokio::fs::remove_file(&full_path).await.map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                StorageError::NotFound(full_path.display().to_string())
            } else {
                StorageError::Io(e)
            }
        })?;
        Ok(())
    }

    async fn file_size(&self, key: &RepoKey, path: &str) -> Result<u64, StorageError> {
        let full_path = self.validate_repo_path(key, path)?;
        let meta = tokio::fs::metadata(&full_path).await.map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                StorageError::NotFound(full_path.display().to_string())
            } else {
                StorageError::Io(e)
            }
        })?;
        Ok(meta.len())
    }

    fn local_path(&self, key: &RepoKey) -> Option<PathBuf> {
        Some(self.repo_dir(key))
    }

    fn review_artifact_dir(&self, key: &RepoKey) -> Option<PathBuf> {
        // A sibling of the organization's repository directories, never
        // inside one: the repository-path validation of every other read
        // confines it to `{base}/{org}/{repo}` and rejects `..`.
        Some(
            self.canonical_base
                .join(key.org_dir())
                .join(".review-artifacts")
                .join(key.repo_id.hyphenated().to_string()),
        )
    }

    fn worktree_root(&self) -> Option<PathBuf> {
        Some(self.worktree_dir.clone())
    }

    async fn init_repo(&self, key: &RepoKey) -> Result<PathBuf, StorageError> {
        let dir = self.repo_dir(key);
        tokio::fs::create_dir_all(&dir).await?;
        Ok(dir)
    }

    async fn remove_repo(&self, key: &RepoKey) -> Result<(), StorageError> {
        let dir = self.repo_dir(key);
        if tokio::fs::try_exists(&dir).await? {
            tokio::fs::remove_dir_all(&dir).await?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn key() -> RepoKey {
        RepoKey {
            org_id: Uuid::new_v4(),
            repo_id: Uuid::new_v4(),
        }
    }

    #[tokio::test]
    async fn local_storage_roundtrip() {
        let tmp = tempfile::tempdir().unwrap();
        let wt = tmp.path().join("worktrees");
        let base = tmp.path().join("repos");
        std::fs::create_dir_all(&base).unwrap();

        let storage = LocalRepoStorage::new(base, wt).unwrap();
        let repo = key();

        // Init repo
        storage.init_repo(&repo).await.unwrap();
        assert!(storage.exists(&repo, ".").await.unwrap());

        // Write + read
        storage
            .write_file(&repo, "hello.txt", b"world")
            .await
            .unwrap();
        let data = storage.read_file(&repo, "hello.txt").await.unwrap();
        assert_eq!(&data[..], b"world");

        // List tree
        let entries = storage.list_tree(&repo, ".", 1).await.unwrap();
        assert!(entries.iter().any(|e| e.path == "hello.txt"));

        // Delete
        storage.delete_file(&repo, "hello.txt").await.unwrap();
        assert!(!storage.exists(&repo, "hello.txt").await.unwrap());

        // Remove repo
        storage.remove_repo(&repo).await.unwrap();
    }

    #[tokio::test]
    async fn list_tree_including_hidden_keeps_dot_artifacts_but_not_git() {
        let tmp = tempfile::tempdir().unwrap();
        let wt = tmp.path().join("worktrees");
        let base = tmp.path().join("repos");
        std::fs::create_dir_all(&base).unwrap();

        let storage = LocalRepoStorage::new(base, wt).unwrap();
        let repo = key();
        storage.init_repo(&repo).await.unwrap();
        storage
            .write_file(&repo, ".semlift/specs/.github/workflow.semlift.md", b"spec")
            .await
            .unwrap();
        storage
            .write_file(&repo, ".git/config", b"internal")
            .await
            .unwrap();

        let visible = storage.list_tree(&repo, ".", 5).await.unwrap();
        assert!(visible.iter().all(|entry| !entry.path.starts_with('.')));

        let internal = storage
            .list_tree_including_hidden(&repo, ".", 5)
            .await
            .unwrap();
        assert!(internal
            .iter()
            .any(|entry| entry.path == ".semlift/specs/.github/workflow.semlift.md"));
        assert!(internal.iter().all(|entry| !entry.path.starts_with(".git")));
    }

    #[tokio::test]
    async fn rejects_path_traversal() {
        let tmp = tempfile::tempdir().unwrap();
        let wt = tmp.path().join("worktrees");
        let base = tmp.path().join("repos");
        std::fs::create_dir_all(&base).unwrap();

        let storage = LocalRepoStorage::new(base, wt).unwrap();
        let repo = key();

        let result = storage.read_file(&repo, "../../etc/passwd").await;
        assert!(matches!(result, Err(StorageError::PathTraversal(_))));

        let result = storage.read_file(&repo, "/etc/passwd").await;
        assert!(matches!(result, Err(StorageError::PathTraversal(_))));
    }

    #[tokio::test]
    async fn repositories_live_under_their_organization() {
        let tmp = tempfile::tempdir().unwrap();
        let base = tmp.path().join("repos");
        let storage = LocalRepoStorage::new(base.clone(), tmp.path().join("worktrees")).unwrap();
        let canonical = base.canonicalize().unwrap();
        let repo = key();

        let dir = storage.init_repo(&repo).await.unwrap();
        let expected = canonical
            .join(repo.org_id.to_string())
            .join(repo.repo_id.to_string());
        assert_eq!(dir, expected);
        assert!(dir.is_dir());
        assert_eq!(storage.local_path(&repo), Some(expected));
        assert_eq!(
            storage.review_artifact_dir(&repo),
            Some(
                canonical
                    .join(repo.org_id.to_string())
                    .join(".review-artifacts")
                    .join(repo.repo_id.to_string())
            )
        );

        // The same repository id under another organization is another,
        // absent directory.
        let elsewhere = RepoKey {
            org_id: Uuid::new_v4(),
            repo_id: repo.repo_id,
        };
        assert!(!storage.exists(&elsewhere, ".").await.unwrap());
        assert!(matches!(
            storage.list_tree(&elsewhere, ".", 1).await,
            Err(StorageError::NotFound(_))
        ));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn filesystem_probe_errors_are_not_reported_as_absence() {
        use std::os::unix::fs::symlink;

        let tmp = tempfile::tempdir().unwrap();
        let base = tmp.path().join("repos");
        let storage = LocalRepoStorage::new(base.clone(), tmp.path().join("worktrees")).unwrap();
        let repo = key();
        let repo_dir = storage.init_repo(&repo).await.unwrap();

        symlink("loop", repo_dir.join("loop")).unwrap();
        assert!(matches!(
            storage.exists(&repo, "loop").await,
            Err(StorageError::Io(_))
        ));

        let looping = key();
        let org_dir = base.join(looping.org_id.to_string());
        std::fs::create_dir_all(&org_dir).unwrap();
        let repo_loop = org_dir.join(looping.repo_id.to_string());
        symlink(&repo_loop, &repo_loop).unwrap();
        assert!(matches!(
            storage.remove_repo(&looping).await,
            Err(StorageError::Io(_))
        ));
    }

    #[test]
    fn local_path_returns_expected() {
        let tmp = tempfile::tempdir().unwrap();
        let wt = tmp.path().join("worktrees");
        let base = tmp.path().join("repos");
        std::fs::create_dir_all(&base).unwrap();

        let storage = LocalRepoStorage::new(base.clone(), wt.clone()).unwrap();
        let repo = key();

        let path = storage.local_path(&repo).unwrap();
        assert!(path.ends_with(repo.relative()));

        let wt_root = storage.worktree_root().unwrap();
        assert_eq!(wt_root, wt);
    }

    #[test]
    fn repo_storage_is_object_safe() {
        fn _accepts(_s: &dyn RepoStorage) {}
    }
}
