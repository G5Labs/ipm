//! `g5d add` — Add a repository to the workspace.
//!
//! Clones a repository using COW where possible, updates workspace config,
//! and optionally pushes the change to g5-api.

use std::path::{Component, Path, PathBuf};

use g5_contracts::error::G5Error;
use g5_scm::workspace as ops;
use g5_scm::{GitAuth, GitAuthor, GitRepo};
use tracing::{info, warn};

use crate::cli::config::{WorkspaceConfig, WorkspaceRepoEntry};

use super::api_client::ApiClient;
use super::workspace_manifest::refresh_workspace_from_api;
use super::workspace_manifest::{
    add_workspace_repo_remote, fetch_workspace_detail, fetch_workspace_manifest, list_org_repos,
    materialize_workspace_repo, sync_workspace_meta, write_workspace_manifest_files,
    AddWorkspaceRepoRemoteRequest,
};
use super::{format_success, resolve_workspace_path, AddArgs, GlobalFlags};

/// Execute the `g5d add` command.
///
/// 1. Load current workspace config.
/// 2. Parse repo URL and determine name.
/// 3. Clone repo using COW where possible.
/// 4. Update `.g5/workspace.yaml`.
/// 5. If remote enabled, push change to g5-api.
/// 6. Commit change to workspace meta repo.
/// 7. Print success message.
pub async fn execute(args: &AddArgs, global: &GlobalFlags) -> Result<(), G5Error> {
    // 1. Find workspace
    let cwd = std::env::current_dir().map_err(|e| G5Error::Internal {
        message: format!("failed to get current directory: {}", e),
    })?;

    let workspace_path =
        resolve_workspace_path(global.workspace.as_ref(), &cwd).ok_or_else(|| {
            G5Error::Internal {
                message: "No workspace found. Run from a workspace directory or use --workspace."
                    .to_string(),
            }
        })?;

    // 2. Load workspace config
    let mut ws_config = WorkspaceConfig::load(&workspace_path)
        .map_err(|e| G5Error::Internal {
            message: format!("Failed to load workspace config: {}", e),
        })?
        .ok_or_else(|| G5Error::Internal {
            message: "No workspace.yaml found".to_string(),
        })?;

    if args.local {
        return execute_add_local(args, global, &workspace_path, &ws_config).await;
    }

    let repo_url = args
        .repo_url
        .as_deref()
        .ok_or_else(|| G5Error::ValidationError {
            details: serde_json::json!({
                "field": "repoUrl",
                "error": "Repository URL is required unless --local is supplied"
            }),
        })?;

    // 3. Determine repo name from --name flag or inferred from URL
    let repo_name = args
        .name
        .clone()
        .unwrap_or_else(|| infer_repo_name(repo_url));

    // Check if repo already exists in workspace
    if ws_config.repos.iter().any(|r| r.name == repo_name) {
        return Err(G5Error::Internal {
            message: format!("Repository '{}' already exists in workspace", repo_name),
        });
    }

    // 4. Determine clone path
    let clone_path = if let Some(ref path) = args.path {
        path.clone()
    } else {
        PathBuf::from(&repo_name)
    };
    validate_workspace_repo_path(&clone_path)?;
    let full_clone_path = workspace_path.join(&clone_path);

    // Validate that clone path doesn't escape workspace root
    let canonical_workspace = workspace_path
        .canonicalize()
        .unwrap_or_else(|_| workspace_path.to_path_buf());
    let canonical_clone = canonicalize_maybe_missing_path(&full_clone_path);
    if !canonical_clone.starts_with(&canonical_workspace) {
        return Err(G5Error::ValidationError {
            details: serde_json::json!({"error": "Clone path must be within the workspace directory"}),
        });
    }

    if let Some(remote_branch) = args.track_remote.as_deref() {
        validate_track_remote_branch_arg(args.branch.as_deref(), remote_branch)?;
        return execute_remote_tracking_add(
            args,
            global,
            &workspace_path,
            &ws_config,
            &repo_name,
            &clone_path,
            remote_branch,
        )
        .await;
    }

    // 5. Clone the repo
    info!("Cloning {} to {}", repo_url, full_clone_path.display());

    let inferred_default_branch = if args.branch.is_none() {
        infer_remote_default_branch(repo_url).await?
    } else {
        None
    };
    let clone_branch = args
        .branch
        .as_deref()
        .or(inferred_default_branch.as_deref())
        .unwrap_or("");
    let repo = crate::cli::native_git::clone_checkout(
        repo_url,
        &full_clone_path,
        clone_branch,
        true,
        &GitAuth::None,
    )
    .await
    .map_err(|e| G5Error::GitError {
        message: format!("Failed to clone {}: {}", repo_url, e),
    })?;

    // Checkout specified branch if provided and not already on it
    if let Some(ref branch) = args.branch {
        if let Ok(current) = ops::current_branch(&repo) {
            if current != *branch {
                ops::checkout_branch(&repo, branch)?;
            }
        }
    }

    // 6. Determine the branch name
    let branch_name = args
        .branch
        .clone()
        .or(inferred_default_branch)
        .or_else(|| repo.default_branch())
        .unwrap_or_else(|| "main".to_string());

    // 7. Update workspace.yaml
    ws_config.repos.push(WorkspaceRepoEntry {
        name: repo_name.clone(),
        url: repo_url.to_string(),
        repo_id: None,
        branch: branch_name.clone(),
        path: clone_path.display().to_string(),
        commit: None,
        remote_branch: None,
    });

    let mut yaml_content = serde_yaml::to_string(&ws_config).map_err(|e| G5Error::Internal {
        message: format!("Failed to serialize workspace config: {}", e),
    })?;

    let workspace_yaml_path = workspace_path.join(".g5").join("workspace.yaml");
    tokio::fs::write(&workspace_yaml_path, &yaml_content)
        .await
        .map_err(|e| G5Error::Internal {
            message: format!("Failed to write workspace.yaml: {}", e),
        })?;

    // 8. If remote enabled, register the repo with g5-api and refresh from the manifest.
    if ws_config.remote.enabled {
        let api_url = add_api_url_from_sources(
            global.api_url.clone(),
            std::env::var("G5D_API_URL").ok().filter(|s| !s.is_empty()),
            ws_config.remote.api_url.clone(),
        );
        if let Some(api_url) = api_url {
            if let Some(workspace_id) = ws_config.workspace_id.as_deref() {
                // Resolve auth through the ApiClient (workspace auth file /
                // refresh / G5D_TOKEN fallback), matching the --track-remote
                // path. Remote sync stays best-effort: an auth or sync failure
                // warns and leaves the completed local change in place.
                let client = ApiClient::new(&api_url, &workspace_path);
                match client.ensure_auth_token().await {
                    Ok(token) => {
                        match sync_remote_workspace_repo(
                            &api_url,
                            workspace_id,
                            repo_url,
                            &repo_name,
                            &branch_name,
                            &token,
                            None,
                            None,
                        )
                        .await
                        {
                            Ok(remote_manifest) => {
                                let manifest_body = serde_yaml::to_string(&remote_manifest)
                                    .map_err(|e| G5Error::Internal {
                                        message: format!(
                                            "Failed to serialize refreshed workspace manifest: {}",
                                            e
                                        ),
                                    })?;
                                write_workspace_manifest_files(&workspace_path, &manifest_body)
                                    .await?;
                                sync_workspace_meta(
                                    &workspace_path,
                                    &manifest_body,
                                    &format!("g5d add: synced remote repo {}", repo_name),
                                )
                                .await?;
                                yaml_content = manifest_body;
                                info!("Successfully synced workspace repo to g5-api");
                            }
                            Err(e) => {
                                warn!(
                                    "Failed to sync workspace repo to g5-api: {}. Local change succeeded.",
                                    e
                                );
                            }
                        }
                    }
                    Err(e) => {
                        warn!(
                            "Remote workspace is enabled but g5-api auth is unavailable ({}); run `g5d auth login` or set G5D_TOKEN. Local change succeeded without remote sync.",
                            e
                        );
                    }
                }
            } else {
                warn!(
                    "Workspace has remote enabled but no remote workspace_id yet. Local change will publish on first push."
                );
            }
        }
    }

    // 9. Commit change to workspace meta repo
    let meta_dir = workspace_path.join(".g5").join("workspace-meta");
    if meta_dir.exists() {
        let meta_ws_yaml = meta_dir.join("workspace.yaml");
        tokio::fs::write(&meta_ws_yaml, &yaml_content)
            .await
            .map_err(|e| G5Error::Internal {
                message: format!("Failed to write workspace.yaml to workspace-meta: {}", e),
            })?;

        let meta_msg = format!("g5d add: added repo {} from {}", repo_name, repo_url);
        if let Ok(meta_repo) = GitRepo::open(&meta_dir) {
            let author = GitAuthor {
                name: "g5d".to_string(),
                email: "g5d@local".to_string(),
            };
            if let Err(e) = ops::checkpoint(&meta_repo, &meta_msg, &author) {
                warn!("Failed to commit workspace-meta after add: {}", e);
            }
        }
    }

    // 10. Print success message
    #[derive(serde::Serialize)]
    struct AddResult {
        status: String,
        repo_name: String,
        repo_url: String,
        branch: String,
        path: String,
    }

    let result = AddResult {
        status: "success".to_string(),
        repo_name: repo_name.clone(),
        repo_url: repo_url.to_string(),
        branch: branch_name,
        path: clone_path.display().to_string(),
    };

    if global.json {
        println!("{}", format_success(&result, true));
    } else if !global.quiet {
        println!("✓ Added repo '{}' to workspace", repo_name);
        println!("  URL: {}", repo_url);
        println!("  Path: {}", clone_path.display());
    }

    Ok(())
}

