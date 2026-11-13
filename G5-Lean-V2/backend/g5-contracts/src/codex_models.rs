//! Codex model catalog + provider-route resolution shared by API and worker
//! code (core D9) — the Codex mirror of `claude_models.rs`.
//!
//! The catalog carries direct Codex ids and OpenRouter `openai/<id>` slugs.
//! User-facing selectors present friendly logical models; the backend resolver
//! projects the route-specific form execution requires. Route selection mirrors
//! the worker's proxy-plan precedence:
//! a non-empty `OPENROUTER_API_KEY` selects OpenRouter, else the effective
//! `OPENAI_BASE_URL` host decides.

use http::Uri;

pub const CODEX_APP_SERVER_TRANSPORT: &str = "app_server_stdio_v1";

/// Readable in frozen receipts, rejected for new launches.
pub const CODEX_RETIRED_CHAT_MODES: &[&str] = &["on-failure/workspace-write"];

pub const CODEX_CLI_VERSION: &str = include_str!("../../../scripts/codex-cli-version");

use crate::claude_models::has_non_empty_value;

/// Catalog default model. Flipped to `gpt-5.6-sol` together with the 0.145.0
/// CLI bump (`scripts/codex-cli-version`, plan Story 4) — never offer gpt-5.6
/// without the runtime that supports it (core D9); any revert must move both.
pub const CODEX_DIRECT_CHAT_DEFAULT_MODEL: &str = "gpt-5.6-sol";
pub const CODEX_OPENROUTER_CHAT_DEFAULT_MODEL: &str = "openai/gpt-5.6-sol";
pub const OPENAI_BASE_URL_ENV: &str = "OPENAI_BASE_URL";
pub const OPENROUTER_API_KEY_ENV: &str = "OPENROUTER_API_KEY";
pub const OPENAI_API_KEY_ENV: &str = "OPENAI_API_KEY";

/// Reasoning-effort ladder for Codex models, weakest to strongest (core D9).
/// Per-model `efforts` sets are ordered sub-slices of this ladder. `minimal`
/// is intentionally absent (#1342 target control set): the 4-choice picker
/// offers catalog efforts only, while the worker/unattended fallback stays
/// `xhigh` (types/core D19, `DEFAULT_CODEX_EFFORT`).
pub const CODEX_EFFORT_ORDER: &[&str] = &["low", "medium", "high", "xhigh", "max", "ultra"];

/// Native compound `approval_policy/sandbox_mode` chat presets, sent verbatim
/// (core D9; no cross-harness mode translation).
pub const CODEX_NATIVE_MODES: &[&str] = &[
    "on-request/read-only",
    "never/read-only",
    "never/workspace-write",
    "never/danger-full-access",
];
/// G5's noninteractive execution subset; no native approval requests are enabled.
pub const CODEX_CHAT_MODES: &[&str] = &[
    "never/read-only",
    "never/workspace-write",
    "never/danger-full-access",
];

/// Catalog default chat mode (core D9, plan D-12); consumed when the 4-choice
/// picker lands — the live D19 preset defaults are unchanged until then.
pub const CODEX_DEFAULT_CHAT_MODE: &str = "never/workspace-write";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CodexModelProvider {
    OpenAiDirect,
    OpenRouter,
}

#[derive(Debug)]
pub struct CodexModelMapping {
    /// Direct OpenAI-route model id (the CLI `model/list` id).
    pub direct: &'static str,
    /// OpenRouter `openai/<id>` slug; `None` = direct-only (never offered on
    /// the OpenRouter route).
    pub openrouter: Option<&'static str>,
    pub label: &'static str,
    /// Whether the picker offers this model (`codex-auto-review` is hidden by
    /// the CLI and excluded from the catalog entirely).
    pub offered: bool,
    /// Ordered sub-slice of `CODEX_EFFORT_ORDER` this model supports.
    pub efforts: &'static [&'static str],
    /// The model's default reasoning effort — `xhigh` for every model
    /// (core D9, chat intent 534d50ad; supersedes the plan D-6
    /// highest-supported rule).
    pub default_effort: &'static str,
}

const EFFORTS_THROUGH_ULTRA: &[&str] = &["low", "medium", "high", "xhigh", "max", "ultra"];
const EFFORTS_THROUGH_MAX: &[&str] = &["low", "medium", "high", "xhigh", "max"];
const EFFORTS_THROUGH_XHIGH: &[&str] = &["low", "medium", "high", "xhigh"];

