//! Local authentication commands for secretless managed workers and workspaces.
//!
//! `g5d auth login` implements the CLI side of the one-time device
//! authorization flow: the CLI receives a short-lived device code, the user
//! approves the matching user code in the browser, and the CLI stores the JWT
//! and refresh token only after approval.

use std::path::{Path, PathBuf};
use std::time::Duration;

use g5_contracts::error::G5Error;
use serde::{Deserialize, Serialize};
use tokio::time::{sleep, Instant};

use crate::cli::config::WorkspaceConfig;

use super::api_client::{AuthState, ResponseEnvelope};
use super::{
    resolve_api_url, resolve_workspace_path, AuthArgs, AuthCommand, AuthLoginArgs, AuthLogoutArgs,
    AuthStatusArgs, GlobalFlags,
};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct CliDeviceStartRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    workspace_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    org_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    device_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    browser_origin: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CliDeviceStartResponse {
    device_code: String,
    user_code: String,
    verification_uri: String,
    verification_uri_complete: String,
    expires_in: i64,
    interval: i64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct CliDeviceTokenRequest {
    device_code: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CliDeviceTokenResponse {
    status: String,
    jwt: Option<String>,
    refresh_token: Option<String>,
    expires_in: Option<i64>,
    user_id: Option<String>,
    org_id: Option<String>,
    workspace_id: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct LogoutRequest {
    refresh_token: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AuthStatusOutput {
    authenticated: bool,
    has_refresh_token: bool,
    api_url: Option<String>,
    scope: String,
    org_id: Option<String>,
    workspace_id: Option<String>,
    auth_mode: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AuthLoginOutput {
    status: String,
    api_url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    workspace_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    org_id: Option<String>,
    user_id: Option<String>,
    expires_in: Option<i64>,
}

pub async fn execute(args: &AuthArgs, global: &GlobalFlags) -> Result<(), G5Error> {
    match &args.command {
        AuthCommand::Login(login_args) => login(login_args, global).await,
        AuthCommand::Status(status_args) => status(status_args, global).await,
        AuthCommand::Logout(logout_args) => logout(logout_args, global).await,
    }
}

pub fn g5_home_dir() -> PathBuf {
    std::env::var_os("G5_HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME")
                .filter(|value| !value.is_empty())
                .map(|home| PathBuf::from(home).join(".g5"))
        })
        .unwrap_or_else(|| PathBuf::from(".g5"))
}

pub fn client_install_marker_path() -> PathBuf {
    std::env::var_os("HOME")
        .filter(|value| !value.is_empty())
        .map(|home| PathBuf::from(home).join(".g5").join("client_install"))
        .unwrap_or_else(|| PathBuf::from(".g5").join("client_install"))
}

pub fn client_install_marker_exists() -> bool {
    client_install_marker_path().is_file()
}

pub fn global_auth_state_path() -> PathBuf {
    std::env::var_os("G5D_AUTH_STATE")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| g5_home_dir().join("auth_state.json"))
}

async fn login(args: &AuthLoginArgs, global: &GlobalFlags) -> Result<(), G5Error> {
    if args.timeout_secs == 0 {
        return Err(G5Error::ValidationError {
            details: serde_json::json!({"timeoutSecs": "must be greater than zero"}),
        });
    }

    let api_url = resolve_api_url(global).trim_end_matches('/').to_string();
    let explicit_workspace_id = args
        .workspace_id
        .clone()
        .filter(|value| !value.trim().is_empty());
    let explicit_org_id = args.org_id.clone().filter(|value| !value.trim().is_empty());

    let (workspace_path, auth_state_path, mut auth_state, workspace_id, save_global) =
        if explicit_workspace_id.is_some() || explicit_org_id.is_some() {
            let path = global_auth_state_path();
            let mut state = AuthState::load_from_path(&path);
            if state.api_url() != Some(api_url.as_str()) {
                state = AuthState::default();
            }
            (None, path, state, explicit_workspace_id.clone(), true)
        } else {
            let workspace_path = require_workspace_path(global)?;
            let state = AuthState::load(&workspace_path);
            let workspace_id = resolve_workspace_id(&workspace_path, &state)?;
            (
                Some(workspace_path.clone()),
                workspace_path.join(super::api_client::AUTH_STATE_FILE),
                state,
                Some(workspace_id),
                false,
            )
        };

    let http = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(30))
        .timeout(Duration::from_secs(120))
        .build()
        .map_err(|e| G5Error::Internal {
            message: format!("Failed to build HTTP client: {e}"),
        })?;

    let start = post_json::<_, CliDeviceStartResponse>(
        &http,
        &format!("{api_url}/api/v1/auth/cli/start"),
        &CliDeviceStartRequest {
            workspace_id: workspace_id.clone(),
            org_id: explicit_org_id
                .clone()
                .or_else(|| auth_state.org_id().map(str::to_string)),
            device_name: args.device_name.clone().or_else(default_device_name),
            browser_origin: auth_state.web_url().map(str::to_string),
        },
        None,
        "start CLI device authorization",
    )
    .await?;

    if global.json {
        println!(
            "{}",
            serde_json::json!({
                "status": "pending",
                "verificationUri": start.verification_uri,
                "verificationUriComplete": start.verification_uri_complete,
                "userCode": start.user_code,
                "expiresIn": start.expires_in,
            })
        );
    } else if !global.quiet {
        println!("Open this URL in your browser:");
        println!("  {}", start.verification_uri_complete);
        println!();
        println!("Code: {}", start.user_code);
        println!("Approve this code in the G5 browser page. Sign in first if prompted.");
        println!("Waiting for approval...");
    }

    let poll_interval_secs = args
        .poll_interval_secs
        .unwrap_or_else(|| start.interval.max(1) as u64)
        .max(1);
    let deadline = Instant::now() + Duration::from_secs(args.timeout_secs);
    let pending_notice_interval = Duration::from_secs(15);
    let mut next_pending_notice_at = Instant::now() + pending_notice_interval;

    loop {
        if Instant::now() >= deadline {
            return Err(G5Error::Internal {
                message: format!(
                    "Timed out waiting for browser approval for code {}. Open {} and approve the request, then rerun `g5d auth login` if the code expired.",
                    start.user_code, start.verification_uri_complete
                ),
            });
        }

        let token_response = post_json::<_, CliDeviceTokenResponse>(
            &http,
            &format!("{api_url}/api/v1/auth/cli/token"),
            &CliDeviceTokenRequest {
                device_code: start.device_code.clone(),
            },
            None,
            "poll CLI device authorization",
        )
        .await?;

        match token_response.status.as_str() {
            "pending" => {
                let now = Instant::now();
                if !global.json && !global.quiet && now >= next_pending_notice_at {
                    let remaining = deadline.saturating_duration_since(now).as_secs();
                    println!(
                        "Still waiting for browser approval for code {} (about {}s remaining).",
                        start.user_code, remaining
                    );
                    println!("  {}", start.verification_uri_complete);
                    next_pending_notice_at = now + pending_notice_interval;
                }
                sleep(Duration::from_secs(poll_interval_secs)).await;
            }
            "approved" => {
                let jwt = token_response.jwt.ok_or_else(|| G5Error::Internal {
                    message: "Approved CLI authorization response omitted jwt".to_string(),
                })?;
                let refresh_token =
                    token_response
                        .refresh_token
                        .ok_or_else(|| G5Error::Internal {
                            message: "Approved CLI authorization response omitted refreshToken"
                                .to_string(),
                        })?;

                let authoritative_org_id =
                    token_response.org_id.clone().or(explicit_org_id.clone());
                let authoritative_workspace_id =
                    token_response.workspace_id.clone().or(workspace_id.clone());
                auth_state.set_authenticated(
                    jwt,
                    refresh_token,
                    api_url.clone(),
                    authoritative_org_id,
                    authoritative_workspace_id.clone(),
                );
                if save_global {
                    auth_state.save_to_path(&auth_state_path)?;
                } else if let Some(workspace_path) = workspace_path.as_ref() {
                    auth_state.save(workspace_path)?;
                } else {
                    auth_state.save_to_path(&auth_state_path)?;
                }

                if global.json {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&AuthLoginOutput {
                            status: "authenticated".to_string(),
                            api_url,
                            workspace_id: authoritative_workspace_id,
                            org_id: auth_state.org_id().map(str::to_string),
                            user_id: token_response.user_id,
                            expires_in: token_response.expires_in,
                        })
                        .unwrap_or_else(|_| "{}".to_string())
                    );
                } else if !global.quiet {
                    println!("Authenticated.");
                }
                return Ok(());
            }
            "denied" => {
                if !global.json && !global.quiet {
                    println!("Authorization denied in browser.");
                }
                return Err(G5Error::Unauthorized);
            }
            "expired" => {
                return Err(G5Error::Internal {
                    message: "CLI authorization expired. Run `g5d auth login` again.".to_string(),
                });
            }
            "consumed" => {
                return Err(G5Error::Internal {
                    message: "CLI authorization was already consumed. Run `g5d auth login` again."
                        .to_string(),
                });
            }
            other => {
                return Err(G5Error::Internal {
                    message: format!("Unexpected CLI authorization status: {other}"),
                });
            }
        }
    }
}