async fn execute_add_local(
    args: &AddArgs,
    global: &GlobalFlags,
    workspace_path: &Path,
    ws_config: &WorkspaceConfig,
) -> Result<(), G5Error> {
    if args.repo_url.is_some() {
        return Err(G5Error::ValidationError {
            details: serde_json::json!({
                "field": "repoUrl",
                "error": "Do not pass a repository URL with --local"
            }),
        });
    }
    if args.path.is_some() {
        return Err(G5Error::ValidationError {
            details: serde_json::json!({
                "field": "path",
                "error": "--path is not supported with --local; the refreshed workspace manifest determines the checkout path"
            }),
        });
    }
    // G5-managed local repos have no upstream — reject `--track-remote` before
    // any API call, matching the server/browser rejection, instead of silently
    // dropping the user's remote-tracking request. Per Codex review (PR #450).
    if args.track_remote.is_some() {
        return Err(G5Error::ValidationError {
            details: serde_json::json!({
                "field": "track_remote",
                "error": "--track-remote is not supported with --local; G5-managed local repos have no upstream to track"
            }),
        });
    }

    // Resolve the API URL with the same CLI > env > workspace precedence the
    // other add paths use (`add_api_url_from_sources`), so a `G5D_API_URL`
    // override is honored even when the manifest records a (stale) URL. Per
    // Codex review (PR #450).
    let api_url = add_api_url_from_sources(
        global.api_url.clone(),
        std::env::var("G5D_API_URL").ok().filter(|s| !s.is_empty()),
        ws_config.remote.api_url.clone(),
    )
    .ok_or_else(|| G5Error::ValidationError {
        details: serde_json::json!({
            "field": "apiUrl",
            "error": "g5d add --local requires a remote API URL"
        }),
    })?;
    let workspace_id =
        ws_config
            .workspace_id
            .as_deref()
            .ok_or_else(|| G5Error::ValidationError {
                details: serde_json::json!({
                    "field": "workspaceId",
                    "error": "g5d add --local requires a remote-bound workspace"
                }),
            })?;
    let token = resolve_add_token(&api_url, workspace_path).await?;
    let workspace = fetch_workspace_detail(&api_url, workspace_id, Some(&token)).await?;
    // When `--name` is omitted, send no explicit name so the server derives a
    // workspace-based default and auto-uniquifies it when the implicit
    // workspace repo already occupies that name. The locally-derived value is
    // only a `g5-local://` URL seed, never the authoritative name — the
    // materialized name/URL come back from the server. [C3]
    let explicit_name = match args.name.as_deref() {
        Some(name) => Some(qualified_local_repo_name(name)?),
        None => None,
    };
    let url_seed = explicit_name
        .clone()
        .unwrap_or_else(|| default_local_repo_name(&workspace.name, workspace_id));
    let default_branch = args.branch.as_deref().unwrap_or("main");
    let git_url = format!("g5-local://workspaces/{workspace_id}/repos/{url_seed}.git");

    let added = add_workspace_repo_remote(AddWorkspaceRepoRemoteRequest {
        api_url: &api_url,
        org_id: &workspace.org_id,
        workspace_id,
        git_url: &git_url,
        name: explicit_name.as_deref(),
        default_branch: Some(default_branch),
        bootstrap_mode: Some("local"),
        branch_name: None,
        head_commit: None,
        repo_role: Some("code"),
        tracking_mode: None,
        remote_branch: None,
        token: Some(&token),
    })
    .await?;

    refresh_workspace_from_api(workspace_path, &api_url, Some(token)).await?;

    #[derive(serde::Serialize)]
    struct AddLocalResult {
        status: String,
        mode: String,
        repo_name: String,
        repo_url: String,
        branch: String,
    }

    let result = AddLocalResult {
        status: "success".to_string(),
        mode: "local".to_string(),
        repo_name: added.repo_name.clone(),
        repo_url: added.git_url.clone(),
        branch: default_branch.to_string(),
    };

    if global.json {
        println!("{}", format_success(&result, true));
    } else if !global.quiet {
        println!("✓ Added local repo '{}' to workspace", added.repo_name);
        println!("  URL: {}", added.git_url);
        println!("  Branch: {}", default_branch);
    }

    Ok(())
}

