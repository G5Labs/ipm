//! Trusted Git origins for per-user SCM connections. Policies come only from
//! deployment configuration, never repository or tenant input.

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ScmProvider {
    GitHub,
    /// One operator-configured GitHub installation, replacing github.com.
    GitHubInstance {
        host: String,
        port: u16,
        path_prefix: String,
    },
    GitLab,
    /// One operator-configured GitLab installation, replacing gitlab.com.
    GitLabInstance {
        host: String,
        port: u16,
    },
}

/// One trusted origin for each credential namespace. A self-managed GitLab
/// replaces the public origin, so its tokens cannot be sent to gitlab.com.
#[derive(Clone, Debug)]
pub struct ScmOriginPolicy {
    github: Option<ScmProvider>,
    gitlab: Option<ScmProvider>,
}

impl Default for ScmOriginPolicy {
    fn default() -> Self {
        Self {
            github: Some(ScmProvider::GitHub),
            gitlab: Some(ScmProvider::GitLab),
        }
    }
}

impl ScmOriginPolicy {
    /// Derive the GitLab clone origin from the operator's configured API URL.
    /// Invalid or insecure overrides disable GitLab credentials; they never
    /// silently fall back to sending those credentials to the public service.
    pub fn from_gitlab_api_url(endpoint: Option<&str>) -> Self {
        let Some(endpoint) = endpoint else {
            return Self::default();
        };
        let gitlab = https_origin(endpoint).and_then(|(host, port)| {
            if host == "github.com" {
                return None;
            }
            Some(if host == "gitlab.com" && port == 443 {
                ScmProvider::GitLab
            } else {
                ScmProvider::GitLabInstance { host, port }
            })
        });
        Self {
            github: Some(ScmProvider::GitHub),
            gitlab,
        }
    }

    /// Select the GitHub clone origin from trusted repository integration
    /// configuration. Invalid overrides disable it rather than falling back.
    pub fn with_github_web_url(mut self, endpoint: &str) -> Self {
        self.github = https_origin(endpoint).and_then(|(host, port)| {
            if self
                .gitlab
                .as_ref()
                .is_some_and(|p| p.host() == host && p.port() == port)
            {
                return None;
            }
            let path_prefix = url::Url::parse(endpoint)
                .ok()?
                .path()
                .trim_end_matches('/')
                .to_owned();
            Some(
                if host == "github.com" && port == 443 && path_prefix.is_empty() {
                    ScmProvider::GitHub
                } else {
                    ScmProvider::GitHubInstance {
                        host,
                        port,
                        path_prefix,
                    }
                },
            )
        });
        self
    }

    pub fn provider_for_url(&self, git_url: &str) -> Option<ScmProvider> {
        if let Some(provider) = self.github.as_ref().filter(|p| p.allows(git_url)) {
            Some(provider.clone())
        } else {
            self.gitlab
                .as_ref()
                .filter(|provider| provider.allows(git_url))
                .cloned()
        }
    }

    /// Persisted metadata must agree with the trusted origin, never override it.
    pub fn effective_provider(&self, column: Option<&str>, git_url: &str) -> Option<ScmProvider> {
        self.provider_for_url(git_url)
            .filter(|provider| column.is_none_or(|name| name == provider.name()))
    }
}

impl ScmProvider {
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "github" => Some(Self::GitHub),
            "gitlab" => Some(Self::GitLab),
            _ => None,
        }
    }

    pub fn is_github(&self) -> bool {
        matches!(self, Self::GitHub | Self::GitHubInstance { .. })
    }

    pub fn name(&self) -> &'static str {
        match self {
            Self::GitHub | Self::GitHubInstance { .. } => "github",
            Self::GitLab | Self::GitLabInstance { .. } => "gitlab",
        }
    }

    pub fn host(&self) -> &str {
        match self {
            Self::GitHub => "github.com",
            Self::GitLab => "gitlab.com",
            Self::GitHubInstance { host, .. } | Self::GitLabInstance { host, .. } => host,
        }
    }

    pub fn port(&self) -> u16 {
        match self {
            Self::GitHubInstance { port, .. } | Self::GitLabInstance { port, .. } => *port,
            _ => 443,
        }
    }

    pub fn from_git_url(value: &str) -> Option<Self> {
        ScmOriginPolicy::default().provider_for_url(value)
    }

    pub fn allows(&self, git_url: &str) -> bool {
        if !https_origin(git_url)
            .is_some_and(|(host, port)| host == self.host() && port == self.port())
        {
            return false;
        }
        if let Self::GitHubInstance { path_prefix, .. } = self {
            // Check the raw spelling before URL normalization: Git and URL
            // parsers must not disagree about a configured instance boundary.
            if git_url
                .split('/')
                .any(|part| matches!(part, "." | "..") || part.contains('%'))
            {
                return false;
            }
            return url::Url::parse(git_url).ok().is_some_and(|url| {
                url.path()
                    .strip_prefix(&format!("{path_prefix}/"))
                    .is_some_and(|name| !name.is_empty())
            });
        }
        true
    }
}

