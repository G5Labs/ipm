//! A person's GitHub App user authorization ("Connect GitHub").
//!
//! Comments and new pull requests a person publishes in G5 use their own
//! identity, never the app's (docs/external-issue-integration-design.md,
//! "Comments from a workspace reviewer"). The person authorizes the G5
//! GitHub App once; G5 keeps the resulting user access token, and its refresh
//! token when GitHub issues expiring tokens, encrypted with the SCM token
//! key. A token is used only for the person who authorized it.
//!
//! These rows are user-owned, not organization data, and are read and
//! written through the application pool.

use chrono::{DateTime, Duration as ChronoDuration, Utc};
use g5_contracts::error::G5Error;
use serde::Serialize;
use sqlx::PgPool;
use uuid::Uuid;

use crate::auth::ScmTokenManager;
use crate::services::git::github_app::{
    connection_state_digest, new_user_connection_state, unix_now, GithubAppClient,
    GithubAuthenticatedUser, GithubTokenRefreshError, GithubUserTokens, UserConnectionAttemptRef,
    CONNECTION_ATTEMPT_TTL,
};

/// GitHub's OAuth error for a refresh token that is revoked, expired or
/// already used.
const GRANT_INVALID: &str = "bad_refresh_token";

/// A user token closer than this to expiry is refreshed before use.
const ACCESS_TOKEN_REFRESH_MARGIN: ChronoDuration = ChronoDuration::minutes(5);
const MAX_RETURN_PATH_CHARS: usize = 2048;

/// Stable marker lets PR recovery surfaces offer the existing App authorization flow.
pub const PR_CONNECTION_REQUIRED: &str = "github_user_connection_required: Connect or reconnect GitHub to create this pull request under your own account, then retry.";

fn db_error(context: &str, error: &sqlx::Error) -> G5Error {
    G5Error::Internal {
        message: format!("github user grant {context}: {error}"),
    }
}

fn app_error(error: impl std::fmt::Display) -> G5Error {
    G5Error::Internal {
        message: format!("github user grant: {error}"),
    }
}

/// A relative G5 path: one leading `/`, not protocol-relative, no backslashes.
#[must_use]
pub fn is_safe_return_path(path: &str) -> bool {
    path.starts_with('/')
        && !path.starts_with("//")
        && !path.contains('\\')
        && !path.chars().any(char::is_control)
        && path.chars().count() <= MAX_RETURN_PATH_CHARS
}

/// Open a "Connect GitHub" attempt and return its signed state.
pub async fn start_connection(
    pool: &PgPool,
    client: &GithubAppClient,
    user_id: Uuid,
    return_path: &str,
) -> Result<String, G5Error> {
    if !is_safe_return_path(return_path) {
        return Err(G5Error::ValidationError {
            details: serde_json::json!({
                "field": "returnPath",
                "error": "must be a relative G5 path",
            }),
        });
    }
    let attempt_id = Uuid::new_v4();
    let ttl = i64::try_from(CONNECTION_ATTEMPT_TTL.as_secs()).unwrap_or(600);
    let expires_at = unix_now() + ttl;
    let state = new_user_connection_state(client.config(), user_id, attempt_id, expires_at);
    sqlx::query(
        r#"
        INSERT INTO github_app_user_connection_attempts
            (id, user_id, state_digest, return_path, expires_at)
        VALUES ($1, $2, $3, $4, to_timestamp($5))
        "#,
    )
    .bind(attempt_id)
    .bind(user_id)
    .bind(connection_state_digest(&state).to_vec())
    .bind(return_path)
    .bind(expires_at)
    .execute(pool)
    .await
    .map_err(|error| db_error("attempt insert", &error))?;
    Ok(state)
}

/// The return path of a live, unconsumed attempt matching `attempt`.
pub async fn pending_attempt_return_path(
    pool: &PgPool,
    attempt: &UserConnectionAttemptRef,
) -> Result<Option<String>, G5Error> {
    sqlx::query_scalar(
        r#"
        SELECT return_path FROM github_app_user_connection_attempts
        WHERE id = $1 AND user_id = $2 AND state_digest = $3
          AND consumed_at IS NULL AND expires_at > now()
        "#,
    )
    .bind(attempt.attempt_id)
    .bind(attempt.user_id)
    .bind(attempt.state_digest.to_vec())
    .fetch_optional(pool)
    .await
    .map_err(|error| db_error("attempt lookup", &error))
}

