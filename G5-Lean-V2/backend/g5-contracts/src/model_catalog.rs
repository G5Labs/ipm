//! Shared projections of the provider catalogs. Offered entries drive new
//! selectors and native gates; effort lookup also recognizes historical aliases.

use crate::claude_models::claude_model_catalog;
use crate::codex_models::codex_model_catalog;
use crate::types::ChatAgentProvider;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModelEfforts {
    pub efforts: &'static [&'static str],
    pub default_effort: &'static str,
}

#[derive(Debug, Clone, Copy)]
pub struct OfferedModelEntry {
    pub provider: ChatAgentProvider,
    pub id: &'static str,
    pub label: &'static str,
    pub route: &'static str,
    pub controls: ModelEfforts,
}

pub fn offered_entries() -> impl Iterator<Item = OfferedModelEntry> {
    let claude = claude_model_catalog().iter().flat_map(|family| {
        let controls = ModelEfforts {
            efforts: family.efforts,
            default_effort: family.default_effort,
        };
        family
            .variants
            .iter()
            .filter(|variant| variant.offered)
            .map(move |variant| OfferedModelEntry {
                provider: ChatAgentProvider::ClaudeCode,
                id: variant.id,
                label: variant.label,
                route: "direct",
                controls,
            })
            .chain(family.openrouter_offered.then_some(OfferedModelEntry {
                provider: ChatAgentProvider::ClaudeCode,
                id: family.openrouter,
                label: family.openrouter_label.unwrap_or(family.openrouter),
                route: "openrouter",
                controls,
            }))
    });
    let codex = codex_model_catalog()
        .iter()
        .filter(|model| model.offered)
        .flat_map(|model| {
            let direct = OfferedModelEntry {
                provider: ChatAgentProvider::Codex,
                id: model.direct,
                label: model.label,
                route: "direct",
                controls: ModelEfforts {
                    efforts: model.efforts,
                    default_effort: model.default_effort,
                },
            };
            std::iter::once(direct).chain(model.openrouter.map(|id| OfferedModelEntry {
                id,
                route: "openrouter",
                ..direct
            }))
        });
    claude.chain(codex)
}

/// Known, unoffered models and aliases retain their controls. Unknown custom
/// pins return None so each caller can preserve its existing fallback policy.
pub fn model_efforts(provider: ChatAgentProvider, model: &str) -> Option<ModelEfforts> {
    match provider {
        ChatAgentProvider::ClaudeCode => claude_model_catalog()
            .iter()
            .find(|entry| {
                entry.openrouter == model
                    || entry.direct_aliases.contains(&model)
                    || entry.variants.iter().any(|variant| variant.id == model)
            })
            .map(|entry| ModelEfforts {
                efforts: entry.efforts,
                default_effort: entry.default_effort,
            }),
        ChatAgentProvider::Codex => codex_model_catalog()
            .iter()
            .find(|entry| entry.direct == model || entry.openrouter == Some(model))
            .map(|entry| ModelEfforts {
                efforts: entry.efforts,
                default_effort: entry.default_effort,
            }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn offered_routes_preserve_labels_and_effort_controls_for_both_providers() {
        let entries: Vec<_> = offered_entries().collect();
        for (provider, id, route, label, efforts, default_effort) in [
            (
                ChatAgentProvider::ClaudeCode,
                "claude-haiku-4-5",
                "direct",
                "Haiku 4.5",
                &["none"][..],
                "none",
            ),
            (
                ChatAgentProvider::ClaudeCode,
                "anthropic/claude-haiku-4.5",
                "openrouter",
                "anthropic/claude-haiku-4.5",
                &["none"][..],
                "none",
            ),
            (
                ChatAgentProvider::Codex,
                "gpt-6.1-sol",
                "direct",
                "GPT 6.1 Sol",
                &["low", "medium", "high", "xhigh", "max", "ultra"][..],
                "xhigh",
            ),
            (
                ChatAgentProvider::Codex,
                "openai/gpt-6.1-sol",
                "openrouter",
                "GPT 6.1 Sol",
                &["low", "medium", "high", "xhigh", "max", "ultra"][..],
                "xhigh",
            ),
        ] {
            let matches: Vec<_> = entries
                .iter()
                .filter(|entry| entry.provider == provider && entry.id == id)
                .collect();
            assert_eq!(matches.len(), 1, "{provider:?}/{id}");
            let entry = matches[0];
            assert_eq!((entry.route, entry.label), (route, label));
            assert_eq!(
                entry.controls,
                ModelEfforts {
                    efforts,
                    default_effort
                }
            );
            assert_eq!(model_efforts(provider, id), Some(entry.controls));
        }
    }

    #[test]
    fn historical_controls_remain_known_without_reoffering_retired_models() {
        for (provider, direct, router, other) in [
            (
                ChatAgentProvider::ClaudeCode,
                "claude-opus-4-7",
                "anthropic/claude-opus-4.7",
                ChatAgentProvider::Codex,
            ),
            (
                ChatAgentProvider::Codex,
                "gpt-5.4",
                "openai/gpt-5.4",
                ChatAgentProvider::ClaudeCode,
            ),
        ] {
            let controls = model_efforts(provider, direct).expect("historical controls");
            assert_eq!(model_efforts(provider, router), Some(controls));
            assert!(!offered_entries().any(|entry| entry.id == direct || entry.id == router));
            assert_eq!(model_efforts(other, direct), None);
            assert_eq!(model_efforts(provider, "custom/model-pin"), None);
        }
    }
}
