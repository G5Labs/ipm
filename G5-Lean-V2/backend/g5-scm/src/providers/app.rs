use super::*;
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};

/// A narrowed installation-token request (`POST
/// /app/installations/{id}/access_tokens`). Empty fields are omitted, which
/// GitHub interprets as "everything the installation grants"; production
/// callers narrow the repositories (by id or, for report delivery, by name)
/// and the permissions.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
pub struct InstallationTokenRequest {
    pub repository_ids: BTreeSet<u64>,
    /// Repository names (without owner) within the installation's account.
    pub repositories: BTreeSet<String>,
    /// GitHub permission name → `read` or `write`.
    pub permissions: BTreeMap<String, String>,
}

impl InstallationTokenRequest {
    /// Request body sent to GitHub.
    #[must_use]
    pub fn body(&self) -> serde_json::Value {
        let mut body = serde_json::Map::new();
        if !self.repository_ids.is_empty() {
            body.insert(
                "repository_ids".to_string(),
                serde_json::json!(self.repository_ids),
            );
        }
        if !self.repositories.is_empty() {
            body.insert(
                "repositories".to_string(),
                serde_json::json!(self.repositories),
            );
        }
        if !self.permissions.is_empty() {
            body.insert(
                "permissions".to_string(),
                serde_json::json!(self.permissions),
            );
        }
        serde_json::Value::Object(body)
    }
}

/// Non-secret installation identity and granted permissions, captured into
/// the immutable binding snapshot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GithubAppInstallation {
    pub installation_id: u64,
    pub account_id: i64,
    pub account_login: String,
    pub permissions: BTreeMap<String, String>,
    pub html_url: Option<String>,
}

/// An installation of this App that an OAuth user can access.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GithubAppAccessibleInstallation {
    pub installation_id: u64,
    pub account_login: String,
}

/// A repository accessible to an installation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GithubAppRepository {
    pub github_repo_id: i64,
    pub full_name: String,
    pub private: bool,
}

#[derive(Clone, Debug)]
pub struct AppGrant {
    pub(super) endpoints: ProviderEndpoints,
    pub(super) jwt: Secret,
}
impl AppGrant {
    pub fn new(endpoints: ProviderEndpoints, selected_jwt: String) -> Self {
        Self {
            endpoints,
            jwt: Secret(selected_jwt),
        }
    }
}
#[derive(Clone, Debug)]
pub struct InstallationGrant {
    endpoints: ProviderEndpoints,
    token: Secret,
    installation_id: u64,
}
impl InstallationGrant {
    pub fn new(endpoints: ProviderEndpoints, installation_id: u64, selected_token: String) -> Self {
        Self {
            endpoints,
            token: Secret(selected_token),
            installation_id,
        }
    }
}
#[derive(Clone, Debug)]
pub struct AppUserGrant {
    endpoints: ProviderEndpoints,
    token: Secret,
    app_id: String,
}
impl AppUserGrant {
    pub fn new(endpoints: ProviderEndpoints, app_id: String, selected_token: String) -> Self {
        Self {
            endpoints,
            token: Secret(selected_token),
            app_id,
        }
    }
}

pub struct AppClient {
    pub(super) http: GithubHttp,
    pub(super) grant: AppGrant,
}
pub struct InstallationClient {
    pub(super) http: GithubHttp,
    pub(super) grant: InstallationGrant,
}
pub struct AppUserClient {
    pub(super) http: GithubHttp,
    pub(super) grant: AppUserGrant,
}