async fn status(args: &AuthStatusArgs, global: &GlobalFlags) -> Result<(), G5Error> {
    let auth_state = if args.global {
        AuthState::load_from_path(&global_auth_state_path())
    } else {
        let workspace_path = require_workspace_path(global)?;
        AuthState::load(&workspace_path)
    };
    let scope = if auth_state.workspace_id().is_some() {
        "workspace"
    } else if auth_state.org_id().is_some() {
        "org"
    } else {
        "unknown"
    };
    let output = AuthStatusOutput {
        authenticated: auth_state.jwt().is_some(),
        has_refresh_token: auth_state.refresh_token().is_some(),
        api_url: auth_state
            .api_url()
            .map(str::to_string)
            .or_else(|| Some(resolve_api_url(global))),
        scope: scope.to_string(),
        org_id: auth_state.org_id().map(str::to_string),
        workspace_id: auth_state.workspace_id().map(str::to_string),
        auth_mode: auth_state.auth_mode().map(str::to_string),
    };

    if global.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&output).unwrap_or_else(|_| "{}".to_string())
        );
    } else if !global.quiet {
        println!(
            "Auth status: {}",
            if output.authenticated {
                "authenticated"
            } else {
                "not authenticated"
            }
        );
        println!("API URL: {}", output.api_url.unwrap_or_default());
        println!("Scope: {}", output.scope);
        println!(
            "Workspace: {}",
            output.workspace_id.unwrap_or_else(|| "unknown".to_string())
        );
        println!(
            "Org: {}",
            output.org_id.unwrap_or_else(|| "unknown".to_string())
        );
        println!(
            "Mode: {}",
            output.auth_mode.unwrap_or_else(|| "unknown".to_string())
        );
    }

    Ok(())
}

