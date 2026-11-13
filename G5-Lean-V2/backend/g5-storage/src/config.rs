// ---------------------------------------------------------------------------
// g5-common/src/storage/config.rs — Storage configuration
//
// Tagged enum specifying which RepoStorage provider to use, driven by
// environment variables:
//   - G5_STORAGE_PROVIDER     = "local" (default) | "filestore"
//   - G5_STORAGE_BASE_DIR     = base directory for bare repos (local)
//   - G5_STORAGE_WORKTREE_DIR = worktree root (local SSD, separate from repos)
//   - G5_STORAGE_MOUNT_POINT  = NFS mount point (filestore)
//   - G5_STORAGE_TENANT_PREFIX = tenant prefix under mount point (filestore)
// ---------------------------------------------------------------------------

use std::path::PathBuf;
use std::sync::Arc;

use super::{FilestoreRepoStorage, LocalRepoStorage, RepoStorage, StorageError};

// ---------------------------------------------------------------------------
// StorageConfig
// ---------------------------------------------------------------------------

/// Storage backend configuration.
///
/// Two providers are supported:
/// - **Local**: Direct filesystem access. Bare repos and worktrees live on
///   the same machine. Used in development and single-node deployments.
/// - **Filestore**: NFS-mounted shared storage. Bare repos live on the NFS
///   mount; worktrees live on local SSD for performance. Used in production
///   multi-node deployments.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum StorageConfig {
    /// Direct filesystem storage (dev / single-node).
    Local {
        /// Base directory for bare repos: `{base_dir}/{org_id}/{repo_id}/`.
        base_dir: PathBuf,
        /// Worktree root on local SSD: `{worktree_dir}/{task_id}/`.
        /// Defaults to `{base_dir}/.worktrees` if unset.
        worktree_dir: PathBuf,
    },
    /// NFS-mounted shared storage (prod / multi-node).
    Filestore {
        /// NFS mount point (e.g. `/mnt/filestore`).
        mount_point: PathBuf,
        /// Tenant prefix under mount point (e.g. `repos`).
        /// Full path: `{mount_point}/{tenant_prefix}/{org_id}/{repo_id}/`.
        tenant_prefix: String,
        /// Worktree root on local SSD (NOT on NFS).
        worktree_dir: PathBuf,
    },
}

impl StorageConfig {
    /// Build a `RepoStorage` provider from this config.
    ///
    /// Returns an `Arc<dyn RepoStorage>` ready for injection into `AppState`.
    pub fn build(&self) -> Result<Arc<dyn RepoStorage>, StorageError> {
        match self {
            StorageConfig::Local {
                base_dir,
                worktree_dir,
            } => {
                let storage = LocalRepoStorage::new(base_dir.clone(), worktree_dir.clone())?;
                Ok(Arc::new(storage))
            }
            StorageConfig::Filestore {
                mount_point,
                tenant_prefix,
                worktree_dir,
            } => {
                let storage = FilestoreRepoStorage::new(
                    mount_point.clone(),
                    tenant_prefix.clone(),
                    worktree_dir.clone(),
                )?;
                Ok(Arc::new(storage))
            }
        }
    }

    /// Parse `StorageConfig` from environment variables.
    ///
    /// | Variable | Default | Description |
    /// |----------|---------|-------------|
    /// | `G5_STORAGE_PROVIDER` | `"local"` | `"local"` or `"filestore"` |
    /// | `G5_STORAGE_BASE_DIR` | `"/data/repos"` | Bare repo base (local mode) |
    /// | `G5_STORAGE_WORKTREE_DIR` | `{base_dir}/.worktrees` | Worktree root |
    /// | `G5_STORAGE_MOUNT_POINT` | — | NFS mount point (filestore mode) |
    /// | `G5_STORAGE_TENANT_PREFIX` | `"repos"` | Path prefix under mount |
    pub fn from_env() -> Result<Self, StorageError> {
        fn non_empty_env(name: &str) -> Option<String> {
            std::env::var(name)
                .ok()
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty())
        }