#[derive(Clone, Deserialize)]
pub struct InstallationToken {
    pub token: String,
    pub expires_at: Option<String>,
}
impl std::fmt::Debug for InstallationToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("InstallationToken")
            .field("token", &"[REDACTED]")
            .field("expires_at", &self.expires_at)
            .finish()
    }
}
impl AppClient {
    pub async fn installation_token(
        &self,
        installation_id: u64,
        request: &InstallationTokenRequest,
    ) -> Result<InstallationToken, ProviderError> {
        const OP: &str = "issue installation token";
        self.grant.endpoints.github(OP)?;
        if installation_id == 0 {
            return Err(ProviderError::invalid(OP, "Invalid installation identity"));
        }
        if request
            .repositories
            .iter()
            .any(|name| !valid_name_segment(name))
        {
            return Err(ProviderError::invalid(OP, "Invalid repository name"));
        }
        let response = self
            .http
            .send(Request {
                operation: OP,
                target: Some(&format!("installation {installation_id}")),
                url: self.grant.endpoints.api(&[
                    "app",
                    "installations",
                    &installation_id.to_string(),
                    "access_tokens",
                ]),
                method: reqwest::Method::POST,
                token: Some(&self.grant.jwt.0),
                secrets: &[],
                payload: Payload::Json(request.body()),
                timeout: Duration::from_secs(20),
                limit: MAX_RESPONSE,
                github: true,
                media: None,
            })
            .await?;
        let token: InstallationToken = response.json()?;
        if token.token.trim().is_empty() {
            return Err(response.failure(
                ProviderErrorKind::InvalidResponse,
                "The provider returned an empty installation token",
            ));
        }
        Ok(token)
    }
    pub async fn installation_metadata(
        &self,
        installation_id: u64,
    ) -> Result<GithubAppInstallation, ProviderError> {
        const OP: &str = "read installation metadata";
        self.grant.endpoints.github(OP)?;
        let response = self
            .http
            .read(
                OP,
                self.grant
                    .endpoints
                    .api(&["app", "installations", &installation_id.to_string()]),
                &self.grant.jwt.0,
                true,
            )
            .await?;
        let parsed: InstallationResponse = response.json()?;
        if parsed.id != installation_id
            || parsed.account.id <= 0
            || parsed.account.login.trim().is_empty()
        {
            return Err(response.failure(
                ProviderErrorKind::InvalidResponse,
                "The provider returned an invalid installation identity",
            ));
        }
        Ok(GithubAppInstallation {
            installation_id: parsed.id,
            account_id: parsed.account.id,
            account_login: parsed.account.login,
            permissions: parsed.permissions,
            html_url: parsed.html_url,
        })
    }
}
impl InstallationClient {
    pub async fn repositories(&self) -> Result<Vec<GithubAppRepository>, ProviderError> {
        const OP: &str = "list installation repositories";
        self.grant.endpoints.github(OP)?;
        if self.grant.installation_id == 0 {
            return Err(ProviderError::invalid(OP, "Invalid installation identity"));
        }
        let repositories = inventory(
            &self.http,
            OP,
            self.grant.endpoints.api(&["installation", "repositories"]),
            &self.grant.token.0,
            |page: RepositoryPage| page.repositories,
            "Installation repository inventory exceeds the 100,000-entry limit; narrow the installation selection",
        )
        .await?;
        Ok(repositories
            .into_iter()
            .filter_map(|repo| {
                Some(GithubAppRepository {
                    github_repo_id: i64::try_from(repo.id).ok().filter(|id| *id > 0)?,
                    full_name: repo.full_name,
                    private: repo.private,
                })
            })
            .collect())
    }
}
/// GitHub serves at most 100 entries per page. The cap only bounds a runaway
/// listing (100,000 entries); real installations stay well below it.
const INVENTORY_PAGES: usize = 1000;
const INVENTORY_PAGE_SIZE: usize = 100;
/// The one bounded inventory read: full pages continue, a short page ends
/// the inventory, and more than ten pages is refused rather than truncated.
async fn inventory<P: serde::de::DeserializeOwned, T>(
    http: &GithubHttp,
    operation: &'static str,
    url: Url,
    token: &str,
    items: impl Fn(P) -> Vec<T>,
    exceeded: &'static str,
) -> Result<Vec<T>, ProviderError> {
    let mut all = Vec::new();
    for page in 1..=INVENTORY_PAGES {
        let response = http
            .read(
                operation,
                query(
                    url.clone(),
                    &[
                        ("per_page", INVENTORY_PAGE_SIZE.to_string()),
                        ("page", page.to_string()),
                    ],
                ),
                token,
                true,
            )
            .await?;
        // The provider's Link header, not the page's size, says whether more
        // remain: an exactly full last page is complete.
        let more = response.has_next_page();
        all.extend(items(response.json()?));
        if !more {
            return Ok(all);
        }
    }
    Err(ProviderError::new(
        operation,
        ProviderErrorKind::InvalidResponse,
        crate::FailureEffect::Refused,
        exceeded,
    ))
}
#[derive(Debug)]
pub enum RepositoryAccessError {
    ConnectionRequired,
    AccessDenied,
    RepositoryChanged,
    Failed(ProviderError),
}
fn repository_access_failure(error: ProviderError) -> RepositoryAccessError {
    use RepositoryAccessError as E;
    if error.rate_limited {
        return E::Failed(error);
    }
    match error.status {
        Some(401) => E::ConnectionRequired,
        Some(403 | 404) => E::AccessDenied,
        _ => E::Failed(error),
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct GithubAuthenticatedUser {
    pub id: u64,
    pub login: String,
}
impl AppUserClient {
    pub async fn authenticated_user(&self) -> Result<GithubAuthenticatedUser, ProviderError> {
        const OP: &str = "read authenticated account";
        self.grant.endpoints.github(OP)?;
        let response = self
            .http
            .read(
                OP,
                self.grant.endpoints.api(&["user"]),
                &self.grant.token.0,
                true,
            )
            .await?;
        let user: GithubAuthenticatedUser = response.json()?;
        if user.id == 0 || user.login.trim().is_empty() {
            return Err(response.failure(
                ProviderErrorKind::InvalidResponse,
                "The provider returned an invalid user",
            ));
        }
        Ok(user)
    }
    pub async fn verify_repository_access(
        &self,
        full_name: &str,
        expected_id: u64,
    ) -> Result<(), RepositoryAccessError> {
        use RepositoryAccessError as E;
        const OP: &str = "verify user repository access";
        self.grant.endpoints.github(OP).map_err(E::Failed)?;
        let (owner, repo) = repository_name(full_name).map_err(E::Failed)?;
        let response = self
            .http
            .read(
                OP,
                self.grant.endpoints.api(&["repos", owner, repo]),
                &self.grant.token.0,
                true,
            )
            .await
            .map_err(repository_access_failure)?;
        response
            .require_success()
            .map_err(repository_access_failure)?;
        {
            #[derive(Deserialize)]
            struct Identity {
                id: u64,
            }
            let identity: Identity = response.json().map_err(E::Failed)?;
            if identity.id != expected_id {
                Err(E::RepositoryChanged)
            } else {
                Ok(())
            }
        }
    }
    pub async fn verify_installation_access(
        &self,
        installation_id: u64,
    ) -> Result<(), ProviderError> {
        const OP: &str = "verify user installation access";
        self.grant.endpoints.github(OP)?;
        let response = self
            .http
            .read(
                OP,
                query(
                    self.grant.endpoints.api(&[
                        "user",
                        "installations",
                        &installation_id.to_string(),
                        "repositories",
                    ]),
                    &[("per_page", "1".into()), ("page", "1".into())],
                ),
                &self.grant.token.0,
                true,
            )
            .await?;
        let _: RepositoryPage = response.json()?;
        Ok(())
    }
    pub async fn accessible_installations(
        &self,
    ) -> Result<Vec<GithubAppAccessibleInstallation>, ProviderError> {
        const OP: &str = "list accessible installations";
        self.grant.endpoints.github(OP)?;
        let mut installations: Vec<_> = inventory(
            &self.http,
            OP,
            self.grant.endpoints.api(&["user", "installations"]),
            &self.grant.token.0,
            |page: UserInstallationPage| page.installations,
            "Installation inventory exceeds the 100,000-entry limit; narrow the account selection",
        )
        .await?
        .into_iter()
        .filter(|i| i.app_id.to_string() == self.grant.app_id)
        .map(|i| GithubAppAccessibleInstallation {
            installation_id: i.id,
            account_login: i.account.login,
        })
        .collect();
        installations.sort_by(|l, r| {
            l.account_login
                .cmp(&r.account_login)
                .then(l.installation_id.cmp(&r.installation_id))
        });
        installations.dedup_by(|l, r| l.installation_id == r.installation_id);
        Ok(installations)
    }
}
#[derive(Deserialize)]
struct InstallationResponse {
    id: u64,
    account: AccountResponse,
    #[serde(default)]
    permissions: BTreeMap<String, String>,
    html_url: Option<String>,
}
#[derive(Deserialize)]
struct AccountResponse {
    id: i64,
    login: String,
}
#[derive(Deserialize)]
struct UserInstallationPage {
    installations: Vec<UserInstallationResponse>,
}
#[derive(Deserialize)]
struct UserInstallationResponse {
    id: u64,
    app_id: u64,
    account: AccountResponse,
}
#[derive(Deserialize)]
struct RepositoryPage {
    repositories: Vec<RepositoryResponse>,
}
#[derive(Deserialize)]
struct RepositoryResponse {
    id: u64,
    full_name: String,
    #[serde(default)]
    private: bool,
}
