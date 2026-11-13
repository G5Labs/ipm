//! Claude Code model alias resolution shared by API and worker code.
//!
//! The catalog carries direct Claude Code ids and OpenRouter `provider/model`
//! ids. User-facing selectors present friendly logical models; the backend
//! resolver projects the form expected by the active provider route.

use http::Uri;

pub const CLAUDE_CODE_CLI_VERSION: &str = include_str!("../../../scripts/claude-code-cli-version");

/// Bundled chat default (core D7; owner request 2026-11-09): the current Opus
/// in its plain direct id, since every offered family runs its 1M window
/// without the `[1m]` suffix. Persisted choices equal to an earlier bundled
/// default follow this one (`useUserSettings` legacy folding).
pub const CLAUDE_DIRECT_CHAT_DEFAULT_MODEL: &str = "claude-opus-5-5";
pub const CLAUDE_OPENROUTER_CHAT_DEFAULT_MODEL: &str = "anthropic/claude-opus-5.5";
pub const CLAUDE_SONNET_4_6_MODEL: &str = "claude-sonnet-4-6";
pub const ANTHROPIC_BASE_URL_ENV: &str = "ANTHROPIC_BASE_URL";
pub const ANTHROPIC_AUTH_TOKEN_ENV: &str = "ANTHROPIC_AUTH_TOKEN";
pub const CLAUDE_CODE_OAUTH_TOKEN_ENV: &str = "CLAUDE_CODE_OAUTH_TOKEN";
pub const ANTHROPIC_API_KEY_ENV: &str = "ANTHROPIC_API_KEY";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClaudeModelProvider {
    DirectAnthropic,
    OpenRouter,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClaudeProviderRouteIssue {
    OAuthTokenWithOpenRouter,
}

impl ClaudeProviderRouteIssue {
    #[must_use]
    pub fn code(self) -> &'static str {
        match self {
            ClaudeProviderRouteIssue::OAuthTokenWithOpenRouter => {
                "mixed_claude_oauth_openrouter_route"
            }
        }
    }

    #[must_use]
    pub fn message(self) -> &'static str {
        match self {
            ClaudeProviderRouteIssue::OAuthTokenWithOpenRouter => {
                "CLAUDE_CODE_OAUTH_TOKEN is only supported for direct Claude Code auth. \
                 Remove CLAUDE_CODE_OAUTH_TOKEN or switch away from the OpenRouter \
                 ANTHROPIC_BASE_URL; for OpenRouter use OPENROUTER_API_KEY, or \
                 ANTHROPIC_AUTH_TOKEN with ANTHROPIC_BASE_URL=https://openrouter.ai/api."
            }
        }
    }
}

/// Reasoning-effort ladder shared by every Claude model, weakest to strongest
/// (core D7). Per-model `efforts` sets are ordered sub-slices of this ladder,
/// or the sole G5 `none` absence marker when the model has no effort control.
pub const CLAUDE_EFFORT_ORDER: &[&str] = &["low", "medium", "high", "xhigh", "max"];

/// Native inventory checked on the pin, including modes G5 does not execute.
pub const CLAUDE_NATIVE_MODES: &[&str] = &["default", "acceptEdits", "plan", "bypassPermissions"];
/// Offered chat modes must be executable by the production permission mapper.
pub const CLAUDE_CHAT_MODES: &[&str] = &["default", "bypassPermissions"];

/// Default chat permission mode for the Claude Code harness (core D7).
pub const CLAUDE_DEFAULT_CHAT_MODE: &str = "default";

/// One direct-route model id. Bare and `[1m]` ids are distinct variants, never
/// aliases of one another (core D2/D7). Every offered family runs its 1M window
/// without the suffix (Claude Code model configuration: Fable, Sonnet 5 and
/// later, Opus 4.7 and later), so `[1m]` variants stay resolver-known for
/// saved selections but are no longer offered.
#[derive(Debug)]
pub struct ClaudeModelVariant {
    pub id: &'static str,
    pub label: &'static str,
    /// Whether the picker offers this variant; resolver-known ids may be unoffered.
    pub offered: bool,
}