async fn logout(args: &AuthLogoutArgs, global: &GlobalFlags) -> Result<(), G5Error> {
    let workspace_path = if args.global {
        None
    } else {
        Some(require_workspace_path(global)?)
    };
    let auth_state_path = args.global.then(global_auth_state_path);
    let mut auth_state = if let Some(path) = auth_state_path.as_ref() {
        AuthState::load_from_path(path)
    } else {
        AuthState::load(workspace_path.as_ref().expect("workspace path present"))
    };
    let api_url = auth_state
        .api_url()
        .map(str::to_string)
        .unwrap_or_else(|| resolve_api_url(global).trim_end_matches('/').to_string());

    if !args.local_only {
        if let (Some(jwt), Some(refresh_token)) = (
            auth_state.jwt().map(str::to_string),
            auth_state.refresh_token().map(str::to_string),
        ) {
            let http = reqwest::Client::builder()
                .connect_timeout(Duration::from_secs(30))
                .timeout(Duration::from_secs(120))
                .build()
                .map_err(|e| G5Error::Internal {
                    message: format!("Failed to build HTTP client: {e}"),
                })?;
            if let Err(err) = post_json::<_, serde_json::Value>(
                &http,
                &format!("{api_url}/api/v1/auth/logout"),
                &LogoutRequest { refresh_token },
                Some(&jwt),
                "logout",
            )
            .await
            {
                tracing::warn!(error = %err, "Remote logout failed; clearing local auth state");
            }
        }
    }

    auth_state.clear_tokens_preserve_metadata();
    if let Some(path) = auth_state_path.as_ref() {
        auth_state.save_to_path(path)?;
    } else if let Some(workspace_path) = workspace_path.as_ref() {
        auth_state.save(workspace_path)?;
    }

    if global.json {
        println!("{}", serde_json::json!({"status": "loggedOut"}));
    } else if !global.quiet {
        println!("Logged out locally.");
    }

    Ok(())
}

