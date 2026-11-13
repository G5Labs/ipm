//! Unified error taxonomy for the G5 workspace.
//!
//! Provides [`G5Error`], the top-level error enum consumed by both `g5-api`
//! (HTTP error responses) and `g5d` (worker error handling). Every error
//! variant maps directly to the error code catalog defined in
//! `designs/API_AND_STORAGE.md` §4B.
//!
//! Also provides [`ErrorResponse`], the JSON error envelope returned by all
//! API error responses.

use http::StatusCode;
use serde::{Deserialize, Serialize};
use thiserror::Error;

// ---------------------------------------------------------------------------
// ErrorResponse — JSON error envelope
// ---------------------------------------------------------------------------

/// JSON-serializable error envelope returned by all API error responses.
///
/// Matches the API error format from §3.0 and §4B of `API_AND_STORAGE.md`.
/// Field names are serialized as camelCase for JSON output.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ErrorResponse {
    pub code: String,
    pub message: String,
    pub details: serde_json::Value,
    pub request_id: String,
}

/// Established effects of a failed Git operation. Backend diagnostics stay in
/// server logs; callers receive this evidence to decide whether/how to retry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GitOperationFailureEffect {
    Refused,
    Unknown,
}

impl std::fmt::Display for GitOperationFailureEffect {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Refused => "no refs were changed",
            Self::Unknown => "one or both refs may have changed",
        })
    }
}

/// Safe failure location, independent of backend diagnostic text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GitOperationStage {
    OpenRepository,
    ValidateCommits,
    LockRefs,
    InspectRefs,
    CheckPreconditions,
    StageRefs,
    CommitRefs,
}

impl std::fmt::Display for GitOperationStage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::OpenRepository => "opening the repository",
            Self::ValidateCommits => "validating the input commits",
            Self::LockRefs => "locking the input refs",
            Self::InspectRefs => "inspecting the input refs",
            Self::CheckPreconditions => "checking the expected ref values",
            Self::StageRefs => "preparing the ref updates",
            Self::CommitRefs => "committing the ref updates",
        })
    }
}

/// One member of the complete current authority cut returned after an
/// expected-cut compare-and-set failure.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthorityCutMember {
    pub repo_id: String,
    pub workspace_id: String,
    pub branch_name: String,
    pub current_head: String,
}

/// One caller expectation that differs from the current authority cut.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthorityCutMismatch {
    pub repo_id: String,
    pub expected_old: String,
    pub current_head: String,
}

/// Structured recovery input for a complete-cut CAS mismatch.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthorityCutStaleDetails {
    pub workspace_id: String,
    pub target_kind: String,
    pub target_id: String,
    pub current_cut: Vec<AuthorityCutMember>,
    pub mismatches: Vec<AuthorityCutMismatch>,
}

// ---------------------------------------------------------------------------
// G5Error — unified error enum
// ---------------------------------------------------------------------------

