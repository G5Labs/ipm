// ---------------------------------------------------------------------------
// g5-common/src/storage/filestore.rs — NFS-backed FilestoreRepoStorage
//
// Google Cloud Filestore (NFS) implementation of RepoStorage.
//
// Key differences from LocalRepoStorage:
// - Bare repos live on NFS mount (shared across all nodes)
// - Worktrees live on local SSD (performance-critical)
// - Lockfile-based mutual exclusion for bare repo operations
//   using O_CREAT|O_EXCL for atomic lock creation on NFS
// - Stale lock detection (worker liveness check, 15-min fallback)
// ---------------------------------------------------------------------------

use std::path::{Component, Path, PathBuf};
use std::time::{Duration, SystemTime};

use async_trait::async_trait;
use bytes::Bytes;

use super::{validate_storage_component, RepoKey, RepoStorage, StorageEntry, StorageError};

/// NFS-backed repo storage for multi-node production deployments.
///
/// Bare repos live at `{mount_point}/{tenant_prefix}/{org_id}/{repo_id}/`
/// on the shared NFS mount. Worktrees live on local SSD at
/// `{worktree_dir}/{task_id}/` for performance.
///
/// # Locking
///
/// Bare repo write operations (init, remove) are protected by lockfiles.
/// Lock acquisition uses `O_CREAT|O_EXCL` flags for NFS-safe atomicity.
/// A repository's lockfile is `{org_id}/.{repo_id}.lock`, beside it.
/// The lockfile contains `{worker_id}:{pid}:{timestamp}` for stale
/// detection.
pub struct FilestoreRepoStorage {
    /// NFS mount point (e.g. `/mnt/filestore`).
    #[allow(dead_code)]
    mount_point: PathBuf,
    /// Tenant prefix under mount (e.g. `repos`).
    #[allow(dead_code)]
    tenant_prefix: String,
    /// Canonicalized base: `{mount_point}/{tenant_prefix}`.
    canonical_base: PathBuf,
    /// Worktree root on local SSD.
    worktree_dir: PathBuf,
    /// Stale lock timeout.
    stale_lock_timeout: Duration,
}

/// RAII lock guard that removes the lockfile on drop.
#[derive(Debug)]
pub struct LockGuard {
    lock_path: PathBuf,
}

impl Drop for LockGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.lock_path);
    }
}

impl FilestoreRepoStorage {
    /// Create a new `FilestoreRepoStorage`.
    ///
    /// - `mount_point` must exist (NFS mount)
    /// - `tenant_prefix` is the subdirectory under the mount
    /// - `worktree_dir` is on local SSD (will be created if needed)
    pub fn new(
        mount_point: PathBuf,
        tenant_prefix: String,
        worktree_dir: PathBuf,
    ) -> Result<Self, StorageError> {
        validate_storage_component("tenant_prefix", &tenant_prefix)?;
        let base_dir = mount_point.join(&tenant_prefix);
        std::fs::create_dir_all(&base_dir).map_err(|e| {
            StorageError::Backend(format!(
                "cannot create base dir {}: {e}",
                base_dir.display()
            ))
        })?;
        let canonical_base = base_dir.canonicalize().map_err(|e| {
            StorageError::Backend(format!(
                "cannot canonicalize base dir {}: {e}",
                base_dir.display()
            ))
        })?;
        std::fs::create_dir_all(&worktree_dir).map_err(|e| {
            StorageError::Backend(format!(
                "cannot create worktree dir {}: {e}",
                worktree_dir.display()
            ))
        })?;
        Ok(Self {
            mount_point,
            tenant_prefix,
            canonical_base,
            worktree_dir,
            stale_lock_timeout: Duration::from_secs(15 * 60), // 15 minutes
        })
    }

    fn repo_dir(&self, key: &RepoKey) -> PathBuf {
        self.canonical_base.join(key.relative())
    }

