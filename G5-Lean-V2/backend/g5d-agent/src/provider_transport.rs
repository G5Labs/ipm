//! Bind the native client's authentication and endpoint to the admitted turn.
//! Credentials stay in the private environment; only routing goes into argv.

use std::collections::BTreeMap;

use g5_contracts::{error::G5Error, types::AgentAuthModeV1};

use crate::types::ProviderId;

const CODEX_PROVIDER: &str = "g5_selected";
const OPENAI_BASE: &str = "https://api.openai.com/v1";
const OPENROUTER_BASE: &str = "https://openrouter.ai/api/v1";

/// Returns native CLI configuration arguments. Refuse missing credentials rather
/// than allowing a saved login or another inherited key to replace this account.
pub(crate) fn configure(
    provider: ProviderId,
    auth: Option<AgentAuthModeV1>,
    base_url: Option<&str>,
    env: &mut BTreeMap<String, String>,
) -> Result<Vec<String>, G5Error> {
    use AgentAuthModeV1::*;
    let auth = auth.ok_or_else(|| invalid("missing provider authentication mode"))?;
    let base_url = base_url.map(str::trim).filter(|base| !base.is_empty());
    match (provider, auth) {
        (
            ProviderId::ClaudeCode,
            AnthropicApiKey | AnthropicBearer | AnthropicOauth | OpenRouterApiKey,
        ) => {
            let key = match auth {
                AnthropicApiKey => "ANTHROPIC_API_KEY",
                AnthropicOauth => "CLAUDE_CODE_OAUTH_TOKEN",
                _ => "ANTHROPIC_AUTH_TOKEN",
            };
            let credential = required_credential(env, key)?;
            for name in [
                "ANTHROPIC_API_KEY",
                "ANTHROPIC_AUTH_TOKEN",
                "CLAUDE_CODE_OAUTH_TOKEN",
                "ANTHROPIC_API_KEY_FILE",
                "ANTHROPIC_UNIX_SOCKET",
            ] {
                env.remove(name);
            }
            env.insert(key.into(), credential);
            let base = base_url.map(str::to_string).unwrap_or_else(|| {
                if auth == OpenRouterApiKey {
                    "https://openrouter.ai/api".into()
                } else {
                    "https://api.anthropic.com".into()
                }
            });
            env.insert("ANTHROPIC_BASE_URL".into(), base);
            Ok(Vec::new())
        }
        (ProviderId::Codex, ChatGptSubscription) => {
            clear_codex_credentials(env);
            env.remove("OPENAI_BASE_URL");
            // An owner may previously have selected an API-key provider in
            // config.toml. Subscription turns always use the built-in provider.
            Ok(vec!["-c".into(), "model_provider=\"openai\"".into()])
        }
        (ProviderId::Codex, OpenAiApiKey | OpenRouterApiKey) => {
            let (key, default_base) = if auth == OpenRouterApiKey {
                ("OPENROUTER_API_KEY", OPENROUTER_BASE)
            } else {
                ("OPENAI_API_KEY", OPENAI_BASE)
            };
            let credential = required_credential(env, key)?;
            let base = base_url
                .map(str::to_string)
                .unwrap_or_else(|| default_base.into());
            // The account catalog stores the first-party origin without /v1.
            // Compatible endpoints already own their exact API path prefix.
            let base = if base.trim_end_matches('/') == "https://api.openai.com" {
                OPENAI_BASE.to_string()
            } else {
                base.trim_end_matches('/').to_string()
            };
            clear_codex_credentials(env);
            env.insert("CODEX_API_KEY".into(), credential);
            env.insert("OPENAI_BASE_URL".into(), base.clone());
            // 0.159.3 ignores OPENAI_BASE_URL for its built-in provider and
            // forbids redefining that provider. An explicit custom provider
            // also prevents saved ChatGPT auth from winning on an API-key turn.
            let definition = format!(
                "model_providers.{CODEX_PROVIDER}={{ name=\"G5 selected account\", base_url={}, env_key=\"CODEX_API_KEY\", wire_api=\"responses\", requires_openai_auth=false, supports_websockets=false }}",
                toml_edit::Value::from(base),
            );
            Ok(vec![
                "-c".into(),
                format!("model_provider=\"{CODEX_PROVIDER}\""),
                "-c".into(),
                definition,
            ])
        }
        _ => Err(invalid("provider and authentication mode do not match")),
    }
}