const CODEX_MODEL_MAPPINGS: &[CodexModelMapping] = &[
    // Added with the 0.159.3 CLI bump (provider-accounts
    // `g5.provider.catalog.pinned`): its ChatGPT-sign-in `model/list` reports
    // gpt-6.1-sol (its own default), gpt-6-sol and gpt-6-luna beside the
    // models below, with these effort ladders. OpenRouter serves the same
    // `openai/<id>` slugs. The catalog default stays `gpt-5.6-sol`.
    CodexModelMapping {
        direct: "gpt-6.1-sol",
        openrouter: Some("openai/gpt-6.1-sol"),
        label: "GPT 6.1 Sol",
        offered: true,
        efforts: EFFORTS_THROUGH_ULTRA,
        default_effort: "xhigh",
    },
    // Added with the 0.153.3 CLI bump (chat intent 8c04277e, core D9): the
    // 0.153.3 `model/list` probe reports gpt-6-astra first with the full
    // ladder through `ultra`. The `openai/gpt-6-astra` OpenRouter slug
    // landed 2026-09-04 (chat intents 8c04277e/a429913a); OpenRouter's
    // `openai/gpt-6-astra-pro` serving variant is deliberately excluded
    // (no direct id — core D9). Upstream stamps astra picker-`hidden`
    // during the staged GPT-6 rollout; offering it here is the registered
    // user decision.
    CodexModelMapping {
        direct: "gpt-6-astra",
        openrouter: Some("openai/gpt-6-astra"),
        label: "GPT 6 Astra",
        offered: true,
        efforts: EFFORTS_THROUGH_ULTRA,
        default_effort: "xhigh",
    },
    CodexModelMapping {
        direct: "gpt-6-sol",
        openrouter: Some("openai/gpt-6-sol"),
        label: "GPT 6 Sol",
        offered: true,
        efforts: EFFORTS_THROUGH_ULTRA,
        default_effort: "xhigh",
    },
    CodexModelMapping {
        direct: "gpt-6-luna",
        openrouter: Some("openai/gpt-6-luna"),
        label: "GPT 6 Luna",
        offered: true,
        efforts: EFFORTS_THROUGH_MAX,
        default_effort: "xhigh",
    },
    CodexModelMapping {
        direct: "gpt-5.6-sol",
        openrouter: Some("openai/gpt-5.6-sol"),
        label: "GPT 5.6 Sol",
        offered: true,
        efforts: EFFORTS_THROUGH_ULTRA,
        default_effort: "xhigh",
    },
    CodexModelMapping {
        direct: "gpt-5.6-terra",
        openrouter: Some("openai/gpt-5.6-terra"),
        label: "GPT 5.6 Terra",
        offered: true,
        efforts: EFFORTS_THROUGH_ULTRA,
        default_effort: "xhigh",
    },
    CodexModelMapping {
        direct: "gpt-5.6-luna",
        openrouter: Some("openai/gpt-5.6-luna"),
        label: "GPT 5.6 Luna",
        offered: true,
        efforts: EFFORTS_THROUGH_MAX,
        default_effort: "xhigh",
    },
    CodexModelMapping {
        direct: "gpt-5.5",
        openrouter: Some("openai/gpt-5.5"),
        label: "GPT 5.5",
        offered: true,
        efforts: EFFORTS_THROUGH_XHIGH,
        default_effort: "xhigh",
    },
    // Kept for label and effort resolution of existing selections, but no
    // longer offered: the 0.159.3 ChatGPT-sign-in `model/list` omits them
    // (`g5.provider.catalog.pinned`).
    CodexModelMapping {
        direct: "gpt-5.4",
        openrouter: Some("openai/gpt-5.4"),
        label: "GPT 5.4",
        offered: false,
        efforts: EFFORTS_THROUGH_XHIGH,
        default_effort: "xhigh",
    },
    CodexModelMapping {
        direct: "gpt-5.4-mini",
        openrouter: Some("openai/gpt-5.4-mini"),
        label: "GPT 5.4 Mini",
        offered: false,
        efforts: EFFORTS_THROUGH_XHIGH,
        default_effort: "xhigh",
    },
    CodexModelMapping {
        direct: "gpt-5.3-codex-spark",
        openrouter: None,
        label: "GPT 5.3 Codex Spark",
        offered: false,
        efforts: EFFORTS_THROUGH_XHIGH,
        default_effort: "xhigh",
    },
];