        Self::from_values(
            non_empty_env("G5_STORAGE_PROVIDER").as_deref(),
            non_empty_env("G5_STORAGE_BASE_DIR").as_deref(),
            non_empty_env("G5_STORAGE_WORKTREE_DIR").as_deref(),
            non_empty_env("G5_STORAGE_MOUNT_POINT").as_deref(),
            non_empty_env("G5_STORAGE_TENANT_PREFIX").as_deref(),
        )
    }

    /// Build the typed provider configuration from already-resolved values.
    ///
    /// Callers that expose a CLI should resolve CLI > environment before this
    /// method. `None` and empty values retain the established local defaults.
    pub fn from_values(
        provider: Option<&str>,
        base_dir: Option<&str>,
        worktree_dir: Option<&str>,
        mount_point: Option<&str>,
        tenant_prefix: Option<&str>,
    ) -> Result<Self, StorageError> {
        fn normalized(value: Option<&str>) -> Option<&str> {
            value.map(str::trim).filter(|value| !value.is_empty())
        }
        match normalized(provider).unwrap_or("local") {
            "filestore" => {
                let mount_point =
                    PathBuf::from(normalized(mount_point).unwrap_or("/mnt/filestore"));
                let tenant_prefix = normalized(tenant_prefix).unwrap_or("repos").to_string();
                let worktree_dir =
                    PathBuf::from(normalized(worktree_dir).unwrap_or("/data/worktrees"));
                Ok(StorageConfig::Filestore {
                    mount_point,
                    tenant_prefix,
                    worktree_dir,
                })
            }
            "local" => {
                let base_dir = PathBuf::from(normalized(base_dir).unwrap_or("/data/repos"));
                let worktree_dir = normalized(worktree_dir)
                    .map(PathBuf::from)
                    .unwrap_or_else(|| base_dir.join(".worktrees"));
                Ok(StorageConfig::Local {
                    base_dir,
                    worktree_dir,
                })
            }
            unknown => Err(StorageError::Backend(format!(
                "unsupported storage provider: {unknown}"
            ))),
        }
    }

    /// Create a config from explicit CLI / config values.
    ///
    /// Used by `AppConfig::from_cli()` in g5-api to bridge the existing
    /// `--repos-base-dir` flag to the new `StorageConfig` system.
    #[must_use]
    pub fn local(base_dir: PathBuf) -> Self {
        let worktree_dir = base_dir.join(".worktrees");
        StorageConfig::Local {
            base_dir,
            worktree_dir,
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_config_build() {
        let dir = std::env::temp_dir().join("g5-storage-cfg-test-local");
        let _ = std::fs::create_dir_all(&dir);
        let cfg = StorageConfig::Local {
            base_dir: dir.clone(),
            worktree_dir: dir.join(".worktrees"),
        };
        let storage = cfg.build().expect("build should succeed");
        assert!(storage.worktree_root().is_some());
    }

    #[test]
    fn filestore_config_build() {
        let dir = std::env::temp_dir().join("g5-storage-cfg-test-fs");
        let mount = dir.join("mount");
        let _ = std::fs::create_dir_all(&mount);
        let worktrees = dir.join("worktrees");
        let cfg = StorageConfig::Filestore {
            mount_point: mount.clone(),
            tenant_prefix: "repos".into(),
            worktree_dir: worktrees,
        };
        let storage = cfg.build().expect("build should succeed");
        // base_dir = mount/repos
        let local_path = storage.local_path(&crate::RepoKey {
            org_id: uuid::Uuid::new_v4(),
            repo_id: uuid::Uuid::new_v4(),
        });
        assert!(local_path.is_some());
        let p = local_path.unwrap();
        assert!(p.to_str().unwrap().contains("repos"));
    }

    #[test]
    fn local_shorthand() {
        let dir = std::env::temp_dir().join("g5-storage-cfg-short");
        let cfg = StorageConfig::local(dir.clone());
        match &cfg {
            StorageConfig::Local {
                base_dir,
                worktree_dir,
            } => {
                assert_eq!(base_dir, &dir);
                assert_eq!(worktree_dir, &dir.join(".worktrees"));
            }
            _ => panic!("expected Local"),
        }
    }

    #[test]
    fn from_env_defaults_to_local() {
        // Clear any env vars that might be set
        std::env::remove_var("G5_STORAGE_PROVIDER");
        std::env::remove_var("G5_STORAGE_BASE_DIR");
        std::env::remove_var("G5_STORAGE_WORKTREE_DIR");
        let cfg = StorageConfig::from_env().expect("unset provider should select local storage");
        match cfg {
            StorageConfig::Local { base_dir, .. } => {
                assert_eq!(base_dir, PathBuf::from("/data/repos"));
            }
            _ => panic!("expected Local"),
        }
    }

    #[test]
    fn unknown_provider_is_rejected() {
        let result = StorageConfig::from_values(Some("object-store"), None, None, None, None);
        assert!(matches!(
            result,
            Err(StorageError::Backend(message))
                if message == "unsupported storage provider: object-store"
        ));
    }
}
