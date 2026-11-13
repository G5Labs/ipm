//! Git transport contract between G5 storage and a workspace runtime.
//!
//! These refs transport observations and immutable task inputs. They never
//! select the workspace's working branch or its reader-visible content.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub const SOCKET_ENV: &str = "G5D_WORKSPACE_GIT_SOCKET";
/// The socket's file name inside the dispatcher's socket directory.
pub const SOCKET_FILE_NAME: &str = "workspace-git.sock";

pub fn provider_ref(workspace_id: Uuid, branch: &str) -> String {
    format!("refs/g5/provider/{workspace_id}/heads/{branch}")
}

pub fn resolution_ref(workspace_id: Uuid, run_id: Uuid, side: &str) -> String {
    format!("refs/g5/provider/{workspace_id}/inputs/{run_id}/{side}")
}

pub fn local_resolution_ref(run_id: Uuid, side: &str) -> String {
    format!("refs/g5/provider-inputs/{run_id}/{side}")
}

/// Explicit, isolated destinations: fetching cannot update HEAD, the index,
/// working branches, origin tracking refs or any user files.
pub fn provider_fetch_refspecs(workspace_id: Uuid) -> [String; 2] {
    [
        format!("+refs/g5/provider/{workspace_id}/heads/*:refs/remotes/provider/*"),
        format!("+refs/g5/provider/{workspace_id}/inputs/*:refs/g5/provider-inputs/*"),
    ]
}

/// The local socket exposes only workspace-scoped operations, never caller
/// URLs, credentials, refspecs, commands or dispatcher grants.
#[derive(Debug, Deserialize, Serialize)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
pub enum LocalGitRequest {
    Refresh { repo_id: Option<Uuid> },
}
