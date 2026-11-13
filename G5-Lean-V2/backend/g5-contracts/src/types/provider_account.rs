//! Named provider accounts: the closed kind catalog, account scope, the launch
//! account selection, and the one kind-to-keys credential projection.
//!
//! A provider account is a scoped credential bundle for one provider family.
//! Every kind maps to exactly one already-approved auth mode; the route
//! follows the base URL host exactly as it does for raw Runtime Config keys,
//! so the resolver in `agent_launch.rs` stays the only route/auth authority.

use http::Uri;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::claude_models::{
    ANTHROPIC_API_KEY_ENV, ANTHROPIC_AUTH_TOKEN_ENV, ANTHROPIC_BASE_URL_ENV,
    CLAUDE_CODE_OAUTH_TOKEN_ENV,
};
use crate::codex_models::{OPENAI_API_KEY_ENV, OPENAI_BASE_URL_ENV, OPENROUTER_API_KEY_ENV};
use crate::error::G5Error;

use super::agent_launch::CODEX_SUBSCRIPTION_ENABLED_SETTING;
use super::ChatAgentProvider;

/// Fixed direct Anthropic host used by the Claude subscription kind and as the
/// default for `anthropic_api_key`.
pub const ANTHROPIC_DIRECT_BASE_URL: &str = "https://api.anthropic.com";
/// Fixed direct OpenAI host used by the ChatGPT subscription kind and as the
/// default for `openai_api_key`.
pub const OPENAI_DIRECT_BASE_URL: &str = "https://api.openai.com";
/// OpenRouter Anthropic-compatible gateway consumed by Claude Code.
pub const OPENROUTER_ANTHROPIC_BASE_URL: &str = "https://openrouter.ai/api";
/// OpenRouter OpenAI-compatible gateway consumed by Codex.
pub const OPENROUTER_OPENAI_BASE_URL: &str = "https://openrouter.ai/api/v1";
/// Upper bound on a stored account base URL.
pub const PROVIDER_ACCOUNT_BASE_URL_MAX_LEN: usize = 2048;
/// Offered presets for the `anthropic_compatible_bearer` kind.
pub const ANTHROPIC_COMPATIBLE_BEARER_BASE_URL_PRESETS: &[&str] = &[
    "https://api.z.ai/api/anthropic",
    "https://open.bigmodel.cn/api/anthropic",
];

/// Credential and route keys scrubbed from both channels before a Claude
/// family account is overlaid.
pub const CLAUDE_FAMILY_SCRUB_KEYS: &[&str] = &[
    CLAUDE_CODE_OAUTH_TOKEN_ENV,
    ANTHROPIC_AUTH_TOKEN_ENV,
    ANTHROPIC_API_KEY_ENV,
    ANTHROPIC_BASE_URL_ENV,
];
/// Credential and route keys scrubbed from both channels before a Codex
/// family account is overlaid.
pub const CODEX_FAMILY_SCRUB_KEYS: &[&str] = &[
    OPENAI_API_KEY_ENV,
    OPENROUTER_API_KEY_ENV,
    OPENAI_BASE_URL_ENV,
    CODEX_SUBSCRIPTION_ENABLED_SETTING,
];

/// Closed provider-account kind catalog. Each kind maps to exactly one
/// approved auth mode; a new kind must be added to the harness catalog first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderAccountKind {
    ClaudeSubscription,
    AnthropicApiKey,
    AnthropicCompatibleBearer,
    OpenrouterApiKey,
    OpenaiApiKey,
    ChatgptSubscription,
}

/// Provider family an account kind serves.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderAccountFamily {
    Claude,
    Codex,
    Both,
}

impl ProviderAccountFamily {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            ProviderAccountFamily::Claude => "claude",
            ProviderAccountFamily::Codex => "codex",
            ProviderAccountFamily::Both => "both",
        }
    }
}

