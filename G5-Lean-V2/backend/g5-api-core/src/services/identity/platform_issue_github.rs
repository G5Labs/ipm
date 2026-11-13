//! GitHub REST client for platform bug-report delivery.
//!
//! This module owns the server-side GitHub App authentication and the two REST
//! calls used to deliver a platform bug report:
//!
//! - `POST /repos/{owner}/{repo}/issues` — create the issue.
//! - `PUT  /repos/{owner}/{repo}/contents/{path}` — commit a screenshot asset
//!   into the same repo (there is no direct attachment upload on the issue API).
//!
//! Authentication uses a single G5-owned **GitHub App**, never the reporting
//! user's identity: a short-lived RS256 App JWT is minted from the App private
//! key and exchanged for a per-installation access token scoped to the one
//! target repo (least privilege — `issues: write`, plus `contents: write` for
//! the screenshot path). The private key is a server-only secret and is redacted
//! in `Debug` per `security_policy/secret_custody_and_redaction#D8`.

use std::sync::Arc;

use g5_contracts::error::G5Error;
pub use g5_scm::providers::CreatedReportIssue as CreatedIssue;
use g5_scm::providers::{
    AppGrant, InstallationTokenRequest, ProviderClients, ProviderEndpoints, ProviderError,
    ReportClient, ReportGrant,
};
use uuid::Uuid;

use crate::services::identity::github_app_auth::{
    redact_message, sign_app_jwt, InstallationTokenCache,
};

use crate::config::PlatformIssueConfig;

/// Resolved GitHub delivery configuration for platform bug reports.
///
/// Constructed only when the feature is enabled and every required field is
/// present (fail-closed). Holds the server-only App private key; the custom
/// [`std::fmt::Debug`] impl redacts it.
#[derive(Clone)]
pub struct PlatformIssueGithubConfig {
    pub owner: String,
    pub repo: String,
    pub app_id: String,
    pub installation_id: String,
    pub private_key_pem: String,
    pub api_base_url: String,
    pub web_base_url: String,
}

impl std::fmt::Debug for PlatformIssueGithubConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PlatformIssueGithubConfig")
            .field("owner", &self.owner)
            .field("repo", &self.repo)
            .field("app_id", &self.app_id)
            .field("installation_id", &self.installation_id)
            .field("private_key_pem", &"[REDACTED]")
            .field("api_base_url", &self.api_base_url)
            .field("web_base_url", &self.web_base_url)
            .finish()
    }
}

impl PlatformIssueGithubConfig {
    /// Resolve the delivery config from [`PlatformIssueConfig`]. Returns `None`
    /// unless the feature is enabled AND all required fields are present AND the
    /// repo parses as `"owner/repo"` — the single fail-closed resolution point.
    #[must_use]
    pub fn from_platform_config(cfg: &PlatformIssueConfig) -> Option<Self> {
        if !cfg.is_deliverable() {
            return None;
        }
        let repo = cfg.github_repo.as_deref()?.trim();
        let (owner, name) = repo.split_once('/')?;
        let owner = owner.trim();
        let name = name.trim();
        if owner.is_empty() || name.is_empty() || name.contains('/') {
            return None;
        }
        Some(Self {
            owner: owner.to_string(),
            repo: name.to_string(),
            app_id: cfg.github_app_id.as_deref()?.trim().to_string(),
            installation_id: cfg
                .github_app_installation_id
                .as_deref()?
                .trim()
                .to_string(),
            private_key_pem: cfg.github_app_private_key.clone()?,
            api_base_url: cfg.github_api_base_url.trim_end_matches('/').to_string(),
            web_base_url: cfg.github_web_base_url.trim_end_matches('/').to_string(),
        })
    }
}

/// GitHub REST client bound to one target repo and one App installation.
pub struct PlatformIssueGithubClient {
    providers: ProviderClients,
    cfg: PlatformIssueGithubConfig,
    /// This App's own cache: one destination, so one entry.
    token: InstallationTokenCache<()>,
}