/// The Codex catalog (core D9): route forms, labels, offered flags, effort
/// sets, and defaults. Feeds the generated client catalog data (core D8).
/// Per-provider fallback effort for models the catalog does not know (#1370,
/// core D9): kept per agent by user decision; replaces the former flat
/// worker fallback `DEFAULT_CODEX_EFFORT`.
pub const CODEX_UNKNOWN_MODEL_EFFORT: &str = "xhigh";

/// Resolve the catalog `default_effort` for a model id in either route form
/// (#1370, core D9); unknown models fall back to
/// `CODEX_UNKNOWN_MODEL_EFFORT`. This is the runtime resolution for
/// blank/unpinned efforts — the same rule the generated client catalog serves.
#[must_use]
pub fn codex_default_effort_for_model(model: &str) -> &'static str {
    let trimmed = model.trim();
    if trimmed.is_empty() {
        return CODEX_UNKNOWN_MODEL_EFFORT;
    }
    for mapping in CODEX_MODEL_MAPPINGS {
        if trimmed == mapping.direct || mapping.openrouter == Some(trimmed) {
            return mapping.default_effort;
        }
    }
    CODEX_UNKNOWN_MODEL_EFFORT
}

#[must_use]
pub fn codex_model_catalog() -> &'static [CodexModelMapping] {
    CODEX_MODEL_MAPPINGS
}

fn is_openrouter_host(base_url: &str) -> bool {
    let Ok(uri) = base_url.parse::<Uri>() else {
        return false;
    };
    let Some(host) = uri.host().map(|value| value.to_ascii_lowercase()) else {
        return false;
    };
    host == "openrouter.ai" || host.ends_with(".openrouter.ai")
}

/// Effective Codex route (core D9): a non-empty `OPENROUTER_API_KEY` selects
/// OpenRouter (the worker's proxy-plan precedence key), else the
/// `OPENAI_BASE_URL` host decides; unset/unparseable/other resolves direct.
#[must_use]
pub fn codex_model_provider(
    base_url: Option<&str>,
    openrouter_api_key: Option<&str>,
) -> CodexModelProvider {
    if has_non_empty_value(openrouter_api_key) {
        return CodexModelProvider::OpenRouter;
    }
    let Some(base_url) = base_url.map(str::trim).filter(|value| !value.is_empty()) else {
        return CodexModelProvider::OpenAiDirect;
    };
    if is_openrouter_host(base_url) {
        CodexModelProvider::OpenRouter
    } else {
        CodexModelProvider::OpenAiDirect
    }
}

#[must_use]
pub fn default_codex_chat_model_for_provider(provider: CodexModelProvider) -> &'static str {
    match provider {
        CodexModelProvider::OpenAiDirect => CODEX_DIRECT_CHAT_DEFAULT_MODEL,
        CodexModelProvider::OpenRouter => CODEX_OPENROUTER_CHAT_DEFAULT_MODEL,
    }
}

/// Cross-route translation only (core D9): a recognized id already in the
/// active route's form passes through verbatim; `gpt-5.3-codex-spark` has no
/// OpenRouter slug and passes through unchanged on that route; unknown ids
/// are trimmed, never rewritten.
#[must_use]
pub fn normalize_codex_model_for_provider(model: &str, provider: CodexModelProvider) -> String {
    let trimmed = model.trim();
    if trimmed.is_empty() {
        return String::new();
    }

    for mapping in CODEX_MODEL_MAPPINGS {
        let matches_direct = trimmed == mapping.direct;
        let matches_slug = mapping.openrouter == Some(trimmed);
        if matches_direct || matches_slug {
            return match provider {
                CodexModelProvider::OpenAiDirect => mapping.direct.to_string(),
                CodexModelProvider::OpenRouter => mapping
                    .openrouter
                    .map_or_else(|| trimmed.to_string(), str::to_string),
            };
        }
    }

    trimmed.to_string()
}

