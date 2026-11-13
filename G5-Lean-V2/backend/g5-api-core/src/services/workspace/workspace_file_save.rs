//! The owner's explicit file Save: a whole-file write to the attached VM under
//! the file-version and VM-instance preconditions, then the selected-file
//! checkpoint of exactly that file (`g5.documents.save`,
//! `g5.git.checkpoint.selected`, `g5.git.checkpoint.retry`).
//!
//! The editor Save route and the question service compose this one operation
//! (`g5.architecture.question_owner`); neither writes then commits on its own.
//! Callers authorize the mutation first and pass repository-relative paths
//! they have already sanitized.

use crate::services::git::repo_store::OrgScope;
use std::collections::BTreeMap;

use serde_json::Value;
use uuid::Uuid;

use crate::errors::AppError;
use crate::services::workspace_runtime::{
    runtime_to_g5, RuntimeRequest, RuntimeRpcError, WorkspaceRuntime, WorkspaceRuntimeHandle,
};
use g5_contracts::error::G5Error;

/// Runtime budget of one file call.
const FILE_CALL_BUDGET_MS: u64 = 30_000;

/// One file a checkpoint commits: its repository, path and the blob hash the
/// caller observed after writing it. The runtime commits only that blob.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectedFile {
    pub repo_id: Uuid,
    pub path: String,
    pub blob_hash: String,
}

/// A live file as the attached VM holds it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LiveFile {
    pub path: String,
    pub content: String,
    pub encoding: String,
    pub size: u64,
    pub hash: String,
    pub vm_id: Uuid,
}

/// Result of an explicit Save whose write landed.
#[derive(Debug)]
pub enum SaveOutcome {
    /// The bytes are written, committed and pushed.
    Published { saved: Value },
    /// The bytes are written to the live file but the commit or push failed.
    /// A retry with the identical bytes publishes them without rewriting.
    Applied { saved: Value, error: AppError },
}

impl SaveOutcome {
    /// The write reply (`path`, `size`, `encoding`, `hash`, `vmId`).
    #[must_use]
    pub fn saved(&self) -> &Value {
        match self {
            Self::Published { saved } | Self::Applied { saved, .. } => saved,
        }
    }
}

/// An owner runtime request on `handle` with the file-call budget.
#[must_use]
pub fn file_runtime_request(
    handle: &WorkspaceRuntimeHandle,
    actor_user_id: Uuid,
    operation: &str,
) -> RuntimeRequest {
    let deadline_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
        + FILE_CALL_BUDGET_MS;
    RuntimeRequest::owner(
        handle,
        actor_user_id,
        deadline_ms,
        format!("workspace-file:{operation}:{}", Uuid::new_v4()),
    )
}

/// The editor conflict contract for runtime file errors.
#[must_use]
pub fn map_file_runtime_error(error: RuntimeRpcError) -> AppError {
    match error.message.as_str() {
        "FILE_VERSION_CONFLICT" => AppError::from_error(G5Error::FileVersionConflict {
            details: error.data,
        }),
        "FILE_EXISTS" => AppError::from_error(G5Error::FileExists {
            details: error.data,
        }),
        "FILE_MISSING" => AppError::from_error(G5Error::NotFound {
            resource: "workspace file".to_string(),
        }),
        _ => AppError::from_error(runtime_to_g5(error)),
    }
}

fn internal(message: &str) -> AppError {
    AppError::from_error(G5Error::Internal {
        message: message.to_string(),
    })
}

/// Read `path` from the running incarnation `handle` for its owner.
pub async fn read_live_file(
    runtime: &WorkspaceRuntime,
    handle: &WorkspaceRuntimeHandle,
    actor_user_id: Uuid,
    repo_id: Uuid,
    path: &str,
) -> Result<LiveFile, AppError> {
    let request = file_runtime_request(handle, actor_user_id, "read");
    let value = runtime
        .file_read(handle, &request, repo_id, path)
        .await
        .map_err(map_file_runtime_error)?;
    let content = value
        .get("content")
        .and_then(Value::as_str)
        .ok_or_else(|| internal("workspace file reply omitted content"))?
        .to_string();
    let encoding = value
        .get("encoding")
        .and_then(Value::as_str)
        .unwrap_or("utf-8")
        .to_string();
    let size = value
        .get("size")
        .and_then(Value::as_u64)
        .unwrap_or(content.len() as u64);
    let hash = value
        .get("hash")
        .and_then(Value::as_str)
        .ok_or_else(|| internal("workspace file reply omitted hash"))?
        .to_string();
    Ok(LiveFile {
        path: path.to_string(),
        content,
        encoding,
        size,
        hash,
        vm_id: handle.vm_id,
    })
}