async fn resolve_add_token(api_url: &str, workspace_path: &Path) -> Result<String, G5Error> {
    if let Some(token) = std::env::var("G5D_TOKEN").ok().filter(|s| !s.is_empty()) {
        return Ok(token);
    }

    let client = ApiClient::new(api_url, workspace_path);
    match client.ensure_auth_token().await {
        Ok(token) => Ok(token),
        Err(G5Error::Unauthorized) => client.refresh_auth_token().await,
        Err(err) => Err(err),
    }
}

/// Infer a repository name from its URL.
///
/// Examples:
/// - `https://github.com/org/repo.git` → `repo`
/// - `git@github.com:org/repo.git` → `repo`
/// - `https://github.com/org/repo` → `repo`
fn infer_repo_name(url: &str) -> String {
    let url = url.trim_end_matches('/');
    let url = url.trim_end_matches(".git");

    // Handle SSH URLs: git@host:org/repo
    if let Some(pos) = url.rfind('/') {
        return url[pos + 1..].to_string();
    }
    if let Some(pos) = url.rfind(':') {
        return url[pos + 1..].to_string();
    }

    url.to_string()
}

fn canonicalize_maybe_missing_path(path: &Path) -> PathBuf {
    let mut cursor = path;

    while !cursor.exists() {
        match cursor.parent() {
            Some(parent) if parent != cursor => cursor = parent,
            _ => break,
        }
    }

    let mut resolved = cursor
        .canonicalize()
        .unwrap_or_else(|_| cursor.to_path_buf());
    if let Ok(suffix) = path.strip_prefix(cursor) {
        for component in suffix.components() {
            match component {
                Component::CurDir => {}
                Component::ParentDir => {
                    resolved.pop();
                }
                Component::Normal(part) => resolved.push(part),
                Component::RootDir | Component::Prefix(_) => {}
            }
        }
    }

    resolved
}

