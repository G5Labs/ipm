//! Repository-scoped credentials for provider operations. Callers choose the
//! operation, never installation IDs, token caches, binding SQL, or a fallback.

use super::ScmTokenManager;
use crate::services::{
    github_app::{GithubAppClient, GithubUserRepositoryAccessError},
    github_app_binding as app, github_user_grants,
};
use g5_authz::{AuthzService, ScopedDb};
use g5_contracts::{
    types::ids::{OrgId, UserId},
    G5Error,
};
use g5_scm::GitAuth;
use sqlx::PgPool;
use std::sync::Arc;
use uuid::Uuid;

#[derive(Clone, Copy, Debug)]
pub enum RepositoryOperation {
    ReadGit,
    WriteGit,
    /// Implicit publication may use only an organization GitHub App installation.
    AutoPush,
    ReadPullRequests,
    ReadPullRequestChecks,
    WritePullRequests,
    /// New PRs are attributed to the initiating user's GitHub account.
    CreatePullRequest,
    /// PR comments, review-thread replies and resolution, as the acting
    /// user's GitHub account.
    PullRequestConversation,
    MergePullRequest,
    /// A merge past GitHub's rules, as the acting person's own GitHub account
    /// so GitHub decides and records their bypass (g5.git.provider-override).
    MergePullRequestWithBypass,
}

impl RepositoryOperation {
    fn capability(self) -> app::GithubAppCapability {
        match self {
            Self::ReadGit => app::GithubAppCapability::ContentsRead,
            Self::WriteGit
            | Self::AutoPush
            | Self::MergePullRequest
            | Self::MergePullRequestWithBypass => app::GithubAppCapability::ContentsWrite,
            Self::ReadPullRequests => app::GithubAppCapability::PullRequestsRead,
            Self::ReadPullRequestChecks => app::GithubAppCapability::PullRequestChecksRead,
            Self::WritePullRequests | Self::CreatePullRequest | Self::PullRequestConversation => {
                app::GithubAppCapability::PullRequestsWrite
            }
        }
    }
    /// Provider writes authored by the acting user, never the installation.
    fn user_authored(self) -> bool {
        matches!(
            self,
            Self::CreatePullRequest
                | Self::PullRequestConversation
                | Self::MergePullRequestWithBypass
        )
    }
}

#[derive(Clone)]
pub struct RepositoryCredentials {
    system_pool: PgPool,
    tenant_pool: PgPool,
    authz: Arc<dyn AuthzService>,
    personal: Option<Arc<ScmTokenManager>>,
    github: Option<Arc<GithubAppClient>>,
    origin_policy: g5_scm::ScmOriginPolicy,
}

impl RepositoryCredentials {
    pub fn new(
        system_pool: PgPool,
        tenant_pool: PgPool,
        authz: Arc<dyn AuthzService>,
        personal: Option<Arc<ScmTokenManager>>,
        github: Option<Arc<GithubAppClient>>,
    ) -> Self {
        Self {
            system_pool,
            tenant_pool,
            authz,
            origin_policy: personal
                .as_ref()
                .map(|manager| manager.origin_policy().clone())
                .unwrap_or_default(),
            personal,
            github,
        }
    }

    pub fn with_origin_policy(mut self, policy: g5_scm::ScmOriginPolicy) -> Self {
        self.origin_policy = policy;
        self
    }

    pub fn origin_policy(&self) -> &g5_scm::ScmOriginPolicy {
        &self.origin_policy
    }

    /// The caller must authorize the workspace action first. Revalidate the
    /// actor's organization membership and captured repository URL here even
    /// for App tokens: installation access never grants a G5 user access.
    /// No tenant transaction is retained while minting a token.
    pub async fn resolve(
        &self,
        org_id: Uuid,
        repo_id: Uuid,
        actor: Uuid,
        expected_url: &str,
        operation: RepositoryOperation,
    ) -> Result<GitAuth, G5Error> {
        self.resolve_target(org_id, repo_id, actor, expected_url, None, operation, None)
            .await
    }

