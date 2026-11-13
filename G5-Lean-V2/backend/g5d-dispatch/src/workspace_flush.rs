//! Receipt-first capture for lifecycle-owned workspace shutdown.

use std::collections::BTreeSet;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use g5_internal_api::workspace_flush::{
    FlushCompleteRequest, FlushCompleteResponse, FlushFailureRequest, FlushLimit, FlushOffender,
    FlushReason, FlushRepoReceipt, FlushStartRequest, FlushStartResponse, SequencerState,
};
use g5_internal_api::InternalApiClient;
use g5d_agent::runner::session_files::{harvest_complete, SessionFileLayout};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use g5_scm::native::{
    MemberFlushCapture, MemberFlushOutcome, MemberFlushPush, MemberGit, MemberObjectOwnership,
    NativeCancellation, NativeGitError,
};

use crate::config::{DispatchConfig, RepositoryLayout, RUNTIME_EXCLUDES};
use crate::git::GitRuntime;
use crate::git_transport::GitTransport;
use crate::protocol::RpcError;

pub const DEFAULT_MAX_FILE_BYTES: u64 = 100 * 1024 * 1024;
pub const DEFAULT_MAX_TREE_BYTES: u64 = 2 * 1024 * 1024 * 1024;
pub const DEFAULT_MAX_PACK_BYTES: u64 = 2 * 1024 * 1024 * 1024;
const PENDING_SESSION_SAVE_BUDGET: std::time::Duration = std::time::Duration::from_secs(5);

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CaptureState {
    request_key: String,
    start: FlushStartResponse,
    repos: Option<Vec<FlushRepoReceipt>>,
    complete: Option<FlushCompleteResponse>,
}

pub struct WorkspaceFlush {
    config: Arc<DispatchConfig>,
    git: Arc<GitRuntime>,
    transport: Arc<GitTransport>,
    client: reqwest::Client,
    state_path: PathBuf,
    gate: tokio::sync::Mutex<()>,
    limits: FlushLimit,
    secrets: crate::modules::redact::Secrets,
}

impl WorkspaceFlush {
    pub fn new(
        config: Arc<DispatchConfig>,
        git: Arc<GitRuntime>,
        transport: Arc<GitTransport>,
    ) -> Self {
        Self {
            secrets: crate::server::configured_secrets(&config),
            state_path: config.paths.state_dir.join("workspace-flush.json"),
            config,
            git,
            transport,
            client: reqwest::Client::new(),
            gate: tokio::sync::Mutex::new(()),
            limits: FlushLimit {
                per_file_bytes: DEFAULT_MAX_FILE_BYTES,
                tree_bytes: DEFAULT_MAX_TREE_BYTES,
                pack_bytes: DEFAULT_MAX_PACK_BYTES,
            },
        }
    }

    pub async fn capture(
        self: &Arc<Self>,
        reason: FlushReason,
        request_key: &str,
        lifecycle: tokio::sync::OwnedMutexGuard<()>,
    ) -> Result<FlushCompleteResponse, RpcError> {
        let owner = self.clone();
        let request_key = request_key.to_owned();
        let cancellation = NativeCancellation::default();
        let (keep_alive, mut dropped) = tokio::sync::oneshot::channel::<()>();
        let task = tokio::spawn(async move {
            let _lifecycle = lifecycle;
            let work = owner.capture_inner(reason, &request_key, &cancellation);
            tokio::pin!(work);
            let result = tokio::select! {
                result = &mut work => result,
                _ = &mut dropped => { cancellation.cancel(); work.await },
            };
            cancellation.wait_idle().await;
            result
        });
        let result = task
            .await
            .map_err(|_| RpcError::new(-32603, "FLUSH_EXECUTION_UNCONFIRMED"))?;
        drop(keep_alive);
        result
    }