fn validate_workspace_repo_path(path: &Path) -> Result<(), G5Error> {
    let components = path.components().collect::<Vec<_>>();
    if components.len() != 1 || !matches!(components[0], Component::Normal(_)) {
        return Err(G5Error::ValidationError {
            details: serde_json::json!({
                "field": "path",
                "error": "Repository path must be exactly one direct child folder of the workspace"
            }),
        });
    }
    Ok(())
}

async fn infer_remote_default_branch(url: &str) -> Result<Option<String>, G5Error> {
    match crate::cli::native_git::runner()?
        .default_remote_branch(url, &GitAuth::None)
        .await
    {
        Ok(branch) => Ok(branch),
        // Discovery was already optional: clone remains the authoritative
        // operation when a remote does not expose its default branch.
        Err(error) if error.kind == g5_scm::native::FailureKind::Exit => {
            tracing::debug!(%error, "remote default branch unavailable");
            Ok(None)
        }
        Err(error) => Err(error.into()),
    }
}

fn normalize_repo_locator(value: &str) -> String {
    value
        .trim()
        .trim_end_matches('/')
        .trim_end_matches(".git")
        .to_lowercase()
}

fn repo_slug_from_name(name: &str) -> String {
    name.to_lowercase()
        .replace(|c: char| !c.is_alphanumeric() && c != '-', "-")
}

fn validate_track_remote_branch_arg(
    branch: Option<&str>,
    remote_branch: &str,
) -> Result<(), G5Error> {
    if remote_branch.trim().is_empty() {
        return Err(G5Error::ValidationError {
            details: serde_json::json!({
                "field": "track_remote",
                "error": "--track-remote requires a non-empty remote branch name"
            }),
        });
    }

    if let Some(branch) = branch {
        if branch != remote_branch {
            return Err(G5Error::ValidationError {
                details: serde_json::json!({
                    "field": "branch",
                    "error": "--branch and --track-remote may only be combined when their values are identical",
                    "branch": branch,
                    "trackRemote": remote_branch,
                }),
            });
        }
    }

    Ok(())
}