#[derive(Debug)]
pub struct ClaudeModelMapping {
    pub openrouter: &'static str,
    pub direct_preferred: &'static str,
    pub direct_aliases: &'static [&'static str],
    pub variants: &'static [ClaudeModelVariant],
    /// Whether the OpenRouter picker offers this family's slug.
    pub openrouter_offered: bool,
    /// Display label for the OpenRouter slug; `None` renders the slug itself.
    pub openrouter_label: Option<&'static str>,
    /// Ordered sub-slice of `CLAUDE_EFFORT_ORDER`, or sole `none` for no control.
    pub efforts: &'static [&'static str],
    /// The family's default reasoning effort — `high` when supported, else `none`
    /// (core D7, chat intent 36046a83; supersedes the plan D-6
    /// highest-supported rule).
    pub default_effort: &'static str,
    /// Anthropic's first-party release day (`YYYY-MM-DD`, model deprecations
    /// table). The catalog lists families newest first, and the composer
    /// picker follows that order (owner request 2026-11-09).
    pub released: &'static str,
}

const FULL_EFFORTS: &[&str] = &["low", "medium", "high", "xhigh", "max"];
const CAPPED_EFFORTS: &[&str] = &["low", "medium", "high"];
/// G5's explicit absence marker: omit --effort for models without that control.
const NO_EFFORT_CONTROL: &[&str] = &["none"];

