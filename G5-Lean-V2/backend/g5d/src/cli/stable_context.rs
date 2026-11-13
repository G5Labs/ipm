//! Stable Provided-Agent cwd discovery and validation helpers.
//!
//! These helpers discover the workspace metadata available to in-VM tools.

use std::collections::BTreeSet;
use std::path::{Component, Path, PathBuf};

use g5_contracts::error::G5Error;
use g5_contracts::types::{ChatAgentProvider, RepoRole, TaskKind};
use g5_internal_api::InternalApiClient;
use serde::{Deserialize, Serialize};

use super::{Command, GlobalFlags};

pub const STABLE_METADATA_REL: &str = ".g5/provided-agent-chat-workspace.json";
pub const STABLE_ROOT_ENV: &str = "G5D_STABLE_CWD_ROOT";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct StableWorkspaceMetadata {
    pub schema_version: u32,
    pub kind: String,
    pub session_id: String,
    pub workspace_id: String,
    pub agent_provider: ChatAgentProvider,
    pub created_by_task_id: String,
    #[serde(default = "default_stable_task_type")]
    pub task_type: TaskKind,
    #[serde(default)]
    pub repo_ids: Vec<String>,
    #[serde(default)]
    pub mounts: Vec<StableWorkspaceMountMetadata>,
}

