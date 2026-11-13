use crate::types::{
    AgentPermissionPolicyV1, ProviderId, QuestionPolicy, SandboxPolicy, ToolApproval, POLICY_SCHEMA,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedPolicy {
    pub permission_mode: &'static str,
    pub permission_prompts: &'static str,
    pub permission_prompt_tool: Option<&'static str>,
    pub questions_enabled: bool,
    pub sandbox: Option<&'static str>,
    pub allowed_tools: Vec<String>,
    pub native_response_timeout_ms: u64,
}

pub fn resolve_policy(policy: &AgentPermissionPolicyV1) -> Result<ResolvedPolicy, String> {
    if policy.schema_version != POLICY_SCHEMA
        || policy.limits.setup_timeout_ms == 0
        || policy.limits.max_turns == Some(0)
    {
        return Err("INVALID_PERMISSION_POLICY".into());
    }
    let native_response_timeout_ms = policy.limits.native_response_timeout_ms.unwrap_or(900_000);
    if native_response_timeout_ms == 0 || native_response_timeout_ms > 900_000 {
        return Err("INVALID_NATIVE_RESPONSE_TIMEOUT".into());
    }
    g5_contracts::types::validate_agent_tool_selection(
        match policy.provider {
            ProviderId::ClaudeCode => g5_contracts::types::ChatAgentProvider::ClaudeCode,
            ProviderId::Codex => g5_contracts::types::ChatAgentProvider::Codex,
        },
        policy.questions == QuestionPolicy::Enabled,
        &policy.allowed_tools,
    )
    .map_err(str::to_owned)?;

    let resolved = match (
        policy.provider,
        policy.questions,
        policy.tool_approval,
        policy.sandbox,
    ) {
        (ProviderId::ClaudeCode, questions, ToolApproval::Prompt, SandboxPolicy::None) => {
            ResolvedPolicy {
                permission_mode: "default",
                permission_prompts: "host",
                permission_prompt_tool: Some("stdio"),
                questions_enabled: questions == QuestionPolicy::Enabled,
                sandbox: None,
                allowed_tools: policy.allowed_tools.clone(),
                native_response_timeout_ms,
            }
        }
        (
            ProviderId::ClaudeCode,
            QuestionPolicy::Disabled,
            ToolApproval::AutoDeny,
            SandboxPolicy::None,
        ) => ResolvedPolicy {
            permission_mode: "dontAsk",
            permission_prompts: "none",
            permission_prompt_tool: None,
            questions_enabled: false,
            sandbox: None,
            allowed_tools: policy.allowed_tools.clone(),
            native_response_timeout_ms,
        },
        (ProviderId::ClaudeCode, questions, ToolApproval::AutoAllow, SandboxPolicy::None) => {
            ResolvedPolicy {
                permission_mode: "bypassPermissions",
                permission_prompts: if questions == QuestionPolicy::Enabled {
                    "host"
                } else {
                    "none"
                },
                permission_prompt_tool: (questions == QuestionPolicy::Enabled).then_some("stdio"),
                questions_enabled: questions == QuestionPolicy::Enabled,
                sandbox: None,
                allowed_tools: policy.allowed_tools.clone(),
                native_response_timeout_ms,
            }
        }
        (
            ProviderId::Codex,
            QuestionPolicy::Disabled,
            ToolApproval::AutoDeny,
            sandbox @ (SandboxPolicy::ReadOnly
            | SandboxPolicy::WorkspaceWrite
            | SandboxPolicy::DangerFullAccess),
        ) => ResolvedPolicy {
            permission_mode: "never",
            permission_prompts: "none",
            permission_prompt_tool: None,
            questions_enabled: false,
            sandbox: Some(match sandbox {
                SandboxPolicy::ReadOnly => "read-only",
                SandboxPolicy::WorkspaceWrite => "workspace-write",
                SandboxPolicy::DangerFullAccess => "danger-full-access",
                SandboxPolicy::None => unreachable!(),
            }),
            allowed_tools: policy.allowed_tools.clone(),
            native_response_timeout_ms,
        },
        _ => return Err("UNSUPPORTED_PERMISSION_POLICY".into()),
    };
    Ok(resolved)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::PermissionLimitsV1;

    fn policy(
        provider: ProviderId,
        questions: QuestionPolicy,
        approval: ToolApproval,
        sandbox: SandboxPolicy,
    ) -> AgentPermissionPolicyV1 {
        AgentPermissionPolicyV1 {
            schema_version: POLICY_SCHEMA.into(),
            provider,
            questions,
            tool_approval: approval,
            sandbox,
            allowed_tools: vec![],
            limits: PermissionLimitsV1 {
                setup_timeout_ms: 10_000,
                max_turns: None,
                max_output_bytes: Some(1024),
                native_response_timeout_ms: None,
            },
        }
    }

    #[test]
    fn maps_only_the_documented_policy_rows() {
        assert_eq!(
            resolve_policy(&policy(
                ProviderId::ClaudeCode,
                QuestionPolicy::Enabled,
                ToolApproval::Prompt,
                SandboxPolicy::None
            ))
            .unwrap()
            .permission_prompt_tool,
            Some("stdio")
        );
        assert_eq!(
            resolve_policy(&policy(
                ProviderId::ClaudeCode,
                QuestionPolicy::Disabled,
                ToolApproval::AutoAllow,
                SandboxPolicy::None
            ))
            .unwrap()
            .permission_mode,
            "bypassPermissions"
        );
        assert_eq!(
            resolve_policy(&policy(
                ProviderId::Codex,
                QuestionPolicy::Disabled,
                ToolApproval::AutoDeny,
                SandboxPolicy::WorkspaceWrite
            ))
            .unwrap()
            .sandbox,
            Some("workspace-write")
        );
    }

    #[test]
    fn tool_selection_narrows_availability_without_changing_approval() {
        for (approval, mode) in [
            (ToolApproval::Prompt, "default"),
            (ToolApproval::AutoDeny, "dontAsk"),
            (ToolApproval::AutoAllow, "bypassPermissions"),
        ] {
            let mut selected = policy(
                ProviderId::ClaudeCode,
                QuestionPolicy::Disabled,
                approval,
                SandboxPolicy::None,
            );
            selected.allowed_tools = vec!["Read".into()];
            let resolved = resolve_policy(&selected).unwrap();
            assert_eq!(resolved.permission_mode, mode);
            assert_eq!(resolved.allowed_tools, ["Read"]);
            for unsupported in ["UnknownTool", "default", "Bash(*)", "mcp__server__tool"] {
                selected.allowed_tools = vec![unsupported.into()];
                assert_eq!(
                    resolve_policy(&selected),
                    Err("UNSUPPORTED_ALLOWED_TOOL".into())
                );
            }
        }
    }

    #[test]
    fn refuses_retired_and_unrepresentable_mcp_tool_selections() {
        let mut selected = policy(
            ProviderId::ClaudeCode,
            QuestionPolicy::Disabled,
            ToolApproval::AutoAllow,
            SandboxPolicy::None,
        );
        for removed in [
            "mcp__g5-tools__read_ontology",
            "mcp__g5-tools__browser_click",
            "mcp__g5-tools__list_review_round",
            "mcp__g5-tools__decide_review_suggestion",
        ] {
            selected.allowed_tools = vec!["Read".into(), removed.into()];
            assert_eq!(
                resolve_policy(&selected),
                Err("INVALID_ALLOWED_TOOL".into())
            );
        }
        // MCP tools remain available through the configured server when no
        // selection is requested. Native --tools cannot select them exactly.
        selected.allowed_tools = vec!["mcp__docs__search".into()];
        assert_eq!(
            resolve_policy(&selected),
            Err("UNSUPPORTED_ALLOWED_TOOL".into())
        );
    }

    #[test]
    fn explicit_selection_cannot_silently_disable_enabled_questions() {
        let mut selected = policy(
            ProviderId::ClaudeCode,
            QuestionPolicy::Enabled,
            ToolApproval::Prompt,
            SandboxPolicy::None,
        );
        selected.allowed_tools = vec!["Read".into()];
        assert_eq!(
            resolve_policy(&selected),
            Err("QUESTION_TOOL_EXCLUDED".into())
        );
        selected.allowed_tools.push("AskUserQuestion".into());
        assert!(resolve_policy(&selected).unwrap().questions_enabled);
        selected.allowed_tools.clear();
        assert!(resolve_policy(&selected).unwrap().questions_enabled);
        selected.questions = QuestionPolicy::Disabled;
        selected.allowed_tools = vec!["Read".into()];
        assert!(!resolve_policy(&selected).unwrap().questions_enabled);
    }

    #[test]
    fn refuses_interactive_codex_and_implicit_claude_sandbox() {
        assert_eq!(
            resolve_policy(&policy(
                ProviderId::Codex,
                QuestionPolicy::Enabled,
                ToolApproval::Prompt,
                SandboxPolicy::WorkspaceWrite
            )),
            Err("UNSUPPORTED_PERMISSION_POLICY".into())
        );
        assert_eq!(
            resolve_policy(&policy(
                ProviderId::ClaudeCode,
                QuestionPolicy::Disabled,
                ToolApproval::AutoDeny,
                SandboxPolicy::WorkspaceWrite
            )),
            Err("UNSUPPORTED_PERMISSION_POLICY".into())
        );
    }

    #[test]
    fn bypass_questions_keep_native_input_without_changing_tool_approval() {
        for questions in [QuestionPolicy::Enabled, QuestionPolicy::Disabled] {
            let resolved = resolve_policy(&policy(
                ProviderId::ClaudeCode,
                questions,
                ToolApproval::AutoAllow,
                SandboxPolicy::None,
            ))
            .unwrap();
            assert_eq!(resolved.permission_mode, "bypassPermissions");
            let enabled = questions == QuestionPolicy::Enabled;
            assert_eq!(resolved.questions_enabled, enabled);
            assert_eq!(
                resolved.permission_prompts,
                if enabled { "host" } else { "none" }
            );
            assert_eq!(resolved.permission_prompt_tool, enabled.then_some("stdio"));
        }
    }
}