async fn remote_tracking_add_context(
    global: &GlobalFlags,
    ws_config: &WorkspaceConfig,
    workspace_path: &Path,
) -> Result<(String, String, String), G5Error> {
    if !ws_config.remote.enabled {
        return Err(G5Error::ValidationError {
            details: serde_json::json!({
                "error": "g5d add --track-remote requires a remote-enabled workspace"
            }),
        });
    }

    let api_url = add_api_url_from_sources(
        global.api_url.clone(),
        std::env::var("G5D_API_URL").ok().filter(|s| !s.is_empty()),
        ws_config.remote.api_url.clone(),
    );
    let api_url = api_url.ok_or_else(|| G5Error::ValidationError {
        details: serde_json::json!({
            "field": "api_url",
            "error": "g5d add --track-remote requires g5-api metadata; set remote.api_url or --api-url"
        }),
    })?;

    let workspace_id = ws_config
        .workspace_id
        .clone()
        .filter(|id| !id.trim().is_empty())
        .ok_or_else(|| G5Error::ValidationError {
            details: serde_json::json!({
                "field": "workspace_id",
                "error": "g5d add --track-remote requires a remote workspace_id"
            }),
        })?;

    let client = ApiClient::new(&api_url, workspace_path);
    let token = client.ensure_auth_token().await.map_err(|e| match e {
        G5Error::Unauthorized => G5Error::ValidationError {
            details: serde_json::json!({
                "field": "auth",
                "error": "g5d add --track-remote requires g5-api auth; run `g5d auth login` or set G5D_TOKEN"
            }),
        },
        other => other,
    })?;

    Ok((api_url, workspace_id, token))
}

fn add_api_url_from_sources(
    cli_api_url: Option<String>,
    env_api_url: Option<String>,
    workspace_api_url: Option<String>,
) -> Option<String> {
    cli_api_url.or(env_api_url).or(workspace_api_url)
}

#[allow(clippy::too_many_arguments)]
async fn execute_remote_tracking_add(
    args: &AddArgs,
    global: &GlobalFlags,
    workspace_path: &Path,
    ws_config: &WorkspaceConfig,
    repo_name: &str,
    clone_path: &Path,
    remote_branch: &str,
) -> Result<(), G5Error> {
    let (api_url, workspace_id, token) =
        remote_tracking_add_context(global, ws_config, workspace_path).await?;
    let repo_url = args
        .repo_url
        .as_deref()
        .ok_or_else(|| G5Error::ValidationError {
            details: serde_json::json!({
                "field": "repoUrl",
                "error": "g5d add --track-remote requires a repository URL"
            }),
        })?;

    let mut remote_manifest = sync_remote_workspace_repo(
        &api_url,
        &workspace_id,
        repo_url,
        repo_name,
        remote_branch,
        &token,
        Some("remote_tracking"),
        Some(remote_branch),
    )
    .await?;
    remote_manifest.clone.checkout_remote_names = ws_config.clone.checkout_remote_names;

    let repo_entry = remote_manifest
        .repos
        .iter_mut()
        .find(|repo| repo.name == repo_name)
        .ok_or_else(|| G5Error::Internal {
            message: format!(
                "g5-api added remote-tracking repo '{repo_name}' but the refreshed manifest did not include it"
            ),
        })?;
    repo_entry.path = clone_path.display().to_string();
    let repo_entry = repo_entry.clone();
    let checkout_remote_names = remote_manifest.clone.checkout_remote_names;

    let git_auth = GitAuth::Token(token.clone());
    materialize_workspace_repo(
        workspace_path,
        &repo_entry,
        &git_auth,
        checkout_remote_names,
    )
    .await?;

    let manifest_body = serde_yaml::to_string(&remote_manifest).map_err(|e| G5Error::Internal {
        message: format!("Failed to serialize refreshed workspace manifest: {}", e),
    })?;
    write_workspace_manifest_files(workspace_path, &manifest_body).await?;
    sync_workspace_meta(
        workspace_path,
        &manifest_body,
        &format!("g5d add: synced remote-tracking repo {}", repo_name),
    )
    .await?;

    #[derive(serde::Serialize)]
    struct AddRemoteTrackingResult {
        status: String,
        repo_name: String,
        repo_url: String,
        branch: String,
        remote_branch: String,
        path: String,
    }

    let result = AddRemoteTrackingResult {
        status: "success".to_string(),
        repo_name: repo_name.to_string(),
        repo_url: repo_url.to_string(),
        branch: repo_entry.branch.clone(),
        remote_branch: repo_entry
            .remote_branch
            .clone()
            .unwrap_or_else(|| remote_branch.to_string()),
        path: repo_entry.path.clone(),
    };

    if global.json {
        println!("{}", format_success(&result, true));
    } else if !global.quiet {
        println!("✓ Added remote-tracking repo '{}' to workspace", repo_name);
        println!("  URL: {}", repo_url);
        println!("  Branch: {}", result.branch);
        println!("  Remote branch: {}", result.remote_branch);
        println!("  Path: {}", result.path);
    }

    Ok(())
}