    async fn capture_inner(
        &self,
        reason: FlushReason,
        request_key: &str,
        cancellation: &NativeCancellation,
    ) -> Result<FlushCompleteResponse, RpcError> {
        tracing::warn!(workspace_id = %self.config.workspace_id, ?reason, "workspace flush capture requested");
        let _gate = self.gate.lock().await;
        if request_key.is_empty()
            || request_key.len() > 256
            || request_key.chars().any(char::is_control)
        {
            return Err(RpcError::new(-32602, "INVALID_PARAMS"));
        }
        if reason == FlushReason::Shutdown
            && tokio::time::timeout(
                PENDING_SESSION_SAVE_BUDGET,
                self.complete_pending_session_files(),
            )
            .await
            .is_err()
        {
            tracing::warn!("pending session bundle save exceeded the workspace flush budget");
        }
        let mut state = match self.read_state().await? {
            Some(state) if state.request_key == request_key => state,
            _ => CaptureState {
                request_key: request_key.to_owned(),
                start: self.start(reason, request_key).await?,
                repos: None,
                complete: None,
            },
        };
        if let Some(complete) = &state.complete {
            return Ok(complete.clone());
        }
        tracing::warn!(flush_id = %state.start.flush_id, reason = ?reason, "workspace flush capture started");
        self.write_state(&state).await?;

        let repos = match state.repos.clone() {
            Some(repos) => repos,
            None => match self.capture_all(&state.start, cancellation).await {
                Ok(repos) => {
                    state.repos = Some(repos.clone());
                    self.write_state(&state).await?;
                    repos
                }
                Err(CaptureError::TooLarge(failure)) => {
                    if self.fail(state.start.flush_id, &failure).await.is_ok() {
                        // A size failure is a completed negative result for this
                        // capture. Release the local operation slot so a later
                        // explicit lifecycle retry can capture repaired files
                        // under its new request key.
                        let _ = tokio::fs::remove_file(&self.state_path).await;
                    }
                    return Err(RpcError {
                        code: 413,
                        name: "FLUSH_TOO_LARGE",
                        data: serde_json::to_value(failure).unwrap_or_default(),
                    });
                }
                Err(CaptureError::Rpc(error)) => {
                    if is_terminal_capture_error(&error) {
                        let _ = tokio::fs::remove_file(&self.state_path).await;
                    }
                    return Err(error);
                }
            },
        };
        if let Err(error) = self
            .push_all(state.start.flush_id, &repos, cancellation)
            .await
        {
            if is_terminal_capture_error(&error) {
                let _ = tokio::fs::remove_file(&self.state_path).await;
            }
            return Err(error);
        }
        tracing::warn!(flush_id = %state.start.flush_id, "workspace flush refs pushed");
        let complete = match self.complete(state.start.flush_id, repos).await {
            Ok(complete) => complete,
            Err(error) => {
                if is_terminal_capture_error(&error) {
                    let _ = tokio::fs::remove_file(&self.state_path).await;
                }
                return Err(error);
            }
        };
        state.complete = Some(complete.clone());
        self.write_state(&state).await?;
        tracing::warn!(flush_id = %state.start.flush_id, "workspace flush capture completed");
        Ok(complete)
    }

    async fn complete_pending_session_files(&self) {
        let client = InternalApiClient::new(self.config.api_url.as_str())
            .with_token_file(self.config.workspace_token_file());
        let pending = match client
            .pending_session_file_bundles(self.config.workspace_id, None)
            .await
        {
            Ok(pending) => pending,
            Err(error) => {
                tracing::warn!(%error, "pending session bundle lookup failed during workspace flush");
                return;
            }
        };
        for bundle in pending {
            let layout = SessionFileLayout::for_provider(
                bundle.provider,
                &self.config.paths.home,
                &self.config.paths.workspace_root,
                std::path::Path::new(&bundle.cwd_relative_path),
            );
            match harvest_complete(
                &client,
                &bundle.session_id.to_string(),
                &layout,
                &bundle.provider_session_id,
                bundle.generation,
            )
            .await
            {
                Ok(report) if report.found > 0 && report.skipped.is_empty() => {}
                Ok(report) => tracing::warn!(session_id = %bundle.session_id, ?report,
                    "pending session bundle cannot be completed during workspace flush"),
                Err(g5_contracts::error::G5Error::SessionFileIntegrity { message }) => {
                    tracing::warn!(session_id = %bundle.session_id, %message,
                        "pending session bundle lacks its resume file during workspace flush");
                }
                Err(error) => {
                    tracing::warn!(session_id = %bundle.session_id, %error,
                        "pending session bundle upload failed during workspace flush");
                }
            }
        }
    }

