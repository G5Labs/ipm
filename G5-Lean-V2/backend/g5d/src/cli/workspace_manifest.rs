use std::path::Path;

use g5_contracts::error::{ErrorResponse, G5Error};
use g5_scm::workspace as ops;
use g5_scm::{GitAuth, GitAuthor, GitRepo};
use serde::Deserialize;
use tracing::warn;
use uuid::Uuid;

use crate::cli::config::{WorkspaceConfig, WorkspaceDocsConfig, WorkspaceRepoEntry};

#[derive(Debug, Deserialize)]
struct WorkspaceManifestEnvelope {
    data: WorkspaceConfig,
}

#[derive(Debug, Deserialize)]
struct ResponseEnvelope<T> {
    data: T,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceResolveResponse {
    pub org_id: String,
    pub workspace_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceDetailResponse {
    pub id: String,
    pub org_id: String,
    pub name: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OrgRepoResponse {
    pub id: String,
    pub name: String,
    pub slug: String,
    pub git_url: String,
    pub default_branch: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddWorkspaceRepoResponse {
    pub repo_id: String,
    pub repo_name: String,
    pub git_url: String,
    pub branch: WorkspaceBranchRow,
}

#[derive(Debug, Clone, Copy)]
pub struct AddWorkspaceRepoRemoteRequest<'a> {
    pub api_url: &'a str,
    pub org_id: &'a str,
    pub workspace_id: &'a str,
    pub git_url: &'a str,
    pub name: Option<&'a str>,
    pub default_branch: Option<&'a str>,
    pub bootstrap_mode: Option<&'a str>,
    pub branch_name: Option<&'a str>,
    pub head_commit: Option<&'a str>,
    pub repo_role: Option<&'a str>,
    pub tracking_mode: Option<&'a str>,
    pub remote_branch: Option<&'a str>,
    pub token: Option<&'a str>,
}

#[derive(Debug, Clone)]
pub struct WorkspaceUrlParts {
    pub api_url: String,
    pub org_slug: String,
    pub workspace_name: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CloneWorkspaceResponse {
    pub workspace: CloneWorkspaceDetail,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CloneWorkspaceDetail {
    pub id: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceBranchRow {
    pub workspace_id: String,
    pub repo_id: String,
    #[serde(default)]
    pub repo_name: String,
    pub branch_name: String,
    pub head_commit: String,
    #[serde(default)]
    pub materialization_status: String,
    #[serde(default)]
    pub has_draft: bool,
    #[serde(default)]
    pub dirty_file_count: i32,
    #[serde(default)]
    pub dirty_files: Vec<String>,
    #[serde(default)]
    pub local_branch_name: Option<String>,
    #[serde(default)]
    pub tracking_mode: Option<String>,
}

pub type WorkspaceBranchPin = WorkspaceBranchRow;

#[derive(Debug, Clone, Copy)]
pub struct WorkspaceBranchSelector<'a> {
    pub repo_name: &'a str,
    pub repo_id: &'a str,
    pub branch_name: &'a str,
    pub checkout_branch: Option<&'a str>,
    pub remote_branch: Option<&'a str>,
    pub local_head: Option<&'a str>,
    pub commit_pin: Option<&'a str>,
}

fn authorized_request(
    builder: reqwest::RequestBuilder,
    token: Option<&str>,
) -> reqwest::RequestBuilder {
    match token.filter(|t| !t.is_empty()) {
        Some(token) => builder.bearer_auth(token),
        None => builder,
    }
}

fn map_api_status(status: reqwest::StatusCode, resource: &str) -> Result<(), G5Error> {
    if status.is_success() {
        return Ok(());
    }
    if status.as_u16() == 409 {
        return Err(G5Error::Conflict {
            reason: resource.to_string(),
        });
    }
    if status.as_u16() == 404 {
        return Err(G5Error::NotFound {
            resource: resource.to_string(),
        });
    }
    if status.as_u16() == 401 || status.as_u16() == 403 {
        return Err(G5Error::Unauthorized);
    }
    Err(G5Error::Internal {
        message: format!("API returned status {}", status),
    })
}

fn api_error_body_message(body: &str) -> String {
    if let Ok(error) = serde_json::from_str::<ErrorResponse>(body) {
        return error.message;
    }

    let trimmed = body.trim();
    if trimmed.is_empty() {
        "<empty response body>".to_string()
    } else {
        trimmed.to_string()
    }
}

fn map_api_status_body(status: reqwest::StatusCode, resource: &str, body: &str) -> G5Error {
    let message = api_error_body_message(body);
    if status.as_u16() == 409 {
        return G5Error::Conflict {
            reason: format!("{resource}: {message}"),
        };
    }
    if status.as_u16() == 404 {
        return G5Error::NotFound {
            resource: format!("{resource}: {message}"),
        };
    }
    if status.as_u16() == 401 || status.as_u16() == 403 {
        return G5Error::Unauthorized;
    }
    if status.as_u16() == 422 {
        return G5Error::ValidationError {
            details: serde_json::json!({
                "resource": resource,
                "message": message,
            }),
        };
    }
    G5Error::Internal {
        message: format!("API returned status {status}: {message}"),
    }
}

fn map_add_repo_status_body(
    status: reqwest::StatusCode,
    req: &AddWorkspaceRepoRemoteRequest<'_>,
    body: &str,
) -> G5Error {
    let message = api_error_body_message(body);
    if status.as_u16() == 422
        && (req.tracking_mode == Some("remote_tracking") || req.remote_branch.is_some())
        && (message.contains("remote_tracking")
            || message.contains("remoteBranch")
            || message.contains("trackingMode"))
        && (message.contains("unknown variant") || message.contains("unknown field"))
    {
        return G5Error::ValidationError {
            details: serde_json::json!({
                "kind": "server_version_deployment_gap",
                "resource": format!("workspace {} add repo", req.workspace_id),
                "message": "The target g5-api does not support remote-tracking repo adds yet. Deploy a g5-api build that accepts trackingMode=remote_tracking and remoteBranch, then rerun g5d add --track-remote.",
                "serverMessage": message,
            }),
        };
    }

    map_api_status_body(
        status,
        &format!("workspace {} add repo", req.workspace_id),
        body,
    )
}

pub fn parse_workspace_url(input: &str) -> Option<WorkspaceUrlParts> {
    let mut url = reqwest::Url::parse(input).ok()?;
    let segments: Vec<String> = url
        .path_segments()?
        .filter(|segment| !segment.is_empty())
        .map(percent_decode_path_segment)
        .collect::<Option<Vec<_>>>()?;

    if segments.len() < 2 {
        return None;
    }

    let org_slug = segments[segments.len() - 2].clone();
    let workspace_name = segments[segments.len() - 1].clone();
    let base_segments = &segments[..segments.len() - 2];
    let base_path = if base_segments.is_empty() {
        String::new()
    } else {
        format!("/{}", base_segments.join("/"))
    };
    url.set_path(&base_path);

    Some(WorkspaceUrlParts {
        api_url: url.to_string().trim_end_matches('/').to_string(),
        org_slug,
        workspace_name,
    })
}

fn percent_decode_path_segment(segment: &str) -> Option<String> {
    let mut decoded = Vec::with_capacity(segment.len());
    let bytes = segment.as_bytes();
    let mut index = 0;

    while index < bytes.len() {
        if bytes[index] == b'%' {
            let high = bytes.get(index + 1).and_then(|byte| hex_value(*byte))?;
            let low = bytes.get(index + 2).and_then(|byte| hex_value(*byte))?;
            decoded.push((high << 4) | low);
            index += 3;
        } else {
            decoded.push(bytes[index]);
            index += 1;
        }
    }

    String::from_utf8(decoded).ok()
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

pub fn parse_repo_id_from_git_url(url: &str) -> Option<Uuid> {
    let parsed = reqwest::Url::parse(url).ok()?;
    let segments: Vec<&str> = parsed
        .path_segments()?
        .filter(|segment| !segment.is_empty())
        .collect();
    if segments.len() < 2 || segments[segments.len() - 2] != "git" {
        return None;
    }
    Uuid::parse_str(segments[segments.len() - 1]).ok()
}

fn normalize_api_url(api_url: &str) -> Result<String, G5Error> {
    let url = reqwest::Url::parse(api_url).map_err(|e| G5Error::Internal {
        message: format!("Failed to parse API URL '{}': {}", api_url, e),
    })?;
    Ok(url.to_string().trim_end_matches('/').to_string())
}

fn rewrite_git_transport_url(original: &str, api_url: &str) -> Result<String, G5Error> {
    let Some(_repo_id) = parse_repo_id_from_git_url(original) else {
        return Ok(original.to_string());
    };

    let source = reqwest::Url::parse(original).map_err(|e| G5Error::Internal {
        message: format!("Failed to parse repo URL '{}': {}", original, e),
    })?;
    let api = reqwest::Url::parse(api_url).map_err(|e| G5Error::Internal {
        message: format!("Failed to parse API URL '{}': {}", api_url, e),
    })?;

    let source_path = source.path().to_string();
    let mut rewritten = source;
    rewritten
        .set_scheme(api.scheme())
        .map_err(|_| G5Error::Internal {
            message: format!(
                "Failed to rewrite repo URL '{}' to API scheme '{}'",
                original,
                api.scheme()
            ),
        })?;
    rewritten
        .set_host(api.host_str())
        .map_err(|e| G5Error::Internal {
            message: format!("Failed to rewrite repo URL host for '{}': {}", original, e),
        })?;
    rewritten
        .set_port(api.port())
        .map_err(|_| G5Error::Internal {
            message: format!("Failed to rewrite repo URL port for '{}'", original),
        })?;

    let api_base_path = api.path().trim_end_matches('/');
    let rewritten_path = if api_base_path.is_empty()
        || api_base_path == "/"
        || source_path.starts_with(api_base_path)
    {
        source_path
    } else {
        format!("{api_base_path}{source_path}")
    };
    rewritten.set_path(&rewritten_path);

    Ok(rewritten.to_string())
}

fn normalize_manifest_urls(
    mut manifest: WorkspaceConfig,
    api_url: &str,
) -> Result<WorkspaceConfig, G5Error> {
    let normalized_api_url = normalize_api_url(api_url)?;
    manifest.remote.api_url = Some(normalized_api_url.clone());
    manifest.remote.enabled = true;

    for repo in &mut manifest.repos {
        repo.url = rewrite_git_transport_url(&repo.url, &normalized_api_url)?;
    }

    if let Some(docs) = manifest.docs.as_mut() {
        docs.repo_url = rewrite_git_transport_url(&docs.repo_url, &normalized_api_url)?;
    }

    Ok(manifest)
}

pub async fn resolve_workspace_slug(
    api_url: &str,
    org_slug: &str,
    workspace_name: &str,
    token: Option<&str>,
) -> Result<WorkspaceResolveResponse, G5Error> {
    let client = reqwest::Client::new();
    let mut url = reqwest::Url::parse(&format!("{}/api/v1/resolve", api_url.trim_end_matches('/')))
        .map_err(|e| G5Error::Internal {
            message: format!("Failed to build resolve URL: {}", e),
        })?;
    url.query_pairs_mut()
        .append_pair("org", org_slug)
        .append_pair("workspace", workspace_name);

    let response = authorized_request(client.get(url), token)
        .send()
        .await
        .map_err(|e| G5Error::Internal {
            message: format!("Failed to resolve workspace URL: {}", e),
        })?;
    map_api_status(
        response.status(),
        &format!("workspace {org_slug}/{workspace_name}"),
    )?;

    let envelope: ResponseEnvelope<WorkspaceResolveResponse> =
        response.json().await.map_err(|e| G5Error::Internal {
            message: format!("Failed to parse workspace resolve response: {}", e),
        })?;
    Ok(envelope.data)
}

pub async fn fetch_workspace_detail(
    api_url: &str,
    workspace_id: &str,
    token: Option<&str>,
) -> Result<WorkspaceDetailResponse, G5Error> {
    let client = reqwest::Client::new();
    let detail_url = format!(
        "{}/api/v1/workspaces/{}",
        api_url.trim_end_matches('/'),
        workspace_id
    );
    let response = authorized_request(client.get(&detail_url), token)
        .send()
        .await
        .map_err(|e| G5Error::Internal {
            message: format!("Failed to fetch workspace detail: {}", e),
        })?;
    map_api_status(response.status(), &format!("workspace {}", workspace_id))?;

    let envelope: ResponseEnvelope<WorkspaceDetailResponse> =
        response.json().await.map_err(|e| G5Error::Internal {
            message: format!("Failed to parse workspace detail response: {}", e),
        })?;
    Ok(envelope.data)
}

pub async fn list_org_repos(
    api_url: &str,
    org_id: &str,
    token: Option<&str>,
) -> Result<Vec<OrgRepoResponse>, G5Error> {
    let client = reqwest::Client::new();
    let list_url = format!(
        "{}/api/v1/orgs/{}/repos",
        api_url.trim_end_matches('/'),
        org_id
    );
    let response = authorized_request(client.get(&list_url), token)
        .send()
        .await
        .map_err(|e| G5Error::Internal {
            message: format!("Failed to list org repos: {}", e),
        })?;
    map_api_status(response.status(), &format!("org {}", org_id))?;

    let envelope: ResponseEnvelope<Vec<OrgRepoResponse>> =
        response.json().await.map_err(|e| G5Error::Internal {
            message: format!("Failed to parse org repo list response: {}", e),
        })?;
    Ok(envelope.data)
}

pub async fn add_workspace_repo_remote(
    req: AddWorkspaceRepoRemoteRequest<'_>,
) -> Result<AddWorkspaceRepoResponse, G5Error> {
    let client = reqwest::Client::new();
    let add_url = format!(
        "{}/api/v1/orgs/{}/workspaces/{}/repos",
        req.api_url.trim_end_matches('/'),
        req.org_id,
        req.workspace_id
    );

    let payload = add_workspace_repo_remote_payload(&req);

    let response = authorized_request(client.post(&add_url).json(&payload), req.token)
        .send()
        .await
        .map_err(|e| G5Error::Internal {
            message: format!("Failed to add repo to workspace: {}", e),
        })?;
    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(map_add_repo_status_body(status, &req, &body));
    }

    let envelope: ResponseEnvelope<AddWorkspaceRepoResponse> =
        response.json().await.map_err(|e| G5Error::Internal {
            message: format!("Failed to parse workspace add repo response: {}", e),
        })?;
    Ok(envelope.data)
}

fn add_workspace_repo_remote_payload(req: &AddWorkspaceRepoRemoteRequest<'_>) -> serde_json::Value {
    let mut payload = serde_json::json!({ "gitUrl": req.git_url });
    if let Some(name) = req.name {
        payload["name"] = serde_json::Value::String(name.to_string());
    }
    if let Some(default_branch) = req.default_branch {
        payload["defaultBranch"] = serde_json::Value::String(default_branch.to_string());
    }
    if let Some(bootstrap_mode) = req.bootstrap_mode {
        payload["bootstrapMode"] = serde_json::Value::String(bootstrap_mode.to_string());
    }
    if let Some(branch_name) = req.branch_name {
        payload["branchName"] = serde_json::Value::String(branch_name.to_string());
    }
    if let Some(head_commit) = req.head_commit {
        payload["headCommit"] = serde_json::Value::String(head_commit.to_string());
    }
    if let Some(repo_role) = req.repo_role {
        payload["repoRole"] = serde_json::Value::String(repo_role.to_string());
    }
    if let Some(tracking_mode) = req.tracking_mode {
        payload["trackingMode"] = serde_json::Value::String(tracking_mode.to_string());
    }
    if let Some(remote_branch) = req.remote_branch {
        payload["remoteBranch"] = serde_json::Value::String(remote_branch.to_string());
    }

    payload
}

pub async fn clone_workspace_remote(
    api_url: &str,
    source_workspace_id: &str,
    name: &str,
    token: Option<&str>,
) -> Result<CloneWorkspaceResponse, G5Error> {
    let client = reqwest::Client::new();
    let clone_url = format!(
        "{}/api/v1/workspaces/{}/clone",
        api_url.trim_end_matches('/'),
        source_workspace_id
    );
    let response = authorized_request(
        client
            .post(&clone_url)
            .json(&serde_json::json!({ "name": name })),
        token,
    )
    .send()
    .await
    .map_err(|e| G5Error::Internal {
        message: format!("Failed to clone workspace remotely: {}", e),
    })?;
    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(map_api_status_body(
            status,
            &format!("workspace {}", source_workspace_id),
            &body,
        ));
    }

    let envelope: ResponseEnvelope<CloneWorkspaceResponse> =
        response.json().await.map_err(|e| G5Error::Internal {
            message: format!("Failed to parse workspace clone response: {}", e),
        })?;
    Ok(envelope.data)
}

pub async fn fetch_workspace_manifest(
    api_url: &str,
    workspace_id: &str,
    token: Option<&str>,
) -> Result<WorkspaceConfig, G5Error> {
    let client = reqwest::Client::new();
    let public_manifest_url = format!(
        "{}/api/v1/workspaces/{}/manifest",
        api_url.trim_end_matches('/'),
        workspace_id
    );

    let response = authorized_request(client.get(&public_manifest_url), token)
        .send()
        .await
        .map_err(|e| G5Error::Internal {
            message: format!("Failed to fetch workspace manifest: {}", e),
        })?;

    let response = if (response.status() == reqwest::StatusCode::UNAUTHORIZED
        || response.status() == reqwest::StatusCode::FORBIDDEN)
        && token.is_some()
    {
        let internal_manifest_url = format!(
            "{}/api/internal/workspaces/{}/manifest",
            api_url.trim_end_matches('/'),
            workspace_id
        );
        authorized_request(client.get(&internal_manifest_url), token)
            .send()
            .await
            .map_err(|e| G5Error::Internal {
                message: format!("Failed to fetch worker workspace manifest: {}", e),
            })?
    } else {
        response
    };

    let status = response.status();
    let body = response.text().await.map_err(|e| G5Error::Internal {
        message: format!("Failed to read workspace manifest response: {e}"),
    })?;
    if !status.is_success() {
        return Err(map_api_status_body(
            status,
            &format!("workspace {workspace_id}"),
            &body,
        ));
    }

    let envelope: WorkspaceManifestEnvelope =
        serde_json::from_str(&body).map_err(|e| G5Error::Internal {
            message: format!("Failed to parse workspace manifest response: {e}"),
        })?;

    normalize_manifest_urls(envelope.data, api_url)
}

pub async fn fetch_workspace_branch_pins(
    api_url: &str,
    workspace_id: &str,
    token: Option<&str>,
) -> Result<Vec<WorkspaceBranchPin>, G5Error> {
    let client = reqwest::Client::new();
    let url = format!(
        "{}/api/v1/workspaces/{workspace_id}/branches",
        api_url.trim_end_matches('/')
    );

    let response = authorized_request(client.get(&url), token)
        .send()
        .await
        .map_err(|e| G5Error::Internal {
            message: format!("Failed to fetch workspace branch rows: {}", e),
        })?;
    let status = response.status();
    let body = response.text().await.map_err(|e| G5Error::Internal {
        message: format!("Failed to read workspace branch rows response: {}", e),
    })?;
    if !status.is_success() {
        return Err(map_api_status_body(status, "workspace branch rows", &body));
    }
    let envelope: ResponseEnvelope<Vec<WorkspaceBranchPin>> =
        serde_json::from_str(&body).map_err(|e| G5Error::Internal {
            message: format!("Failed to parse workspace branch rows: {}", e),
        })?;
    Ok(envelope.data)
}

pub fn apply_workspace_branch_commit_pins(
    manifest: &mut WorkspaceConfig,
    branches: &[WorkspaceBranchPin],
) -> Result<(), G5Error> {
    for repo in &mut manifest.repos {
        if has_commit_pin(repo.commit.as_deref()) {
            continue;
        }
        repo.commit = Some(resolve_branch_pin(
            &repo.name,
            &repo.url,
            &repo.branch,
            branches,
        )?);
    }

    if let Some(docs) = manifest.docs.as_mut() {
        if !has_commit_pin(docs.commit.as_deref()) {
            docs.commit = Some(resolve_branch_pin(
                "docs",
                &docs.repo_url,
                &docs.branch,
                branches,
            )?);
        }
    }

    Ok(())
}

pub fn workspace_manifest_needs_commit_pins(manifest: &WorkspaceConfig) -> bool {
    manifest
        .repos
        .iter()
        .any(|repo| !has_commit_pin(repo.commit.as_deref()))
        || manifest
            .docs
            .as_ref()
            .is_some_and(|docs| !has_commit_pin(docs.commit.as_deref()))
}

fn has_commit_pin(commit: Option<&str>) -> bool {
    commit
        .map(str::trim)
        .is_some_and(|commit| !commit.is_empty())
}

pub fn select_executable_workspace_branch(
    branches: &[WorkspaceBranchRow],
    selector: WorkspaceBranchSelector<'_>,
) -> Result<WorkspaceBranchRow, G5Error> {
    let branch_name = selector.branch_name.trim();
    if branch_name.is_empty() {
        return Err(manifest_validation_error(format!(
            "Configured repo '{}' is missing a canonical branch label",
            selector.repo_name
        )));
    }

    let matches: Vec<&WorkspaceBranchRow> = branches
        .iter()
        .filter(|branch| branch.repo_id == selector.repo_id)
        .filter(|branch| branch.branch_name == branch_name)
        .filter(|branch| workspace_branch_checkout_matches(branch, &selector))
        .filter(|branch| {
            selector
                .local_head
                .map(str::trim)
                .filter(|head| !head.is_empty())
                .is_none_or(|head| normalize_sha(head) == normalize_sha(&branch.head_commit))
        })
        .filter(|branch| {
            selector
                .commit_pin
                .map(str::trim)
                .filter(|commit| !commit.is_empty())
                .is_none_or(|commit| normalize_sha(commit) == normalize_sha(&branch.head_commit))
        })
        .collect();

    let branch = match matches.as_slice() {
        [branch] => *branch,
        [] => {
            return Err(manifest_validation_error(format!(
                "No authoritative G5 branch row matched manifest repo '{}' id {} branch {}",
                selector.repo_name, selector.repo_id, branch_name
            )));
        }
        _ => {
            return Err(manifest_validation_error(format!(
                "Multiple authoritative G5 branch rows matched manifest repo '{}' id {} branch {}",
                selector.repo_name, selector.repo_id, branch_name
            )));
        }
    };

    ensure_workspace_branch_executable(branch, selector.repo_name, branch_name)?;
    Ok(branch.clone())
}

pub fn workspace_branch_checkout_matches(
    branch: &WorkspaceBranchRow,
    selector: &WorkspaceBranchSelector<'_>,
) -> bool {
    let Some(checkout_branch) = selector.checkout_branch else {
        return true;
    };
    checkout_branch == selector.branch_name
        || branch.local_branch_name.as_deref() == Some(checkout_branch)
        || selector
            .remote_branch
            .is_some_and(|remote| remote == checkout_branch)
}

pub fn ensure_workspace_branch_executable(
    branch: &WorkspaceBranchRow,
    repo_name: &str,
    branch_name: &str,
) -> Result<(), G5Error> {
    if branch.materialization_status != "materialized" {
        return Err(manifest_validation_error(format!(
            "Workspace branch for manifest repo '{repo_name}' id {} branch {branch_name} is not materialized (status: {})",
            branch.repo_id, branch.materialization_status
        )));
    }
    if is_placeholder_head(&branch.head_commit) {
        return Err(manifest_validation_error(format!(
            "Workspace branch for manifest repo '{repo_name}' id {} branch {branch_name} has no executable head commit",
            branch.repo_id
        )));
    }
    Ok(())
}

fn resolve_branch_pin(
    name: &str,
    repo_url: &str,
    branch_name: &str,
    branches: &[WorkspaceBranchPin],
) -> Result<String, G5Error> {
    let repo_id = parse_repo_id_from_git_url(repo_url)
        .map(|id| id.to_string())
        .ok_or_else(|| {
            manifest_validation_error(format!(
                "Configured repo '{name}' URL '{repo_url}' does not contain a stable G5 /git/{{repo_id}} identity"
            ))
        })?;
    let branch = select_executable_workspace_branch(
        branches,
        WorkspaceBranchSelector {
            repo_name: name,
            repo_id: &repo_id,
            branch_name,
            checkout_branch: None,
            remote_branch: None,
            local_head: None,
            commit_pin: None,
        },
    )?;

    Ok(branch.head_commit.clone())
}

pub fn is_placeholder_head(head: &str) -> bool {
    let trimmed = head.trim();
    trimmed.is_empty()
        || trimmed == "HEAD"
        || trimmed == "no-changes"
        || (trimmed.len() >= 40 && trimmed.chars().all(|ch| ch == '0'))
}

fn normalize_sha(sha: &str) -> String {
    sha.trim().to_ascii_lowercase()
}

fn manifest_validation_error(message: String) -> G5Error {
    G5Error::ValidationError {
        details: serde_json::json!({ "message": message }),
    }
}

pub fn serialize_workspace_manifest(manifest: &WorkspaceConfig) -> Result<String, G5Error> {
    serde_yaml::to_string(manifest).map_err(|e| G5Error::Internal {
        message: format!("Failed to serialize workspace manifest: {}", e),
    })
}

pub async fn write_workspace_manifest_files(
    workspace_root: &Path,
    yaml_content: &str,
) -> Result<(), G5Error> {
    let g5_dir = workspace_root.join(".g5");
    tokio::fs::create_dir_all(&g5_dir)
        .await
        .map_err(|e| G5Error::Internal {
            message: format!("Failed to create workspace directory: {}", e),
        })?;

    let workspace_yaml_path = g5_dir.join("workspace.yaml");
    tokio::fs::write(&workspace_yaml_path, yaml_content)
        .await
        .map_err(|e| G5Error::Internal {
            message: format!("Failed to write workspace.yaml: {}", e),
        })?;

    let meta_dir = g5_dir.join("workspace-meta");
    tokio::fs::create_dir_all(&meta_dir)
        .await
        .map_err(|e| G5Error::Internal {
            message: format!("Failed to create workspace-meta directory: {}", e),
        })?;

    Ok(())
}

pub async fn sync_workspace_meta(
    workspace_root: &Path,
    yaml_content: &str,
    commit_message: &str,
) -> Result<(), G5Error> {
    let meta_dir = workspace_root.join(".g5").join("workspace-meta");
    tokio::fs::create_dir_all(&meta_dir)
        .await
        .map_err(|e| G5Error::Internal {
            message: format!("Failed to create workspace-meta directory: {}", e),
        })?;

    let meta_ws_yaml = meta_dir.join("workspace.yaml");
    tokio::fs::write(&meta_ws_yaml, yaml_content)
        .await
        .map_err(|e| G5Error::Internal {
            message: format!("Failed to write workspace.yaml to workspace-meta: {}", e),
        })?;

    let repo = match GitRepo::open(&meta_dir) {
        Ok(repo) => repo,
        Err(_) => ops::init(&meta_dir, false)?,
    };

    let author = GitAuthor {
        name: "g5d".to_string(),
        email: "g5d@local".to_string(),
    };
    if let Err(e) = ops::checkpoint(&repo, commit_message, &author) {
        warn!("Failed to commit workspace-meta: {}", e);
    }

    Ok(())
}

pub async fn materialize_workspace_repo(
    workspace_root: &Path,
    repo_entry: &WorkspaceRepoEntry,
    auth: &GitAuth,
    checkout_remote_names: bool,
) -> Result<(), G5Error> {
    let repo_path = workspace_root.join(&repo_entry.path);
    if let Some(parent) = repo_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| G5Error::Internal {
            message: format!(
                "Failed to create repo parent directory '{}': {}",
                parent.display(),
                e
            ),
        })?;
    }

    let repo_exists = repo_path.join(".git").exists() || repo_path.join("objects").exists();
    let clone_branch = if repo_entry.commit.is_some() {
        ""
    } else {
        repo_entry.branch.as_str()
    };

    let repo = materialize_checkout(
        &repo_path,
        &repo_entry.url,
        clone_branch,
        &repo_entry.branch,
        auth,
        repo_exists,
    )
    .await?;

    if repo_entry.commit.is_some() {
        let (path, url, auth) = (repo_path.clone(), repo_entry.url.clone(), auth.clone());
        tokio::task::spawn_blocking(move || {
            crate::cli::native_git::fetch_origin_blocking(&path, &url, &auth)
        })
        .await
        .map_err(|error| G5Error::Internal {
            message: format!("Fetch task failed: {error}"),
        })??;
    }

    pin_repo_materialization(&repo, repo_entry)?;
    apply_remote_name_alias(&repo, repo_entry, checkout_remote_names)?;
    Ok(())
}

pub fn workspace_docs_checkout_path(
    workspace_root: &Path,
    docs: &WorkspaceDocsConfig,
) -> std::path::PathBuf {
    let docs_rel_path = docs.path.trim_matches('/');

    if docs_rel_path.is_empty() {
        return workspace_root.join("repos").join("docs");
    }

    // The server manifest's root_path is the shared physical checkout contract.
    // Do not reinterpret a direct child such as `docs` as private `.g5/docs`:
    // clone, refresh, commit, push, and tools must all see the same repository.
    workspace_root.join(docs_rel_path)
}

pub async fn materialize_workspace_docs(
    workspace_root: &Path,
    docs: &WorkspaceDocsConfig,
    auth: &GitAuth,
) -> Result<(), G5Error> {
    let docs_path = workspace_docs_checkout_path(workspace_root, docs);

    if let Some(parent) = docs_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| G5Error::Internal {
            message: format!(
                "Failed to create docs parent directory '{}': {}",
                parent.display(),
                e
            ),
        })?;
    }

