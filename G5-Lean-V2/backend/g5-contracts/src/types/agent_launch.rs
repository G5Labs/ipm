//! Versioned agent-launch preferences, server resolution, and worker receipt.
//!
//! The browser chooses sources, the API resolves concrete values exactly once,
//! and workers execute the immutable resolution without fallback.

use std::collections::HashMap;
use std::fmt::Write as _;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::claude_models::{
    claude_default_effort_for_model, claude_model_provider_from_anthropic_base_url,
    default_claude_chat_model_for_provider, normalize_claude_model_for_provider,
    ClaudeModelProvider, ANTHROPIC_API_KEY_ENV, ANTHROPIC_AUTH_TOKEN_ENV, ANTHROPIC_BASE_URL_ENV,
    CLAUDE_CHAT_MODES, CLAUDE_CODE_OAUTH_TOKEN_ENV, CLAUDE_DEFAULT_CHAT_MODE, CLAUDE_EFFORT_ORDER,
};
use crate::codex_models::{
    codex_default_effort_for_model, codex_model_catalog, default_codex_chat_model_for_provider,
    normalize_codex_model_for_provider, CodexModelProvider, CODEX_CHAT_MODES,
    CODEX_DEFAULT_CHAT_MODE, CODEX_EFFORT_ORDER, OPENAI_API_KEY_ENV, OPENAI_BASE_URL_ENV,
    OPENROUTER_API_KEY_ENV,
};
use crate::error::G5Error;

use super::provider_account::{AgentAccountSelectionV1, ProviderAccountKind, ProviderAccountScope};
use super::ChatAgentProvider;

pub const AGENT_LAUNCH_SCHEMA_VERSION: u8 = 1;
pub const AGENT_LAUNCH_CATALOG_VERSION: &str = "agent-launch-catalog-v1";
pub const CODEX_SUBSCRIPTION_ENABLED_SETTING: &str = "codex_subscription_enabled";

/// Whether an allowed-tools rule names a `g5-tools` tool G5 no longer
/// provides. Such a request is refused, never passed to a provider that
/// cannot supply the tool.
pub fn names_removed_agent_tool(tool: &str) -> bool {
    tool == "mcp__g5-tools" || tool.starts_with("mcp__g5-tools__")
}