/// Top-level error enum for the G5 system.
///
/// Each variant corresponds to an error code from the API error catalog
/// (`designs/API_AND_STORAGE.md` §4B). Uses `thiserror::Error` for
/// `Display` and `std::error::Error` implementations.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum G5Error {
    /// Missing or invalid authentication token.
    #[error("Missing or invalid authentication token")]
    Unauthorized,

    /// Valid token but the action is refused (403): either insufficient scopes
    /// (non-empty `missing_scopes`) or a generic non-scope authorization
    /// rejection (empty `missing_scopes`).
    ///
    /// An empty `missing_scopes` marks a non-scope 403 (a generic authorization
    /// rejection — e.g. a workspace-access denial or task-scope-resolution
    /// failure) and renders the bare `"Insufficient permissions"` rather than a
    /// misleading `missing scopes []` scope list. This removes the recurring
    /// red-herring string; it does not recover the specific 403 reason, which
    /// the server does not encode on these paths (ontology D6, issue #1102).
    #[error("{}", if missing_scopes.is_empty() { String::from("Insufficient permissions") } else { format!("Insufficient permissions: missing scopes {missing_scopes:?}") })]
    Forbidden { missing_scopes: Vec<String> },

    /// A workspace-owned content mutation was attempted against a read-only
    /// workspace (`workspaces.mutation_policy = 'read_only'`). `operation` is a
    /// stable label (e.g. `"chat.send_message"`); `workspace_id` is the target.
    /// See `services/workspace_mutation_policy` [I4].
    #[error("This workspace is read-only")]
    WorkspaceReadOnly {
        operation: &'static str,
        workspace_id: String,
    },

    #[error("Workspace is not running")]
    WorkspaceNotRunning,

    #[error("Workspace is stopping")]
    WorkspaceStopping,

    /// The running checkout no longer matches the workspace's authoritative
    /// shared branch association. The caller must switch the checkout back or
    /// stop it before durable Git views can be treated as the live workspace.
    #[error("Workspace checkout does not match its shared branch")]
    WorkspaceBranchMismatch { details: serde_json::Value },

    #[error("Base workspaces do not have a runtime")]
    WorkspaceIsBase,

    /// The operation is refused for a personal workspace's registry kind
    /// (`g5.personal.refusals`).
    #[error("Personal workspaces do not support {operation}")]
    WorkspaceIsPersonal { operation: String },

    /// The owner already holds as many workspaces or running VMs as their
    /// organization allows one owner (`g5.organization.owner-limits`).
    #[error("{}", kind.message(*limit))]
    OwnerLimitReached { kind: OwnerLimitKind, limit: i64 },

    /// A workspace agent addressed another workspace while its workspace's
    /// setting forbids that (`g5.security.agent-other-workspaces`).
    #[error(
        "\"Agent may modify other workspaces\" is off for this workspace. Ask its \
         owner to turn it on in the workspace's settings."
    )]
    AgentOtherWorkspacesDisabled,

    /// Resource does not exist or is not visible to caller.
    #[error("Resource not found: {resource}")]
    NotFound { resource: String },

    /// Optimistic concurrency conflict.
    #[error("Conflict: {reason}")]
    Conflict { reason: String },

    #[error("File changed since it was loaded")]
    FileVersionConflict { details: serde_json::Value },

    #[error("File already exists")]
    FileExists { details: serde_json::Value },

    /// A referenced-question command refused by the set's current state
    /// (`g5.documents.question.writer`): `details.code` names the refusal
    /// (`revision-moved`, `idempotency-key-reused`, `reload-required`) and
    /// carries the current entry when there is one.
    #[error("Question conflict")]
    QuestionConflict { details: serde_json::Value },

    /// Endpoint or resource deliberately retired (410) — terminal for this
    /// request shape. `resource` names the retired surface and the replacement
    /// flow (ontology `wire_contract/error_taxonomy` [D1]/[D2]).
    #[error("No longer available: {resource}")]
    Gone { resource: String },

    /// A complete authority operation lost its expected-cut CAS. The current
    /// complete cut is returned so a client can recover without a helper read.
    #[error(
        "Authority cut changed before publication; run `g5d task rebase`, resolve any reported conflicts, and retry completion"
    )]
    AuthorityCutStale {
        details: Box<AuthorityCutStaleDetails>,
    },

    /// Worker attempted a live workspace file edit from an older branch head.
    #[error(
        "Stale branch head for '{path}': worker observed {worker_git_sha}, current head is {current_git_sha}"
    )]
    StaleHead {
        worker_git_sha: String,
        current_git_sha: String,
        workspace_id: Option<String>,
        path: String,
    },

    /// A commit-history pagination cursor is no longer reachable from the
    /// selected branch head, usually because history was rewritten.
    #[error("Commit history changed; restart pagination")]
    StaleHistory,

    /// A selected Diff Graph node was requested against an older projection.
    #[error("Diff Graph revision changed; reload the graph")]
    StaleGraphRevision {
        requested_revision: String,
        current_revision: String,
    },

    /// The selected Diff Graph comparison is not available for this workspace.
    #[error("Workspace Diff Graph comparison is unavailable")]
    DiffGraphUnavailable {
        reason: String,
        missing_repo_count: usize,
    },

    /// A complete Workspace comparison crossed a required-root or input-byte
    /// hard limit and therefore was rejected without truncating roots.
    #[error("Workspace Diff Graph comparison exceeds its limits")]
    DiffGraphTooLarge {
        observed_roots: usize,
        max_roots: usize,
        observed_bytes: usize,
        max_bytes: usize,
    },

    /// A committed file is larger than the bytes one read may materialize;
    /// refused from its object header before any content is loaded.
    #[error("{path} is {size} bytes; at most {max_bytes} bytes can be read here")]
    FileTooLarge {
        path: String,
        size: u64,
        max_bytes: u64,
    },

    /// A committed tree has more entries, or longer paths, than one
    /// enumeration may list; refused rather than listed in part.
    #[error("{path:?} has more entries than the {max_entries} one listing may carry")]
    TreeTooLarge {
        path: String,
        max_entries: usize,
        max_bytes: usize,
    },

    /// Request body fails schema validation.
    #[error("Validation error: {details}")]
    ValidationError { details: serde_json::Value },

    /// Too many requests.
    #[error("Rate limit exceeded")]
    RateLimited { retry_after_secs: Option<u64> },

    /// Workspace spend cap exceeded.
    #[error("Spend limit reached: ${period_spend_usd:.2} of ${limit_usd:.2} limit")]
    SpendLimitReached {
        period_spend_usd: f64,
        limit_usd: f64,
    },

    /// Branch head_commit is behind base — rebase required.
    #[error("Branch has diverged from base — rebase required")]
    BranchDiverged,

    /// live workspace file draft cannot be committed — concurrent modification detected.
    #[error("Draft conflict — concurrent modification detected")]
    DraftConflict,

    /// Task cannot transition to requested status.
    #[error("Task cannot transition from '{current_status}' to '{requested_status}'")]
    TaskNotEligible {
        current_status: String,
        requested_status: String,
    },

    /// Worker ID not in registry.
    #[error("Worker not found")]
    WorkerNotFound,

    /// Worker already assigned to a task.
    #[error("Worker is already assigned to a task")]
    WorkerBusy,

    /// Specific scope(s) required for this action.
    #[error("Required scopes missing: {missing:?}")]
    ScopeMissing { missing: Vec<String> },

    /// Org-level resource quota exceeded.
    #[error("Organization resource quota exceeded")]
    OrgQuotaExceeded,

    /// A backend service is misconfigured or unreachable so the request
    /// cannot be served. Used by routes that must fail closed at the
    /// boundary rather than silently degrade — e.g. worker-host provisioning
    /// when `cloud_features_enabled=true` but no `CloudProvider` is wired
    /// (ontology [D22]/[D24]).
    #[error("Service unavailable: {message}")]
    ServiceUnavailable { message: String },

    /// Previews are served only from isolated preview hosts
    /// (`g5.security.mvp-exposure.preview`), and this installation has no
    /// preview origin (`G5_PREVIEW_HOST_SUFFIX` unset). The platform keeps
    /// running; opening a preview is refused with this reason.
    #[error(
        "Previews are not configured on this installation: set G5_PREVIEW_HOST_SUFFIX to an isolated preview origin"
    )]
    PreviewOriginUnconfigured,

    /// A retryable failure while reading a published native session set.
    #[error("Session file store unavailable: {message}")]
    SessionFileStoreUnavailable { message: String },

    /// Stored native session content failed its declared integrity checks.
    #[error("Session file content failed integrity checks: {message}")]
    SessionFileIntegrity { message: String },

    /// A transient, retryable database failure: this transaction was chosen
    /// as a deadlock (SQLSTATE 40P01) or serialization-failure (40001)
    /// victim. The operation itself is valid; retrying it on a fresh
    /// transaction is expected to succeed. Classified once, where the
    /// `sqlx::Error` is still typed (`sqlx_error_is_retryable`), so callers
    /// can branch on retryability without matching error text.
    #[error("Retryable database error: {message}")]
    DatabaseRetryable { message: String },

    /// Unhandled server error.
    #[error("Internal error: {message}")]
    Internal { message: String },

    /// Git operation failure.
    #[error("Git error: {message}")]
    GitError { message: String },

    /// Deliberately public recovery evidence for a typed operation. Only pass
    /// authorized repository/ref identities, never paths or backend messages.
    #[error("Git operation {operation} failed while {stage}: {effect}. Inspect the requested commits and refs before deciding how to retry")]
    GitOperationFailed {
        operation: String,
        repo_id: uuid::Uuid,
        refs: Vec<String>,
        effect: GitOperationFailureEffect,
        stage: GitOperationStage,
    },

    /// Unsaved file changes prevent a destructive operation.
    #[error("Branch '{branch}' has {dirty_count} unflushed draft deltas")]
    DraftDirty { branch: String, dirty_count: u32 },

    /// Merge failed due to conflicts.
    #[error("Merge conflict: {message}")]
    MergeConflict { message: String },

    /// Configuration error (missing or invalid config value).
    #[error("Configuration error: {0}")]
    Config(String),

    /// A local host allocation failed; image or guest text cannot create this fact.
    #[error("{0}")]
    HostDiskPressure(#[from] HostDiskPressure),

    /// Feature not yet implemented.
    #[error("Not implemented: {feature}")]
    NotImplemented { feature: String },

    /// A typed external issue tracker failure. The response keeps the
    /// standard status and code for `kind` and reports the typed reason in
    /// `details.issueError`, so a client never mistakes a tracker failure
    /// for an empty result. `message` must not contain credentials.
    #[error("{message}")]
    ExternalIssue {
        kind: ExternalIssueErrorKind,
        message: String,
    },

    /// A conflict carrying structured recovery details (409 `CONFLICT`).
    #[error("Conflict: {reason}")]
    ConflictDetails {
        reason: String,
        details: serde_json::Value,
    },
}

/// Which per-owner limit refused an admission, serialized as `details.kind`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OwnerLimitKind {
    /// Live workspaces, pending forks and transfers to the owner.
    Workspaces,
    /// VMs starting, running or stopping, plus earlier waiting Starts.
    RunningVms,
}

impl OwnerLimitKind {
    /// Wire value for `details.kind`.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Workspaces => "workspaces",
            Self::RunningVms => "running_vms",
        }
    }

    #[must_use]
    pub fn message(self, limit: i64) -> String {
        match self {
            Self::Workspaces => format!(
                "The workspace owner already has {limit} workspaces, the most this \
                 organization allows one owner. Delete one before adding another."
            ),
            Self::RunningVms => format!(
                "The workspace owner already has {limit} workspaces running or starting, \
                 the most this organization allows one owner. Stop one before starting another."
            ),
        }
    }
}

