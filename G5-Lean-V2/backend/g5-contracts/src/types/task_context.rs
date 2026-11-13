//! Shared task execution, document-reference, repository, and credential metadata.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::error::G5Error;

/// Evidence mode used by non-isolated workspace-chat sync.
pub const WORKSPACE_SYNC_CHAT_EVIDENCE_MODE: &str = "current_target_snapshot";

/// One exact repository member used to resolve an immutable sync reference scope.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SyncReferenceRepoVersion {
    pub repo_id: String,
    pub head_commit: String,
}

/// Canonical document-reference evidence captured before a sync task is created.
///
/// Projection UUIDs are intentionally absent. `resolved_component_ids` contains
/// repo-routed selectors, while the exact repository vector and effective
/// content hash make the prepared scope reproducible after later advances.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SyncReferenceScope {
    pub repository_versions: Vec<SyncReferenceRepoVersion>,
    pub source_content_hash: String,
    pub source_dirty_overlay: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub raw_references: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub resolved_component_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub unresolved_references: Vec<String>,
}

/// SCM credential for worker git operations.
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScmCredential {
    pub url: String,
    pub token: String,
    pub provider: String,
}

impl std::fmt::Debug for ScmCredential {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ScmCredential")
            .field("url", &self.url)
            .field("token", &"[REDACTED]")
            .field("provider", &self.provider)
            .finish()
    }
}

// ---------------------------------------------------------------------------
// SecretString — redacted wrapper for sensitive values
// ---------------------------------------------------------------------------

/// A string value that redacts itself in `Debug` and `Display` output.
#[derive(Clone, Serialize, Deserialize)]
pub struct SecretString(String);

impl SecretString {
    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }

    /// Access the underlying secret value.
    #[must_use]
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Debug for SecretString {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("[REDACTED]")
    }
}

impl std::fmt::Display for SecretString {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("[REDACTED]")
    }
}

// ---------------------------------------------------------------------------
// TaskSecrets — per-dispatch credential bucket
// ---------------------------------------------------------------------------

/// Secrets delivered with one module dispatch.
///
/// Runtime processes never provision credentials themselves. Everything
/// arrives in this opaque bucket, scoped to a single execution.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskSecrets {
    /// SCM credentials for external repo access (future — unused in mirror mode).
    #[serde(default)]
    pub scm: Vec<ScmCredential>,
    /// Task-scoped environment variables exported to the agent.
    #[serde(default, alias = "custom", skip_serializing_if = "HashMap::is_empty")]
    pub env: HashMap<String, SecretString>,
}

pub(crate) const RESERVED_TASK_ENV_PREFIXES: &[&str] = &["G5_", "G5D_"];

/// Keys the platform owns in every owner process: the shell's path and home,
/// the environment the dispatcher installs for Git identity and
/// configuration, the provider homes, and the API's own configuration
/// (`g5.terminal.secret-admission`). Loader paths are deliberately absent:
/// they are not a boundary against the owner's guest authority.
pub(crate) const RESERVED_TASK_ENV_KEYS: &[&str] = &[
    "PATH",
    "HOME",
    "GIT_AUTHOR_NAME",
    "GIT_AUTHOR_EMAIL",
    "GIT_COMMITTER_NAME",
    "GIT_COMMITTER_EMAIL",
    "GIT_CONFIG_GLOBAL",
    "GIT_CONFIG_NOSYSTEM",
    "GIT_TERMINAL_PROMPT",
    "CODEX_HOME",
    "CLAUDE_CONFIG_DIR",
    "PWD",
    "OLDPWD",
    "SHELL",
    "USER",
    "USERNAME",
    "LOGNAME",
    "TMPDIR",
    "TMP",
    "TEMP",
    "DATABASE_URL",
    "REDIS_URL",
    "PGHOST",
    "PGPORT",
    "PGUSER",
    "PGPASSWORD",
    "PGDATABASE",
    "JWT_SECRET",
    "JWT_PRIVATE_KEY",
    "JWT_PUBLIC_KEY",
    "PRIVATE_KEY",
    "SSL_PRIVATE_KEY",
    "RUST_LOG",
    "RUST_BACKTRACE",
];

