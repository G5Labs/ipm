//! Current approval request identity, admission and human settlement contracts.
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

/// Compatibility vocabulary for workspace-wide approval requests.
pub const WORKSPACE_APPROVAL_COMPAT_PHASE: &str = "ie";

pub const APPROVAL_REQUEST_SETTLEMENT_SCHEMA_VERSION: u8 = 2;
pub const APPROVAL_REQUEST_KEY_SCHEMA_VERSION: u8 = 2;
pub const APPROVAL_REVIEW_REQUEST_CONTEXT_SCHEMA_VERSION_V3: u8 = 3;
pub const APPROVAL_REVIEW_CONTEXT_MAX_JSON_BYTES: usize = 256 * 1024;

pub const APPROVAL_PHASES: [&str; 3] = ["be", "se", "ie"];

#[must_use]
pub fn is_approval_phase(value: &str) -> bool {
    APPROVAL_PHASES.contains(&value)
}

/// Tolerant decoder for evidence recorded under the stage model (migration
/// 067 maps `planning` to `be`; `finalizing` and the absent default were the
/// ship gate, now `ie`). History reads only; new submissions name a phase.
#[must_use]
pub fn approval_phase_from_stage(stage: Option<&str>) -> &'static str {
    match stage {
        Some("planning") => "be",
        _ => "ie",
    }
}

fn validate_phase(phase: &str, field: &str) -> Result<(), String> {
    if is_approval_phase(phase) {
        Ok(())
    } else {
        Err(format!("{field} must be \"be\", \"se\", or \"ie\""))
    }
}

/// Canonical, unhashed request identity. Callers never provide this value;
/// services fill every field from tenant, source, definition, phase and
/// actor authority before serializing and hashing it. One request chain per
/// (definition version, phase, actor slot): the phase is part of the key so
/// a BE request never converges on an IE one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ApprovalRequestKeyV2 {
    pub schema_version: u8,
    pub org_id: Uuid,
    pub source_workspace_id: Uuid,
    pub approval_definition_id: Uuid,
    pub approval_definition_version: i32,
    pub phase: String,
    pub actor_slot: String,
}

impl ApprovalRequestKeyV2 {
    pub fn validate(&self) -> Result<(), String> {
        if self.schema_version != APPROVAL_REQUEST_KEY_SCHEMA_VERSION {
            return Err("approval request key schemaVersion must be 2".to_string());
        }
        if self.approval_definition_version < 1 {
            return Err("approvalDefinitionVersion must be positive".to_string());
        }
        validate_phase(&self.phase, "phase")?;
        let human_actor = self
            .actor_slot
            .strip_prefix("human:")
            .is_some_and(|id| Uuid::parse_str(id).is_ok());
        let agent_actor = self.actor_slot == "agent";
        if !human_actor && !agent_actor {
            return Err("actorSlot must be agent or human:<uuid>".to_string());
        }
        Ok(())
    }

    /// The canonical hex SHA-256 request key.
    pub fn digest(&self) -> Result<String, String> {
        self.validate()?;
        let canonical = serde_json::to_vec(self)
            .map_err(|error| format!("failed to serialize approval request key: {error}"))?;
        Ok(Sha256::digest(canonical)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect())
    }
}

/// Admission-time identity for a v3 approval review. Mutable workspace and
/// Git authority is intentionally absent; the API resolves it atomically when
/// the first bootstrap assignment is claimed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ApprovalReviewRequestContextV3 {
    pub schema_version: u8,
    pub approval_request_id: Uuid,
    pub request_key: String,
    pub definition_id: Uuid,
    pub definition_version: i32,
    /// Phase this review settles (`be` | `se` | `ie`, g5.workbench.gates).
    /// Every new admission writes it; it is absent only on contexts admitted
    /// under the stage model, which carry `stage` instead. Read through
    /// [`Self::settled_phase`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub phase: Option<String>,
    /// Historical stage-model binding (`planning` | `finalizing`, or absent
    /// meaning finalizing) of contexts admitted before migration 067. Never
    /// written anew; a document carries `stage` or `phase`, not both.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stage: Option<String>,
}