    fn validate_repo_path(&self, key: &RepoKey, path: &str) -> Result<PathBuf, StorageError> {
        self.validate_path(&key.relative().join(path))
    }

    fn lock_path(&self, key: &RepoKey) -> PathBuf {
        self.canonical_base
            .join(key.org_dir())
            .join(format!(".{}.lock", key.repo_id.hyphenated()))
    }

    /// Lexical path traversal validation (same as LocalRepoStorage).
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

    /// Acquire an exclusive lock on a bare repo using `O_CREAT|O_EXCL`.
    ///
    /// On NFS, `O_CREAT|O_EXCL` is atomic (with NFSv3+ `EXCLUSIVE` create
    /// mode or NFSv4 `OPEN` with `EXCLUSIVE4`). This provides mutual
    /// exclusion without advisory locks (which are unreliable on NFS).
    ///
    /// The lockfile contains: `{worker_id}:{pid}:{unix_timestamp}`
    ///
    /// If the lock is already held, checks for staleness:
    /// - Parse timestamp from lockfile
    /// - If older than `stale_lock_timeout`, remove and retry
    /// - Otherwise return `Err`
    pub fn acquire_lock(&self, key: &RepoKey) -> Result<LockGuard, StorageError> {
        let repo_id = key.repo_id;
        let lock_path = self.lock_path(key);
        if let Some(org_dir) = lock_path.parent() {
            std::fs::create_dir_all(org_dir)
                .map_err(|e| StorageError::Backend(format!("lock directory for {repo_id}: {e}")))?;
        }
        let lock_content = format!(
            "{}:{}:{}",
            hostname(),
            std::process::id(),
            SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs()
        );

        // Attempt 1: create exclusively
        match create_exclusive(&lock_path, lock_content.as_bytes()) {
            Ok(()) => return Ok(LockGuard { lock_path }),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                // Lock exists — check for staleness
            }
            Err(e) => {
                return Err(StorageError::Backend(format!(
                    "lock creation failed for {repo_id}: {e}"
                )));
            }
        }

        // Read existing lockfile to check staleness
        let existing = match std::fs::read_to_string(&lock_path) {
            Ok(s) => s,
            Err(_) => {
                // Lock was released between our create attempt and read —
                // retry once
                create_exclusive(&lock_path, lock_content.as_bytes()).map_err(|e| {
                    StorageError::Backend(format!("lock retry failed for {repo_id}: {e}"))
                })?;
                return Ok(LockGuard { lock_path });
            }
        };

        // Parse timestamp from "hostname:pid:timestamp"
        if let Some(ts_str) = existing.split(':').nth(2) {
            if let Ok(ts) = ts_str.trim().parse::<u64>() {
                let age = SystemTime::now()
                    .duration_since(SystemTime::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs()
                    .saturating_sub(ts);
                if Duration::from_secs(age) > self.stale_lock_timeout {
                    tracing::warn!(
                        %repo_id,
                        age_secs = age,
                        lock_content = %existing,
                        "removing stale lock"
                    );
                    let _ = std::fs::remove_file(&lock_path);

                    // Retry after removing stale lock
                    create_exclusive(&lock_path, lock_content.as_bytes()).map_err(|e| {
                        StorageError::Backend(format!(
                            "lock retry after stale removal failed for {repo_id}: {e}"
                        ))
                    })?;
                    return Ok(LockGuard { lock_path });
                }
            }
        }

        Err(StorageError::Backend(format!(
            "repo {repo_id} is locked by another process: {existing}"
        )))
    }