    let repo_exists = docs_path.join(".git").exists() || docs_path.join("objects").exists();
    let repo = materialize_checkout(
        &docs_path,
        &docs.repo_url,
        "",
        &docs.branch,
        auth,
        repo_exists,
    )
    .await?;

    pin_docs_materialization(&repo, docs)?;
    Ok(())
}

/// Refresh an existing checkout or clone a fresh one through the native
/// runner, for every remote scheme. An existing checkout first tracks
/// `track_branch`, so a single-branch clone can switch to another branch.
async fn materialize_checkout(
    repo_path: &Path,
    url: &str,
    branch: &str,
    track_branch: &str,
    auth: &GitAuth,
    repo_exists: bool,
) -> Result<GitRepo, G5Error> {
    if repo_exists {
        let git = crate::cli::native_git::runner()?;
        git.set_remote_url(repo_path, "origin", url).await?;
        let track_branch = track_branch.trim();
        if !track_branch.is_empty() {
            git.track_remote_branch(repo_path, "origin", track_branch)
                .await?;
        }
        git.fetch_checkout(
            repo_path,
            "origin",
            url,
            &crate::cli::native_git::transport_auth(url, auth),
        )
        .await?;
        return GitRepo::open(repo_path);
    }
    if repo_path.exists() {
        std::fs::remove_dir_all(repo_path).map_err(|e| G5Error::Internal {
            message: format!("Failed to reset repo path '{}': {}", repo_path.display(), e),
        })?;
    }
    crate::cli::native_git::clone_checkout(url, repo_path, branch, false, auth).await
}

