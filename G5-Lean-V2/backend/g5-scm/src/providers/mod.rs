//! Provider protocol clients. Credential selection, caching and domain policy
//! remain with the owner that supplies each scoped grant.
//! Consumers cannot obtain the private HTTP client or transport:
//!
//! ```compile_fail
//! use g5_scm::providers::ProviderClients;
//! let raw = ProviderClients::new().unwrap().http;
//! ```
//!
//! ```compile_fail
//! let _ = g5_scm::providers::transport::GithubHttp::new;
//! ```
//!
//! ```
//! # #[cfg(feature = "providers")]
//! # {
//! use g5_scm::providers::{ProviderClients, ProviderEndpoints, RepositoryGrant, RepositoryIdentity, RepositoryCapability};
//! let endpoints = ProviderEndpoints::new(g5_scm::ScmProvider::GitHub, "https://api.github.com", "https://github.com").unwrap();
//! let grant = RepositoryGrant::new(endpoints, RepositoryIdentity::Id(1), RepositoryCapability::IssuesRead, "selected-token".into()).unwrap();
//! let _client = ProviderClients::new().unwrap().repository(grant);
//! # }
//! ```
mod account;
mod app;
mod checks;
mod conversation;
mod error;
pub mod issues;
mod login;
mod pull_attachments;
mod pull_list;
mod pulls;
mod reports;
mod transport;
use crate::ScmProvider;
pub use account::*;
pub use app::*;
pub use checks::{PullRequestChecksQuery, RequiredChecksCache};
pub use conversation::{PostedPullComment, ReviewThreadState};
pub use error::{ProviderError, ProviderErrorKind};
pub use login::*;
pub use pull_attachments::{PullTextKind, RenderedPull, RenderedPullText};
pub use pull_list::*;
pub use pulls::*;
pub use reports::*;
use std::time::Duration;
use transport::{GithubHttp, Payload, Request, Response, MAX_RESPONSE};
use url::Url;

#[derive(Clone)]
struct Secret(String);
impl std::fmt::Debug for Secret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("[REDACTED]")
    }
}

/// Independently configured integration API and browser origins. Construction
/// belongs to the deployment/account owner, never to repository response data.
#[derive(Clone, Debug)]
pub struct ProviderEndpoints {
    provider: ScmProvider,
    api: Url,
    web: Url,
}
impl ProviderEndpoints {
    pub fn new(provider: ScmProvider, api: &str, web: &str) -> Result<Self, ProviderError> {
        Ok(Self {
            provider,
            api: endpoint(api, false)?,
            web: endpoint(web, false)?,
        })
    }
    /// Interpret a clone identity only within this integration's selected web
    /// origin and path prefix. Persisted provider names never override it.
    pub fn repository_name_from_clone_url(&self, value: &str) -> Option<String> {
        self.github("select repository").ok()?;
        clone_repository_name(&self.web, value)
    }
    /// Path below this integration's web base when `value` names a resource
    /// on it: the same scheme, host and port, no userinfo, under the configured
    /// prefix. Fragments and queries are allowed (provider links carry them).
    pub(super) fn web_path(&self, value: &str) -> Option<Vec<String>> {
        owned_path(&self.web, value, false)
    }
    /// [`Self::web_path`] for an exact resource identity: no query or fragment.
    pub(super) fn exact_web_path(&self, value: &str) -> Option<Vec<String>> {
        owned_path(&self.web, value, true)
    }
    /// Path below this integration's API base when `value` belongs to it.
    pub(super) fn api_path(&self, value: &str) -> Option<Vec<String>> {
        owned_path(&self.api, value, false)
    }