/// Typed reason for an external issue tracker failure, serialized as
/// `details.issueError`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExternalIssueErrorKind {
    /// The tracker timed out, returned 5xx or was unreachable.
    Unavailable,
    /// The tracker throttled the integration; retrying later succeeds.
    RateLimited,
    /// The tracker refused the integration's credential.
    AccessDenied,
    /// The issue or container does not exist.
    NotFound,
    /// The reference names a container that is not a configured issue domain.
    OutsideDomain,
    /// The reference names a pull request.
    NotAnIssue,
    /// The integration binding cannot serve the domain (inactive, container
    /// not covered, or missing issue permission).
    BindingNeedsAttention,
    /// The reference could not be parsed.
    InvalidReference,
    /// The tracker refused a search query it could not run (for example,
    /// a qualifier naming a user that does not exist).
    InvalidQuery,
}

impl ExternalIssueErrorKind {
    /// Wire value for `details.issueError`.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Unavailable => "unavailable",
            Self::RateLimited => "rate_limited",
            Self::AccessDenied => "access_denied",
            Self::NotFound => "not_found",
            Self::OutsideDomain => "outside_domain",
            Self::NotAnIssue => "not_an_issue",
            Self::BindingNeedsAttention => "binding_needs_attention",
            Self::InvalidReference => "invalid_reference",
            Self::InvalidQuery => "invalid_query",
        }
    }

    #[must_use]
    pub fn status_code(self) -> StatusCode {
        match self {
            Self::Unavailable | Self::RateLimited => StatusCode::SERVICE_UNAVAILABLE,
            Self::AccessDenied => StatusCode::FORBIDDEN,
            Self::NotFound => StatusCode::NOT_FOUND,
            Self::OutsideDomain
            | Self::NotAnIssue
            | Self::InvalidReference
            | Self::InvalidQuery => StatusCode::UNPROCESSABLE_ENTITY,
            Self::BindingNeedsAttention => StatusCode::CONFLICT,
        }
    }

    /// The standard error code for this kind's HTTP status.
    #[must_use]
    pub const fn error_code(self) -> &'static str {
        match self {
            Self::Unavailable | Self::RateLimited => "SERVICE_UNAVAILABLE",
            Self::AccessDenied => "FORBIDDEN",
            Self::NotFound => "NOT_FOUND",
            Self::OutsideDomain
            | Self::NotAnIssue
            | Self::InvalidReference
            | Self::InvalidQuery => "VALIDATION_ERROR",
            Self::BindingNeedsAttention => "CONFLICT",
        }
    }
}

// ---------------------------------------------------------------------------
// G5Error — status code & error code methods
// ---------------------------------------------------------------------------

impl G5Error {
    /// Returns the appropriate HTTP status code for this error variant.
    #[must_use]
    pub fn status_code(&self) -> StatusCode {
        match self {
            G5Error::Unauthorized => StatusCode::UNAUTHORIZED, // 401
            G5Error::Forbidden { .. } => StatusCode::FORBIDDEN, // 403
            G5Error::WorkspaceReadOnly { .. } => StatusCode::FORBIDDEN, // 403
            G5Error::WorkspaceNotRunning => StatusCode::CONFLICT,
            G5Error::WorkspaceStopping => StatusCode::CONFLICT,
            G5Error::WorkspaceBranchMismatch { .. } => StatusCode::CONFLICT,
            G5Error::WorkspaceIsBase => StatusCode::CONFLICT,
            G5Error::WorkspaceIsPersonal { .. } => StatusCode::CONFLICT,
            G5Error::OwnerLimitReached { .. } => StatusCode::CONFLICT,
            G5Error::AgentOtherWorkspacesDisabled => StatusCode::FORBIDDEN,
            G5Error::NotFound { .. } => StatusCode::NOT_FOUND, // 404
            G5Error::Conflict { .. } => StatusCode::CONFLICT,  // 409
            G5Error::FileVersionConflict { .. } => StatusCode::CONFLICT,
            G5Error::FileExists { .. } => StatusCode::CONFLICT,
            G5Error::QuestionConflict { .. } => StatusCode::CONFLICT,
            G5Error::Gone { .. } => StatusCode::GONE, // 410
            G5Error::AuthorityCutStale { .. } => StatusCode::CONFLICT, // 409
            G5Error::StaleHead { .. } => StatusCode::CONFLICT, // 409
            G5Error::StaleHistory => StatusCode::CONFLICT, // 409
            G5Error::StaleGraphRevision { .. } => StatusCode::CONFLICT, // 409
            G5Error::DiffGraphUnavailable { .. } => StatusCode::CONFLICT, // 409
            G5Error::DiffGraphTooLarge { .. } => StatusCode::PAYLOAD_TOO_LARGE, // 413
            G5Error::FileTooLarge { .. } => StatusCode::PAYLOAD_TOO_LARGE, // 413
            G5Error::TreeTooLarge { .. } => StatusCode::PAYLOAD_TOO_LARGE, // 413
            G5Error::ValidationError { .. } => StatusCode::UNPROCESSABLE_ENTITY, // 422
            G5Error::RateLimited { .. } => StatusCode::TOO_MANY_REQUESTS, // 429
            G5Error::SpendLimitReached { .. } => StatusCode::PAYMENT_REQUIRED, // 402
            G5Error::BranchDiverged => StatusCode::CONFLICT, // 409
            G5Error::DraftConflict => StatusCode::CONFLICT, // 409
            G5Error::TaskNotEligible { .. } => StatusCode::CONFLICT, // 409
            G5Error::WorkerNotFound => StatusCode::NOT_FOUND, // 404
            G5Error::WorkerBusy => StatusCode::CONFLICT, // 409
            G5Error::ScopeMissing { .. } => StatusCode::FORBIDDEN, // 403
            G5Error::OrgQuotaExceeded => StatusCode::PAYMENT_REQUIRED, // 402
            G5Error::ServiceUnavailable { .. } => StatusCode::SERVICE_UNAVAILABLE, // 503
            G5Error::PreviewOriginUnconfigured => StatusCode::SERVICE_UNAVAILABLE, // 503
            G5Error::SessionFileStoreUnavailable { .. } => StatusCode::SERVICE_UNAVAILABLE,
            G5Error::SessionFileIntegrity { .. } => StatusCode::UNPROCESSABLE_ENTITY,
            G5Error::DatabaseRetryable { .. } => StatusCode::SERVICE_UNAVAILABLE, // 503
            G5Error::Internal { .. } => StatusCode::INTERNAL_SERVER_ERROR,        // 500
            G5Error::GitError { .. } => StatusCode::INTERNAL_SERVER_ERROR,        // 500
            G5Error::GitOperationFailed { effect, .. } => match effect {
                GitOperationFailureEffect::Refused => StatusCode::CONFLICT,
                GitOperationFailureEffect::Unknown => StatusCode::INTERNAL_SERVER_ERROR,
            },
            G5Error::DraftDirty { .. } => StatusCode::CONFLICT, // 409
            G5Error::MergeConflict { .. } => StatusCode::CONFLICT, // 409
            G5Error::Config(_) => StatusCode::INTERNAL_SERVER_ERROR, // 500
            G5Error::HostDiskPressure(_) => StatusCode::SERVICE_UNAVAILABLE,
            G5Error::NotImplemented { .. } => StatusCode::NOT_IMPLEMENTED, // 501
            G5Error::ExternalIssue { kind, .. } => kind.status_code(),
            G5Error::ConflictDetails { .. } => StatusCode::CONFLICT, // 409
        }
    }