const CLAUDE_MODEL_MAPPINGS: &[ClaudeModelMapping] = &[
    // Added with the 2.1.321 CLI bump (owner request 2026-11-09): Anthropic
    // released Fable 5.2 on 2026-11-09 as the current Fable; Claude Code
    // accepts `claude-fable-5-2` from 2.1.321 and OpenRouter serves the slug.
    ClaudeModelMapping {
        openrouter: "anthropic/claude-fable-5.2",
        direct_preferred: "claude-fable-5-2",
        direct_aliases: &["claude-fable-5-2"],
        variants: &[ClaudeModelVariant {
            id: "claude-fable-5-2",
            label: "Fable 5.2",
            offered: true,
        }],
        openrouter_offered: true,
        openrouter_label: None,
        efforts: FULL_EFFORTS,
        default_effort: "high",
        released: "2026-11-09",
    },
    ClaudeModelMapping {
        openrouter: "anthropic/claude-sonnet-5.5",
        direct_preferred: "claude-sonnet-5-5",
        direct_aliases: &["claude-sonnet-5-5"],
        variants: &[ClaudeModelVariant {
            id: "claude-sonnet-5-5",
            label: "Sonnet 5.5",
            offered: true,
        }],
        openrouter_offered: true,
        openrouter_label: None,
        efforts: FULL_EFFORTS,
        default_effort: "high",
        released: "2026-09-28",
    },
    // Added with the 2.1.285 CLI bump (provider-accounts
    // `g5.provider.catalog.pinned`): Anthropic lists Opus 5.5 and Sonnet 5.5
    // as current models, 2.1.285 accepted both ids and their `[1m]` forms on a
    // subscription login, and OpenRouter serves the `anthropic/` slugs.
    // The bundled chat default since 2026-11-09.
    ClaudeModelMapping {
        openrouter: "anthropic/claude-opus-5.5",
        direct_preferred: "claude-opus-5-5",
        direct_aliases: &["claude-opus-5-5", "claude-opus-5-5[1m]"],
        variants: &[
            ClaudeModelVariant {
                id: "claude-opus-5-5",
                label: "Opus 5.5",
                offered: true,
            },
            ClaudeModelVariant {
                id: "claude-opus-5-5[1m]",
                label: "Opus 5.5 (1M)",
                offered: false,
            },
        ],
        openrouter_offered: true,
        openrouter_label: Some("Opus 5.5"),
        efforts: FULL_EFFORTS,
        default_effort: "high",
        released: "2026-09-22",
    },
    ClaudeModelMapping {
        openrouter: "anthropic/claude-fable-5.1",
        direct_preferred: "claude-fable-5-1",
        direct_aliases: &["claude-fable-5-1", "claude-fable-5-1[1m]"],
        variants: &[
            ClaudeModelVariant {
                id: "claude-fable-5-1",
                label: "Fable 5.1",
                offered: true,
            },
            ClaudeModelVariant {
                id: "claude-fable-5-1[1m]",
                label: "Fable 5.1 (1M context)",
                offered: false,
            },
        ],
        openrouter_offered: true,
        openrouter_label: None,
        efforts: FULL_EFFORTS,
        default_effort: "high",
        released: "2026-09-01",
    },
    ClaudeModelMapping {
        openrouter: "anthropic/claude-opus-5",
        direct_preferred: "claude-opus-5",
        direct_aliases: &["claude-opus-5", "claude-opus-5[1m]"],
        variants: &[
            ClaudeModelVariant {
                id: "claude-opus-5",
                label: "Opus 5",
                offered: true,
            },
            ClaudeModelVariant {
                id: "claude-opus-5[1m]",
                label: "Opus 5 (1M)",
                offered: false,
            },
        ],
        openrouter_offered: true,
        openrouter_label: Some("Opus 5"),
        efforts: FULL_EFFORTS,
        default_effort: "high",
        released: "2026-07-24",
    },
    ClaudeModelMapping {
        openrouter: "anthropic/claude-sonnet-5",
        direct_preferred: "claude-sonnet-5",
        direct_aliases: &["claude-sonnet-5"],
        variants: &[ClaudeModelVariant {
            id: "claude-sonnet-5",
            label: "Sonnet 5",
            offered: true,
        }],
        openrouter_offered: true,
        openrouter_label: None,
        efforts: FULL_EFFORTS,
        default_effort: "high",
        released: "2026-06-30",
    },
    ClaudeModelMapping {
        openrouter: "anthropic/claude-fable-5",
        direct_preferred: "claude-fable-5",
        direct_aliases: &["claude-fable-5", "claude-fable-5[1m]"],
        variants: &[
            ClaudeModelVariant {
                id: "claude-fable-5",
                label: "Fable 5",
                offered: true,
            },
            ClaudeModelVariant {
                id: "claude-fable-5[1m]",
                label: "Fable 5 (1M context)",
                offered: false,
            },
        ],
        openrouter_offered: true,
        openrouter_label: None,
        efforts: FULL_EFFORTS,
        default_effort: "high",
        released: "2026-06-09",
    },
    // The bundled chat default until 2026-11-09; its `[1m]` id was that default.
    ClaudeModelMapping {
        openrouter: "anthropic/claude-opus-4.8",
        direct_preferred: "claude-opus-4-8",
        direct_aliases: &["claude-opus-4-8", "claude-opus-4-8[1m]"],
        variants: &[
            ClaudeModelVariant {
                id: "claude-opus-4-8",
                label: "Opus 4.8",
                offered: true,
            },
            ClaudeModelVariant {
                id: "claude-opus-4-8[1m]",
                label: "Opus 4.8 (1M)",
                offered: false,
            },
        ],
        openrouter_offered: true,
        openrouter_label: Some("Opus 4.8"),
        efforts: FULL_EFFORTS,
        default_effort: "high",
        released: "2026-05-28",
    },
    ClaudeModelMapping {
        openrouter: "anthropic/claude-opus-4.7",
        direct_preferred: "claude-opus-4-7",
        direct_aliases: &["claude-opus-4-7"],
        variants: &[ClaudeModelVariant {
            id: "claude-opus-4-7",
            label: "Opus 4.7",
            offered: false,
        }],
        openrouter_offered: false,
        openrouter_label: None,
        efforts: FULL_EFFORTS,
        default_effort: "high",
        released: "2026-04-16",
    },
    ClaudeModelMapping {
        openrouter: "anthropic/claude-sonnet-4.6",
        direct_preferred: CLAUDE_SONNET_4_6_MODEL,
        direct_aliases: &[CLAUDE_SONNET_4_6_MODEL],
        variants: &[ClaudeModelVariant {
            id: CLAUDE_SONNET_4_6_MODEL,
            label: "Sonnet 4.6",
            offered: true,
        }],
        openrouter_offered: true,
        openrouter_label: None,
        efforts: CAPPED_EFFORTS,
        default_effort: "high",
        released: "2026-02-17",
    },
    ClaudeModelMapping {
        openrouter: "anthropic/claude-opus-4.6",
        direct_preferred: "claude-opus-4-6",
        direct_aliases: &["claude-opus-4-6"],
        variants: &[ClaudeModelVariant {
            id: "claude-opus-4-6",
            label: "Opus 4.6",
            offered: false,
        }],
        openrouter_offered: false,
        openrouter_label: None,
        efforts: FULL_EFFORTS,
        default_effort: "high",
        released: "2026-02-05",
    },
    ClaudeModelMapping {
        openrouter: "anthropic/claude-haiku-4.5",
        direct_preferred: "claude-haiku-4-5",
        direct_aliases: &["claude-haiku-4-5"],
        variants: &[ClaudeModelVariant {
            id: "claude-haiku-4-5",
            label: "Haiku 4.5",
            offered: true,
        }],
        openrouter_offered: true,
        openrouter_label: None,
        efforts: NO_EFFORT_CONTROL,
        default_effort: "none",
        released: "2025-10-15",
    },
];

