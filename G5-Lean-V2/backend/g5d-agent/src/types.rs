use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const INPUT_SCHEMA: &str = "g5.agent.turn.input/v1";
pub const POLICY_SCHEMA: &str = "g5.agent.permission-policy/v1";
pub const OUTCOME_SCHEMA: &str = "g5.agent.turn.outcome/v1";
pub const MAX_PROMPT_BYTES: usize = 512 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct AgentTurnInputV1 {
    pub schema_version: String,
    pub task: AgentTurnTaskV1,
    pub provider: AgentTurnProviderV1,
    pub prompt: FrozenPromptV1,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resume_prompt: Option<FrozenPromptV1>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub native_question_reply: Option<NativeQuestionReplyV1>,
    #[serde(default)]
    pub attachments: Vec<TurnAttachmentV1>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attachment_preflight_error: Option<String>,
    /// The API withheld part of the configured secrets from this invoke
    /// because they no longer pass admission; a fresh start fails before the
    /// provider runs rather than running with a reduced set
    /// (`g5.chat.secret-admission`).
    #[serde(default)]
    pub secret_preflight_error: Option<String>,
    pub workspace_target: WorkspaceTargetV1,
    pub session_binding: SessionBindingV1,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recovery: Option<RecoveryContextV1>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub continuation: Option<ContinuationV1>,
    pub limits: AgentTurnLimitsV1,
}