    /// Credentials for an explicitly authorized Make Remote proposal. The
    /// persisted identity must still match, remain local, and the actor must
    /// still be an organization administrator. Adoption itself is finalized
    /// separately with the repository identity CAS.
    #[allow(clippy::too_many_arguments)]
    pub async fn resolve_adoption(
        &self,
        org_id: Uuid,
        repo_id: Uuid,
        actor: Uuid,
        expected_url: &str,
        proposed_url: &str,
        operation: RepositoryOperation,
    ) -> Result<GitAuth, G5Error> {
        self.resolve_target(
            org_id,
            repo_id,
            actor,
            expected_url,
            Some(proposed_url),
            operation,
            None,
        )
        .await
    }

    /// Reuse a caller's tenant transaction when it already holds publication
    /// locks. All credential policy stays here; no nested tenant lease is needed.
    pub async fn resolve_in_scope(
        &self,
        db: &mut ScopedDb,
        org_id: Uuid,
        repo_id: Uuid,
        actor: Uuid,
        expected_url: &str,
        operation: RepositoryOperation,
    ) -> Result<GitAuth, G5Error> {
        if db
            .primary_org_id()
            .map_err(|_| denied("credential_scope_mismatch"))?
            .into_uuid()
            != org_id
            || db.actor_id().into_uuid() != actor
        {
            return Err(denied("credential_scope_mismatch"));
        }
        self.resolve_target(
            org_id,
            repo_id,
            actor,
            expected_url,
            None,
            operation,
            Some(db),
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    async fn resolve_target(
        &self,
        org_id: Uuid,
        repo_id: Uuid,
        actor: Uuid,
        expected_url: &str,
        proposed_url: Option<&str>,
        operation: RepositoryOperation,
        caller_db: Option<&mut ScopedDb>,
    ) -> Result<GitAuth, G5Error> {
        let member: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM org_memberships WHERE org_id = $1 AND user_id = $2 AND deactivated_at IS NULL)",
        )
        .bind(org_id)
        .bind(actor)
        .fetch_one(&self.system_pool)
        .await?;
        if !member {
            return Err(denied("owner_not_org_member"));
        }
        let mut owned_db = None;
        let db = if let Some(db) = caller_db {
            db
        } else {
            let capability = self
                .authz
                .mint_service_db_capability(
                    UserId::from_uuid(actor),
                    OrgId::from_uuid(org_id),
                    &["*".into()],
                )
                .await
                .map_err(|_| denied("credential_scope_unavailable"))?;
            owned_db = Some(
                ScopedDb::from_capability(&self.tenant_pool, capability)
                    .await
                    .map_err(|_| denied("credential_scope_unavailable"))?,
            );
            owned_db.as_mut().expect("new credential scope")
        };
        let (url, provider): (String, Option<String>) =
            sqlx::query_as("SELECT git_url, provider FROM repos WHERE id = $1 AND org_id = $2")
                .bind(repo_id)
                .bind(org_id)
                .fetch_one(db.conn())
                .await?;
        if url != expected_url {
            return Err(denied("repository_changed"));
        }
        let (url, provider) = if let Some(proposed) = proposed_url {
            let admin: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM org_memberships WHERE org_id=$1 AND user_id=$2 AND deactivated_at IS NULL AND role = 'admin')")
                .bind(org_id).bind(actor).fetch_one(&self.system_pool).await?;
            if !admin
                || !crate::services::git_policy::is_local_repo_identity(provider.as_deref(), &url)
            {
                return Err(denied("remote_adoption_not_authorized"));
            }
            (
                proposed.to_owned(),
                self.origin_policy.effective_provider(None, proposed),
            )
        } else {
            let provider = self
                .origin_policy
                .effective_provider(provider.as_deref(), &url);
            (url, provider)
        };
        if let Some(transport_provider) = provider.as_ref().filter(|p| p.is_github()) {
            // Exact transport identity, never a host substring or an unchecked
            // provider_repo_id (older imports stored owner/name in that field).
            if let Some(binding) = app::load_latest_binding(db, org_id).await? {
                let client = self
                    .github
                    .as_deref()
                    .ok_or_else(|| denied("github_app_unavailable"))?;
                let full_name = client
                    .endpoints()?
                    .repository_name_from_clone_url(&url)
                    .ok_or_else(|| denied("invalid_github_repository_url"))?;

                if binding.disposition != app::DISPOSITION_ACTIVE {
                    return Err(denied("github_app_binding_inactive"));
                }
                let coverage = app::load_binding_coverage(db, org_id, binding.id).await?;
                let repository = coverage
                    .iter()
                    .find(|repo| repo.full_name.eq_ignore_ascii_case(&full_name))
                    .ok_or_else(|| denied("github_app_repository_uncovered"))?;
                let id = u64::try_from(repository.github_repo_id)
                    .map_err(|_| denied("github_app_repository_uncovered"))?;
                let client = self
                    .github
                    .as_deref()
                    .ok_or_else(|| denied("github_app_unavailable"))?;
                if operation.user_authored() {
                    let selection =
                        app::lookup_repository_selection(db, org_id, id, operation.capability())
                            .await?;
                    if let Some(owned) = owned_db.take() {
                        owned
                            .commit()
                            .await
                            .map_err(|_| denied("credential_scope_unavailable"))?;
                    }
                    if !matches!(selection, app::GithubAppSelectionPreflight::Eligible(_)) {
                        return Err(denied("github_app_permission_denied"));
                    }
                    let tokens = self
                        .personal
                        .as_deref()
                        .ok_or_else(|| denied("no_scm_token_manager"))?;
                    let user =
                        github_user_grants::acting_user(&self.system_pool, tokens, client, actor)
                            .await
                            .map_err(|error| match error {
                                github_user_grants::UserTokenError::ConnectionRequired => {
                                    denied(github_user_grants::PR_CONNECTION_REQUIRED)
                                }
                                github_user_grants::UserTokenError::Failed(error) => error,
                            })?;
                    client
                        .verify_user_repository_access(&user.token, &full_name, id)
                        .await
                        .map_err(|error| match error {
                            GithubUserRepositoryAccessError::ConnectionRequired => {
                                denied(github_user_grants::PR_CONNECTION_REQUIRED)
                            }
                            GithubUserRepositoryAccessError::AccessDenied => denied(
                                "Your connected GitHub account cannot access this repository. Check your repository access and GitHub App permissions, then retry.",
                            ),
                            GithubUserRepositoryAccessError::RepositoryChanged => denied(
                                "GitHub repository identity no longer matches the App binding. Refresh the organization's GitHub App settings and retry.",
                            ),
                            GithubUserRepositoryAccessError::Failed(error) => error,
                        })?;
                    return Ok(GitAuth::scm_token_for_provider(
                        user.token,
                        transport_provider.clone(),
                        &url,
                    ));
                }
                let selection =
                    app::lookup_repository_selection(db, org_id, id, operation.capability())
                        .await?;
                if let Some(owned) = owned_db.take() {
                    owned
                        .commit()
                        .await
                        .map_err(|_| denied("credential_scope_unavailable"))?;
                }
                let app::GithubAppSelectionPreflight::Eligible(selection) = selection else {
                    return Err(denied("github_app_permission_denied"));
                };
                return app::mint_repository_token(client, &selection)
                    .await
                    .map(|token| {
                        GitAuth::scm_token_for_provider(
                            token.token,
                            transport_provider.clone(),
                            &url,
                        )
                    });
            }
        }
        if matches!(operation, RepositoryOperation::AutoPush) {
            return Err(denied(
                "Automatic push requires a GitHub App with repository write access. Use Push to Remote to publish explicitly with your personal connection.",
            ));
        }
        if let Some(owned) = owned_db.take() {
            owned
                .commit()
                .await
                .map_err(|_| denied("credential_scope_unavailable"))?;
        }
        let Some(provider) = provider else {
            return Ok(GitAuth::None);
        };
        self.personal
            .as_deref()
            .ok_or_else(|| denied("no_scm_token_manager"))?
            .token_for_user(&self.system_pool, actor, org_id, provider.name(), &url)
            .await
            .map_err(|error| match error {
                super::TokenResolutionError::OwnerNotOrgMember { .. } => {
                    denied("owner_not_org_member")
                }
                super::TokenResolutionError::NoScmConnection { .. } => denied("no_scm_connection"),
                super::TokenResolutionError::DecryptFailed(_) => denied("decrypt_failed"),
                super::TokenResolutionError::TokenExpired { .. } => denied("scm_token_expired"),
                super::TokenResolutionError::Database(error) => G5Error::from(error),
            })
    }
}

fn denied(reason: &str) -> G5Error {
    G5Error::Conflict {
        reason: format!("Repository credentials: {reason}"),
    }
}