fn default_stable_task_type() -> TaskKind {
    TaskKind::Agent
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct StableWorkspaceMountMetadata {
    pub repo_id: String,
    pub mount_path: String,
    #[serde(default)]
    pub repo_name: Option<String>,
    #[serde(default)]
    pub repo_role: RepoRole,
}

#[derive(Debug, Clone)]
pub struct StableContext {
    pub root: PathBuf,
    pub metadata: StableWorkspaceMetadata,
}

#[must_use]
pub fn metadata_path(root: &Path) -> PathBuf {
    root.join(STABLE_METADATA_REL)
}

pub fn discover(global: &GlobalFlags) -> Result<Option<StableContext>, G5Error> {
    let mut starts = Vec::new();

    if let Some(explicit) = global.workspace.as_ref() {
        starts.push(absolutize(explicit)?);
    }
    if let Ok(root) = std::env::var(STABLE_ROOT_ENV) {
        if !root.trim().is_empty() {
            starts.push(absolutize(Path::new(root.trim()))?);
        }
    }
    if let Ok(pwd) = std::env::var("PWD") {
        if !pwd.trim().is_empty() {
            starts.push(absolutize(Path::new(pwd.trim()))?);
        }
    }
    if let Ok(cwd) = std::env::current_dir() {
        starts.push(cwd);
    }

    let mut found: Vec<StableContext> = Vec::new();
    let mut seen = BTreeSet::new();
    for start in starts {
        let Some(root) = find_stable_root_from(&start) else {
            continue;
        };
        let key = root.to_string_lossy().to_string();
        if !seen.insert(key) {
            continue;
        }
        found.push(read_context(root)?);
    }

    match found.len() {
        0 => Ok(None),
        1 => Ok(found.into_iter().next()),
        _ => {
            let first = &found[0];
            if found.iter().all(|ctx| ctx.metadata == first.metadata)
                && found
                    .iter()
                    .all(|ctx| stable_roots_match(&ctx.root, &first.root))
            {
                Ok(Some(first.clone()))
            } else {
                Err(G5Error::Config(
                    "Multiple Provided-Agent stable cwd roots were found; set G5D_STABLE_CWD_ROOT explicitly to the intended task cwd.".to_string(),
                ))
            }
        }
    }
}

pub fn require_worker_client(global: &GlobalFlags) -> Result<InternalApiClient, G5Error> {
    InternalApiClient::from_workspace_env(&super::resolve_api_url(global), |key| {
        std::env::var(key).ok()
    })
    .ok_or(G5Error::Unauthorized)
}

pub fn resolve_task_id(
    stable: &StableContext,
    explicit_task_id: Option<&str>,
) -> Result<String, G5Error> {
    let task_id = stable.metadata.created_by_task_id.trim();
    if task_id.is_empty() {
        return Err(G5Error::ValidationError {
            details: serde_json::json!({
                "field": "createdByTaskId",
                "error": "stable cwd metadata is missing createdByTaskId"
            }),
        });
    }
    if let Some(explicit) = explicit_task_id.map(str::trim).filter(|s| !s.is_empty()) {
        if explicit != task_id {
            return Err(G5Error::Forbidden {
                missing_scopes: vec!["task:self-context-match".to_string()],
            });
        }
    }
    Ok(task_id.to_string())
}

pub fn mount_path_for_repo(
    stable: &StableContext,
    repo_id: &str,
) -> Result<Option<PathBuf>, G5Error> {
    if stable.metadata.mounts.is_empty()
        && (stable.metadata.repo_ids.is_empty()
            || stable.metadata.repo_ids.iter().any(|id| id == repo_id))
    {
        return Ok(Some(stable.root.clone()));
    }

    let Some(mount) = stable
        .metadata
        .mounts
        .iter()
        .find(|mount| mount.repo_id == repo_id)
    else {
        return Ok(None);
    };
    validate_mount_path(&mount.mount_path)?;
    let mount_path = stable.root.join(&mount.mount_path);
    verify_repo_mount_target(stable, repo_id, &mount_path)?;
    Ok(Some(mount_path))
}

pub fn fallback_single_repo_path(stable: &StableContext) -> PathBuf {
    stable.root.clone()
}

pub fn stable_guard_error(
    command: &Command,
    global: &GlobalFlags,
) -> Result<Option<G5Error>, G5Error> {
    let stable = discover(global)?;
    let Some(stable) = stable.as_ref() else {
        return Ok(None);
    };
    if explicit_classic_workspace_outside_stable_root(global, stable)? {
        return Ok(None);
    }

    let guarded = match command {
        Command::Add(_) => Some("g5d add changes workspace repo membership and is refused from a Provided-Agent stable cwd."),
        Command::Dispatch => Some("starting a nested workspace dispatcher from a Provided-Agent stable cwd is refused."),
        Command::Bootstrap => Some("g5d bootstrap is the dispatcher's module supervisor and is refused from a Provided-Agent stable cwd."),
        Command::AgentTurn | Command::ApprovalReviewTools { .. } => None,
        #[cfg(feature = "test-helpers")]
        Command::DispatchFixtureModule => None,
        Command::Host(_) => Some("g5d host is a host-level runtime and must not run from an agent workspace."),
        // `g5d workspace refresh` is served by the dispatcher's workspace.git
        // module over the local socket and never retargets the task cwd;
        // `g5d workspace save` and `email-owner` only call g5-api.
        Command::Workspace(_) => None,
        Command::Issue(_)
        | Command::Pr(_)
        | Command::Question(_)
        | Command::Chat(_)
        | Command::User(_) => None,
        Command::Spec(_) => None,
        Command::Auth(args) => match args.command {
            super::AuthCommand::Login(_) | super::AuthCommand::Logout(_) => {
                Some("g5d auth login/logout must not write task-cwd auth state; worker-child commands use G5D_TOKEN.")
            }
            super::AuthCommand::Status(_) => None,
        },
        Command::Init(_) => Some("g5d init would create classic workspace metadata inside a Provided-Agent stable cwd and is refused; choose an explicit non-task directory."),
        Command::Clone(_) => Some("g5d clone materializes a classic workspace and is refused from a Provided-Agent stable cwd; choose an explicit non-task directory outside this shell."),
        Command::Doctor(_) => None,
        // `oci materialize` extracts a rootfs to an explicit target dir; it does
        // not touch the task cwd, so it is safe from a Provided-Agent stable cwd.
        Command::Oci(_) => None,
        #[cfg(feature = "cloud-hypervisor")]
        Command::VmExec(_) | Command::VmInit => None,
        // Internal in-VM entrypoint; reads its own boot-injected env and does
        // not touch a Provided-Agent stable cwd.
        // Internal in-VM entrypoint; reads its app config from the materialized
        // /work/.g5/workspace.yaml and does not touch a Provided-Agent stable cwd.
        Command::PreviewServe(_) => None,
    };

    Ok(guarded.map(|message| G5Error::Config(message.to_string())))
}

pub(crate) fn explicit_classic_workspace_outside_stable_root(
    global: &GlobalFlags,
    stable: &StableContext,
) -> Result<bool, G5Error> {
    let Some(path) = global.workspace.as_ref() else {
        return Ok(false);
    };
    if !path.join(".g5").join("workspace.yaml").exists() {
        return Ok(false);
    }
    let explicit = absolutize(path)?;
    Ok(!stable_roots_match(&explicit, &stable.root))
}

pub fn read_context(root: PathBuf) -> Result<StableContext, G5Error> {
    let raw = std::fs::read_to_string(metadata_path(&root)).map_err(|e| G5Error::Internal {
        message: format!(
            "Failed to read Provided-Agent stable cwd metadata under '{}': {e}",
            root.display()
        ),
    })?;
    let mut metadata: StableWorkspaceMetadata =
        serde_json::from_str(&raw).map_err(|e| G5Error::ValidationError {
            details: serde_json::json!({
                "path": metadata_path(&root).display().to_string(),
                "error": format!("invalid stable cwd metadata JSON: {e}")
            }),
        })?;
    validate_metadata(&metadata)?;
    metadata.repo_ids.sort();
    metadata.mounts.sort_by(|a, b| {
        a.repo_id
            .cmp(&b.repo_id)
            .then(a.mount_path.cmp(&b.mount_path))
    });
    Ok(StableContext { root, metadata })
}

fn validate_metadata(metadata: &StableWorkspaceMetadata) -> Result<(), G5Error> {
    if metadata.schema_version != 1 || metadata.kind != "provided-agent-stable-chat-cwd" {
        return Err(G5Error::ValidationError {
            details: serde_json::json!({
                "schemaVersion": metadata.schema_version,
                "kind": metadata.kind,
                "error": "unsupported Provided-Agent stable cwd metadata"
            }),
        });
    }
    for (field, value) in [
        ("sessionId", metadata.session_id.as_str()),
        ("workspaceId", metadata.workspace_id.as_str()),
        ("createdByTaskId", metadata.created_by_task_id.as_str()),
    ] {
        if value.trim().is_empty() {
            return Err(G5Error::ValidationError {
                details: serde_json::json!({
                    "field": field,
                    "error": "required stable cwd metadata field is empty"
                }),
            });
        }
    }
    for mount in &metadata.mounts {
        validate_mount_path(&mount.mount_path)?;
    }
    Ok(())
}

pub fn validate_mount_path(mount_path: &str) -> Result<(), G5Error> {
    let trimmed = mount_path.trim();
    let path = Path::new(trimmed);
    if trimmed.is_empty()
        || path.is_absolute()
        || path.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return Err(G5Error::ValidationError {
            details: serde_json::json!({
                "mountPath": mount_path,
                "error": "mountPath must be a non-empty safe relative path without '..'"
            }),
        });
    }
    Ok(())
}

