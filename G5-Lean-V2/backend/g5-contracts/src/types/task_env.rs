//! The exportable-environment policy that Runtime Config secrets follow at
//! every producer and consumer (`g5.chat.secret-admission`,
//! `g5.terminal.secret-admission`): the configuration writer, agent and
//! command dispatch admission, the owner terminal's `open` frame on both ends.
//!
//! Key-local rules (name, reserved key, value size, NUL) hold for any union of
//! scopes and are final at write time. The aggregate rules (entry count and
//! serialized size) are checked for one scope at write time and for the merged
//! effective set at admission, where they are final.

use std::collections::BTreeMap;
use std::fmt;

use super::task_context::{RESERVED_TASK_ENV_KEYS, RESERVED_TASK_ENV_PREFIXES};

/// Most secrets one effective configuration may carry.
pub const MAX_TASK_ENV_SECRETS: usize = 128;
/// Longest secret name in bytes.
pub const MAX_TASK_ENV_KEY_BYTES: usize = 128;
/// Largest secret value in bytes.
pub const MAX_TASK_ENV_VALUE_BYTES: usize = 64 * 1024;
/// Largest serialized JSON object of one effective secret set. The terminal
/// `open` frame and the module invocation both carry the set as one JSON
/// object; both frame limits are derived from this budget.
pub const MAX_TASK_ENV_SERIALIZED_BYTES: usize = 256 * 1024;

/// Why a secret set is refused. `Display` is the owner-facing sentence used by
/// the configuration editor, the chat notice and the terminal status.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TaskEnvSecretError {
    InvalidName { key: String },
    ReservedName { key: String },
    ValueTooLarge { key: String, bytes: usize },
    ValueContainsNul { key: String },
    TooMany { count: usize },
    TooLarge { bytes: usize },
}

impl TaskEnvSecretError {
    /// A stable machine-readable code.
    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            Self::InvalidName { .. } => "secret_name_invalid",
            Self::ReservedName { .. } => "secret_name_reserved",
            Self::ValueTooLarge { .. } => "secret_value_too_large",
            Self::ValueContainsNul { .. } => "secret_value_nul",
            Self::TooMany { .. } => "secrets_too_many",
            Self::TooLarge { .. } => "secrets_too_large",
        }
    }
}

impl fmt::Display for TaskEnvSecretError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidName { key } => write!(
                f,
                "Runtime Config secret name {} is not an environment name: use uppercase letters, digits and underscores, start with a letter or underscore, and stay within {MAX_TASK_ENV_KEY_BYTES} bytes",
                display_key(key)
            ),
            Self::ReservedName { key } => write!(
                f,
                "Runtime Config secret {} is reserved for the platform and cannot be set",
                display_key(key)
            ),
            Self::ValueTooLarge { key, bytes } => write!(
                f,
                "Runtime Config secret {} is {bytes} bytes; the limit is {MAX_TASK_ENV_VALUE_BYTES} bytes",
                display_key(key)
            ),
            Self::ValueContainsNul { key } => write!(
                f,
                "Runtime Config secret {} contains a NUL character",
                display_key(key)
            ),
            Self::TooMany { count } => write!(
                f,
                "Runtime Config has {count} secrets; the limit is {MAX_TASK_ENV_SECRETS}"
            ),
            Self::TooLarge { bytes } => write!(
                f,
                "Runtime Config secrets serialize to {bytes} bytes; the limit is {MAX_TASK_ENV_SERIALIZED_BYTES} bytes"
            ),
        }
    }
}

impl std::error::Error for TaskEnvSecretError {}

/// A key quoted for a sentence: bounded and free of control characters, so a
/// hostile stored name cannot smuggle terminal control into a notice.
fn display_key(key: &str) -> String {
    let mut shown: String = key
        .chars()
        .take(MAX_TASK_ENV_KEY_BYTES)
        .map(|ch| if ch.is_control() { '?' } else { ch })
        .collect();
    if key.chars().count() > MAX_TASK_ENV_KEY_BYTES {
        shown.push('…');
    }
    format!("`{shown}`")
}

/// `[A-Z_][A-Z0-9_]*`, 1 to [`MAX_TASK_ENV_KEY_BYTES`] bytes, and not a key
/// the platform owns.
pub fn validate_task_env_secret_name(key: &str) -> Result<(), TaskEnvSecretError> {
    let valid_shape = !key.is_empty()
        && key.len() <= MAX_TASK_ENV_KEY_BYTES
        && key.bytes().enumerate().all(|(index, byte)| {
            byte == b'_' || byte.is_ascii_uppercase() || (index > 0 && byte.is_ascii_digit())
        });
    if !valid_shape {
        return Err(TaskEnvSecretError::InvalidName {
            key: key.to_string(),
        });
    }
    if RESERVED_TASK_ENV_PREFIXES
        .iter()
        .any(|prefix| key.starts_with(prefix))
        || RESERVED_TASK_ENV_KEYS.contains(&key)
    {
        return Err(TaskEnvSecretError::ReservedName {
            key: key.to_string(),
        });
    }
    Ok(())
}