/// The enriched per-family Claude catalog (core D7): route forms plus picker
/// variants, offered flags, effort sets, and defaults. Feeds the generated
/// client catalog data.
#[must_use]
pub fn claude_model_catalog() -> &'static [ClaudeModelMapping] {
    CLAUDE_MODEL_MAPPINGS
}

/// Teach the native CLI the capabilities of G5's dotted OpenRouter slugs.
/// Without this documented mapping, the CLI accepts older slugs but silently
/// omits output_config.effort. Unknown pins retain their native configuration.
#[must_use]
pub fn claude_native_model_overrides(model: &str) -> Option<serde_json::Value> {
    if !CLAUDE_MODEL_MAPPINGS
        .iter()
        .any(|family| family.openrouter == model)
    {
        return None;
    }
    Some(serde_json::Value::Object(
        CLAUDE_MODEL_MAPPINGS
            .iter()
            .map(|family| {
                (
                    family.direct_preferred.trim_end_matches("[1m]").to_string(),
                    serde_json::Value::String(family.openrouter.to_string()),
                )
            })
            .collect(),
    ))
}

#[must_use]
pub fn claude_model_provider_from_anthropic_base_url(
    base_url: Option<&str>,
) -> ClaudeModelProvider {
    let Some(base_url) = base_url.map(str::trim).filter(|value| !value.is_empty()) else {
        return ClaudeModelProvider::DirectAnthropic;
    };

    let Ok(uri) = base_url.parse::<Uri>() else {
        return ClaudeModelProvider::DirectAnthropic;
    };
    let Some(host) = uri.host().map(|value| value.to_ascii_lowercase()) else {
        return ClaudeModelProvider::DirectAnthropic;
    };

    if host == "openrouter.ai" || host.ends_with(".openrouter.ai") {
        ClaudeModelProvider::OpenRouter
    } else {
        ClaudeModelProvider::DirectAnthropic
    }
}

#[must_use]
pub fn has_non_empty_value(value: Option<&str>) -> bool {
    value.map(str::trim).is_some_and(|value| !value.is_empty())
}

pub fn validate_claude_provider_route(
    base_url: Option<&str>,
    claude_code_oauth_token: Option<&str>,
) -> Result<(), ClaudeProviderRouteIssue> {
    let provider = claude_model_provider_from_anthropic_base_url(base_url);
    if provider == ClaudeModelProvider::OpenRouter && has_non_empty_value(claude_code_oauth_token) {
        return Err(ClaudeProviderRouteIssue::OAuthTokenWithOpenRouter);
    }
    Ok(())
}

/// Per-provider fallback effort for models the catalog does not know (#1370,
/// core D7): out-of-catalog pins have no `default_effort` to derive, so a
/// conservative mid-ladder value applies. Kept per agent by user decision.
pub const CLAUDE_UNKNOWN_MODEL_EFFORT: &str = "xhigh";

/// Resolve the catalog `default_effort` for a model id in either route form
/// (#1370, core D7): variant ids and `direct_aliases` match their family, the
/// OpenRouter slug matches too, and unknown models fall back to
/// `CLAUDE_UNKNOWN_MODEL_EFFORT`. This is the runtime resolution for
/// blank/unpinned efforts — the same rule the generated client catalog serves.
#[must_use]
pub fn claude_default_effort_for_model(model: &str) -> &'static str {
    let trimmed = model.trim();
    if trimmed.is_empty() {
        return CLAUDE_UNKNOWN_MODEL_EFFORT;
    }
    for mapping in CLAUDE_MODEL_MAPPINGS {
        if trimmed == mapping.openrouter
            || mapping.direct_aliases.contains(&trimmed)
            || mapping.variants.iter().any(|variant| variant.id == trimmed)
        {
            return mapping.default_effort;
        }
    }
    CLAUDE_UNKNOWN_MODEL_EFFORT
}

