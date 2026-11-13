//! Stable identities shared by the owners of one formal review attempt.
use sha2::{Digest, Sha256};
use uuid::Uuid;

pub const APPROVAL_EVIDENCE_REF_PREFIX: &str = "refs/g5/approval-evidence/";

pub const FORMAL_APPROVAL_CREDENTIAL_KEYS: &[&str] = &[
    "ANTHROPIC_API_KEY",
    "ANTHROPIC_AUTH_TOKEN",
    "CLAUDE_CODE_OAUTH_TOKEN",
    "OPENAI_API_KEY",
    "CODEX_API_KEY",
    "OPENROUTER_API_KEY",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ApprovalReviewIdentity(Uuid);

impl ApprovalReviewIdentity {
    pub const fn new(attempt: Uuid) -> Self {
        Self(attempt)
    }

    pub fn resolution_id(self) -> String {
        format!("apr_{:x}", Sha256::digest(self.0.as_bytes()))
    }

    pub fn workspace_id(self) -> Uuid {
        Uuid::new_v5(&self.0, b"formal-review-workspace")
    }

    pub fn artifact_id(self) -> Uuid {
        Uuid::new_v5(&self.0, b"artifact")
    }

    pub fn output_doc_id(self) -> Uuid {
        Uuid::new_v5(&self.0, b"report")
    }

    pub fn output_doc_path(self, source: Uuid) -> String {
        format!("docs/{source}/reviews/agent-approval-{}.md", self.0)
    }

    pub fn workspace_branch(self) -> String {
        approval_review_workspace_branch(self.workspace_id())
    }

    pub fn operation_key(self) -> String {
        format!("formal-agent-review:{}", self.0)
    }

    pub fn base_branch(self) -> String {
        format!("g5-agent-review-base-{}", self.0.simple())
    }

    pub fn base_proof_id(self) -> Uuid {
        Uuid::new_v5(&self.0, b"formal-review-base")
    }
}

pub fn approval_review_workspace_branch(workspace: Uuid) -> String {
    format!("agent-review-{}", workspace.simple())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApprovalEvidenceSide {
    Source,
    Base,
    Parent,
}

pub fn approval_evidence_ref(
    resolution: &str,
    repo: Uuid,
    side: ApprovalEvidenceSide,
) -> Result<String, String> {
    super::approval_artifact::validate_prefixed_hash(resolution, "apr_", 64, "resolutionId")?;
    let side = match side {
        ApprovalEvidenceSide::Source => "source",
        ApprovalEvidenceSide::Base => "base",
        ApprovalEvidenceSide::Parent => "parent",
    };
    Ok(format!(
        "{APPROVAL_EVIDENCE_REF_PREFIX}{resolution}/{repo}/{side}"
    ))
}

pub fn formal_review_runtime_scope(workspace: Uuid) -> String {
    format!("formal-review:{workspace}")
}

pub fn formal_review_read_scope(workspace: Uuid) -> String {
    format!("formal-review-read:{workspace}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn existing_attempt_names_remain_compatible_and_evidence_is_resolution_scoped() {
        let attempt = Uuid::from_u128(1);
        let identity = ApprovalReviewIdentity::new(attempt);
        let workspace = identity.workspace_id();
        assert_eq!(
            identity.operation_key(),
            "formal-agent-review:00000000-0000-0000-0000-000000000001"
        );
        assert_eq!(
            workspace.to_string(),
            "017d203d-64f7-58e0-ab2a-191c6d09396d"
        );
        assert_eq!(
            identity.workspace_branch(),
            "agent-review-017d203d64f758e0ab2a191c6d09396d"
        );
        assert_eq!(
            identity.artifact_id().to_string(),
            "ac5ea4a1-ddf9-5d7b-809a-1924d9ce130a"
        );
        assert_eq!(
            identity.base_proof_id().to_string(),
            "216dfb4b-6842-5fd9-b8e6-ecc793fed767"
        );
        assert_eq!(
            identity.base_branch(),
            "g5-agent-review-base-00000000000000000000000000000001"
        );
        assert_ne!(workspace, identity.artifact_id());
        assert_ne!(workspace, identity.base_proof_id());
        let repo = Uuid::from_u128(2);
        let resolution = identity.resolution_id();
        assert_eq!(
            resolution,
            "apr_".to_string() + "7c3ccd10bb7ec37b46d37926ae6274267f007a34aeaf15c882a715a7f3300529"
        );
        assert_eq!(
            identity.output_doc_id().to_string(),
            "a2e3c204-590c-558a-a517-c32fb77e83f6"
        );
        assert_eq!(identity.output_doc_path(Uuid::nil()), "docs/00000000-0000-0000-0000-000000000000/reviews/agent-approval-00000000-0000-0000-0000-000000000001.md");
        for (side, suffix) in [
            (ApprovalEvidenceSide::Source, "source"),
            (ApprovalEvidenceSide::Base, "base"),
            (ApprovalEvidenceSide::Parent, "parent"),
        ] {
            assert_eq!(
                approval_evidence_ref(&resolution, repo, side).unwrap(),
                format!("refs/g5/approval-evidence/{resolution}/{repo}/{suffix}")
            );
            assert!(approval_evidence_ref("../../escape", repo, side).is_err());
        }
        assert_eq!(
            formal_review_runtime_scope(workspace),
            format!("formal-review:{workspace}")
        );
        assert_eq!(
            formal_review_read_scope(workspace),
            format!("formal-review-read:{workspace}")
        );
    }
}