    /// Returns the machine-readable error code string for this variant.
    ///
    /// Error codes are SCREAMING_SNAKE_CASE and match §4B of
    /// `API_AND_STORAGE.md` exactly.
    #[must_use]
    pub fn error_code(&self) -> &'static str {
        match self {
            G5Error::Unauthorized => "UNAUTHORIZED",
            G5Error::Forbidden { .. } => "FORBIDDEN",
            G5Error::WorkspaceReadOnly { .. } => "WORKSPACE_READ_ONLY",
            G5Error::WorkspaceNotRunning => "WORKSPACE_NOT_RUNNING",
            G5Error::WorkspaceStopping => "WORKSPACE_STOPPING",
            G5Error::WorkspaceBranchMismatch { .. } => "WORKSPACE_BRANCH_MISMATCH",
            G5Error::WorkspaceIsBase => "WORKSPACE_IS_BASE",
            G5Error::WorkspaceIsPersonal { .. } => "WORKSPACE_IS_PERSONAL",
            G5Error::OwnerLimitReached { .. } => "OWNER_LIMIT_REACHED",
            G5Error::AgentOtherWorkspacesDisabled => "AGENT_OTHER_WORKSPACES_DISABLED",
            G5Error::NotFound { .. } => "NOT_FOUND",
            G5Error::Conflict { .. } => "CONFLICT",
            G5Error::FileVersionConflict { .. } => "FILE_VERSION_CONFLICT",
            G5Error::FileExists { .. } => "FILE_EXISTS",
            G5Error::QuestionConflict { .. } => "QUESTION_CONFLICT",
            G5Error::Gone { .. } => "GONE",
            G5Error::AuthorityCutStale { .. } => "AUTHORITY_CUT_STALE",
            G5Error::StaleHead { .. } => "STALE_HEAD",
            G5Error::StaleHistory => "STALE_HISTORY",
            G5Error::StaleGraphRevision { .. } => "STALE_GRAPH_REVISION",
            G5Error::DiffGraphUnavailable { .. } => "DIFF_GRAPH_UNAVAILABLE",
            G5Error::DiffGraphTooLarge { .. } => "DIFF_GRAPH_TOO_LARGE",
            G5Error::FileTooLarge { .. } => "FILE_TOO_LARGE",
            G5Error::TreeTooLarge { .. } => "TREE_TOO_LARGE",
            G5Error::ValidationError { .. } => "VALIDATION_ERROR",
            G5Error::RateLimited { .. } => "RATE_LIMITED",
            G5Error::SpendLimitReached { .. } => "SPEND_LIMIT_REACHED",
            G5Error::BranchDiverged => "BRANCH_DIVERGED",
            G5Error::DraftConflict => "DRAFT_CONFLICT",
            G5Error::TaskNotEligible { .. } => "TASK_NOT_ELIGIBLE",
            G5Error::WorkerNotFound => "WORKER_NOT_FOUND",
            G5Error::WorkerBusy => "WORKER_BUSY",
            G5Error::ScopeMissing { .. } => "SCOPE_MISSING",
            G5Error::OrgQuotaExceeded => "ORG_QUOTA_EXCEEDED",
            G5Error::ServiceUnavailable { .. } => "SERVICE_UNAVAILABLE",
            G5Error::PreviewOriginUnconfigured => "PREVIEW_ORIGIN_UNCONFIGURED",
            G5Error::SessionFileStoreUnavailable { .. } => "SESSION_FILE_STORE_UNAVAILABLE",
            G5Error::SessionFileIntegrity { .. } => "SESSION_FILE_INTEGRITY",
            G5Error::DatabaseRetryable { .. } => "DATABASE_RETRYABLE",
            G5Error::Internal { .. } => "INTERNAL_ERROR",
            G5Error::GitError { .. } => "INTERNAL_ERROR",
            G5Error::GitOperationFailed { .. } => "GIT_OPERATION_FAILED",
            G5Error::DraftDirty { .. } => "DRAFT_DIRTY",
            G5Error::MergeConflict { .. } => "MERGE_CONFLICT",
            G5Error::Config(_) => "CONFIG_ERROR",
            G5Error::HostDiskPressure(_) => "HOST_DISK_CAPACITY",
            G5Error::NotImplemented { .. } => "NOT_IMPLEMENTED",
            G5Error::ExternalIssue { kind, .. } => kind.error_code(),
            G5Error::ConflictDetails { .. } => "CONFLICT",
        }
    }

    /// Returns variant-specific structured details as a JSON value.
    ///
    /// Client-facing errors include relevant details; internal errors
    /// return an empty object.
    fn error_details(&self) -> serde_json::Value {
        match self {
            G5Error::GitOperationFailed {
                operation,
                repo_id,
                refs,
                effect,
                stage,
            } => serde_json::json!({
                "operation": operation,
                "repoId": repo_id,
                "refs": refs,
                "effect": effect,
                "stage": stage,
                "action": "Inspect the requested commits and both refs before deciding how and when to retry through the existing operation",
            }),
            G5Error::Forbidden { missing_scopes } => {
                serde_json::json!({ "missing_scopes": missing_scopes })
            }
            G5Error::WorkspaceIsPersonal { operation } => {
                serde_json::json!({ "operation": operation })
            }
            G5Error::ValidationError { details } => details.clone(),
            G5Error::FileVersionConflict { details }
            | G5Error::FileExists { details }
            | G5Error::QuestionConflict { details } => details.clone(),
            G5Error::AuthorityCutStale { details } => {
                serde_json::to_value(details).unwrap_or_else(|_| serde_json::json!({}))
            }
            G5Error::StaleHead {
                worker_git_sha,
                current_git_sha,
                workspace_id,
                path,
            } => serde_json::json!({
                "worker_git_sha": worker_git_sha,
                "current_git_sha": current_git_sha,
                "workspace_id": workspace_id,
                "path": path,
            }),
            G5Error::StaleGraphRevision {
                requested_revision,
                current_revision,
            } => serde_json::json!({
                "requestedRevision": requested_revision,
                "currentRevision": current_revision,
            }),
            G5Error::DiffGraphUnavailable {
                reason,
                missing_repo_count,
            } => serde_json::json!({
                "reason": reason,
                "missingRepoCount": missing_repo_count,
            }),
            G5Error::DiffGraphTooLarge {
                observed_roots,
                max_roots,
                observed_bytes,
                max_bytes,
            } => serde_json::json!({
                "observedRoots": observed_roots,
                "maxRoots": max_roots,
                "observedBytes": observed_bytes,
                "maxBytes": max_bytes,
            }),
            G5Error::FileTooLarge {
                path,
                size,
                max_bytes,
            } => serde_json::json!({
                "path": path,
                "size": size,
                "maxBytes": max_bytes,
            }),
            G5Error::TreeTooLarge {
                path,
                max_entries,
                max_bytes,
            } => serde_json::json!({
                "path": path,
                "maxEntries": max_entries,
                "maxBytes": max_bytes,
            }),
            G5Error::SpendLimitReached {
                period_spend_usd,
                limit_usd,
            } => {
                serde_json::json!({
                    "period_spend_usd": period_spend_usd,
                    "limit_usd": limit_usd,
                })
            }
            G5Error::TaskNotEligible {
                current_status,
                requested_status,
            } => {
                serde_json::json!({
                    "current_status": current_status,
                    "requested_status": requested_status,
                })
            }
            G5Error::ScopeMissing { missing } => {
                serde_json::json!({ "missing": missing })
            }
            G5Error::WorkspaceReadOnly {
                operation,
                workspace_id,
            } => serde_json::json!({
                "operation": operation,
                "workspaceId": workspace_id,
            }),
            G5Error::WorkspaceBranchMismatch { details } => details.clone(),
            G5Error::ExternalIssue { kind, .. } => {
                serde_json::json!({ "issueError": kind.as_str() })
            }
            G5Error::ConflictDetails { details, .. } => details.clone(),
            G5Error::OwnerLimitReached { kind, limit } => serde_json::json!({
                "kind": kind.as_str(),
                "limit": limit,
            }),
            G5Error::RateLimited { retry_after_secs } => {
                if let Some(secs) = retry_after_secs {
                    serde_json::json!({ "retry_after_secs": secs })
                } else {
                    serde_json::json!({})
                }
            }
            // All other variants (including internal errors) return empty details.
            _ => serde_json::json!({}),
        }
    }

    /// Constructs the full [`ErrorResponse`] envelope for this error.
    ///
    /// # Security
    ///
    /// For `Internal` and `GitError` variants, the response
    /// `message` is sanitized to `"An internal error occurred"`. The detailed
    /// message is available via `Display` for server-side logging only.
    #[must_use]
    pub fn to_error_response(&self, request_id: String) -> ErrorResponse {
        match self {
            // Internal errors: sanitize the message — never expose details to clients.
            G5Error::Internal { .. }
            | G5Error::GitError { .. }
            | G5Error::SessionFileStoreUnavailable { .. }
            | G5Error::SessionFileIntegrity { .. } => ErrorResponse {
                code: self.error_code().to_string(),
                message: "An internal error occurred".to_string(),
                details: serde_json::json!({}),
                request_id,
            },
            // Retryable database victims share the sanitization discipline
            // (the deadlock/serialization detail stays server-side) but keep
            // their own code so callers can branch on retryability.
            G5Error::DatabaseRetryable { .. } => ErrorResponse {
                code: self.error_code().to_string(),
                message: "A transient database conflict interrupted the request; retry it"
                    .to_string(),
                details: serde_json::json!({}),
                request_id,
            },
            // Client errors: include the full Display message and variant-specific details.
            _ => ErrorResponse {
                code: self.error_code().to_string(),
                message: self.to_string(),
                details: self.error_details(),
                request_id,
            },
        }
    }
}