impl PlatformIssueGithubClient {
    pub fn provider_clients() -> Result<ProviderClients, ProviderError> {
        ProviderClients::new()
    }
    #[must_use]
    pub fn new(providers: ProviderClients, cfg: PlatformIssueGithubConfig) -> Self {
        Self {
            providers,
            cfg,
            token: InstallationTokenCache::new(),
        }
    }
    #[must_use]
    pub fn from_platform_config(
        providers: ProviderClients,
        cfg: &PlatformIssueConfig,
    ) -> Option<Arc<Self>> {
        let cfg = PlatformIssueGithubConfig::from_platform_config(cfg)?;
        if let Err(error) = ProviderEndpoints::new(
            g5_scm::ScmProvider::GitHub,
            &cfg.api_base_url,
            &cfg.web_base_url,
        ) {
            tracing::warn!(%error, "Platform issue delivery disabled: invalid GitHub endpoints");
            return None;
        }
        Some(Arc::new(Self::new(providers, cfg)))
    }
    fn endpoints(&self) -> Result<ProviderEndpoints, G5Error> {
        ProviderEndpoints::new(
            g5_scm::ScmProvider::GitHub,
            &self.cfg.api_base_url,
            &self.cfg.web_base_url,
        )
        .map_err(G5Error::from)
    }
    async fn protocol(&self) -> Result<ReportClient, G5Error> {
        let token = self.installation_token().await?;
        Ok(self.providers.reports(ReportGrant::new(
            self.endpoints()?,
            format!("{}/{}", self.cfg.owner, self.cfg.repo),
            token,
        )?))
    }
    pub async fn create_issue(
        &self,
        title: &str,
        body: &str,
        labels: &[String],
    ) -> Result<CreatedIssue, G5Error> {
        self.protocol()
            .await?
            .create_issue(title, body, labels)
            .await
            .map_err(G5Error::from)
    }
    pub async fn find_issue_by_marker(&self, id: Uuid) -> Result<Option<CreatedIssue>, G5Error> {
        self.protocol()
            .await?
            .find_issue_by_marker(id)
            .await
            .map_err(G5Error::from)
    }
    pub async fn find_screenshot_url(&self, id: Uuid) -> Result<Option<String>, G5Error> {
        self.protocol()
            .await?
            .find_screenshot_url(id)
            .await
            .map_err(G5Error::from)
    }
    pub async fn put_screenshot(&self, id: Uuid, png: &[u8]) -> Result<String, G5Error> {
        self.protocol()
            .await?
            .put_screenshot(id, png)
            .await
            .map_err(G5Error::from)
    }

    /// Issue (or reuse a cached) installation token narrowed to the one report
    /// destination, through the same issuance request as every App token.
    async fn installation_token(&self) -> Result<String, G5Error> {
        if let Some(token) = self.token.get(&()).await {
            return Ok(token);
        }
        let installation = self
            .cfg
            .installation_id
            .parse::<u64>()
            .ok()
            .filter(|id| *id > 0)
            .ok_or_else(|| github_error("Invalid report installation identity"))?;
        let app_jwt =
            sign_app_jwt(&self.cfg.app_id, &self.cfg.private_key_pem).map_err(github_error)?;
        let issued = self
            .providers
            .app(AppGrant::new(self.endpoints()?, app_jwt))
            .installation_token(
                installation,
                &InstallationTokenRequest::report_delivery(&format!(
                    "{}/{}",
                    self.cfg.owner, self.cfg.repo
                ))?,
            )
            .await?;
        self.token.insert((), &issued).await;
        Ok(issued.token)
    }
}

/// Build a redacted `G5Error::Internal` for a GitHub delivery failure. The
/// message never carries credentials, tokens, or URLs (D1 no-leak invariant).
fn github_error(message: impl AsRef<str>) -> G5Error {
    G5Error::Internal {
        message: format!(
            "platform-issue GitHub delivery failed: {}",
            redact_message(message.as_ref())
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base_cfg() -> PlatformIssueConfig {
        PlatformIssueConfig {
            enabled: true,
            github_repo: Some("g5labs/platform-bugs".to_string()),
            github_app_id: Some("123456".to_string()),
            github_app_installation_id: Some("987654".to_string()),
            github_app_private_key: Some(
                "-----BEGIN PRIVATE KEY-----\nx\n-----END PRIVATE KEY-----".to_string(),
            ),
            github_api_base_url: "https://api.github.com".to_string(),
            github_web_base_url: "https://github.com".to_string(),
        }
    }

    #[test]
    fn resolves_owner_and_repo_when_deliverable() {
        let resolved = PlatformIssueGithubConfig::from_platform_config(&base_cfg()).unwrap();
        assert_eq!(resolved.owner, "g5labs");
        assert_eq!(resolved.repo, "platform-bugs");
        assert_eq!(resolved.api_base_url, "https://api.github.com");
    }

    #[test]
    fn none_when_disabled() {
        let mut cfg = base_cfg();
        cfg.enabled = false;
        assert!(PlatformIssueGithubConfig::from_platform_config(&cfg).is_none());
    }

    #[test]
    fn none_when_repo_missing_or_malformed() {
        let mut cfg = base_cfg();
        cfg.github_repo = None;
        assert!(PlatformIssueGithubConfig::from_platform_config(&cfg).is_none());

        let mut cfg2 = base_cfg();
        cfg2.github_repo = Some("no-slash".to_string());
        assert!(PlatformIssueGithubConfig::from_platform_config(&cfg2).is_none());
    }

    #[test]
    fn debug_redacts_private_key() {
        let resolved = PlatformIssueGithubConfig::from_platform_config(&base_cfg()).unwrap();
        let rendered = format!("{resolved:?}");
        assert!(rendered.contains("[REDACTED]"));
        assert!(!rendered.contains("BEGIN PRIVATE KEY"));
    }

    #[test]
    fn delivery_errors_use_the_shared_redactor() {
        let error =
            github_error("failed https://x-access-token:ghs_secret@api.github.com/repos/x/y plain");
        let text = error.to_string();
        assert!(!text.contains("ghs_secret"), "{text}");
        assert!(text.contains("plain"), "{text}");
    }
}