fn pin_repo_materialization(
    repo: &GitRepo,
    repo_entry: &WorkspaceRepoEntry,
) -> Result<(), G5Error> {
    if let Some(commit) = repo_entry
        .commit
        .as_deref()
        .filter(|c| !c.trim().is_empty())
    {
        if repo_entry.branch.trim().is_empty() {
            repo.checkout_commit(commit)?;
        } else {
            repo.checkout_branch_at(&repo_entry.branch, commit)?;
        }
        return Ok(());
    }

    if !repo_entry.branch.trim().is_empty() {
        repo.checkout_branch(&repo_entry.branch)?;
    }

    Ok(())
}

/// When `g5d clone --checkout-remote-names` is set, check out a remote-tracking
/// repo under its upstream `remote_branch` label instead of the canonical local
/// name. The canonical branch stays in the repo; the alias maps back to it on
/// `origin` so server interactions (and plain `git push`/`pull`) remain
/// canonical. Non-remote-tracking repos (no `remote_branch`, or already aligned)
/// keep the canonical name — and so does the cloud worker, which never sets this
/// flag (preserves the split-branch materialization invariant).
fn apply_remote_name_alias(
    repo: &GitRepo,
    repo_entry: &WorkspaceRepoEntry,
    checkout_remote_names: bool,
) -> Result<(), G5Error> {
    if !checkout_remote_names {
        return Ok(());
    }
    let canonical = repo_entry.branch.trim();
    if canonical.is_empty() {
        return Ok(());
    }
    let Some(remote_branch) = repo_entry
        .remote_branch
        .as_deref()
        .map(str::trim)
        .filter(|rb| !rb.is_empty())
    else {
        return Ok(());
    };
    if remote_branch == canonical {
        return Ok(());
    }

    // Point the alias at the canonical tip, map it back to canonical on origin,
    // and check it out so the working tree shows the upstream branch name.
    // Idempotent across rematerialization/retry: the alias is rewritten in one
    // ref update even when it is already checked out.
    repo.checkout_branch_at(remote_branch, canonical)?;
    repo.configure_branch_alias(remote_branch, canonical, "origin")?;
    Ok(())
}