    fn api(&self, segments: &[&str]) -> Url {
        append(&self.api, segments)
    }
    /// GitHub's GraphQL endpoint: `/graphql` beside the REST root, or
    /// `/api/graphql` for an Enterprise Server REST base ending in `/api/v3`.
    fn graphql(&self) -> Url {
        let mut url = self.api.clone();
        let segments: Vec<_> = url
            .path_segments()
            .into_iter()
            .flatten()
            .filter(|s| !s.is_empty())
            .collect();
        let prefix = match segments.as_slice() {
            [prefix @ .., "api", "v3"] => [prefix, &["api"]].concat(),
            all => all.to_vec(),
        }
        .join("/");
        url.set_path(&prefix);
        append(&url, &["graphql"])
    }
    fn web(&self, segments: &[&str]) -> Url {
        append(&self.web, segments)
    }
    fn github(&self, operation: &'static str) -> Result<(), ProviderError> {
        if self.provider.is_github() {
            Ok(())
        } else {
            Err(ProviderError::unsupported(operation))
        }
    }
}
fn endpoint(value: &str, allow_query: bool) -> Result<Url, ProviderError> {
    let url = Url::parse(value)
        .map_err(|_| ProviderError::invalid("configure endpoints", "Invalid provider endpoint"))?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
        || (!allow_query && url.query().is_some())
    {
        return Err(ProviderError::invalid(
            "configure endpoints",
            "Provider endpoints require an HTTP origin without credentials or fragments",
        ));
    }
    Ok(url)
}
fn append(base: &Url, segments: &[&str]) -> Url {
    let mut url = base.clone();
    // Endpoint validation requires HTTP, which always supports path segments.
    url.path_segments_mut()
        .expect("validated HTTP endpoint")
        .pop_if_empty()
        .extend(segments);
    url
}
fn query(mut url: Url, values: &[(&str, String)]) -> Url {
    url.query_pairs_mut()
        .extend_pairs(values.iter().map(|(k, v)| (*k, v.as_str())));
    url
}

/// The one ownership check for provider-returned and user-supplied links.
fn owned_path(base: &Url, value: &str, exact: bool) -> Option<Vec<String>> {
    let url = Url::parse(value).ok()?;
    if url.origin() != base.origin()
        || !url.username().is_empty()
        || url.password().is_some()
        || (exact && (url.query().is_some() || url.fragment().is_some()))
    {
        return None;
    }
    let prefix: Vec<_> = base.path_segments()?.filter(|s| !s.is_empty()).collect();
    let segments: Vec<_> = url.path_segments()?.filter(|s| !s.is_empty()).collect();
    Some(
        segments
            .strip_prefix(prefix.as_slice())?
            .iter()
            .map(|s| (*s).to_owned())
            .collect(),
    )
}

/// The one clone-URL identity parser for a selected GitHub web origin, port
/// and path prefix. SSH spellings are accepted only for a default-port origin.
fn clone_repository_name(web: &Url, value: &str) -> Option<String> {
    if value.contains('\\') || value.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return None;
    }
    let ssh_prefix = format!("git@{}:", web.host_str()?);
    let normalized;
    let value = if let Some(path) = value.strip_prefix(&ssh_prefix) {
        if web.port().is_some() {
            return None;
        }
        normalized = format!("{}://{}/{path}", web.scheme(), web.host_str()?);
        &normalized
    } else {
        value
    };
    // Reject path normalization that could differ from the Git transport.
    if value
        .split('/')
        .any(|p| matches!(p, "." | "..") || p.contains('%'))
    {
        return None;
    }
    let mut url = Url::parse(value).ok()?;
    if url.scheme() == "ssh"
        && url.username() == "git"
        && url.password().is_none()
        && url.port().is_none()
        && web.port().is_none()
    {
        let path = url.path().to_owned();
        let host = url.host_str()?;
        url = Url::parse(&format!("{}://{host}{path}", web.scheme())).ok()?;
    }
    if url.origin() != web.origin()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return None;
    }
    let prefix = format!("{}/", web.path().trim_end_matches('/'));
    // Stored clone URLs may carry trailing slashes (`…/repo/`, `…/repo.git/`);
    // they name the same repository, as they did before this parser.
    let path = url.path().strip_prefix(&prefix)?.trim_end_matches('/');
    let name = path.strip_suffix(".git").unwrap_or(path);
    repository_name(name).ok()?;
    Some(name.to_owned())
}