fn qualified_local_repo_name(value: &str) -> Result<String, G5Error> {
    let name = local_repo_name_token(value);
    if name.is_empty() {
        return Err(G5Error::ValidationError {
            details: serde_json::json!({
                "field": "name",
                "error": "Repository name must include a letter or number"
            }),
        });
    }
    Ok(name)
}

fn local_repo_name_token(value: &str) -> String {
    let mut token = String::new();
    let mut prev_separator = false;
    for c in value.trim().chars() {
        if c.is_ascii_alphanumeric() {
            token.push(c.to_ascii_lowercase());
            prev_separator = false;
        } else if !prev_separator && !token.is_empty() {
            token.push('_');
            prev_separator = true;
        }
    }
    token.trim_matches('_').to_string()
}

fn default_local_repo_name(workspace_name: &str, workspace_id: &str) -> String {
    let base = local_repo_name_token(workspace_name);
    let base = if base.is_empty() {
        "workspace".to_string()
    } else {
        base
    };
    let uniquifier = workspace_id
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .map(|c| c.to_ascii_lowercase())
        .take(8)
        .collect::<String>();
    let uniquifier = if uniquifier.is_empty() {
        "local".to_string()
    } else {
        uniquifier
    };
    format!("{base}_code_{uniquifier}")
}