fn pin_docs_materialization(repo: &GitRepo, docs: &WorkspaceDocsConfig) -> Result<(), G5Error> {
    let branch = docs.branch.trim();
    if let Some(commit) = docs.commit.as_deref().filter(|c| !c.trim().is_empty()) {
        if branch.is_empty() {
            repo.checkout_commit(commit)?;
        } else {
            repo.checkout_branch_at(branch, commit)?;
        }
        return Ok(());
    }

    if branch.is_empty() {
        return Ok(());
    }

    match repo.checkout_branch(branch) {
        Ok(()) => Ok(()),
        Err(_) => repo.checkout_branch_at(branch, "HEAD"),
    }
}

// ─── Safe-additive manifest refresh (retained from the retired `g5d refresh`) ─

fn restore_local_unpublished_commits(
    repo_path: &std::path::Path,
    repo_entry: &crate::cli::config::WorkspaceRepoEntry,
    previous_head: Option<&str>,
) -> Result<(), G5Error> {
    let Some(previous_head) = previous_head else {
        return Ok(());
    };

    let repo = g5_scm::GitRepo::open(repo_path)?;
    let refreshed_head = repo.current_head()?;
    if refreshed_head == previous_head {
        return Ok(());
    }

    let branch = if repo_entry.branch.trim().is_empty() {
        repo.default_branch().unwrap_or_else(|| "main".to_string())
    } else {
        repo_entry.branch.clone()
    };

    if repo.is_ancestor_of(&refreshed_head, previous_head)? {
        repo.apply_branch_commit(&branch, &refreshed_head, previous_head)?;
        repo.checkout_branch(&branch)?;
        return Ok(());
    }

    if repo.is_ancestor_of(previous_head, &refreshed_head)? {
        repo.checkout_branch(&branch)?;
        return Ok(());
    }

    let rebased_head = repo.rebase_onto(
        previous_head,
        &refreshed_head,
        &branch,
        &format!("origin/{}", branch),
    )?;
    repo.apply_branch_commit(&branch, &refreshed_head, &rebased_head)?;
    repo.checkout_branch(&branch)?;
    Ok(())
}

