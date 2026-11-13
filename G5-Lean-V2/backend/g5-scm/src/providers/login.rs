//! OAuth identity uses its owner's exact endpoints, independently of repository
//! integration endpoints. Verified-email and identity-linking policy stay above SCM.
use super::*;
use serde::Deserialize;

#[derive(Clone)]
pub struct LoginConfig {
    provider: ScmProvider,
    token_url: Url,
    userinfo_url: Url,
    client_id: String,
    client_secret: Secret,
    redirect_uri: String,
}
impl std::fmt::Debug for LoginConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LoginConfig")
            .field("provider", &self.provider)
            .finish_non_exhaustive()
    }
}
impl LoginConfig {
    pub fn new(
        provider: ScmProvider,
        token_url: &str,
        userinfo_url: &str,
        client_id: String,
        selected_client_secret: String,
        redirect_uri: String,
    ) -> Result<Self, ProviderError> {
        Ok(Self {
            provider,
            token_url: endpoint(token_url, true)?,
            userinfo_url: endpoint(userinfo_url, true)?,
            client_id,
            client_secret: Secret(selected_client_secret),
            redirect_uri,
        })
    }
    pub fn github_app(
        endpoints: &ProviderEndpoints,
        client_id: String,
        selected_client_secret: String,
        redirect_uri: String,
    ) -> Result<Self, ProviderError> {
        endpoints.github("configure App OAuth")?;
        Ok(Self {
            provider: ScmProvider::GitHub,
            token_url: endpoints.web(&["login", "oauth", "access_token"]),
            userinfo_url: endpoints.api(&["user"]),
            client_id,
            client_secret: Secret(selected_client_secret),
            redirect_uri,
        })
    }
}
pub struct LoginClient {
    pub(super) http: GithubHttp,
    pub(super) config: LoginConfig,
}

/// Secret-bearing normalized result. Debug never reveals token values.
#[derive(Clone, PartialEq, Eq)]
pub struct OAuthTokens {
    pub access_token: String,
    pub expires_in: Option<i64>,
    pub refresh_token: Option<String>,
    pub refresh_token_expires_in: Option<i64>,
}
pub type GithubUserTokens = OAuthTokens;
impl std::fmt::Debug for OAuthTokens {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OAuthTokens")
            .field("access_token", &"[REDACTED]")
            .field("refresh_token", &"[REDACTED]")
            .field("expires_in", &self.expires_in)
            .field("refresh_token_expires_in", &self.refresh_token_expires_in)
            .finish()
    }
}
#[derive(Debug)]
pub enum TokenRefreshError {
    Rejected { code: Option<String> },
    Failed(ProviderError),
}
#[derive(Deserialize)]
struct TokenResponse {
    access_token: Option<String>,
    expires_in: Option<i64>,
    refresh_token: Option<String>,
    refresh_token_expires_in: Option<i64>,
    error: Option<String>,
}
impl TokenResponse {
    fn tokens(self) -> Option<OAuthTokens> {
        Some(OAuthTokens {
            access_token: self.access_token.filter(|v| !v.trim().is_empty())?,
            expires_in: self.expires_in,
            refresh_token: self.refresh_token.filter(|v| !v.trim().is_empty()),
            refresh_token_expires_in: self.refresh_token_expires_in,
        })
    }
}
impl LoginClient {
    async fn token_request(
        &self,
        operation: &'static str,
        fields: &[(&str, &str)],
        secrets: &[&str],
    ) -> Result<Response, ProviderError> {
        self.http
            .send(Request {
                operation,
                target: self.config.token_url.host_str(),
                url: self.config.token_url.clone(),
                method: reqwest::Method::POST,
                token: None,
                secrets,
                payload: Payload::Form(fields),
                timeout: Duration::from_secs(20),
                limit: MAX_RESPONSE,
                github: false,
                media: None,
            })
            .await
    }
    pub async fn exchange_code(
        &self,
        code: &str,
        verifier: Option<&str>,
    ) -> Result<OAuthTokens, ProviderError> {
        const OP: &str = "exchange OAuth code";
        if code.trim().is_empty() || verifier.is_some_and(|v| v.trim().is_empty()) {
            return Err(ProviderError::invalid(
                OP,
                "The OAuth callback was incomplete",
            ));
        }
        let mut fields = vec![
            ("client_id", self.config.client_id.as_str()),
            ("client_secret", self.config.client_secret.0.as_str()),
            ("code", code),
            ("redirect_uri", self.config.redirect_uri.as_str()),
            ("grant_type", "authorization_code"),
        ];
        if let Some(verifier) = verifier {
            fields.push(("code_verifier", verifier));
        }
        let response = self
            .token_request(
                OP,
                &fields,
                &[
                    &self.config.client_secret.0,
                    code,
                    verifier.unwrap_or_default(),
                ],
            )
            .await?;
        let parsed: TokenResponse = response.json()?;
        if parsed
            .access_token
            .as_deref()
            .is_none_or(|token| token.trim().is_empty())
            && parsed.error.is_some()
        {
            let mut error = response.failure(
                ProviderErrorKind::Rejected,
                "The provider refused the OAuth code exchange",
            );
            error.effect = crate::FailureEffect::Refused;
            return Err(error);
        }
        parsed.tokens().ok_or_else(|| {
            response.failure(
                ProviderErrorKind::InvalidResponse,
                "The provider returned an incomplete OAuth response",
            )
        })
    }
    pub async fn refresh_token(
        &self,
        selected_refresh_token: &str,
    ) -> Result<OAuthTokens, TokenRefreshError> {
        let response = self
            .token_request(
                "refresh OAuth token",
                &[
                    ("client_id", &self.config.client_id),
                    ("client_secret", &self.config.client_secret.0),
                    ("grant_type", "refresh_token"),
                    ("refresh_token", selected_refresh_token),
                ],
                &[&self.config.client_secret.0, selected_refresh_token],
            )
            .await
            .map_err(TokenRefreshError::Failed)?;
        let parsed: TokenResponse = response.json().map_err(TokenRefreshError::Failed)?;
        if parsed
            .access_token
            .as_deref()
            .is_none_or(|v| v.trim().is_empty())
        {
            return Err(TokenRefreshError::Rejected {
                code: parsed.error.map(|v| {
                    transport::redact(
                        &v,
                        &[
                            self.config.client_secret.0.clone(),
                            selected_refresh_token.to_owned(),
                        ],
                    )
                }),
            });
        }
        parsed.tokens().ok_or_else(|| {
            TokenRefreshError::Failed(response.failure(
                ProviderErrorKind::InvalidResponse,
                "The provider returned invalid OAuth tokens",
            ))
        })
    }
    pub async fn user_info(&self, selected_token: &str) -> Result<LoginUser, ProviderError> {
        self.identity("read login identity", selected_token).await
    }
    async fn identity(
        &self,
        operation: &'static str,
        selected_token: &str,
    ) -> Result<LoginUser, ProviderError> {
        let github = self.config.provider.is_github();
        let response = self
            .http
            .read(
                operation,
                self.config.userinfo_url.clone(),
                selected_token,
                github,
            )
            .await?;
        match self.config.provider {
            ScmProvider::GitHub | ScmProvider::GitHubInstance { .. } => {
                response.json().map(LoginUser::GitHub)
            }
            ScmProvider::GitLab | ScmProvider::GitLabInstance { .. } => {
                response.json().map(LoginUser::GitLab)
            }
        }
    }
    pub async fn github_emails(
        &self,
        selected_token: &str,
    ) -> Result<Vec<GithubEmail>, ProviderError> {
        if !self.config.provider.is_github() {
            return Err(ProviderError::unsupported("read login email evidence"));
        }
        self.http
            .read(
                "read login email evidence",
                append(&self.config.userinfo_url, &["emails"]),
                selected_token,
                true,
            )
            .await?
            .json()
    }
}
#[derive(Debug)]
pub enum LoginUser {
    GitHub(GithubLoginUser),
    GitLab(GitlabLoginUser),
}
#[derive(Debug, Deserialize)]
pub struct GithubLoginUser {
    pub id: i64,
    pub login: String,
    pub email: Option<String>,
    pub name: Option<String>,
    pub avatar_url: Option<String>,
}
#[derive(Debug, Deserialize)]
pub struct GitlabLoginUser {
    pub id: i64,
    pub username: String,
    pub email: Option<String>,
    pub name: Option<String>,
    pub avatar_url: Option<String>,
}
#[derive(Debug, Deserialize)]
pub struct GithubEmail {
    pub email: String,
    pub primary: bool,
    pub verified: bool,
}