fn require_workspace_path(global: &GlobalFlags) -> Result<PathBuf, G5Error> {
    let cwd = std::env::current_dir().map_err(|e| G5Error::Internal {
        message: format!("Failed to determine current directory: {e}"),
    })?;
    resolve_workspace_path(global.workspace.as_ref(), &cwd).ok_or_else(|| G5Error::Internal {
        message:
            "No G5 workspace found. Run from a managed G5 workspace checkout or pass --workspace."
                .to_string(),
    })
}

fn resolve_workspace_id(workspace_path: &Path, auth_state: &AuthState) -> Result<String, G5Error> {
    if let Some(workspace_id) = auth_state.workspace_id() {
        return Ok(workspace_id.to_string());
    }

    let config = WorkspaceConfig::load(workspace_path).map_err(|e| G5Error::Internal {
        message: format!("Failed to load workspace.yaml: {e}"),
    })?;

    config
        .and_then(|config| config.workspace_id)
        .ok_or_else(|| G5Error::Internal {
            message: "No workspaceId found in .g5/auth_state.json or .g5/workspace.yaml"
                .to_string(),
        })
}

fn default_device_name() -> Option<String> {
    std::env::var("HOSTNAME")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .or_else(|| {
            std::env::var("COMPUTERNAME")
                .ok()
                .filter(|value| !value.trim().is_empty())
        })
}

async fn post_json<B, T>(
    http: &reqwest::Client,
    url: &str,
    body: &B,
    bearer_token: Option<&str>,
    action: &str,
) -> Result<T, G5Error>
where
    B: Serialize + ?Sized,
    T: for<'de> Deserialize<'de>,
{
    let mut request = http.post(url).json(body);
    if let Some(token) = bearer_token {
        request = request.bearer_auth(token);
    }
    let response = request.send().await.map_err(|e| G5Error::Internal {
        message: format!("Failed to {action}: {e}"),
    })?;
    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(if status == reqwest::StatusCode::UNAUTHORIZED {
            G5Error::Unauthorized
        } else if status == reqwest::StatusCode::FORBIDDEN {
            G5Error::Forbidden {
                missing_scopes: vec![body],
            }
        } else {
            G5Error::Internal {
                message: format!("Failed to {action}: API returned {status} - {body}"),
            }
        });
    }

    let envelope: ResponseEnvelope<T> = response.json().await.map_err(|e| G5Error::Internal {
        message: format!("Failed to parse {action} response: {e}"),
    })?;
    Ok(envelope.data)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cli_device_start_request_serializes_org_only_scope() {
        let value = serde_json::to_value(CliDeviceStartRequest {
            workspace_id: None,
            org_id: Some("org-1".to_string()),
            device_name: Some("laptop".to_string()),
            browser_origin: None,
        })
        .expect("serialize request");

        assert_eq!(value["orgId"], serde_json::json!("org-1"));
        assert_eq!(value.get("workspaceId"), None);
        assert_eq!(value.get("daemonId"), None);
    }

    #[test]
    fn cli_device_start_request_serializes_workspace_scope_with_optional_org_guard() {
        let value = serde_json::to_value(CliDeviceStartRequest {
            workspace_id: Some("workspace-1".to_string()),
            org_id: Some("org-1".to_string()),
            device_name: None,
            browser_origin: None,
        })
        .expect("serialize request");

        assert_eq!(value["workspaceId"], serde_json::json!("workspace-1"));
        assert_eq!(value["orgId"], serde_json::json!("org-1"));
        assert_eq!(value.get("deviceName"), None);
    }

    #[test]
    fn auth_login_output_supports_org_scope_without_workspace() {
        let value = serde_json::to_value(AuthLoginOutput {
            status: "authenticated".to_string(),
            api_url: "http://localhost:3000".to_string(),
            workspace_id: None,
            org_id: Some("org-1".to_string()),
            user_id: Some("user-1".to_string()),
            expires_in: Some(3600),
        })
        .expect("serialize output");

        assert_eq!(value["orgId"], serde_json::json!("org-1"));
        assert_eq!(value.get("workspaceId"), None);
    }
}