pub async fn refresh_workspace_from_api(
    workspace_path: &Path,
    api_url: &str,
    token: Option<String>,
) -> Result<(u32, u32), G5Error> {
    let ws_config = crate::cli::config::WorkspaceConfig::load(workspace_path)
        .map_err(|e| G5Error::Internal {
            message: format!("Failed to load workspace config: {}", e),
        })?
        .ok_or_else(|| G5Error::Internal {
            message: "No workspace.yaml found".to_string(),
        })?;

    let workspace_id = ws_config.workspace_id.as_deref().unwrap_or("unknown");
    tracing::info!("Refreshing workspace {} from {}", workspace_id, api_url);

    let mut updated_config =
        fetch_workspace_manifest(api_url, workspace_id, token.as_deref()).await?;
    // Preserve the persisted remote-name-checkout opt-in so a refresh does not
    // revert the local branch labels back to canonical.
    updated_config.clone.checkout_remote_names = ws_config.clone.checkout_remote_names;
    let checkout_remote_names = updated_config.clone.checkout_remote_names;
    let manifest_body = serialize_workspace_manifest(&updated_config)?;

    let mut repos_added = 0u32;
    let mut remotes_updated = 0u32;

    let existing_repo_names: std::collections::HashSet<String> =
        ws_config.repos.iter().map(|r| r.name.clone()).collect();
    let git_auth = token
        .as_ref()
        .map(|token| g5_scm::GitAuth::Token(token.clone()))
        .unwrap_or(g5_scm::GitAuth::None);

    for repo_entry in &updated_config.repos {
        if !existing_repo_names.contains(&repo_entry.name) {
            tracing::info!("Adding new repo {}", repo_entry.name);

            match materialize_workspace_repo(
                workspace_path,
                repo_entry,
                &git_auth,
                checkout_remote_names,
            )
            .await
            {
                Ok(_) => {
                    repos_added += 1;
                }
                Err(e) => {
                    tracing::warn!(
                        "Failed to clone new repo {} (non-fatal): {}",
                        repo_entry.name,
                        e
                    );
                }
            }
        } else {
            let repo_path = workspace_path.join(&repo_entry.path);
            let previous_head = g5_scm::GitRepo::open(&repo_path)
                .ok()
                .and_then(|repo| repo.current_head().ok());
            match materialize_workspace_repo(
                workspace_path,
                repo_entry,
                &git_auth,
                checkout_remote_names,
            )
            .await
            {
                Ok(_) => {
                    if let Err(e) = restore_local_unpublished_commits(
                        &repo_path,
                        repo_entry,
                        previous_head.as_deref(),
                    ) {
                        tracing::warn!(
                            "Failed to preserve local commits for {} after refresh: {}",
                            repo_entry.name,
                            e
                        );
                    }
                    remotes_updated += 1;
                }
                Err(e) => {
                    tracing::warn!(
                        "Failed to refresh repo {} (non-fatal): {}",
                        repo_entry.name,
                        e
                    );
                }
            }
        }
    }

    if let Some(ref docs) = updated_config.docs {
        match materialize_workspace_docs(workspace_path, docs, &git_auth).await {
            Ok(_) => tracing::info!("Refreshed workspace docs"),
            Err(e) => tracing::warn!("Failed to refresh workspace docs (non-fatal): {}", e),
        }
    }

    write_workspace_manifest_files(workspace_path, &manifest_body).await?;

    let meta_msg = format!(
        "g5d refresh: updated workspace ({} repos added, {} remotes updated)",
        repos_added, remotes_updated
    );
    sync_workspace_meta(workspace_path, &manifest_body, &meta_msg).await?;

    Ok((repos_added, remotes_updated))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn branch_pin(repo_id: &str, branch_name: &str, head_commit: &str) -> WorkspaceBranchPin {
        WorkspaceBranchPin {
            workspace_id: "workspace-1".to_string(),
            repo_id: repo_id.to_string(),
            repo_name: format!("repo-{repo_id}"),
            branch_name: branch_name.to_string(),
            head_commit: head_commit.to_string(),
            materialization_status: "materialized".to_string(),
            has_draft: false,
            dirty_file_count: 0,
            dirty_files: Vec::new(),
            local_branch_name: None,
            tracking_mode: None,
        }
    }

    #[test]
    fn parse_workspace_url_decodes_percent_encoded_workspace_name() {
        let parts = parse_workspace_url("https://dev.g5labs.ai/timsystem/Test%20Project")
            .expect("workspace URL should parse");

        assert_eq!(parts.api_url, "https://dev.g5labs.ai");
        assert_eq!(parts.org_slug, "timsystem");
        assert_eq!(parts.workspace_name, "Test Project");
    }

    #[test]
    fn docs_checkout_honors_the_authoritative_manifest_path() {
        let workspace_root = Path::new("/workspaces/example");
        let direct_docs: WorkspaceDocsConfig = serde_yaml::from_str(
            r#"
repo_url: https://api.g5.dev/git/00000000-0000-0000-0000-000000000002
branch: main
tracking_mode: tracking
path: docs
"#,
        )
        .unwrap();
        let nested_docs: WorkspaceDocsConfig = serde_yaml::from_str(
            r#"
repo_url: https://api.g5.dev/git/00000000-0000-0000-0000-000000000002
branch: main
tracking_mode: tracking
path: repos/docs
"#,
        )
        .unwrap();

        assert_eq!(
            workspace_docs_checkout_path(workspace_root, &direct_docs),
            workspace_root.join("docs")
        );
        assert_eq!(
            workspace_docs_checkout_path(workspace_root, &nested_docs),
            workspace_root.join("repos/docs")
        );
    }

    #[test]
    fn branch_pins_fill_missing_repo_and_docs_commits() {
        let mut manifest: WorkspaceConfig = serde_yaml::from_str(
            r#"
workspace_id: workspace-1
docs:
  repo_url: https://api.g5.dev/git/00000000-0000-0000-0000-000000000002
  branch: main
  tracking_mode: tracking
  path: repos/docs
repos:
  - name: code
    url: https://api.g5.dev/git/00000000-0000-0000-0000-000000000001
    branch: feat/workspace
    path: repos/code
"#,
        )
        .unwrap();
        let branches = vec![
            branch_pin(
                "00000000-0000-0000-0000-000000000001",
                "feat/workspace",
                "1111111111111111111111111111111111111111",
            ),
            branch_pin(
                "00000000-0000-0000-0000-000000000002",
                "main",
                "2222222222222222222222222222222222222222",
            ),
        ];

        apply_workspace_branch_commit_pins(&mut manifest, &branches).unwrap();

        assert_eq!(
            manifest.repos[0].commit.as_deref(),
            Some("1111111111111111111111111111111111111111")
        );
        assert_eq!(
            manifest
                .docs
                .as_ref()
                .and_then(|docs| docs.commit.as_deref()),
            Some("2222222222222222222222222222222222222222")
        );
    }

    #[test]
    fn branch_pins_preserve_existing_commit_pins() {
        let mut manifest: WorkspaceConfig = serde_yaml::from_str(
            r#"
workspace_id: workspace-1
docs:
  repo_url: https://api.g5.dev/git/00000000-0000-0000-0000-000000000002
  branch: main
  tracking_mode: tracking
  path: repos/docs
  commit: aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa
repos: []
"#,
        )
        .unwrap();
        let branches = vec![branch_pin(
            "00000000-0000-0000-0000-000000000002",
            "main",
            "2222222222222222222222222222222222222222",
        )];

        apply_workspace_branch_commit_pins(&mut manifest, &branches).unwrap();

        assert_eq!(
            manifest
                .docs
                .as_ref()
                .and_then(|docs| docs.commit.as_deref()),
            Some("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
        );
    }

    #[test]
    fn add_workspace_repo_remote_payload_includes_local_fields() {
        let req = AddWorkspaceRepoRemoteRequest {
            api_url: "https://dev.g5labs.ai",
            org_id: "org-1",
            workspace_id: "workspace-1",
            git_url: "g5-local://workspaces/workspace-1/repos/my_service_code_123e4567.git",
            name: Some("my_service_code_123e4567"),
            default_branch: Some("main"),
            bootstrap_mode: Some("local"),
            branch_name: None,
            head_commit: None,
            repo_role: Some("code"),
            tracking_mode: None,
            remote_branch: None,
            token: Some("redacted"),
        };

        let payload = add_workspace_repo_remote_payload(&req);

        assert_eq!(
            payload["gitUrl"],
            "g5-local://workspaces/workspace-1/repos/my_service_code_123e4567.git"
        );
        assert_eq!(payload["name"], "my_service_code_123e4567");
        assert_eq!(payload["defaultBranch"], "main");
        assert_eq!(payload["bootstrapMode"], "local");
        assert_eq!(payload["repoRole"], "code");
        assert!(payload.get("branchName").is_none());
        assert!(payload.get("headCommit").is_none());
        assert!(payload.get("token").is_none());
    }

    #[test]
    fn add_workspace_repo_remote_payload_includes_remote_tracking_fields() {
        let req = AddWorkspaceRepoRemoteRequest {
            api_url: "https://dev.g5labs.ai",
            org_id: "org-1",
            workspace_id: "workspace-1",
            git_url: "file:///tmp/code",
            name: Some("code"),
            default_branch: Some("feature/login"),
            bootstrap_mode: None,
            branch_name: None,
            head_commit: None,
            repo_role: None,
            tracking_mode: Some("remote_tracking"),
            remote_branch: Some("feature/login"),
            token: Some("redacted"),
        };

        let payload = add_workspace_repo_remote_payload(&req);

        assert_eq!(payload["gitUrl"], "file:///tmp/code");
        assert_eq!(payload["defaultBranch"], "feature/login");
        assert_eq!(payload["trackingMode"], "remote_tracking");
        assert_eq!(payload["remoteBranch"], "feature/login");
        assert!(payload.get("token").is_none());
    }

    #[test]
    fn remote_tracking_unsupported_error_mentions_server_version_gap() {
        let req = AddWorkspaceRepoRemoteRequest {
            api_url: "https://dev.g5labs.ai",
            org_id: "org-1",
            workspace_id: "workspace-1",
            git_url: "file:///tmp/code",
            name: Some("code"),
            default_branch: Some("main"),
            bootstrap_mode: None,
            branch_name: None,
            head_commit: None,
            repo_role: None,
            tracking_mode: Some("remote_tracking"),
            remote_branch: Some("feature/login"),
            token: None,
        };

        let error = map_add_repo_status_body(
            reqwest::StatusCode::UNPROCESSABLE_ENTITY,
            &req,
            "Failed to deserialize the JSON body into the target type: trackingMode: unknown variant `remote_tracking`",
        );
        let rendered = error.to_string();

        assert!(
            matches!(
                &error,
                G5Error::ValidationError { details }
                    if details["message"]
                        .as_str()
                        .is_some_and(|message| message.contains("does not support remote-tracking repo adds yet"))
                        && details["kind"] == "server_version_deployment_gap"
            ),
            "old server remote_tracking rejection should be actionable: {rendered}"
        );
    }

    fn repo_entry(branch: &str, remote_branch: Option<&str>) -> WorkspaceRepoEntry {
        WorkspaceRepoEntry {
            name: "r".to_string(),
            url: "https://api.example/git/repo".to_string(),
            repo_id: None,
            branch: branch.to_string(),
            path: "repos/r".to_string(),
            commit: None,
            remote_branch: remote_branch.map(str::to_string),
        }
    }

    fn temp_repo_on_branch(branch: &str) -> (GitRepo, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let raw = git2::Repository::init(dir.path()).unwrap();
        std::fs::write(dir.path().join("README.md"), "hi").unwrap();
        let mut index = raw.index().unwrap();
        index.add_path(std::path::Path::new("README.md")).unwrap();
        index.write().unwrap();
        let tree = raw.find_tree(index.write_tree().unwrap()).unwrap();
        let sig = git2::Signature::now("t", "t@e").unwrap();
        let oid = raw
            .commit(Some("HEAD"), &sig, &sig, "init", &tree, &[])
            .unwrap();
        let commit = raw.find_commit(oid).unwrap();
        if raw.find_branch(branch, git2::BranchType::Local).is_err() {
            raw.branch(branch, &commit, true).unwrap();
        }
        drop(tree);
        drop(commit);
        let repo = GitRepo::open(dir.path()).unwrap();
        repo.checkout_branch(branch).unwrap();
        if branch != "main" {
            if let Ok(mut main) = git2::Repository::open(dir.path())
                .unwrap()
                .find_branch("main", git2::BranchType::Local)
            {
                let _ = main.delete();
            }
        }
        (repo, dir)
    }

    #[test]
    fn refresh_restores_unpublished_commits_and_rebases_divergent_heads() {
        let (repo, dir) = temp_repo_on_branch("work");
        let base = repo.current_head().unwrap();
        let author = g5_scm::GitAuthor {
            name: "test".into(),
            email: "test@example.com".into(),
        };
        std::fs::write(dir.path().join("local.txt"), "local\n").unwrap();
        repo.add_all().unwrap();
        let local = repo.commit("unpublished", &author).unwrap();
        repo.apply_branch_commit("work", &local, &base).unwrap();
        repo.checkout_branch("work").unwrap();
        restore_local_unpublished_commits(dir.path(), &repo_entry("work", None), Some(&local))
            .unwrap();
        assert_eq!(repo.current_head().unwrap(), local);
        assert_eq!(
            std::fs::read_to_string(dir.path().join("local.txt")).unwrap(),
            "local\n"
        );

        repo.apply_branch_commit("work", &local, &base).unwrap();
        repo.checkout_branch("work").unwrap();
        std::fs::write(dir.path().join("remote.txt"), "remote\n").unwrap();
        repo.add_all().unwrap();
        let remote = repo.commit("remote advance", &author).unwrap();
        restore_local_unpublished_commits(dir.path(), &repo_entry("work", None), Some(&local))
            .unwrap();
        let restored = repo.current_head().unwrap();
        assert!(repo.is_ancestor_of(&remote, &restored).unwrap());
        assert_eq!(
            std::fs::read_to_string(dir.path().join("local.txt")).unwrap(),
            "local\n"
        );
        assert_eq!(
            std::fs::read_to_string(dir.path().join("remote.txt")).unwrap(),
            "remote\n"
        );
        restore_local_unpublished_commits(dir.path(), &repo_entry("work", None), Some(&base))
            .unwrap();
        assert_eq!(repo.current_head().unwrap(), restored);
    }

    fn head_branch(dir: &tempfile::TempDir) -> String {
        let raw = git2::Repository::open(dir.path()).unwrap();
        let head = raw.head().unwrap();
        head.shorthand().unwrap().to_string()
    }

    #[test]
    fn test_alias_noop_when_flag_disabled() {
        // The worker/default path passes checkout_remote_names=false and must
        // stay on the canonical branch: the host materializes on the canonical
        // branch name recorded for the workspace member.
        let (repo, dir) = temp_repo_on_branch("feat/x");
        apply_remote_name_alias(&repo, &repo_entry("feat/x", Some("main")), false).unwrap();
        assert_eq!(head_branch(&dir), "feat/x");
        assert!(git2::Repository::open(dir.path())
            .unwrap()
            .find_branch("main", git2::BranchType::Local)
            .is_err());
    }

    #[test]
    fn test_alias_noop_when_no_remote_branch() {
        // Opt-in but non-remote-tracking repo: no alias, stays canonical.
        let (repo, dir) = temp_repo_on_branch("feat/x");
        apply_remote_name_alias(&repo, &repo_entry("feat/x", None), true).unwrap();
        assert_eq!(head_branch(&dir), "feat/x");
    }

    #[test]
    fn test_alias_created_when_enabled() {
        // Opt-in + remote-tracking: the working tree moves to the upstream-named
        // alias, mapped back to the canonical branch on origin; canonical stays.
        let (repo, dir) = temp_repo_on_branch("feat/x");
        apply_remote_name_alias(&repo, &repo_entry("feat/x", Some("main")), true).unwrap();
        assert_eq!(head_branch(&dir), "main");
        let raw = git2::Repository::open(dir.path()).unwrap();
        let cfg = raw.config().unwrap();
        assert_eq!(
            cfg.get_string("branch.main.merge").unwrap(),
            "refs/heads/feat/x"
        );
        assert!(raw.find_branch("feat/x", git2::BranchType::Local).is_ok());
    }
}