/// Write `path` into the running incarnation `handle` for its owner, with the
/// branch-association guard and the editor's conflict contract (a version
/// conflict carries the current bytes and hash).
#[allow(clippy::too_many_arguments)]
pub async fn write_live_file(
    runtime: &WorkspaceRuntime,
    scope: &(impl OrgScope + ?Sized),
    handle: &WorkspaceRuntimeHandle,
    actor_user_id: Uuid,
    repo_id: Uuid,
    path: &str,
    content: &str,
    encoding: &str,
    expected_hash: Option<&str>,
) -> Result<Value, AppError> {
    // The association report is the authoritative guard against writing a
    // checkout that a terminal or native process switched away from the
    // workspace's fixed member branch; it fails with
    // WORKSPACE_BRANCH_MISMATCH before fs.write.
    runtime
        .workspace_changes(scope, handle.workspace_id, actor_user_id)
        .await?;
    let request = file_runtime_request(handle, actor_user_id, "write");
    let mut result = match runtime
        .file_write(
            handle,
            &request,
            repo_id,
            path,
            content,
            encoding,
            expected_hash,
        )
        .await
    {
        Ok(result) => result,
        Err(mut error) if error.message == "FILE_VERSION_CONFLICT" => {
            // Read through the same fenced incarnation so the caller never
            // compares against a different VM or a Git fallback.
            if let Ok(current) = runtime.file_read(handle, &request, repo_id, path).await {
                if let Some(details) = error.data.as_object_mut() {
                    for field in ["content", "encoding", "hash", "size"] {
                        if let Some(value) = current.get(field) {
                            details.insert(field.to_string(), value.clone());
                        }
                    }
                }
            }
            return Err(map_file_runtime_error(error));
        }
        Err(error) => return Err(map_file_runtime_error(error)),
    };
    if let Some(fields) = result.as_object_mut() {
        fields.insert("vmId".into(), serde_json::json!(handle.vm_id));
    }
    Ok(result)
}

/// The owner's attached incarnation, refused when it is not the one the
/// caller observed (`g5.data.identity.incarnation`).
pub async fn expected_owner_handle(
    runtime: &WorkspaceRuntime,
    workspace_id: Uuid,
    owner_id: Uuid,
    expected_vm_id: Option<Uuid>,
    reason: &str,
) -> Result<WorkspaceRuntimeHandle, AppError> {
    let handle = runtime.owner_handle(workspace_id, owner_id).await?;
    if expected_vm_id.is_some_and(|observed| observed != handle.vm_id) {
        return Err(AppError::from_error(G5Error::Conflict {
            reason: reason.to_string(),
        }));
    }
    Ok(handle)
}

/// Whether one `workspace.changes` action succeeded for every member, as a
/// conflict naming the failing members and their errors otherwise.
pub fn ensure_changes_action_succeeded(outcome: &Value, what: &str) -> Result<(), AppError> {
    if outcome["status"] == "succeeded" {
        return Ok(());
    }
    let details: Vec<String> = outcome["members"]
        .as_array()
        .map(|members| {
            members
                .iter()
                .filter(|member| !member["errors"].is_null())
                .map(|member| {
                    let errors = member["errors"]
                        .as_array()
                        .map(|errors| {
                            errors
                                .iter()
                                .filter_map(Value::as_str)
                                .collect::<Vec<_>>()
                                .join(", ")
                        })
                        .unwrap_or_default();
                    match member["detail"].as_str() {
                        Some(detail) if !detail.is_empty() => format!("{errors}: {detail}"),
                        _ => errors,
                    }
                })
                .collect()
        })
        .unwrap_or_default();
    Err(AppError::from_error(G5Error::Conflict {
        reason: format!(
            "could not {what} the workspace's changes ({})",
            if details.is_empty() {
                "unknown reason".to_string()
            } else {
                details.join("; ")
            }
        ),
    }))
}