    async fn start(
        &self,
        reason: FlushReason,
        request_key: &str,
    ) -> Result<FlushStartResponse, RpcError> {
        tracing::warn!(workspace_id = %self.config.workspace_id, "workspace flush receipt start requested");
        let result = self
            .post(
                &format!(
                    "api/internal/workspace-incarnations/{}/flushes/start",
                    self.config.workspace_id
                ),
                &FlushStartRequest {
                    reason,
                    idempotency_key: request_key.to_owned(),
                },
            )
            .await;
        tracing::warn!(workspace_id = %self.config.workspace_id, succeeded = result.is_ok(), "workspace flush receipt start finished");
        result
    }

    async fn complete(
        &self,
        flush_id: Uuid,
        repos: Vec<FlushRepoReceipt>,
    ) -> Result<FlushCompleteResponse, RpcError> {
        tracing::warn!(%flush_id, "workspace flush receipt completion requested");
        let result = self
            .post(
                &format!(
                    "api/internal/workspace-incarnations/{}/flushes/{flush_id}/complete",
                    self.config.workspace_id
                ),
                &FlushCompleteRequest { repos },
            )
            .await;
        tracing::warn!(%flush_id, succeeded = result.is_ok(), "workspace flush receipt completion finished");
        result
    }

    async fn fail(
        &self,
        flush_id: Uuid,
        failure: &FlushFailureRequest,
    ) -> Result<serde_json::Value, RpcError> {
        self.post(
            &format!(
                "api/internal/workspace-incarnations/{}/flushes/{flush_id}/fail",
                self.config.workspace_id
            ),
            failure,
        )
        .await
    }

    async fn post<T: Serialize + ?Sized, R: serde::de::DeserializeOwned>(
        &self,
        path: &str,
        body: &T,
    ) -> Result<R, RpcError> {
        let url = self
            .config
            .api_url
            .join(path)
            .map_err(|_| RpcError::new(-32603, "FLUSH_API_UNAVAILABLE"))?;
        let response = self
            .client
            .post(url)
            .bearer_auth(
                g5_internal_api::client::read_workspace_token(&self.config.workspace_token_file())
                    .map_err(|_| RpcError::new(503, "FLUSH_CREDENTIAL_UNAVAILABLE"))?,
            )
            .json(body)
            .timeout(std::time::Duration::from_secs(120))
            .send()
            .await
            .map_err(|_| RpcError::new(503, "FLUSH_API_UNAVAILABLE"))?;
        let status = response.status();
        let bytes = response
            .bytes()
            .await
            .map_err(|_| RpcError::new(503, "FLUSH_API_UNAVAILABLE"))?;
        if !status.is_success() {
            let response_name = serde_json::from_slice::<serde_json::Value>(&bytes)
                .ok()
                .and_then(|value| {
                    value["error"]["code"]
                        .as_str()
                        .or_else(|| value["code"].as_str())
                        .map(str::to_owned)
                })
                .unwrap_or_else(|| "FLUSH_API_REJECTED".to_owned());
            let name = match response_name.as_str() {
                "FLUSH_TOO_LARGE" => "FLUSH_TOO_LARGE",
                "FLUSH_INCOMPLETE_VECTOR" => "FLUSH_INCOMPLETE_VECTOR",
                "FLUSH_MEMBER_VECTOR_MISMATCH" => "FLUSH_MEMBER_VECTOR_MISMATCH",
                "FLUSH_COMPLETION_CONFLICT" => "FLUSH_COMPLETION_CONFLICT",
                "FLUSH_INVALID_PROOF" => "FLUSH_INVALID_PROOF",
                "FLUSH_CLEAN_MISMATCH" => "FLUSH_CLEAN_MISMATCH",
                "FLUSH_REPO_NOT_FOUND" => "FLUSH_REPO_NOT_FOUND",
                "FLUSH_BRANCH_MISMATCH" => "FLUSH_BRANCH_MISMATCH",
                "FLUSH_ASSOCIATION_MISMATCH" => "FLUSH_ASSOCIATION_MISMATCH",
                "FLUSH_REF_MISMATCH" => "FLUSH_REF_MISMATCH",
                "FLUSH_HEAD_UNREACHABLE" => "FLUSH_HEAD_UNREACHABLE",
                "FLUSH_NOT_COMMIT" => "FLUSH_NOT_COMMIT",
                "STALE_INCARNATION" => "STALE_INCARNATION",
                "FORBIDDEN" => "FORBIDDEN",
                "UNAUTHORIZED" => "UNAUTHORIZED",
                _ => "FLUSH_API_REJECTED",
            };
            return Err(RpcError {
                code: status.as_u16() as i64,
                name,
                data: serde_json::from_slice(&bytes).unwrap_or_default(),
            });
        }
        serde_json::from_slice(&bytes).map_err(|_| RpcError::new(-32603, "FLUSH_API_INVALID_REPLY"))
    }