fn expiry(seconds: Option<i64>) -> Option<DateTime<Utc>> {
    seconds.map(|seconds| Utc::now() + ChronoDuration::seconds(seconds))
}

/// Consume the attempt and store (or replace) the person's grant. Returns
/// the attempt's return path; `None` when the attempt was already used or
/// expired, in which case nothing is stored.
pub async fn complete_connection(
    pool: &PgPool,
    tokens: &ScmTokenManager,
    attempt: &UserConnectionAttemptRef,
    user_tokens: &GithubUserTokens,
    github_user: &GithubAuthenticatedUser,
) -> Result<Option<String>, G5Error> {
    let access = tokens
        .encrypt_token(&user_tokens.access_token)
        .map_err(|error| app_error(error.inner))?;
    let refresh = user_tokens
        .refresh_token
        .as_deref()
        .map(|token| tokens.encrypt_token(token))
        .transpose()
        .map_err(|error| app_error(error.inner))?;
    let mut tx = pool
        .begin()
        .await
        .map_err(|error| db_error("begin", &error))?;
    let return_path: Option<String> = sqlx::query_scalar(
        r#"
        UPDATE github_app_user_connection_attempts
        SET consumed_at = now()
        WHERE id = $1 AND user_id = $2 AND state_digest = $3
          AND consumed_at IS NULL AND expires_at > now()
        RETURNING return_path
        "#,
    )
    .bind(attempt.attempt_id)
    .bind(attempt.user_id)
    .bind(attempt.state_digest.to_vec())
    .fetch_optional(&mut *tx)
    .await
    .map_err(|error| db_error("attempt consume", &error))?;
    let Some(return_path) = return_path else {
        return Ok(None);
    };
    sqlx::query(
        r#"
        INSERT INTO github_app_user_grants
            (user_id, github_user_id, github_login, access_token_enc, access_token_expires_at,
             refresh_token_enc, refresh_token_expires_at)
        VALUES ($1, $2, $3, $4, $5, $6, $7)
        ON CONFLICT (user_id) DO UPDATE
        SET github_user_id = EXCLUDED.github_user_id,
            github_login = EXCLUDED.github_login,
            access_token_enc = EXCLUDED.access_token_enc,
            access_token_expires_at = EXCLUDED.access_token_expires_at,
            refresh_token_enc = EXCLUDED.refresh_token_enc,
            refresh_token_expires_at = EXCLUDED.refresh_token_expires_at,
            updated_at = now()
        "#,
    )
    .bind(attempt.user_id)
    .bind(i64::try_from(github_user.id).map_err(app_error)?)
    .bind(&github_user.login)
    .bind(access)
    .bind(expiry(user_tokens.expires_in))
    .bind(refresh)
    .bind(
        user_tokens
            .refresh_token
            .as_ref()
            .and(expiry(user_tokens.refresh_token_expires_in)),
    )
    .execute(&mut *tx)
    .await
    .map_err(|error| db_error("grant upsert", &error))?;
    tx.commit()
        .await
        .map_err(|error| db_error("commit", &error))?;
    Ok(Some(return_path))
}

/// Whether a person has connected GitHub, and as whom.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UserGrantStatus {
    pub connected: bool,
    pub github_login: Option<String>,
}

pub async fn grant_status(pool: &PgPool, user_id: Uuid) -> Result<UserGrantStatus, G5Error> {
    let login: Option<String> =
        sqlx::query_scalar("SELECT github_login FROM github_app_user_grants WHERE user_id = $1")
            .bind(user_id)
            .fetch_optional(pool)
            .await
            .map_err(|error| db_error("status", &error))?;
    Ok(UserGrantStatus {
        connected: login.is_some(),
        github_login: login,
    })
}