/// Returns true when a Runtime Config secret key may be exported as a
/// task-scoped process environment variable.
#[must_use]
pub fn is_exportable_task_env_key(key: &str) -> bool {
    let key = key.trim();
    if key.is_empty() {
        return false;
    }
    if RESERVED_TASK_ENV_PREFIXES
        .iter()
        .any(|prefix| key.starts_with(prefix))
    {
        return false;
    }
    if RESERVED_TASK_ENV_KEYS.contains(&key) {
        return false;
    }
    let mut chars = key.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !(first == '_' || first.is_ascii_alphabetic()) {
        return false;
    }
    chars.all(|ch| ch == '_' || ch.is_ascii_alphanumeric())
}

/// Retired credential-upload key, retained only for rejection and scrubbing.
/// Subscription credentials now belong exclusively to the owner-bound VM.
pub const CODEX_AUTH_JSON_ENV_KEY: &str = "CODEX_AUTH_JSON";

/// Env keys that represent provider credentials and therefore must be delivered
/// to a task ONLY through the write-only secrets channel — never honored from
/// the readable, non-secret Runtime Config `settings` map. This prevents a
/// credential value accidentally (or via the legacy raw-JSON editor) stored in
/// the plaintext settings channel from acting as an agent credential.
// `OPENAI_API_KEY` is the Codex alternative credential. The retired
// `CODEX_AUTH_JSON` remains listed defensively, but is never dispatched or
// materialized: subscription login now belongs to the VM. `OPENROUTER_API_KEY` (the primary
// Codex credential) is intentionally NOT here: it is a worker provider-route key,
// and classifying it credential-only would regress how Claude-via-OpenRouter
// routing is detected.
const CREDENTIAL_ONLY_TASK_ENV_KEYS: &[&str] = &[
    "ANTHROPIC_AUTH_TOKEN",
    "CLAUDE_CODE_OAUTH_TOKEN",
    "ANTHROPIC_API_KEY",
    "OPENAI_API_KEY",
    CODEX_AUTH_JSON_ENV_KEY,
];

/// Returns true when `key` names a credential that must only be sourced from the
/// secrets channel (see [`CREDENTIAL_ONLY_TASK_ENV_KEYS`]).
#[must_use]
pub fn is_credential_only_task_env_key(key: &str) -> bool {
    CREDENTIAL_ONLY_TASK_ENV_KEYS.contains(&key.trim())
}

// ---------------------------------------------------------------------------
// ChatTaskContext
// ---------------------------------------------------------------------------

/// Provider discriminator for chat tasks routed through Provided Agents.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum ChatAgentProvider {
    /// Claude Code CLI Provided Agent.
    #[default]
    ClaudeCode,
    /// OpenAI Codex CLI Provided Agent.
    Codex,
}

impl ChatAgentProvider {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            ChatAgentProvider::ClaudeCode => "claude_code",
            ChatAgentProvider::Codex => "codex",
        }
    }

    #[must_use]
    pub fn requires_worker_capability(self) -> Option<&'static str> {
        match self {
            ChatAgentProvider::ClaudeCode => Some("agent_provider:claude_code"),
            ChatAgentProvider::Codex => Some("agent_provider:codex"),
        }
    }
}

impl std::fmt::Display for ChatAgentProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl std::str::FromStr for ChatAgentProvider {
    type Err = G5Error;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "claude_code" => Ok(ChatAgentProvider::ClaudeCode),
            "codex" => Ok(ChatAgentProvider::Codex),
            other => Err(G5Error::ValidationError {
                details: serde_json::json!({
                    "agentProvider": format!("Unsupported chat agent provider '{other}'"),
                }),
            }),
        }
    }
}

