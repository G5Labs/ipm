//! Neutral commit author identity shared by contracts and SCM.

use serde::{Deserialize, Serialize};

/// Author/committer information for Git commits.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitAuthor {
    pub name: String,
    pub email: String,
}

impl GitAuthor {
    /// Builds an author from untrusted profile fields while guaranteeing that
    /// each component is usable by a Git signature implementation.
    #[must_use]
    pub fn from_profile(display_name: Option<&str>, email: Option<&str>) -> Self {
        GitAuthor {
            name: display_name
                .filter(|name| is_valid_signature_component(name))
                .unwrap_or("G5")
                .to_string(),
            email: email
                .filter(|email| is_valid_signature_component(email))
                .unwrap_or("g5@g5labs.dev")
                .to_string(),
        }
    }
}

#[must_use]
fn is_valid_signature_component(value: &str) -> bool {
    !value.trim().is_empty()
        && !value
            .chars()
            .any(|character| character == '<' || character == '>' || character.is_control())
}

#[cfg(any(test, feature = "test-helpers"))]
impl GitAuthor {
    /// Creates a deterministic author for unit tests.
    #[must_use]
    pub fn test_author() -> Self {
        GitAuthor {
            name: "Test User".to_string(),
            email: "test@example.com".to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_author_helper_is_stable() {
        let author = GitAuthor::test_author();
        assert_eq!(author.name, "Test User");
        assert_eq!(author.email, "test@example.com");
    }

    #[test]
    fn clone_preserves_fields() {
        let author = GitAuthor {
            name: "Alice".to_string(),
            email: "alice@example.com".to_string(),
        };
        let cloned = author.clone();
        assert_eq!(cloned.name, "Alice");
        assert_eq!(cloned.email, "alice@example.com");
    }

    #[test]
    fn profile_fields_are_sanitized_independently() {
        let author = GitAuthor::from_profile(Some("Ada Lovelace"), Some("ada@example.com"));
        assert_eq!(author.name, "Ada Lovelace");
        assert_eq!(author.email, "ada@example.com");

        assert_eq!(GitAuthor::from_profile(Some("Eve <evil>"), None).name, "G5");
        assert_eq!(
            GitAuthor::from_profile(Some("Eve\nMallory"), None).name,
            "G5"
        );
        assert_eq!(GitAuthor::from_profile(Some("   "), None).name, "G5");
        assert_eq!(GitAuthor::from_profile(None, None).name, "G5");
        assert_eq!(GitAuthor::from_profile(None, None).email, "g5@g5labs.dev");

        let mixed = GitAuthor::from_profile(Some("Grace Hopper"), Some("grace<@>x"));
        assert_eq!(mixed.name, "Grace Hopper");
        assert_eq!(mixed.email, "g5@g5labs.dev");
    }
}