impl ProviderAccountKind {
    /// Every kind in catalog order.
    pub const ALL: &'static [ProviderAccountKind] = &[
        ProviderAccountKind::ClaudeSubscription,
        ProviderAccountKind::AnthropicApiKey,
        ProviderAccountKind::AnthropicCompatibleBearer,
        ProviderAccountKind::OpenrouterApiKey,
        ProviderAccountKind::OpenaiApiKey,
        ProviderAccountKind::ChatgptSubscription,
    ];

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            ProviderAccountKind::ClaudeSubscription => "claude_subscription",
            ProviderAccountKind::AnthropicApiKey => "anthropic_api_key",
            ProviderAccountKind::AnthropicCompatibleBearer => "anthropic_compatible_bearer",
            ProviderAccountKind::OpenrouterApiKey => "openrouter_api_key",
            ProviderAccountKind::OpenaiApiKey => "openai_api_key",
            ProviderAccountKind::ChatgptSubscription => "chatgpt_subscription",
        }
    }

    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        let trimmed = value.trim();
        Self::ALL
            .iter()
            .copied()
            .find(|kind| kind.as_str() == trimmed)
    }

    #[must_use]
    pub const fn family(self) -> ProviderAccountFamily {
        match self {
            ProviderAccountKind::ClaudeSubscription
            | ProviderAccountKind::AnthropicApiKey
            | ProviderAccountKind::AnthropicCompatibleBearer => ProviderAccountFamily::Claude,
            ProviderAccountKind::OpenrouterApiKey => ProviderAccountFamily::Both,
            ProviderAccountKind::OpenaiApiKey | ProviderAccountKind::ChatgptSubscription => {
                ProviderAccountFamily::Codex
            }
        }
    }

    #[must_use]
    pub const fn supports_agent(self, agent: ChatAgentProvider) -> bool {
        matches!(
            (self.family(), agent),
            (ProviderAccountFamily::Claude, ChatAgentProvider::ClaudeCode)
                | (ProviderAccountFamily::Codex, ChatAgentProvider::Codex)
                | (
                    ProviderAccountFamily::Both,
                    ChatAgentProvider::ClaudeCode | ChatAgentProvider::Codex
                )
        )
    }

    /// False only for `chatgpt_subscription`, whose credential is the
    /// VM-native device login rather than a stored platform secret.
    #[must_use]
    pub const fn requires_secret(self) -> bool {
        !matches!(self, ProviderAccountKind::ChatgptSubscription)
    }

    /// True only for the compatible-endpoint bearer kind, which has no
    /// meaningful default host.
    #[must_use]
    pub const fn requires_base_url(self) -> bool {
        matches!(self, ProviderAccountKind::AnthropicCompatibleBearer)
    }

    /// Kinds whose host may be chosen by the caller. Subscription kinds and
    /// the OpenRouter kind have a fixed host and reject a supplied URL.
    #[must_use]
    pub const fn accepts_base_url(self) -> bool {
        matches!(
            self,
            ProviderAccountKind::AnthropicCompatibleBearer
                | ProviderAccountKind::AnthropicApiKey
                | ProviderAccountKind::OpenaiApiKey
        )
    }

    /// Validate a caller-supplied base URL for this kind.
    ///
    /// Accepts only an absolute `https` URL with a host, without userinfo
    /// credentials, within [`PROVIDER_ACCOUNT_BASE_URL_MAX_LEN`], and never an
    /// OpenRouter host (only the `openrouter_api_key` kind routes there;
    /// letting another kind point at OpenRouter would change its auth mode).
    pub fn validate_base_url(self, base_url: Option<&str>) -> Result<(), G5Error> {
        let supplied = base_url.map(str::trim).filter(|value| !value.is_empty());
        match supplied {
            None if self.requires_base_url() => Err(account_validation_error(
                "baseUrl",
                "account_base_url_invalid",
                format!("{} accounts require a base URL", self.as_str()),
            )),
            None => Ok(()),
            Some(_) if !self.accepts_base_url() => Err(account_validation_error(
                "baseUrl",
                "account_base_url_invalid",
                format!("{} accounts use a fixed base URL", self.as_str()),
            )),
            Some(url) => validate_https_base_url(url),
        }
    }

    /// The published shape of a credential of this kind, when the provider
    /// has one. Secret scanners agree on the prefixes (gitleaks, TruffleHog
    /// and the Anthropic-supplied GitLab rule): `sk-ant-api03-` and
    /// `sk-ant-oat01-` followed by 95 base64url characters, `sk-or-v1-`
    /// followed by 64 hex characters, and `sk-`, `sk-proj-`, `sk-svcacct-`
    /// or `sk-admin-` for OpenAI. This rule pins only what a wrong paste
    /// would violate: the prefix family with its version digits left free
    /// (`oat01` may become `oat02`; an API key is any `sk-ant-<family>-`
    /// other than the OAuth token and Admin key families), the character class, and a
    /// floor on the body length well under the real one, so a truncated or
    /// wrong-kind paste is refused while a provider rotating its format is
    /// not. The compatible-endpoint bearer has no known shape.
    #[must_use]
    pub const fn secret_shape(self) -> Option<SecretShape> {
        match self {
            ProviderAccountKind::ClaudeSubscription => Some(SecretShape {
                prefix: "sk-ant-oat",
                family: SecretFamily::Digits,
                charset: SecretCharset::Base64Url,
                min_body: SECRET_MIN_BODY_CHARS,
            }),
            ProviderAccountKind::AnthropicApiKey => Some(SecretShape {
                prefix: "sk-ant-",
                family: SecretFamily::LettersDigitsExcept(ANTHROPIC_NON_INFERENCE_FAMILIES),
                charset: SecretCharset::Base64Url,
                min_body: SECRET_MIN_BODY_CHARS,
            }),
            ProviderAccountKind::OpenrouterApiKey => Some(SecretShape {
                prefix: "sk-or-v",
                family: SecretFamily::Digits,
                charset: SecretCharset::Hex,
                min_body: SECRET_MIN_BODY_CHARS,
            }),
            ProviderAccountKind::OpenaiApiKey => Some(SecretShape {
                prefix: "sk-",
                family: SecretFamily::None,
                charset: SecretCharset::Base64Url,
                min_body: SECRET_MIN_BODY_CHARS,
            }),
            ProviderAccountKind::AnthropicCompatibleBearer
            | ProviderAccountKind::ChatgptSubscription => None,
        }
    }

    /// The prefix a credential of this kind starts with, for messages and
    /// clients; `None` when the kind has no known shape.
    #[must_use]
    pub const fn secret_prefix(self) -> Option<&'static str> {
        match self.secret_shape() {
            Some(shape) => Some(shape.prefix),
            None => None,
        }
    }

    /// Whether the kind's shape applies to a credential routed at
    /// `base_url`: always for a kind with a fixed host, and for an API-key
    /// kind only on the provider's own host. A compatible gateway behind a
    /// custom base URL issues keys of its own shape, which G5 cannot know.
    #[must_use]
    pub fn secret_shape_applies(self, base_url: Option<&str>) -> bool {
        if self.secret_shape().is_none() {
            return false;
        }
        if !self.accepts_base_url() {
            return true;
        }
        let native = match self {
            ProviderAccountKind::AnthropicApiKey => ANTHROPIC_DIRECT_BASE_URL,
            ProviderAccountKind::OpenaiApiKey => OPENAI_DIRECT_BASE_URL,
            _ => return false,
        };
        match base_url.map(str::trim).filter(|value| !value.is_empty()) {
            None => true,
            Some(url) => is_native_host(url, native),
        }
    }

    /// What a person should paste into an account of this kind, for the
    /// `account_secret_invalid` message when the shape does not match.
    #[must_use]
    pub const fn secret_shape_help(self) -> &'static str {
        match self {
            ProviderAccountKind::ClaudeSubscription => {
                "Claude subscription accounts take the OAuth token printed by \
                 `claude setup-token`, which starts with sk-ant-oat01-; an Anthropic \
                 API key (sk-ant-api03-…) belongs in an Anthropic API key account"
            }
            ProviderAccountKind::AnthropicApiKey => {
                "Anthropic API key accounts take a key starting with sk-ant-api03-; \
                 a `claude setup-token` value (sk-ant-oat01-…) belongs in a Claude \
                 subscription account"
            }
            ProviderAccountKind::OpenrouterApiKey => {
                "OpenRouter API keys start with sk-or-v1- followed by the key's hex digits"
            }
            ProviderAccountKind::OpenaiApiKey => {
                "OpenAI API keys start with sk- (sk-proj-, sk-svcacct-, …)"
            }
            ProviderAccountKind::AnthropicCompatibleBearer
            | ProviderAccountKind::ChatgptSubscription => "",
        }
    }

    /// Validate a credential a person submits for this kind: trimmed, present
    /// exactly when the kind stores one, a single token without spaces or
    /// line breaks, and matching the kind's [`secret_shape`] when it
    /// [applies](Self::secret_shape_applies) at `base_url`. Returns the
    /// trimmed value. This guards create and update only; a credential
    /// already stored is projected as it is.
    ///
    /// [`secret_shape`]: Self::secret_shape
    pub fn validate_secret(
        self,
        raw: Option<&str>,
        base_url: Option<&str>,
    ) -> Result<Option<String>, G5Error> {
        let Some(secret) = validate_secret_presence(self, raw)? else {
            return Ok(None);
        };
        if secret.chars().any(char::is_whitespace) {
            return Err(account_validation_error(
                "secret",
                "account_secret_invalid",
                "the credential contains a space or line break; paste it as one line",
            ));
        }
        if self.secret_shape_applies(base_url) {
            let shape = self
                .secret_shape()
                .expect("a shape applies only when the kind has one");
            if !shape.matches(secret) {
                return Err(account_validation_error(
                    "secret",
                    "account_secret_invalid",
                    self.secret_shape_help(),
                ));
            }
        }
        Ok(Some(secret.to_string()))
    }
}

