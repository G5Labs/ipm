//! Shared enumerations matching Postgres CHECK constraints.
//!
//! Every enum serializes to lowercase `snake_case` strings via
//! `#[serde(rename_all = "snake_case")]` and implements `Display` by
//! delegating to the same snake_case serialization.
//!
//! All enums derive: `Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize`

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Macro for Display delegation to serde
// ---------------------------------------------------------------------------

/// Implements `Display` for an enum by serializing via serde_json and
/// stripping the surrounding quotes. This ensures `Display` outputs the
/// same snake_case string as serde.
macro_rules! impl_display_via_serde {
    ($t:ty) => {
        impl fmt::Display for $t {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                // Serialize to JSON string (e.g., `"rolled_back"`), then strip quotes.
                let s = serde_json::to_string(self).map_err(|_| fmt::Error)?;
                // Strip the surrounding double quotes
                write!(f, "{}", &s[1..s.len() - 1])
            }
        }
    };
}

/// How a task executes. Its descriptive label never selects launch policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, strum::EnumIter)]
#[serde(rename_all = "snake_case")]
pub enum TaskKind {
    Agent,
    Command,
    Preview,
}

impl_display_via_serde!(TaskKind);

impl FromStr for TaskKind {
    type Err = String;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "agent" => Ok(Self::Agent),
            "command" => Ok(Self::Command),
            "preview" => Ok(Self::Preview),
            _ => Err(format!("unsupported task kind {value}")),
        }
    }
}

// ---------------------------------------------------------------------------
// NodeLevel
// ---------------------------------------------------------------------------

/// Node hierarchy level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum NodeLevel {
    File,
    Module,
    Project,
}

impl_display_via_serde!(NodeLevel);

// ---------------------------------------------------------------------------
// OrgRole
// ---------------------------------------------------------------------------

/// A workspace's immutable registry kind (`g5.workspace.kind`). It is fixed
/// at creation and never inferred from a name, owner or ancestry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkspaceKind {
    Base,
    Working,
    Personal,
}

impl_display_via_serde!(WorkspaceKind);

impl WorkspaceKind {
    /// Derive the kind from the registry columns that store it.
    pub fn from_registry(is_base: bool, personal_user_id: Option<uuid::Uuid>) -> Self {
        if is_base {
            Self::Base
        } else if personal_user_id.is_some() {
            Self::Personal
        } else {
            Self::Working
        }
    }

    pub fn is_personal(self) -> bool {
        self == Self::Personal
    }
}

/// Organization membership role.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum OrgRole {
    Admin,
    Member,
    Viewer,
}

impl_display_via_serde!(OrgRole);

impl FromStr for OrgRole {
    type Err = String;

    /// Case-insensitive parse: "admin", "Admin", "ADMIN" all produce `OrgRole::Admin`.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "admin" => Ok(Self::Admin),
            "member" => Ok(Self::Member),
            "viewer" => Ok(Self::Viewer),
            other => Err(format!("unknown org role: {other:?}")),
        }
    }
}

// ---------------------------------------------------------------------------
// TimelineEntryType
// ---------------------------------------------------------------------------

/// Timeline entry type for agent execution traces.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum TimelineEntryType {
    Thought,
    ToolCall,
    ToolResult,
    Output,
    Error,
    UserMessage,
    AgentMessage,
    Checkpoint,
    SkillInvocation,
    SubagentInvocation,
}

impl_display_via_serde!(TimelineEntryType);

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn execution_kinds_are_closed_and_labels_are_not_kinds() {
        for (kind, wire) in [
            (TaskKind::Agent, "agent"),
            (TaskKind::Command, "command"),
            (TaskKind::Preview, "preview"),
        ] {
            assert_eq!(kind.to_string(), wire);
            assert_eq!(wire.parse::<TaskKind>().unwrap(), kind);
            assert_eq!(
                serde_json::from_value::<TaskKind>(serde_json::json!(wire)).unwrap(),
                kind
            );
        }
        for label in ["chat", "sync", "review", "explainer_generate", "custom"] {
            assert!(label.parse::<TaskKind>().is_err());
        }
    }

    // -- NodeLevel --

    #[test]
    fn test_node_level_variants() {
        let variants = [
            (NodeLevel::File, "file"),
            (NodeLevel::Module, "module"),
            (NodeLevel::Project, "project"),
        ];
        for (v, expected) in &variants {
            assert_eq!(v.to_string(), *expected);
        }
    }

    // -- OrgRole --

    #[test]
    fn test_org_role_variants() {
        let variants = [
            (OrgRole::Admin, "admin"),
            (OrgRole::Member, "member"),
            (OrgRole::Viewer, "viewer"),
        ];
        for (v, expected) in &variants {
            assert_eq!(v.to_string(), *expected);
        }
        assert!("owner".parse::<OrgRole>().is_err());
        assert!(serde_json::from_str::<OrgRole>("\"owner\"").is_err());
    }

    // -- TimelineEntryType --

    #[test]
    fn test_timeline_entry_type_variants() {
        let variants = [
            (TimelineEntryType::Thought, "thought"),
            (TimelineEntryType::ToolCall, "tool_call"),
            (TimelineEntryType::ToolResult, "tool_result"),
            (TimelineEntryType::Output, "output"),
            (TimelineEntryType::Error, "error"),
            (TimelineEntryType::UserMessage, "user_message"),
            (TimelineEntryType::AgentMessage, "agent_message"),
            (TimelineEntryType::Checkpoint, "checkpoint"),
            (TimelineEntryType::SkillInvocation, "skill_invocation"),
            (TimelineEntryType::SubagentInvocation, "subagent_invocation"),
        ];
        assert_eq!(variants.len(), 10);
        for (v, expected) in &variants {
            assert_eq!(v.to_string(), *expected);
        }
    }

    // -- Cross-cutting tests --

    #[test]
    fn test_enum_copy_clone() {
        let t = TaskKind::Agent;
        let t2 = t; // Copy
        #[allow(clippy::clone_on_copy)]
        let t3 = t.clone(); // Clone — intentionally testing Clone impl
        assert_eq!(t, t2);
        assert_eq!(t, t3);
    }

    #[test]
    fn test_enum_hash() {
        use std::collections::HashSet;
        let mut set = HashSet::new();
        set.insert(TaskKind::Agent);
        set.insert(TaskKind::Command);
        set.insert(TaskKind::Agent); // duplicate
        assert_eq!(set.len(), 2);
    }

    #[test]
    fn test_enum_debug() {
        assert_eq!(format!("{:?}", TaskKind::Agent), "Agent");
    }

    #[test]
    fn test_all_enums_exist_and_serialize() {
        // Verify the enums in this file exist and serialize correctly.
        let checks: Vec<(&str, String)> = vec![
            ("TaskKind", serde_json::to_string(&TaskKind::Agent).unwrap()),
            (
                "NodeLevel",
                serde_json::to_string(&NodeLevel::File).unwrap(),
            ),
            ("OrgRole", serde_json::to_string(&OrgRole::Admin).unwrap()),
            (
                "TimelineEntryType",
                serde_json::to_string(&TimelineEntryType::Thought).unwrap(),
            ),
        ];
        for (name, json) in &checks {
            assert!(
                json.len() > 2,
                "{} should serialize to a non-empty string",
                name
            );
        }
    }
}