impl ScmProvider {
    /// Canonical `owner/repository` of a clone URL on this configured GitHub
    /// origin, port and path prefix; `None` for other providers or URLs.
    pub fn github_repository_name(&self, git_url: &str) -> Option<String> {
        let web = match self {
            Self::GitHub => Url::parse("https://github.com").ok()?,
            Self::GitHubInstance {
                host,
                port,
                path_prefix,
            } => Url::parse(&format!("https://{host}:{port}{path_prefix}")).ok()?,
            Self::GitLab | Self::GitLabInstance { .. } => return None,
        };
        clone_repository_name(&web, git_url)
    }
}

#[derive(Clone)]
pub struct ProviderClients {
    http: GithubHttp,
}
impl ProviderClients {
    pub fn new() -> Result<Self, ProviderError> {
        Ok(Self {
            http: GithubHttp::new()?,
        })
    }
    pub fn app(&self, grant: AppGrant) -> AppClient {
        AppClient {
            http: self.http.clone(),
            grant,
        }
    }
    pub fn installation(&self, grant: InstallationGrant) -> InstallationClient {
        InstallationClient {
            http: self.http.clone(),
            grant,
        }
    }
    pub fn app_user(&self, grant: AppUserGrant) -> AppUserClient {
        AppUserClient {
            http: self.http.clone(),
            grant,
        }
    }
    pub fn login(&self, config: LoginConfig) -> LoginClient {
        LoginClient {
            http: self.http.clone(),
            config,
        }
    }
}

impl GithubHttp {
    async fn read(
        &self,
        operation: &'static str,
        url: Url,
        token: &str,
        github: bool,
    ) -> Result<Response, ProviderError> {
        self.send(Request {
            operation,
            target: url.host_str(),
            url: url.clone(),
            method: reqwest::Method::GET,
            token: Some(token),
            secrets: &[],
            payload: Payload::Empty,
            timeout: Duration::from_secs(20),
            limit: MAX_RESPONSE,
            github,
            media: None,
        })
        .await
    }
}
/// The one owner/repository validator: GitHub's name alphabet, never a path.
fn repository_name(name: &str) -> Result<(&str, &str), ProviderError> {
    let (owner, repo) = name.split_once('/').ok_or_else(|| {
        ProviderError::invalid("select repository", "Expected an owner/repository name")
    })?;
    if !valid_repository_parts(owner, repo) {
        return Err(ProviderError::invalid(
            "select repository",
            "Invalid repository name",
        ));
    }
    Ok((owner, repo))
}
fn valid_repository_parts(owner: &str, repo: &str) -> bool {
    valid_name_segment(owner) && valid_name_segment(repo)
}
fn valid_name_segment(part: &str) -> bool {
    (1..=100).contains(&part.len())
        && !matches!(part, "." | "..")
        && part
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'-' | b'_' | b'.'))
}