/// Whether `org_id` uses GitHub (`g5.settings.github-connection`): it
/// configures a GitHub issue domain or tracks a repository on a GitHub remote,
/// the two surfaces where this shared connection publishes under the person's
/// own identity. A repository counts under the rule publication uses
/// (`effective_provider`): its clone URL is on the configured GitHub origin
/// and any stored provider agrees, so manually added repositories without a
/// stored provider are recognized from their URL.
///
/// Unlike the grant rows above this reads organization data, so `conn` must be
/// a tenant session whose verified context covers `org_id`.
pub async fn org_uses_github(
    conn: &mut sqlx::PgConnection,
    org_id: Uuid,
    origin_policy: &g5_scm::ScmOriginPolicy,
) -> Result<bool, G5Error> {
    let has_issue_domain: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM issue_domains WHERE org_id = $1 AND provider = 'github')",
    )
    .bind(org_id)
    .fetch_one(&mut *conn)
    .await
    .map_err(|error| db_error("organization issue domains", &error))?;
    if has_issue_domain {
        return Ok(true);
    }
    // Only HTTPS clone URLs can be on a provider origin, which keeps G5-local
    // docs, code and personal repositories out of the candidate rows.
    let repos: Vec<(Option<String>, String)> = sqlx::query_as(
        "SELECT provider, git_url FROM repos
          WHERE org_id = $1
            AND (provider IS NULL OR provider = 'github')
            AND git_url ILIKE 'https://%'",
    )
    .bind(org_id)
    .fetch_all(&mut *conn)
    .await
    .map_err(|error| db_error("organization repositories", &error))?;
    Ok(repos.iter().any(|(provider, git_url)| {
        origin_policy
            .effective_provider(provider.as_deref(), git_url)
            .is_some_and(|provider| provider.is_github())
    }))
}

/// The GitHub logins `user_ids` have connected, by user; a person without a
/// grant is absent. For the agent's member lookup
/// (`g5.organization.agent-member-directory`).
pub async fn connected_logins(
    pool: &PgPool,
    user_ids: &[Uuid],
) -> Result<std::collections::HashMap<Uuid, String>, G5Error> {
    if user_ids.is_empty() {
        return Ok(std::collections::HashMap::new());
    }
    let rows: Vec<(Uuid, String)> = sqlx::query_as(
        "SELECT user_id, github_login FROM github_app_user_grants WHERE user_id = ANY($1)",
    )
    .bind(user_ids)
    .fetch_all(pool)
    .await
    .map_err(|error| db_error("logins", &error))?;
    Ok(rows.into_iter().collect())
}

/// The GitHub account a person connected, if any.
pub async fn connected_account(
    pool: &PgPool,
    user_id: Uuid,
) -> Result<Option<GithubAuthenticatedUser>, G5Error> {
    let row: Option<(i64, String)> = sqlx::query_as(
        "SELECT github_user_id, github_login FROM github_app_user_grants WHERE user_id = $1",
    )
    .bind(user_id)
    .fetch_optional(pool)
    .await
    .map_err(|error| db_error("account", &error))?;
    row.map(|(id, login)| {
        Ok(GithubAuthenticatedUser {
            id: u64::try_from(id).map_err(app_error)?,
            login,
        })
    })
    .transpose()
}

/// Forget a person's grant. GitHub keeps the authorization until the person
/// revokes it there; G5 stops using it immediately.
pub async fn delete_grant(pool: &PgPool, user_id: Uuid) -> Result<(), G5Error> {
    sqlx::query("DELETE FROM github_app_user_grants WHERE user_id = $1")
        .bind(user_id)
        .execute(pool)
        .await
        .map_err(|error| db_error("delete", &error))?;
    Ok(())
}

/// A usable user access token and the GitHub account it acts for.
#[derive(Debug, Clone)]
pub struct ActingGithubUser {
    pub token: String,
    pub github_user_id: u64,
    pub github_login: String,
}

/// Why no user token is available.
#[derive(Debug)]
pub enum UserTokenError {
    /// The person has not connected GitHub, or the grant can no longer be
    /// renewed; they must connect again.
    ConnectionRequired,
    Failed(G5Error),
}

#[derive(sqlx::FromRow)]
struct GrantRow {
    github_user_id: i64,
    github_login: String,
    access_token_enc: Vec<u8>,
    access_token_expires_at: Option<DateTime<Utc>>,
    refresh_token_enc: Option<Vec<u8>>,
    refresh_token_expires_at: Option<DateTime<Utc>>,
}

/// The GitHub login shown with a person's G5-posted comment: their GitHub
/// App connection's, else their personal GitHub connection's, else empty.
pub async fn display_login(pool: &PgPool, user_id: Uuid) -> Result<String, G5Error> {
    sqlx::query_scalar(
        "SELECT COALESCE( \
           (SELECT github_login FROM github_app_user_grants WHERE user_id = $1), \
           (SELECT provider_username FROM user_scm_connections \
             WHERE user_id = $1 AND provider = 'github' ORDER BY updated_at DESC LIMIT 1), \
           '')",
    )
    .bind(user_id)
    .fetch_one(pool)
    .await
    .map_err(|error| db_error("display login", &error))
}