/// Chat-specific metadata carried within a `TaskContext` for chat tasks.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ForceCompleteTaskRequest {
    /// Human/UX supplied completion summary and commit message.
    pub commit_message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PostSyncContinuationContext {
    pub schema_version: u32,
    pub source: String,
    pub session_id: String,
    pub default_lifecycle: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ChatAttachmentContext {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message_id: Option<String>,
    pub file_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_type: Option<String>,
    pub size_bytes: i64,
    pub sha256: String,
    pub scan_status: String,
    pub storage_key: String,
    pub provider_mount_path: String,
    pub download_path: String,
    #[serde(default)]
    pub read_only: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatHistoryMessage {
    pub message_id: String,
    pub role: String,
    pub content: String,
    pub status: String,
    pub message_index: i32,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatTaskContext {
    /// The chat session this task belongs to.
    pub session_id: String,
    /// The pending assistant message this task should populate.
    pub message_id: String,
    /// The assistant message index for this turn. Used to distinguish first vs follow-up chat turns.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub assistant_message_index: Option<i32>,
    /// The user's message content that triggered this task.
    pub user_content: String,
    /// Prior persisted messages for this chat session, ordered oldest first.
    ///
    /// This is copied from G5's `chat_messages` table at dispatch time so
    /// module execution can restore provider history.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub conversation_history: Vec<ChatHistoryMessage>,
    /// Local-file chat attachments selected for this same user turn.
    ///
    /// Workers fetch each attachment through `download_path` using their
    /// internal worker token and materialize it read-only into the task
    /// workspace before launching the provider.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attachments: Vec<ChatAttachmentContext>,
    /// §38.840: Optional model override requested by the user.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// §38.840: Optional max_tokens override requested by the user.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<i32>,
    /// Claude Code permission mode snapshot for this turn.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub permission_mode: Option<String>,
    /// Claude Code reasoning effort snapshot for this turn.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effort: Option<String>,
    /// Provided Agents provider selected for this chat turn.
    #[serde(default)]
    pub agent_provider: ChatAgentProvider,
    /// Worker-native Force Complete request for waiting Provided-Agent task sessions.
    ///
    /// This is a routing hint only. The worker still performs commit/push and
    /// the authoritative task transition.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub force_complete: Option<ForceCompleteTaskRequest>,
    /// Post-sync continuation lifecycle metadata for resumed completed sync chats.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub post_sync_continuation: Option<PostSyncContinuationContext>,
    /// Merge workflow subtype for task-scoped merge sessions.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub merge_subtype: Option<String>,
    /// Source branch name for workspace merge workflows.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_branch_name: Option<String>,
    /// Source workspace id for workspace-merge workflows.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_workspace_id: Option<String>,
    /// Source workspace display name for workspace-merge workflows.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_workspace_name: Option<String>,
    /// Target workspace id for workspace-merge workflows.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_workspace_id: Option<String>,
    /// Target workspace display name for workspace-merge workflows.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_workspace_name: Option<String>,
}

impl ChatTaskContext {
    #[must_use]
    pub fn user_prompt_for_warm_continuation(&self) -> String {
        self.user_content.clone()
    }

    #[must_use]
    pub fn user_prompt_with_conversation_history(&self) -> String {
        if self.conversation_history.is_empty() {
            return self.user_content.clone();
        }

        let mut prompt = String::from("Previous G5 chat session history, oldest first:\n\n");
        for message in &self.conversation_history {
            let role = match message.role.as_str() {
                "user" => "User",
                "assistant" => "Assistant",
                "system" => "System",
                "tool" => "Tool",
                other => other,
            };
            let content = message.content.trim();
            if content.is_empty() {
                continue;
            }
            if message.role == "assistant" && message.status != "complete" {
                prompt.push_str(&format!(
                    "[{}] {} ({}): {}\n\n",
                    message.message_index, role, message.status, content
                ));
            } else {
                prompt.push_str(&format!(
                    "[{}] {}: {}\n\n",
                    message.message_index, role, content
                ));
            }
        }
        prompt.push_str("Current user request:\n");
        prompt.push_str(self.user_content.trim());
        prompt
    }
}

/// Maximum length of a preview profile name (file-stem).
pub const PREVIEW_PROFILE_NAME_MAX_LEN: usize = 64;

/// Conventional well-known preview profile name for a repository's frontend
/// (web client) app: `.g5/preview_profiles/frontend.yaml` (G5-Lean's own
/// hand-written client profile is `frontend-only`, not this one). Agent surfaces
/// generate and drift-update this profile under a fixed name so repeated runs
/// stay idempotent (plan `pr-body-and-preview-profiles` v1).
pub const PREVIEW_PROFILE_NAME_FRONTEND: &str = "frontend";

/// Conventional well-known preview profile name for a repository's backend
/// (API server) app: `.g5/preview_profiles/backend.yaml`. Same maintenance
/// contract as [`PREVIEW_PROFILE_NAME_FRONTEND`].
pub const PREVIEW_PROFILE_NAME_BACKEND: &str = "backend";

/// Validate a preview profile name as a safe kebab-case file-stem (D18). It
/// must be non-empty, at most [`PREVIEW_PROFILE_NAME_MAX_LEN`] chars, and
/// contain only `[a-z0-9-]`, so it can never contain `/`, `.`, or `..` and
/// thus can never escape the `.g5/preview_profiles/` directory when joined to
/// a path. The API and the guest dispatcher both apply this one rule.
pub fn validate_preview_profile_name(name: &str) -> Result<(), String> {
    if name.is_empty() {
        return Err("profile name must not be empty".to_string());
    }
    if name.len() > PREVIEW_PROFILE_NAME_MAX_LEN {
        return Err(format!(
            "profile name must be at most {PREVIEW_PROFILE_NAME_MAX_LEN} characters"
        ));
    }
    if !name
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
    {
        return Err(
            "profile name must contain only lowercase ASCII letters, digits or '-'".to_string(),
        );
    }
    Ok(())
}

/// Output format for a modal-driven explainer (g5-common/types/core D25).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExplainerFormat {
    /// One self-contained single-page HTML document.
    Page,
    /// A keyboard-navigable slide deck in the same single HTML file.
    Slides,
    /// One self-contained Markdown document.
    Markdown,
}

impl ExplainerFormat {
    /// Managed file extension for this format (core D25): the server stamps
    /// the target path with it and the agent never chooses another.
    pub fn managed_extension(self) -> &'static str {
        match self {
            ExplainerFormat::Page | ExplainerFormat::Slides => "html",
            ExplainerFormat::Markdown => "md",
        }
    }
}

/// Whether `path` sits under a managed explainer root (core D25): the
/// workspace Project-Docs subfolder `docs/<segment>/explainers/` for
/// current tasks, or the legacy top-level `explainers/` root for
/// pre-cutover documents. Single source for both server crates.
pub fn is_managed_explainer_path(path: &str) -> bool {
    // No component may be empty, `.`, or `..` — a traversal segment would
    // let a "managed" path escape the declared root (review finding
    // 2026-08-10: `explainers/../outside.md` previously passed).
    fn components_are_clean(path: &str) -> bool {
        path.split('/')
            .all(|segment| !segment.is_empty() && segment != "." && segment != "..")
    }
    if let Some(rest) = path.strip_prefix("explainers/") {
        return !rest.is_empty() && components_are_clean(rest);
    }
    let Some(rest) = path.strip_prefix("docs/") else {
        return false;
    };
    let Some((folder, tail)) = rest.split_once('/') else {
        return false;
    };
    if folder.is_empty() || folder == "." || folder == ".." {
        return false;
    }
    match tail.strip_prefix("explainers/") {
        Some(name) => !name.is_empty() && components_are_clean(name),
        None => false,
    }
}

/// Ownership role for a repository in a multi-repo workspace.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RepoRole {
    Docs,
    Code,
    #[default]
    Unknown,
}