#[derive(Clone, Debug)]
pub enum RepositoryIdentity {
    Name(String),
    Id(u64),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RepositoryCapability {
    MetadataRead,
    ContentsRead,
    IssuesRead,
    IssueCommentWrite,
    /// Create an issue, or close or reopen one, with the person's user
    /// access token.
    IssueWrite,
    PullRequestRead,
    PullRequestWrite,
    PullRequestMerge,
    ChecksRead,
    /// The platform report owner's narrowed issues/contents write grant; it is
    /// usable only through [`ReportClient`].
    ReportDelivery,
}
#[derive(Clone, Debug)]
pub struct RepositoryGrant {
    endpoints: ProviderEndpoints,
    repository: RepositoryIdentity,
    capability: RepositoryCapability,
    token: Secret,
}
impl RepositoryGrant {
    pub fn new(
        endpoints: ProviderEndpoints,
        repository: RepositoryIdentity,
        capability: RepositoryCapability,
        selected_token: String,
    ) -> Result<Self, ProviderError> {
        match &repository {
            RepositoryIdentity::Name(name) => {
                repository_name(name)?;
            }
            RepositoryIdentity::Id(0) => {
                return Err(ProviderError::invalid(
                    "select repository",
                    "Invalid repository identity",
                ))
            }
            RepositoryIdentity::Id(_) => {}
        }
        Ok(Self {
            endpoints,
            repository,
            capability,
            token: Secret(selected_token),
        })
    }
}
pub struct RepositoryClient {
    http: GithubHttp,
    grant: RepositoryGrant,
}
impl ProviderClients {
    pub fn repository(&self, grant: RepositoryGrant) -> RepositoryClient {
        RepositoryClient {
            http: self.http.clone(),
            grant,
        }
    }
}
impl RepositoryClient {
    fn target(&self) -> String {
        match &self.grant.repository {
            RepositoryIdentity::Name(name) => name.clone(),
            RepositoryIdentity::Id(id) => format!("repository {id}"),
        }
    }
    fn require(
        &self,
        operation: &'static str,
        capabilities: &[RepositoryCapability],
    ) -> Result<(), ProviderError> {
        self.grant.endpoints.github(operation)?;
        if capabilities.contains(&self.grant.capability) {
            Ok(())
        } else {
            Err(ProviderError::invalid(
                operation,
                "The selected repository grant does not permit this operation",
            ))
        }
    }
    fn repository_id(&self) -> Result<u64, ProviderError> {
        match self.grant.repository {
            RepositoryIdentity::Id(id) => Ok(id),
            _ => Err(ProviderError::invalid(
                "read issue repository",
                "This operation requires the selected numeric repository identity",
            )),
        }
    }
    fn repository_name(&self) -> Result<&str, ProviderError> {
        match &self.grant.repository {
            RepositoryIdentity::Name(name) => Ok(name),
            _ => Err(ProviderError::invalid(
                "read repository",
                "This operation requires the selected repository name",
            )),
        }
    }
    /// `repos/{owner}/{name}/...` under the selected API base.
    fn repo_url(&self, suffix: &[&str]) -> Result<Url, ProviderError> {
        let (owner, name) = repository_name(self.repository_name()?)?;
        let mut segments = vec!["repos", owner, name];
        segments.extend_from_slice(suffix);
        Ok(self.grant.endpoints.api(&segments))
    }
    async fn request(
        &self,
        operation: &'static str,
        method: reqwest::Method,
        url: Url,
        payload: Payload<'_>,
    ) -> Result<Response, ProviderError> {
        self.send(operation, method, url, payload, &[], REPOSITORY_TIMEOUT)
            .await
    }
    async fn send(
        &self,
        operation: &'static str,
        method: reqwest::Method,
        url: Url,
        payload: Payload<'_>,
        secrets: &[&str],
        timeout: Duration,
    ) -> Result<Response, ProviderError> {
        self.send_media(operation, method, url, payload, secrets, timeout, None)
            .await
    }
    /// The one request path for every repository-scoped client: the grant's
    /// token and target, owner-supplied extra secrets, the call's budget and
    /// any GitHub media type it asks for.
    #[allow(clippy::too_many_arguments)]
    async fn send_media(
        &self,
        operation: &'static str,
        method: reqwest::Method,
        url: Url,
        payload: Payload<'_>,
        secrets: &[&str],
        timeout: Duration,
        media: Option<&'static str>,
    ) -> Result<Response, ProviderError> {
        self.http
            .send(Request {
                operation,
                target: Some(&self.target()),
                url,
                method,
                token: Some(&self.grant.token.0),
                secrets,
                payload,
                timeout,
                // A rendered representation carries each text about three
                // times (`body`, `body_text`, `body_html`), so a page of 100
                // comments needs more than a plain read.
                limit: if media.is_some() {
                    MAX_RENDERED_RESPONSE
                } else {
                    MAX_RESPONSE
                },
                github: true,
                media,
            })
            .await
    }
}
const REPOSITORY_TIMEOUT: Duration = Duration::from_secs(30);
/// Largest response to a read in a GitHub media type such as `full+json`.
const MAX_RENDERED_RESPONSE: usize = 4 * MAX_RESPONSE;

#[cfg(test)]
mod tests;
