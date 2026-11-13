//! The readable-workspace audience (`g5.workspace.audience`), shared by every
//! org-wide reader that must apply workspace readability in its own SQL because
//! the underlying tables carry only org-level RLS: pull-request reads
//! (`g5.git.pr-reads`) and the approval activity feed
//! (`g5.governance.approval.activity-feed`). One definition, no copies.

use g5_contracts::G5Error;
use sqlx::PgPool;
use uuid::Uuid;

/// An organization member reading under the workspace audience, as themselves or
/// through their workspace's agent, and whether they read every workspace. The
/// shape the [`readable`]/[`pr_audience`] predicates bind.
#[derive(Clone, Copy, Debug)]
pub struct AudienceReader {
    pub org_id: Uuid,
    pub actor: Uuid,
    pub org_wide: bool,
}

impl AudienceReader {
    /// Resolve the reader's org-wide reach once off the system pool.
    pub async fn resolve(pool: &PgPool, org_id: Uuid, actor: Uuid) -> Result<Self, G5Error> {
        Ok(Self {
            org_id,
            actor,
            org_wide: org_wide_reader(pool, org_id, actor).await?,
        })
    }
}

/// Organization administrators read every live workspace
/// (`g5.workspace.audience`). Resolved off the system pool because org role is
/// not visible under a tenant-scoped connection.
pub(crate) async fn org_wide_reader(
    pool: &PgPool,
    org_id: Uuid,
    actor: Uuid,
) -> Result<bool, G5Error> {
    Ok(sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS (SELECT 1 FROM org_memberships WHERE org_id = $1 AND user_id = $2 \
         AND deactivated_at IS NULL AND role = 'admin')",
    )
    .bind(org_id)
    .bind(actor)
    .fetch_one(pool)
    .await?)
}

/// The readable-workspace audience (`g5.workspace.audience`): the shared fixed
/// audience, with liveness for ordinary workspace reads, as a predicate on
/// workspace alias `w`. Bind parameter `org_wide` says whether the reader is an
/// organization administrator and `reader` is the reader.
/// Organization admission and current active membership are held by the reader.
pub(crate) fn readable(w: &str, org_wide: u8, reader: u8) -> String {
    format!(
        "({w}.deleted_at IS NULL AND {})",
        pr_audience(w, org_wide, reader)
    )
}

/// Who reads a workspace's PRs (`g5.git.pr-reads`): the [`readable`] audience,
/// except that retained PR history keeps the same identity-based audience after
/// deletion (`g5.workspace.delete.pull-requests`).
pub(crate) fn pr_audience(w: &str, org_wide: u8, reader: u8) -> String {
    format!(
        "authz.workspace_audience_allows({w}.personal_user_id, {w}.owner_user_id, \
         {w}.agent_approval_review_id, ${reader}, ${org_wide})"
    )
}