#[must_use]
pub fn normalize_optional_codex_model_for_provider(
    model: Option<&str>,
    provider: CodexModelProvider,
) -> Option<String> {
    let model = model.map(str::trim).filter(|value| !value.is_empty())?;
    Some(normalize_codex_model_for_provider(model, provider))
}

#[must_use]
pub fn resolve_codex_chat_model_for_provider(
    model: Option<&str>,
    provider: CodexModelProvider,
) -> String {
    normalize_optional_codex_model_for_provider(model, provider)
        .unwrap_or_else(|| default_codex_chat_model_for_provider(provider).to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_defaults_to_direct_without_openrouter_signals() {
        assert_eq!(
            codex_model_provider(None, None),
            CodexModelProvider::OpenAiDirect
        );
        assert_eq!(
            codex_model_provider(Some("https://api.openai.com/v1"), None),
            CodexModelProvider::OpenAiDirect
        );
        assert_eq!(
            codex_model_provider(Some("not a url"), Some("  ")),
            CodexModelProvider::OpenAiDirect
        );
    }

    #[test]
    fn provider_detects_openrouter_hosts() {
        assert_eq!(
            codex_model_provider(Some("https://openrouter.ai/api/v1"), None),
            CodexModelProvider::OpenRouter
        );
        assert_eq!(
            codex_model_provider(Some("https://gateway.openrouter.ai/api/v1"), None),
            CodexModelProvider::OpenRouter
        );
    }

    #[test]
    fn provider_selects_openrouter_when_key_present() {
        assert_eq!(
            codex_model_provider(None, Some("sk-or-key")),
            CodexModelProvider::OpenRouter
        );
        assert_eq!(
            codex_model_provider(Some("https://api.openai.com/v1"), Some("sk-or-key")),
            CodexModelProvider::OpenRouter
        );
    }

    #[test]
    fn direct_provider_maps_openrouter_slugs_to_direct_ids() {
        assert_eq!(
            normalize_codex_model_for_provider(
                "openai/gpt-5.6-sol",
                CodexModelProvider::OpenAiDirect
            ),
            "gpt-5.6-sol"
        );
        assert_eq!(
            normalize_codex_model_for_provider("openai/gpt-5.5", CodexModelProvider::OpenAiDirect),
            "gpt-5.5"
        );
    }

    #[test]
    fn direct_provider_preserves_direct_ids_verbatim() {
        for mapping in codex_model_catalog() {
            assert_eq!(
                normalize_codex_model_for_provider(
                    mapping.direct,
                    CodexModelProvider::OpenAiDirect
                ),
                mapping.direct
            );
        }
    }

    #[test]
    fn openrouter_provider_maps_direct_ids_to_slugs_and_keeps_slugs() {
        assert_eq!(
            normalize_codex_model_for_provider("gpt-5.5", CodexModelProvider::OpenRouter),
            "openai/gpt-5.5"
        );
        assert_eq!(
            normalize_codex_model_for_provider("gpt-5.6-sol", CodexModelProvider::OpenRouter),
            "openai/gpt-5.6-sol"
        );
        assert_eq!(
            normalize_codex_model_for_provider(
                "openai/gpt-5.6-terra",
                CodexModelProvider::OpenRouter
            ),
            "openai/gpt-5.6-terra"
        );
    }

    #[test]
    fn spark_is_direct_only_and_astra_carries_its_openrouter_slug() {
        // core D9 (chat intents 8c04277e/a429913a, 2026-09-04): spark stays
        // the only direct-only model; astra's OpenRouter slug landed and
        // round-trips on both routes.
        let spark = codex_model_catalog()
            .iter()
            .find(|mapping| mapping.direct == "gpt-5.3-codex-spark")
            .expect("spark in catalog");
        assert_eq!(spark.openrouter, None);
        assert_eq!(
            normalize_codex_model_for_provider(
                "gpt-5.3-codex-spark",
                CodexModelProvider::OpenRouter
            ),
            "gpt-5.3-codex-spark",
            "direct-only model must pass through unchanged on the OpenRouter route"
        );
        let astra = codex_model_catalog()
            .iter()
            .find(|mapping| mapping.direct == "gpt-6-astra")
            .expect("astra in catalog");
        assert_eq!(astra.openrouter, Some("openai/gpt-6-astra"));
        assert_eq!(
            normalize_codex_model_for_provider("gpt-6-astra", CodexModelProvider::OpenRouter),
            "openai/gpt-6-astra"
        );
        assert_eq!(
            normalize_codex_model_for_provider(
                "openai/gpt-6-astra",
                CodexModelProvider::OpenAiDirect
            ),
            "gpt-6-astra"
        );
        let openrouter_offered: Vec<&str> = codex_model_catalog()
            .iter()
            .filter(|mapping| mapping.offered)
            .filter_map(|mapping| mapping.openrouter)
            .collect();
        assert!(!openrouter_offered.iter().any(|slug| slug.contains("spark")));
        assert!(openrouter_offered.contains(&"openai/gpt-6-astra"));
        // The astra-pro serving variant is deliberately not catalog data
        // (user decision a429913a): no direct id exists for it.
        assert!(!openrouter_offered.iter().any(|slug| slug.contains("pro")));
        assert_eq!(openrouter_offered.len(), 8);
    }

    #[test]
    fn offered_models_match_the_pinned_bundled_model_list() {
        // The list below was re-checked against this pin; a pin change must
        // re-check it (`g5.provider.catalog.pinned`).
        assert_eq!(
            include_str!("../../../scripts/codex-cli-version").trim(),
            "0.164.0",
            "re-check the offered Codex models against the new pin, then update this test"
        );
        // `g5.provider.catalog.pinned`: the offered direct ids are exactly
        // the pinned 0.164.0 CLI's bundled/custom-provider model/list, in
        // its order. Authenticated subscription lists can filter an account's
        // availability (the native #809 probe omitted gpt-5.5). Keep API-route
        // ids and saved selections; re-check both lists whenever the pin moves.
        let offered: Vec<&str> = codex_model_catalog()
            .iter()
            .filter(|mapping| mapping.offered)
            .map(|mapping| mapping.direct)
            .collect();
        assert_eq!(
            offered,
            [
                "gpt-6.1-sol",
                "gpt-6-astra",
                "gpt-6-sol",
                "gpt-6-luna",
                "gpt-5.6-sol",
                "gpt-5.6-terra",
                "gpt-5.6-luna",
                "gpt-5.5",
            ]
        );
        // Withdrawn models stay resolvable for existing selections.
        for withdrawn in ["gpt-5.4", "gpt-5.4-mini", "gpt-5.3-codex-spark"] {
            assert!(codex_model_catalog()
                .iter()
                .any(|mapping| mapping.direct == withdrawn && !mapping.offered));
        }
        assert!(offered.contains(&CODEX_DIRECT_CHAT_DEFAULT_MODEL));
    }

    #[test]
    fn unknown_models_are_trimmed_but_not_rewritten() {
        assert_eq!(
            normalize_codex_model_for_provider(
                "  custom/model  ",
                CodexModelProvider::OpenAiDirect
            ),
            "custom/model"
        );
        assert_eq!(
            normalize_codex_model_for_provider("gpt-9.9", CodexModelProvider::OpenRouter),
            "gpt-9.9"
        );
    }

    #[test]
    fn chat_default_is_provider_specific_and_flipped_with_the_0_145_0_bump() {
        assert_eq!(
            resolve_codex_chat_model_for_provider(None, CodexModelProvider::OpenAiDirect),
            CODEX_DIRECT_CHAT_DEFAULT_MODEL
        );
        assert_eq!(
            resolve_codex_chat_model_for_provider(None, CodexModelProvider::OpenRouter),
            CODEX_OPENROUTER_CHAT_DEFAULT_MODEL
        );
        // Guardrail (core D9): the gpt-5.6 default is only valid on the
        // 0.145.0+ runtime pinned in scripts/codex-cli-version.
        assert_eq!(CODEX_DIRECT_CHAT_DEFAULT_MODEL, "gpt-5.6-sol");
    }

    fn effort_rank(effort: &str) -> usize {
        CODEX_EFFORT_ORDER
            .iter()
            .position(|candidate| *candidate == effort)
            .unwrap_or_else(|| panic!("effort {effort} is not on CODEX_EFFORT_ORDER"))
    }

    #[test]
    fn default_effort_resolves_per_model_in_both_route_forms() {
        // #1370 alignment (core D9): direct ids and OpenRouter slugs resolve
        // to the model's default_effort; unknown/blank models use the
        // per-provider fallback that replaced the flat worker default.
        assert_eq!(codex_default_effort_for_model("gpt-6-astra"), "xhigh");
        assert_eq!(codex_default_effort_for_model("gpt-5.6-sol"), "xhigh");
        assert_eq!(
            codex_default_effort_for_model("openai/gpt-5.6-terra"),
            "xhigh"
        );
        assert_eq!(codex_default_effort_for_model("gpt-5.6-luna"), "xhigh");
        assert_eq!(codex_default_effort_for_model("openai/gpt-5.5"), "xhigh");
        assert_eq!(
            codex_default_effort_for_model("custom/unknown"),
            CODEX_UNKNOWN_MODEL_EFFORT
        );
        assert_eq!(
            codex_default_effort_for_model(""),
            CODEX_UNKNOWN_MODEL_EFFORT
        );
        assert_eq!(CODEX_UNKNOWN_MODEL_EFFORT, "xhigh");
    }

    #[test]
    fn effort_sets_are_ladder_ordered_with_xhigh_default() {
        for mapping in codex_model_catalog() {
            let ranks: Vec<usize> = mapping
                .efforts
                .iter()
                .map(|effort| effort_rank(effort))
                .collect();
            assert!(
                ranks.windows(2).all(|pair| pair[0] < pair[1]),
                "{} efforts are not in ladder order",
                mapping.direct
            );
            assert!(mapping.efforts.contains(&mapping.default_effort));
            // Core D9 (chat intent 534d50ad): every Codex model defaults to
            // `xhigh` — a cost/latency default, not the highest supported.
            assert_eq!(
                mapping.default_effort, "xhigh",
                "{} default effort must be xhigh",
                mapping.direct
            );
        }
    }

    #[test]
    fn per_model_effort_sets_match_the_cli_probes() {
        // gpt-6-astra: 0.153.3 probe; the rest: 0.145.0 probe (core D9).
        let by_id = |id: &str| {
            codex_model_catalog()
                .iter()
                .find(|mapping| mapping.direct == id)
                .unwrap_or_else(|| panic!("{id} in catalog"))
        };
        for id in [
            "gpt-6.1-sol",
            "gpt-6-astra",
            "gpt-6-sol",
            "gpt-5.6-sol",
            "gpt-5.6-terra",
        ] {
            let mapping = by_id(id);
            assert!(mapping.efforts.contains(&"max") && mapping.efforts.contains(&"ultra"));
            assert_eq!(mapping.default_effort, "xhigh");
        }
        for id in ["gpt-6-luna", "gpt-5.6-luna"] {
            let luna = by_id(id);
            assert!(luna.efforts.contains(&"max") && !luna.efforts.contains(&"ultra"));
            assert_eq!(luna.default_effort, "xhigh");
        }
        for id in ["gpt-5.5", "gpt-5.4", "gpt-5.4-mini", "gpt-5.3-codex-spark"] {
            let mapping = by_id(id);
            assert!(!mapping.efforts.contains(&"max") && !mapping.efforts.contains(&"ultra"));
            assert_eq!(mapping.default_effort, "xhigh");
        }
    }

    #[test]
    fn catalog_carries_the_probed_models_then_the_withdrawn_ones() {
        let direct_ids: Vec<&str> = codex_model_catalog()
            .iter()
            .map(|mapping| mapping.direct)
            .collect();
        assert_eq!(
            direct_ids,
            vec![
                "gpt-6.1-sol",
                "gpt-6-astra",
                "gpt-6-sol",
                "gpt-6-luna",
                "gpt-5.6-sol",
                "gpt-5.6-terra",
                "gpt-5.6-luna",
                "gpt-5.5",
                "gpt-5.4",
                "gpt-5.4-mini",
                "gpt-5.3-codex-spark",
            ]
        );
        assert!(codex_model_catalog()
            .iter()
            .all(|mapping| !mapping.label.trim().is_empty()));
    }

    #[test]
    fn chat_modes_carry_the_native_presets_with_workspace_default() {
        assert_eq!(
            CODEX_CHAT_MODES,
            &[
                "never/read-only",
                "never/workspace-write",
                "never/danger-full-access",
            ]
        );
        assert!(CODEX_CHAT_MODES.contains(&CODEX_DEFAULT_CHAT_MODE));
        assert_eq!(CODEX_DEFAULT_CHAT_MODE, "never/workspace-write");
    }
}
