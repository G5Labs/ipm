//! Registry-kind lookups and the personal-workspace refusals
//! (`g5.workspace.kind`, `g5.personal.refusals`).
//!
//! Kind is read from the registry columns, never from a workspace name, so an
//! ordinary workspace whose name looks personal stays a working workspace.

use g5_contracts::error::G5Error;
use g5_contracts::types::WorkspaceKind;
use sqlx::PgConnection;
use uuid::Uuid;

/// The registry kind of a workspace visible on this connection, or `None` when
/// the row is absent (callers keep their own not-found handling).
pub async fn workspace_kind(
    conn: &mut PgConnection,
    workspace_id: Uuid,
) -> Result<Option<WorkspaceKind>, G5Error> {
    let row = sqlx::query_as::<_, (bool, Option<Uuid>)>(
        "SELECT is_base, personal_user_id FROM workspaces WHERE id = $1",
    )
    .bind(workspace_id)
    .fetch_optional(conn)
    .await
    .map_err(|error| G5Error::Internal {
        message: format!("Failed to load workspace kind: {error}"),
    })?;
    Ok(row
        .map(|(is_base, personal_user_id)| WorkspaceKind::from_registry(is_base, personal_user_id)))
}

/// The error every personal refusal returns.
pub fn personal_refusal(operation: &str) -> G5Error {
    G5Error::WorkspaceIsPersonal {
        operation: operation.to_string(),
    }
}

/// Refuse `operation` when the workspace is personal. An absent row is left to
/// the caller's own lookup.
pub async fn refuse_personal(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    operation: &str,
) -> Result<(), G5Error> {
    match workspace_kind(conn, workspace_id).await? {
        Some(WorkspaceKind::Personal) => Err(personal_refusal(operation)),
        _ => Ok(()),
    }
}