// ---------------------------------------------------------------------------
// Into<(StatusCode, ErrorResponse)> for axum integration
// ---------------------------------------------------------------------------

impl From<G5Error> for (StatusCode, ErrorResponse) {
    /// Converts a `G5Error` into an axum-compatible `(StatusCode, ErrorResponse)` tuple.
    ///
    /// The `request_id` defaults to an empty string — the axum middleware layer
    /// is responsible for injecting the actual request ID before serializing.
    fn from(err: G5Error) -> Self {
        let status = err.status_code();
        let response = err.to_error_response(String::new());
        (status, response)
    }
}

// ---------------------------------------------------------------------------
// From<T> conversions for ? propagation
// ---------------------------------------------------------------------------

impl From<serde_json::Error> for G5Error {
    fn from(e: serde_json::Error) -> Self {
        G5Error::Internal {
            message: format!("Serialization error: {}", e),
        }
    }
}

impl From<std::io::Error> for G5Error {
    fn from(e: std::io::Error) -> Self {
        G5Error::Internal {
            message: format!("IO error: {}", e),
        }
    }
}

#[cfg(feature = "jwt-errors")]
impl From<jsonwebtoken::errors::Error> for G5Error {
    fn from(_e: jsonwebtoken::errors::Error) -> Self {
        G5Error::Unauthorized
    }
}

/// Returns true when the `sqlx::Error` is a transient, retryable database
/// failure: Postgres chose this transaction as a deadlock (SQLSTATE `40P01`)
/// or serialization-failure (`40001`) victim. This is the single typed
/// classification point — callers must branch here (or on the
/// `G5Error::DatabaseRetryable` variant it feeds), never by matching
/// stringified error text.
#[cfg(feature = "sqlx")]
#[must_use]
pub fn sqlx_error_is_retryable(error: &sqlx::Error) -> bool {
    error
        .as_database_error()
        .and_then(|db| db.code())
        .is_some_and(|code| code == "40P01" || code == "40001")
}

