//! Repository-wide network push protection. Git refs remain the head authority.

use std::collections::BTreeSet;

use sqlx::PgConnection;
use uuid::Uuid;

/// Return branch names, not full refs, on a connection scoped to the repo's org.
/// RLS is tenant-scoped; branch protection is independent of read access.
/// The default and base branches cannot be unprotected by editing the array.
pub async fn protected_branches_for_repo(
    conn: &mut PgConnection,
    repo_id: Uuid,
) -> Result<BTreeSet<String>, sqlx::Error> {
    let branches: Vec<String> = sqlx::query_scalar(
        r#"
        SELECT default_branch FROM repos WHERE id = $1
        UNION
        SELECT unnest(protected_branches) FROM repos WHERE id = $1
        "#,
    )
    .bind(repo_id)
    .fetch_all(conn)
    .await?;
    // Every visible repo has a non-null default branch. An empty result is an
    // absent/inaccessible repo, never an unprotected repository.
    if branches.is_empty() {
        return Err(sqlx::Error::RowNotFound);
    }
    Ok(branches.into_iter().collect())
}

/// The deployment-only local fixture escape hatch cannot authorize a network
/// destination or an external helper transport.
pub(crate) fn is_local_git_remote(url: &str) -> bool {
    (std::path::Path::new(url).is_absolute() && !url.contains(':'))
        || url::Url::parse(url).is_ok_and(|u| u.scheme() == "file" && u.to_file_path().is_ok())
}

/// Identity classification is independent of whether a URL can receive a token.
pub(crate) fn is_local_repo_identity(provider: Option<&str>, git_url: &str) -> bool {
    provider.is_none() && (git_url.starts_with("g5-local://") || is_local_git_remote(git_url))
}
