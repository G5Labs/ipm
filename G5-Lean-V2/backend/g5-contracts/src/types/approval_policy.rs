//! Selected approval request and status policy
//! (g5.governance.approval.request-pr, g5.governance.approval.source-commit-policy).

use serde::Serialize;

/// The action taken on a counted decision when the source commit vector moves.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SourceCommitVectorPolicy {
    /// A counted decision keeps counting after later source commits.
    Ignore,
    /// Only decisions on the current source vector count.
    MakePending,
}

/// `on_source_commit_vector_change`: IGNORE is selected.
pub const ON_SOURCE_COMMIT_VECTOR_CHANGE: SourceCommitVectorPolicy =
    SourceCommitVectorPolicy::Ignore;

/// `PR_REQUIRED`: whether human and agent approval requests need an open PR.
pub const PR_REQUIRED: bool = false;

/// The policy one deployment evaluates. Production always uses the selected
/// constants; only tests construct the alternatives.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApprovalPolicy {
    pub pr_required: bool,
    pub on_source_commit_vector_change: SourceCommitVectorPolicy,
}

impl Default for ApprovalPolicy {
    fn default() -> Self {
        Self {
            pr_required: PR_REQUIRED,
            on_source_commit_vector_change: ON_SOURCE_COMMIT_VECTOR_CHANGE,
        }
    }
}