#[allow(clippy::too_many_arguments)]
async fn sync_remote_workspace_repo(
    api_url: &str,
    workspace_id: &str,
    git_url: &str,
    repo_name: &str,
    default_branch: &str,
    token: &str,
    tracking_mode: Option<&str>,
    remote_branch: Option<&str>,
) -> Result<WorkspaceConfig, G5Error> {
    let workspace = fetch_workspace_detail(api_url, workspace_id, Some(token)).await?;
    let normalized_git_url = normalize_repo_locator(git_url);
    let normalized_slug = repo_slug_from_name(repo_name);
    let remote_tracking = tracking_mode == Some("remote_tracking") || remote_branch.is_some();

    match add_workspace_repo_remote(AddWorkspaceRepoRemoteRequest {
        api_url,
        org_id: &workspace.org_id,
        workspace_id,
        git_url,
        name: Some(repo_name),
        default_branch: Some(default_branch),
        bootstrap_mode: None,
        branch_name: None,
        head_commit: None,
        repo_role: None,
        tracking_mode,
        remote_branch,
        token: Some(token),
    })
    .await
    {
        Ok(_) => {}
        Err(G5Error::Conflict { reason }) if remote_tracking => {
            return Err(G5Error::Conflict {
                reason: format!(
                    "remote-tracking repo '{repo_name}' could not be added without losing remoteBranch metadata: {reason}"
                ),
            });
        }
        Err(G5Error::Conflict { .. }) => {
            let existing_repo = list_org_repos(api_url, &workspace.org_id, Some(token))
                .await?
                .into_iter()
                .find(|repo| {
                    normalize_repo_locator(&repo.git_url) == normalized_git_url
                        || normalize_repo_locator(&repo.name) == normalize_repo_locator(repo_name)
                        || normalize_repo_locator(&repo.slug) == normalized_slug
                })
                .ok_or_else(|| G5Error::Conflict {
                    reason: format!(
                        "workspace {workspace_id} repo '{repo_name}' conflicts with an existing org repo that could not be resolved"
                    ),
                })?;

            return Err(G5Error::Conflict {
                reason: format!("repository '{}' already exists in this organization ({}); workspace membership must be established through workspace creation or fork", existing_repo.name, existing_repo.id),
            });
        }
        Err(e) => return Err(e),
    }

    fetch_workspace_manifest(api_url, workspace_id, Some(token)).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn add_discovers_git_daemon_default_without_global_config_writes() {
        const CASE: &str = "G5_CLI_DAEMON_ADD_CASE";
        if let Ok(url) = std::env::var(CASE) {
            use clap::Parser;
            let cli =
                crate::cli::Cli::try_parse_from(["g5d", "--quiet", "add", &url, "--name", "added"])
                    .unwrap();
            let crate::cli::Command::Add(args) = cli.command else {
                unreachable!()
            };
            execute(&args, &cli.global).await.unwrap();
            let cwd = std::env::current_dir().unwrap();
            let config = WorkspaceConfig::load(&cwd).unwrap().unwrap();
            assert_eq!(config.repos[0].branch, "topic");
            let mut materialized = config.repos[0].clone();
            materialized.path = "materialized".into();
            materialize_workspace_repo(&cwd, &materialized, &GitAuth::None, false)
                .await
                .unwrap();
            materialize_workspace_repo(&cwd, &materialized, &GitAuth::None, false)
                .await
                .unwrap();
            assert_eq!(
                std::fs::read_to_string(cwd.join("materialized/content.txt")).unwrap(),
                "daemon content"
            );
            assert_eq!(
                std::fs::read_to_string(cwd.join("added/content.txt")).unwrap(),
                "daemon content"
            );
            return;
        }
        use std::process::{Command, Stdio};
        use std::time::{Duration, Instant};
        struct Daemon(std::process::Child);
        impl Drop for Daemon {
            fn drop(&mut self) {
                let _ = self.0.kill();
                let _ = self.0.wait();
            }
        }
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("source");
        let workspace = dir.path().join("workspace");
        let home = dir.path().join("home");
        for path in [&source, &workspace.join(".g5"), &home] {
            std::fs::create_dir_all(path).unwrap();
        }
        let git = |args: &[&str]| {
            let output = Command::new("git")
                .args(args)
                .current_dir(&source)
                .env("GIT_AUTHOR_NAME", "Test")
                .env("GIT_AUTHOR_EMAIL", "test@example.com")
                .env("GIT_COMMITTER_NAME", "Test")
                .env("GIT_COMMITTER_EMAIL", "test@example.com")
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
        };
        git(&["init", "--initial-branch=topic"]);
        std::fs::write(source.join("content.txt"), "daemon content").unwrap();
        git(&["add", "content.txt"]);
        git(&["commit", "-m", "daemon"]);
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);
        let mut daemon = Daemon(
            Command::new("git")
                .args([
                    "daemon",
                    "--export-all",
                    "--reuseaddr",
                    "--listen=127.0.0.1",
                ])
                .arg(format!("--port={port}"))
                .arg(format!("--base-path={}", dir.path().display()))
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap(),
        );
        let deadline = Instant::now() + Duration::from_secs(5);
        while std::net::TcpStream::connect(("127.0.0.1", port)).is_err() {
            assert!(
                daemon.0.try_wait().unwrap().is_none(),
                "Git daemon exited before readiness"
            );
            assert!(Instant::now() < deadline, "Git daemon never listened");
            std::thread::sleep(Duration::from_millis(10));
        }
        std::fs::write(
            workspace.join(".g5/workspace.yaml"),
            "workspace_id: null\nremote:\n  enabled: false\nrepos: []\n",
        )
        .unwrap();
        let global = home.join(".gitconfig");
        let system = home.join("system.gitconfig");
        let global_bytes = b"[user]\n  name = Unchanged\n  email = unchanged@example.com\n[safe]\n  directory = /unchanged\n";
        let system_bytes = b"[core]\n  autocrlf = false\n";
        std::fs::write(&global, global_bytes).unwrap();
        std::fs::write(&system, system_bytes).unwrap();
        let output = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "cli::add::tests::add_discovers_git_daemon_default_without_global_config_writes",
                "--nocapture",
            ])
            .env(CASE, format!("git://127.0.0.1:{port}/source"))
            .env("HOME", &home)
            .env("GIT_CONFIG_GLOBAL", &global)
            .env("GIT_CONFIG_SYSTEM", &system)
            .env_remove(g5_contracts::workspace_git::SOCKET_ENV)
            .current_dir(&workspace)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(std::fs::read(&global).unwrap(), global_bytes);
        assert_eq!(std::fs::read(&system).unwrap(), system_bytes);
    }

    #[test]
    fn test_infer_repo_name_https() {
        assert_eq!(
            infer_repo_name("https://github.com/org/my-repo.git"),
            "my-repo"
        );
    }

    #[test]
    fn test_infer_repo_name_https_no_git() {
        assert_eq!(infer_repo_name("https://github.com/org/my-repo"), "my-repo");
    }

    #[test]
    fn test_infer_repo_name_ssh() {
        assert_eq!(infer_repo_name("git@github.com:org/my-repo.git"), "my-repo");
    }

    #[test]
    fn test_infer_repo_name_trailing_slash() {
        assert_eq!(
            infer_repo_name("https://github.com/org/my-repo/"),
            "my-repo"
        );
    }

    #[test]
    fn test_normalize_repo_locator() {
        assert_eq!(
            normalize_repo_locator("https://github.com/openai/G5.git/"),
            "https://github.com/openai/g5"
        );
    }

    #[test]
    fn test_default_local_repo_name_uses_workspace_name_and_short_id() {
        assert_eq!(
            default_local_repo_name("My Service!", "123e4567-e89b-12d3-a456-426614174000"),
            "my_service_code_123e4567"
        );
    }

    #[test]
    fn test_qualified_local_repo_name_normalizes_url_safe_token() {
        assert_eq!(
            qualified_local_repo_name("Custom Repo ++ API")
                .expect("valid local repo name should normalize"),
            "custom_repo_api"
        );
    }

    #[test]
    fn test_qualified_local_repo_name_rejects_empty_token() {
        assert!(qualified_local_repo_name("!!!").is_err());
    }

    #[test]
    fn test_canonicalize_maybe_missing_path_preserves_symlinked_workspace_child() {
        let temp = tempfile::TempDir::new().expect("temp dir");
        let real_workspace = temp.path().join("real-workspace");
        std::fs::create_dir_all(&real_workspace).expect("real workspace dir");
        let linked_workspace = temp.path().join("linked-workspace");

        #[cfg(unix)]
        std::os::unix::fs::symlink(&real_workspace, &linked_workspace).expect("workspace symlink");
        #[cfg(windows)]
        std::os::windows::fs::symlink_dir(&real_workspace, &linked_workspace)
            .expect("workspace symlink");

        let clone_path = linked_workspace.join("repos").join("remote");
        let canonical_workspace = linked_workspace
            .canonicalize()
            .expect("canonical workspace");
        let canonical_real_workspace = real_workspace
            .canonicalize()
            .expect("canonical real workspace");
        let canonical_clone = canonicalize_maybe_missing_path(&clone_path);

        assert!(canonical_clone.starts_with(&canonical_workspace));
        assert_eq!(
            canonical_clone,
            canonical_real_workspace.join("repos").join("remote")
        );
    }

    #[test]
    fn test_canonicalize_maybe_missing_path_resolves_parent_escape() {
        let temp = tempfile::TempDir::new().expect("temp dir");
        let workspace = temp.path().join("workspace");
        std::fs::create_dir_all(&workspace).expect("workspace dir");

        let clone_path = workspace
            .join("repos")
            .join("..")
            .join("..")
            .join("outside");
        let canonical_workspace = workspace.canonicalize().expect("canonical workspace");
        let canonical_clone = canonicalize_maybe_missing_path(&clone_path);

        assert!(!canonical_clone.starts_with(&canonical_workspace));
    }

    #[test]
    fn test_validate_workspace_repo_path_accepts_only_one_direct_child() {
        assert!(validate_workspace_repo_path(Path::new("code")).is_ok());
        for invalid in ["", ".", "..", "repos/code", "../code", "/code"] {
            assert!(
                validate_workspace_repo_path(Path::new(invalid)).is_err(),
                "path {invalid:?} must not describe a direct workspace child"
            );
        }
    }

    #[test]
    fn add_api_url_uses_cli_then_env_then_workspace_precedence() {
        assert_eq!(
            add_api_url_from_sources(
                Some("https://cli.example".to_string()),
                Some("https://env.example".to_string()),
                Some("https://workspace.example".to_string()),
            )
            .as_deref(),
            Some("https://cli.example")
        );
        assert_eq!(
            add_api_url_from_sources(
                None,
                Some("https://env.example".to_string()),
                Some("https://workspace.example".to_string()),
            )
            .as_deref(),
            Some("https://env.example")
        );
        assert_eq!(
            add_api_url_from_sources(None, None, Some("https://workspace.example".to_string()))
                .as_deref(),
            Some("https://workspace.example")
        );
    }
}
