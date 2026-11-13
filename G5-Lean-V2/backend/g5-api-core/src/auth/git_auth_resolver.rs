//! Provider identity utilities; credential selection belongs to RepositoryCredentials.

use super::ScmTokenManager;

/// Infers the SCM provider from a clone URL when `repos.provider` is
/// NULL. Manually-added repos store `provider = NULL` by design (per
/// `g5/g5-api/models/repo` D1), but their clone URL still identifies the
/// host. Returns the canonical provider slug used in
/// `user_scm_connections.provider` (`github`, `gitlab`), or `None` for an
/// unrecognized / self-hosted host.
///
/// Only an exact supported HTTPS origin without userinfo is eligible for a token.
#[must_use]
#[cfg(test)]
fn infer_provider_from_url(git_url: &str) -> Option<&'static str> {
    g5_scm::ScmProvider::from_git_url(git_url).map(|provider| provider.name())
}

/// Resolve a provider only when persisted metadata agrees with the destination.
/// A NULL column may infer a supported provider; a mismatched column fails closed.
#[must_use]
pub fn effective_provider(
    scm: Option<&ScmTokenManager>,
    column: Option<&str>,
    git_url: &str,
) -> Option<String> {
    let defaults = g5_scm::ScmOriginPolicy::default();
    scm.map_or(&defaults, ScmTokenManager::origin_policy)
        .effective_provider(column, git_url)
        .map(|provider| provider.name().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn infer_provider_from_url_recognizes_known_hosts() {
        assert_eq!(
            infer_provider_from_url("https://github.com/o/r.git"),
            Some("github")
        );
        assert_eq!(infer_provider_from_url("git@github.com:o/r.git"), None);
        // Case-insensitive host match.
        assert_eq!(
            infer_provider_from_url("https://GitHub.COM/o/r.git"),
            Some("github")
        );
        assert_eq!(
            infer_provider_from_url("https://gitlab.com/o/r.git"),
            Some("gitlab")
        );
        // Self-hosted / unrecognized host → no provider.
        assert_eq!(
            infer_provider_from_url("https://git.example.com/o/r.git"),
            None
        );
        assert_eq!(infer_provider_from_url("/tmp/local/bare-repo.git"), None);
        assert_eq!(
            infer_provider_from_url("https://github.com.evil/repo"),
            None
        );
        assert_eq!(
            infer_provider_from_url("https://evil/github.com/repo"),
            None
        );
    }

    #[test]
    fn effective_provider_requires_column_to_match_origin() {
        // A persisted provider never overrides the actual destination.
        assert_eq!(
            effective_provider(None, Some("gitlab"), "https://github.com/o/r.git"),
            None
        );
        // NULL column + recognized host → inferred provider.
        assert_eq!(
            effective_provider(None, None, "https://github.com/o/r.git"),
            Some("github".to_string())
        );
        // NULL column + self-hosted host → still None (unsupported).
        assert_eq!(
            effective_provider(None, None, "https://git.example.com/o/r.git"),
            None
        );
    }
}