#[must_use]
pub fn default_claude_chat_model_for_provider(provider: ClaudeModelProvider) -> &'static str {
    match provider {
        ClaudeModelProvider::DirectAnthropic => CLAUDE_DIRECT_CHAT_DEFAULT_MODEL,
        ClaudeModelProvider::OpenRouter => CLAUDE_OPENROUTER_CHAT_DEFAULT_MODEL,
    }
}

#[must_use]
pub fn normalize_claude_model_for_provider(model: &str, provider: ClaudeModelProvider) -> String {
    let trimmed = model.trim();
    if trimmed.is_empty() {
        return String::new();
    }

    for mapping in CLAUDE_MODEL_MAPPINGS {
        if trimmed == mapping.openrouter || mapping.direct_aliases.contains(&trimmed) {
            return match provider {
                // Cross-route translation only (core D2): a recognized direct id —
                // bare or [1m] — is already in the direct route's form and passes
                // through verbatim; the [1m] context opt-in is the user's choice.
                ClaudeModelProvider::DirectAnthropic => {
                    if mapping.direct_aliases.contains(&trimmed) {
                        trimmed.to_string()
                    } else {
                        mapping.direct_preferred.to_string()
                    }
                }
                ClaudeModelProvider::OpenRouter => mapping.openrouter.to_string(),
            };
        }
    }

    trimmed.to_string()
}

#[must_use]
pub fn normalize_optional_claude_model_for_provider(
    model: Option<&str>,
    provider: ClaudeModelProvider,
) -> Option<String> {
    let model = model.map(str::trim).filter(|value| !value.is_empty())?;
    Some(normalize_claude_model_for_provider(model, provider))
}

