//! Workspace names: the organization-wide claim rules shared by creation
//! (fork) and rename (`g5.workspace.identity`, `g5.workspace.rename`,
//! `g5.workspace.former-names`).
//!
//! A name is held by a live workspace, by a workspace whose deletion has not
//! completed (its repository membership is still present), as a former name
//! of such a workspace, or by a pending fork claim. Holders are compared
//! case-insensitively, as the resolver matches names. The branch a workspace
//! owns is a separate, immutable fact (`workspaces.branch_name`) and is not
//! consulted here.

use sqlx::PgConnection;
use uuid::Uuid;

use crate::errors::G5Error;

/// Workspace names reserved because they collide with org-level client route
/// segments. The workspace URL segment is the raw workspace name
/// (`/:org/<name>`), so a workspace named e.g. "settings" would be shadowed by
/// the static `/:org/settings` route (which ranks above `/:org/:workspace`).
/// Reserving the org-level segments keeps `:workspace` from ever shadowing an
/// org route, now or if a segment is promoted to a direct `/:org/<segment>`
/// route. Comparison is case-insensitive against the trimmed name.
/// See `g5/client/settings-page`, organization Home route D51 and the
/// organization fork picker at `/:org/fork` (workbench g5.workbench.tabs).
/// `me` and `users` are the personal-workspace routes (`g5.personal.routes`).
const RESERVED_WORKSPACE_NAMES: &[&str] = &[
    "settings", "members", "workers", "home", "fork", "me", "users",
];

/// Names under this prefix belong to personal workspaces, which the system
/// names itself (`g5.personal.names`).
pub const PERSONAL_WORKSPACE_NAME_PREFIX: &str = "users/";

/// Returns true when `name` (trimmed, case-insensitive) is a reserved workspace
/// name (see [`RESERVED_WORKSPACE_NAMES`]) or lies under `users/`. Shared so
/// every ordinary workspace-creation path (create, init, rename, clone)
/// enforces the same guard; only the personal creator chooses `users/` names.
#[must_use]
pub fn is_reserved_workspace_name(name: &str) -> bool {
    let normalized = name.trim().to_lowercase();
    RESERVED_WORKSPACE_NAMES.contains(&normalized.as_str())
        || normalized.starts_with(PERSONAL_WORKSPACE_NAME_PREFIX)
}

/// Build the validation error returned when a reserved workspace name is used.
pub fn reserved_workspace_name_error(name: &str) -> G5Error {
    G5Error::ValidationError {
        details: serde_json::json!({
            "field": "name",
            "error": format!(
                "'{}' is a reserved name and cannot be used for a workspace",
                name.trim()
            )
        }),
    }
}

/// Longest workspace name accepted (`g5_git_branch_name_is_valid`).
pub const MAX_WORKSPACE_NAME_BYTES: usize = 255;

/// A workspace name is an exact Git branch name (the branch a new workspace
/// owns is derived from it) and must not be a reserved route segment.
pub fn check_workspace_name(name: &str) -> Result<(), G5Error> {
    if name.len() > MAX_WORKSPACE_NAME_BYTES || name == "@" || !g5_scm::is_valid_ref_name(name) {
        return Err(G5Error::ValidationError {
            details: serde_json::json!({
                "field": "name",
                "message": "Workspace name must be an exact valid Git branch name",
            }),
        });
    }
    if is_reserved_workspace_name(name) {
        return Err(reserved_workspace_name_error(name));
    }
    Ok(())
}

/// Serialize every name claim in an organization for the rest of the
/// transaction: a rename, a fork's availability check and claim. Rename and
/// fork claims both take this lock before checking, so a check never races a
/// commit it did not see.
pub async fn lock_org_workspace_names(
    conn: &mut PgConnection,
    org_id: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "SELECT pg_advisory_xact_lock(hashtextextended('g5:workspace-name:' || $1::text, 0))",
    )
    .bind(org_id)
    .execute(conn)
    .await
    .map(|_| ())
}

/// Who holds a name in the organization.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NameHolder {
    /// The current name of a live workspace, or of one whose deletion has not
    /// completed.
    Current,
    /// A former name reserved by such a workspace.
    Former,
    /// The proposed name of a pending fork.
    ForkClaim,
}

impl NameHolder {
    fn parse(kind: &str) -> Option<Self> {
        match kind {
            "current" => Some(Self::Current),
            "former" => Some(Self::Former),
            "fork_claim" => Some(Self::ForkClaim),
            _ => None,
        }
    }

    /// The conflict a caller reports when the name is taken.
    pub fn conflict(self, name: &str) -> G5Error {
        let holder = match self {
            Self::Current => "another workspace",
            Self::Former => "a former name of another workspace",
            Self::ForkClaim => "a workspace being forked",
        };
        G5Error::Conflict {
            reason: format!("The name '{name}' is {holder} in this organization"),
        }
    }
}

/// The holder of `name` in `org_id`, ignoring `exclude_workspace` itself (a
/// workspace may take one of its own former names back).
pub async fn name_holder(
    conn: &mut PgConnection,
    org_id: Uuid,
    name: &str,
    exclude_workspace: Option<Uuid>,
) -> Result<Option<NameHolder>, sqlx::Error> {
    let kind: Option<String> = sqlx::query_scalar(
        r#"
        SELECT CASE
            WHEN EXISTS (
                SELECT 1 FROM workspaces w
                WHERE w.org_id = $1 AND lower(w.name) = lower($2)
                  AND w.id IS DISTINCT FROM $3
                  AND (w.deleted_at IS NULL OR EXISTS (
                      SELECT 1 FROM workspace_repos m WHERE m.workspace_id = w.id))
            ) THEN 'current'
            WHEN EXISTS (
                SELECT 1 FROM workspace_former_names f
                WHERE f.org_id = $1 AND lower(f.name) = lower($2)
                  AND f.workspace_id IS DISTINCT FROM $3
            ) THEN 'former'
            WHEN EXISTS (
                SELECT 1 FROM workspace_forks fork
                WHERE fork.org_id = $1 AND fork.status IN ('pending', 'aborting')
                  AND lower(fork.proposed_name) = lower($2)
            ) THEN 'fork_claim'
        END
        "#,
    )
    .bind(org_id)
    .bind(name)
    .bind(exclude_workspace)
    .fetch_one(conn)
    .await?;
    Ok(kind.as_deref().and_then(NameHolder::parse))
}

/// A workspace's former names, newest first.
pub async fn former_names(
    conn: &mut PgConnection,
    workspace_id: Uuid,
) -> Result<Vec<String>, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT name FROM workspace_former_names
         WHERE workspace_id = $1
         ORDER BY created_at DESC, name",
    )
    .bind(workspace_id)
    .fetch_all(conn)
    .await
}