    async fn capture_all(
        &self,
        start: &FlushStartResponse,
        cancellation: &NativeCancellation,
    ) -> Result<Vec<FlushRepoReceipt>, CaptureError> {
        let required: BTreeSet<_> = start.required_repo_ids.iter().copied().collect();
        let actual: BTreeSet<_> = self
            .config
            .layout
            .repos
            .iter()
            .map(|repo| repo.repo_id)
            .collect();
        if required != actual || required.len() != start.required_repo_ids.len() {
            return Err(CaptureError::Rpc(RpcError::new(
                409,
                "FLUSH_MEMBER_VECTOR_MISMATCH",
            )));
        }
        let mut receipts = Vec::with_capacity(self.config.layout.repos.len());
        for repo in &self.config.layout.repos {
            receipts.push(
                self.capture_repo(start.flush_id, repo, cancellation)
                    .await?,
            );
        }
        Ok(receipts)
    }

    fn member_git(&self, cancellation: &NativeCancellation) -> Result<MemberGit, RpcError> {
        // Flush scratch indices are dispatcher-private. SCM restores object
        // directory ownership before the lifecycle owner releases custody.
        self.git
            .member_git(None, 16 * 1024 * 1024, cancellation.clone(), None, None)
            .map_err(|error| self.native_error(error))
    }

    async fn capture_repo(
        &self,
        flush_id: Uuid,
        repo: &RepositoryLayout,
        cancellation: &NativeCancellation,
    ) -> Result<FlushRepoReceipt, CaptureError> {
        let root = self.config.paths.workspace_root.join(&repo.root_path);
        let git = self.member_git(cancellation)?;
        self.preflight_tree(repo.repo_id, &root, &git).await?;
        let lease = self
            .transport
            .lease(&repo.repo_id.to_string(), repo.transport_url());
        let snapshot = git
            .capture_flush(&MemberFlushCapture {
                repository: root,
                branch: repo.branch.clone(),
                transport_url: lease.url().into(),
                private_state: self.config.paths.state_dir.clone(),
                excluded_roots: RUNTIME_EXCLUDES
                    .iter()
                    .map(|value| (*value).into())
                    .collect(),
                // Agent memory is ordinary content of the personal workspace's
                // own repository (`g5.personal.memory`).
                required_capture_roots: if repo.follow_branch {
                    crate::modules::builtin::PERSONAL_MEMORY_DIRECTORIES
                        .map(String::from)
                        .to_vec()
                } else {
                    Vec::new()
                },
                ownership: MemberObjectOwnership {
                    workspace: self.config.paths.workspace_root.clone(),
                    uid: self.config.module_uid,
                    gid: self.config.module_gid,
                },
                pack_limit: self.limits.pack_bytes,
            })
            .await
            .map_err(|error| self.native_error(error))?;
        drop(lease);
        let snapshot = match snapshot {
            MemberFlushOutcome::Captured(snapshot) => snapshot,
            MemberFlushOutcome::IgnoredPaths(paths) => {
                return Err(CaptureError::Rpc(RpcError {
                    code: 409,
                    name: "FLUSH_MEMORY_IGNORED",
                    data: serde_json::json!({
                        "repo_id": repo.repo_id,
                        "paths": paths,
                        "message": "Repository ignore rules omit native memory; repair them and retry.",
                    }),
                }));
            }
        };
        if snapshot.pack_bytes > self.limits.pack_bytes {
            return Err(CaptureError::TooLarge(self.too_large(vec![
                FlushOffender {
                    repo_id: repo.repo_id,
                    path: None,
                    bytes: snapshot.pack_bytes,
                    category: "pack".into(),
                },
            ])));
        }
        let sequencer = if snapshot.sequencers.iter().any(|v| v == "rebase") {
            SequencerState::Rebase
        } else if snapshot.sequencers.iter().any(|v| v == "merge") {
            SequencerState::Merge
        } else if snapshot.sequencers.iter().any(|v| v == "cherry-pick") {
            SequencerState::CherryPick
        } else if snapshot.sequencers.iter().any(|v| v == "revert") {
            SequencerState::Revert
        } else {
            SequencerState::None
        };
        tracing::warn!(%flush_id, repo_id = %repo.repo_id, "workspace flush repo capture completed");
        Ok(FlushRepoReceipt {
            repo_id: repo.repo_id,
            branch_oid: snapshot.branch_oid,
            local_branch_oid: snapshot.local_branch_oid,
            local_head_oid: snapshot.local_head_oid,
            head_ref: snapshot.head_ref,
            sequencer,
            snapshot_oid: snapshot.snapshot_oid,
            dirty: !snapshot.dirty_paths.is_empty(),
            dirty_paths: snapshot.dirty_paths,
        })
    }