/// Anthropic `sk-ant-` families that are not inference API keys: the
/// `claude setup-token` access token (`oat`), the OAuth refresh token (`ort`)
/// and the Admin API key (`admin`, which cannot call Messages). Any other
/// family is accepted so a new key family does not lock people out.
pub const ANTHROPIC_NON_INFERENCE_FAMILIES: &[&str] = &["oat", "ort", "admin"];

/// Floor on the characters after a credential's prefix: real Anthropic
/// tokens carry 95, OpenRouter 64 and OpenAI keys over 100, so 20 refuses a
/// truncated paste without pinning any provider's exact length.
pub const SECRET_MIN_BODY_CHARS: usize = 20;

/// Character class of a credential body.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SecretCharset {
    /// `A-Z a-z 0-9 _ -`
    Base64Url,
    /// `0-9 a-f A-F`
    Hex,
}

impl SecretCharset {
    const fn allows(self, c: char) -> bool {
        match self {
            SecretCharset::Base64Url => c.is_ascii_alphanumeric() || c == '_' || c == '-',
            SecretCharset::Hex => c.is_ascii_hexdigit(),
        }
    }
}

/// The family segment between a shape's prefix and the body.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SecretFamily {
    /// The prefix runs straight into the body (`sk-` …).
    None,
    /// One or more version digits and a dash (`sk-ant-oat` `01-`).
    Digits,
    /// Lower-case letters, optional digits and a dash (`sk-ant-` `api03-`),
    /// except the named families (so an API-key shape never admits an OAuth
    /// token or an Admin key).
    LettersDigitsExcept(&'static [&'static str]),
}

/// The published shape of one kind's credential: the regex
/// `^<prefix><family><charset>{min_body,}$`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SecretShape {
    pub prefix: &'static str,
    pub family: SecretFamily,
    pub charset: SecretCharset,
    pub min_body: usize,
}

impl SecretShape {
    #[must_use]
    pub fn matches(self, secret: &str) -> bool {
        let Some(rest) = secret.strip_prefix(self.prefix) else {
            return false;
        };
        let body = match self.family {
            SecretFamily::None => rest,
            SecretFamily::Digits => {
                let digits = rest.chars().take_while(char::is_ascii_digit).count();
                if digits == 0 {
                    return false;
                }
                match rest[digits..].strip_prefix('-') {
                    Some(body) => body,
                    None => return false,
                }
            }
            SecretFamily::LettersDigitsExcept(excluded) => {
                let letters = rest.chars().take_while(char::is_ascii_lowercase).count();
                if letters == 0 || excluded.contains(&&rest[..letters]) {
                    return false;
                }
                let digits = rest[letters..]
                    .chars()
                    .take_while(char::is_ascii_digit)
                    .count();
                match rest[letters + digits..].strip_prefix('-') {
                    Some(body) => body,
                    None => return false,
                }
            }
        };
        body.chars().count() >= self.min_body && body.chars().all(|c| self.charset.allows(c))
    }
}

/// Whether `url` reaches the provider's own host `native` (an `https://host`
/// literal): same scheme and host, ignoring case, and the default port only,
/// so an explicit `:443` still counts as native and a look-alike host with a
/// longer name does not.
fn is_native_host(url: &str, native: &str) -> bool {
    let Some(native_host) = native.strip_prefix("https://") else {
        return false;
    };
    let Ok(parsed) = url.parse::<Uri>() else {
        return false;
    };
    parsed.scheme_str() == Some("https")
        && parsed
            .host()
            .is_some_and(|host| host.eq_ignore_ascii_case(native_host))
        && parsed.port_u16().is_none_or(|port| port == 443)
}

/// Secret presence must match the kind: every kind but `chatgpt_subscription`
/// stores one (secret_custody D10). Returns the trimmed value.
fn validate_secret_presence(
    kind: ProviderAccountKind,
    raw: Option<&str>,
) -> Result<Option<&str>, G5Error> {
    let secret = raw.map(str::trim).filter(|value| !value.is_empty());
    match (kind.requires_secret(), secret) {
        (true, None) => Err(account_validation_error(
            "secret",
            "account_secret_invalid",
            format!("{} accounts require a secret", kind.as_str()),
        )),
        (false, Some(_)) => Err(account_validation_error(
            "secret",
            "account_secret_invalid",
            format!("{} accounts store no secret", kind.as_str()),
        )),
        (_, secret) => Ok(secret),
    }
}

impl std::fmt::Display for ProviderAccountKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl std::str::FromStr for ProviderAccountKind {
    type Err = G5Error;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::parse(value).ok_or_else(|| {
            account_validation_error(
                "kind",
                "account_kind_invalid",
                format!("{value} is not a provider account kind"),
            )
        })
    }
}

