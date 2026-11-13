//! Source-compatible SCM credential record (`g5d::git::credentials`).
//!
//! Git credentials are injected only per invocation by SCM through the
//! in-memory relay; nothing here writes a credential store or a token-bearing
//! URL. The `Debug` implementation redacts the token.

use serde::Deserialize;

/// SCM credential injected at task dispatch time.
///
/// Contains the repository URL, access token, and SCM provider. The `Debug`
/// implementation redacts the token field for security.
#[derive(Clone, Deserialize)]
pub struct ScmCredential {
    /// Repository URL (e.g., "github.com/org/repo").
    pub url: String,
    /// Access token.
    pub token: String,
    /// SCM provider (e.g., "github", "gitlab", "bitbucket").
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_redacts_the_token() {
        let credential = ScmCredential {
            url: "github.com/org/repo".into(),
            token: "ghs_secret".into(),
            provider: "github".into(),
        };
        let rendered = format!("{credential:?}");
        assert!(rendered.contains("[REDACTED]"));
        assert!(!rendered.contains("ghs_secret"));
    }
}
