//! `g5d clone` — Materialize a workspace from g5-api.
//!
//! Fetches a workspace manifest from g5-api, clones all repos using COW
//! where possible, initializes the independent docs checkout, and sets up the workspace
//! meta repo.

use std::path::PathBuf;

use g5_contracts::error::G5Error;
use g5_scm::GitAuth;
use tracing::{info, warn};
use uuid::Uuid;

use crate::cli::config::WorkspaceConfig;

use super::api_client::ApiClient;
use super::workspace_manifest::{
    apply_workspace_branch_commit_pins, clone_workspace_remote, fetch_workspace_branch_pins,
    fetch_workspace_manifest, materialize_workspace_docs, materialize_workspace_repo,
    parse_workspace_url, resolve_workspace_slug, serialize_workspace_manifest, sync_workspace_meta,
    workspace_manifest_needs_commit_pins, write_workspace_manifest_files,
};
use super::{format_success, resolve_workspace_path, CloneArgs, GlobalFlags};

fn target_workspace_name(path: &std::path::Path) -> Result<String, G5Error> {
    path.file_name()
        .map(|name| name.to_string_lossy().trim().to_string())
        .filter(|name| !name.is_empty())
        .ok_or_else(|| G5Error::Internal {
            message: format!(
                "Cannot derive new workspace name from target path '{}'",
                path.display()
            ),
        })
}

fn is_direct_workspace_ref(workspace_ref: &str, parsed_workspace_url: bool) -> bool {
    parsed_workspace_url || Uuid::parse_str(workspace_ref).is_ok()
}

fn load_current_workspace_path(global: &GlobalFlags) -> Result<Option<PathBuf>, G5Error> {
    let cwd = std::env::current_dir().map_err(|e| G5Error::Internal {
        message: format!("failed to get current directory: {}", e),
    })?;
    Ok(resolve_workspace_path(global.workspace.as_ref(), &cwd))
}

async fn resolve_auth_token(
    api_url: &str,
    args: &CloneArgs,
    current_workspace_path: Option<&std::path::Path>,
) -> Result<String, G5Error> {
    if let Some(token) = args
        .token
        .as_deref()
        .and_then(non_empty_token)
        .or_else(|| {
            std::env::var("G5D_TOKEN")
                .ok()
                .and_then(|s| non_empty_token(&s))
        })
        .or_else(|| {
            std::env::var("G5_API_KEY")
                .ok()
                .and_then(|s| non_empty_token(&s))
        })
    {
        return Ok(token);
    }

    let client = if let Some(workspace_path) = current_workspace_path {
        ApiClient::new(api_url, workspace_path)
    } else {
        ApiClient::new_global(api_url)
    };
    client.ensure_auth_token().await
}