fn validate_https_base_url(url: &str) -> Result<(), G5Error> {
    if url.len() > PROVIDER_ACCOUNT_BASE_URL_MAX_LEN {
        return Err(account_validation_error(
            "baseUrl",
            "account_base_url_invalid",
            format!("base URL must be at most {PROVIDER_ACCOUNT_BASE_URL_MAX_LEN} characters"),
        ));
    }
    let parsed: Uri = url.parse().map_err(|_| {
        account_validation_error(
            "baseUrl",
            "account_base_url_invalid",
            "base URL must be an absolute https URL",
        )
    })?;
    if parsed.scheme_str() != Some("https") {
        return Err(account_validation_error(
            "baseUrl",
            "account_base_url_invalid",
            "base URL must use https",
        ));
    }
    let Some(authority) = parsed.authority() else {
        return Err(account_validation_error(
            "baseUrl",
            "account_base_url_invalid",
            "base URL must include a host",
        ));
    };
    if authority.as_str().contains('@') {
        return Err(account_validation_error(
            "baseUrl",
            "account_base_url_invalid",
            "base URL must not embed credentials",
        ));
    }
    let host = authority.host().to_ascii_lowercase();
    if host.is_empty() {
        return Err(account_validation_error(
            "baseUrl",
            "account_base_url_invalid",
            "base URL must include a host",
        ));
    }
    if host == "openrouter.ai" || host.ends_with(".openrouter.ai") {
        return Err(account_validation_error(
            "baseUrl",
            "account_base_url_invalid",
            "OpenRouter is reachable only through an openrouter_api_key account",
        ));
    }
    Ok(())
}

/// Where a provider account lives and who may use it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderAccountScope {
    Org,
    Workspace,
    User,
}

impl ProviderAccountScope {
    pub const ALL: &'static [ProviderAccountScope] = &[
        ProviderAccountScope::Org,
        ProviderAccountScope::Workspace,
        ProviderAccountScope::User,
    ];

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            ProviderAccountScope::Org => "org",
            ProviderAccountScope::Workspace => "workspace",
            ProviderAccountScope::User => "user",
        }
    }

    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        let trimmed = value.trim();
        Self::ALL
            .iter()
            .copied()
            .find(|scope| scope.as_str() == trimmed)
    }
}

impl std::fmt::Display for ProviderAccountScope {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl std::str::FromStr for ProviderAccountScope {
    type Err = G5Error;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::parse(value).ok_or_else(|| {
            account_validation_error(
                "scope",
                "account_scope_invalid",
                format!("{value} is not a provider account scope"),
            )
        })
    }
}

/// Explicit value of the `account` launch preference: one named account, or
/// the provider's raw Runtime Config variables.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum AgentAccountSelectionV1 {
    #[serde(rename_all = "camelCase")]
    Account {
        account_id: Uuid,
    },
    RuntimeConfig,
}

/// Non-secret settings, secret-channel entries, and the family scrub set a
/// provider account contributes to one effective-config resolution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderAccountProjection {
    pub settings: Vec<(String, Value)>,
    pub secrets: Vec<(String, String)>,
    pub scrub_keys: Vec<&'static str>,
}

/// The only kind-to-keys mapping. The caller scrubs `scrub_keys` from both the
/// settings and the secret channel, then overlays `settings` and `secrets`.
pub fn project_provider_account(
    kind: ProviderAccountKind,
    base_url: Option<&str>,
    secret: Option<&str>,
    agent: ChatAgentProvider,
) -> Result<ProviderAccountProjection, G5Error> {
    if !kind.supports_agent(agent) {
        return Err(account_validation_error(
            "kind",
            "account_kind_mismatch",
            format!(
                "{} accounts cannot be used with {}",
                kind.as_str(),
                agent.as_str()
            ),
        ));
    }
    kind.validate_base_url(base_url)?;
    let base_url = base_url.map(str::trim).filter(|value| !value.is_empty());
    // Presence only: a stored credential is projected as it is, even when
    // it predates the shape rule applied at create and update.
    let secret = validate_secret_presence(kind, secret)?;
    let scrub_keys = match kind.family() {
        ProviderAccountFamily::Claude => CLAUDE_FAMILY_SCRUB_KEYS.to_vec(),
        ProviderAccountFamily::Codex => CODEX_FAMILY_SCRUB_KEYS.to_vec(),
        ProviderAccountFamily::Both => match agent {
            ChatAgentProvider::Codex => CODEX_FAMILY_SCRUB_KEYS.to_vec(),
            _ => CLAUDE_FAMILY_SCRUB_KEYS.to_vec(),
        },
    };
    let (settings, secrets) = match kind {
        ProviderAccountKind::ClaudeSubscription => (
            vec![setting(ANTHROPIC_BASE_URL_ENV, ANTHROPIC_DIRECT_BASE_URL)],
            vec![secret_entry(CLAUDE_CODE_OAUTH_TOKEN_ENV, secret)],
        ),
        ProviderAccountKind::AnthropicApiKey => (
            vec![setting(
                ANTHROPIC_BASE_URL_ENV,
                base_url.unwrap_or(ANTHROPIC_DIRECT_BASE_URL),
            )],
            vec![secret_entry(ANTHROPIC_API_KEY_ENV, secret)],
        ),
        ProviderAccountKind::AnthropicCompatibleBearer => (
            vec![setting(
                ANTHROPIC_BASE_URL_ENV,
                base_url.expect("validate_base_url requires a URL for this kind"),
            )],
            vec![secret_entry(ANTHROPIC_AUTH_TOKEN_ENV, secret)],
        ),
        ProviderAccountKind::OpenrouterApiKey => match agent {
            ChatAgentProvider::Codex => (
                vec![setting(OPENAI_BASE_URL_ENV, OPENROUTER_OPENAI_BASE_URL)],
                vec![secret_entry(OPENROUTER_API_KEY_ENV, secret)],
            ),
            _ => (
                vec![setting(
                    ANTHROPIC_BASE_URL_ENV,
                    OPENROUTER_ANTHROPIC_BASE_URL,
                )],
                vec![secret_entry(ANTHROPIC_AUTH_TOKEN_ENV, secret)],
            ),
        },
        ProviderAccountKind::OpenaiApiKey => (
            vec![
                setting(
                    OPENAI_BASE_URL_ENV,
                    base_url.unwrap_or(OPENAI_DIRECT_BASE_URL),
                ),
                (
                    CODEX_SUBSCRIPTION_ENABLED_SETTING.to_string(),
                    Value::Bool(false),
                ),
            ],
            vec![secret_entry(OPENAI_API_KEY_ENV, secret)],
        ),
        ProviderAccountKind::ChatgptSubscription => (
            vec![
                setting(OPENAI_BASE_URL_ENV, OPENAI_DIRECT_BASE_URL),
                (
                    CODEX_SUBSCRIPTION_ENABLED_SETTING.to_string(),
                    Value::Bool(true),
                ),
            ],
            Vec::new(),
        ),
    };
    Ok(ProviderAccountProjection {
        settings,
        secrets,
        scrub_keys,
    })
}