/// A repository in the workspace, with its local checkout path and metadata.
///
/// Populated at dispatch so a module can address every repository in a
/// multi-repository workspace.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RepoInfo {
    /// Repo UUID (from `repos.id`).
    pub repo_id: String,
    /// Human-readable repo name (e.g. "tinydb").
    pub name: String,
    /// Workspace-relative folder prefix owned by this repo in the overlay.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub root_path: Option<String>,
    /// Workspace-relative folder prefixes owned by this repo in the overlay.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub root_paths: Vec<String>,
    /// Explicit repository ownership role for multi-repo behavior.
    #[serde(default)]
    pub repo_role: RepoRole,
    /// Smart HTTP URL for worker clone/fetch operations.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub git_url: Option<String>,
    /// Repo-specific branch name for bootstrap checkout.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch_name: Option<String>,
    /// Repo-specific base commit for bootstrap checkout.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_commit: Option<String>,
}

impl RepoInfo {
    /// Canonical workspace-relative mount used by the materializer.
    ///
    /// `root_path` is authoritative when populated; `root_paths` is the
    /// rolling-compatibility fallback. The workspace materializer enforces
    /// structural safety before filesystem use.
    #[must_use]
    pub fn canonical_workspace_mount(&self) -> Option<&str> {
        fn normalized(value: &str) -> Option<&str> {
            let value = value.trim();
            if value.is_empty()
                || value.contains('\\')
                || value.contains('\0')
                || value
                    .split('/')
                    .any(|segment| segment.is_empty() || segment == "." || segment == "..")
                || value.split('/').next() == Some(".g5")
            {
                return None;
            }
            Some(value)
        }

        if let Some(root_path) = self
            .root_path
            .as_deref()
            .filter(|root| !root.trim().is_empty())
        {
            return normalized(root_path);
        }
        self.root_paths.iter().find_map(|root| normalized(root))
    }
}