/// At most [`MAX_TASK_ENV_VALUE_BYTES`] and no NUL: an environment value
/// cannot carry one, and the shell or module would fail to spawn.
pub fn validate_task_env_secret_value(key: &str, value: &str) -> Result<(), TaskEnvSecretError> {
    if value.len() > MAX_TASK_ENV_VALUE_BYTES {
        return Err(TaskEnvSecretError::ValueTooLarge {
            key: key.to_string(),
            bytes: value.len(),
        });
    }
    if value.contains('\0') {
        return Err(TaskEnvSecretError::ValueContainsNul {
            key: key.to_string(),
        });
    }
    Ok(())
}

/// One entry: name and value rules.
pub fn validate_task_env_secret(key: &str, value: &str) -> Result<(), TaskEnvSecretError> {
    validate_task_env_secret_name(key)?;
    validate_task_env_secret_value(key, value)
}

/// The byte length of the set as one JSON object, which is what every frame
/// carries. Key order does not change the length.
pub fn task_env_serialized_bytes<'a>(
    entries: impl IntoIterator<Item = (&'a str, &'a str)>,
) -> usize {
    let object: BTreeMap<&str, &str> = entries.into_iter().collect();
    serde_json::to_vec(&object).map_or(usize::MAX, |bytes| bytes.len())
}

/// The aggregate rules alone: entry count and serialized size.
pub fn validate_task_env_secret_bounds<'a>(
    entries: impl IntoIterator<Item = (&'a str, &'a str)>,
) -> Result<(), TaskEnvSecretError> {
    let object: BTreeMap<&str, &str> = entries.into_iter().collect();
    validate_bounds(&object)
}

fn validate_bounds(object: &BTreeMap<&str, &str>) -> Result<(), TaskEnvSecretError> {
    if object.len() > MAX_TASK_ENV_SECRETS {
        return Err(TaskEnvSecretError::TooMany {
            count: object.len(),
        });
    }
    let bytes = serde_json::to_vec(object).map_or(usize::MAX, |bytes| bytes.len());
    if bytes > MAX_TASK_ENV_SERIALIZED_BYTES {
        return Err(TaskEnvSecretError::TooLarge { bytes });
    }
    Ok(())
}