    async fn preflight_tree(
        &self,
        repo_id: Uuid,
        root: &Path,
        git: &MemberGit,
    ) -> Result<(), CaptureError> {
        // Cached paths include tracked files that were deliberately deleted.
        // Record those before scanning so an admitted delete is part of the
        // snapshot, while a file that vanishes during preflight still fails.
        let inventory = git
            .file_inventory(root)
            .await
            .map_err(|error| self.native_error(error))?;
        let deleted: std::collections::HashSet<_> = inventory.deleted.into_iter().collect();
        let mut total = 0u64;
        let mut offenders = Vec::new();
        for path in inventory.paths {
            if RUNTIME_EXCLUDES
                .iter()
                .any(|excluded| path == *excluded || path.starts_with(&format!("{excluded}/")))
            {
                continue;
            }
            let metadata = match tokio::fs::symlink_metadata(root.join(&path)).await {
                Ok(metadata) => metadata,
                Err(error)
                    if error.kind() == std::io::ErrorKind::NotFound && deleted.contains(&path) =>
                {
                    continue;
                }
                Err(_) => return Err(CaptureError::Rpc(RpcError::new(409, "FLUSH_FILE_CHANGED"))),
            };
            let bytes = if metadata.file_type().is_symlink() {
                tokio::fs::read_link(root.join(&path))
                    .await
                    .map_err(|_| CaptureError::Rpc(RpcError::new(409, "FLUSH_FILE_CHANGED")))?
                    .as_os_str()
                    .len() as u64
            } else if metadata.is_file() {
                metadata.len()
            } else {
                0
            };
            total = total.saturating_add(bytes);
            if bytes > self.limits.per_file_bytes {
                offenders.push(FlushOffender {
                    repo_id,
                    path: Some(path),
                    bytes,
                    category: "file".into(),
                });
            }
        }
        if total > self.limits.tree_bytes {
            offenders.push(FlushOffender {
                repo_id,
                path: None,
                bytes: total,
                category: "tree".into(),
            });
        }
        if offenders.is_empty() {
            Ok(())
        } else {
            Err(CaptureError::TooLarge(self.too_large(offenders)))
        }
    }

    fn too_large(&self, mut offenders: Vec<FlushOffender>) -> FlushFailureRequest {
        offenders.truncate(16);
        let fingerprint = format!(
            "{:x}",
            Sha256::digest(serde_json::to_vec(&(&self.limits, &offenders)).unwrap_or_default())
        );
        FlushFailureRequest {
            code: "FLUSH_TOO_LARGE".into(),
            fingerprint,
            limits: self.limits.clone(),
            offenders,
            detail: "workspace capture exceeds configured durability limits".into(),
        }
    }