fn https_origin(value: &str) -> Option<(String, u16)> {
    // Reject normalization that can disagree with Git's transport parser.
    if value.contains('\\') || value.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return None;
    }
    let (_, authority_and_path) = value.split_once("://")?;
    let authority = authority_and_path.split(['/', '?', '#']).next()?;
    if authority.is_empty()
        || !authority.is_ascii()
        || authority.contains('@')
        || authority.contains('%')
    {
        return None;
    }
    let url = url::Url::parse(value).ok()?;
    if url.scheme() != "https"
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return None;
    }
    let host = url.host_str()?;
    if host.ends_with('.') {
        return None;
    }
    Some((host.to_string(), url.port_or_known_default()?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn operator_gitlab_origin_replaces_public_origin_and_binds_port() {
        let policy =
            ScmOriginPolicy::from_gitlab_api_url(Some("https://GitLab.internal:8443/api/v4/user"));
        let provider = policy
            .effective_provider(
                Some("gitlab"),
                "https://gitlab.internal:8443/group/repo.git",
            )
            .unwrap();
        assert_eq!(provider.name(), "gitlab");
        assert_eq!(provider.host(), "gitlab.internal");
        assert_eq!(provider.port(), 8443);
        for url in [
            "https://gitlab.com/group/repo.git",
            "https://gitlab.internal/group/repo.git",
            "http://gitlab.internal:8443/group/repo.git",
            "https://user@gitlab.internal:8443/group/repo.git",
            "https://gitlab.internal.attacker:8443/group/repo.git",
        ] {
            assert!(
                policy.effective_provider(Some("gitlab"), url).is_none(),
                "{url}"
            );
            assert!(!provider.allows(url));
        }
        assert!(policy
            .effective_provider(
                Some("github"),
                "https://gitlab.internal:8443/group/repo.git"
            )
            .is_none());
        assert!(policy
            .effective_provider(Some("github"), "https://github.com/group/repo.git")
            .is_some());
        for endpoint in [
            "http://gitlab.internal/api/v4/user",
            "https://user@gitlab.internal/api/v4/user",
            "https://github.com/api/v4/user",
        ] {
            assert!(ScmOriginPolicy::from_gitlab_api_url(Some(endpoint))
                .effective_provider(Some("gitlab"), "https://gitlab.com/group/repo.git")
                .is_none());
        }
    }

    #[test]
    fn exact_https_origins_only() {
        for url in [
            "https://github.com/o/r.git",
            "https://GitHub.COM:443/o/r.git",
        ] {
            assert_eq!(ScmProvider::from_git_url(url), Some(ScmProvider::GitHub));
        }
        assert_eq!(
            ScmProvider::from_git_url("https://gitlab.com/g/sub/r.git"),
            Some(ScmProvider::GitLab)
        );
        for url in [
            "https://github.com.attacker.example/o/r.git",
            "https://attacker.example/github.com/r.git",
            "https://attacker.example/?github.com",
            "https://github.com@attacker.example/o/r.git",
            "https://attacker@github.com/o/r.git",
            "https://@github.com/o/r.git",
            "https://github.com:secret@github.com/o/r.git",
            "http://github.com/o/r.git",
            "ssh://git@github.com/o/r.git",
            "git@github.com:o/r.git",
            "https://github.com:8443/o/r.git",
            "https://github.com./o/r.git",
            "https://%67ithub.com/o/r.git",
            "https://ｇithub.com/o/r.git",
            "https://github.com\\@attacker.example/o/r.git",
            "https://git\nhub.com/o/r.git",
            " https://github.com/o/r.git",
            "https:///github.com/o/r.git",
            "/tmp/github.com/r.git",
        ] {
            assert_eq!(ScmProvider::from_git_url(url), None, "{url}");
        }
        assert!(!ScmProvider::GitHub.allows("https://gitlab.com/o/r.git"));
    }
}
