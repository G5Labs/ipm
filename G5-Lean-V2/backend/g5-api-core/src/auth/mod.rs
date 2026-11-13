//! Identity login (GitHub, Microsoft Entra ID), personal token storage, and repository-scoped provider credentials.

pub mod entra;
mod git_auth_resolver;
pub mod host_validator;
mod oauth;
pub mod oauth_attempt;
mod repository_credentials;
pub mod return_path;
mod scm;
pub use repository_credentials::{RepositoryCredentials, RepositoryOperation};

pub use git_auth_resolver::effective_provider;
pub use host_validator::{HostAuthError, HostAuthValidator, PgHostAuthValidator};

pub use oauth::{
    build_authorize_url, generate_oauth_state, OAuthProvider, OAuthProviderConfig, OAuthRedirect,
    OAuthTokenResponse, OAuthUserInfo,
};

pub use scm::{configured_scm_origin_policy, ScmTokenManager, TokenResolutionError};