    async fn push_all(
        &self,
        flush_id: Uuid,
        repos: &[FlushRepoReceipt],
        cancellation: &NativeCancellation,
    ) -> Result<(), RpcError> {
        for receipt in repos {
            tracing::warn!(%flush_id, repo_id = %receipt.repo_id, "workspace flush repo push started");
            let repo = self
                .config
                .layout
                .repos
                .iter()
                .find(|repo| repo.repo_id == receipt.repo_id)
                .ok_or_else(|| RpcError::new(409, "FLUSH_MEMBER_VECTOR_MISMATCH"))?;
            let root = self.config.paths.workspace_root.join(&repo.root_path);
            let lease = self
                .transport
                .lease(&repo.repo_id.to_string(), repo.transport_url());
            let request = MemberFlushPush {
                repository: root,
                transport_url: lease.url().into(),
                workspace_id: self.config.workspace_id,
                flush_id,
                snapshot_oid: receipt.snapshot_oid.clone(),
                branch_oid: receipt.local_branch_oid.clone(),
            };
            let git = self.member_git(cancellation)?;
            retry_flush_ref_push(|| async {
                git.push_flush(&request)
                    .await
                    .map(|()| String::new())
                    .map_err(|error| self.native_error(error))
            })
            .await?;
            tracing::warn!(%flush_id, repo_id = %receipt.repo_id, "workspace flush repo push completed");
        }
        Ok(())
    }

    async fn read_state(&self) -> Result<Option<CaptureState>, RpcError> {
        match tokio::fs::read(&self.state_path).await {
            Ok(bytes) => serde_json::from_slice(&bytes)
                .map(Some)
                .map_err(|_| RpcError::new(-32603, "FLUSH_STATE_INVALID")),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(_) => Err(RpcError::new(-32603, "FLUSH_STATE_UNAVAILABLE")),
        }
    }

    fn native_error(&self, mut error: NativeGitError) -> RpcError {
        error.detail = self.secrets.redact_diagnostic(&error.detail);
        error.target = error.target.map(|target| self.secrets.redact_str(&target));
        tracing::warn!(%error, "workspace flush Git failed");
        let name = if error.detail.contains("WORKSPACE_BRANCH_MISSING") {
            "WORKSPACE_BRANCH_MISSING"
        } else if error.detail.contains("ownership handoff failed") {
            "FLUSH_GIT_OWNERSHIP_FAILED"
        } else {
            "FLUSH_GIT_FAILED"
        };
        RpcError {
            code: -32603,
            name,
            data: serde_json::json!({"detail":error.to_string()}),
        }
    }

    async fn write_state(&self, state: &CaptureState) -> Result<(), RpcError> {
        let temporary = self
            .state_path
            .with_extension(format!("tmp-{}", Uuid::new_v4()));
        tokio::fs::write(
            &temporary,
            serde_json::to_vec(state).map_err(|_| RpcError::new(-32603, "FLUSH_STATE_INVALID"))?,
        )
        .await
        .map_err(|_| RpcError::new(-32603, "FLUSH_STATE_UNAVAILABLE"))?;
        tokio::fs::rename(&temporary, &self.state_path)
            .await
            .map_err(|_| RpcError::new(-32603, "FLUSH_STATE_UNAVAILABLE"))
    }
}

/// Replaying fixed OIDs to immutable flush refs is safe even if the first
/// receive-pack updated one leaf. A shared Git repository can briefly reject
/// another workspace's ref update while its own receive-pack is active.
async fn retry_flush_ref_push<F, Fut>(mut push: F) -> Result<(), RpcError>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<String, RpcError>>,
{
    for attempt in 0..3 {
        match push().await {
            Ok(_) => return Ok(()),
            Err(_) if attempt < 2 => {
                tokio::time::sleep(std::time::Duration::from_millis(100 * (attempt + 1))).await;
            }
            Err(error) => {
                return Err(RpcError {
                    code: 409,
                    name: "FLUSH_REF_CONFLICT",
                    data: error.data,
                })
            }
        }
    }
    unreachable!("bounded Git push attempts always return")
}

fn is_terminal_capture_error(error: &RpcError) -> bool {
    matches!(
        error.name,
        "FLUSH_TOO_LARGE"
            | "FLUSH_MEMORY_IGNORED"
            | "FLUSH_MEMBER_VECTOR_MISMATCH"
            | "FLUSH_INCOMPLETE_VECTOR"
            | "FLUSH_COMPLETION_CONFLICT"
            | "STALE_INCARNATION"
            | "FORBIDDEN"
            | "UNAUTHORIZED"
    )
}

enum CaptureError {
    TooLarge(FlushFailureRequest),
    Rpc(RpcError),
}

impl From<RpcError> for CaptureError {
    fn from(value: RpcError) -> Self {
        Self::Rpc(value)
    }
}