fn find_stable_root_from(start: &Path) -> Option<PathBuf> {
    let mut current = if start.is_file() {
        start.parent()?.to_path_buf()
    } else {
        start.to_path_buf()
    };
    loop {
        if metadata_path(&current).exists() {
            return Some(current);
        }
        if !current.pop() {
            return None;
        }
    }
}

fn absolutize(path: &Path) -> Result<PathBuf, G5Error> {
    if path.is_absolute() {
        return Ok(path.to_path_buf());
    }
    let cwd = std::env::current_dir().map_err(|e| G5Error::Internal {
        message: format!("failed to read current directory: {e}"),
    })?;
    Ok(cwd.join(path))
}

fn canonical_or_self(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

fn stable_roots_match(a: &Path, b: &Path) -> bool {
    canonical_or_self(a) == canonical_or_self(b)
}

fn verify_repo_mount_target(
    stable: &StableContext,
    repo_id: &str,
    mount_path: &Path,
) -> Result<(), G5Error> {
    let metadata = std::fs::symlink_metadata(mount_path).map_err(|e| G5Error::ValidationError {
        details: serde_json::json!({
            "repoId": repo_id,
            "mountPath": mount_path.display().to_string(),
            "error": format!("stable cwd mount target is missing or unreadable: {e}")
        }),
    })?;
    if metadata.file_type().is_symlink() {
        return Err(G5Error::Forbidden {
            missing_scopes: vec!["stable-cwd:legacy-symlink-mount".to_string()],
        });
    }
    if !metadata.is_dir() {
        return Err(G5Error::ValidationError {
            details: serde_json::json!({
                "repoId": repo_id,
                "mountPath": mount_path.display().to_string(),
                "error": "stable cwd mount target is not a directory"
            }),
        });
    }
    let stable_root = stable
        .root
        .canonicalize()
        .unwrap_or_else(|_| stable.root.clone());
    let mount_canon = mount_path
        .canonicalize()
        .unwrap_or_else(|_| mount_path.to_path_buf());
    if !mount_canon.starts_with(&stable_root) {
        return Err(G5Error::Forbidden {
            missing_scopes: vec!["stable-cwd:mount-outside-root".to_string()],
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use tempfile::TempDir;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    fn write_metadata(root: &Path, task_id: &str, mount_path: &str) {
        std::fs::create_dir_all(root.join(".g5")).unwrap();
        std::fs::write(
            metadata_path(root),
            serde_json::json!({
                "schemaVersion": 1,
                "kind": "provided-agent-stable-chat-cwd",
                "sessionId": "session-1",
                "workspaceId": "workspace-1",
                "agentProvider": "claude_code",
                "createdByTaskId": task_id,
                "repoIds": ["repo-1"],
                "mounts": [{"repoId": "repo-1", "mountPath": mount_path}]
            })
            .to_string(),
        )
        .unwrap();
    }

    #[test]
    fn stable_context_discovers_from_pwd_and_validates_mounts() {
        let dir = TempDir::new().unwrap();
        write_metadata(dir.path(), "task-1", "docs");
        let nested = dir.path().join("docs/sub");
        std::fs::create_dir_all(&nested).unwrap();
        let ctx = read_context(dir.path().to_path_buf()).unwrap();
        assert_eq!(ctx.metadata.created_by_task_id, "task-1");
        assert_eq!(ctx.metadata.task_type, TaskKind::Agent);
        assert_eq!(ctx.metadata.mounts[0].mount_path, "docs");
        validate_mount_path("docs/sub").unwrap();
        assert!(validate_mount_path("../escape").is_err());
        assert!(validate_mount_path("").is_err());

        let mut single_root = ctx.clone();
        single_root.metadata.mounts.clear();
        assert_eq!(
            mount_path_for_repo(&single_root, "repo-1").unwrap(),
            Some(dir.path().to_path_buf())
        );
    }

    #[test]
    fn stable_context_rejects_missing_created_by_task_id() {
        let dir = TempDir::new().unwrap();
        write_metadata(dir.path(), "", "docs");
        assert!(read_context(dir.path().to_path_buf()).is_err());
    }

    #[test]
    fn stable_context_rejects_non_directory_mount_target() {
        let worktree = TempDir::new().unwrap();
        let root = worktree.path().join("chat-session-claude-code");
        write_metadata(&root, "task-1", "repo");
        std::fs::write(root.join("repo"), "not a directory").unwrap();
        let ctx = read_context(root).unwrap();
        assert!(mount_path_for_repo(&ctx, "repo-1").is_err());
    }

    #[test]
    fn stable_context_accepts_real_repo_mount_target() {
        let worktree = TempDir::new().unwrap();
        let root = worktree.path().join("chat-session-claude-code");
        write_metadata(&root, "task-1", "repo");
        std::fs::create_dir_all(root.join("repo")).unwrap();
        let ctx = read_context(root.clone()).unwrap();
        assert_eq!(
            mount_path_for_repo(&ctx, "repo-1").unwrap(),
            Some(root.join("repo"))
        );
    }

    #[test]
    #[serial_test::serial]
    fn stable_context_rejects_multiple_distinct_roots_even_with_same_metadata() {
        let _guard = ENV_LOCK.lock().unwrap();
        let dir = TempDir::new().unwrap();
        let root_a = dir.path().join("a/chat-session-claude-code");
        let root_b = dir.path().join("b/chat-session-claude-code");
        write_metadata(&root_a, "task-1", "repo");
        write_metadata(&root_b, "task-1", "repo");
        let global = GlobalFlags {
            workspace: Some(root_a.join("repo")),
            verbose: 0,
            quiet: false,
            json: false,
            api_url: None,
        };
        let old_pwd = std::env::var("PWD").ok();
        std::env::set_var("PWD", root_b.join("repo"));
        let result = discover(&global);
        if let Some(value) = old_pwd {
            std::env::set_var("PWD", value);
        } else {
            std::env::remove_var("PWD");
        }
        assert!(result.is_err());
    }

    #[test]
    #[serial_test::serial]
    fn stable_guard_does_not_treat_same_root_classic_workspace_as_escape_hatch() {
        let _guard = ENV_LOCK.lock().unwrap();
        let old_pwd = std::env::var("PWD").ok();
        std::env::remove_var("PWD");
        let dir = TempDir::new().unwrap();
        write_metadata(dir.path(), "task-1", "repo");
        std::fs::write(
            dir.path().join(".g5/workspace.yaml"),
            "workspace_id: classic",
        )
        .unwrap();
        let global = GlobalFlags {
            workspace: Some(dir.path().to_path_buf()),
            verbose: 0,
            quiet: false,
            json: false,
            api_url: None,
        };
        let command = Command::Init(crate::cli::InitArgs {
            path: None,
            force: false,
            docs_repo: None,
        });
        let result = stable_guard_error(&command, &global);
        if let Some(value) = old_pwd {
            std::env::set_var("PWD", value);
        }
        assert!(result.unwrap().is_some());
    }
}