/// The person's access token, renewed first when it is about to expire. The
/// grant row is locked while renewing because GitHub rotates the refresh
/// token: a concurrent renewal with the old one would fail.
pub async fn acting_user(
    pool: &PgPool,
    tokens: &ScmTokenManager,
    client: &GithubAppClient,
    user_id: Uuid,
) -> Result<ActingGithubUser, UserTokenError> {
    let failed =
        |context: &str, error: &sqlx::Error| UserTokenError::Failed(db_error(context, error));
    let mut tx = pool
        .begin()
        .await
        .map_err(|error| failed("begin", &error))?;
    let row: Option<GrantRow> = sqlx::query_as(
        r#"
        SELECT github_user_id, github_login, access_token_enc, access_token_expires_at,
               refresh_token_enc, refresh_token_expires_at
        FROM github_app_user_grants WHERE user_id = $1
        FOR UPDATE
        "#,
    )
    .bind(user_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|error| failed("load", &error))?;
    let Some(row) = row else {
        return Err(UserTokenError::ConnectionRequired);
    };
    let github_user_id = u64::try_from(row.github_user_id)
        .map_err(|error| UserTokenError::Failed(app_error(error)))?;
    let now = Utc::now();
    let fresh = row
        .access_token_expires_at
        .is_none_or(|expires_at| expires_at > now + ACCESS_TOKEN_REFRESH_MARGIN);
    if fresh {
        let token = tokens
            .decrypt_token(&row.access_token_enc)
            .map_err(|error| UserTokenError::Failed(app_error(error.inner)))?;
        return Ok(ActingGithubUser {
            token,
            github_user_id,
            github_login: row.github_login,
        });
    }
    let renewable = row.refresh_token_enc.as_ref().filter(|_| {
        row.refresh_token_expires_at
            .is_none_or(|expires_at| expires_at > now)
    });
    let Some(refresh_enc) = renewable else {
        return Err(UserTokenError::ConnectionRequired);
    };
    let refresh = tokens
        .decrypt_token(refresh_enc)
        .map_err(|error| UserTokenError::Failed(app_error(error.inner)))?;
    let renewed = match client.refresh_user_token(&refresh).await {
        Ok(renewed) => renewed,
        // A revoked or superseded grant: only a new authorization helps.
        Err(GithubTokenRefreshError::Rejected { code })
            if code.as_deref() == Some(GRANT_INVALID) =>
        {
            return Err(UserTokenError::ConnectionRequired);
        }
        // Anything else (such as the app's client credentials) is not the
        // person's to fix by reconnecting.
        Err(GithubTokenRefreshError::Rejected { code }) => {
            return Err(UserTokenError::Failed(G5Error::ServiceUnavailable {
                message: format!(
                    "GitHub refused to renew the connection ({})",
                    code.as_deref().unwrap_or("no error code")
                ),
            }));
        }
        Err(GithubTokenRefreshError::Failed(error)) => return Err(UserTokenError::Failed(error)),
    };
    let access = tokens
        .encrypt_token(&renewed.access_token)
        .map_err(|error| UserTokenError::Failed(app_error(error.inner)))?;
    let refresh = renewed
        .refresh_token
        .as_deref()
        .map(|token| tokens.encrypt_token(token))
        .transpose()
        .map_err(|error| UserTokenError::Failed(app_error(error.inner)))?;
    sqlx::query(
        r#"
        UPDATE github_app_user_grants
        SET access_token_enc = $2, access_token_expires_at = $3,
            refresh_token_enc = $4, refresh_token_expires_at = $5, updated_at = now()
        WHERE user_id = $1
        "#,
    )
    .bind(user_id)
    .bind(access)
    .bind(expiry(renewed.expires_in))
    .bind(refresh)
    .bind(
        renewed
            .refresh_token
            .as_ref()
            .and(expiry(renewed.refresh_token_expires_in)),
    )
    .execute(&mut *tx)
    .await
    .map_err(|error| failed("renew", &error))?;
    tx.commit()
        .await
        .map_err(|error| failed("commit", &error))?;
    Ok(ActingGithubUser {
        token: renewed.access_token,
        github_user_id,
        github_login: row.github_login,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn return_paths_stay_inside_g5() {
        for path in ["/acme/web?view=activity", "/acme/web", "/"] {
            assert!(is_safe_return_path(path), "{path}");
        }
        for path in [
            "",
            "acme/web",
            "//evil.example/x",
            "https://evil.example",
            "/\\evil.example",
            "/a\nb",
        ] {
            assert!(!is_safe_return_path(path), "{path:?}");
        }
    }
}
