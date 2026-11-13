//! Typed workspace materialization errors.
//!
//! Variants are designed so the supervisor can distinguish reflink failure
//! (operator action: enable `mkfs.xfs -m reflink=1` or move the data root onto
//! an XFS volume) from git failures (operator action: check repo URL / network)
//! at telemetry time, rather than collapsing every failure into a single
//! `G5Error::Internal` envelope.

use std::path::PathBuf;

use g5_contracts::error::G5Error;
use thiserror::Error;

/// Errors produced by the workspace materialization module.
///
/// See `g5/g5d/workspace` ontology D11 for the typed-error contract this enum
/// satisfies.
#[derive(Debug, Error)]
pub enum WorkspaceError {
    /// Trusted local filesystem evidence, preserved across subprocess cleanup.
    #[error("{0}")]
    DiskPressure(#[from] crate::disk::DiskPressure),

    /// Shared execution failure retains refused/partial/unknown effect evidence.
    #[error(transparent)]
    GitExecution {
        source: g5_scm::native::NativeGitError,
    },

    /// `cp --reflink=always` exited non-zero AND stderr indicates the host
    /// filesystem does not support reflinks (operator action: enable
    /// `mkfs.xfs -m reflink=1` or move the data root onto an XFS volume).
    /// The supervisor must NOT fall back to a non-reflink copy — see
    /// `g5/g5d/workspace` ontology D6.
    #[error(
        "reflink-or-fail-loud: cp --reflink=always {src} -> {dst} exited \
         {exit_code:?} (reflinks unsupported by host filesystem): {stderr}"
    )]
    ReflinkUnsupported {
        src: PathBuf,
        dst: PathBuf,
        exit_code: Option<i32>,
        stderr: String,
    },

    /// `cp --reflink=always` exited non-zero with a stderr pattern that
    /// does NOT indicate reflink-unsupported (e.g. permission denied, disk
    /// full, missing source). Kept distinct from `ReflinkUnsupported` so
    /// the operator chases the real root cause (per opus46-of-codex review:
    /// the `exit_code == 1` blanket-classify was wrong).
    #[error("cp --reflink=always {src} -> {dst} failed with exit {exit_code:?}: {stderr}")]
    ReflinkCopyFailed {
        src: PathBuf,
        dst: PathBuf,
        exit_code: Option<i32>,
        stderr: String,
    },

    /// Plain-copy fallback (`cp -a`, workspace D6 standalone-daemon
    /// exception) exited non-zero.
    #[error("cp -a {src} -> {dst} failed: {stderr}")]
    PlainCopyFailed {
        src: PathBuf,
        dst: PathBuf,
        stderr: String,
    },

    /// Repo URL failed the scheme allowlist. Defense against git-CLI
    /// argument injection (per opus47-of-opus46 review B2). Allowed
    /// shapes: `https://…`, `ssh://…`, `git@…` (scp-style), `file://…`,
    /// or an absolute `/path` (the latter two are accepted because
    /// PR-rev7-6 supports local-dev fixtures + test workflows that clone
    /// from a temp bare repo). No leading `-`. The trust model is
    /// operator-supplies-a-trusted-URL via `TaskAssignment.workspace_git_url`;
    /// this validation is defense-in-depth, not the primary permission
    /// boundary. Codex round 2 of PR-rev7-6.
    #[error(
        "repo URL rejected (allowed shapes: https://, ssh://, git@, file://, /abs; \
         no leading '-'): {repo_url}"
    )]
    RepoUrlRejected { repo_url: String },

    /// Bounded wait on the cross-process cache lock expired. Mirror of the
    /// OCI cache `CacheLockTimeout` (PR #122 Codex final-review B2).
    #[error("workspace cache lock {lock_dir} not released within {waited_secs}s")]
    CacheLockTimeout { lock_dir: PathBuf, waited_secs: u64 },

    /// `task_id` was empty, contained a path separator, or was a reserved
    /// component (`.` / `..`). The supervisor rejects the assignment before
    /// touching the filesystem.
    #[error("invalid task_id {task_id:?}: must be a non-empty filesystem-safe path component")]
    InvalidTaskId { task_id: String },

    /// `branch_name` from the inbound `TaskAssignment` would have been
    /// reinterpreted by `git checkout`'s CLI parser as a flag (leading
    /// `-`, embedded NUL/control bytes, whitespace, or oversize). Mirror
    /// of `RepoUrlRejected` for the checkout step. Codex round 2 /
    /// opus47 B-NEW-1 / opus46 of PR-rev7-6.
    #[error(
        "branch name rejected (must be non-empty, no leading '-', no NUL / control \
         bytes / whitespace, length <= 250): {branch:?}"
    )]
    InvalidBranchName { branch: String },

    /// Task assignment omitted the authoritative workspace repository vector.
    /// Host materialization never reconstructs one from singular compatibility
    /// fields or ambient Git state.
    #[error("task assignment is missing its authoritative repo_paths vector")]
    MissingRepoInventory,

    /// A populated `TaskAssignment.repo_paths` entry did not carry a smart-HTTP
    /// clone URL, so the host cannot materialize that repo into the workspace
    /// root.
    #[error("repo_paths entry {repo_id} ({repo_name}) is missing git_url")]
    MissingRepoGitUrl { repo_id: String, repo_name: String },

    /// A repository binding omitted its transport branch label.
    #[error("repo_paths entry {repo_id} ({repo_name}) is missing branch_name")]
    MissingRepoBranchName { repo_id: String, repo_name: String },

    /// A repository binding omitted or malformed its DB-selected exact SHA.
    #[error(
        "repo_paths entry {repo_id} ({repo_name}) has invalid exact base_commit {base_commit:?}"
    )]
    InvalidRepoBaseCommit {
        repo_id: String,
        repo_name: String,
        base_commit: String,
    },

    /// A populated `TaskAssignment.repo_paths` entry did not carry a usable
    /// workspace-relative mount path.
    #[error("repo_paths entry {repo_id} ({repo_name}) is missing root_path/root_paths")]
    MissingRepoMountPath { repo_id: String, repo_name: String },

    /// A `RepoInfo.root_path` / `root_paths` value would escape or collide with
    /// supervisor-owned workspace metadata.
    #[error("repo_paths entry {repo_id} ({repo_name}) has unsafe mount path {mount_path:?}")]
    UnsafeRepoMountPath {
        repo_id: String,
        repo_name: String,
        mount_path: String,
    },

    /// Two repo inventory entries claim the same path or one path nested under
    /// the other. Host-up must fail closed instead of letting one checkout
    /// obscure another.
    #[error(
        "repo_paths mount conflict: repo {first_repo_id} at {first_mount_path} conflicts \
         with repo {second_repo_id} at {second_mount_path}"
    )]
    RepoMountPathConflict {
        first_repo_id: String,
        first_mount_path: PathBuf,
        second_repo_id: String,
        second_mount_path: PathBuf,
    },

    /// More than one repository claims the docs role, so the materializer
    /// cannot choose one authoritative Project Docs target.
    #[error("project_docs alias requires exactly one docs-role repository; found {count}")]
    AmbiguousProjectDocsRepo { count: usize },

    /// The workspace id would introduce path traversal or an unsupported
    /// filesystem component beneath the docs repo's `docs/` directory.
    #[error(
        "workspace id rejected for project_docs alias (must be one safe ASCII path component): {workspace_id:?}"
    )]
    InvalidProjectDocsWorkspaceId { workspace_id: String },

    /// A repo mount or existing task-root entry collides with the reserved
    /// top-level `project_docs` alias. Existing content is never overwritten.
    #[error("project_docs alias collision at {path}: {reason}")]
    ProjectDocsAliasCollision { path: PathBuf, reason: String },

    /// The canonical docs target or one of its components is a symlink,
    /// non-directory, or otherwise escapes the docs repository.
    #[error("unsafe project_docs target at {path}: {reason}")]
    UnsafeProjectDocsTarget { path: PathBuf, reason: String },

    /// The physical alias is intentionally Unix-only for the MVP.
    #[error("project_docs filesystem alias is unsupported on this platform")]
    ProjectDocsAliasUnsupportedPlatform,

    /// Another process still owns the process-lifetime workspace-root lease.
    /// Registration must fail before rotating a same-id worker generation.
    #[error("workspace root lease is already held at {lock_path}")]
    WorkspaceRootLeaseHeld { lock_path: PathBuf },

    /// Workspace-root custody is implemented with Unix `flock` and therefore
    /// fails closed on platforms where that primitive is unavailable.
    #[error("workspace root lease is unsupported on this platform")]
    WorkspaceRootLeaseUnsupportedPlatform,

    /// Reconciliation is cleanup authority and may run only while this
    /// materializer retains the process-lifetime root lease.
    #[error("workspace root lease is not active for {workspaces_root}")]
    WorkspaceRootLeaseNotActive { workspaces_root: PathBuf },

    /// The server-issued cleanup capability was incomplete or internally
    /// inconsistent. Reject it before touching the filesystem.
    #[error("invalid registration-retirement workspace reconciliation instruction: {reason}")]
    InvalidWorkspaceReconciliationInstruction { reason: String },

    /// Existing operation metadata or filesystem state disagrees with the
    /// exact task/operation identity. Never overwrite or guess recovery state.
    #[error(
        "registration-retirement workspace reconciliation conflict for task {task_id}, \
         operation {operation_id}: {reason}"
    )]
    WorkspaceReconciliationConflict {
        task_id: String,
        operation_id: uuid::Uuid,
        reason: String,
    },

    /// A blocking filesystem lane failed to join. The operation's durable
    /// marker, when present, remains the restart source of truth.
    #[error("workspace {operation} blocking task failed: {message}")]
    WorkspaceOperationJoin {
        operation: &'static str,
        message: String,
    },

    /// Ambient filesystem I/O failure (mkdir, remove_dir_all, metadata read).
    #[error("workspace I/O at {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    /// The filesystem-copy binary could not start. Native Git preserves its
    /// typed execution error through `GitExecution`.
    #[error("failed to spawn `{program}`: {source}")]
    Spawn {
        program: String,
        #[source]
        source: std::io::Error,
    },
}