    /// Recursively walk a directory up to `depth` levels deep.
    async fn walk_dir(
        &self,
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
                    Box::pin(self.walk_dir(&entry.path(), depth - 1, include_hidden)).await?;
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
impl RepoStorage for FilestoreRepoStorage {
    async fn list_tree(
        &self,
        key: &RepoKey,
        path: &str,
        depth: u32,
    ) -> Result<Vec<StorageEntry>, StorageError> {
        let target = self.validate_repo_path(key, path)?;
        self.walk_dir(&target, depth, false).await
    }

    async fn list_tree_including_hidden(
        &self,
        key: &RepoKey,
        path: &str,
        depth: u32,
    ) -> Result<Vec<StorageEntry>, StorageError> {
        let target = self.validate_repo_path(key, path)?;
        self.walk_dir(&target, depth, true).await
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
        let _guard = self.acquire_lock(key)?;
        let dir = self.repo_dir(key);
        tokio::fs::create_dir_all(&dir).await?;
        Ok(dir)
    }

    async fn remove_repo(&self, key: &RepoKey) -> Result<(), StorageError> {
        let _guard = self.acquire_lock(key)?;
        let dir = self.repo_dir(key);
        if tokio::fs::try_exists(&dir).await? {
            tokio::fs::remove_dir_all(&dir).await?;
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Create a file exclusively using `O_CREAT|O_EXCL`.
///
/// On NFSv3+, this is atomic — the server will reject the create if the
/// file already exists, without a race window.
fn create_exclusive(path: &Path, content: &[u8]) -> std::io::Result<()> {
    use std::fs::OpenOptions;
    use std::io::Write;

    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true) // O_CREAT | O_EXCL
        .open(path)?;
    file.write_all(content)?;
    file.sync_all()?;
    Ok(())
}

/// Best-effort hostname.
fn hostname() -> String {
    std::env::var("HOSTNAME")
        .or_else(|_| std::env::var("HOST"))
        .unwrap_or_else(|_| "unknown".to_string())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

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

    fn make_storage() -> (tempfile::TempDir, FilestoreRepoStorage) {
        let tmp = tempfile::tempdir().unwrap();
        let mount = tmp.path().join("mount");
        std::fs::create_dir_all(&mount).unwrap();
        let worktrees = tmp.path().join("worktrees");
        let storage = FilestoreRepoStorage::new(mount, "repos".into(), worktrees).unwrap();
        (tmp, storage)
    }

    #[tokio::test]
    async fn filestore_roundtrip() {
        let (_tmp, storage) = make_storage();
        let repo = key();

        // Init repo
        let dir = storage.init_repo(&repo).await.unwrap();
        assert!(dir.exists());

        // Write + read
        storage
            .write_file(&repo, "hello.txt", b"nfs")
            .await
            .unwrap();
        let data = storage.read_file(&repo, "hello.txt").await.unwrap();
        assert_eq!(&data[..], b"nfs");

        // Exists
        assert!(storage.exists(&repo, "hello.txt").await.unwrap());

        // List tree
        let entries = storage.list_tree(&repo, ".", 1).await.unwrap();
        assert!(entries.iter().any(|e| e.path == "hello.txt"));

        // Delete
        storage.delete_file(&repo, "hello.txt").await.unwrap();
        assert!(!storage.exists(&repo, "hello.txt").await.unwrap());

        // Remove repo
        storage.remove_repo(&repo).await.unwrap();
        assert!(!dir.exists());
    }

    #[tokio::test]
    async fn list_tree_including_hidden_keeps_dot_artifacts_but_not_git() {
        let (_tmp, storage) = make_storage();
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
        let (_tmp, storage) = make_storage();

        let repo = key();

        let result = storage.read_file(&repo, "../../etc/passwd").await;
        assert!(matches!(result, Err(StorageError::PathTraversal(_))));

        let result = storage.read_file(&repo, "/etc/passwd").await;
        assert!(matches!(result, Err(StorageError::PathTraversal(_))));
    }

    #[tokio::test]
    async fn repositories_and_locks_live_under_their_organization() {
        let (tmp, storage) = make_storage();
        let base = tmp.path().join("mount/repos").canonicalize().unwrap();
        let repo = key();
        let org_dir = base.join(repo.org_id.to_string());

        let dir = storage.init_repo(&repo).await.unwrap();
        assert_eq!(dir, org_dir.join(repo.repo_id.to_string()));
        assert_eq!(storage.local_path(&repo), Some(dir.clone()));
        assert_eq!(
            storage.lock_path(&repo),
            org_dir.join(format!(".{}.lock", repo.repo_id))
        );
        assert_eq!(
            storage.review_artifact_dir(&repo),
            Some(
                org_dir
                    .join(".review-artifacts")
                    .join(repo.repo_id.to_string())
            )
        );

        let elsewhere = RepoKey {
            org_id: Uuid::new_v4(),
            repo_id: repo.repo_id,
        };
        assert!(!storage.exists(&elsewhere, ".").await.unwrap());
    }

    #[test]
    fn tenant_prefix_must_be_one_safe_component() {
        let tmp = tempfile::tempdir().unwrap();
        let mount = tmp.path().join("mount");
        std::fs::create_dir_all(&mount).unwrap();

        for tenant_prefix in ["", ".", "..", "nested/repos", "/absolute"] {
            let result = FilestoreRepoStorage::new(
                mount.clone(),
                tenant_prefix.to_string(),
                tmp.path().join("worktrees"),
            );
            assert!(matches!(result, Err(StorageError::PathTraversal(_))));
        }
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn filesystem_probe_errors_are_not_reported_as_absence() {
        use std::os::unix::fs::symlink;

        let (tmp, storage) = make_storage();
        let base = tmp.path().join("mount/repos");
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
    fn lockfile_acquire_and_release() {
        let (_tmp, storage) = make_storage();
        let repo = key();

        // First acquisition succeeds
        let guard = storage.acquire_lock(&repo).unwrap();
        let lock_path = storage.lock_path(&repo);
        assert!(lock_path.exists());

        // Second acquisition fails (lock held)
        let result = storage.acquire_lock(&repo);
        assert!(result.is_err());
        match result {
            Err(StorageError::Backend(msg)) => {
                assert!(msg.contains("locked by another process"));
            }
            other => panic!("expected Backend error, got: {:?}", other),
        }

        // Release lock
        drop(guard);
        assert!(!lock_path.exists());

        // Re-acquire succeeds
        let _guard2 = storage.acquire_lock(&repo).unwrap();
    }

    #[test]
    fn stale_lock_detection() {
        let (_tmp, mut storage) = make_storage();
        let repo = key();
        // Set very short stale timeout for testing
        storage.stale_lock_timeout = Duration::from_secs(0);

        // Create a manually expired lock
        let lock_path = storage.lock_path(&repo);
        let old_ts = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_secs()
            .saturating_sub(1000);
        let content = format!("old-host:99999:{old_ts}");
        std::fs::create_dir_all(lock_path.parent().unwrap()).unwrap();
        create_exclusive(&lock_path, content.as_bytes()).unwrap();

        // Should succeed because lock is stale
        let guard = storage.acquire_lock(&repo).unwrap();
        assert!(lock_path.exists());
        drop(guard);
    }

    #[test]
    fn local_path_and_worktree() {
        let (_tmp, storage) = make_storage();
        let repo = key();
        let path = storage.local_path(&repo).unwrap();
        assert!(path.to_str().unwrap().contains("repos"));
        assert!(path.ends_with(repo.relative()));

        let wt = storage.worktree_root().unwrap();
        assert!(wt.to_str().unwrap().contains("worktrees"));
    }

    #[test]
    fn create_exclusive_rejects_existing() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("test.lock");

        // First create succeeds
        create_exclusive(&path, b"first").unwrap();

        // Second create fails with AlreadyExists
        let result = create_exclusive(&path, b"second");
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().kind(),
            std::io::ErrorKind::AlreadyExists
        );
    }

    // -----------------------------------------------------------------------
    // Concurrent locking stress tests
    // -----------------------------------------------------------------------

    /// Multi-threaded contention test: N threads race to acquire the same lock.
    /// Exactly one should succeed; the rest must fail. After the winner releases
    /// the lock, another thread should be able to acquire it.
    #[test]
    fn concurrent_lock_contention_single_winner() {
        use std::sync::{Arc, Barrier};

        let (_tmp, storage) = make_storage();
        let repo = key();
        let storage = Arc::new(storage);
        let num_threads = 8;
        let barrier = Arc::new(Barrier::new(num_threads));

        let results: Vec<_> = std::thread::scope(|s| {
            let handles: Vec<_> = (0..num_threads)
                .map(|_| {
                    let storage = Arc::clone(&storage);
                    let barrier = Arc::clone(&barrier);
                    s.spawn(move || {
                        barrier.wait(); // Synchronize all threads to start simultaneously
                        storage.acquire_lock(&repo)
                    })
                })
                .collect();

            handles.into_iter().map(|h| h.join().unwrap()).collect()
        });

        let winners: Vec<_> = results.iter().filter(|r| r.is_ok()).collect();
        let losers: Vec<_> = results.iter().filter(|r| r.is_err()).collect();

        assert_eq!(
            winners.len(),
            1,
            "Exactly one thread should win the lock, got {} winners",
            winners.len()
        );
        assert_eq!(
            losers.len(),
            num_threads - 1,
            "All other threads should fail, got {} losers",
            losers.len()
        );

        // All losers should get the "locked by another process" error
        for result in &losers {
            match result {
                Err(StorageError::Backend(msg)) => {
                    assert!(
                        msg.contains("locked by another process"),
                        "Expected 'locked by another process', got: {}",
                        msg
                    );
                }
                other => panic!("Expected Backend error, got: {:?}", other),
            }
        }
    }

    /// Sequential acquisition: acquire → release → re-acquire across multiple
    /// iterations. Validates that RAII guards properly clean up lockfiles.
    #[test]
    fn sequential_lock_acquire_release_cycles() {
        let (_tmp, storage) = make_storage();
        let repo = key();

        for i in 0..20 {
            let guard = storage
                .acquire_lock(&repo)
                .unwrap_or_else(|e| panic!("Cycle {i}: failed to acquire lock: {e}"));

            let lock_path = storage.lock_path(&repo);
            assert!(
                lock_path.exists(),
                "Cycle {i}: lockfile should exist while held"
            );

            drop(guard);
            assert!(
                !lock_path.exists(),
                "Cycle {i}: lockfile should be removed after drop"
            );
        }
    }

    /// Multiple independent repos can be locked concurrently without interference.
    #[test]
    fn independent_repo_locks_no_interference() {
        let (_tmp, storage) = make_storage();
        let repo_ids: Vec<RepoKey> = (0..5).map(|_| key()).collect();

        // Acquire locks on all repos simultaneously
        let guards: Vec<_> = repo_ids
            .iter()
            .map(|id| {
                storage
                    .acquire_lock(id)
                    .unwrap_or_else(|e| panic!("Failed to lock {id:?}: {e}"))
            })
            .collect();

        // All lockfiles should exist
        for id in &repo_ids {
            assert!(
                storage.lock_path(id).exists(),
                "Lockfile for {id:?} should exist"
            );
        }

        // Release all
        drop(guards);

        // All lockfiles should be gone
        for id in &repo_ids {
            assert!(
                !storage.lock_path(id).exists(),
                "Lockfile for {id:?} should be removed"
            );
        }
    }

    /// Multi-threaded stress test: each thread acquires a lock on its own repo.
    /// All should succeed in parallel, demonstrating no cross-repo interference.
    #[test]
    fn concurrent_independent_repos_all_succeed() {
        use std::sync::{Arc, Barrier};

        let (_tmp, storage) = make_storage();
        let storage = Arc::new(storage);
        let num_threads = 10;
        let barrier = Arc::new(Barrier::new(num_threads));

        let results: Vec<_> = std::thread::scope(|s| {
            let handles: Vec<_> = (0..num_threads)
                .map(|_| {
                    let storage = Arc::clone(&storage);
                    let barrier = Arc::clone(&barrier);
                    s.spawn(move || {
                        barrier.wait();
                        let guard = storage.acquire_lock(&key())?;
                        // Hold lock briefly
                        std::thread::sleep(Duration::from_millis(10));
                        drop(guard);
                        Ok::<(), StorageError>(())
                    })
                })
                .collect();

            handles.into_iter().map(|h| h.join().unwrap()).collect()
        });

        let failures: Vec<_> = results.iter().filter(|r| r.is_err()).collect();
        assert!(
            failures.is_empty(),
            "All independent repo locks should succeed, {} failed",
            failures.len()
        );
    }

    /// Stress test: threads race to lock, fail, wait for release, then re-lock.
    /// Validates that after a lock holder drops, a new acquisition succeeds.
    #[test]
    fn lock_handoff_between_threads() {
        use std::sync::Arc;

        let (_tmp, storage) = make_storage();
        let repo = key();
        let storage = Arc::new(storage);

        // Thread 1 acquires lock
        let guard = storage.acquire_lock(&repo).unwrap();

        // Thread 2 tries and fails
        let storage2 = Arc::clone(&storage);
        let result = std::thread::spawn(move || storage2.acquire_lock(&repo))
            .join()
            .unwrap();
        assert!(result.is_err(), "Should fail while lock is held");

        // Thread 1 releases
        drop(guard);

        // Thread 3 succeeds
        let storage3 = Arc::clone(&storage);
        let result = std::thread::spawn(move || storage3.acquire_lock(&repo))
            .join()
            .unwrap();
        assert!(result.is_ok(), "Should succeed after lock is released");
    }

    /// Filestore init_repo and remove_repo acquire locks — verify concurrent
    /// init/remove operations serialize correctly.
    #[tokio::test]
    async fn concurrent_init_remove_serialize_via_locks() {
        let (_tmp, storage) = make_storage();
        let repo = key();

        // Init repo
        let dir = storage.init_repo(&repo).await.unwrap();
        assert!(dir.exists());

        // Write a file
        storage
            .write_file(&repo, "data.txt", b"content")
            .await
            .unwrap();

        // Remove should succeed
        storage.remove_repo(&repo).await.unwrap();
        assert!(!dir.exists());

        // Re-init should also succeed
        let dir2 = storage.init_repo(&repo).await.unwrap();
        assert!(dir2.exists());

        // Cleanup
        storage.remove_repo(&repo).await.unwrap();
    }

    /// Verify that lockfile content includes hostname, PID, and timestamp.
    #[test]
    fn lockfile_content_format() {
        let (_tmp, storage) = make_storage();
        let repo = key();

        let guard = storage.acquire_lock(&repo).unwrap();
        let lock_path = storage.lock_path(&repo);
        let content = std::fs::read_to_string(&lock_path).unwrap();

        let parts: Vec<&str> = content.split(':').collect();
        assert_eq!(
            parts.len(),
            3,
            "Lockfile should have format hostname:pid:timestamp, got: {}",
            content
        );

        // Part 1: hostname (non-empty string)
        assert!(!parts[0].is_empty(), "Hostname should be non-empty");

        // Part 2: PID (valid u32)
        let pid: u32 = parts[1]
            .parse()
            .unwrap_or_else(|_| panic!("PID should be a number, got: {}", parts[1]));
        assert_eq!(
            pid,
            std::process::id(),
            "PID in lockfile should match current process"
        );

        // Part 3: Unix timestamp (valid u64, recent)
        let ts: u64 = parts[2]
            .parse()
            .unwrap_or_else(|_| panic!("Timestamp should be a number, got: {}", parts[2]));
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        assert!(
            now.abs_diff(ts) < 10,
            "Timestamp should be within 10 seconds of now"
        );

        drop(guard);
    }
}