/// Frozen answer to a deferred native tool. The source attempt is provenance;
/// only the current attempt's restored native callback can receive the answer.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct NativeQuestionReplyV1 {
    pub source_attempt: u32,
    pub provider_request_id: String,
    pub provider_session_id: String,
    pub request_id: String,
    pub response_digest: String,
    pub validated_response: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RecoveryContextV1 {
    pub included_messages: u32,
    pub omitted_messages: u64,
    pub truncated_messages: u32,
    pub attachment_references: u32,
    #[serde(default)]
    pub omitted_attachment_references: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unavailable_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct AgentTurnTaskV1 {
    pub task_id: String,
    pub attempt: u32,
    pub session_id: String,
    pub input_origin: InputOrigin,
    pub initial_input: AgentSessionInputReservationV1,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct AgentSessionInputReservationV1 {
    pub reservation_id: String,
    pub input_id: String,
    pub revision: u32,
    pub delivery_generation: u32,
    pub input_origin: InputOrigin,
    #[serde(default)]
    pub system_notice_ids: Vec<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum InputOrigin {
    Human,
    Workflow,
    Continuation,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct AgentTurnProviderV1 {
    pub provider_id: ProviderId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth_mode: Option<g5_contracts::types::AgentAuthModeV1>,
    /// Selected account/configuration endpoint, carried with this invocation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,
    pub model_id: String,
    pub policy_ref: String,
    /// The immutable policy document named by `policy_ref`. P4.6 persists the
    /// same document on the attempt; carrying it here makes launch validation
    /// local and leaves no mutable policy lookup in the VM.
    pub policy_snapshot: AgentPermissionPolicyV1,
    #[serde(default)]
    pub options: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProviderId {
    ClaudeCode,
    Codex,
}

impl ProviderId {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ClaudeCode => "claude_code",
            Self::Codex => "codex",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct FrozenPromptV1 {
    pub text: String,
    pub sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TurnAttachmentKindV1 {
    Image,
    File,
    Reference,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct TurnAttachmentV1 {
    pub kind: TurnAttachmentKindV1,
    pub id: String,
    pub input_id: String,
    pub file_name: String,
    pub content_type: Option<String>,
    pub size_bytes: i64,
    pub sha256: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct WorkspaceTargetV1 {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repo_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub relative_path: Option<String>,
    /// The launch is in a personal workspace (`g5.personal.project-docs`),
    /// whose harness adds the personal-repository guidance.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub personal: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SessionBindingV1 {
    pub session_files_state: SessionFilesState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_session_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_manifest_generation: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub restore_ref: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SessionFilesState {
    Present,
    Absent,
    PersistFailed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ContinuationV1 {
    pub prior_exec_id: String,
    pub reason: ContinuationReason,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ContinuationReason {
    Interrupted,
    VmLost,
    Suspended,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct AgentTurnLimitsV1 {
    pub setup_timeout_ms: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_turns: Option<u32>,
    /// Retired: a turn has no cumulative output limit
    /// (`g5.agents.output-bounds`). The API still sends it for workers that
    /// require it, so it is accepted, never read, and written back as sent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_output_bytes: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct AgentPermissionPolicyV1 {
    pub schema_version: String,
    pub provider: ProviderId,
    pub questions: QuestionPolicy,
    pub tool_approval: ToolApproval,
    pub sandbox: SandboxPolicy,
    #[serde(default)]
    pub allowed_tools: Vec<String>,
    pub limits: PermissionLimitsV1,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum QuestionPolicy {
    Enabled,
    Disabled,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ToolApproval {
    Prompt,
    AutoAllow,
    AutoDeny,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum SandboxPolicy {
    None,
    ReadOnly,
    WorkspaceWrite,
    DangerFullAccess,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PermissionLimitsV1 {
    pub setup_timeout_ms: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_turns: Option<u32>,
    /// Retired like [`AgentTurnLimitsV1::max_output_bytes`]: accepted and
    /// never read, so a frozen policy snapshot serializes as it was frozen.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_output_bytes: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub native_response_timeout_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentTurnOutcomeV1 {
    pub schema_version: String,
    pub status: AgentTurnStatus,
    pub code: String,
    pub retryable: bool,
    #[serde(default)]
    pub input_delivery_unknown: bool,
    /// Positive evidence that the provider process crossed the spawn boundary.
    /// Missing on older outcomes, which must never authorize fence restoration.
    #[serde(default)]
    pub provider_started: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub final_message_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub approval_artifact: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider_session_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_manifest_generation: Option<u32>,
    pub session_files_state: SessionFilesState,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<crate::runner::TurnUsage>,
    pub last_event_seq: u64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentTurnStatus {
    Succeeded,
    Failed,
    LimitExceeded,
    Cancelled,
    Interrupted,
    TimedOut,
    Suspended,
}

impl AgentTurnInputV1 {
    pub fn validate(&self) -> Result<(), String> {
        if self.schema_version != INPUT_SCHEMA {
            return Err("UNSUPPORTED_INPUT_SCHEMA".into());
        }
        if self.attachment_preflight_error.is_some() {
            return Err("INVALID_ATTACHMENT_DESCRIPTOR".into());
        }
        if self.secret_preflight_error.is_some() {
            return Err("INVALID_DISPATCH_SECRETS".into());
        }
        if self.task.attempt == 0
            || self.task.task_id.trim().is_empty()
            || self.task.session_id.trim().is_empty()
            || self.task.initial_input.reservation_id.trim().is_empty()
            || self.task.initial_input.input_id.trim().is_empty()
            || self.task.initial_input.revision == 0
            || self.task.initial_input.delivery_generation == 0
        {
            return Err("INVALID_INPUT_RESERVATION".into());
        }
        if self.task.input_origin != self.task.initial_input.input_origin {
            return Err("INPUT_ORIGIN_MISMATCH".into());
        }
        if self.provider.model_id.trim().is_empty() || self.provider.policy_ref.trim().is_empty() {
            return Err("INVALID_PROVIDER_BINDING".into());
        }
        if self.provider.provider_id != self.provider.policy_snapshot.provider {
            return Err("POLICY_PROVIDER_MISMATCH".into());
        }
        if let Some(answer) = &self.native_question_reply {
            if self.provider.provider_id != ProviderId::ClaudeCode
                || answer.source_attempt == 0
                || answer.source_attempt >= self.task.attempt
                || self.session_binding.provider_session_id.as_deref()
                    != Some(answer.provider_session_id.as_str())
                || answer.provider_request_id.is_empty()
                || answer.provider_request_id.len() > 512
                || answer.request_id.is_empty()
                || answer.request_id.len() > 128
                || self.continuation.is_none()
                || format!(
                    "{:x}",
                    Sha256::digest(
                        serde_json::to_vec(&answer.validated_response)
                            .map_err(|_| "INVALID_NATIVE_ANSWER")?
                    )
                ) != answer.response_digest
            {
                return Err("INVALID_NATIVE_ANSWER_BINDING".into());
            }
        }
        if self.provider.auth_mode
            == Some(g5_contracts::types::AgentAuthModeV1::ChatGptSubscription)
            && self.provider.provider_id != ProviderId::Codex
        {
            return Err("AUTH_PROVIDER_MISMATCH".into());
        }
        if self.prompt.text.len() > MAX_PROMPT_BYTES {
            return Err("PROMPT_TOO_LARGE".into());
        }
        let digest = format!("{:x}", Sha256::digest(self.prompt.text.as_bytes()));
        if digest != self.prompt.sha256.to_ascii_lowercase() {
            return Err("PROMPT_DIGEST_MISMATCH".into());
        }
        if let Some(prompt) = &self.resume_prompt {
            if prompt.text.len() > MAX_PROMPT_BYTES
                || format!("{:x}", Sha256::digest(prompt.text.as_bytes()))
                    != prompt.sha256.to_ascii_lowercase()
            {
                return Err("RESUME_PROMPT_INVALID".into());
            }
        }
        let total_bytes = self
            .attachments
            .iter()
            .try_fold(0i64, |total, item| total.checked_add(item.size_bytes));
        if self.attachments.len() > 8 || total_bytes.is_none_or(|total| total > 10 * 1024 * 1024) {
            return Err("TOO_MANY_ATTACHMENTS".into());
        }
        let mut ids = std::collections::HashSet::new();
        for attachment in &self.attachments {
            if attachment.input_id != self.task.initial_input.input_id
                || uuid::Uuid::parse_str(&attachment.id).is_err()
                || !ids.insert(&attachment.id)
                || attachment.size_bytes <= 0
                || attachment.sha256.len() != 64
                || !attachment
                    .sha256
                    .bytes()
                    .all(|byte| byte.is_ascii_hexdigit())
            {
                return Err("INVALID_ATTACHMENT_DESCRIPTOR".into());
            }
            if attachment.kind != TurnAttachmentKindV1::Image
                || !attachment
                    .content_type
                    .as_deref()
                    .is_some_and(|content_type| {
                        ["image/png", "image/jpeg", "image/gif", "image/webp"]
                            .iter()
                            .any(|allowed| content_type.eq_ignore_ascii_case(allowed))
                    })
            {
                return Err("UNSUPPORTED_ATTACHMENT_CONTENT".into());
            }
        }
        if self.limits.setup_timeout_ms == 0
            || std::time::Instant::now()
                .checked_add(std::time::Duration::from_millis(
                    self.limits.setup_timeout_ms,
                ))
                .is_none()
            || self.limits.max_turns == Some(0)
        {
            return Err("INVALID_LIMITS".into());
        }
        if self.provider.provider_id == ProviderId::Codex
            && (self.limits.max_turns.is_some()
                || self.provider.policy_snapshot.limits.max_turns.is_some())
        {
            return Err("UNSUPPORTED_LIMIT_MAX_TURNS".into());
        }
        if self.limits.setup_timeout_ms > self.provider.policy_snapshot.limits.setup_timeout_ms
            || match (
                self.limits.max_turns,
                self.provider.policy_snapshot.limits.max_turns,
            ) {
                (Some(actual), Some(granted)) => actual > granted,
                (Some(_), None) => true,
                _ => false,
            }
        {
            return Err("LIMIT_EXCEEDS_POLICY".into());
        }
        if let Some(path) = self.workspace_target.relative_path.as_deref() {
            let path = std::path::Path::new(path);
            if path.is_absolute()
                || path.components().any(|part| {
                    matches!(
                        part,
                        std::path::Component::ParentDir
                            | std::path::Component::RootDir
                            | std::path::Component::Prefix(_)
                    )
                })
            {
                return Err("INVALID_WORKSPACE_TARGET".into());
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input() -> AgentTurnInputV1 {
        let text = "hello".to_string();
        AgentTurnInputV1 {
            recovery: None,
            schema_version: INPUT_SCHEMA.into(),
            task: AgentTurnTaskV1 {
                task_id: "task-1".into(),
                attempt: 1,
                session_id: "session-1".into(),
                input_origin: InputOrigin::Human,
                initial_input: AgentSessionInputReservationV1 {
                    reservation_id: "reservation-1".into(),
                    input_id: "input-1".into(),
                    revision: 1,
                    delivery_generation: 1,
                    input_origin: InputOrigin::Human,
                    system_notice_ids: vec![],
                },
            },
            provider: AgentTurnProviderV1 {
                provider_id: ProviderId::ClaudeCode,
                auth_mode: None,
                base_url: None,
                model_id: "claude-test".into(),
                policy_ref: "policy:1".into(),
                policy_snapshot: AgentPermissionPolicyV1 {
                    schema_version: POLICY_SCHEMA.into(),
                    provider: ProviderId::ClaudeCode,
                    questions: QuestionPolicy::Disabled,
                    tool_approval: ToolApproval::AutoDeny,
                    sandbox: SandboxPolicy::None,
                    allowed_tools: vec![],
                    limits: PermissionLimitsV1 {
                        setup_timeout_ms: 60_000,
                        max_turns: Some(5),
                        max_output_bytes: Some(1024),
                        native_response_timeout_ms: None,
                    },
                },
                options: BTreeMap::new(),
            },
            prompt: FrozenPromptV1 {
                sha256: format!("{:x}", Sha256::digest(text.as_bytes())),
                text,
            },
            resume_prompt: None,
            native_question_reply: None,
            attachments: vec![],
            attachment_preflight_error: None,
            secret_preflight_error: None,
            workspace_target: WorkspaceTargetV1::default(),
            session_binding: SessionBindingV1 {
                session_files_state: SessionFilesState::Absent,
                provider_session_id: None,
                expected_manifest_generation: None,
                restore_ref: None,
            },
            continuation: None,
            limits: AgentTurnLimitsV1 {
                setup_timeout_ms: 60_000,
                max_turns: Some(5),
                max_output_bytes: Some(1024),
            },
        }
    }

    #[test]
    fn validates_exact_prompt_and_reservation() {
        assert_eq!(input().validate(), Ok(()));
        let mut changed = input();
        changed.prompt.text.push('!');
        assert_eq!(changed.validate(), Err("PROMPT_DIGEST_MISMATCH".into()));
    }

    #[test]
    fn rejects_origin_and_policy_confusion() {
        let mut changed = input();
        changed.task.input_origin = InputOrigin::Workflow;
        assert_eq!(changed.validate(), Err("INPUT_ORIGIN_MISMATCH".into()));
        let mut changed = input();
        changed.provider.policy_snapshot.provider = ProviderId::Codex;
        assert_eq!(changed.validate(), Err("POLICY_PROVIDER_MISMATCH".into()));
    }

    #[test]
    fn rejects_unenforceable_turn_limits_before_delivery() {
        let mut zero = input();
        zero.limits.max_turns = Some(0);
        assert_eq!(zero.validate(), Err("INVALID_LIMITS".into()));

        let mut codex = input();
        codex.provider.provider_id = ProviderId::Codex;
        codex.provider.policy_snapshot.provider = ProviderId::Codex;
        assert_eq!(codex.validate(), Err("UNSUPPORTED_LIMIT_MAX_TURNS".into()));
        codex.limits.max_turns = None;
        assert_eq!(codex.validate(), Err("UNSUPPORTED_LIMIT_MAX_TURNS".into()));
        codex.provider.policy_snapshot.limits.max_turns = None;
        assert_eq!(codex.validate(), Ok(()));
    }

    /// The retired output limit is wire compatibility only
    /// (`g5.agents.output-bounds`): present with any value, including one
    /// above the policy's or zero, or absent, it never refuses a turn, and a
    /// frozen policy snapshot serializes exactly as it was sent.
    #[test]
    fn the_retired_output_limit_is_accepted_ignored_and_round_trips() {
        for (request, policy) in [
            (Some(64 * 1024 * 1024), Some(64 * 1024 * 1024)),
            (Some(2048), Some(1024)),
            (Some(0), Some(0)),
            (None, Some(1024)),
            (Some(1024), None),
            (None, None),
        ] {
            let mut turn = input();
            turn.limits.max_output_bytes = request;
            turn.provider.policy_snapshot.limits.max_output_bytes = policy;
            assert_eq!(turn.validate(), Ok(()), "{request:?} {policy:?}");
            assert!(
                crate::policy::resolve_policy(&turn.provider.policy_snapshot).is_ok(),
                "{request:?} {policy:?}"
            );
        }

        let with = serde_json::json!({
            "schema_version": POLICY_SCHEMA,
            "provider": "claude_code",
            "questions": "disabled",
            "tool_approval": "auto_deny",
            "sandbox": "none",
            "allowed_tools": [],
            "limits": {
                "setup_timeout_ms": 300_000,
                "max_output_bytes": 67_108_864u64,
                "native_response_timeout_ms": 900_000
            }
        });
        let mut without = with.clone();
        without["limits"]
            .as_object_mut()
            .unwrap()
            .remove("max_output_bytes");
        for snapshot in [with, without] {
            let parsed: AgentPermissionPolicyV1 = serde_json::from_value(snapshot.clone()).unwrap();
            assert_eq!(serde_json::to_value(&parsed).unwrap(), snapshot);
        }
        let limits: AgentTurnLimitsV1 =
            serde_json::from_str(r#"{"setup_timeout_ms":300000,"max_output_bytes":67108864}"#)
                .unwrap();
        assert_eq!(
            serde_json::to_string(&limits).unwrap(),
            r#"{"setup_timeout_ms":300000,"max_output_bytes":67108864}"#
        );
        let limits: AgentTurnLimitsV1 =
            serde_json::from_str(r#"{"setup_timeout_ms":300000}"#).unwrap();
        assert_eq!(
            serde_json::to_string(&limits).unwrap(),
            r#"{"setup_timeout_ms":300000}"#
        );
        // Any other unknown limit is still refused.
        assert!(serde_json::from_str::<AgentTurnLimitsV1>(
            r#"{"setup_timeout_ms":300000,"max_spend_usd":1}"#
        )
        .is_err());
    }

    #[test]
    fn refuses_absolute_or_parent_workspace_targets() {
        for path in ["/etc", "../outside", "repo/../../outside"] {
            let mut changed = input();
            changed.workspace_target.relative_path = Some(path.into());
            assert_eq!(changed.validate(), Err("INVALID_WORKSPACE_TARGET".into()));
        }
    }

    #[test]
    fn refuses_unsupported_or_unlinked_attachment_before_launch() {
        let mut turn = input();
        turn.attachments.push(TurnAttachmentV1 {
            kind: TurnAttachmentKindV1::File,
            id: uuid::Uuid::new_v4().to_string(),
            input_id: turn.task.initial_input.input_id.clone(),
            file_name: "notes.pdf".into(),
            content_type: Some("application/pdf".into()),
            size_bytes: 3,
            sha256: "a".repeat(64),
        });
        assert_eq!(
            turn.validate(),
            Err("UNSUPPORTED_ATTACHMENT_CONTENT".into())
        );
        turn.attachments[0].kind = TurnAttachmentKindV1::Image;
        turn.attachments[0].content_type = Some("IMAGE/PNG".into());
        assert_eq!(turn.validate(), Ok(()));
        turn.attachments[0].input_id = "another-input".into();
        assert_eq!(turn.validate(), Err("INVALID_ATTACHMENT_DESCRIPTOR".into()));
        turn.attachments[0].input_id = turn.task.initial_input.input_id.clone();
        turn.attachments.push(turn.attachments[0].clone());
        assert_eq!(turn.validate(), Err("INVALID_ATTACHMENT_DESCRIPTOR".into()));
        let mut malformed = input();
        malformed.attachment_preflight_error = Some("invalid stored manifest".into());
        assert_eq!(
            malformed.validate(),
            Err("INVALID_ATTACHMENT_DESCRIPTOR".into())
        );
        let mut withheld = input();
        withheld.secret_preflight_error =
            Some("Runtime Config secret `bad` is not an environment name".into());
        assert_eq!(withheld.validate(), Err("INVALID_DISPATCH_SECRETS".into()));
    }
}