/// Feature-gated conversion from `sqlx::Error` to `G5Error`.
///
/// Only available when the `sqlx` feature is enabled. This allows `g5d` to
/// depend on `g5-common` without pulling in the sqlx dependency. Retryable
/// deadlock/serialization victims become `DatabaseRetryable`; everything
/// else stays `Internal`.
#[cfg(feature = "sqlx")]
impl From<sqlx::Error> for G5Error {
    fn from(e: sqlx::Error) -> Self {
        if e.as_database_error().is_some_and(|error| {
            error.code().as_deref() == Some("P0001")
                && error.constraint() == Some("workspace_deletion_reserved")
        }) {
            G5Error::Conflict {
                reason: "Workspace deletion is in progress".into(),
            }
        } else if sqlx_error_is_retryable(&e) {
            G5Error::DatabaseRetryable {
                message: format!("Database error: {}", e),
            }
        } else {
            G5Error::Internal {
                message: format!("Database error: {}", e),
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Test helpers
// ---------------------------------------------------------------------------

#[cfg(test)]
impl G5Error {
    /// Create a `NotFound` error for tests.
    #[must_use]
    pub fn test_not_found(resource: &str) -> Self {
        G5Error::NotFound {
            resource: resource.to_string(),
        }
    }

    /// Create an `Internal` error for tests.
    #[must_use]
    pub fn test_internal(msg: &str) -> Self {
        G5Error::Internal {
            message: msg.to_string(),
        }
    }
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -- Status code mapping tests --

    #[test]
    fn test_unauthorized_status() {
        assert_eq!(
            G5Error::Unauthorized.status_code(),
            StatusCode::UNAUTHORIZED
        );
    }

    #[test]
    fn test_forbidden_status() {
        let err = G5Error::Forbidden {
            missing_scopes: vec!["admin".into()],
        };
        assert_eq!(err.status_code(), StatusCode::FORBIDDEN);
    }

    #[test]
    fn test_not_found_status() {
        let err = G5Error::NotFound {
            resource: "workspace ws-123".into(),
        };
        assert_eq!(err.status_code(), StatusCode::NOT_FOUND);
    }

    #[test]
    fn test_conflict_status() {
        let err = G5Error::Conflict {
            reason: "stale".into(),
        };
        assert_eq!(err.status_code(), StatusCode::CONFLICT);
    }

    #[test]
    fn test_validation_error_status() {
        let err = G5Error::ValidationError {
            details: serde_json::json!({}),
        };
        assert_eq!(err.status_code(), StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[test]
    fn test_rate_limited_status() {
        let err = G5Error::RateLimited {
            retry_after_secs: Some(60),
        };
        assert_eq!(err.status_code(), StatusCode::TOO_MANY_REQUESTS);
    }

    #[test]
    fn test_spend_limit_reached_status() {
        let err = G5Error::SpendLimitReached {
            period_spend_usd: 50.0,
            limit_usd: 100.0,
        };
        assert_eq!(err.status_code(), StatusCode::PAYMENT_REQUIRED);
    }

    #[test]
    fn test_branch_diverged_status() {
        assert_eq!(G5Error::BranchDiverged.status_code(), StatusCode::CONFLICT);
    }

    #[test]
    fn test_draft_conflict_status() {
        assert_eq!(G5Error::DraftConflict.status_code(), StatusCode::CONFLICT);
    }

    #[test]
    fn test_task_not_eligible_status() {
        let err = G5Error::TaskNotEligible {
            current_status: "pending".into(),
            requested_status: "completed".into(),
        };
        assert_eq!(err.status_code(), StatusCode::CONFLICT);
    }

    #[test]
    fn test_worker_not_found_status() {
        assert_eq!(G5Error::WorkerNotFound.status_code(), StatusCode::NOT_FOUND);
    }

    #[test]
    fn test_worker_busy_status() {
        assert_eq!(G5Error::WorkerBusy.status_code(), StatusCode::CONFLICT);
    }

    #[test]
    fn test_scope_missing_status() {
        let err = G5Error::ScopeMissing {
            missing: vec!["write".into()],
        };
        assert_eq!(err.status_code(), StatusCode::FORBIDDEN);
    }

    #[test]
    fn test_org_quota_exceeded_status() {
        assert_eq!(
            G5Error::OrgQuotaExceeded.status_code(),
            StatusCode::PAYMENT_REQUIRED
        );
    }

    #[test]
    fn test_service_unavailable_status_and_code() {
        let err = G5Error::ServiceUnavailable {
            message: "backend not configured".to_string(),
        };
        assert_eq!(err.status_code(), StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(err.error_code(), "SERVICE_UNAVAILABLE");
        let body = err.to_error_response(String::new());
        assert!(body.message.contains("backend not configured"));
    }

    #[test]
    fn test_database_retryable_status_code_and_sanitized_envelope() {
        let err = G5Error::DatabaseRetryable {
            message: "Database error: error returned from database: deadlock detected".to_string(),
        };
        assert_eq!(err.status_code(), StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(err.error_code(), "DATABASE_RETRYABLE");
        let body = err.to_error_response("req-1".to_string());
        assert_eq!(body.code, "DATABASE_RETRYABLE");
        assert_eq!(
            body.message,
            "A transient database conflict interrupted the request; retry it"
        );
        assert!(
            !body.message.contains("deadlock"),
            "database detail must stay server-side"
        );
        assert_eq!(body.details, serde_json::json!({}));
    }

    #[cfg(feature = "sqlx")]
    mod sqlx_retryable_classification {
        use super::super::*;
        use std::borrow::Cow;

        #[derive(Debug)]
        struct FakePgError {
            code: &'static str,
        }

        impl std::fmt::Display for FakePgError {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, "error returned from database: fake ({})", self.code)
            }
        }

        impl std::error::Error for FakePgError {}

        impl sqlx::error::DatabaseError for FakePgError {
            fn message(&self) -> &str {
                "fake database error"
            }

            fn code(&self) -> Option<Cow<'_, str>> {
                Some(Cow::Borrowed(self.code))
            }

            fn kind(&self) -> sqlx::error::ErrorKind {
                sqlx::error::ErrorKind::Other
            }

            fn as_error(&self) -> &(dyn std::error::Error + Send + Sync + 'static) {
                self
            }

            fn as_error_mut(&mut self) -> &mut (dyn std::error::Error + Send + Sync + 'static) {
                self
            }

            fn into_error(self: Box<Self>) -> Box<dyn std::error::Error + Send + Sync + 'static> {
                self
            }
        }

        fn database_error(code: &'static str) -> sqlx::Error {
            sqlx::Error::Database(Box::new(FakePgError { code }))
        }

        #[test]
        fn deadlock_and_serialization_victims_are_retryable() {
            assert!(sqlx_error_is_retryable(&database_error("40P01")));
            assert!(sqlx_error_is_retryable(&database_error("40001")));
        }

        #[test]
        fn other_database_and_transport_errors_are_not_retryable() {
            assert!(!sqlx_error_is_retryable(&database_error("23505")));
            assert!(!sqlx_error_is_retryable(&sqlx::Error::RowNotFound));
            assert!(!sqlx_error_is_retryable(&sqlx::Error::Protocol(
                "deadlock detected".to_string()
            )));
        }

        #[test]
        fn from_sqlx_error_routes_retryable_victims_to_typed_variant() {
            match G5Error::from(database_error("40P01")) {
                G5Error::DatabaseRetryable { message } => {
                    assert!(message.starts_with("Database error: "));
                }
                other => panic!("deadlock victim must classify as DatabaseRetryable: {other:?}"),
            }
            match G5Error::from(database_error("23505")) {
                G5Error::Internal { .. } => {}
                other => panic!("unique violation must stay Internal: {other:?}"),
            }
        }
    }

    #[test]
    fn test_gone_status_code_and_envelope() {
        let err = G5Error::Gone {
            resource: "POST /docs/views — persona view creation is retired; create an HTML explainer via POST /docs/explainers instead".to_string(),
        };
        assert_eq!(err.status_code(), StatusCode::GONE);
        assert_eq!(err.error_code(), "GONE");
        let body = err.to_error_response("req_gone".into());
        assert_eq!(body.code, "GONE");
        assert!(body.message.starts_with("No longer available: "));
        assert!(body.message.contains("POST /docs/explainers"));
        assert_eq!(body.details, serde_json::json!({}));
        assert_eq!(body.request_id, "req_gone");
    }

    #[test]
    fn test_internal_status() {
        let err = G5Error::Internal {
            message: "oops".into(),
        };
        assert_eq!(err.status_code(), StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[test]
    fn test_git_error_status() {
        let err = G5Error::GitError {
            message: "bad ref".into(),
        };
        assert_eq!(err.status_code(), StatusCode::INTERNAL_SERVER_ERROR);
    }

    // -- Error code tests --

    #[test]
    fn test_error_codes() {
        assert_eq!(G5Error::Unauthorized.error_code(), "UNAUTHORIZED");
        assert_eq!(
            G5Error::Forbidden {
                missing_scopes: vec![]
            }
            .error_code(),
            "FORBIDDEN"
        );
        assert_eq!(
            G5Error::NotFound {
                resource: String::new()
            }
            .error_code(),
            "NOT_FOUND"
        );
        assert_eq!(
            G5Error::Conflict {
                reason: String::new()
            }
            .error_code(),
            "CONFLICT"
        );
        assert_eq!(
            G5Error::ValidationError {
                details: serde_json::json!(null)
            }
            .error_code(),
            "VALIDATION_ERROR"
        );
        assert_eq!(
            G5Error::RateLimited {
                retry_after_secs: None
            }
            .error_code(),
            "RATE_LIMITED"
        );
        assert_eq!(
            G5Error::SpendLimitReached {
                period_spend_usd: 0.0,
                limit_usd: 0.0
            }
            .error_code(),
            "SPEND_LIMIT_REACHED"
        );
        assert_eq!(G5Error::BranchDiverged.error_code(), "BRANCH_DIVERGED");
        assert_eq!(G5Error::DraftConflict.error_code(), "DRAFT_CONFLICT");
        assert_eq!(
            G5Error::TaskNotEligible {
                current_status: String::new(),
                requested_status: String::new()
            }
            .error_code(),
            "TASK_NOT_ELIGIBLE"
        );
        assert_eq!(G5Error::WorkerNotFound.error_code(), "WORKER_NOT_FOUND");
        assert_eq!(G5Error::WorkerBusy.error_code(), "WORKER_BUSY");
        assert_eq!(
            G5Error::ScopeMissing { missing: vec![] }.error_code(),
            "SCOPE_MISSING"
        );
        assert_eq!(G5Error::OrgQuotaExceeded.error_code(), "ORG_QUOTA_EXCEEDED");
        assert_eq!(
            G5Error::Internal {
                message: String::new()
            }
            .error_code(),
            "INTERNAL_ERROR"
        );
        assert_eq!(
            G5Error::GitError {
                message: String::new()
            }
            .error_code(),
            "INTERNAL_ERROR"
        );
    }

    #[test]
    fn workspace_diff_graph_errors_have_stable_status_codes_and_details() {
        let unavailable = G5Error::DiffGraphUnavailable {
            reason: "no_common_ancestor".to_string(),
            missing_repo_count: 2,
        };
        assert_eq!(unavailable.status_code(), StatusCode::CONFLICT);
        assert_eq!(unavailable.error_code(), "DIFF_GRAPH_UNAVAILABLE");
        assert_eq!(
            unavailable
                .to_error_response("request-1".to_string())
                .details,
            serde_json::json!({
                "reason": "no_common_ancestor",
                "missingRepoCount": 2,
            })
        );

        let too_large = G5Error::DiffGraphTooLarge {
            observed_roots: 501,
            max_roots: 500,
            observed_bytes: 10_485_761,
            max_bytes: 10_485_760,
        };
        assert_eq!(too_large.status_code(), StatusCode::PAYLOAD_TOO_LARGE);
        assert_eq!(too_large.error_code(), "DIFF_GRAPH_TOO_LARGE");
        assert_eq!(
            too_large.to_error_response("request-3".to_string()).details,
            serde_json::json!({
                "observedRoots": 501,
                "maxRoots": 500,
                "observedBytes": 10_485_761,
                "maxBytes": 10_485_760,
            })
        );

        let file = G5Error::FileTooLarge {
            path: "assets/video.mp4".to_string(),
            size: 33_554_433,
            max_bytes: 33_554_432,
        };
        assert_eq!(file.status_code(), StatusCode::PAYLOAD_TOO_LARGE);
        assert_eq!(file.error_code(), "FILE_TOO_LARGE");
        assert_eq!(
            file.to_string(),
            "assets/video.mp4 is 33554433 bytes; at most 33554432 bytes can be read here"
        );
        assert_eq!(
            file.to_error_response("request-4".to_string()).details,
            serde_json::json!({
                "path": "assets/video.mp4",
                "size": 33_554_433,
                "maxBytes": 33_554_432,
            })
        );

        let tree = G5Error::TreeTooLarge {
            path: String::new(),
            max_entries: 250_000,
            max_bytes: 33_554_432,
        };
        assert_eq!(tree.status_code(), StatusCode::PAYLOAD_TOO_LARGE);
        assert_eq!(tree.error_code(), "TREE_TOO_LARGE");
        assert_eq!(
            tree.to_error_response("request-5".to_string()).details,
            serde_json::json!({
                "path": "",
                "maxEntries": 250_000,
                "maxBytes": 33_554_432,
            })
        );
    }

    // -- ErrorResponse tests --

    #[test]
    fn test_error_response_serialization_camel_case() {
        let resp = ErrorResponse {
            code: "NOT_FOUND".into(),
            message: "Resource not found: workspace ws-123".into(),
            details: serde_json::json!({}),
            request_id: "req_abc123".into(),
        };
        let json = serde_json::to_value(&resp).unwrap();
        // Verify camelCase field names
        assert!(json.get("code").is_some());
        assert!(json.get("message").is_some());
        assert!(json.get("details").is_some());
        assert!(json.get("requestId").is_some());
        // Verify snake_case is NOT present
        assert!(json.get("request_id").is_none());
    }

    #[test]
    fn test_error_response_deserialization() {
        let json_str = r#"{
            "code": "NOT_FOUND",
            "message": "Resource not found",
            "details": {},
            "requestId": "req_123"
        }"#;
        let resp: ErrorResponse = serde_json::from_str(json_str).unwrap();
        assert_eq!(resp.code, "NOT_FOUND");
        assert_eq!(resp.request_id, "req_123");
    }

    // -- to_error_response tests --

    #[test]
    fn test_internal_error_sanitized() {
        let err = G5Error::Internal {
            message: "secret database password exposed".into(),
        };
        let resp = err.to_error_response("req_1".into());
        assert_eq!(resp.code, "INTERNAL_ERROR");
        assert_eq!(resp.message, "An internal error occurred");
        assert_eq!(resp.details, serde_json::json!({}));
        assert_eq!(resp.request_id, "req_1");
    }

    #[test]
    fn test_git_error_sanitized() {
        let err = G5Error::GitError {
            message: "internal git detail".into(),
        };
        let resp = err.to_error_response("req_2".into());
        assert_eq!(resp.message, "An internal error occurred");
    }

    #[test]
    fn test_not_found_error_response() {
        let err = G5Error::NotFound {
            resource: "workspace ws-123".into(),
        };
        let resp = err.to_error_response("req_4".into());
        assert_eq!(resp.code, "NOT_FOUND");
        assert_eq!(resp.message, "Resource not found: workspace ws-123");
        assert_eq!(resp.details, serde_json::json!({}));
    }

    #[test]
    fn test_forbidden_error_response_details() {
        let err = G5Error::Forbidden {
            missing_scopes: vec!["admin".into(), "write".into()],
        };
        let resp = err.to_error_response("req_5".into());
        assert_eq!(resp.code, "FORBIDDEN");
        let scopes = resp
            .details
            .get("missing_scopes")
            .unwrap()
            .as_array()
            .unwrap();
        assert_eq!(scopes.len(), 2);
    }

    #[test]
    fn test_validation_error_response_details() {
        let details = serde_json::json!({"field": "email", "error": "invalid"});
        let err = G5Error::ValidationError {
            details: details.clone(),
        };
        let resp = err.to_error_response("req_6".into());
        assert_eq!(resp.code, "VALIDATION_ERROR");
        assert_eq!(resp.details, details);
    }

    #[test]
    fn test_spend_limit_error_response_details() {
        let err = G5Error::SpendLimitReached {
            period_spend_usd: 95.50,
            limit_usd: 100.00,
        };
        let resp = err.to_error_response("req_7".into());
        assert_eq!(resp.code, "SPEND_LIMIT_REACHED");
        assert_eq!(resp.details["period_spend_usd"], 95.50);
        assert_eq!(resp.details["limit_usd"], 100.00);
    }

    #[test]
    fn test_task_not_eligible_error_response_details() {
        let err = G5Error::TaskNotEligible {
            current_status: "pending".into(),
            requested_status: "completed".into(),
        };
        let resp = err.to_error_response("req_8".into());
        assert_eq!(resp.code, "TASK_NOT_ELIGIBLE");
        assert_eq!(resp.details["current_status"], "pending");
        assert_eq!(resp.details["requested_status"], "completed");
    }

    #[test]
    fn test_scope_missing_error_response_details() {
        let err = G5Error::ScopeMissing {
            missing: vec!["read".into(), "write".into()],
        };
        let resp = err.to_error_response("req_9".into());
        assert_eq!(resp.code, "SCOPE_MISSING");
        let missing = resp.details.get("missing").unwrap().as_array().unwrap();
        assert_eq!(missing.len(), 2);
    }

    #[test]
    fn test_rate_limited_with_retry_after() {
        let err = G5Error::RateLimited {
            retry_after_secs: Some(30),
        };
        let resp = err.to_error_response("req_10".into());
        assert_eq!(resp.code, "RATE_LIMITED");
        assert_eq!(resp.details["retry_after_secs"], 30);
    }

    #[test]
    fn test_rate_limited_without_retry_after() {
        let err = G5Error::RateLimited {
            retry_after_secs: None,
        };
        let resp = err.to_error_response("req_11".into());
        assert_eq!(resp.code, "RATE_LIMITED");
        assert_eq!(resp.details, serde_json::json!({}));
    }

    // -- Into<(StatusCode, ErrorResponse)> tests --

    #[test]
    fn test_into_tuple() {
        let err = G5Error::NotFound {
            resource: "test".into(),
        };
        let (status, resp): (StatusCode, ErrorResponse) = err.into();
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(resp.code, "NOT_FOUND");
        assert!(resp.request_id.is_empty()); // default empty string
    }

    // -- From<T> conversion tests --

    #[test]
    fn test_from_serde_json_error() {
        let json_err = serde_json::from_str::<String>("invalid json").unwrap_err();
        let g5_err = G5Error::from(json_err);
        match g5_err {
            G5Error::Internal { message } => {
                assert!(message.starts_with("Serialization error:"));
            }
            _ => panic!("Expected Internal variant"),
        }
    }

    #[test]
    fn test_from_io_error() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file missing");
        let g5_err = G5Error::from(io_err);
        match g5_err {
            G5Error::Internal { message } => {
                assert!(message.starts_with("IO error:"));
            }
            _ => panic!("Expected Internal variant"),
        }
    }

    // -- Display tests --

    #[test]
    fn test_display_unauthorized() {
        let err = G5Error::Unauthorized;
        assert_eq!(err.to_string(), "Missing or invalid authentication token");
    }

    #[test]
    fn test_display_not_found() {
        let err = G5Error::NotFound {
            resource: "workspace ws-123".into(),
        };
        assert_eq!(err.to_string(), "Resource not found: workspace ws-123");
    }

    #[test]
    fn test_display_internal_contains_message() {
        let err = G5Error::Internal {
            message: "db connection lost".into(),
        };
        // Display shows the internal detail (for server-side logging)
        assert_eq!(err.to_string(), "Internal error: db connection lost");
    }

    #[test]
    fn test_display_forbidden_empty_scopes_is_not_a_scope_message() {
        // A non-scope 403 (generic authorization rejection — e.g. a
        // workspace-access denial or task-scope-resolution failure) carries no
        // missing scopes. The Display MUST NOT fabricate a scope list
        // ("missing scopes []"), which has been a recurring cross-incident
        // diagnostic red herring (issue #1102).
        let err = G5Error::Forbidden {
            missing_scopes: vec![],
        };
        assert_eq!(err.to_string(), "Insufficient permissions");
        assert!(
            !err.to_string().contains("missing scopes"),
            "empty-scope Forbidden must not mention 'missing scopes'"
        );

        // The server envelope message is derived from Display, so the fix must
        // also reach the wire surface: no fabricated scope list, stable code,
        // and an empty `missing_scopes` details array (unchanged contract).
        let resp = err.to_error_response("req_403_empty".into());
        assert_eq!(resp.code, "FORBIDDEN");
        assert_eq!(resp.message, "Insufficient permissions");
        assert!(!resp.message.contains("missing scopes"));
        assert_eq!(
            resp.details
                .get("missing_scopes")
                .unwrap()
                .as_array()
                .unwrap()
                .len(),
            0
        );
    }

    #[test]
    fn test_display_forbidden_with_scopes_lists_them() {
        let err = G5Error::Forbidden {
            missing_scopes: vec!["admin".into(), "write".into()],
        };
        assert_eq!(
            err.to_string(),
            "Insufficient permissions: missing scopes [\"admin\", \"write\"]"
        );

        // The scope case is unchanged on the wire: the envelope message still
        // lists the scopes and `details.missing_scopes` carries them.
        let resp = err.to_error_response("req_403_scopes".into());
        assert_eq!(resp.code, "FORBIDDEN");
        assert_eq!(
            resp.message,
            "Insufficient permissions: missing scopes [\"admin\", \"write\"]"
        );
        let scopes = resp
            .details
            .get("missing_scopes")
            .unwrap()
            .as_array()
            .unwrap();
        assert_eq!(scopes.len(), 2);
        assert_eq!(scopes[0], "admin");
        assert_eq!(scopes[1], "write");
    }

    // -- Test helpers --

    #[test]
    fn test_test_helpers() {
        let err = G5Error::test_not_found("workspace ws-1");
        assert_eq!(err.status_code(), StatusCode::NOT_FOUND);

        let err = G5Error::test_internal("boom");
        assert_eq!(err.status_code(), StatusCode::INTERNAL_SERVER_ERROR);
    }

    // -- Variant count test --

    #[test]
    fn test_all_variants_have_error_codes() {
        // Ensure all 17 variants are covered by creating one of each
        // and verifying error_code() returns a non-empty string.
        let variants: Vec<G5Error> = vec![
            G5Error::Unauthorized,
            G5Error::Forbidden {
                missing_scopes: vec![],
            },
            G5Error::NotFound {
                resource: String::new(),
            },
            G5Error::Conflict {
                reason: String::new(),
            },
            G5Error::Gone {
                resource: String::new(),
            },
            G5Error::ValidationError {
                details: serde_json::json!(null),
            },
            G5Error::RateLimited {
                retry_after_secs: None,
            },
            G5Error::SpendLimitReached {
                period_spend_usd: 0.0,
                limit_usd: 0.0,
            },
            G5Error::BranchDiverged,
            G5Error::DraftConflict,
            G5Error::TaskNotEligible {
                current_status: String::new(),
                requested_status: String::new(),
            },
            G5Error::WorkerNotFound,
            G5Error::WorkerBusy,
            G5Error::ScopeMissing { missing: vec![] },
            G5Error::OrgQuotaExceeded,
            G5Error::ServiceUnavailable {
                message: String::new(),
            },
            G5Error::Internal {
                message: String::new(),
            },
            G5Error::GitError {
                message: String::new(),
            },
        ];
        assert_eq!(variants.len(), 18);
        for v in &variants {
            assert!(!v.error_code().is_empty());
            // Verify status code is valid
            assert!(v.status_code().as_u16() >= 400);
        }
    }
}

pub const PRESSURE_PREFIX: &str = "host_disk_capacity:";

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct HostDiskPressure {
    pub phase: String,
    pub available_bytes: u64,
    pub required_bytes: u64,
}
impl HostDiskPressure {
    pub fn reason(&self) -> String {
        format!(
            "{PRESSURE_PREFIX}{}",
            serde_json::to_string(self).expect("disk diagnostic")
        )
    }
    pub fn parse(reason: &str) -> Option<Self> {
        let json = reason.strip_prefix(PRESSURE_PREFIX)?;
        let value: Self = serde_json::from_str(json.trim()).ok()?;
        (!value.phase.is_empty()
            && value.phase.len() <= 64
            && value
                .phase
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b == b'_'))
        .then_some(value)
    }

    /// Helpers emit one canonical diagnostic on stderr. Guest serial output is
    /// stdout, and the host attaches the stream prefix to every captured line.
    pub fn from_console_tail(tail: &str) -> Option<Self> {
        tail.lines()
            .filter_map(|line| line.strip_prefix("[stderr] "))
            .find_map(Self::parse)
    }
}
impl std::fmt::Display for HostDiskPressure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.reason())
    }
}
impl std::error::Error for HostDiskPressure {}