/// A whole secret set: the count, then every entry in key order so the
/// refused key is deterministic, then the serialized size.
pub fn validate_task_env_secrets<'a>(
    entries: impl IntoIterator<Item = (&'a str, &'a str)>,
) -> Result<(), TaskEnvSecretError> {
    let object: BTreeMap<&str, &str> = entries.into_iter().collect();
    if object.len() > MAX_TASK_ENV_SECRETS {
        return Err(TaskEnvSecretError::TooMany {
            count: object.len(),
        });
    }
    for (key, value) in &object {
        validate_task_env_secret(key, value)?;
    }
    validate_bounds(&object)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn set(entries: &[(&str, &str)]) -> Result<(), TaskEnvSecretError> {
        validate_task_env_secrets(entries.iter().copied())
    }

    #[test]
    fn names_follow_the_uppercase_environment_shape_within_128_bytes() {
        assert_eq!(validate_task_env_secret_name("OPENAI_API_KEY"), Ok(()));
        assert_eq!(validate_task_env_secret_name("_X1"), Ok(()));
        let longest = "A".repeat(MAX_TASK_ENV_KEY_BYTES);
        assert_eq!(validate_task_env_secret_name(&longest), Ok(()));
        for bad in [
            "",
            "1ABC",
            "lower_case",
            "WITH-DASH",
            "WITH SPACE",
            "ÄPI_KEY",
        ] {
            assert!(
                matches!(
                    validate_task_env_secret_name(bad),
                    Err(TaskEnvSecretError::InvalidName { .. })
                ),
                "{bad:?}"
            );
        }
        let too_long = "A".repeat(MAX_TASK_ENV_KEY_BYTES + 1);
        assert!(matches!(
            validate_task_env_secret_name(&too_long),
            Err(TaskEnvSecretError::InvalidName { .. })
        ));
    }

    #[test]
    fn platform_owned_names_are_reserved() {
        for reserved in [
            "PATH",
            "HOME",
            "G5_ANYTHING",
            "G5D_WORKSPACE_TOKEN",
            "GIT_AUTHOR_EMAIL",
            "GIT_COMMITTER_NAME",
            "GIT_CONFIG_GLOBAL",
            "GIT_CONFIG_NOSYSTEM",
            "GIT_TERMINAL_PROMPT",
            "CODEX_HOME",
            "CLAUDE_CONFIG_DIR",
        ] {
            assert!(
                matches!(
                    validate_task_env_secret_name(reserved),
                    Err(TaskEnvSecretError::ReservedName { .. })
                ),
                "{reserved}"
            );
        }
        // Ordinary user-owned variables, including loader paths, stay allowed.
        for allowed in ["GIT_SSH_COMMAND", "LD_LIBRARY_PATH", "NPM_TOKEN"] {
            assert_eq!(validate_task_env_secret_name(allowed), Ok(()), "{allowed}");
        }
    }

    #[test]
    fn values_are_bounded_and_nul_free() {
        let limit = "v".repeat(MAX_TASK_ENV_VALUE_BYTES);
        assert_eq!(validate_task_env_secret_value("K", &limit), Ok(()));
        let over = "v".repeat(MAX_TASK_ENV_VALUE_BYTES + 1);
        assert_eq!(
            validate_task_env_secret_value("K", &over),
            Err(TaskEnvSecretError::ValueTooLarge {
                key: "K".into(),
                bytes: MAX_TASK_ENV_VALUE_BYTES + 1
            })
        );
        assert_eq!(
            validate_task_env_secret_value("K", "a\0b"),
            Err(TaskEnvSecretError::ValueContainsNul { key: "K".into() })
        );
    }

    #[test]
    fn the_set_is_bounded_in_count_and_serialized_size() {
        let keys: Vec<String> = (0..MAX_TASK_ENV_SECRETS)
            .map(|index| format!("K{index}"))
            .collect();
        let full: Vec<(&str, &str)> = keys.iter().map(|key| (key.as_str(), "v")).collect();
        assert_eq!(set(&full), Ok(()));
        let extra = "EXTRA".to_string();
        let mut over = full.clone();
        over.push((extra.as_str(), "v"));
        assert_eq!(
            set(&over),
            Err(TaskEnvSecretError::TooMany {
                count: MAX_TASK_ENV_SECRETS + 1
            })
        );

        // Three values at the value limit fit the budget; four values alone
        // equal it before the JSON object's own bytes, so they do not.
        let value = "x".repeat(MAX_TASK_ENV_VALUE_BYTES);
        let three: Vec<(&str, &str)> = ["A", "B", "C"]
            .into_iter()
            .map(|key| (key, value.as_str()))
            .collect();
        assert_eq!(set(&three), Ok(()));
        let four: Vec<(&str, &str)> = ["A", "B", "C", "D"]
            .into_iter()
            .map(|key| (key, value.as_str()))
            .collect();
        assert!(matches!(
            set(&four),
            Err(TaskEnvSecretError::TooLarge { .. })
        ));
        assert_eq!(
            task_env_serialized_bytes(four.iter().copied()),
            serde_json::to_vec(&four.iter().copied().collect::<BTreeMap<_, _>>())
                .unwrap()
                .len()
        );
        // The exact budget boundary: one value sized so the object is the
        // budget, then one byte more.
        let fill =
            |extra: usize| "y".repeat(MAX_TASK_ENV_SERIALIZED_BYTES - r#"{"K":""}"#.len() + extra);
        let exact = fill(0);
        assert_eq!(
            set(&[("K", exact.as_str())]),
            Err(TaskEnvSecretError::ValueTooLarge {
                key: "K".into(),
                bytes: exact.len()
            })
        );
        let small_keys: Vec<String> = (0..4).map(|index| format!("K{index}")).collect();
        let chunk = "z".repeat(MAX_TASK_ENV_VALUE_BYTES - 16);
        let near: Vec<(&str, &str)> = small_keys
            .iter()
            .map(|key| (key.as_str(), chunk.as_str()))
            .collect();
        assert_eq!(
            set(&near),
            Ok(()),
            "{}",
            task_env_serialized_bytes(near.iter().copied())
        );
    }

    #[test]
    fn the_refused_key_is_deterministic_and_the_sentence_is_bounded() {
        let control = "B\u{1}AD".to_string();
        let entries = [("ZZZ", "fine"), (control.as_str(), "x"), ("lower", "x")];
        let error = set(&entries).unwrap_err();
        assert_eq!(
            error,
            TaskEnvSecretError::InvalidName {
                key: control.clone()
            }
        );
        let sentence = error.to_string();
        assert!(sentence.contains("`B?AD`"), "{sentence}");
        assert!(!sentence.contains('\u{1}'));
        assert_eq!(error.code(), "secret_name_invalid");
    }
}
