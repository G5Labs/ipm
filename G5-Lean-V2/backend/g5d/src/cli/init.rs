//! `g5d init` — Create an empty workspace.
//!
//! Creates a new workspace with a generated workspace_id, `.g5/workspace.yaml`,
//! and `.g5/workspace-meta/` git repository.

use std::path::Path;

use g5_contracts::error::G5Error;
use g5_scm::workspace as ops;
use g5_scm::{GitAuth, GitAuthor};
use tracing::{info, warn};
use uuid::Uuid;

use crate::cli::config::{WorkspaceConfig, WorkspaceRemoteConfig};

use super::{format_success, GlobalFlags, InitArgs};

/// Execute the `g5d init` command.
///
/// 1. Determine target path (argument or current directory).
/// 2. Reject a Git repository root and check that `.g5/workspace.yaml` does not already exist.
/// 3. Generate a `workspace_id` (UUID v7).
/// 4. Create `.g5/` directory structure with `workspace.yaml` and `workspace-meta/`.
/// 5. If `--api-url` is provided or `G5D_API_URL` is set, enable remote.
/// 6. If docs config is available, initialize the independent docs checkout.
/// 7. Print success message.
pub async fn execute(args: &InitArgs, global: &GlobalFlags) -> Result<(), G5Error> {
    // 1. Determine target path
    let target_path = if let Some(ref path) = args.path {
        path.clone()
    } else {
        std::env::current_dir().map_err(|e| G5Error::Internal {
            message: format!("failed to get current directory: {}", e),
        })?
    };

    validate_workspace_envelope_root(&target_path)?;

    let g5_dir = target_path.join(".g5");
    let workspace_yaml_path = g5_dir.join("workspace.yaml");

    // 2. Check if workspace already exists
    if workspace_yaml_path.exists() && !args.force {
        let msg = format!("Workspace already exists at {}", target_path.display());
        return Err(G5Error::Internal { message: msg });
    }

    // 3. Generate workspace_id (UUID v7)
    let workspace_id = format!("ws_{}", Uuid::now_v7().to_string().replace('-', ""));

    // 4. Create .g5/ directory structure
    tokio::fs::create_dir_all(&g5_dir)
        .await
        .map_err(|e| G5Error::Internal {
            message: format!("failed to create .g5 directory: {}", e),
        })?;

    // Determine if remote should be enabled
    let remote_enabled = global.api_url.is_some()
        || std::env::var("G5D_API_URL")
            .ok()
            .filter(|s| !s.is_empty())
            .is_some();

    let api_url = global
        .api_url
        .clone()
        .or_else(|| std::env::var("G5D_API_URL").ok().filter(|s| !s.is_empty()));

    // Build workspace config
    let ws_config = WorkspaceConfig {
        workspace_id: Some(workspace_id.clone()),
        remote: WorkspaceRemoteConfig {
            enabled: remote_enabled,
            api_url,
            project_id: None,
        },
        clone: Default::default(),
        fork: Default::default(),
        isolation: Default::default(),
        cache: Default::default(),
        docs: None,
        repos: Vec::new(),
        preview: None,
        // No workspace image preference at init: the database pin applies
        // until the workspace names one (`g5d/config` D33).
        base_image: None,
    };

    // Write workspace.yaml
    let yaml_content = serde_yaml::to_string(&ws_config).map_err(|e| G5Error::Internal {
        message: format!("failed to serialize workspace config: {}", e),
    })?;
    tokio::fs::write(&workspace_yaml_path, &yaml_content)
        .await
        .map_err(|e| G5Error::Internal {
            message: format!("failed to write workspace.yaml: {}", e),
        })?;

    // Initialize workspace-meta git repository
    let meta_dir = g5_dir.join("workspace-meta");
    tokio::fs::create_dir_all(&meta_dir)
        .await
        .map_err(|e| G5Error::Internal {
            message: format!("failed to create workspace-meta directory: {}", e),
        })?;

    // Initialize git repo in workspace-meta
    match ops::init(&meta_dir, false) {
        Ok(repo) => {
            // Copy workspace.yaml into meta repo
            let meta_ws_yaml = meta_dir.join("workspace.yaml");
            tokio::fs::write(&meta_ws_yaml, &yaml_content)
                .await
                .map_err(|e| G5Error::Internal {
                    message: format!("failed to write workspace.yaml to meta repo: {}", e),
                })?;

            let author = GitAuthor {
                name: "g5d".to_string(),
                email: "g5d@local".to_string(),
            };
            let msg = format!("g5d init: created workspace {}", workspace_id);
            if let Err(e) = ops::checkpoint(&repo, &msg, &author) {
                warn!("Failed to commit workspace-meta: {}", e);
            }
        }
        Err(e) => {
            warn!("Failed to initialize workspace-meta git repo: {}", e);
        }
    }

    // 6. Docs repo initialization
    let docs_dir = g5_dir.join("docs");
    if let Some(ref docs_repo_url) = args.docs_repo {
        match crate::cli::native_git::clone_checkout(
            docs_repo_url,
            &docs_dir,
            "",
            true,
            &GitAuth::None,
        )
        .await
        {
            Ok(_) => {
                info!(
                    "Initialized independent docs checkout from {}",
                    docs_repo_url
                );
            }
            Err(e) => {
                warn!(
                    "Failed to clone docs repo (non-fatal): {}. Workspace still created.",
                    e
                );
            }
        }
    } else {
        match ops::init(&docs_dir, false) {
            Ok(repo) => {
                let readme_path = docs_dir.join("README.md");
                if let Err(e) = std::fs::write(&readme_path, "# Workspace Docs\n") {
                    warn!("Failed to seed docs README: {}", e);
                } else if let Err(e) = repo.add_all() {
                    warn!("Failed to stage docs repo seed files: {}", e);
                } else {
                    let author = GitAuthor {
                        name: "g5d".to_string(),
                        email: "g5d@local".to_string(),
                    };
                    if let Err(e) = repo.commit("g5d init: create docs repo", &author) {
                        warn!("Failed to create initial docs repo commit: {}", e);
                    } else {
                        info!("Initialized local docs repo at {}", docs_dir.display());
                    }
                }
            }
            Err(e) => {
                warn!(
                    "Failed to initialize local docs repo (non-fatal): {}. Workspace still created.",
                    e
                );
            }
        }
    }

    // 7. Print success message
    #[derive(serde::Serialize)]
    struct InitResult {
        status: String,
        workspace_id: String,
        workspace_path: String,
        remote_enabled: bool,
    }

    let result = InitResult {
        status: "success".to_string(),
        workspace_id: workspace_id.clone(),
        workspace_path: target_path.display().to_string(),
        remote_enabled,
    };

    if global.json {
        println!("{}", format_success(&result, true));
    } else if !global.quiet {
        println!("✓ Workspace initialized at {}", target_path.display());
        println!("  Workspace ID: {}", workspace_id);
        if remote_enabled {
            println!("  Remote: enabled");
        } else {
            println!("  Remote: disabled (use --api-url to enable)");
        }
    }

    Ok(())
}

fn validate_workspace_envelope_root(target_path: &Path) -> Result<(), G5Error> {
    if target_path.join(".git").exists() {
        return Err(G5Error::ValidationError {
            details: serde_json::json!({
                "path": target_path,
                "error": "workspace root must be an envelope containing repositories in child directories, not a Git repository"
            }),
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workspace_envelope_root_rejects_git_repository() {
        let root = tempfile::tempdir().expect("workspace root");
        validate_workspace_envelope_root(root.path()).expect("empty envelope root should pass");

        std::fs::create_dir(root.path().join(".git")).expect("git marker");
        let error = validate_workspace_envelope_root(root.path())
            .expect_err("Git repository root must fail closed");
        assert!(error
            .to_string()
            .contains("workspace root must be an envelope"));
    }
}
