//! Frozen formal-review invocation. This does not create another task kind.
use super::ApprovalReviewResolutionV2;
#[cfg(feature = "render")]
use super::{render_agent_approval_frontmatter_v2, AgentApprovalOutcome};
use serde::{Deserialize, Serialize};

pub const APPROVAL_EXECUTION_MANIFEST: &str =
    include_str!("../../agent-approval-contracts/v1.json");

pub const APPROVAL_EXECUTION_CONTRACT: &str = "g5.formal-agent-approval/v1";
pub const APPROVAL_SKILL: &str = include_str!("../prompts/formal_approval/v1/SKILL.md");
pub const APPROVAL_PROMPT_GUIDE: &str =
    include_str!("../prompts/formal_approval/v1/agent-prompt.md");
pub const APPROVAL_SYSTEM_TEMPLATE: &str =
    include_str!("../prompts/formal_approval/v1/system.tera");
pub const APPROVAL_USER_TEMPLATE: &str = include_str!("../prompts/formal_approval/v1/user.tera");

/// One assignment schema's frozen runtime contract and prompt assets.
struct ContractAssets {
    contract_version: &'static str,
    skill: &'static str,
    prompt_guide: &'static str,
    system: &'static str,
    user: &'static str,
}

/// Schemas 1–5 keep their published assets for frozen retries; schema 5 adds
/// the agent-authored summary under `g5.formal-agent-approval/v4`, and
/// schema 6 the ordered report body under `g5.formal-agent-approval/v5`.
fn contract_assets(schema_version: u8) -> ContractAssets {
    match schema_version {
        6 => ContractAssets {
            contract_version: "g5.formal-agent-approval/v5",
            skill: include_str!("../prompts/formal_approval/v5/SKILL.md"),
            prompt_guide: include_str!("../prompts/formal_approval/v5/agent-prompt.md"),
            system: include_str!("../prompts/formal_approval/v5/system.tera"),
            user: include_str!("../prompts/formal_approval/v5/user.tera"),
        },
        5 => ContractAssets {
            contract_version: "g5.formal-agent-approval/v4",
            skill: include_str!("../prompts/formal_approval/v4/SKILL.md"),
            prompt_guide: include_str!("../prompts/formal_approval/v4/agent-prompt.md"),
            system: include_str!("../prompts/formal_approval/v4/system.tera"),
            user: include_str!("../prompts/formal_approval/v4/user.tera"),
        },
        4 => ContractAssets {
            contract_version: "g5.formal-agent-approval/v3",
            skill: include_str!("../prompts/formal_approval/v3/SKILL.md"),
            prompt_guide: include_str!("../prompts/formal_approval/v3/agent-prompt.md"),
            system: include_str!("../prompts/formal_approval/v3/system.tera"),
            user: include_str!("../prompts/formal_approval/v3/user.tera"),
        },
        3 => ContractAssets {
            contract_version: "g5.formal-agent-approval/v2",
            skill: include_str!("../prompts/formal_approval/v2/SKILL.md"),
            prompt_guide: include_str!("../prompts/formal_approval/v2/agent-prompt.md"),
            system: include_str!("../prompts/formal_approval/v2/system.tera"),
            user: include_str!("../prompts/formal_approval/v2/user.tera"),
        },
        _ => ContractAssets {
            contract_version: APPROVAL_EXECUTION_CONTRACT,
            skill: APPROVAL_SKILL,
            prompt_guide: APPROVAL_PROMPT_GUIDE,
            system: APPROVAL_SYSTEM_TEMPLATE,
            user: APPROVAL_USER_TEMPLATE,
        },
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FormalApprovalInvocation {
    pub contract_version: String,
    pub resolution: ApprovalReviewResolutionV2,
    pub system_prompt: String,
    pub user_prompt: String,
}

impl FormalApprovalInvocation {
    pub fn skill(&self) -> &'static str {
        contract_assets(self.resolution.schema_version).skill
    }
    pub fn prompt_guide(&self) -> &'static str {
        contract_assets(self.resolution.schema_version).prompt_guide
    }

    #[cfg(feature = "render")]
    pub fn render(
        resolution: ApprovalReviewResolutionV2,
        name: &str,
        agent_prompt: &str,
    ) -> Result<Self, String> {
        resolution.validate()?;
        let assets = contract_assets(resolution.schema_version);
        if agent_prompt.trim().is_empty()
            || [
                "-----BEGIN_UNTRUSTED_AGENT_PROMPT-----",
                "-----END_UNTRUSTED_AGENT_PROMPT-----",
            ]
            .iter()
            .any(|sentinel| agent_prompt.contains(sentinel))
        {
            return Err(
                "review policy must be nonblank and cannot contain a policy delimiter".into(),
            );
        }
        let mut engine = tera::Tera::default();
        engine
            .add_raw_template("review", assets.user)
            .map_err(|e| e.to_string())?;
        let mut context = tera::Context::new();
        for (key, value) in [
            ("approval_definition_name", name.to_string()),
            (
                "approval_definition_id",
                resolution.definition_id.to_string(),
            ),
            (
                "approval_definition_version",
                resolution.definition_version.to_string(),
            ),
            (
                "approval_source_workspace_id",
                resolution.source_workspace_id.to_string(),
            ),
            (
                "approval_target_workspace_id",
                resolution.target_workspace_id.to_string(),
            ),
            ("approval_resolution_id", resolution.resolution_id.clone()),
            (
                "approval_output_doc_path",
                resolution.output_doc_path.clone(),
            ),
            (
                "approval_current_heads_json",
                serde_json::to_string_pretty(&resolution.source_heads)
                    .map_err(|e| e.to_string())?,
            ),
            (
                "approval_base_heads_json",
                serde_json::to_string_pretty(resolution.comparison_heads())
                    .map_err(|e| e.to_string())?,
            ),
            (
                "approval_parent_heads_json",
                serde_json::to_string_pretty(&resolution.target_heads)
                    .map_err(|e| e.to_string())?,
            ),
            (
                "approval_repository_names_json",
                serde_json::to_string_pretty(
                    &resolution
                        .repository_names
                        .iter()
                        .map(|(id, name)| serde_json::json!({"repoId":id,"repoName":name}))
                        .collect::<Vec<_>>(),
                )
                .map_err(|e| e.to_string())?,
            ),
            ("approval_agent_prompt", agent_prompt.to_string()),
            (
                "approval_canonical_frontmatter_approved",
                render_agent_approval_frontmatter_v2(&resolution, AgentApprovalOutcome::Approved)?,
            ),
            (
                "approval_canonical_frontmatter_rejected",
                render_agent_approval_frontmatter_v2(&resolution, AgentApprovalOutcome::Rejected)?,
            ),
        ] {
            context.insert(key, &value);
        }
        context.insert(
            "approval_assignment_json",
            &serde_json::to_string_pretty(&resolution).map_err(|e| e.to_string())?,
        );
        context.insert("approval_output_schema_version", &resolution.schema_version);
        let user_prompt = engine
            .render("review", &context)
            .map_err(|e| e.to_string())?;
        Ok(Self {
            contract_version: assets.contract_version.into(),
            resolution: resolution.clone(),
            system_prompt: assets.system.into(),
            user_prompt,
        })
    }
    pub fn validate(&self) -> Result<(), String> {
        self.resolution.validate()?;
        let assets = contract_assets(self.resolution.schema_version);
        if self.contract_version != assets.contract_version
            || self.system_prompt != assets.system
            || self.user_prompt.is_empty()
        {
            return Err("unsupported formal review invocation".into());
        }
        Ok(())
    }
}