/// SCM/App OAuth has a distinct authority from platform login. Its owner supplies
/// exact endpoints and selected client credentials; it cannot read login emails.
#[derive(Clone, Debug)]
pub struct ConnectionOAuthConfig(LoginConfig);
impl ConnectionOAuthConfig {
    pub fn new(
        provider: ScmProvider,
        token_url: &str,
        userinfo_url: &str,
        client_id: String,
        selected_client_secret: String,
        redirect_uri: String,
    ) -> Result<Self, ProviderError> {
        LoginConfig::new(
            provider,
            token_url,
            userinfo_url,
            client_id,
            selected_client_secret,
            redirect_uri,
        )
        .map(Self)
    }
    pub fn github_app(
        endpoints: &ProviderEndpoints,
        client_id: String,
        selected_client_secret: String,
        redirect_uri: String,
    ) -> Result<Self, ProviderError> {
        LoginConfig::github_app(endpoints, client_id, selected_client_secret, redirect_uri)
            .map(Self)
    }
}
pub struct ConnectionOAuthClient {
    protocol: LoginClient,
}
impl ProviderClients {
    pub fn connection_oauth(&self, config: ConnectionOAuthConfig) -> ConnectionOAuthClient {
        ConnectionOAuthClient {
            protocol: LoginClient {
                http: self.http.clone(),
                config: config.0,
            },
        }
    }
}
impl ConnectionOAuthClient {
    pub async fn exchange_code(
        &self,
        code: &str,
        verifier: Option<&str>,
    ) -> Result<OAuthTokens, ProviderError> {
        self.protocol.exchange_code(code, verifier).await
    }
    pub async fn refresh_token(
        &self,
        selected_refresh_token: &str,
    ) -> Result<OAuthTokens, TokenRefreshError> {
        self.protocol.refresh_token(selected_refresh_token).await
    }
    pub async fn user_info(&self, selected_token: &str) -> Result<LoginUser, ProviderError> {
        self.protocol
            .identity("read SCM connection identity", selected_token)
            .await
    }
}