fn value(env: &BTreeMap<String, String>, key: &str) -> Option<String> {
    env.get(key)
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn required_credential(env: &BTreeMap<String, String>, key: &str) -> Result<String, G5Error> {
    value(env, key).ok_or_else(|| invalid("the selected provider credential is missing"))
}

fn clear_codex_credentials(env: &mut BTreeMap<String, String>) {
    for key in [
        "CODEX_API_KEY",
        "OPENAI_API_KEY",
        "OPENROUTER_API_KEY",
        "CODEX_ACCESS_TOKEN",
        "CODEX_AUTH_JSON",
    ] {
        env.remove(key);
    }
}

fn invalid(message: &str) -> G5Error {
    G5Error::ValidationError {
        details: serde_json::json!({"field":"provider.auth_mode", "message":message}),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use AgentAuthModeV1::*;

    fn conflicting_credentials() -> BTreeMap<String, String> {
        [
            ("ANTHROPIC_API_KEY", "anthropic-key"),
            ("ANTHROPIC_AUTH_TOKEN", "anthropic-bearer"),
            ("CLAUDE_CODE_OAUTH_TOKEN", "claude-oauth"),
            ("OPENAI_API_KEY", "openai-key"),
            ("OPENROUTER_API_KEY", "router-key"),
            ("CODEX_API_KEY", "stale-key"),
            ("CODEX_AUTH_JSON", "stale-auth"),
            ("CODEX_ACCESS_TOKEN", "stale-access"),
            ("ANTHROPIC_API_KEY_FILE", "/stale/key"),
            ("ANTHROPIC_UNIX_SOCKET", "/stale/proxy.sock"),
        ]
        .into_iter()
        .map(|(k, v)| (k.into(), v.into()))
        .collect()
    }

    #[test]
    fn claude_uses_only_the_frozen_auth_mode_and_preserves_the_endpoint() {
        for (auth, key, secret) in [
            (AnthropicApiKey, "ANTHROPIC_API_KEY", "anthropic-key"),
            (AnthropicBearer, "ANTHROPIC_AUTH_TOKEN", "anthropic-bearer"),
            (AnthropicOauth, "CLAUDE_CODE_OAUTH_TOKEN", "claude-oauth"),
            (OpenRouterApiKey, "ANTHROPIC_AUTH_TOKEN", "anthropic-bearer"),
        ] {
            let mut env = conflicting_credentials();
            env.insert(
                "ANTHROPIC_BASE_URL".into(),
                "https://provider.example/api".into(),
            );
            assert!(configure(
                ProviderId::ClaudeCode,
                Some(auth),
                Some("https://provider.example/api"),
                &mut env
            )
            .unwrap()
            .is_empty());
            assert_eq!(env[key], secret);
            assert_eq!(env["ANTHROPIC_BASE_URL"], "https://provider.example/api");
            for other in [
                "ANTHROPIC_API_KEY",
                "ANTHROPIC_AUTH_TOKEN",
                "CLAUDE_CODE_OAUTH_TOKEN",
                "ANTHROPIC_API_KEY_FILE",
                "ANTHROPIC_UNIX_SOCKET",
            ] {
                if other != key {
                    assert!(!env.contains_key(other), "{auth:?}: {other}");
                }
            }
        }
    }

    #[test]
    fn missing_selected_credential_never_falls_back_to_another_key_or_login() {
        for (provider, auth, key) in [
            (ProviderId::ClaudeCode, AnthropicApiKey, "ANTHROPIC_API_KEY"),
            (
                ProviderId::ClaudeCode,
                AnthropicBearer,
                "ANTHROPIC_AUTH_TOKEN",
            ),
            (
                ProviderId::ClaudeCode,
                AnthropicOauth,
                "CLAUDE_CODE_OAUTH_TOKEN",
            ),
            (ProviderId::Codex, OpenAiApiKey, "OPENAI_API_KEY"),
            (ProviderId::Codex, OpenRouterApiKey, "OPENROUTER_API_KEY"),
        ] {
            let mut env = conflicting_credentials();
            env.remove(key);
            assert!(configure(provider, Some(auth), None, &mut env).is_err());
            env.insert(key.into(), "  ".into());
            assert!(configure(provider, Some(auth), None, &mut env).is_err());
        }
        assert!(configure(
            ProviderId::Codex,
            None,
            None,
            &mut conflicting_credentials()
        )
        .is_err());
        assert!(configure(
            ProviderId::Codex,
            Some(AnthropicApiKey),
            None,
            &mut conflicting_credentials()
        )
        .is_err());
    }

    #[test]
    fn codex_api_routes_override_cached_login_without_serializing_the_key() {
        for (auth, secret, base) in [
            (OpenAiApiKey, "openai-key", OPENAI_BASE),
            (OpenRouterApiKey, "router-key", OPENROUTER_BASE),
        ] {
            let mut env = conflicting_credentials();
            let args = configure(ProviderId::Codex, Some(auth), None, &mut env).unwrap();
            assert_eq!(env["CODEX_API_KEY"], secret);
            assert_eq!(env["OPENAI_BASE_URL"], base);
            assert!(!args.join(" ").contains(secret));
            for key in [
                "OPENAI_API_KEY",
                "OPENROUTER_API_KEY",
                "CODEX_AUTH_JSON",
                "CODEX_ACCESS_TOKEN",
            ] {
                assert!(!env.contains_key(key));
            }
            let config = args
                .chunks_exact(2)
                .map(|pair| pair[1].as_str())
                .collect::<Vec<_>>()
                .join("\n");
            let config = config.parse::<toml_edit::DocumentMut>().unwrap();
            assert_eq!(config["model_provider"].as_str(), Some(CODEX_PROVIDER));
            let provider = &config["model_providers"][CODEX_PROVIDER];
            assert_eq!(provider["base_url"].as_str(), Some(base));
            assert_eq!(provider["env_key"].as_str(), Some("CODEX_API_KEY"));
            assert_eq!(provider["requires_openai_auth"].as_bool(), Some(false));
        }
    }

    #[test]
    fn codex_preserves_custom_api_prefixes_and_quotes_configuration_values() {
        for (base, expected) in [
            ("https://api.openai.com/", OPENAI_BASE),
            (
                "https://provider.example/custom/v2/",
                "https://provider.example/custom/v2",
            ),
            (
                "https://provider.example/a\"b",
                "https://provider.example/a\"b",
            ),
        ] {
            let mut env = conflicting_credentials();
            env.insert("OPENAI_BASE_URL".into(), base.into());
            let args =
                configure(ProviderId::Codex, Some(OpenAiApiKey), Some(base), &mut env).unwrap();
            let config = args[3].parse::<toml_edit::DocumentMut>().unwrap();
            assert_eq!(
                config["model_providers"][CODEX_PROVIDER]["base_url"].as_str(),
                Some(expected)
            );
        }
    }

    #[test]
    fn subscription_uses_native_login_and_clears_api_route_overrides() {
        let mut env = conflicting_credentials();
        env.insert("OPENAI_BASE_URL".into(), "https://stale.example/v1".into());
        let args = configure(
            ProviderId::Codex,
            Some(ChatGptSubscription),
            Some("https://stale.example/v1"),
            &mut env,
        )
        .unwrap();
        assert_eq!(args, ["-c", "model_provider=\"openai\""]);
        for key in [
            "OPENAI_BASE_URL",
            "OPENAI_API_KEY",
            "OPENROUTER_API_KEY",
            "CODEX_API_KEY",
            "CODEX_ACCESS_TOKEN",
            "CODEX_AUTH_JSON",
        ] {
            assert!(!env.contains_key(key));
        }
    }
}
