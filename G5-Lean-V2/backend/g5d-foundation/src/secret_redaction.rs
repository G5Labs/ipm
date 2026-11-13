//! Shared policy for runtime credential identification and output redaction.

/// Environment key used for the one-shot agent-carrier capability token.
pub const AGENT_CARRIER_TOKEN_ENV: &str = "G5D_AGENT_CARRIER_TOKEN";

const GUEST_SECRET_MIN_LEN: usize = 8;
const EXACT_CREDENTIAL_ENV_KEYS: &[&str] = &[
    "G5_API_KEY",
    "G5D_API_KEY",
    "G5D_REGISTRATION_TOKEN",
    "G5_HOST_JWT",
    "G5D_TOKEN",
];

fn is_exact_credential_key(key: &str) -> bool {
    EXACT_CREDENTIAL_ENV_KEYS.contains(&key)
}

fn is_sensitive_env_key(key: &str) -> bool {
    let key = key.to_ascii_uppercase();
    ["KEY", "TOKEN", "SECRET", "PASSWORD", "JWT", "CREDENTIAL"]
        .iter()
        .any(|needle| key.contains(needle))
}

/// Collect values that must be scrubbed before guest output reaches logs.
/// The runtime's own image key list (`G5D_IMAGE_ENV_KEYS`) carries variable
/// names, not a credential; its "KEYS" spelling must not turn every mention
/// of those names in guest diagnostics into `[REDACTED]`.
pub fn collect_secret_env_values(env: &[(String, String)]) -> Vec<String> {
    env.iter()
        .filter(|(key, value)| {
            !value.is_empty()
                && key != crate::runtime_environment::IMAGE_ENV_KEYS_ENV
                && (is_exact_credential_key(key)
                    || (is_sensitive_env_key(key) && value.len() >= GUEST_SECRET_MIN_LEN))
        })
        .map(|(_, value)| value.clone())
        .collect()
}

/// Replace every occurrence of each non-empty secret with `[REDACTED]`.
pub fn redact_secrets(input: &str, secrets: &[String]) -> String {
    let mut output = input.to_owned();
    for secret in secrets {
        if !secret.is_empty() {
            output = output.replace(secret, "[REDACTED]");
        }
    }
    output
}

#[cfg(test)]
mod tests {
    use super::{collect_secret_env_values, redact_secrets};

    #[test]
    fn the_image_key_list_is_metadata_not_a_secret() {
        let env = vec![
            (
                crate::runtime_environment::IMAGE_ENV_KEYS_ENV.to_owned(),
                "JAVA_HOME:MAVEN_HOME".to_owned(),
            ),
            (
                "IMAGE_API_KEY".to_owned(),
                "image-secret-token-12345".to_owned(),
            ),
        ];
        let secrets = collect_secret_env_values(&env);
        assert_eq!(secrets, vec!["image-secret-token-12345".to_owned()]);
        assert_eq!(
            redact_secrets("JAVA_HOME:MAVEN_HOME is not set correctly", &secrets),
            "JAVA_HOME:MAVEN_HOME is not set correctly",
            "variable names stay readable in guest diagnostics"
        );
    }

    #[test]
    fn exact_credentials_are_collected_even_when_short() {
        let env = vec![("G5_API_KEY".to_owned(), "tiny".to_owned())];
        assert_eq!(collect_secret_env_values(&env), vec!["tiny"]);
    }

    #[test]
    fn heuristic_credentials_require_minimum_length() {
        let env = vec![
            ("CUSTOM_TOKEN".to_owned(), "short".to_owned()),
            ("CUSTOM_SECRET".to_owned(), "long-enough".to_owned()),
        ];
        assert_eq!(collect_secret_env_values(&env), vec!["long-enough"]);
    }

    #[test]
    fn collected_values_are_redacted_everywhere() {
        assert_eq!(
            redact_secrets("token=secret and secret", &["secret".to_owned()]),
            "token=[REDACTED] and [REDACTED]"
        );
    }
}