impl ApprovalReviewRequestContextV3 {
    pub const MAX_JSON_BYTES: usize = APPROVAL_REVIEW_CONTEXT_MAX_JSON_BYTES;

    /// The phase this review settles: the recorded `phase`, else the
    /// stage-model mapping (`planning` → `be`, otherwise `ie`).
    #[must_use]
    pub fn settled_phase(&self) -> &str {
        self.phase
            .as_deref()
            .unwrap_or_else(|| approval_phase_from_stage(self.stage.as_deref()))
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.schema_version != APPROVAL_REVIEW_REQUEST_CONTEXT_SCHEMA_VERSION_V3 {
            return Err("schemaVersion must be 3".to_string());
        }
        if self.definition_version < 1 {
            return Err("definitionVersion must be positive".to_string());
        }
        validate_lower_hex(&self.request_key, &[64], "requestKey")?;
        if self.phase.is_some() && self.stage.is_some() {
            return Err("a context carries phase or stage, not both".to_string());
        }
        if let Some(phase) = self.phase.as_deref() {
            validate_phase(phase, "phase")?;
        }
        if self
            .stage
            .as_deref()
            .is_some_and(|stage| !matches!(stage, "planning" | "finalizing"))
        {
            return Err("stage must be omitted, \"planning\", or \"finalizing\"".to_string());
        }
        validate_json_size(
            self,
            APPROVAL_REVIEW_CONTEXT_MAX_JSON_BYTES,
            "approval review request context",
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ApprovalRequestSettlementV2 {
    pub schema_version: u8,
    pub request_id: Uuid,
    pub org_id: Uuid,
    pub source_workspace_id: Uuid,
    pub approval_definition_id: Uuid,
    pub approval_definition_version: i32,
    pub phase: String,
    pub actor_slot: String,
    pub outcome: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub comment: Option<String>,
}

impl ApprovalRequestSettlementV2 {
    pub fn validate(&self) -> Result<(), String> {
        if self.schema_version != APPROVAL_REQUEST_SETTLEMENT_SCHEMA_VERSION {
            return Err("approval request settlement schemaVersion must be 2".to_string());
        }
        if self.approval_definition_version < 1 {
            return Err("approvalDefinitionVersion must be positive".to_string());
        }
        validate_phase(&self.phase, "phase")?;
        let human_actor = self
            .actor_slot
            .strip_prefix("human:")
            .is_some_and(|id| Uuid::parse_str(id).is_ok());
        if !human_actor {
            return Err("settlements require a human actor".to_string());
        }
        if !matches!(self.outcome.as_str(), "approved" | "rejected" | "deferred") {
            return Err("outcome must be approved, rejected, or deferred".to_string());
        }
        if self
            .comment
            .as_deref()
            .is_some_and(|comment| comment.is_empty() || comment.trim() != comment)
        {
            return Err("comment must be normalized and nonblank when present".to_string());
        }
        Ok(())
    }

    pub fn fingerprint(&self) -> Result<String, String> {
        self.validate()?;
        let canonical = serde_json::to_vec(self)
            .map_err(|error| format!("failed to serialize settlement command: {error}"))?;
        Ok(Sha256::digest(canonical)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect())
    }
}

fn validate_json_size<T: Serialize>(value: &T, maximum: usize, field: &str) -> Result<(), String> {
    let encoded =
        serde_json::to_vec(value).map_err(|error| format!("failed to encode {field}: {error}"))?;
    if encoded.len() > maximum {
        return Err(format!("{field} must be at most {maximum} bytes"));
    }
    Ok(())
}

fn validate_lower_hex(value: &str, lengths: &[usize], field: &str) -> Result<(), String> {
    if !lengths.contains(&value.len())
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(format!("{field} must be lowercase hexadecimal"));
    }
    Ok(())
}