/// Validate tool availability against the resolved provider before reservation
/// and again in the worker. An empty selection keeps the native tool set.
pub fn validate_agent_tool_selection(
    provider: ChatAgentProvider,
    questions_enabled: bool,
    tools: &[String],
) -> Result<(), &'static str> {
    if tools.len() > 256
        || tools.iter().any(|tool| {
            tool.trim().is_empty()
                || tool.len() > 256
                || tool.chars().any(char::is_control)
                || names_removed_agent_tool(tool)
        })
    {
        return Err("INVALID_ALLOWED_TOOL");
    }
    if tools
        .iter()
        .enumerate()
        .any(|(index, tool)| tools[..index].contains(tool))
    {
        return Err("DUPLICATE_ALLOWED_TOOL");
    }
    if tools.is_empty() {
        return Ok(());
    }
    if provider == ChatAgentProvider::Codex {
        return Err("UNSUPPORTED_PERMISSION_POLICY: Codex allowed_tools");
    }
    // Claude's native availability selector supports built-ins; permission
    // expressions and MCP selections cannot be mapped to it safely.
    if tools.iter().any(|tool| {
        !matches!(
            tool.as_str(),
            "Read"
                | "Write"
                | "Edit"
                | "Bash"
                | "Glob"
                | "Grep"
                | "NotebookEdit"
                | "WebFetch"
                | "WebSearch"
                | "AskUserQuestion"
        )
    }) {
        return Err("UNSUPPORTED_ALLOWED_TOOL");
    }
    if questions_enabled && !tools.iter().any(|tool| tool == "AskUserQuestion") {
        return Err("QUESTION_TOOL_EXCLUDED");
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "source",
    content = "value",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum AgentLaunchFieldPreferenceV1<T> {
    InheritConfigured,
    CanonicalDefault,
    Explicit(T),
}

// Manual impl: the default source is `inherit_configured` for every `T`,
// including explicit value types that have no meaningful `Default`.
// A derived `Default` would add a `T: Default` bound; the inherit variant
// needs none, so the impl stays manual (`AgentAccountSelectionV1` has no Default).
#[allow(clippy::derivable_impls)]
impl<T> Default for AgentLaunchFieldPreferenceV1<T> {
    fn default() -> Self {
        AgentLaunchFieldPreferenceV1::InheritConfigured
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AgentLaunchPreferencesV1 {
    pub schema_version: u8,
    #[serde(default)]
    pub provider: AgentLaunchFieldPreferenceV1<ChatAgentProvider>,
    #[serde(default)]
    pub model: AgentLaunchFieldPreferenceV1<String>,
    #[serde(default)]
    pub permission_mode: AgentLaunchFieldPreferenceV1<String>,
    #[serde(default)]
    pub effort: AgentLaunchFieldPreferenceV1<String>,
    /// Additive provider-account choice. Absent on the wire means
    /// `inherit_configured` (the effective per-agent assignment), so an
    /// account-less payload is unchanged.
    #[serde(default)]
    pub account: AgentLaunchFieldPreferenceV1<AgentAccountSelectionV1>,
}

impl Default for AgentLaunchPreferencesV1 {
    fn default() -> Self {
        Self {
            schema_version: AGENT_LAUNCH_SCHEMA_VERSION,
            provider: AgentLaunchFieldPreferenceV1::InheritConfigured,
            model: AgentLaunchFieldPreferenceV1::InheritConfigured,
            permission_mode: AgentLaunchFieldPreferenceV1::InheritConfigured,
            effort: AgentLaunchFieldPreferenceV1::InheritConfigured,
            account: AgentLaunchFieldPreferenceV1::InheritConfigured,
        }
    }
}

impl AgentLaunchPreferencesV1 {
    pub fn validate(&self) -> Result<(), G5Error> {
        if self.schema_version != AGENT_LAUNCH_SCHEMA_VERSION {
            return Err(launch_validation_error(
                "schemaVersion",
                "unsupported_agent_launch_schema",
                "agentLaunch.schemaVersion must be 1",
            ));
        }
        validate_explicit_string("model", &self.model)?;
        validate_explicit_string("permissionMode", &self.permission_mode)?;
        validate_explicit_string("effort", &self.effort)?;
        if let AgentLaunchFieldPreferenceV1::Explicit(AgentAccountSelectionV1::Account {
            account_id,
        }) = &self.account
        {
            if account_id.is_nil() {
                return Err(launch_validation_error(
                    "account",
                    "invalid_agent_account_id",
                    "agentLaunch.account explicit accountId must not be nil",
                ));
            }
        }
        Ok(())
    }
}

/// Browser-to-API launch envelope used after the user confirms a preview.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AgentLaunchSelectionV1 {
    pub preferences: AgentLaunchPreferencesV1,
    pub confirmed_resolution_id: String,
}

/// Durable task snapshot written after the API confirms a launch preview.
///
/// Preferences preserve the user's requested source semantics for audit and
/// future display. `resolved` is the immutable execution contract workers must
/// follow; workers never resolve the preferences again.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PersistedAgentLaunchV1 {
    pub preferences: AgentLaunchPreferencesV1,
    pub resolved: ResolvedAgentExecutionV1,
}

impl PersistedAgentLaunchV1 {
    pub fn validate(&self) -> Result<(), G5Error> {
        self.preferences.validate()?;
        self.resolved.validate()
    }
}

impl AgentLaunchSelectionV1 {
    pub fn validate(&self) -> Result<(), G5Error> {
        self.preferences.validate()?;
        if !self.confirmed_resolution_id.starts_with("alr_") {
            return Err(launch_validation_error(
                "confirmedResolutionId",
                "invalid_agent_resolution_id",
                "confirmedResolutionId must be a server-issued agent launch resolution id",
            ));
        }
        Ok(())
    }
}

fn validate_explicit_string(
    field: &str,
    preference: &AgentLaunchFieldPreferenceV1<String>,
) -> Result<(), G5Error> {
    if matches!(preference, AgentLaunchFieldPreferenceV1::Explicit(value) if value.trim().is_empty())
    {
        return Err(launch_validation_error(
            field,
            "blank_explicit_agent_launch_value",
            format!("agentLaunch.{field} explicit value must not be blank"),
        ));
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentLaunchProvenanceV1 {
    Explicit,
    Configured,
    CanonicalDefault,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ResolvedAgentLaunchFieldV1<T> {
    pub value: T,
    pub provenance: AgentLaunchProvenanceV1,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub config_key: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentAuthModeV1 {
    NoCredential,
    OpenRouterApiKey,
    OpenAiApiKey,
    ChatGptSubscription,
    AnthropicApiKey,
    AnthropicBearer,
    AnthropicOauth,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentEgressModeV1 {
    NoNetwork,
    /// Historical stored launch records only; new launches require direct egress.
    HostLlmProxy,
    DirectProvider,
}

/// Server-owned provider route. Clients display this value and never derive it
/// from base URLs, credentials, or provider-prefixed model identifiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentProviderRouteV1 {
    None,
    Openrouter,
    OpenaiDirect,
    AnthropicDirect,
    CompatibleEndpoint,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ResolvedAgentExecutionV1 {
    pub schema_version: u8,
    pub resolution_id: String,
    pub config_revision: String,
    pub catalog_version: String,
    pub provider: ResolvedAgentLaunchFieldV1<ChatAgentProvider>,
    pub provider_route: AgentProviderRouteV1,
    pub model: ResolvedAgentLaunchFieldV1<String>,
    pub upstream_model: String,
    pub permission_mode: ResolvedAgentLaunchFieldV1<String>,
    pub effort: ResolvedAgentLaunchFieldV1<String>,
    pub auth_mode: AgentAuthModeV1,
    pub egress_mode: AgentEgressModeV1,
    pub credential_ready: bool,
    pub requires_interactive_auth: bool,
    /// Frozen provider-account identity, serialized only when an account was
    /// selected so every account-less resolution re-hashes to its existing
    /// id. The account name is never frozen.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account_id: Option<Uuid>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account_kind: Option<ProviderAccountKind>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account_scope: Option<ProviderAccountScope>,
}

impl ResolvedAgentExecutionV1 {
    /// The frozen account triple when an account was selected.
    #[must_use]
    pub fn account(&self) -> Option<AgentLaunchAccountInput> {
        match (self.account_id, self.account_kind, self.account_scope) {
            (Some(id), Some(kind), Some(scope)) => {
                Some(AgentLaunchAccountInput { id, kind, scope })
            }
            _ => None,
        }
    }

    pub fn validate(&self) -> Result<(), G5Error> {
        if self.schema_version != AGENT_LAUNCH_SCHEMA_VERSION
            || self.catalog_version != AGENT_LAUNCH_CATALOG_VERSION
        {
            return Err(launch_validation_error(
                "resolvedAgentExecution",
                "unsupported_resolved_agent_execution",
                "resolved agent execution version is not supported",
            ));
        }
        validate_provider_controls(
            self.provider.value,
            &self.permission_mode.value,
            &self.effort.value,
            true,
        )?;
        match (self.account_id, self.account_kind, self.account_scope) {
            (None, None, None) => {}
            (Some(id), Some(kind), Some(_)) => {
                if id.is_nil() {
                    return Err(launch_validation_error(
                        "accountId",
                        "invalid_agent_account_id",
                        "resolved accountId must not be nil",
                    ));
                }
                if !kind.supports_agent(self.provider.value) {
                    return Err(launch_validation_error(
                        "accountKind",
                        "account_kind_mismatch",
                        format!(
                            "{} accounts cannot be used with {}",
                            kind.as_str(),
                            self.provider.value.as_str()
                        ),
                    ));
                }
            }
            _ => {
                return Err(launch_validation_error(
                    "account",
                    "agent_account_incomplete",
                    "resolved accountId, accountKind, and accountScope must be set together",
                ));
            }
        }
        let expected_egress = match self.auth_mode {
            AgentAuthModeV1::NoCredential => AgentEgressModeV1::NoNetwork,
            AgentAuthModeV1::ChatGptSubscription => AgentEgressModeV1::DirectProvider,
            _ => AgentEgressModeV1::DirectProvider,
        };
        if self.egress_mode != expected_egress {
            return Err(launch_validation_error(
                "egressMode",
                "agent_egress_auth_mismatch",
                "resolved auth and egress modes are incompatible",
            ));
        }
        if self.auth_mode == AgentAuthModeV1::ChatGptSubscription
            && self.provider.value != ChatAgentProvider::Codex
        {
            return Err(launch_validation_error(
                "authMode",
                "agent_auth_harness_mismatch",
                "ChatGPT subscription auth is valid only for Codex",
            ));
        }
        if matches!(
            self.auth_mode,
            AgentAuthModeV1::AnthropicBearer | AgentAuthModeV1::AnthropicOauth
        ) && self.provider.value != ChatAgentProvider::ClaudeCode
        {
            return Err(launch_validation_error(
                "authMode",
                "agent_auth_harness_mismatch",
                "Anthropic bearer and OAuth modes are valid only for Claude Code",
            ));
        }
        if self.auth_mode == AgentAuthModeV1::NoCredential {
            return Err(launch_validation_error(
                "authMode",
                "agent_auth_harness_mismatch",
                "Claude Code and Codex require provider authentication",
            ));
        }
        let route_is_coherent = matches!(
            (
                self.provider.value,
                self.provider_route,
                self.auth_mode,
                self.egress_mode,
            ),
            (
                ChatAgentProvider::Codex | ChatAgentProvider::ClaudeCode,
                AgentProviderRouteV1::Openrouter,
                AgentAuthModeV1::OpenRouterApiKey,
                AgentEgressModeV1::DirectProvider,
            ) | (
                ChatAgentProvider::Codex,
                AgentProviderRouteV1::OpenaiDirect,
                AgentAuthModeV1::OpenAiApiKey,
                AgentEgressModeV1::DirectProvider,
            ) | (
                ChatAgentProvider::Codex,
                AgentProviderRouteV1::OpenaiDirect,
                AgentAuthModeV1::ChatGptSubscription,
                AgentEgressModeV1::DirectProvider,
            ) | (
                ChatAgentProvider::Codex,
                AgentProviderRouteV1::CompatibleEndpoint,
                AgentAuthModeV1::OpenAiApiKey,
                AgentEgressModeV1::DirectProvider,
            ) | (
                ChatAgentProvider::ClaudeCode,
                AgentProviderRouteV1::AnthropicDirect,
                AgentAuthModeV1::AnthropicApiKey
                    | AgentAuthModeV1::AnthropicBearer
                    | AgentAuthModeV1::AnthropicOauth,
                AgentEgressModeV1::DirectProvider,
            ) | (
                ChatAgentProvider::ClaudeCode,
                AgentProviderRouteV1::CompatibleEndpoint,
                AgentAuthModeV1::AnthropicApiKey | AgentAuthModeV1::AnthropicBearer,
                AgentEgressModeV1::DirectProvider,
            )
        );
        if !route_is_coherent {
            return Err(launch_validation_error(
                "providerRoute",
                "agent_provider_route_mismatch",
                "resolved provider route is incompatible with provider, auth, or egress",
            ));
        }
        if self.resolution_id != resolution_id_for(self)? {
            return Err(launch_validation_error(
                "resolutionId",
                "agent_resolution_hash_mismatch",
                "resolved agent execution hash does not match its contents",
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AgentExecutionReceiptV1 {
    pub schema_version: u8,
    pub resolution_id: String,
    pub provider: ChatAgentProvider,
    pub provider_route: AgentProviderRouteV1,
    pub model: String,
    pub upstream_model: String,
    pub permission_mode: String,
    pub effort: String,
    pub auth_mode: AgentAuthModeV1,
    pub egress_mode: AgentEgressModeV1,
}

impl AgentExecutionReceiptV1 {
    #[must_use]
    pub fn from_resolution(resolution: &ResolvedAgentExecutionV1) -> Self {
        Self {
            schema_version: AGENT_LAUNCH_SCHEMA_VERSION,
            resolution_id: resolution.resolution_id.clone(),
            provider: resolution.provider.value,
            provider_route: resolution.provider_route,
            model: resolution.model.value.clone(),
            upstream_model: resolution.upstream_model.clone(),
            permission_mode: resolution.permission_mode.value.clone(),
            effort: resolution.effort.value.clone(),
            auth_mode: resolution.auth_mode,
            egress_mode: resolution.egress_mode,
        }
    }

    pub fn validate_against(&self, resolution: &ResolvedAgentExecutionV1) -> Result<(), G5Error> {
        let expected = Self::from_resolution(resolution);
        if self != &expected {
            return Err(launch_validation_error(
                "agentExecutionReceipt",
                "agent_execution_receipt_mismatch",
                "worker execution receipt does not match the frozen resolution",
            ));
        }
        Ok(())
    }
}

/// Already-visibility-checked provider account the caller selected for this
/// resolution. The resolver copies it verbatim into the frozen resolution; the
/// caller has already projected the account into `effective_settings` and
/// `effective_secrets`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AgentLaunchAccountInput {
    pub id: Uuid,
    pub kind: ProviderAccountKind,
    pub scope: ProviderAccountScope,
}

pub struct AgentLaunchResolutionInput<'a> {
    pub preferences: &'a AgentLaunchPreferencesV1,
    pub effective_settings: &'a Value,
    pub effective_secrets: &'a HashMap<String, String>,
    pub config_revision: &'a str,
    pub account: Option<AgentLaunchAccountInput>,
}

pub fn resolve_agent_launch(
    input: AgentLaunchResolutionInput<'_>,
) -> Result<ResolvedAgentExecutionV1, G5Error> {
    resolve_agent_launch_with_credential_policy(input, true)
}

/// Resolve the complete launch projection for pre-confirmation UX without
/// treating an absent credential as a resolution failure. Invalid
/// provider/auth combinations still fail closed; only credential readiness is
/// reported instead of enforced.
pub fn preview_agent_launch(
    input: AgentLaunchResolutionInput<'_>,
) -> Result<ResolvedAgentExecutionV1, G5Error> {
    resolve_agent_launch_with_credential_policy(input, false)
}

/// Resolve only the launch provider from the preferences and the effective
/// settings, using the resolver's own precedence (explicit, then the
/// configured `default_agent_provider`, then the canonical Claude Code
/// default). The config service uses it to pick the provider-namespaced
/// account assignment before projecting an account into the effective config
/// (external_agent_provider_contract D11); it is the same precedence the full
/// resolution applies, not a second implementation.
pub fn resolve_launch_provider(
    preferences: &AgentLaunchPreferencesV1,
    settings: &Value,
) -> Result<ChatAgentProvider, G5Error> {
    Ok(resolve_provider(&preferences.provider, settings)?.value)
}

fn resolve_agent_launch_with_credential_policy(
    input: AgentLaunchResolutionInput<'_>,
    require_credential: bool,
) -> Result<ResolvedAgentExecutionV1, G5Error> {
    input.preferences.validate()?;
    let settings = input.effective_settings;
    let provider = resolve_provider(&input.preferences.provider, settings)?;
    let route = resolve_route(provider.value, settings);
    let model = resolve_model(&input.preferences.model, provider.value, route, settings)?;
    let permission_mode = resolve_permission_mode(
        &input.preferences.permission_mode,
        provider_setting_key(provider.value, "permission_mode"),
        settings,
        canonical_permission_mode(provider.value),
    )?;
    let effort = resolve_effort(
        &input.preferences.effort,
        provider.value,
        &model.value,
        settings,
    )?;
    validate_provider_controls(provider.value, &permission_mode.value, &effort.value, false)?;
    if crate::model_catalog::model_efforts(provider.value, &model.value)
        .is_some_and(|controls| !controls.efforts.contains(&effort.value.as_str()))
    {
        return Err(launch_validation_error(
            "effort",
            "unsupported_agent_model_effort",
            format!("{} does not support effort {}", model.value, effort.value),
        ));
    }
    let (auth_mode, egress_mode, credential_ready, requires_interactive_auth) = resolve_auth(
        provider.value,
        route,
        settings,
        input.effective_secrets,
        require_credential,
    )?;
    let upstream_model = upstream_model(provider.value, route, &model.value);
    let mut resolved = ResolvedAgentExecutionV1 {
        schema_version: AGENT_LAUNCH_SCHEMA_VERSION,
        resolution_id: String::new(),
        config_revision: input.config_revision.to_string(),
        catalog_version: AGENT_LAUNCH_CATALOG_VERSION.to_string(),
        provider,
        provider_route: route,
        model,
        upstream_model,
        permission_mode,
        effort,
        auth_mode,
        egress_mode,
        credential_ready,
        requires_interactive_auth,
        account_id: input.account.map(|account| account.id),
        account_kind: input.account.map(|account| account.kind),
        account_scope: input.account.map(|account| account.scope),
    };
    resolved.resolution_id = resolution_id_for(&resolved)?;
    resolved.validate()?;
    Ok(resolved)
}

fn resolve_route(provider: ChatAgentProvider, settings: &Value) -> AgentProviderRouteV1 {
    let key = match provider {
        ChatAgentProvider::Codex => OPENAI_BASE_URL_ENV,
        ChatAgentProvider::ClaudeCode => ANTHROPIC_BASE_URL_ENV,
    };
    let base_url = configured_string(settings, key);
    let openrouter = match provider {
        ChatAgentProvider::Codex => matches!(
            crate::codex_models::codex_model_provider(base_url, None),
            CodexModelProvider::OpenRouter
        ),
        ChatAgentProvider::ClaudeCode => matches!(
            claude_model_provider_from_anthropic_base_url(base_url),
            ClaudeModelProvider::OpenRouter
        ),
    };
    if openrouter {
        AgentProviderRouteV1::Openrouter
    } else {
        match provider {
            ChatAgentProvider::Codex if codex_route_is_openai(settings) => {
                AgentProviderRouteV1::OpenaiDirect
            }
            ChatAgentProvider::ClaudeCode if claude_route_is_anthropic(settings) => {
                AgentProviderRouteV1::AnthropicDirect
            }
            ChatAgentProvider::Codex | ChatAgentProvider::ClaudeCode => {
                AgentProviderRouteV1::CompatibleEndpoint
            }
        }
    }
}

fn resolve_provider(
    preference: &AgentLaunchFieldPreferenceV1<ChatAgentProvider>,
    settings: &Value,
) -> Result<ResolvedAgentLaunchFieldV1<ChatAgentProvider>, G5Error> {
    let (value, provenance, config_key) = match preference {
        AgentLaunchFieldPreferenceV1::Explicit(value) => {
            (*value, AgentLaunchProvenanceV1::Explicit, None)
        }
        AgentLaunchFieldPreferenceV1::CanonicalDefault => (
            ChatAgentProvider::ClaudeCode,
            AgentLaunchProvenanceV1::CanonicalDefault,
            None,
        ),
        AgentLaunchFieldPreferenceV1::InheritConfigured => {
            match configured_string(settings, "default_agent_provider")
                .and_then(|raw| raw.parse::<ChatAgentProvider>().ok())
            {
                Some(provider) => (
                    provider,
                    AgentLaunchProvenanceV1::Configured,
                    Some("default_agent_provider".to_string()),
                ),
                None => (
                    ChatAgentProvider::ClaudeCode,
                    AgentLaunchProvenanceV1::CanonicalDefault,
                    None,
                ),
            }
        }
    };
    Ok(ResolvedAgentLaunchFieldV1 {
        value,
        provenance,
        config_key,
    })
}

fn resolve_model(
    preference: &AgentLaunchFieldPreferenceV1<String>,
    provider: ChatAgentProvider,
    route: AgentProviderRouteV1,
    settings: &Value,
) -> Result<ResolvedAgentLaunchFieldV1<String>, G5Error> {
    let config_key = match provider {
        ChatAgentProvider::Codex => "OPENAI_MODEL",
        ChatAgentProvider::ClaudeCode => "ANTHROPIC_MODEL",
    };
    let canonical = canonical_model(provider);
    let mut field = resolve_string_field(preference, config_key, settings, &canonical)?;
    let requested_model = field.value.clone();
    if provider == ChatAgentProvider::Codex && route == AgentProviderRouteV1::Openrouter {
        let trimmed = requested_model.trim();
        if codex_model_catalog()
            .iter()
            .any(|model| model.direct == trimmed && model.openrouter.is_none())
        {
            return Err(launch_validation_error(
                "model",
                "agent_model_unavailable_on_provider_route",
                format!("{trimmed} is not available through OpenRouter"),
            ));
        }
    }
    // Every Claude family's direct forms (plain and saved `[1m]`) name the same
    // 1M model since 2026-11-09, so each collapses to the family's OpenRouter
    // slug on that route; the former direct-only refusal guarded a plain
    // 200K pin against silent substitution by the 1M variant, a distinction
    // the catalog no longer carries.
    field.value = logical_agent_model(provider, &requested_model);
    Ok(field)
}

fn resolve_effort(
    preference: &AgentLaunchFieldPreferenceV1<String>,
    provider: ChatAgentProvider,
    model: &str,
    settings: &Value,
) -> Result<ResolvedAgentLaunchFieldV1<String>, G5Error> {
    let key = provider_setting_key(provider, "effort");
    let canonical = match provider {
        ChatAgentProvider::Codex => codex_default_effort_for_model(model),
        ChatAgentProvider::ClaudeCode => claude_default_effort_for_model(model),
    };
    // A provider-wide default effort cannot create a control the model lacks.
    if canonical == "none" && !matches!(preference, AgentLaunchFieldPreferenceV1::Explicit(_)) {
        return resolve_string_field(
            &AgentLaunchFieldPreferenceV1::CanonicalDefault,
            key,
            settings,
            canonical,
        );
    }
    resolve_string_field(preference, key, settings, canonical)
}

/// Keep a model/effort pair inside the generated provider catalog. Known
/// models clamp an absent or unsupported prior effort to their catalog
/// default; unknown model pins preserve a non-blank explicit effort.
#[must_use]
pub fn clamp_agent_effort_for_model(
    provider: ChatAgentProvider,
    model: &str,
    prior_effort: Option<&str>,
) -> String {
    let prior_effort = prior_effort
        .map(str::trim)
        .filter(|value| !value.is_empty());
    if let Some(controls) = crate::model_catalog::model_efforts(provider, model) {
        return prior_effort
            .filter(|effort| controls.efforts.contains(effort))
            .unwrap_or(controls.default_effort)
            .to_string();
    }
    prior_effort
        .unwrap_or_else(|| match provider {
            ChatAgentProvider::ClaudeCode => claude_default_effort_for_model(model),
            ChatAgentProvider::Codex => codex_default_effort_for_model(model),
        })
        .to_string()
}

fn resolve_string_field(
    preference: &AgentLaunchFieldPreferenceV1<String>,
    config_key: &str,
    settings: &Value,
    canonical: &str,
) -> Result<ResolvedAgentLaunchFieldV1<String>, G5Error> {
    let resolved = match preference {
        AgentLaunchFieldPreferenceV1::Explicit(value) => ResolvedAgentLaunchFieldV1 {
            value: value.trim().to_string(),
            provenance: AgentLaunchProvenanceV1::Explicit,
            config_key: None,
        },
        AgentLaunchFieldPreferenceV1::CanonicalDefault => ResolvedAgentLaunchFieldV1 {
            value: canonical.to_string(),
            provenance: AgentLaunchProvenanceV1::CanonicalDefault,
            config_key: None,
        },
        AgentLaunchFieldPreferenceV1::InheritConfigured => {
            if let Some(value) = configured_string(settings, config_key) {
                ResolvedAgentLaunchFieldV1 {
                    value: value.to_string(),
                    provenance: AgentLaunchProvenanceV1::Configured,
                    config_key: Some(config_key.to_string()),
                }
            } else {
                ResolvedAgentLaunchFieldV1 {
                    value: canonical.to_string(),
                    provenance: AgentLaunchProvenanceV1::CanonicalDefault,
                    config_key: None,
                }
            }
        }
    };
    if resolved.value.is_empty() {
        return Err(launch_validation_error(
            config_key,
            "blank_resolved_agent_launch_value",
            format!("resolved {config_key} must not be blank"),
        ));
    }
    Ok(resolved)
}

fn resolve_permission_mode(
    preference: &AgentLaunchFieldPreferenceV1<String>,
    config_key: &str,
    settings: &Value,
    canonical: &str,
) -> Result<ResolvedAgentLaunchFieldV1<String>, G5Error> {
    if matches!(preference, AgentLaunchFieldPreferenceV1::InheritConfigured)
        && configured_string(settings, config_key).is_none()
    {
        return Err(launch_validation_error(
            "permissionMode",
            "missing_agent_permission_mode",
            format!("{config_key} must be configured or permissionMode must be selected"),
        ));
    }
    resolve_string_field(preference, config_key, settings, canonical)
}

fn resolve_auth(
    provider: ChatAgentProvider,
    route: AgentProviderRouteV1,
    settings: &Value,
    secrets: &HashMap<String, String>,
    require_credential: bool,
) -> Result<(AgentAuthModeV1, AgentEgressModeV1, bool, bool), G5Error> {
    match (provider, route) {
        (ChatAgentProvider::Codex, AgentProviderRouteV1::Openrouter) => Ok((
            AgentAuthModeV1::OpenRouterApiKey,
            AgentEgressModeV1::DirectProvider,
            resolve_credential_readiness(secrets, OPENROUTER_API_KEY_ENV, require_credential)?,
            false,
        )),
        (
            ChatAgentProvider::Codex,
            AgentProviderRouteV1::OpenaiDirect | AgentProviderRouteV1::CompatibleEndpoint,
        ) => {
            // `codex_subscription_enabled` defaults ON: on the direct OpenAI
            // route an ABSENT preference selects task-bound ChatGPT subscription
            // auth, matching the settings-page checkbox default and the
            // @ref system_invariants/external_agent_provider_contract#D11
            // "subscription by default unless effectively false" contract. Only
            // an effective `false` routes the direct URL to an API key. A stored
            // NON-boolean is rejected rather than silently defaulted, so a
            // legacy `"false"` string that predates the config_service
            // boolean-only write guard cannot flip from the API-key path to
            // subscription/pending-auth. Absent -> true; Some(bool) -> that bool;
            // Some(non-bool) -> validation error.
            let subscription_enabled = if route == AgentProviderRouteV1::OpenaiDirect {
                match settings.get(CODEX_SUBSCRIPTION_ENABLED_SETTING) {
                    None => true,
                    Some(Value::Bool(enabled)) => *enabled,
                    Some(_) => {
                        return Err(launch_validation_error(
                            "settings.codex_subscription_enabled",
                            "invalid_setting_type",
                            "codex_subscription_enabled must be a boolean",
                        ));
                    }
                }
            } else {
                false
            };
            if subscription_enabled {
                Ok((
                    AgentAuthModeV1::ChatGptSubscription,
                    AgentEgressModeV1::DirectProvider,
                    false,
                    true,
                ))
            } else {
                Ok((
                    AgentAuthModeV1::OpenAiApiKey,
                    AgentEgressModeV1::DirectProvider,
                    resolve_credential_readiness(secrets, OPENAI_API_KEY_ENV, require_credential)?,
                    false,
                ))
            }
        }
        (ChatAgentProvider::ClaudeCode, AgentProviderRouteV1::Openrouter) => {
            if has_secret(secrets, CLAUDE_CODE_OAUTH_TOKEN_ENV) {
                return Err(launch_validation_error(
                    "authMode",
                    "mixed_claude_oauth_openrouter_route",
                    "Anthropic OAuth is valid only for direct Claude Code",
                ));
            }
            Ok((
                AgentAuthModeV1::OpenRouterApiKey,
                AgentEgressModeV1::DirectProvider,
                resolve_credential_readiness(
                    secrets,
                    ANTHROPIC_AUTH_TOKEN_ENV,
                    require_credential,
                )?,
                false,
            ))
        }
        (ChatAgentProvider::ClaudeCode, AgentProviderRouteV1::AnthropicDirect) => {
            if has_secret(secrets, CLAUDE_CODE_OAUTH_TOKEN_ENV) {
                Ok((
                    AgentAuthModeV1::AnthropicOauth,
                    AgentEgressModeV1::DirectProvider,
                    true,
                    false,
                ))
            } else {
                resolve_anthropic_bearer_or_api_key(secrets, require_credential)
            }
        }
        (ChatAgentProvider::ClaudeCode, AgentProviderRouteV1::CompatibleEndpoint) => {
            if has_secret(secrets, CLAUDE_CODE_OAUTH_TOKEN_ENV) {
                return Err(launch_validation_error(
                    "authMode",
                    "mixed_claude_oauth_compatible_route",
                    "Anthropic OAuth is valid only for direct Anthropic Claude Code",
                ));
            }
            resolve_anthropic_bearer_or_api_key(secrets, require_credential)
        }
        _ => Err(launch_validation_error(
            "provider",
            "unsupported_agent_launch_provider",
            "agent launch requires Claude Code or Codex",
        )),
    }
}

fn resolve_anthropic_bearer_or_api_key(
    secrets: &HashMap<String, String>,
    require_credential: bool,
) -> Result<(AgentAuthModeV1, AgentEgressModeV1, bool, bool), G5Error> {
    if has_secret(secrets, ANTHROPIC_AUTH_TOKEN_ENV) {
        Ok((
            AgentAuthModeV1::AnthropicBearer,
            AgentEgressModeV1::DirectProvider,
            true,
            false,
        ))
    } else {
        Ok((
            AgentAuthModeV1::AnthropicApiKey,
            AgentEgressModeV1::DirectProvider,
            resolve_credential_readiness(secrets, ANTHROPIC_API_KEY_ENV, require_credential)?,
            false,
        ))
    }
}

fn codex_route_is_openai(settings: &Value) -> bool {
    let Some(base_url) = configured_string(settings, OPENAI_BASE_URL_ENV) else {
        return true;
    };
    let normalized = base_url.trim_end_matches('/').to_ascii_lowercase();
    normalized == "https://api.openai.com" || normalized.starts_with("https://api.openai.com/")
}

fn claude_route_is_anthropic(settings: &Value) -> bool {
    let Some(base_url) = configured_string(settings, ANTHROPIC_BASE_URL_ENV) else {
        return true;
    };
    let normalized = base_url.trim_end_matches('/').to_ascii_lowercase();
    normalized == "https://api.anthropic.com"
        || normalized.starts_with("https://api.anthropic.com/")
}

fn resolve_credential_readiness(
    secrets: &HashMap<String, String>,
    key: &str,
    require_credential: bool,
) -> Result<bool, G5Error> {
    if has_secret(secrets, key) {
        Ok(true)
    } else if !require_credential {
        Ok(false)
    } else {
        Err(launch_validation_error(
            "authMode",
            "missing_llm_credential",
            format!("resolved agent route requires {key}"),
        ))
    }
}

fn has_secret(secrets: &HashMap<String, String>, key: &str) -> bool {
    secrets
        .get(key)
        .map(String::as_str)
        .map(str::trim)
        .is_some_and(|value| !value.is_empty())
}

fn configured_string<'a>(settings: &'a Value, key: &str) -> Option<&'a str> {
    settings
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

fn canonical_model(provider: ChatAgentProvider) -> String {
    match provider {
        ChatAgentProvider::Codex => {
            default_codex_chat_model_for_provider(CodexModelProvider::OpenAiDirect).to_string()
        }
        ChatAgentProvider::ClaudeCode => {
            default_claude_chat_model_for_provider(ClaudeModelProvider::DirectAnthropic).to_string()
        }
    }
}

pub fn logical_agent_model(provider: ChatAgentProvider, model: &str) -> String {
    match provider {
        ChatAgentProvider::Codex => {
            normalize_codex_model_for_provider(model, CodexModelProvider::OpenAiDirect)
        }
        ChatAgentProvider::ClaudeCode => {
            normalize_claude_model_for_provider(model, ClaudeModelProvider::DirectAnthropic)
        }
    }
}

fn upstream_model(provider: ChatAgentProvider, route: AgentProviderRouteV1, model: &str) -> String {
    match (provider, route) {
        (
            ChatAgentProvider::Codex,
            AgentProviderRouteV1::OpenaiDirect | AgentProviderRouteV1::CompatibleEndpoint,
        ) => normalize_codex_model_for_provider(model, CodexModelProvider::OpenAiDirect),
        (ChatAgentProvider::Codex, AgentProviderRouteV1::Openrouter) => {
            normalize_codex_model_for_provider(model, CodexModelProvider::OpenRouter)
        }
        (
            ChatAgentProvider::ClaudeCode,
            AgentProviderRouteV1::AnthropicDirect | AgentProviderRouteV1::CompatibleEndpoint,
        ) => normalize_claude_model_for_provider(model, ClaudeModelProvider::DirectAnthropic),
        (ChatAgentProvider::ClaudeCode, AgentProviderRouteV1::Openrouter) => {
            normalize_claude_model_for_provider(model, ClaudeModelProvider::OpenRouter)
        }
        _ => model.to_string(),
    }
}

fn provider_setting_key(provider: ChatAgentProvider, suffix: &str) -> &'static str {
    match (provider, suffix) {
        (ChatAgentProvider::ClaudeCode, "permission_mode") => "claude_permission_mode",
        (ChatAgentProvider::ClaudeCode, "effort") => "claude_effort",
        (ChatAgentProvider::Codex, "permission_mode") => "codex_permission_mode",
        (ChatAgentProvider::Codex, "effort") => "codex_effort",
        _ => unreachable!("unsupported provider setting"),
    }
}

fn canonical_permission_mode(provider: ChatAgentProvider) -> &'static str {
    match provider {
        ChatAgentProvider::ClaudeCode => CLAUDE_DEFAULT_CHAT_MODE,
        ChatAgentProvider::Codex => CODEX_DEFAULT_CHAT_MODE,
    }
}

fn validate_provider_controls(
    provider: ChatAgentProvider,
    permission_mode: &str,
    effort: &str,
    allow_historical_modes: bool,
) -> Result<(), G5Error> {
    let (modes, efforts) = match provider {
        ChatAgentProvider::ClaudeCode => (CLAUDE_CHAT_MODES, CLAUDE_EFFORT_ORDER),
        ChatAgentProvider::Codex => (CODEX_CHAT_MODES, CODEX_EFFORT_ORDER),
    };
    if !modes.contains(&permission_mode)
        && !(allow_historical_modes
            && match provider {
                ChatAgentProvider::ClaudeCode => {
                    crate::claude_models::CLAUDE_NATIVE_MODES.contains(&permission_mode)
                }
                ChatAgentProvider::Codex => {
                    crate::codex_models::CODEX_NATIVE_MODES.contains(&permission_mode)
                        || crate::codex_models::CODEX_RETIRED_CHAT_MODES.contains(&permission_mode)
                }
            })
    {
        return Err(launch_validation_error(
            "permissionMode",
            "unsupported_agent_permission_mode",
            format!(
                "{permission_mode} is not supported by {}",
                provider.as_str()
            ),
        ));
    }
    if !efforts.contains(&effort)
        && !(provider == ChatAgentProvider::ClaudeCode && effort == "none")
    {
        return Err(launch_validation_error(
            "effort",
            "unsupported_agent_effort",
            format!("{effort} is not supported by {}", provider.as_str()),
        ));
    }
    Ok(())
}

fn resolution_id_for(resolution: &ResolvedAgentExecutionV1) -> Result<String, G5Error> {
    let mut value = resolution.clone();
    value.resolution_id.clear();
    let bytes = serde_json::to_vec(&value).map_err(|error| G5Error::Internal {
        message: format!("failed to serialize agent launch resolution: {error}"),
    })?;
    let digest = Sha256::digest(bytes);
    let mut encoded = String::with_capacity(digest.len() * 2);
    for byte in digest {
        write!(&mut encoded, "{byte:02x}").expect("writing to String cannot fail");
    }
    Ok(format!("alr_{encoded}"))
}

fn launch_validation_error(
    field: impl Into<String>,
    code: impl Into<String>,
    error: impl Into<String>,
) -> G5Error {
    G5Error::ValidationError {
        details: serde_json::json!({
            "field": field.into(),
            "code": code.into(),
            "error": error.into(),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn removed_agent_tools_are_refused() {
        for removed in [
            "mcp__g5-tools",
            "mcp__g5-tools__read_ontology",
            "mcp__g5-tools__*",
            "mcp__g5-tools__list_review_round",
            "mcp__g5-tools__decide_review_suggestion",
        ] {
            assert!(names_removed_agent_tool(removed), "{removed}");
        }
        for kept in ["Read", "mcp__docs__search", "mcp__g5-toolsmith__run"] {
            assert!(!names_removed_agent_tool(kept), "{kept}");
        }
    }

    fn secrets(entries: &[(&str, &str)]) -> HashMap<String, String> {
        entries
            .iter()
            .map(|(key, value)| ((*key).to_string(), (*value).to_string()))
            .collect()
    }

    fn resolve(
        preferences: &AgentLaunchPreferencesV1,
        mut settings: Value,
        secrets: HashMap<String, String>,
    ) -> Result<ResolvedAgentExecutionV1, G5Error> {
        let settings = settings
            .as_object_mut()
            .expect("agent launch test settings must be an object");
        settings
            .entry("claude_permission_mode")
            .or_insert_with(|| Value::String("default".into()));
        settings
            .entry("codex_permission_mode")
            .or_insert_with(|| Value::String("never/workspace-write".into()));
        resolve_agent_launch(AgentLaunchResolutionInput {
            preferences,
            effective_settings: &Value::Object(settings.clone()),
            effective_secrets: &secrets,
            config_revision: "cfg-test",
            account: None,
        })
    }

    fn resolve_with_account(
        preferences: &AgentLaunchPreferencesV1,
        mut settings: Value,
        secrets: HashMap<String, String>,
        account: AgentLaunchAccountInput,
    ) -> Result<ResolvedAgentExecutionV1, G5Error> {
        let settings = settings
            .as_object_mut()
            .expect("agent launch test settings must be an object");
        settings
            .entry("claude_permission_mode")
            .or_insert_with(|| Value::String("default".into()));
        settings
            .entry("codex_permission_mode")
            .or_insert_with(|| Value::String("never/workspace-write".into()));
        resolve_agent_launch(AgentLaunchResolutionInput {
            preferences,
            effective_settings: &Value::Object(settings.clone()),
            effective_secrets: &secrets,
            config_revision: "cfg-test",
            account: Some(account),
        })
    }

    #[test]
    fn model_effort_clamp_uses_catalog_and_preserves_unknown_pins() {
        assert_eq!(
            clamp_agent_effort_for_model(ChatAgentProvider::Codex, "gpt-5.5", Some("ultra")),
            "xhigh"
        );
        assert_eq!(
            clamp_agent_effort_for_model(
                ChatAgentProvider::ClaudeCode,
                "claude-sonnet-4-6",
                Some("max")
            ),
            "high"
        );
        assert_eq!(
            clamp_agent_effort_for_model(
                ChatAgentProvider::ClaudeCode,
                "acme/custom-claude",
                Some("max")
            ),
            "max"
        );
    }

    #[test]
    fn untouched_preferences_use_runtime_model_not_remembered_browser_state() {
        let resolution = resolve(
            &AgentLaunchPreferencesV1::default(),
            serde_json::json!({"ANTHROPIC_MODEL":"z-ai/glm-5.2-20260616"}),
            secrets(&[(ANTHROPIC_API_KEY_ENV, "sk-ant-test")]),
        )
        .expect("configured model resolves");
        assert_eq!(resolution.model.value, "z-ai/glm-5.2-20260616");
        assert_eq!(
            resolution.model.provenance,
            AgentLaunchProvenanceV1::Configured
        );
    }

    #[test]
    fn preview_returns_resolved_values_when_the_route_credential_is_missing() {
        let preferences = AgentLaunchPreferencesV1::default();
        let settings = serde_json::json!({
            "ANTHROPIC_MODEL":"claude-sonnet-4-6",
            "claude_permission_mode":"default"
        });
        let preview = preview_agent_launch(AgentLaunchResolutionInput {
            preferences: &preferences,
            effective_settings: &settings,
            effective_secrets: &HashMap::new(),
            config_revision: "cfg-preview",
            account: None,
        })
        .expect("missing credentials are readiness state during preview");

        assert_eq!(preview.provider.value, ChatAgentProvider::ClaudeCode);
        assert_eq!(preview.model.value, "claude-sonnet-4-6");
        assert_eq!(preview.auth_mode, AgentAuthModeV1::AnthropicApiKey);
        assert!(!preview.credential_ready);
        assert!(!preview.requires_interactive_auth);

        let strict_error = resolve_agent_launch(AgentLaunchResolutionInput {
            preferences: &preferences,
            effective_settings: &settings,
            effective_secrets: &HashMap::new(),
            config_revision: "cfg-preview",
            account: None,
        })
        .expect_err("dispatch resolution still requires the API key");
        assert!(format!("{strict_error:?}").contains("missing_llm_credential"));
    }

    #[test]
    fn inherited_permission_mode_requires_an_explicit_runtime_setting() {
        let preferences = AgentLaunchPreferencesV1::default();
        let settings = serde_json::json!({"ANTHROPIC_MODEL":"claude-sonnet-4-6"});
        let error = preview_agent_launch(AgentLaunchResolutionInput {
            preferences: &preferences,
            effective_settings: &settings,
            effective_secrets: &HashMap::new(),
            config_revision: "cfg-missing-permission",
            account: None,
        })
        .expect_err("an inherited permission mode must not become implicit elevation");
        assert!(format!("{error:?}").contains("missing_agent_permission_mode"));
    }

    #[test]
    fn canonical_default_bypasses_runtime_config_and_explicit_wins() {
        let mut preferences = AgentLaunchPreferencesV1 {
            model: AgentLaunchFieldPreferenceV1::CanonicalDefault,
            ..Default::default()
        };
        let canonical = resolve(
            &preferences,
            serde_json::json!({"ANTHROPIC_MODEL":"custom/runtime-model"}),
            secrets(&[(ANTHROPIC_API_KEY_ENV, "sk-ant-test")]),
        )
        .unwrap();
        assert_eq!(
            canonical.model.value,
            crate::claude_models::CLAUDE_DIRECT_CHAT_DEFAULT_MODEL
        );
        preferences.model = AgentLaunchFieldPreferenceV1::Explicit("custom/launch-model".into());
        let explicit = resolve(
            &preferences,
            serde_json::json!({"ANTHROPIC_MODEL":"custom/runtime-model"}),
            secrets(&[(ANTHROPIC_API_KEY_ENV, "sk-ant-test")]),
        )
        .unwrap();
        assert_eq!(explicit.model.value, "custom/launch-model");
        assert_eq!(explicit.model.provenance, AgentLaunchProvenanceV1::Explicit);
    }

    #[test]
    fn explicit_frontend_choices_are_preserved_without_runtime_default_fill() {
        let preferences = AgentLaunchPreferencesV1 {
            schema_version: AGENT_LAUNCH_SCHEMA_VERSION,
            provider: AgentLaunchFieldPreferenceV1::Explicit(ChatAgentProvider::Codex),
            model: AgentLaunchFieldPreferenceV1::Explicit("gpt-5.6-sol".into()),
            permission_mode: AgentLaunchFieldPreferenceV1::Explicit(
                "never/danger-full-access".into(),
            ),
            effort: AgentLaunchFieldPreferenceV1::Explicit("high".into()),
            account: AgentLaunchFieldPreferenceV1::InheritConfigured,
        };
        let resolved = resolve(
            &preferences,
            serde_json::json!({
                "default_agent_provider": "claude_code",
                "OPENAI_MODEL": "gpt-5.3-codex",
                "codex_permission_mode": "never/read-only",
                "codex_effort": "low"
            }),
            secrets(&[(OPENAI_API_KEY_ENV, "sk-openai-test")]),
        )
        .expect("an explicit valid launch should resolve without default substitution");

        assert_eq!(resolved.provider.value, ChatAgentProvider::Codex);
        assert_eq!(resolved.model.value, "gpt-5.6-sol");
        assert_eq!(resolved.permission_mode.value, "never/danger-full-access");
        assert_eq!(resolved.effort.value, "high");
        assert_eq!(
            resolved.provider.provenance,
            AgentLaunchProvenanceV1::Explicit
        );
        assert_eq!(resolved.model.provenance, AgentLaunchProvenanceV1::Explicit);
        assert_eq!(
            resolved.permission_mode.provenance,
            AgentLaunchProvenanceV1::Explicit
        );
        assert_eq!(
            resolved.effort.provenance,
            AgentLaunchProvenanceV1::Explicit
        );
    }

    #[test]
    fn new_launches_reject_unsupported_model_efforts_and_haiku_ignores_inherited_effort() {
        for (provider, model, unsupported) in [
            (ChatAgentProvider::ClaudeCode, "claude-haiku-4-5", "high"),
            (
                ChatAgentProvider::ClaudeCode,
                "anthropic/claude-sonnet-4.6",
                "max",
            ),
            (ChatAgentProvider::Codex, "gpt-5.5", "ultra"),
        ] {
            let mut preferences = AgentLaunchPreferencesV1 {
                provider: AgentLaunchFieldPreferenceV1::Explicit(provider),
                model: AgentLaunchFieldPreferenceV1::Explicit(model.into()),
                effort: AgentLaunchFieldPreferenceV1::Explicit(unsupported.into()),
                ..AgentLaunchPreferencesV1::default()
            };
            let error = resolve(&preferences, serde_json::json!({}), HashMap::new()).unwrap_err();
            assert!(format!("{error:?}").contains("unsupported_agent_model_effort"));
            if model == "claude-haiku-4-5" {
                preferences.effort = AgentLaunchFieldPreferenceV1::InheritConfigured;
                let resolved = resolve(
                    &preferences,
                    serde_json::json!({"claude_effort":"high"}),
                    secrets(&[(ANTHROPIC_API_KEY_ENV, "fixture-key")]),
                )
                .unwrap();
                assert_eq!(resolved.effort.value, "none");
            }
        }
    }

    #[test]
    fn unexecutable_modes_are_refused_for_new_launches_but_preserved_in_frozen_receipts() {
        for (provider, mode, supported) in [
            (
                ChatAgentProvider::Codex,
                "on-failure/workspace-write",
                "never/workspace-write",
            ),
            (
                ChatAgentProvider::Codex,
                "on-request/read-only",
                "never/read-only",
            ),
            (ChatAgentProvider::ClaudeCode, "plan", "default"),
            (ChatAgentProvider::ClaudeCode, "acceptEdits", "default"),
        ] {
            let mut preferences = AgentLaunchPreferencesV1 {
                provider: AgentLaunchFieldPreferenceV1::Explicit(provider),
                permission_mode: AgentLaunchFieldPreferenceV1::Explicit(mode.into()),
                ..AgentLaunchPreferencesV1::default()
            };
            let credentials = secrets(&[(ANTHROPIC_API_KEY_ENV, "fixture-key")]);
            assert!(format!(
                "{:?}",
                resolve(&preferences, serde_json::json!({}), credentials.clone()).unwrap_err()
            )
            .contains("unsupported_agent_permission_mode"));
            preferences.permission_mode = AgentLaunchFieldPreferenceV1::InheritConfigured;
            let key = match provider {
                ChatAgentProvider::Codex => "codex_permission_mode",
                ChatAgentProvider::ClaudeCode => "claude_permission_mode",
            };
            assert!(
                format!(
                    "{:?}",
                    resolve(
                        &preferences,
                        serde_json::json!({key:mode}),
                        credentials.clone()
                    )
                    .unwrap_err()
                )
                .contains("unsupported_agent_permission_mode"),
                "retired configured {mode} must be refused before dispatch"
            );
            preferences.permission_mode = AgentLaunchFieldPreferenceV1::Explicit(supported.into());
            let mut historical = resolve(&preferences, serde_json::json!({}), credentials).unwrap();
            historical.permission_mode.value = mode.into();
            historical.resolution_id = resolution_id_for(&historical).unwrap();
            historical
                .validate()
                .expect("historical frozen receipt remains readable");
        }
    }

    #[test]
    fn every_native_catalog_combination_resolves_verbatim_through_api_contract() {
        let catalog: Value =
            serde_json::from_str(&crate::model_catalog_gen::render_native_provider_catalog_json())
                .unwrap();
        for (name, entry) in catalog["providers"].as_object().unwrap() {
            let provider: ChatAgentProvider =
                serde_json::from_value(serde_json::json!(name)).unwrap();
            for model in entry["models"].as_array().unwrap() {
                let id = model["id"].as_str().unwrap();
                let router = model["route"] == "openrouter";
                let settings = serde_json::json!({
                    "codex_subscription_enabled": false,
                    "OPENAI_BASE_URL": if router { "https://openrouter.ai/api/v1" } else { "https://api.openai.com/v1" },
                    "ANTHROPIC_BASE_URL": if router { "https://openrouter.ai/api" } else { "https://api.anthropic.com" },
                });
                let credentials = if router {
                    secrets(&[
                        (OPENROUTER_API_KEY_ENV, "fixture-key"),
                        (ANTHROPIC_AUTH_TOKEN_ENV, "fixture-key"),
                    ])
                } else {
                    secrets(&[
                        (OPENAI_API_KEY_ENV, "fixture-key"),
                        (ANTHROPIC_API_KEY_ENV, "fixture-key"),
                    ])
                };
                for mode in entry["modes"].as_array().unwrap() {
                    for effort in model["efforts"].as_array().unwrap() {
                        let mode = mode.as_str().unwrap();
                        let effort = effort.as_str().unwrap();
                        let preferences = AgentLaunchPreferencesV1 {
                            provider: AgentLaunchFieldPreferenceV1::Explicit(provider),
                            model: AgentLaunchFieldPreferenceV1::Explicit(id.into()),
                            permission_mode: AgentLaunchFieldPreferenceV1::Explicit(mode.into()),
                            effort: AgentLaunchFieldPreferenceV1::Explicit(effort.into()),
                            ..AgentLaunchPreferencesV1::default()
                        };
                        let resolved = resolve(&preferences, settings.clone(), credentials.clone())
                            .unwrap_or_else(|error| {
                                panic!("{name}/{id}/{mode}/{effort}: {error:?}")
                            });
                        assert_eq!(resolved.upstream_model, id, "{name}/{mode}/{effort}");
                        assert_eq!(resolved.permission_mode.value, mode);
                        assert_eq!(resolved.effort.value, effort);
                        resolved
                            .validate()
                            .expect("worker accepts the API's frozen resolution");
                    }
                }
            }
        }
    }

    #[test]
    fn codex_route_auth_matrix_is_url_coherent_and_subscription_preferred() {
        let preferences = AgentLaunchPreferencesV1 {
            provider: AgentLaunchFieldPreferenceV1::Explicit(ChatAgentProvider::Codex),
            ..Default::default()
        };

        let openrouter = resolve(
            &preferences,
            serde_json::json!({OPENAI_BASE_URL_ENV:"https://openrouter.ai/api/v1", CODEX_SUBSCRIPTION_ENABLED_SETTING:true}),
            secrets(&[(OPENROUTER_API_KEY_ENV, "sk-or-test"), (OPENAI_API_KEY_ENV, "sk-openai-unused")]),
        )
        .unwrap();
        assert_eq!(openrouter.provider_route, AgentProviderRouteV1::Openrouter);
        assert_eq!(openrouter.model.value, "gpt-5.6-sol");
        assert_eq!(openrouter.upstream_model, "openai/gpt-5.6-sol");
        assert_eq!(openrouter.auth_mode, AgentAuthModeV1::OpenRouterApiKey);
        assert_eq!(openrouter.egress_mode, AgentEgressModeV1::DirectProvider);

        let subscription = resolve(
            &preferences,
            serde_json::json!({OPENAI_BASE_URL_ENV:"https://api.openai.com/v1", CODEX_SUBSCRIPTION_ENABLED_SETTING:true}),
            secrets(&[(OPENAI_API_KEY_ENV, "sk-openai-must-not-fallback")]),
        )
        .unwrap();
        assert_eq!(
            subscription.provider_route,
            AgentProviderRouteV1::OpenaiDirect
        );
        assert_eq!(subscription.auth_mode, AgentAuthModeV1::ChatGptSubscription);
        assert_eq!(subscription.egress_mode, AgentEgressModeV1::DirectProvider);
        assert!(subscription.requires_interactive_auth);
        assert!(!subscription.credential_ready);

        // Default-on: an unset `codex_subscription_enabled` on the direct OpenAI
        // route resolves to ChatGPT subscription auth (external_agent_provider_contract
        // D11), never falling back to the API key even when one is present.
        let subscription_default = resolve(
            &preferences,
            serde_json::json!({OPENAI_BASE_URL_ENV:"https://api.openai.com/v1"}),
            secrets(&[(OPENAI_API_KEY_ENV, "sk-openai-must-not-fallback")]),
        )
        .unwrap();
        assert_eq!(
            subscription_default.provider_route,
            AgentProviderRouteV1::OpenaiDirect
        );
        assert_eq!(
            subscription_default.auth_mode,
            AgentAuthModeV1::ChatGptSubscription
        );
        assert_eq!(
            subscription_default.egress_mode,
            AgentEgressModeV1::DirectProvider
        );
        assert!(subscription_default.requires_interactive_auth);
        assert!(!subscription_default.credential_ready);

        let legacy_runtime_auth = resolve(
            &preferences,
            serde_json::json!({OPENAI_BASE_URL_ENV:"https://api.openai.com/v1", CODEX_SUBSCRIPTION_ENABLED_SETTING:true}),
            secrets(&[(crate::types::CODEX_AUTH_JSON_ENV_KEY, r#"{"tokens":{"access_token":"legacy"}}"#)]),
        )
        .unwrap();
        assert_eq!(
            legacy_runtime_auth.auth_mode,
            AgentAuthModeV1::ChatGptSubscription
        );
        assert!(legacy_runtime_auth.requires_interactive_auth);
        assert!(!legacy_runtime_auth.credential_ready);

        let api_key = resolve(
            &preferences,
            serde_json::json!({OPENAI_BASE_URL_ENV:"https://api.openai.com/v1", CODEX_SUBSCRIPTION_ENABLED_SETTING:false}),
            secrets(&[(OPENAI_API_KEY_ENV, "sk-openai")]),
        )
        .unwrap();
        assert_eq!(api_key.auth_mode, AgentAuthModeV1::OpenAiApiKey);
        assert_eq!(api_key.egress_mode, AgentEgressModeV1::DirectProvider);

        let compatible_endpoint = resolve(
            &preferences,
            serde_json::json!({OPENAI_BASE_URL_ENV:"https://llm.example.test/v1", CODEX_SUBSCRIPTION_ENABLED_SETTING:true}),
            secrets(&[(OPENAI_API_KEY_ENV, "custom-route-key")]),
        )
        .unwrap();
        assert_eq!(
            compatible_endpoint.provider_route,
            AgentProviderRouteV1::CompatibleEndpoint
        );
        assert_eq!(compatible_endpoint.auth_mode, AgentAuthModeV1::OpenAiApiKey);
        assert_eq!(
            compatible_endpoint.egress_mode,
            AgentEgressModeV1::DirectProvider
        );
    }

    #[test]
    fn codex_subscription_non_boolean_setting_is_rejected_not_defaulted() {
        // A legacy/non-boolean stored `codex_subscription_enabled` must NOT be
        // silently treated as the default-on value on the direct OpenAI route.
        // A stored `"false"` string that predates the config_service
        // boolean-only write guard previously resolved to the API-key path;
        // defaulting it to subscription would silently change auth. It must fail
        // with a validation error instead (external_agent_provider_contract D11).
        let preferences = AgentLaunchPreferencesV1 {
            provider: AgentLaunchFieldPreferenceV1::Explicit(ChatAgentProvider::Codex),
            ..Default::default()
        };
        let error = resolve(
            &preferences,
            serde_json::json!({OPENAI_BASE_URL_ENV:"https://api.openai.com/v1", CODEX_SUBSCRIPTION_ENABLED_SETTING:"false"}),
            secrets(&[(OPENAI_API_KEY_ENV, "sk-openai")]),
        )
        .unwrap_err();
        assert!(
            matches!(error, G5Error::ValidationError { .. }),
            "expected ValidationError, got {error:?}"
        );
        assert!(
            format!("{error:?}").contains(CODEX_SUBSCRIPTION_ENABLED_SETTING),
            "error should name the offending setting: {error:?}"
        );
    }

    #[test]
    fn codex_model_unavailable_on_openrouter_fails_without_fallback() {
        let preferences = AgentLaunchPreferencesV1 {
            provider: AgentLaunchFieldPreferenceV1::Explicit(ChatAgentProvider::Codex),
            model: AgentLaunchFieldPreferenceV1::Explicit("gpt-5.3-codex-spark".into()),
            ..Default::default()
        };
        let error = resolve(
            &preferences,
            serde_json::json!({OPENAI_BASE_URL_ENV:"https://openrouter.ai/api/v1"}),
            secrets(&[(OPENROUTER_API_KEY_ENV, "sk-or-test")]),
        )
        .expect_err("a direct-only model must not be silently retained or substituted");
        assert!(format!("{error:?}").contains("agent_model_unavailable_on_provider_route"));
    }

    #[test]
    fn claude_plain_and_saved_suffixed_ids_both_route_to_openrouter() {
        // Since 2026-11-09 every family's preferred direct form is its plain id
        // (the 1M window needs no suffix). Plain ids and saved `[1m]` picks name
        // the same model, so both resolve on OpenRouter to the family slug.
        use crate::claude_models as g5_claude;
        for model in [
            "claude-opus-5-5",
            "claude-opus-5",
            "claude-opus-4-8",
            "claude-fable-5-1",
            "claude-fable-5",
        ] {
            let mut preferences = AgentLaunchPreferencesV1 {
                provider: AgentLaunchFieldPreferenceV1::Explicit(ChatAgentProvider::ClaudeCode),
                model: AgentLaunchFieldPreferenceV1::Explicit(format!("{model}[1m]")),
                ..Default::default()
            };
            let settings = serde_json::json!({
                ANTHROPIC_BASE_URL_ENV: "https://openrouter.ai/api"
            });
            let credentials = secrets(&[(ANTHROPIC_AUTH_TOKEN_ENV, "sk-or-test")]);
            let router = resolve(&preferences, settings.clone(), credentials.clone())
                .expect("a saved [1m] pick names the same 1M model as the plain id");
            assert_eq!(router.provider_route, AgentProviderRouteV1::Openrouter);
            assert_eq!(router.model.value, format!("{model}[1m]"));
            assert_eq!(
                router.upstream_model,
                g5_claude::normalize_claude_model_for_provider(
                    model,
                    g5_claude::ClaudeModelProvider::OpenRouter
                )
            );

            let direct = resolve(
                &preferences,
                serde_json::json!({}),
                secrets(&[(ANTHROPIC_API_KEY_ENV, "sk-ant-test")]),
            )
            .expect("a saved [1m] variant remains valid on the direct route");
            assert_eq!(direct.model.value, format!("{model}[1m]"));

            preferences.model = AgentLaunchFieldPreferenceV1::Explicit(model.into());
            let router = resolve(&preferences, settings, credentials)
                .expect("the plain id is valid on OpenRouter");
            assert_eq!(router.provider_route, AgentProviderRouteV1::Openrouter);
            assert_eq!(router.model.value, model);
        }
        // Fable 5.2 has one form only.
        let preferences = AgentLaunchPreferencesV1 {
            provider: AgentLaunchFieldPreferenceV1::Explicit(ChatAgentProvider::ClaudeCode),
            model: AgentLaunchFieldPreferenceV1::Explicit("claude-fable-5-2".into()),
            ..Default::default()
        };
        let router = resolve(
            &preferences,
            serde_json::json!({ANTHROPIC_BASE_URL_ENV: "https://openrouter.ai/api"}),
            secrets(&[(ANTHROPIC_AUTH_TOKEN_ENV, "sk-or-test")]),
        )
        .expect("Fable 5.2 is valid on OpenRouter");
        assert_eq!(router.upstream_model, "anthropic/claude-fable-5.2");
    }

    #[test]
    fn claude_oauth_with_openrouter_fails_closed() {
        let error = resolve(
            &AgentLaunchPreferencesV1::default(),
            serde_json::json!({ANTHROPIC_BASE_URL_ENV:"https://openrouter.ai/api"}),
            secrets(&[
                (CLAUDE_CODE_OAUTH_TOKEN_ENV, "oauth-secret"),
                (ANTHROPIC_AUTH_TOKEN_ENV, "sk-or"),
            ]),
        )
        .expect_err("OAuth must not cross into OpenRouter");
        assert!(format!("{error:?}").contains("mixed_claude_oauth_openrouter_route"));
    }

    #[test]
    fn claude_direct_oauth_is_preferred_but_keeps_host_proxy_egress() {
        for base_url in [
            "https://api.anthropic.com",
            "https://api.anthropic.com/",
            "https://api.anthropic.com/v1",
        ] {
            let resolution = resolve(
                &AgentLaunchPreferencesV1::default(),
                serde_json::json!({ANTHROPIC_BASE_URL_ENV: base_url}),
                secrets(&[
                    (CLAUDE_CODE_OAUTH_TOKEN_ENV, "oauth-secret"),
                    (ANTHROPIC_API_KEY_ENV, "api-key-must-not-win"),
                ]),
            )
            .expect("direct Claude OAuth resolves");
            assert_eq!(
                resolution.provider_route,
                AgentProviderRouteV1::AnthropicDirect,
                "unexpected route for {base_url}"
            );
            assert_eq!(resolution.auth_mode, AgentAuthModeV1::AnthropicOauth);
            assert_eq!(resolution.egress_mode, AgentEgressModeV1::DirectProvider);
        }
    }

    #[test]
    fn claude_bearer_preserves_direct_compatible_and_openrouter_routes() {
        let direct_settings =
            serde_json::json!({ANTHROPIC_BASE_URL_ENV: "https://api.anthropic.com"});
        let direct = resolve(
            &AgentLaunchPreferencesV1::default(),
            direct_settings.clone(),
            secrets(&[
                (ANTHROPIC_AUTH_TOKEN_ENV, "direct-bearer"),
                (ANTHROPIC_API_KEY_ENV, "api-key-must-not-win"),
            ]),
        )
        .expect("direct Anthropic bearer resolves");
        assert_eq!(direct.provider_route, AgentProviderRouteV1::AnthropicDirect);
        assert_eq!(direct.auth_mode, AgentAuthModeV1::AnthropicBearer);
        assert_eq!(direct.egress_mode, AgentEgressModeV1::DirectProvider);

        let oauth = resolve(
            &AgentLaunchPreferencesV1::default(),
            direct_settings,
            secrets(&[
                (CLAUDE_CODE_OAUTH_TOKEN_ENV, "oauth-must-win"),
                (ANTHROPIC_AUTH_TOKEN_ENV, "direct-bearer"),
                (ANTHROPIC_API_KEY_ENV, "api-key"),
            ]),
        )
        .expect("direct Anthropic OAuth keeps precedence");
        assert_eq!(oauth.auth_mode, AgentAuthModeV1::AnthropicOauth);

        let openrouter = resolve(
            &AgentLaunchPreferencesV1::default(),
            serde_json::json!({ANTHROPIC_BASE_URL_ENV: "https://openrouter.ai/api"}),
            secrets(&[(ANTHROPIC_AUTH_TOKEN_ENV, "openrouter-token")]),
        )
        .expect("OpenRouter keeps its existing bearer-shaped API-key contract");
        assert_eq!(openrouter.provider_route, AgentProviderRouteV1::Openrouter);
        assert_eq!(openrouter.auth_mode, AgentAuthModeV1::OpenRouterApiKey);

        let compatible_settings =
            serde_json::json!({ANTHROPIC_BASE_URL_ENV: "https://llm.example.test/v1"});
        let compatible_bearer = resolve(
            &AgentLaunchPreferencesV1::default(),
            compatible_settings.clone(),
            secrets(&[(ANTHROPIC_AUTH_TOKEN_ENV, "compatible-bearer")]),
        )
        .expect("compatible Claude route preserves bearer support");
        assert_eq!(
            compatible_bearer.provider_route,
            AgentProviderRouteV1::CompatibleEndpoint
        );
        assert_eq!(
            compatible_bearer.auth_mode,
            AgentAuthModeV1::AnthropicBearer
        );

        let compatible = resolve(
            &AgentLaunchPreferencesV1::default(),
            compatible_settings,
            secrets(&[
                (ANTHROPIC_AUTH_TOKEN_ENV, "compatible-bearer-must-win"),
                (ANTHROPIC_API_KEY_ENV, "compatible-api-key"),
            ]),
        )
        .expect("compatible Claude route keeps bearer precedence over its API key");
        assert_eq!(
            compatible.provider_route,
            AgentProviderRouteV1::CompatibleEndpoint
        );
        assert_eq!(compatible.auth_mode, AgentAuthModeV1::AnthropicBearer);

        let compatible_oauth_error = resolve(
            &AgentLaunchPreferencesV1::default(),
            serde_json::json!({ANTHROPIC_BASE_URL_ENV: "https://llm.example.test/v1"}),
            secrets(&[
                (CLAUDE_CODE_OAUTH_TOKEN_ENV, "must-not-cross-routes"),
                (ANTHROPIC_AUTH_TOKEN_ENV, "compatible-bearer"),
            ]),
        )
        .expect_err("compatible Claude routes continue to reject OAuth");
        assert!(
            format!("{compatible_oauth_error:?}").contains("mixed_claude_oauth_compatible_route")
        );
    }

    #[test]
    fn execution_receipt_must_match_every_frozen_field() {
        let resolution = resolve(
            &AgentLaunchPreferencesV1::default(),
            serde_json::json!({}),
            secrets(&[(ANTHROPIC_API_KEY_ENV, "sk-ant-test")]),
        )
        .unwrap();
        let mut receipt = AgentExecutionReceiptV1::from_resolution(&resolution);
        receipt.validate_against(&resolution).unwrap();
        receipt.provider_route = AgentProviderRouteV1::Openrouter;
        assert!(receipt.validate_against(&resolution).is_err());
    }

    #[test]
    fn resolution_id_detects_any_frozen_contract_tampering() {
        let mut resolution = resolve(
            &AgentLaunchPreferencesV1::default(),
            serde_json::json!({}),
            secrets(&[(ANTHROPIC_API_KEY_ENV, "sk-ant-test")]),
        )
        .unwrap();
        resolution.effort.value = "low".to_string();

        let error = resolution
            .validate()
            .expect_err("a changed frozen field must invalidate the resolution id");
        assert!(format!("{error:?}").contains("agent_resolution_hash_mismatch"));
    }

    #[test]
    fn historical_proxy_records_remain_readable_but_cannot_launch() {
        let mut old = resolve(
            &AgentLaunchPreferencesV1::default(),
            serde_json::json!({}),
            secrets(&[(ANTHROPIC_API_KEY_ENV, "fixture-key")]),
        )
        .unwrap();
        old.egress_mode = AgentEgressModeV1::HostLlmProxy;
        old.resolution_id = resolution_id_for(&old).unwrap();
        let wire = serde_json::to_value(&old).unwrap();
        assert_eq!(wire["egressMode"], "host_llm_proxy");
        let decoded: ResolvedAgentExecutionV1 = serde_json::from_value(wire).unwrap();
        assert_eq!(decoded, old);
        assert!(
            format!("{:?}", decoded.validate().unwrap_err()).contains("agent_egress_auth_mismatch")
        );
    }

    #[test]
    fn incoherent_provider_route_fails_closed() {
        let mut tampered = resolve(
            &AgentLaunchPreferencesV1::default(),
            serde_json::json!({}),
            secrets(&[(ANTHROPIC_API_KEY_ENV, "sk-ant-test")]),
        )
        .unwrap();
        tampered.provider_route = AgentProviderRouteV1::OpenaiDirect;
        tampered.resolution_id = resolution_id_for(&tampered).unwrap();

        let error = tampered
            .validate()
            .expect_err("an undeclared provider/route pair must be rejected");
        assert!(format!("{error:?}").contains("agent_provider_route_mismatch"));
    }

    #[test]
    fn launch_selection_uses_the_v1_camel_case_wire_vocabulary() {
        let selection = AgentLaunchSelectionV1 {
            preferences: AgentLaunchPreferencesV1 {
                provider: AgentLaunchFieldPreferenceV1::Explicit(ChatAgentProvider::Codex),
                model: AgentLaunchFieldPreferenceV1::Explicit("gpt-5.3-codex".into()),
                permission_mode: AgentLaunchFieldPreferenceV1::CanonicalDefault,
                effort: AgentLaunchFieldPreferenceV1::InheritConfigured,
                ..Default::default()
            },
            confirmed_resolution_id: format!("alr_{}", "0".repeat(64)),
        };

        let value = serde_json::to_value(&selection).unwrap();
        assert_eq!(value["preferences"]["schemaVersion"], 1);
        assert_eq!(value["preferences"]["provider"]["source"], "explicit");
        assert_eq!(value["preferences"]["provider"]["value"], "codex");
        assert_eq!(
            value["preferences"]["permissionMode"]["source"],
            "canonical_default"
        );
        assert_eq!(
            value["preferences"]["effort"]["source"],
            "inherit_configured"
        );
        assert!(value.get("confirmedResolutionId").is_some());
        assert_eq!(
            serde_json::from_value::<AgentLaunchSelectionV1>(value).unwrap(),
            selection
        );
    }

    fn account_input() -> AgentLaunchAccountInput {
        AgentLaunchAccountInput {
            id: Uuid::from_u128(0x0123_4567_89ab_cdef_0123_4567_89ab_cdef),
            kind: ProviderAccountKind::AnthropicApiKey,
            scope: ProviderAccountScope::Workspace,
        }
    }

    #[test]
    fn preferences_without_account_deserialize_as_inherit_and_serialize_unchanged() {
        let wire = serde_json::json!({
            "schemaVersion": 1,
            "provider": {"source": "explicit", "value": "claude_code"},
            "model": {"source": "inherit_configured"},
            "permissionMode": {"source": "canonical_default"},
            "effort": {"source": "inherit_configured"}
        });
        let preferences: AgentLaunchPreferencesV1 = serde_json::from_value(wire).unwrap();
        assert_eq!(
            preferences.account,
            AgentLaunchFieldPreferenceV1::InheritConfigured
        );
        assert_eq!(
            AgentLaunchPreferencesV1::default().account,
            AgentLaunchFieldPreferenceV1::InheritConfigured
        );
        let explicit: AgentLaunchPreferencesV1 = serde_json::from_value(serde_json::json!({
            "schemaVersion": 1,
            "provider": {"source": "inherit_configured"},
            "model": {"source": "inherit_configured"},
            "permissionMode": {"source": "inherit_configured"},
            "effort": {"source": "inherit_configured"},
            "account": {"source": "explicit", "value": {"kind": "runtime_config"}}
        }))
        .unwrap();
        assert_eq!(
            explicit.account,
            AgentLaunchFieldPreferenceV1::Explicit(AgentAccountSelectionV1::RuntimeConfig)
        );
        explicit.validate().unwrap();
        let unknown = serde_json::from_value::<AgentLaunchPreferencesV1>(serde_json::json!({
            "schemaVersion": 1,
            "account": {"source": "explicit", "value": {"kind": "account", "accountId": Uuid::new_v4(), "name": "x"}}
        }));
        assert!(
            unknown.is_err(),
            "unknown keys inside the account selection must be rejected"
        );
        let unknown_top = serde_json::from_value::<AgentLaunchPreferencesV1>(serde_json::json!({
            "schemaVersion": 1,
            "accountName": "x"
        }));
        assert!(unknown_top.is_err());
    }

    #[test]
    fn explicit_account_with_nil_id_is_rejected() {
        let preferences = AgentLaunchPreferencesV1 {
            account: AgentLaunchFieldPreferenceV1::Explicit(AgentAccountSelectionV1::Account {
                account_id: Uuid::nil(),
            }),
            ..Default::default()
        };
        let error = preferences.validate().unwrap_err();
        assert!(format!("{error:?}").contains("invalid_agent_account_id"));
        let error = resolve(
            &preferences,
            serde_json::json!({}),
            secrets(&[(ANTHROPIC_API_KEY_ENV, "sk-ant-test")]),
        )
        .unwrap_err();
        assert!(format!("{error:?}").contains("invalid_agent_account_id"));
    }

    #[test]
    fn account_less_resolution_wire_and_hash_are_unchanged() {
        let settings = serde_json::json!({"ANTHROPIC_MODEL": "claude-sonnet-4-6"});
        let secrets = secrets(&[(ANTHROPIC_API_KEY_ENV, "sk-ant-test")]);
        let resolution = resolve(
            &AgentLaunchPreferencesV1::default(),
            settings.clone(),
            secrets.clone(),
        )
        .unwrap();
        let wire = serde_json::to_value(&resolution).unwrap();
        let object = wire.as_object().unwrap();
        assert!(
            object.keys().all(|key| !key.starts_with("account")),
            "account-less resolution must not serialize account keys: {wire}"
        );
        // The pre-change hashing path is the SHA-256 of the serialized struct
        // with `resolutionId` cleared. Recompute it over a mirror of the exact
        // pre-change struct (same fields, same order, no account fields) to
        // prove every persisted account-less id is unchanged.
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct LegacyResolvedAgentExecutionV1<'a> {
            schema_version: u8,
            resolution_id: &'a str,
            config_revision: &'a str,
            catalog_version: &'a str,
            provider: &'a ResolvedAgentLaunchFieldV1<ChatAgentProvider>,
            provider_route: AgentProviderRouteV1,
            model: &'a ResolvedAgentLaunchFieldV1<String>,
            upstream_model: &'a str,
            permission_mode: &'a ResolvedAgentLaunchFieldV1<String>,
            effort: &'a ResolvedAgentLaunchFieldV1<String>,
            auth_mode: AgentAuthModeV1,
            egress_mode: AgentEgressModeV1,
            credential_ready: bool,
            requires_interactive_auth: bool,
        }
        let legacy = LegacyResolvedAgentExecutionV1 {
            schema_version: resolution.schema_version,
            resolution_id: "",
            config_revision: &resolution.config_revision,
            catalog_version: &resolution.catalog_version,
            provider: &resolution.provider,
            provider_route: resolution.provider_route,
            model: &resolution.model,
            upstream_model: &resolution.upstream_model,
            permission_mode: &resolution.permission_mode,
            effort: &resolution.effort,
            auth_mode: resolution.auth_mode,
            egress_mode: resolution.egress_mode,
            credential_ready: resolution.credential_ready,
            requires_interactive_auth: resolution.requires_interactive_auth,
        };
        let legacy_bytes = serde_json::to_vec(&legacy).unwrap();
        let mut current = resolution.clone();
        current.resolution_id.clear();
        assert_eq!(
            serde_json::to_vec(&current).unwrap(),
            legacy_bytes,
            "account-less serialization must be byte-identical to the pre-change wire"
        );
        let mut expected = String::from("alr_");
        for byte in Sha256::digest(legacy_bytes) {
            write!(&mut expected, "{byte:02x}").unwrap();
        }
        assert_eq!(resolution.resolution_id, expected);
        // A persisted account-less resolution round-trips and validates.
        let restored: ResolvedAgentExecutionV1 = serde_json::from_value(wire).unwrap();
        assert_eq!(restored, resolution);
        restored.validate().unwrap();
        assert!(restored.account().is_none());
    }

    #[test]
    fn selecting_an_account_changes_the_resolution_id_and_freezes_the_triple() {
        let settings = serde_json::json!({"ANTHROPIC_MODEL": "claude-sonnet-4-6"});
        let secrets = secrets(&[(ANTHROPIC_API_KEY_ENV, "sk-ant-test")]);
        let without = resolve(
            &AgentLaunchPreferencesV1::default(),
            settings.clone(),
            secrets.clone(),
        )
        .unwrap();
        let account = account_input();
        let with = resolve_with_account(
            &AgentLaunchPreferencesV1::default(),
            settings,
            secrets,
            account,
        )
        .unwrap();
        assert_ne!(with.resolution_id, without.resolution_id);
        assert_eq!(with.account(), Some(account));
        let wire = serde_json::to_value(&with).unwrap();
        assert_eq!(wire["accountId"], Value::String(account.id.to_string()));
        assert_eq!(wire["accountKind"], "anthropic_api_key");
        assert_eq!(wire["accountScope"], "workspace");
        // Every other frozen field is identical; only the account triple and id differ.
        let mut stripped = with.clone();
        stripped.account_id = None;
        stripped.account_kind = None;
        stripped.account_scope = None;
        stripped.resolution_id = without.resolution_id.clone();
        assert_eq!(stripped, without);
        // The receipt never carries account fields and still matches.
        let receipt = AgentExecutionReceiptV1::from_resolution(&with);
        receipt.validate_against(&with).unwrap();
        assert!(serde_json::to_value(&receipt)
            .unwrap()
            .as_object()
            .unwrap()
            .keys()
            .all(|key| !key.starts_with("account")));
    }

    #[test]
    fn partial_or_mismatched_account_triple_fails_closed() {
        let settings = serde_json::json!({});
        let secrets = secrets(&[(ANTHROPIC_API_KEY_ENV, "sk-ant-test")]);
        let resolution = resolve_with_account(
            &AgentLaunchPreferencesV1::default(),
            settings,
            secrets,
            account_input(),
        )
        .unwrap();
        let mut partial = resolution.clone();
        partial.account_scope = None;
        let error = partial.validate().unwrap_err();
        assert!(format!("{error:?}").contains("agent_account_incomplete"));
        let mut kind_only = ResolvedAgentExecutionV1 {
            account_id: None,
            account_kind: Some(ProviderAccountKind::AnthropicApiKey),
            account_scope: None,
            ..resolution.clone()
        };
        kind_only.resolution_id = String::new();
        let error = kind_only.validate().unwrap_err();
        assert!(format!("{error:?}").contains("agent_account_incomplete"));
        // A kind from the other provider family cannot be frozen against Claude Code.
        let mut mismatched = resolution.clone();
        mismatched.account_kind = Some(ProviderAccountKind::OpenaiApiKey);
        mismatched.resolution_id = resolution_id_for(&mismatched).unwrap();
        let error = mismatched.validate().unwrap_err();
        assert!(format!("{error:?}").contains("account_kind_mismatch"));
        // Tampering with the frozen account id is caught by the hash.
        let mut tampered = resolution;
        tampered.account_id = Some(Uuid::new_v4());
        let error = tampered.validate().unwrap_err();
        assert!(format!("{error:?}").contains("agent_resolution_hash_mismatch"));
    }
}