fn non_empty_token(token: &str) -> Option<String> {
    let trimmed = token.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

/// A bare name resolves like a URL's workspace segment: the current name or
/// a former name the workspace still reserves (`g5.workspace.former-names`).
async fn resolve_workspace_in_org(
    api_url: &str,
    org_id: &str,
    workspace_name: &str,
    token: &str,
) -> Result<String, G5Error> {
    resolve_workspace_slug(api_url, org_id, workspace_name, Some(token))
        .await?
        .workspace_id
        .ok_or_else(|| G5Error::NotFound {
            resource: format!("workspace named {workspace_name}"),
        })
}

async fn resolve_workspace_id(
    api_url: &str,
    workspace_ref: &str,
    current_workspace_path: Option<&std::path::Path>,
    token: &str,
) -> Result<String, G5Error> {
    if let Some(parts) = parse_workspace_url(workspace_ref) {
        let resolved = resolve_workspace_slug(
            &parts.api_url,
            &parts.org_slug,
            &parts.workspace_name,
            Some(token),
        )
        .await?;
        return resolved.workspace_id.ok_or_else(|| G5Error::NotFound {
            resource: format!("workspace {}/{}", parts.org_slug, parts.workspace_name),
        });
    }

    if Uuid::parse_str(workspace_ref).is_ok() {
        return Ok(workspace_ref.to_string());
    }

    if let Some(workspace_path) = current_workspace_path {
        let client = ApiClient::new(api_url, workspace_path);
        if client.ensure_auth().await.is_ok() {
            if let Ok(org_id) = client.org_id() {
                return resolve_workspace_in_org(api_url, &org_id, workspace_ref, token).await;
            }
        }
        // Ambient-workspace auth is unusable for org-scoped name resolution
        // (e.g. an explicit --token/env token forced from inside an unrelated
        // or unauthenticated workspace). Per the clone auth-source contract
        // (cli D4: explicit tokens override stored auth so operators can force a
        // token), pass the ref through to the server instead of failing closed.
        return Ok(workspace_ref.to_string());
    }

    let client = ApiClient::new_global(api_url);
    if client.ensure_auth().await.is_ok() {
        if let Ok(org_id) = client.org_id() {
            return resolve_workspace_in_org(api_url, &org_id, workspace_ref, token).await;
        }
    }

    Ok(workspace_ref.to_string())
}

/// Execute the `g5d clone` command.
///
/// 1. Determine target path (argument or workspace ID as directory name).
/// 2. Authenticate with g5-api.
/// 3. Fetch workspace manifest from g5-api.
/// 4. Create `.g5/workspace.yaml` from the manifest.
/// 5. Initialize `.g5/workspace-meta/` git repository.
/// 6. Clone repos using COW where possible.
/// 7. Initialize the independent docs checkout.
/// 8. Commit initial state to workspace meta repo.
/// 9. Print success message.
pub async fn execute(args: &CloneArgs, global: &GlobalFlags) -> Result<(), G5Error> {
    let parsed_workspace_url = parse_workspace_url(&args.workspace_ref);
    let current_workspace_path = load_current_workspace_path(global)?;

    // 0. Resolve the logical workspace ID and API URL.
    let api_url = global
        .api_url
        .clone()
        .or_else(|| {
            current_workspace_path
                .as_ref()
                .and_then(|path| WorkspaceConfig::load(path).ok().flatten())
                .and_then(|config| config.remote.api_url)
        })
        .or_else(|| parsed_workspace_url.as_ref().map(|url| url.api_url.clone()))
        .or_else(|| std::env::var("G5D_API_URL").ok().filter(|s| !s.is_empty()))
        .ok_or_else(|| G5Error::Internal {
            message: "No API URL configured. Use --api-url or G5D_API_URL".to_string(),
        })?;

    // 1. Determine target path
    let target_path = if let Some(ref path) = args.path {
        path.clone()
    } else if let Some(parts) = parsed_workspace_url.as_ref() {
        PathBuf::from(&parts.workspace_name)
    } else {
        PathBuf::from(&args.workspace_ref)
    };

    let token = resolve_auth_token(&api_url, args, current_workspace_path.as_deref()).await?;
    let source_workspace_id = resolve_workspace_id(
        &api_url,
        &args.workspace_ref,
        current_workspace_path.as_deref(),
        &token,
    )
    .await?;

    // Check target directory doesn't already exist
    if target_path.exists() {
        let msg = format!("Target directory already exists: {}", target_path.display());
        return Err(G5Error::Internal { message: msg });
    }

    let should_create_server_clone = args.path.is_some()
        && !is_direct_workspace_ref(&args.workspace_ref, parsed_workspace_url.is_some());

    let workspace_id = if should_create_server_clone {
        let clone_name = target_workspace_name(&target_path)?;
        let clone =
            clone_workspace_remote(&api_url, &source_workspace_id, &clone_name, Some(&token))
                .await?;
        clone.workspace.id
    } else {
        source_workspace_id
    };

    info!(
        "Cloning workspace {} from {} to {}",
        workspace_id,
        api_url,
        target_path.display()
    );

    // 4. Fetch workspace manifest from g5-api
    let mut manifest = fetch_workspace_manifest(&api_url, &workspace_id, Some(&token)).await?;
    if workspace_manifest_needs_commit_pins(&manifest) {
        let branch_pins =
            fetch_workspace_branch_pins(&api_url, &workspace_id, Some(&token)).await?;
        apply_workspace_branch_commit_pins(&mut manifest, &branch_pins)?;
    }
    // Persist the opt-in so refresh/switch/push keep the same local labels.
    manifest.clone.checkout_remote_names = args.checkout_remote_names;
    let checkout_remote_names = manifest.clone.checkout_remote_names;
    let manifest_body = serialize_workspace_manifest(&manifest)?;

    // 5. Create workspace directory structure + workspace.yaml
    write_workspace_manifest_files(&target_path, &manifest_body).await?;

    // 7. Clone repos from manifest (parse workspace.yaml to find repos)
    let mut repo_count = 0u32;
    let mut materialization_errors = Vec::new();
    let git_auth = GitAuth::Token(token.clone());
    for repo_entry in &manifest.repos {
        let repo_path = target_path.join(&repo_entry.path);
        info!(
            "Materializing repo {} to {}",
            repo_entry.url,
            repo_path.display()
        );

        match materialize_workspace_repo(&target_path, repo_entry, &git_auth, checkout_remote_names)
            .await
        {
            Ok(()) => {
                repo_count += 1;
                info!("Successfully materialized {}", repo_entry.name);
            }
            Err(e) => {
                materialization_errors.push(format!("{}: {}", repo_entry.name, e));
                warn!("Failed to materialize repo {}: {}", repo_entry.name, e);
            }
        }
    }

    if let Some(ref docs) = manifest.docs {
        info!("Materializing docs repo {} to {}", docs.repo_url, docs.path);
        match materialize_workspace_docs(&target_path, docs, &git_auth).await {
            Ok(()) => {
                repo_count += 1;
                info!("Successfully materialized workspace docs");
            }
            Err(e) => {
                materialization_errors.push(format!("docs: {}", e));
                warn!("Failed to materialize docs repo: {}", e);
            }
        }
    }

    if !materialization_errors.is_empty() {
        return Err(G5Error::Internal {
            message: format!(
                "Failed to materialize {} repo(s): {}",
                materialization_errors.len(),
                materialization_errors.join("; ")
            ),
        });
    }

    // 8. Initialize or update workspace-meta
    sync_workspace_meta(
        &target_path,
        &manifest_body,
        &format!("g5d clone: materialized workspace {}", workspace_id),
    )
    .await?;

    // 9. Print success message
    #[derive(serde::Serialize)]
    struct CloneResult {
        status: String,
        workspace_id: String,
        workspace_path: String,
        repos_cloned: u32,
    }

    let result = CloneResult {
        status: "success".to_string(),
        workspace_id: workspace_id.clone(),
        workspace_path: target_path.display().to_string(),
        repos_cloned: repo_count,
    };

    if global.json {
        println!("{}", format_success(&result, true));
    } else if !global.quiet {
        println!(
            "✓ Workspace cloned to {} ({} repos)",
            target_path.display(),
            repo_count
        );
    }

    Ok(())
}
