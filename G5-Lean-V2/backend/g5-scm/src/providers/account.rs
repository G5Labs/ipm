//! Candidate validation and account discovery precede repository selection.
use super::*;
use chrono::{DateTime, Utc};
use serde::Deserialize;

#[derive(Clone, Debug)]
pub struct CandidateGrant {
    provider: ScmProvider,
    api: Url,
    token: Secret,
}
impl CandidateGrant {
    pub fn new(
        provider: ScmProvider,
        api_base: &str,
        selected_token: String,
    ) -> Result<Self, ProviderError> {
        Ok(Self {
            provider,
            api: endpoint(api_base, false)?,
            token: Secret(selected_token),
        })
    }
}
#[derive(Clone, Debug)]
pub struct AccountGrant {
    provider: ScmProvider,
    api: Url,
    token: Secret,
}
impl AccountGrant {
    pub fn new(
        provider: ScmProvider,
        api_base: &str,
        selected_token: String,
    ) -> Result<Self, ProviderError> {
        Ok(Self {
            provider,
            api: endpoint(api_base, false)?,
            token: Secret(selected_token),
        })
    }
}
pub struct CandidateClient {
    pub(super) http: GithubHttp,
    pub(super) grant: CandidateGrant,
}
pub struct AccountClient {
    pub(super) http: GithubHttp,
    pub(super) grant: AccountGrant,
}
impl ProviderClients {
    pub fn candidate(&self, grant: CandidateGrant) -> CandidateClient {
        CandidateClient {
            http: self.http.clone(),
            grant,
        }
    }
    pub fn account(&self, grant: AccountGrant) -> AccountClient {
        AccountClient {
            http: self.http.clone(),
            grant,
        }
    }
}
#[derive(Clone, Debug)]
pub struct CandidateIdentity {
    pub provider_user_id: String,
    pub login: String,
    pub expires_at: Option<DateTime<Utc>>,
}
impl CandidateClient {
    pub async fn validate(&self) -> Result<CandidateIdentity, ProviderError> {
        let github = self.grant.provider.is_github();
        let response = self
            .http
            .read(
                "validate candidate connection",
                append(&self.grant.api, &["user"]),
                &self.grant.token.0,
                github,
            )
            .await?;
        #[derive(Deserialize)]
        struct GithubIdentity {
            id: i64,
            login: String,
        }
        #[derive(Deserialize)]
        struct GitlabIdentity {
            id: i64,
            username: String,
        }
        let (id, login) = match self.grant.provider {
            ScmProvider::GitHub | ScmProvider::GitHubInstance { .. } => {
                let user: GithubIdentity = response.json()?;
                (user.id, user.login)
            }
            ScmProvider::GitLab | ScmProvider::GitLabInstance { .. } => {
                let user: GitlabIdentity = response.json()?;
                (user.id, user.username)
            }
        };
        if id <= 0 || login.trim().is_empty() {
            return Err(response.failure(
                ProviderErrorKind::InvalidResponse,
                "The provider returned an invalid candidate identity",
            ));
        }
        let expires_at = if github {
            response
                .headers
                .get("github-authentication-token-expiration")
                .and_then(|v| v.to_str().ok())
                .and_then(parse_github_token_expiry)
        } else {
            None
        };
        Ok(CandidateIdentity {
            provider_user_id: id.to_string(),
            login,
            expires_at,
        })
    }
}
fn parse_github_token_expiry(raw: &str) -> Option<DateTime<Utc>> {
    let raw = raw.trim();
    let raw = raw.strip_suffix(" UTC").unwrap_or(raw);
    chrono::NaiveDateTime::parse_from_str(raw, "%Y-%m-%d %H:%M:%S")
        .ok()
        .map(|naive| DateTime::from_naive_utc_and_offset(naive, Utc))
}
#[derive(Clone, Debug)]
pub struct AccountRepository {
    pub provider_repo_id: String,
    pub full_name: String,
    pub name: String,
    pub owner: String,
    pub default_branch: String,
    pub is_private: bool,
    pub clone_url: String,
    pub description: Option<String>,
    pub language: Option<String>,
    pub updated_at: String,
}
#[derive(Clone, Debug)]
pub struct AccountRepositoryPage {
    pub repositories: Vec<AccountRepository>,
    pub next_page: Option<u32>,
}
#[derive(Clone, Debug)]
pub struct AccountOrganization {
    pub id: String,
    pub name: String,
    pub avatar_url: Option<String>,
}
impl AccountClient {
    pub async fn repositories(&self, page: u32) -> Result<AccountRepositoryPage, ProviderError> {
        const OP: &str = "list account repositories";
        if page == 0 {
            return Err(ProviderError::invalid(OP, "Repository pages start at one"));
        }
        let github = self.grant.provider.is_github();
        let (path, mut params) = match self.grant.provider {
            ScmProvider::GitHub | ScmProvider::GitHubInstance { .. } => {
                (vec!["user", "repos"], vec![("sort", "updated".to_owned())])
            }
            ScmProvider::GitLab | ScmProvider::GitLabInstance { .. } => {
                (vec!["projects"], vec![("membership", "true".to_owned())])
            }
        };
        params.extend([("page", page.to_string()), ("per_page", "100".into())]);
        let response = self
            .http
            .read(
                OP,
                query(append(&self.grant.api, &path), &params),
                &self.grant.token.0,
                github,
            )
            .await?;
        let repositories: Vec<AccountRepository> = match self.grant.provider {
            ScmProvider::GitHub | ScmProvider::GitHubInstance { .. } => response
                .json::<Vec<GithubRepository>>()?
                .into_iter()
                .map(|r| AccountRepository {
                    provider_repo_id: r.id.to_string(),
                    full_name: r.full_name,
                    name: r.name,
                    owner: r.owner.login,
                    default_branch: r.default_branch.unwrap_or_else(|| "main".into()),
                    is_private: r.private,
                    clone_url: r.clone_url,
                    description: r.description,
                    language: r.language,
                    updated_at: r.updated_at,
                })
                .collect(),
            ScmProvider::GitLab | ScmProvider::GitLabInstance { .. } => response
                .json::<Vec<GitlabProject>>()?
                .into_iter()
                .map(|r| AccountRepository {
                    provider_repo_id: r.id.to_string(),
                    full_name: r.path_with_namespace,
                    name: r.name,
                    owner: r.namespace.path,
                    default_branch: r.default_branch.unwrap_or_else(|| "main".into()),
                    is_private: r.visibility == "private",
                    clone_url: r.http_url_to_repo,
                    description: r.description,
                    language: None,
                    updated_at: r.last_activity_at.unwrap_or_default(),
                })
                .collect(),
        };
        let next_page = if repositories.len() == 100 {
            Some(page.checked_add(1).ok_or_else(|| {
                response.failure(
                    ProviderErrorKind::InvalidResponse,
                    "Repository page limit exceeded",
                )
            })?)
        } else {
            None
        };
        Ok(AccountRepositoryPage {
            repositories,
            next_page,
        })
    }
    pub async fn organizations(&self) -> Result<Vec<AccountOrganization>, ProviderError> {
        const OP: &str = "list account organizations";
        let github = self.grant.provider.is_github();
        let (path, params) = match self.grant.provider {
            ScmProvider::GitHub | ScmProvider::GitHubInstance { .. } => {
                (vec!["user", "orgs"], vec![("per_page", "100".into())])
            }
            ScmProvider::GitLab | ScmProvider::GitLabInstance { .. } => (
                vec!["groups"],
                vec![
                    ("per_page", "100".into()),
                    ("min_access_level", "10".into()),
                ],
            ),
        };
        let response = self
            .http
            .read(
                OP,
                query(append(&self.grant.api, &path), &params),
                &self.grant.token.0,
                github,
            )
            .await?;
        #[derive(Deserialize)]
        struct GithubOrg {
            id: i64,
            login: String,
            avatar_url: Option<String>,
        }
        #[derive(Deserialize)]
        struct GitlabGroup {
            id: i64,
            name: String,
            avatar_url: Option<String>,
        }
        Ok(match self.grant.provider {
            ScmProvider::GitHub | ScmProvider::GitHubInstance { .. } => response
                .json::<Vec<GithubOrg>>()?
                .into_iter()
                .map(|o| AccountOrganization {
                    id: o.id.to_string(),
                    name: o.login,
                    avatar_url: o.avatar_url,
                })
                .collect(),
            ScmProvider::GitLab | ScmProvider::GitLabInstance { .. } => response
                .json::<Vec<GitlabGroup>>()?
                .into_iter()
                .map(|o| AccountOrganization {
                    id: o.id.to_string(),
                    name: o.name,
                    avatar_url: o.avatar_url,
                })
                .collect(),
        })
    }
}
#[derive(Deserialize)]
struct GithubRepository {
    id: i64,
    full_name: String,
    name: String,
    owner: GithubRepoOwner,
    default_branch: Option<String>,
    private: bool,
    clone_url: String,
    description: Option<String>,
    language: Option<String>,
    updated_at: String,
}
#[derive(Deserialize)]
struct GithubRepoOwner {
    login: String,
}
#[derive(Deserialize)]
struct GitlabProject {
    id: i64,
    path_with_namespace: String,
    name: String,
    namespace: GitlabNamespace,
    default_branch: Option<String>,
    visibility: String,
    http_url_to_repo: String,
    description: Option<String>,
    last_activity_at: Option<String>,
}
#[derive(Deserialize)]
struct GitlabNamespace {
    path: String,
}