#[must_use]
pub fn resolve_claude_chat_model_for_provider(
    model: Option<&str>,
    provider: ClaudeModelProvider,
) -> String {
    normalize_optional_claude_model_for_provider(model, provider)
        .unwrap_or_else(|| default_claude_chat_model_for_provider(provider).to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_effort_resolves_per_model_in_both_route_forms() {
        // #1370 alignment (core D7): variant ids, direct aliases, and the
        // OpenRouter slug all resolve to the family's default_effort; unknown
        // and blank models use the per-provider fallback.
        assert_eq!(
            claude_default_effort_for_model("claude-opus-4-8[1m]"),
            "high"
        );
        assert_eq!(claude_default_effort_for_model("claude-sonnet-4-6"), "high");
        assert_eq!(claude_default_effort_for_model("claude-sonnet-5"), "high");
        assert_eq!(claude_default_effort_for_model("claude-opus-5[1m]"), "high");
        assert_eq!(
            claude_default_effort_for_model("anthropic/claude-opus-4.8"),
            "high"
        );
        assert_eq!(
            claude_default_effort_for_model("custom/claude-unknown"),
            CLAUDE_UNKNOWN_MODEL_EFFORT
        );
        assert_eq!(
            claude_default_effort_for_model("  "),
            CLAUDE_UNKNOWN_MODEL_EFFORT
        );
        assert_eq!(CLAUDE_UNKNOWN_MODEL_EFFORT, "xhigh");
    }

    #[test]
    fn provider_defaults_to_direct_without_openrouter_base_url() {
        assert_eq!(
            claude_model_provider_from_anthropic_base_url(None),
            ClaudeModelProvider::DirectAnthropic
        );
        assert_eq!(
            claude_model_provider_from_anthropic_base_url(Some("https://api.anthropic.com")),
            ClaudeModelProvider::DirectAnthropic
        );
    }

    #[test]
    fn provider_detects_openrouter_hosts() {
        assert_eq!(
            claude_model_provider_from_anthropic_base_url(Some("https://openrouter.ai/api")),
            ClaudeModelProvider::OpenRouter
        );
        assert_eq!(
            claude_model_provider_from_anthropic_base_url(Some(
                "https://gateway.openrouter.ai/api/v1"
            )),
            ClaudeModelProvider::OpenRouter
        );
    }

    #[test]
    fn provider_route_rejects_oauth_token_with_openrouter_base_url() {
        assert_eq!(
            validate_claude_provider_route(
                Some("https://openrouter.ai/api"),
                Some("sk-ant-oat01-real")
            ),
            Err(ClaudeProviderRouteIssue::OAuthTokenWithOpenRouter)
        );
    }

    #[test]
    fn provider_route_allows_oauth_token_with_direct_base_url() {
        assert_eq!(
            validate_claude_provider_route(
                Some("https://api.anthropic.com"),
                Some("sk-ant-oat01-real")
            ),
            Ok(())
        );
        assert_eq!(
            validate_claude_provider_route(None, Some("sk-ant-oat01-real")),
            Ok(())
        );
    }

    #[test]
    fn direct_provider_maps_openrouter_opus_to_claude_code_default() {
        assert_eq!(
            normalize_claude_model_for_provider(
                "anthropic/claude-opus-5.5",
                ClaudeModelProvider::DirectAnthropic
            ),
            CLAUDE_DIRECT_CHAT_DEFAULT_MODEL
        );
        // Every family's preferred direct form is its plain id: the 1M window
        // needs no `[1m]` suffix on any offered family.
        assert_eq!(
            normalize_claude_model_for_provider(
                "anthropic/claude-opus-4.8",
                ClaudeModelProvider::DirectAnthropic
            ),
            "claude-opus-4-8"
        );
    }

    #[test]
    fn direct_provider_preserves_explicit_direct_ids_verbatim() {
        for model in [
            "claude-fable-5-2",
            "claude-fable-5",
            "claude-fable-5[1m]",
            "claude-opus-5-5",
            "claude-opus-5-5[1m]",
            "claude-opus-5",
            "claude-opus-5[1m]",
            "claude-opus-4-8",
            "claude-opus-4-8[1m]",
            "claude-sonnet-5",
            "claude-sonnet-4-6",
        ] {
            assert_eq!(
                normalize_claude_model_for_provider(model, ClaudeModelProvider::DirectAnthropic),
                model,
                "direct route must not rewrite an explicitly selected direct id"
            );
        }
    }

    #[test]
    fn direct_provider_maps_openrouter_fable_to_preferred_direct_id() {
        assert_eq!(
            normalize_claude_model_for_provider(
                "anthropic/claude-fable-5",
                ClaudeModelProvider::DirectAnthropic
            ),
            "claude-fable-5"
        );
        assert_eq!(
            normalize_claude_model_for_provider(
                "anthropic/claude-fable-5.2",
                ClaudeModelProvider::DirectAnthropic
            ),
            "claude-fable-5-2"
        );
    }

    #[test]
    fn openrouter_provider_collapses_direct_fable_variants_to_family_slug() {
        assert_eq!(
            normalize_claude_model_for_provider("claude-fable-5", ClaudeModelProvider::OpenRouter),
            "anthropic/claude-fable-5"
        );
        assert_eq!(
            normalize_claude_model_for_provider(
                "claude-fable-5[1m]",
                ClaudeModelProvider::OpenRouter
            ),
            "anthropic/claude-fable-5"
        );
    }

    #[test]
    fn openrouter_provider_maps_direct_opus_variants_to_openrouter_slug() {
        // Saved `[1m]` selections stay resolver-known although unoffered.
        assert_eq!(
            normalize_claude_model_for_provider(
                "claude-opus-4-8[1m]",
                ClaudeModelProvider::OpenRouter
            ),
            "anthropic/claude-opus-4.8"
        );
        assert_eq!(
            normalize_claude_model_for_provider("claude-opus-4-8", ClaudeModelProvider::OpenRouter),
            "anthropic/claude-opus-4.8"
        );
        assert_eq!(
            normalize_claude_model_for_provider("claude-opus-5-5", ClaudeModelProvider::OpenRouter),
            CLAUDE_OPENROUTER_CHAT_DEFAULT_MODEL
        );
    }

    #[test]
    fn unknown_models_are_trimmed_but_not_rewritten() {
        assert_eq!(
            normalize_claude_model_for_provider(
                "  custom/provider-model  ",
                ClaudeModelProvider::DirectAnthropic
            ),
            "custom/provider-model"
        );
    }

    #[test]
    fn chat_default_is_provider_specific() {
        assert_eq!(
            resolve_claude_chat_model_for_provider(None, ClaudeModelProvider::DirectAnthropic),
            CLAUDE_DIRECT_CHAT_DEFAULT_MODEL
        );
        assert_eq!(
            resolve_claude_chat_model_for_provider(None, ClaudeModelProvider::OpenRouter),
            CLAUDE_OPENROUTER_CHAT_DEFAULT_MODEL
        );
    }

    fn effort_rank(effort: &str) -> usize {
        CLAUDE_EFFORT_ORDER
            .iter()
            .position(|candidate| *candidate == effort)
            .unwrap_or_else(|| panic!("effort {effort} is not on CLAUDE_EFFORT_ORDER"))
    }

    #[test]
    fn catalog_exposes_labeled_variants_and_ordered_efforts_for_every_family() {
        for mapping in claude_model_catalog() {
            assert!(
                !mapping.variants.is_empty(),
                "{} has no variants",
                mapping.openrouter
            );
            for variant in mapping.variants {
                assert!(
                    !variant.label.trim().is_empty(),
                    "{} has an unlabeled variant",
                    variant.id
                );
            }
            assert!(
                !mapping.efforts.is_empty(),
                "{} has no efforts",
                mapping.openrouter
            );
            let ranks: Vec<usize> = mapping
                .efforts
                .iter()
                .filter(|effort| **effort != "none")
                .map(|effort| effort_rank(effort))
                .collect();
            assert!(
                ranks.windows(2).all(|pair| pair[0] < pair[1]),
                "{} efforts are not in ladder order",
                mapping.openrouter
            );
        }
    }

    #[test]
    fn default_effort_is_high_or_absent_and_within_each_effort_set() {
        // Core D7 (chat intent 36046a83): every Claude family defaults to
        // `high` — a cost/latency default, not the highest supported effort.
        for mapping in claude_model_catalog() {
            assert!(
                mapping.efforts.contains(&mapping.default_effort),
                "{} default effort is outside its effort set",
                mapping.openrouter
            );
            assert_eq!(
                mapping.default_effort,
                if mapping.efforts == NO_EFFORT_CONTROL {
                    "none"
                } else {
                    "high"
                },
                "{} default effort must be high, or absent when unsupported",
                mapping.openrouter
            );
        }
    }

    #[test]
    fn effort_sets_match_per_family_capabilities() {
        // Per-family capability table (core D7): the cap is a per-model
        // capability fact, not a Sonnet-family policy — Sonnet 5 is the first
        // Sonnet with the full ladder. Sonnet 4.6 is capped; Haiku 4.5 has
        // no effort control (the native pin matrix checks actual requests).
        // Families are listed newest first (see
        // `families_are_listed_newest_first_by_release_date`).
        let expected: &[(&str, &[&str], &str)] = &[
            ("anthropic/claude-fable-5.2", FULL_EFFORTS, "high"),
            ("anthropic/claude-sonnet-5.5", FULL_EFFORTS, "high"),
            ("anthropic/claude-opus-5.5", FULL_EFFORTS, "high"),
            ("anthropic/claude-fable-5.1", FULL_EFFORTS, "high"),
            ("anthropic/claude-opus-5", FULL_EFFORTS, "high"),
            ("anthropic/claude-sonnet-5", FULL_EFFORTS, "high"),
            ("anthropic/claude-fable-5", FULL_EFFORTS, "high"),
            ("anthropic/claude-opus-4.8", FULL_EFFORTS, "high"),
            ("anthropic/claude-opus-4.7", FULL_EFFORTS, "high"),
            ("anthropic/claude-sonnet-4.6", CAPPED_EFFORTS, "high"),
            ("anthropic/claude-opus-4.6", FULL_EFFORTS, "high"),
            ("anthropic/claude-haiku-4.5", NO_EFFORT_CONTROL, "none"),
        ];
        assert_eq!(
            claude_model_catalog().len(),
            expected.len(),
            "catalog families drifted from the per-family effort table"
        );
        for (mapping, (slug, efforts, default_effort)) in
            claude_model_catalog().iter().zip(expected)
        {
            assert_eq!(mapping.openrouter, *slug, "catalog family order drifted");
            assert_eq!(mapping.efforts, *efforts, "{slug} effort set drifted");
            assert_eq!(
                mapping.default_effort, *default_effort,
                "{slug} default effort drifted"
            );
        }
    }

    #[test]
    fn variant_ids_stay_in_lockstep_with_direct_aliases() {
        for mapping in claude_model_catalog() {
            let variant_ids: Vec<&str> =
                mapping.variants.iter().map(|variant| variant.id).collect();
            assert_eq!(
                variant_ids, mapping.direct_aliases,
                "{} catalog variants drifted from resolver aliases",
                mapping.openrouter
            );
        }
    }

    #[test]
    fn offered_variants_match_the_current_picker_lineup() {
        // The lineup below was re-checked against this pin; a pin change must
        // re-check it (`g5.provider.catalog.pinned`).
        assert_eq!(
            include_str!("../../../scripts/claude-code-cli-version").trim(),
            "2.1.321",
            "re-check the offered Claude models against the new pin, then update this test"
        );
        let offered_direct: Vec<&str> = claude_model_catalog()
            .iter()
            .flat_map(|mapping| mapping.variants)
            .filter(|variant| variant.offered)
            .map(|variant| variant.id)
            .collect();
        // One plain form per family (owner request 2026-11-09): every offered
        // family runs its 1M window without the `[1m]` suffix, so the suffixed
        // variants are no longer offered. They stay resolver-known for saved
        // selections (`variant_ids_stay_in_lockstep_with_direct_aliases`).
        assert_eq!(
            offered_direct,
            vec![
                "claude-fable-5-2",
                "claude-sonnet-5-5",
                "claude-opus-5-5",
                "claude-fable-5-1",
                "claude-opus-5",
                "claude-sonnet-5",
                "claude-fable-5",
                "claude-opus-4-8",
                "claude-sonnet-4-6",
                "claude-haiku-4-5",
            ]
        );
        assert!(
            offered_direct.contains(&CLAUDE_DIRECT_CHAT_DEFAULT_MODEL),
            "the bundled chat default must be an offered plain variant"
        );
        assert!(
            !offered_direct.iter().any(|id| id.ends_with("[1m]")),
            "no `[1m]` variant is offered: every offered family defaults to 1M"
        );

        let offered_openrouter: Vec<&str> = claude_model_catalog()
            .iter()
            .filter(|mapping| mapping.openrouter_offered)
            .map(|mapping| mapping.openrouter)
            .collect();
        assert_eq!(
            offered_openrouter,
            vec![
                "anthropic/claude-fable-5.2",
                "anthropic/claude-sonnet-5.5",
                "anthropic/claude-opus-5.5",
                "anthropic/claude-fable-5.1",
                "anthropic/claude-opus-5",
                "anthropic/claude-sonnet-5",
                "anthropic/claude-fable-5",
                "anthropic/claude-opus-4.8",
                "anthropic/claude-sonnet-4.6",
                "anthropic/claude-haiku-4.5",
            ]
        );
        assert!(offered_openrouter.contains(&CLAUDE_OPENROUTER_CHAT_DEFAULT_MODEL));
    }

    #[test]
    fn families_are_listed_newest_first_by_release_date() {
        // The composer picker follows catalog order (owner request 2026-11-09),
        // so the catalog itself is the release order, newest first. Dates are
        // Anthropic's first-party release days from the model deprecations
        // table; distinct families never share a day.
        let dates: Vec<&str> = claude_model_catalog()
            .iter()
            .map(|mapping| mapping.released)
            .collect();
        for date in &dates {
            assert!(
                date.len() == 10
                    && date.as_bytes()[4] == b'-'
                    && date.as_bytes()[7] == b'-'
                    && date
                        .bytes()
                        .enumerate()
                        .all(|(i, b)| matches!(i, 4 | 7) || b.is_ascii_digit()),
                "{date} is not a YYYY-MM-DD release day"
            );
        }
        assert!(
            dates.windows(2).all(|pair| pair[0] > pair[1]),
            "catalog families are not newest first: {dates:?}"
        );
        assert_eq!(dates[0], "2026-11-09", "Fable 5.2 leads the lineup");
    }

    #[test]
    fn every_preferred_direct_form_is_the_plain_id() {
        for mapping in claude_model_catalog() {
            assert!(
                !mapping.direct_preferred.ends_with("[1m]"),
                "{} prefers a suffixed direct form",
                mapping.openrouter
            );
            assert!(
                mapping
                    .variants
                    .iter()
                    .any(|variant| variant.id == mapping.direct_preferred),
                "{} preferred form is not one of its variants",
                mapping.openrouter
            );
            for variant in mapping.variants.iter().filter(|v| v.id.ends_with("[1m]")) {
                assert!(!variant.offered, "{} is offered", variant.id);
            }
        }
    }

    #[test]
    fn chat_modes_carry_the_native_set_with_safe_default() {
        assert_eq!(CLAUDE_CHAT_MODES, &["default", "bypassPermissions"]);
        assert!(CLAUDE_CHAT_MODES.contains(&CLAUDE_DEFAULT_CHAT_MODE));
        assert_eq!(CLAUDE_DEFAULT_CHAT_MODE, "default");
    }
}