/// Git's porcelain ref rejection is on stdout; stderr often says only that
/// some refs failed. Keep the rejection status, but never log the porcelain
/// destination line (which contains the short-lived transport lease URL).
#[cfg(test)]
fn describe_git_failure(output: &std::process::Output) -> String {
    let mut detail = format!(
        "exit {:?}: {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr).trim()
    );
    let rejected = String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| {
            line.strip_prefix("!\t")
                .and_then(|line| line.rsplit_once('\t'))
                .map(|(_, status)| status.trim().chars().take(256).collect::<String>())
        })
        .take(4)
        .collect::<Vec<_>>();
    if !rejected.is_empty() {
        detail.push_str("; rejected: ");
        detail.push_str(&rejected.join("; "));
    }
    detail
}

#[cfg(all(test, unix))]
mod tests {
    use std::os::unix::fs::PermissionsExt;
    use std::os::unix::process::ExitStatusExt;
    use std::process::Command;

    use super::*;

    fn git(root: &Path, args: &[&str]) -> String {
        let output = Command::new("git")
            .arg("-C")
            .arg(root)
            .args(args)
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_AUTHOR_NAME", "Fixture")
            .env("GIT_AUTHOR_EMAIL", "fixture@example.test")
            .env("GIT_COMMITTER_NAME", "Fixture")
            .env("GIT_COMMITTER_EMAIL", "fixture@example.test")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout).unwrap().trim().to_owned()
    }

    #[test]
    fn failed_push_status_excludes_transport_lease_url() {
        let output = std::process::Output {
            status: std::process::ExitStatus::from_raw(256),
            stdout: b"To http://127.0.0.1:7101/private-lease/git/repo\n!\tHEAD:refs/g5/flush/snapshot\t[remote rejected] (unpacker error)\nDone\n".to_vec(),
            stderr: b"error: failed to push some refs\n".to_vec(),
        };
        let detail = describe_git_failure(&output);
        assert!(detail.contains("[remote rejected] (unpacker error)"));
        assert!(!detail.contains("private-lease"));
    }

    #[tokio::test]
    async fn retry_flush_ref_push_replays_fixed_oids_after_receive_pack_rejects_once() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("source");
        let bare = temp.path().join("origin.git");
        std::fs::create_dir(&source).unwrap();
        std::fs::create_dir(&bare).unwrap();
        git(&source, &["init", "-q", "-b", "main"]);
        std::fs::write(source.join("proof.txt"), b"exact flush bytes\n").unwrap();
        git(&source, &["add", "proof.txt"]);
        git(&source, &["commit", "-q", "-m", "source cut"]);
        let oid = git(&source, &["rev-parse", "HEAD"]);
        git(&bare, &["init", "-q", "--bare"]);
        let hook = bare.join("hooks/pre-receive");
        std::fs::write(
            &hook,
            b"#!/bin/sh\ncat >/dev/null\nmarker=\"$(dirname \"$0\")/../reject-once\"\nif [ ! -e \"$marker\" ]; then : > \"$marker\"; exit 1; fi\nexit 0\n",
        )
        .unwrap();
        std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o755)).unwrap();
        let bare_path = bare.to_str().unwrap();
        let snapshot = format!("{oid}:refs/g5/flush/example/snapshot");
        let branch = format!("{oid}:refs/g5/flush/example/branch");
        let mut attempts = 0;
        retry_flush_ref_push(|| {
            attempts += 1;
            let output = Command::new("git")
                .arg("-C")
                .arg(&source)
                .args(["push", "--porcelain", bare_path, &snapshot, &branch])
                .output()
                .unwrap();
            if attempts == 1 {
                let detail = describe_git_failure(&output);
                assert!(detail.contains("[remote rejected]"), "{detail}");
            }
            async move {
                if output.status.success() {
                    Ok(String::from_utf8(output.stdout).unwrap())
                } else {
                    Err(RpcError::new(-32603, "FLUSH_GIT_FAILED"))
                }
            }
        })
        .await
        .unwrap();
        assert_eq!(attempts, 2);
        assert!(bare.join("reject-once").exists());
        assert_eq!(
            git(&bare, &["rev-parse", "refs/g5/flush/example/snapshot"]),
            oid
        );
        assert_eq!(
            git(&bare, &["rev-parse", "refs/g5/flush/example/branch"]),
            oid
        );
    }
}