impl From<WorkspaceError> for G5Error {
    fn from(value: WorkspaceError) -> Self {
        if let WorkspaceError::DiskPressure(pressure) = value {
            return G5Error::HostDiskPressure(pressure);
        }
        G5Error::Internal {
            message: value.to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capacity_evidence_survives_workspace_and_command_error_chains() {
        let pressure = crate::disk::DiskPressure {
            phase: "repository_seed_refresh".into(),
            available_bytes: 0,
            required_bytes: 4096,
        };
        let error = WorkspaceError::from(pressure.clone());
        assert_eq!(
            crate::disk::classify_io(&error, "fallback"),
            Some(pressure.clone())
        );
        let error = G5Error::from(error);
        assert_eq!(crate::disk::classify_io(&error, "fallback"), Some(pressure));
    }

    #[test]
    fn reflink_unsupported_display_carries_stderr_for_operator() {
        let err = WorkspaceError::ReflinkUnsupported {
            src: PathBuf::from("/cache/foo"),
            dst: PathBuf::from("/work/bar"),
            exit_code: Some(1),
            stderr: "cp: failed to clone: Operation not supported".to_string(),
        };
        let msg = err.to_string();
        assert!(msg.contains("reflink-or-fail-loud"));
        assert!(msg.contains("/cache/foo"));
        assert!(msg.contains("/work/bar"));
        assert!(msg.contains("Operation not supported"));
    }

    #[test]
    fn invalid_task_id_lists_the_offending_value() {
        let err = WorkspaceError::InvalidTaskId {
            task_id: "../escape".to_string(),
        };
        let msg = err.to_string();
        assert!(msg.contains("../escape"));
        assert!(msg.contains("filesystem-safe"));
    }

    #[test]
    fn from_workspace_error_maps_to_g5_internal() {
        let err = WorkspaceError::GitExecution {
            source: g5_scm::native::NativeGitError {
                operation: "clone bare cache",
                target: Some("https://example.com/repo.git".into()),
                kind: g5_scm::native::FailureKind::Exit,
                effect: g5_scm::native::FailureEffect::Unknown,
                detail: "repository not found".into(),
            },
        };
        let g5: G5Error = err.into();
        match g5 {
            G5Error::Internal { message } => {
                assert!(message.contains("Git clone bare cache"));
                assert!(message.contains("repository not found"));
            }
            other => panic!("expected Internal, got {other:?}"),
        }
    }
}
