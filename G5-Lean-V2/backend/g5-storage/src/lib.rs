//! Repository storage abstraction.
//!
//! Provides the [`RepoStorage`] trait that mediates all filesystem access
//! to repo data. Two providers are planned:
//! - [`LocalRepoStorage`] — dev / single-node (direct filesystem)
//! - `FilestoreRepoStorage` — prod / multi-node (Google Cloud Filestore NFS)
//!
//! Both expose POSIX paths via [`RepoStorage::local_path()`] because git
//! operations require a real filesystem.

mod attachments;
mod config;
mod filestore;
mod local;

pub use attachments::{
    AttachmentKey, AttachmentStore, ByteStream, DirectoryAttachmentStore, StoredObject,
};
pub use config::StorageConfig;
pub use filestore::FilestoreRepoStorage;
pub use local::LocalRepoStorage;

use async_trait::async_trait;
use bytes::Bytes;
use std::path::{Component, Path, PathBuf};
use uuid::Uuid;

fn validate_storage_component(label: &str, value: &str) -> Result<(), StorageError> {
    let mut components = Path::new(value).components();
    let valid =
        matches!(components.next(), Some(Component::Normal(_))) && components.next().is_none();
    if valid {
        Ok(())
    } else {
        Err(StorageError::PathTraversal(format!(
            "{label} must be one non-empty path component: {value}"
        )))
    }
}

// ---------------------------------------------------------------------------
// RepoKey
// ---------------------------------------------------------------------------

/// A repository's storage name: its organization and its own id
/// (`g5.repo.storage-tenancy`).
///
/// Tenancy is checked above this layer, by the store's sole holder; this
/// layer only guarantees that every repository path is built from typed ids
/// and nothing else, through [`RepoKey::relative`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RepoKey {
    pub org_id: Uuid,
    pub repo_id: Uuid,
}

impl RepoKey {
    /// `<org>/<repo>`, relative to the storage base.
    #[must_use]
    pub fn relative(&self) -> PathBuf {
        Path::new(&self.org_id.hyphenated().to_string()).join(self.repo_id.hyphenated().to_string())
    }

    /// `<org>`: the directory holding everything of the organization's
    /// repositories.
    #[must_use]
    pub fn org_dir(&self) -> PathBuf {
        PathBuf::from(self.org_id.hyphenated().to_string())
    }
}

// ---------------------------------------------------------------------------
// StorageEntry
// ---------------------------------------------------------------------------

/// Metadata about a file or directory entry.
#[derive(Debug, Clone)]
pub struct StorageEntry {
    /// Path relative to repo root.
    pub path: String,
    pub is_dir: bool,
    pub size: Option<u64>,
    pub last_modified: Option<chrono::DateTime<chrono::Utc>>,
}

// ---------------------------------------------------------------------------
// StorageError
// ---------------------------------------------------------------------------

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum StorageError {
    #[error("not found: {0}")]
    NotFound(String),
    #[error("path traversal blocked: {0}")]
    PathTraversal(String),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("storage backend error: {0}")]
    Backend(String),
}

// ---------------------------------------------------------------------------
// RepoStorage trait
// ---------------------------------------------------------------------------

/// Abstraction over repository storage backends.
///
/// Repositories live at `{base}/{org_id}/{repo_id}/`; all other paths are
/// relative to that repo root.
#[async_trait]
pub trait RepoStorage: Send + Sync + 'static {
    /// List entries in a directory, up to `depth` levels deep.
    async fn list_tree(
        &self,
        key: &RepoKey,
        path: &str,
        depth: u32,
    ) -> Result<Vec<StorageEntry>, StorageError>;

    /// List entries in a directory, including hidden files/directories.
    ///
    /// This is for internal maintenance/indexing paths that must see durable
    /// system artifacts stored under dot-directories. Implementations must
    /// still exclude `.git`.
    async fn list_tree_including_hidden(
        &self,
        key: &RepoKey,
        path: &str,
        depth: u32,
    ) -> Result<Vec<StorageEntry>, StorageError> {
        self.list_tree(key, path, depth).await
    }

    /// Read file content as bytes.
    async fn read_file(&self, key: &RepoKey, path: &str) -> Result<Bytes, StorageError>;

    /// Write file content (creates intermediate dirs).
    async fn write_file(
        &self,
        key: &RepoKey,
        path: &str,
        content: &[u8],
    ) -> Result<(), StorageError>;

    /// Check if a path exists.
    async fn exists(&self, key: &RepoKey, path: &str) -> Result<bool, StorageError>;

    /// Delete a file.
    async fn delete_file(&self, key: &RepoKey, path: &str) -> Result<(), StorageError>;

    /// Get the local filesystem path for a repo (bare repo root).
    ///
    /// Returns `Some(path)` for all currently supported backends (Local,
    /// Filestore). Git operations require a real filesystem path.
    /// `None` is reserved for future object-store backends.
    fn local_path(&self, key: &RepoKey) -> Option<PathBuf>;

    /// The directory for a repository's review artifacts (files attached to
    /// a workspace review), outside the repository root. `list_tree`,
    /// `read_file` and the blob capability routes confine their paths to
    /// `{org}/{repo}`, so nothing under this directory is reachable through them;
    /// the workspace-authorized review routes are its only readers. `None`
    /// for a backend without a local filesystem.
    fn review_artifact_dir(&self, key: &RepoKey) -> Option<PathBuf> {
        let _ = key;
        None
    }

    /// Get the worktree root directory.
    ///
    /// Worktrees are created under this directory (local SSD, not NFS).
    /// Returns `None` for backends that don't support worktrees.
    fn worktree_root(&self) -> Option<PathBuf>;

    /// Initialize storage for a new repo (create directories, etc.).
    async fn init_repo(&self, key: &RepoKey) -> Result<PathBuf, StorageError>;

    /// Remove all storage for a repo.
    async fn remove_repo(&self, key: &RepoKey) -> Result<(), StorageError>;

    /// Get the size of a file in bytes without reading its content.
    ///
    /// Used by blob service to pre-check size limits before loading
    /// large files into memory.
    async fn file_size(&self, key: &RepoKey, path: &str) -> Result<u64, StorageError> {
        // Default implementation falls back to reading the file.
        // Concrete backends should override with `tokio::fs::metadata`.
        let data = self.read_file(key, path).await?;
        Ok(data.len() as u64)
    }
}