/// Commit only these files and publish their member branches. The runtime
/// stages untracked paths too, commits only the observed blob of each file
/// and skips a commit when the final bytes match HEAD. A push still runs for
/// a clean path to recover an earlier failed push.
pub async fn checkpoint_selected_files(
    runtime: &WorkspaceRuntime,
    workspace_id: Uuid,
    owner_id: Uuid,
    files: &[SelectedFile],
    message: &str,
) -> Result<(), AppError> {
    let mut by_repo = BTreeMap::<Uuid, (Vec<String>, BTreeMap<String, String>)>::new();
    for file in files {
        let (paths, blobs) = by_repo.entry(file.repo_id).or_default();
        if !paths.contains(&file.path) {
            paths.push(file.path.clone());
        }
        blobs.insert(file.path.clone(), file.blob_hash.clone());
    }
    for (repo_id, (paths, blobs)) in by_repo {
        let committed = runtime
            .workspace_changes_commit_paths(
                workspace_id,
                owner_id,
                repo_id,
                &paths,
                &blobs,
                message,
            )
            .await?;
        ensure_changes_action_succeeded(&committed, "commit")?;
        let head = committed["members"]
            .as_array()
            .and_then(|members| {
                members
                    .iter()
                    .find(|member| member["repo_id"] == serde_json::json!(repo_id))
            })
            .and_then(|member| member["head"].as_str())
            .ok_or_else(|| internal("scoped commit did not return a head"))?;
        let pushed = runtime
            .workspace_changes_push_head(workspace_id, owner_id, repo_id, head)
            .await?;
        ensure_changes_action_succeeded(&pushed, "push")?;
    }
    Ok(())
}

/// One explicit owner Save of a whole file (`g5.documents.save`): write the
/// bytes to the attached VM under `expected_hash` and `expected_vm_id`, then
/// checkpoint exactly the written blob. When the write meets a version
/// conflict and the live file already holds these exact bytes — an earlier
/// Save of the same bytes whose publication failed — the write is skipped and
/// the existing bytes are published (`g5.git.checkpoint.retry`).
///
/// A write that never landed is an `Err`; a landed write whose checkpoint
/// failed is [`SaveOutcome::Applied`], so the caller can tell "nothing
/// changed" from "applied, not published".
#[allow(clippy::too_many_arguments)]
pub async fn save_live_file(
    runtime: &WorkspaceRuntime,
    scope: &(impl OrgScope + ?Sized),
    workspace_id: Uuid,
    owner_id: Uuid,
    repo_id: Uuid,
    path: &str,
    content: &str,
    encoding: &str,
    expected_hash: Option<&str>,
    expected_vm_id: Uuid,
    message: &str,
) -> Result<SaveOutcome, AppError> {
    let stale = "the workspace changed after this file was opened; reload before saving";
    let handle =
        expected_owner_handle(runtime, workspace_id, owner_id, Some(expected_vm_id), stale).await?;
    let saved = match write_live_file(
        runtime,
        scope,
        &handle,
        owner_id,
        repo_id,
        path,
        content,
        encoding,
        expected_hash,
    )
    .await
    {
        Ok(saved) => saved,
        Err(error)
            if matches!(
                &error.inner,
                G5Error::FileVersionConflict { .. } | G5Error::FileExists { .. }
            ) =>
        {
            let current = read_live_file(runtime, &handle, owner_id, repo_id, path).await?;
            if current.content != content || current.encoding != encoding {
                return Err(error);
            }
            serde_json::json!({
                "path": current.path,
                "size": current.size,
                "encoding": current.encoding,
                "hash": current.hash,
                "vmId": handle.vm_id,
            })
        }
        Err(error) => return Err(error),
    };
    let hash = saved
        .get("hash")
        .and_then(Value::as_str)
        .ok_or_else(|| internal("saved file did not include its blob hash"))?
        .to_string();
    Ok(
        match publish_saved_file(
            runtime,
            workspace_id,
            owner_id,
            expected_vm_id,
            repo_id,
            path,
            &hash,
            message,
        )
        .await
        {
            Ok(()) => SaveOutcome::Published { saved },
            Err(error) => SaveOutcome::Applied { saved, error },
        },
    )
}

/// Checkpoint one already-written blob on the incarnation that wrote it.
#[allow(clippy::too_many_arguments)]
pub async fn publish_saved_file(
    runtime: &WorkspaceRuntime,
    workspace_id: Uuid,
    owner_id: Uuid,
    expected_vm_id: Uuid,
    repo_id: Uuid,
    path: &str,
    blob_hash: &str,
    message: &str,
) -> Result<(), AppError> {
    expected_owner_handle(
        runtime,
        workspace_id,
        owner_id,
        Some(expected_vm_id),
        "the workspace changed before its selected file could be published",
    )
    .await?;
    checkpoint_selected_files(
        runtime,
        workspace_id,
        owner_id,
        &[SelectedFile {
            repo_id,
            path: path.to_string(),
            blob_hash: blob_hash.to_string(),
        }],
        message,
    )
    .await
}