fn setting(key: &str, value: &str) -> (String, Value) {
    (key.to_string(), Value::String(value.to_string()))
}

fn secret_entry(key: &str, secret: Option<&str>) -> (String, String) {
    (
        key.to_string(),
        secret
            .expect("secret presence is validated before projection")
            .to_string(),
    )
}

fn account_validation_error(
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
    use std::collections::HashMap;

    use super::*;
    use crate::types::agent_launch::{
        preview_agent_launch, AgentAuthModeV1, AgentLaunchFieldPreferenceV1,
        AgentLaunchPreferencesV1, AgentLaunchResolutionInput, AgentProviderRouteV1,
    };

    const MODEL_KEYS: &[&str] = &[
        "ANTHROPIC_MODEL",
        "CLAUDE_CODE_SUBAGENT_MODEL",
        "OPENAI_MODEL",
    ];

    fn error_code(error: &G5Error) -> String {
        match error {
            G5Error::ValidationError { details } => details["code"]
                .as_str()
                .map(str::to_string)
                .unwrap_or_default(),
            other => panic!("expected a validation error, got {other:?}"),
        }
    }

    fn error_text(error: &G5Error) -> String {
        match error {
            G5Error::ValidationError { details } => details["error"]
                .as_str()
                .map(str::to_string)
                .unwrap_or_default(),
            other => panic!("expected a validation error, got {other:?}"),
        }
    }

    fn preview_projection(
        kind: ProviderAccountKind,
        base_url: Option<&str>,
        agent: ChatAgentProvider,
    ) -> (AgentProviderRouteV1, AgentAuthModeV1) {
        let secret = kind.requires_secret().then_some("account-secret");
        let projection = project_provider_account(kind, base_url, secret, agent)
            .unwrap_or_else(|error| panic!("{kind} projects for {}: {error:?}", agent.as_str()));
        // The scrub set is the whole family route/credential set and must
        // cover every key the projection overlays.
        for (key, _) in &projection.settings {
            assert!(
                projection.scrub_keys.contains(&key.as_str()),
                "{kind}: setting {key} is not in its scrub set"
            );
        }
        for (key, _) in &projection.secrets {
            assert!(
                projection.scrub_keys.contains(&key.as_str()),
                "{kind}: secret {key} is not in its scrub set"
            );
        }
        for model_key in MODEL_KEYS {
            assert!(
                !projection.scrub_keys.contains(model_key),
                "{kind}: scrub set must never include {model_key}"
            );
        }
        let settings: serde_json::Map<String, Value> = projection.settings.into_iter().collect();
        let secrets: HashMap<String, String> = projection.secrets.into_iter().collect();
        let preferences = AgentLaunchPreferencesV1 {
            provider: AgentLaunchFieldPreferenceV1::Explicit(agent),
            permission_mode: AgentLaunchFieldPreferenceV1::Explicit(match agent {
                ChatAgentProvider::ClaudeCode => "default".to_string(),
                ChatAgentProvider::Codex => "never/workspace-write".to_string(),
            }),
            ..Default::default()
        };
        let resolved = preview_agent_launch(AgentLaunchResolutionInput {
            preferences: &preferences,
            effective_settings: &Value::Object(settings),
            effective_secrets: &secrets,
            config_revision: "cfg-account",
            account: None,
        })
        .unwrap_or_else(|error| panic!("{kind} resolves for {}: {error:?}", agent.as_str()));
        if kind == ProviderAccountKind::ChatgptSubscription {
            assert!(resolved.requires_interactive_auth);
        } else {
            assert!(
                resolved.credential_ready,
                "{kind}: projected secret must make the route credential-ready"
            );
        }
        (resolved.provider_route, resolved.auth_mode)
    }

    #[test]
    fn every_kind_projects_to_exactly_one_route_and_auth_pair() {
        let expected = [
            (
                ProviderAccountKind::ClaudeSubscription,
                ChatAgentProvider::ClaudeCode,
                AgentProviderRouteV1::AnthropicDirect,
                AgentAuthModeV1::AnthropicOauth,
            ),
            (
                ProviderAccountKind::AnthropicApiKey,
                ChatAgentProvider::ClaudeCode,
                AgentProviderRouteV1::AnthropicDirect,
                AgentAuthModeV1::AnthropicApiKey,
            ),
            (
                ProviderAccountKind::AnthropicCompatibleBearer,
                ChatAgentProvider::ClaudeCode,
                AgentProviderRouteV1::CompatibleEndpoint,
                AgentAuthModeV1::AnthropicBearer,
            ),
            (
                ProviderAccountKind::OpenrouterApiKey,
                ChatAgentProvider::ClaudeCode,
                AgentProviderRouteV1::Openrouter,
                AgentAuthModeV1::OpenRouterApiKey,
            ),
            (
                ProviderAccountKind::OpenrouterApiKey,
                ChatAgentProvider::Codex,
                AgentProviderRouteV1::Openrouter,
                AgentAuthModeV1::OpenRouterApiKey,
            ),
            (
                ProviderAccountKind::OpenaiApiKey,
                ChatAgentProvider::Codex,
                AgentProviderRouteV1::OpenaiDirect,
                AgentAuthModeV1::OpenAiApiKey,
            ),
            (
                ProviderAccountKind::ChatgptSubscription,
                ChatAgentProvider::Codex,
                AgentProviderRouteV1::OpenaiDirect,
                AgentAuthModeV1::ChatGptSubscription,
            ),
        ];
        let mut covered = Vec::new();
        for (kind, agent, route, auth) in expected {
            let base_url = kind
                .requires_base_url()
                .then_some(ANTHROPIC_COMPATIBLE_BEARER_BASE_URL_PRESETS[0]);
            assert_eq!(
                preview_projection(kind, base_url, agent),
                (route, auth),
                "{kind} on {}",
                agent.as_str()
            );
            covered.push(kind);
        }
        for kind in ProviderAccountKind::ALL {
            assert!(covered.contains(kind), "{kind} is missing from the table");
        }
    }

    #[test]
    fn openrouter_kind_projects_per_agent() {
        let claude = project_provider_account(
            ProviderAccountKind::OpenrouterApiKey,
            None,
            Some("sk-or"),
            ChatAgentProvider::ClaudeCode,
        )
        .unwrap();
        assert_eq!(
            claude.settings,
            vec![setting(
                ANTHROPIC_BASE_URL_ENV,
                OPENROUTER_ANTHROPIC_BASE_URL
            )]
        );
        assert_eq!(
            claude.secrets,
            vec![(ANTHROPIC_AUTH_TOKEN_ENV.to_string(), "sk-or".to_string())]
        );
        assert_eq!(claude.scrub_keys, CLAUDE_FAMILY_SCRUB_KEYS.to_vec());

        let codex = project_provider_account(
            ProviderAccountKind::OpenrouterApiKey,
            None,
            Some("sk-or"),
            ChatAgentProvider::Codex,
        )
        .unwrap();
        assert_eq!(
            codex.settings,
            vec![setting(OPENAI_BASE_URL_ENV, OPENROUTER_OPENAI_BASE_URL)]
        );
        assert_eq!(
            codex.secrets,
            vec![(OPENROUTER_API_KEY_ENV.to_string(), "sk-or".to_string())]
        );
        assert_eq!(codex.scrub_keys, CODEX_FAMILY_SCRUB_KEYS.to_vec());
    }

    #[test]
    fn api_key_kinds_follow_a_compatible_base_url_without_changing_auth_mode() {
        assert_eq!(
            preview_projection(
                ProviderAccountKind::AnthropicApiKey,
                Some("https://llm.example.test/anthropic"),
                ChatAgentProvider::ClaudeCode,
            ),
            (
                AgentProviderRouteV1::CompatibleEndpoint,
                AgentAuthModeV1::AnthropicApiKey
            )
        );
        assert_eq!(
            preview_projection(
                ProviderAccountKind::OpenaiApiKey,
                Some("https://llm.example.test/v1"),
                ChatAgentProvider::Codex,
            ),
            (
                AgentProviderRouteV1::CompatibleEndpoint,
                AgentAuthModeV1::OpenAiApiKey
            )
        );
        // openai_api_key forces subscription auth off even on the direct host.
        let direct = project_provider_account(
            ProviderAccountKind::OpenaiApiKey,
            None,
            Some("sk-openai"),
            ChatAgentProvider::Codex,
        )
        .unwrap();
        assert!(direct.settings.contains(&(
            CODEX_SUBSCRIPTION_ENABLED_SETTING.to_string(),
            Value::Bool(false)
        )));
        let subscription = project_provider_account(
            ProviderAccountKind::ChatgptSubscription,
            None,
            None,
            ChatAgentProvider::Codex,
        )
        .unwrap();
        assert!(subscription.secrets.is_empty());
        assert!(subscription.settings.contains(&(
            CODEX_SUBSCRIPTION_ENABLED_SETTING.to_string(),
            Value::Bool(true)
        )));
    }

    #[test]
    fn kind_agent_mismatch_fails_closed() {
        let error = project_provider_account(
            ProviderAccountKind::ClaudeSubscription,
            None,
            Some("oauth"),
            ChatAgentProvider::Codex,
        )
        .unwrap_err();
        assert_eq!(error_code(&error), "account_kind_mismatch");
        let error = project_provider_account(
            ProviderAccountKind::OpenaiApiKey,
            None,
            Some("sk"),
            ChatAgentProvider::ClaudeCode,
        )
        .unwrap_err();
        assert_eq!(error_code(&error), "account_kind_mismatch");
    }

    #[test]
    fn base_url_rules_per_kind() {
        use ProviderAccountKind::*;
        // Required only for the compatible bearer kind.
        assert_eq!(
            error_code(
                &AnthropicCompatibleBearer
                    .validate_base_url(None)
                    .unwrap_err()
            ),
            "account_base_url_invalid"
        );
        for preset in ANTHROPIC_COMPATIBLE_BEARER_BASE_URL_PRESETS {
            AnthropicCompatibleBearer
                .validate_base_url(Some(preset))
                .expect("preset is valid");
        }
        // Optional for the api-key kinds.
        AnthropicApiKey.validate_base_url(None).unwrap();
        OpenaiApiKey.validate_base_url(None).unwrap();
        AnthropicApiKey
            .validate_base_url(Some("https://llm.example.test"))
            .unwrap();
        // Fixed-host kinds reject a supplied URL.
        for kind in [ClaudeSubscription, ChatgptSubscription, OpenrouterApiKey] {
            assert_eq!(
                error_code(
                    &kind
                        .validate_base_url(Some("https://api.anthropic.com"))
                        .unwrap_err()
                ),
                "account_base_url_invalid",
                "{kind}"
            );
            kind.validate_base_url(None).unwrap();
            kind.validate_base_url(Some("  ")).unwrap();
        }
        // Shape rules.
        for bad in [
            "http://llm.example.test",
            "ftp://llm.example.test",
            "llm.example.test",
            "https://user:pass@llm.example.test",
            "https://",
            "https://openrouter.ai/api",
            "https://gateway.openrouter.ai/api/v1",
        ] {
            assert_eq!(
                error_code(&OpenaiApiKey.validate_base_url(Some(bad)).unwrap_err()),
                "account_base_url_invalid",
                "{bad}"
            );
        }
        let too_long = format!(
            "https://llm.example.test/{}",
            "a".repeat(PROVIDER_ACCOUNT_BASE_URL_MAX_LEN)
        );
        assert_eq!(
            error_code(&OpenaiApiKey.validate_base_url(Some(&too_long)).unwrap_err()),
            "account_base_url_invalid"
        );
    }

    #[test]
    fn secret_presence_matches_kind() {
        let error = project_provider_account(
            ProviderAccountKind::AnthropicApiKey,
            None,
            Some("   "),
            ChatAgentProvider::ClaudeCode,
        )
        .unwrap_err();
        assert_eq!(error_code(&error), "account_secret_invalid");
        let error = project_provider_account(
            ProviderAccountKind::ChatgptSubscription,
            None,
            Some("not-allowed"),
            ChatAgentProvider::Codex,
        )
        .unwrap_err();
        assert_eq!(error_code(&error), "account_secret_invalid");
        assert!(ProviderAccountKind::ALL
            .iter()
            .all(|kind| kind.requires_secret()
                != (*kind == ProviderAccountKind::ChatgptSubscription)));
    }

    const OAUTH: &str = "sk-ant-oat01-0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789abcdefghij-A_cdefgh";
    const API_KEY: &str = "sk-ant-api03-0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789abcdefghij_A-cdefAA";

    #[test]
    fn submitted_secrets_are_trimmed_single_tokens_with_the_kind_shape() {
        use ProviderAccountKind::*;
        assert_eq!(
            ClaudeSubscription
                .validate_secret(Some(&format!("  {OAUTH}\n")), None)
                .unwrap()
                .as_deref(),
            Some(OAUTH)
        );
        assert_eq!(
            AnthropicApiKey
                .validate_secret(Some(API_KEY), None)
                .unwrap()
                .as_deref(),
            Some(API_KEY)
        );
        // Version digits and the API-key family are not pinned.
        assert!(ClaudeSubscription
            .validate_secret(Some("sk-ant-oat02-0123456789abcdefghijklmnop"), None)
            .is_ok());
        for key in [
            "sk-ant-api04-0123456789abcdefghijklmnop",
            "sk-ant-svc-0123456789abcdefghijklmnop",
        ] {
            assert!(
                AnthropicApiKey.validate_secret(Some(key), None).is_ok(),
                "{key}"
            );
        }
        assert!(OpenrouterApiKey
            .validate_secret(
                Some("sk-or-v1-0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"),
                None
            )
            .is_ok());
        for key in [
            "sk-proj-0123456789abcdefghijT3BlbkFJ0123456789abcdefghij",
            "sk-svcacct-0123456789abcdefghijT3BlbkFJ0123456789abcdefghij",
            "sk-0123456789abcdefghijT3BlbkFJ0123456789abcdefghij",
        ] {
            assert!(
                OpenaiApiKey.validate_secret(Some(key), None).is_ok(),
                "{key}"
            );
        }
        // The compatible bearer has no known shape; only the token rule applies.
        assert_eq!(
            AnthropicCompatibleBearer
                .validate_secret(
                    Some(" zai-bearer-1 "),
                    Some("https://api.z.ai/api/anthropic")
                )
                .unwrap()
                .as_deref(),
            Some("zai-bearer-1")
        );
        assert_eq!(
            ChatgptSubscription.validate_secret(None, None).unwrap(),
            None
        );

        // An API key in a subscription account is the QA failure this guards.
        let error = ClaudeSubscription
            .validate_secret(Some(API_KEY), None)
            .unwrap_err();
        assert_eq!(error_code(&error), "account_secret_invalid");
        assert!(error_text(&error).contains("claude setup-token"));
        let error = AnthropicApiKey
            .validate_secret(Some(OAUTH), None)
            .unwrap_err();
        assert_eq!(error_code(&error), "account_secret_invalid");
        assert!(error_text(&error).contains("sk-ant-api03-"));
        // Nor is an OAuth refresh token (`sk-ant-ort01-…`) or an Admin API
        // key (`sk-ant-admin01-…`), which cannot call Messages.
        for key in [
            "sk-ant-ort01-0123456789abcdefghijklmnop",
            "sk-ant-admin01-0123456789abcdefghijklmnop",
        ] {
            let error = AnthropicApiKey
                .validate_secret(Some(key), None)
                .unwrap_err();
            assert_eq!(error_code(&error), "account_secret_invalid", "{key}");
        }

        // Wrong prefix, missing family dash, truncated body, bad character.
        for (kind, value) in [
            (OpenrouterApiKey, "or-v1-0123456789abcdef0123456789abcdef"),
            (OpenaiApiKey, "pk-0123456789abcdefghijklmnop"),
            (ClaudeSubscription, "sk-ant-oat0123456789abcdefghijklmnop"),
            (ClaudeSubscription, "sk-ant-oat01-0123456789abcdefghi"),
            (AnthropicApiKey, "sk-ant-api030123456789abcdefghijklmnop"),
            (AnthropicApiKey, "sk-ant-api03-0123456789abcdefghij$klmnop"),
            (
                OpenrouterApiKey,
                "sk-or-v1-0123456789abcdef_0123456789abcdef",
            ),
            (
                OpenrouterApiKey,
                "sk-or-v1-zzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz",
            ),
        ] {
            let error = kind.validate_secret(Some(value), None).unwrap_err();
            assert_eq!(
                error_code(&error),
                "account_secret_invalid",
                "{kind}: {value}"
            );
            assert!(!error_text(&error).contains("one line"), "{kind}: {value}");
        }

        // Whitespace inside the value is rejected for every kind, including
        // the bearer, so a line-wrapped paste never becomes a credential.
        for (kind, value) in [
            (
                ClaudeSubscription,
                "sk-ant-oat01-0123456789 abcdefghijklmnop",
            ),
            (AnthropicApiKey, "sk-ant-api03-0123456789\nabcdefghijklmnop"),
            (AnthropicCompatibleBearer, "zai\tbearer"),
            (OpenaiApiKey, "sk-proj-0123456789\r\nabcdefghijklmnop"),
        ] {
            let error = kind.validate_secret(Some(value), None).unwrap_err();
            assert_eq!(error_code(&error), "account_secret_invalid", "{kind}");
            assert!(error_text(&error).contains("one line"), "{kind}");
        }
        assert_eq!(
            error_code(
                &ClaudeSubscription
                    .validate_secret(Some("  "), None)
                    .unwrap_err()
            ),
            "account_secret_invalid"
        );
        assert_eq!(
            error_code(
                &ChatgptSubscription
                    .validate_secret(Some("x"), None)
                    .unwrap_err()
            ),
            "account_secret_invalid"
        );
    }

    #[test]
    fn native_shapes_do_not_apply_behind_a_custom_gateway() {
        use ProviderAccountKind::*;
        // An API-key kind pointed at a compatible gateway carries that
        // gateway's key, whose shape G5 cannot know.
        for (kind, url) in [
            (AnthropicApiKey, "https://llm.example.test/anthropic"),
            (OpenaiApiKey, "https://llm.example.test/openai/v1"),
        ] {
            assert!(!kind.secret_shape_applies(Some(url)), "{kind}");
            assert_eq!(
                kind.validate_secret(Some("gateway-key-0001"), Some(url))
                    .unwrap()
                    .as_deref(),
                Some("gateway-key-0001"),
                "{kind}"
            );
        }
        // The provider's own host, with or without a path or trailing slash,
        // keeps the shape; so does an absent base URL.
        for url in [
            None,
            Some("https://api.anthropic.com"),
            Some("https://api.anthropic.com/"),
            Some("HTTPS://API.ANTHROPIC.COM/v1"),
            Some("https://api.anthropic.com:443/v1"),
        ] {
            assert!(AnthropicApiKey.secret_shape_applies(url), "{url:?}");
            assert!(AnthropicApiKey
                .validate_secret(Some("gateway-key-0001"), url)
                .is_err());
        }
        for url in [
            "https://api.anthropic.com.evil.test",
            "https://evil.test/https://api.anthropic.com",
            "https://api.anthropic.com:8443",
            "http://api.anthropic.com",
            "not a url",
        ] {
            assert!(!AnthropicApiKey.secret_shape_applies(Some(url)), "{url}");
        }
        assert!(OpenaiApiKey.secret_shape_applies(Some("https://api.openai.com/v1")));
        // Fixed-host kinds always apply; shapeless kinds never do.
        assert!(ClaudeSubscription.secret_shape_applies(None));
        assert!(OpenrouterApiKey.secret_shape_applies(None));
        assert!(!AnthropicCompatibleBearer.secret_shape_applies(None));
        assert!(!ChatgptSubscription.secret_shape_applies(None));
    }

    #[test]
    fn stored_secrets_project_without_the_shape_rule() {
        // A credential saved before the shape rule still dispatches.
        let projection = project_provider_account(
            ProviderAccountKind::ClaudeSubscription,
            None,
            Some("legacy-token-without-prefix"),
            ChatAgentProvider::ClaudeCode,
        )
        .unwrap();
        assert!(projection
            .secrets
            .iter()
            .any(|(key, value)| key == CLAUDE_CODE_OAUTH_TOKEN_ENV
                && value == "legacy-token-without-prefix"));
    }

    #[test]
    fn kind_and_scope_round_trip_through_serde_and_parse() {
        for kind in ProviderAccountKind::ALL {
            assert_eq!(ProviderAccountKind::parse(kind.as_str()), Some(*kind));
            assert_eq!(kind.as_str().parse::<ProviderAccountKind>().unwrap(), *kind);
            let json = serde_json::to_value(kind).unwrap();
            assert_eq!(json, Value::String(kind.as_str().to_string()));
            assert_eq!(
                serde_json::from_value::<ProviderAccountKind>(json).unwrap(),
                *kind
            );
        }
        assert_eq!(ProviderAccountKind::parse("gemini_api_key"), None);
        assert_eq!(
            error_code(&"gemini_api_key".parse::<ProviderAccountKind>().unwrap_err()),
            "account_kind_invalid"
        );
        for scope in ProviderAccountScope::ALL {
            assert_eq!(ProviderAccountScope::parse(scope.as_str()), Some(*scope));
            assert_eq!(
                serde_json::to_value(scope).unwrap(),
                Value::String(scope.as_str().to_string())
            );
        }
        assert_eq!(ProviderAccountScope::parse("team"), None);
        assert_eq!(
            ProviderAccountKind::ClaudeSubscription.family().as_str(),
            "claude"
        );
        assert_eq!(ProviderAccountKind::OpenaiApiKey.family().as_str(), "codex");
        assert_eq!(
            ProviderAccountKind::OpenrouterApiKey.family().as_str(),
            "both"
        );
    }

    #[test]
    fn account_selection_uses_the_kind_tagged_camel_case_wire_shape() {
        let id = Uuid::new_v4();
        let account = AgentAccountSelectionV1::Account { account_id: id };
        assert_eq!(
            serde_json::to_value(&account).unwrap(),
            serde_json::json!({"kind": "account", "accountId": id.to_string()})
        );
        assert_eq!(
            serde_json::to_value(AgentAccountSelectionV1::RuntimeConfig).unwrap(),
            serde_json::json!({"kind": "runtime_config"})
        );
        assert_eq!(
            serde_json::from_value::<AgentAccountSelectionV1>(
                serde_json::json!({"kind": "runtime_config"})
            )
            .unwrap(),
            AgentAccountSelectionV1::RuntimeConfig
        );
        assert!(serde_json::from_value::<AgentAccountSelectionV1>(
            serde_json::json!({"kind": "account", "accountId": id.to_string(), "name": "x"})
        )
        .is_err());
        assert!(serde_json::from_value::<AgentAccountSelectionV1>(
            serde_json::json!({"kind": "account"})
        )
        .is_err());
        assert!(serde_json::from_value::<AgentAccountSelectionV1>(
            serde_json::json!({"kind": "raw_keys"})
        )
        .is_err());
    }
}