/// Captured source branch head used to validate workspace-source merge mounts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MergeSourceHead {
    pub repo_id: String,
    pub branch_name: String,
    pub workspace_id: String,
    pub head_commit: String,
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[allow(clippy::too_many_arguments)]
    #[test]
    fn task_env_key_validator_allows_standard_agent_env_names() {
        for key in [
            "ANTHROPIC_API_KEY",
            "ANTHROPIC_AUTH_TOKEN",
            "CLAUDE_CODE_OAUTH_TOKEN",
            "ANTHROPIC_BASE_URL",
            "ANTHROPIC_MODEL",
            "CLAUDE_CODE_SUBAGENT_MODEL",
            "ANTHROPOC_IMPORTANT_KEY",
            "ARBITRARY_CUSTOM_ENV_SECRET",
            "_CUSTOM_TOKEN",
        ] {
            assert!(is_exportable_task_env_key(key), "{key} should export");
        }
    }

    #[test]
    fn task_env_key_validator_denies_platform_and_process_keys() {
        for key in [
            "",
            "llm.apiKey",
            "1TOKEN",
            "G5D_TOKEN",
            "G5_CLAUDE_EXPLICIT_PROVIDER_ENV_JSON",
            "PATH",
            "HOME",
            "DATABASE_URL",
            "JWT_PRIVATE_KEY",
        ] {
            assert!(!is_exportable_task_env_key(key), "{key} should be denied");
        }
    }

    #[test]
    fn credential_only_task_env_key_includes_all_provider_credentials() {
        for key in [
            "ANTHROPIC_AUTH_TOKEN",
            "CLAUDE_CODE_OAUTH_TOKEN",
            "ANTHROPIC_API_KEY",
        ] {
            assert!(
                is_credential_only_task_env_key(key),
                "{key} must only be honored from Runtime Config secrets"
            );
        }
        assert!(!is_credential_only_task_env_key("ANTHROPIC_BASE_URL"));
    }

    // -- RepoInfo tests --

    #[test]
    fn test_repo_info_serde_emits_repo_role() {
        let repo = RepoInfo {
            repo_id: "repo-docs".to_string(),
            name: "docs".to_string(),
            root_path: Some("docs".to_string()),
            root_paths: vec!["docs".to_string()],
            repo_role: RepoRole::Docs,
            git_url: None,
            branch_name: None,
            base_commit: None,
        };

        let json = serde_json::to_value(&repo).unwrap();
        assert_eq!(json.get("repoRole").and_then(|v| v.as_str()), Some("docs"));
        assert!(json.get("workspaceBranchId").is_none());
        assert!(json.get("workspaceBranchRepoId").is_none());
        assert!(json.get("baseWorkspaceBranchId").is_none());
        assert!(json.get("repo_role").is_none());
    }

    #[test]
    fn test_repo_info_missing_repo_role_deserializes_unknown() {
        let json = serde_json::json!({
            "repoId": "legacy-repo",
            "name": "legacy"
        });

        let repo: RepoInfo = serde_json::from_value(json).unwrap();
        assert_eq!(repo.repo_role, RepoRole::Unknown);
    }

    #[test]
    fn test_preview_context_validate_profile_name() {
        assert!(validate_preview_profile_name("web-client-2").is_ok());
        for bad in [
            "../etc",
            "a/b",
            "with.dot",
            "",
            &"x".repeat(65),
            "web_client",
            "Web",
        ] {
            assert!(
                validate_preview_profile_name(bad).is_err(),
                "profile name {bad:?} must be rejected"
            );
        }

        // The standalone validator matches the contract.
        assert!(validate_preview_profile_name("api").is_ok());
        assert!(validate_preview_profile_name("..").is_err());
        assert!(validate_preview_profile_name("a.b").is_err());

        // The conventional well-known generated profile names are valid stems.
        assert!(validate_preview_profile_name(PREVIEW_PROFILE_NAME_FRONTEND).is_ok());
        assert!(validate_preview_profile_name(PREVIEW_PROFILE_NAME_BACKEND).is_ok());
    }

    /// The managed-root predicate accepts the workspace-scoped shape and the
    /// legacy top-level root, and rejects lookalikes (core D25).
    #[test]
    fn test_is_managed_explainer_path_shapes() {
        // Workspace-scoped (current) shape.
        assert!(is_managed_explainer_path(
            "docs/0e1f2a3b-0000-0000-0000-000000000001/explainers/topic-2026-08-10-0100.html"
        ));
        assert!(is_managed_explainer_path(
            "docs/0e1f2a3b-0000-0000-0000-000000000001/explainers/topic-2026-08-10-0100.md"
        ));
        // Legacy top-level root.
        assert!(is_managed_explainer_path("explainers/topic.html"));
        // Rejected lookalikes.
        assert!(!is_managed_explainer_path("explainers/"));
        assert!(!is_managed_explainer_path("docs/ws/explainers/"));
        assert!(!is_managed_explainer_path("docs/explainers/topic.html"));
        assert!(!is_managed_explainer_path("docs/ws/notes/topic.html"));
        assert!(!is_managed_explainer_path("src/explainers/nope.html"));
        assert!(!is_managed_explainer_path(
            "docs/ws/sub/explainers/topic.html"
        ));
        assert!(!is_managed_explainer_path("views/explainers-guide.md"));
        // Traversal / degenerate components never count as managed
        // (review finding 2026-08-10).
        assert!(!is_managed_explainer_path("explainers/../outside.md"));
        assert!(!is_managed_explainer_path(
            "docs/ws/explainers/../outside.md"
        ));
        assert!(!is_managed_explainer_path("docs/../explainers/escape.html"));
        assert!(!is_managed_explainer_path("explainers//double.md"));
        assert!(!is_managed_explainer_path("explainers/./dot.md"));
        assert!(!is_managed_explainer_path("docs/ws/explainers/a/../b.html"));
    }

    /// The managed extension is format-derived (core D25).
    #[test]
    fn test_explainer_format_managed_extension() {
        assert_eq!(ExplainerFormat::Page.managed_extension(), "html");
        assert_eq!(ExplainerFormat::Slides.managed_extension(), "html");
        assert_eq!(ExplainerFormat::Markdown.managed_extension(), "md");
        assert_eq!(
            serde_json::to_value(ExplainerFormat::Markdown).expect("serialize"),
            serde_json::json!("markdown"),
            "snake_case wire literal"
        );
    }

    #[test]
    fn test_chat_agent_provider_defaults_to_claude_code() {
        let json = serde_json::json!({
            "sessionId": "s1",
            "messageId": "m1",
            "userContent": "hello"
        });
        let ctx: ChatTaskContext = serde_json::from_value(json).unwrap();
        assert_eq!(ctx.agent_provider, ChatAgentProvider::ClaudeCode);
        assert_eq!(ctx.assistant_message_index, None);
    }

    #[test]
    fn test_chat_task_context_assistant_message_index_round_trips() {
        let json = serde_json::json!({
            "sessionId": "s1",
            "messageId": "m1",
            "assistantMessageIndex": 3,
            "userContent": "hello",
            "agentProvider": "claude_code"
        });
        let ctx: ChatTaskContext = serde_json::from_value(json).unwrap();
        assert_eq!(ctx.assistant_message_index, Some(3));
        let encoded = serde_json::to_value(&ctx).unwrap();
        assert_eq!(
            encoded
                .get("assistantMessageIndex")
                .and_then(|v| v.as_i64()),
            Some(3)
        );
    }

    #[test]
    fn test_chat_agent_provider_claude_code_round_trips() {
        let ctx = ChatTaskContext {
            session_id: "s1".to_string(),
            message_id: "m1".to_string(),
            assistant_message_index: Some(1),
            user_content: "hello".to_string(),
            attachments: Vec::new(),
            conversation_history: Vec::new(),
            model: None,
            max_tokens: None,
            permission_mode: None,
            effort: None,
            agent_provider: ChatAgentProvider::ClaudeCode,
            force_complete: None,
            post_sync_continuation: None,
            merge_subtype: None,
            source_branch_name: None,
            source_workspace_id: None,
            source_workspace_name: None,
            target_workspace_id: None,
            target_workspace_name: None,
        };
        let json = serde_json::to_value(&ctx).unwrap();
        assert_eq!(
            json.get("agentProvider").and_then(|v| v.as_str()),
            Some("claude_code")
        );
        let decoded: ChatTaskContext = serde_json::from_value(json).unwrap();
        assert_eq!(decoded.agent_provider, ChatAgentProvider::ClaudeCode);
        assert_eq!(
            ChatAgentProvider::ClaudeCode.requires_worker_capability(),
            Some("agent_provider:claude_code")
        );
    }

    #[test]
    fn test_chat_agent_provider_codex_round_trips() {
        let ctx = ChatTaskContext {
            session_id: "s1".to_string(),
            message_id: "m1".to_string(),
            assistant_message_index: Some(1),
            user_content: "hello".to_string(),
            attachments: Vec::new(),
            conversation_history: Vec::new(),
            model: None,
            max_tokens: None,
            permission_mode: None,
            effort: None,
            agent_provider: ChatAgentProvider::Codex,
            force_complete: None,
            post_sync_continuation: None,
            merge_subtype: None,
            source_branch_name: None,
            source_workspace_id: None,
            source_workspace_name: None,
            target_workspace_id: None,
            target_workspace_name: None,
        };
        let json = serde_json::to_value(&ctx).unwrap();
        assert_eq!(
            json.get("agentProvider").and_then(|v| v.as_str()),
            Some("codex")
        );
        let decoded: ChatTaskContext = serde_json::from_value(json).unwrap();
        assert_eq!(decoded.agent_provider, ChatAgentProvider::Codex);
        assert_eq!(ChatAgentProvider::Codex.as_str(), "codex");
        assert_eq!(
            "codex".parse::<ChatAgentProvider>().unwrap(),
            ChatAgentProvider::Codex
        );
        assert_eq!(
            ChatAgentProvider::Codex.requires_worker_capability(),
            Some("agent_provider:codex")
        );
    }

    #[test]
    fn test_codex_openai_key_is_credential_only_openrouter_stays_route_key() {
        // OPENAI_API_KEY and the ChatGPT-subscription CODEX_AUTH_JSON join the
        // Anthropic secrets as credential-only (AC-3).
        assert!(is_credential_only_task_env_key("OPENAI_API_KEY"));
        assert!(is_credential_only_task_env_key("CODEX_AUTH_JSON"));
        assert!(is_credential_only_task_env_key("ANTHROPIC_API_KEY"));
        assert!(is_credential_only_task_env_key("ANTHROPIC_AUTH_TOKEN"));
        // OPENROUTER_API_KEY stays a provider-route key, NOT credential-only, so
        // Claude-via-OpenRouter routing classification is unchanged.
        assert!(!is_credential_only_task_env_key("OPENROUTER_API_KEY"));
        assert!(!is_credential_only_task_env_key("OPENAI_BASE_URL"));
        // All Codex env names are valid, non-reserved, and therefore exportable.
        assert!(is_exportable_task_env_key("OPENAI_API_KEY"));
        assert!(is_exportable_task_env_key("OPENROUTER_API_KEY"));
        assert!(is_exportable_task_env_key("OPENAI_BASE_URL"));
    }

    #[test]
    fn chat_prompt_preserves_history_without_cross_session_context() {
        let ctx = ChatTaskContext {
            user_content: "current request".to_string(),
            conversation_history: vec![ChatHistoryMessage {
                message_id: "message-0".to_string(),
                message_index: 0,
                role: "user".to_string(),
                content: "earlier request".to_string(),
                status: "complete".to_string(),
            }],
            ..Default::default()
        };

        assert_eq!(
            ctx.user_prompt_with_conversation_history(),
            "Previous G5 chat session history, oldest first:\n\n[0] User: earlier request\n\nCurrent user request:\ncurrent request"
        );
        assert_eq!(ctx.user_prompt_for_warm_continuation(), "current request");
    }
}
