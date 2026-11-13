//! Workspace mutation policy guard — the central server-side authorization
//! boundary for read-only workspaces.
//!
//! Backs the durable `workspaces.mutation_policy` column
//! (`software_architecture/g5-api/models/workspace` [I2b]). Every
//! workspace-scoped write route, internal worker mutation endpoint, live workspace file
//! upgrade, and background scheduler consults this module instead of
//! re-deriving read-only state from `is_base`, `status`, role scopes, live workspace file
//! reviewer mode, or agent-sandbox settings — none of which is comprehensive
//! or semantically correct for content mutability.
//!
//! Read-only is an app/content mutability policy, not a Git-ref protection
//! rule: Git operations remain allowed under normal Git permissions.
//!
//! Spec: `software_architecture/g5-api/services/workspace_mutation_policy`.

use g5_authz::ScopedDb;
use g5_contracts::error::G5Error;
use serde_json::json;
use uuid::Uuid;

/// The category of a workspace-scoped mutation, used to decide whether it is
/// blocked when the workspace is read-only (spec [I1]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkspaceMutationKind {
    /// User-driven write to workspace-owned content (metadata, docs/views,
    /// ontology, files, drafts, chat).
    DirectWorkspaceWrite,
    /// Agent/worker write to workspace-owned content on behalf of a session.
    DirectAgentWrite,
    /// Background/scheduler write to workspace-owned content.
    BackgroundContentWrite,
    /// Workspace-owned comment, reply, reaction, or review-annotation write.
    WorkspaceOwnedCommentWrite,
    /// Git protocol/tracking operation (clone/fetch, receive-pack, remote
    /// tracking, pull/auto-pull, publish, branch integration, rebase,
    /// child→parent merge). Always allowed under normal Git permissions.
    GitOperation,
    /// Admin settings/config/permission change, and
    /// workspace deletion (lifecycle removal). Deleting a workspace removes the
    /// whole container rather than mutating workspace-owned content, so it is a
    /// lifecycle/admin operation that stays allowed on a read-only workspace.
    AdminWorkspaceAdministration,
    /// Per-user local/preference state not visible to other users.
    PerUserState,
}

impl WorkspaceMutationKind {
    /// Whether a read-only workspace blocks this kind (spec [I2]). Only the
    /// four workspace-owned-content kinds are blocked; Git, admin, and
    /// per-user operations always proceed to their own auth/permission checks.
    #[must_use]
    pub fn blocked_when_read_only(self) -> bool {
        matches!(
            self,
            WorkspaceMutationKind::DirectWorkspaceWrite
                | WorkspaceMutationKind::DirectAgentWrite
                | WorkspaceMutationKind::BackgroundContentWrite
                | WorkspaceMutationKind::WorkspaceOwnedCommentWrite
        )
    }

    /// Whether the transient `freezing` state blocks this kind (spec [I10],
    /// resolved Open Decision O1: do not hard-block — let already-admitted
    /// agents drain).
    ///
    /// During a read-write → read-only drain the guard is *narrower* than steady
    /// `read_only`: it blocks new user-initiated content (`DirectWorkspaceWrite`,
    /// which includes new non-preview `create_job`) and
    /// `WorkspaceOwnedCommentWrite`, but **allows** `DirectAgentWrite` /
    /// `BackgroundContentWrite` so in-flight agent tasks finish their normal
    /// COLLECT → commit → push → terminalize. The seven non-content exception
    /// kinds stay allowed exactly as under steady read-only.
    #[must_use]
    pub fn blocked_when_freezing(self) -> bool {
        matches!(
            self,
            WorkspaceMutationKind::DirectWorkspaceWrite
                | WorkspaceMutationKind::WorkspaceOwnedCommentWrite
        )
    }
}

/// The persisted mutation-policy state of a workspace: the enforced
/// `mutation_policy` plus the transient `mutation_policy_transition` marker.
/// Loaded together so a single read can make the freezing-aware decision
/// (spec [I10]).
#[derive(Debug, Clone)]
pub struct WorkspacePolicyState {
    /// `workspaces.mutation_policy` — `'read_write'` or `'read_only'`.
    pub mutation_policy: String,
    /// `workspaces.mutation_policy_transition` — `Some("freezing")` while a
    /// read-write → read-only transition is draining, else `None`.
    pub mutation_policy_transition: Option<String>,
}

impl WorkspacePolicyState {
    /// Steady-state read-only (`mutation_policy = 'read_only'`).
    #[must_use]
    pub fn is_read_only(&self) -> bool {
        self.mutation_policy == "read_only"
    }

    /// Transient read-write → read-only drain (`mutation_policy_transition =
    /// 'freezing'`).
    #[must_use]
    pub fn is_freezing(&self) -> bool {
        self.mutation_policy_transition.as_deref() == Some("freezing")
    }

    /// The read-once "treat as read-only" decision used by consumers that only
    /// need a boolean (live workspace file `can_edit`, scheduler admission gate): `true` when
    /// steady read-only **or** freezing (spec [I10]).
    #[must_use]
    pub fn treated_read_only(&self) -> bool {
        self.is_read_only() || self.is_freezing()
    }
}

/// Load `workspaces.mutation_policy` and `workspaces.mutation_policy_transition`
/// together (spec [I10]). A missing workspace is a `NotFound`.
pub async fn load_workspace_policy_state(
    db: &mut ScopedDb,
    workspace_id: Uuid,
) -> Result<WorkspacePolicyState, G5Error> {
    let row: Option<(String, Option<String>)> = sqlx::query_as::<_, (String, Option<String>)>(
        "SELECT mutation_policy, mutation_policy_transition FROM workspaces WHERE id = $1",
    )
    .bind(workspace_id)
    .fetch_optional(db.conn())
    .await
    .map_err(|e| G5Error::Internal {
        message: format!("Failed to load workspace mutation policy: {e}"),
    })?;
    match row {
        Some((mutation_policy, mutation_policy_transition)) => Ok(WorkspacePolicyState {
            mutation_policy,
            mutation_policy_transition,
        }),
        None => Err(G5Error::NotFound {
            resource: format!("workspace {workspace_id}"),
        }),
    }
}

/// Load `workspaces.mutation_policy` and return whether it is `read_only`
/// (spec [I3]).
///
/// Reads through the caller-supplied tenant-scoped `ScopedDb`, preserving org
/// isolation. A missing workspace is a `NotFound`. Use this from callers that
/// must branch on read-only state without erroring (live workspace file `can_edit`,
/// schedulers, worker manifest).
///
/// Freezing-aware (spec [I10]): returns `true` for a steady `read_only`
/// workspace **and** for one that is transitioning (`mutation_policy_transition
/// = 'freezing'`), so read-once consumers treat a draining workspace as
/// non-admitting. Callers that must distinguish steady read-only from a drain
/// use [`load_workspace_policy_state`] and read the raw columns.
pub async fn is_workspace_read_only(
    db: &mut ScopedDb,
    workspace_id: Uuid,
) -> Result<bool, G5Error> {
    Ok(load_workspace_policy_state(db, workspace_id)
        .await?
        .treated_read_only())
}

/// Map a workspace's loaded `is_base` to the Base refusal: Base is read-only
/// for every user, so a write that has read `is_base` refuses it with the
/// structured `WORKSPACE_READ_ONLY` error; an absent row is `NotFound`.
/// Shared by thread writes and issue links, which load the row themselves
/// (the issue link under its own `FOR SHARE` lock).
pub fn refuse_base_workspace(
    is_base: Option<bool>,
    workspace_id: Uuid,
    operation: &'static str,
) -> Result<(), G5Error> {
    match is_base {
        Some(false) => Ok(()),
        Some(true) => Err(G5Error::WorkspaceReadOnly {
            operation,
            workspace_id: workspace_id.to_string(),
        }),
        None => Err(G5Error::NotFound {
            resource: format!("workspace {workspace_id}"),
        }),
    }
}

/// General guard entry point (spec [I3]). Returns `Ok(())` immediately for a
/// non-blocked kind; otherwise loads the policy and returns the structured
/// `WORKSPACE_READ_ONLY` error (spec [I4]) when the workspace is read-only.
///
/// `operation` is a stable `&'static str` label (e.g. `"chat.send_message"`)
/// surfaced in the error `details` and logs. Call this after normal auth and
/// workspace-access checks, but before any local write, remote side effect,
/// worker launch, or queue enqueue for a blocked kind.
pub async fn ensure_workspace_allows_mutation(
    db: &mut ScopedDb,
    workspace_id: Uuid,
    kind: WorkspaceMutationKind,
    operation: &'static str,
) -> Result<(), G5Error> {
    if matches!(
        kind,
        WorkspaceMutationKind::BackgroundContentWrite | WorkspaceMutationKind::GitOperation
    ) {
        let formal:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM workspaces WHERE id=$1 AND agent_approval_review_id IS NOT NULL)").bind(workspace_id).fetch_one(db.conn()).await.map_err(|e|G5Error::Internal{message:e.to_string()})?;
        if formal {
            return Err(G5Error::WorkspaceReadOnly {
                operation,
                workspace_id: workspace_id.to_string(),
            });
        }
    }
    if !kind.blocked_when_read_only() {
        return Ok(());
    }
    let state = load_workspace_policy_state(db, workspace_id).await?;
    // Steady read-only blocks all four content kinds; the transient `freezing`
    // drain window blocks only the user-initiated kinds and lets already-admitted
    // agent writes finish (spec [I10], resolved Open Decision O1).
    let blocked = if state.is_read_only() {
        true
    } else if state.is_freezing() {
        kind.blocked_when_freezing()
    } else {
        false
    };
    if blocked {
        return Err(G5Error::WorkspaceReadOnly {
            operation,
            workspace_id: workspace_id.to_string(),
        });
    }
    Ok(())
}

/// Shorthand for the common case: a user-driven workspace-owned content write
/// (`DirectWorkspaceWrite`).
pub async fn ensure_workspace_mutable(
    db: &mut ScopedDb,
    workspace_id: Uuid,
    operation: &'static str,
) -> Result<(), G5Error> {
    ensure_workspace_mutable_by(db, workspace_id, db.actor_id().into_uuid(), operation).await
}

/// [`ensure_workspace_mutable`] for an explicitly identified actor, including
/// workspace mutations performed on a service-scoped connection.
pub async fn ensure_workspace_mutable_by(
    db: &mut ScopedDb,
    workspace_id: Uuid,
    actor_user_id: Uuid,
    operation: &'static str,
) -> Result<(), G5Error> {
    ensure_workspace_owner_is(db, workspace_id, actor_user_id).await?;
    ensure_workspace_allows_mutation(
        db,
        workspace_id,
        WorkspaceMutationKind::DirectWorkspaceWrite,
        operation,
    )
    .await
}

/// Enforce the MVP single-writer rule at the final database-backed mutation
/// boundary. Workspace roles, creation history and organization administration
/// do not confer write authority; an administrator must transfer ownership
/// first. Reading the actor from `ScopedDb` keeps this check coupled to the
/// exact capability whose transaction performs the write.
pub async fn ensure_workspace_owner(db: &mut ScopedDb, workspace_id: Uuid) -> Result<(), G5Error> {
    ensure_workspace_owner_is(db, workspace_id, db.actor_id().into_uuid()).await
}

/// The organization of a live Base workspace, and when `pull_request_id` is
/// given, only if that G5 PR targets it: the subjects an organization
/// administrator may act on there (g5.governance.pr.base-admin,
/// g5.workspace.remote.base-admin-pull). `None` otherwise.
pub async fn base_admin_subject_org(
    db: &mut ScopedDb,
    workspace_id: Uuid,
    pull_request_id: Option<Uuid>,
) -> Result<Option<Uuid>, G5Error> {
    sqlx::query_scalar(
        "SELECT workspace.org_id
           FROM workspaces AS workspace
          WHERE workspace.id = $1
            AND workspace.is_base
            AND workspace.deleted_at IS NULL
            AND ($2::uuid IS NULL OR EXISTS (
                    SELECT 1 FROM workspace_pull_requests AS pull_request
                     WHERE pull_request.id = $2
                       AND pull_request.target_workspace_id = workspace.id))",
    )
    .bind(workspace_id)
    .bind(pull_request_id)
    .fetch_optional(db.conn())
    .await
    .map_err(|error| G5Error::Internal {
        message: format!("Failed to load the Base workspace: {error}"),
    })
}

/// Does `target_workspace_id` have at least one active attached approval
/// requirement (`g5.governance.approval.requirements`)? Any kind counts,
/// agent-only and self-approval-allowing definitions included, matching the
/// set `evaluate_merge_admission` evaluates. Used to admit the pull request's
/// [source owner](g5.governance.pr.source-owner-merge) as a merger.
pub async fn target_has_attached_requirement(
    db: &mut ScopedDb,
    org_id: Uuid,
    target_workspace_id: Uuid,
) -> Result<bool, G5Error> {
    sqlx::query_scalar(
        "SELECT EXISTS (
             WITH active AS (
                 SELECT DISTINCT ON (approval_definition_id) retired
                   FROM approval_configs
                  WHERE org_id = $1 AND target_workspace_id = $2
                  ORDER BY approval_definition_id, seq DESC
             )
             SELECT 1 FROM active WHERE retired = false
         )",
    )
    .bind(org_id)
    .bind(target_workspace_id)
    .fetch_one(db.conn())
    .await
    .map_err(|error| G5Error::Internal {
        message: format!("Failed to read target approval requirements: {error}"),
    })
}

/// Does `actor_user_id` currently own the source workspace of pull request
/// `pull_request_id` (whose target must be `target_workspace_id`), with the
/// source not deleted and no transfer pending? Row-locks the source so a
/// transfer committing concurrently is serialized, as [`ensure_workspace_owner_is`]
/// does. A merge-authority question, so a lost or transferring ownership reads
/// as `false` rather than an error (`g5.governance.pr.source-owner-merge`).
pub async fn is_source_owner_of_pull_request(
    db: &mut ScopedDb,
    target_workspace_id: Uuid,
    pull_request_id: Uuid,
    actor_user_id: Uuid,
) -> Result<bool, G5Error> {
    let source: Option<Uuid> = sqlx::query_scalar(
        "SELECT source_workspace_id FROM workspace_pull_requests
          WHERE id = $1 AND target_workspace_id = $2",
    )
    .bind(pull_request_id)
    .bind(target_workspace_id)
    .fetch_optional(db.conn())
    .await
    .map_err(|error| G5Error::Internal {
        message: format!("Failed to resolve pull request source: {error}"),
    })?;
    let Some(source) = source else {
        return Ok(false);
    };
    is_merge_source_owner(db, source, actor_user_id).await
}

/// The source ownership fact shared by ordinary admission and override
/// execution, including deletion and transfer serialization.
async fn is_merge_source_owner(
    db: &mut ScopedDb,
    source: Uuid,
    actor_user_id: Uuid,
) -> Result<bool, G5Error> {
    let row: Option<(Option<Uuid>, Option<Uuid>)> = sqlx::query_as(
        "SELECT owner_user_id, deletion_operation_id
           FROM workspaces
          WHERE id = $1 AND deleted_at IS NULL
          FOR SHARE OF workspaces",
    )
    .bind(source)
    .fetch_optional(db.conn())
    .await
    .map_err(|error| G5Error::Internal {
        message: format!("Failed to load source workspace owner: {error}"),
    })?;
    let Some((owner, deletion)) = row else {
        return Ok(false);
    };
    if owner != Some(actor_user_id) || deletion.is_some() {
        return Ok(false);
    }
    // A transfer admitted while the row lock was acquired is visible now.
    match ensure_no_pending_transfer(db.conn(), source).await {
        Ok(()) => Ok(true),
        Err(G5Error::WorkspaceStopping) => Ok(false),
        Err(error) => Err(error),
    }
}

/// Merge authority of a pull request's [source owner](g5.governance.pr.source-owner-merge):
/// the actor currently owns the source (clean, no pending transfer) and the
/// target has an attached approval requirement. The one predicate every merge
/// surface asks for the member path.
pub async fn source_owner_may_merge(
    db: &mut ScopedDb,
    org_id: Uuid,
    target_workspace_id: Uuid,
    pull_request_id: Uuid,
    actor_user_id: Uuid,
) -> Result<bool, G5Error> {
    Ok(
        is_source_owner_of_pull_request(db, target_workspace_id, pull_request_id, actor_user_id)
            .await?
            && target_has_attached_requirement(db, org_id, target_workspace_id).await?,
    )
}

/// Recheck the source-owner merge path for an already resolved source/target
/// pair. Execution must retain the target requirement-set lock as well as the
/// source ownership lock when it relies on this result.
pub async fn source_workspace_owner_may_merge(
    db: &mut ScopedDb,
    org_id: Uuid,
    target_workspace_id: Uuid,
    source_workspace_id: Uuid,
    actor_user_id: Uuid,
) -> Result<bool, G5Error> {
    Ok(
        is_merge_source_owner(db, source_workspace_id, actor_user_id).await?
            && target_has_attached_requirement(db, org_id, target_workspace_id).await?,
    )
}

/// [`ensure_workspace_owner`] with an explicit actor, for a service-scoped
/// connection acting for a user it has already authenticated another way
/// (the owner's agent through its workspace incarnation).
pub async fn ensure_workspace_owner_is(
    db: &mut ScopedDb,
    workspace_id: Uuid,
    actor_user_id: Uuid,
) -> Result<(), G5Error> {
    let row = sqlx::query_as::<_, (Option<Uuid>, Option<Uuid>)>(
        "SELECT workspace.owner_user_id, workspace.deletion_operation_id
         FROM workspaces AS workspace
         WHERE workspace.id = $1 AND workspace.deleted_at IS NULL
         FOR SHARE OF workspace",
    )
    .bind(workspace_id)
    .fetch_optional(db.conn())
    .await
    .map_err(|error| G5Error::Internal {
        message: format!("Failed to load workspace owner: {error}"),
    })?
    .ok_or_else(|| G5Error::NotFound {
        resource: format!("workspace {workspace_id}"),
    })?;
    let (owner, deletion) = row;
    if owner != Some(actor_user_id) {
        return Err(G5Error::Forbidden {
            missing_scopes: Vec::new(),
        });
    }
    if deletion.is_some() {
        return Err(workspace_deletion_pending());
    }
    ensure_no_pending_transfer(db.conn(), workspace_id).await
}

/// Read after acquiring the workspace row lock, so a transfer admitted while
/// that lock was being acquired is visible in this statement's snapshot.
pub(crate) async fn ensure_no_pending_transfer(
    conn: &mut sqlx::PgConnection,
    workspace_id: Uuid,
) -> Result<(), G5Error> {
    let pending: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM workspace_ownership_transfers WHERE workspace_id=$1 AND status='pending')",
    )
    .bind(workspace_id)
    .fetch_one(conn)
    .await?;
    if pending {
        return Err(G5Error::WorkspaceStopping);
    }
    Ok(())
}

pub(crate) fn workspace_deletion_pending() -> G5Error {
    G5Error::Conflict {
        reason: "Workspace deletion is in progress; retry deletion to complete it".into(),
    }
}

/// Retain availability through a dependent operation's admission. Call after
/// any repository/commit fences required by the operation's lock ordering.
pub(crate) async fn ensure_workspace_not_deleting(
    conn: &mut sqlx::PgConnection,
    workspace_id: Uuid,
) -> Result<(), G5Error> {
    let deletion: Option<Uuid> = sqlx::query_scalar(
        "SELECT deletion_operation_id FROM workspaces WHERE id=$1 AND deleted_at IS NULL FOR SHARE",
    )
    .bind(workspace_id)
    .fetch_optional(conn)
    .await?
    .ok_or_else(|| G5Error::NotFound {
        resource: format!("workspace {workspace_id}"),
    })?;
    if deletion.is_some() {
        return Err(workspace_deletion_pending());
    }
    Ok(())
}

/// Idempotency-key prefix that marks a fork as a review workspace. The key
/// is immutable claim identity scoped to `(org, requester)`, so it doubles as
/// the durable review marker without a schema change.
pub const REVIEW_FORK_KEY_PREFIX: &str = "review-workspace:";

/// Whether `workspace_id` is a live review workspace: the completed target
/// of a review fork (`workspace_review_service` owns the review records;
/// this predicate is the one it uses to find a review by its workspace).
async fn is_review_workspace(
    db: &mut ScopedDb,
    org_id: Uuid,
    workspace_id: Uuid,
) -> Result<bool, G5Error> {
    Ok(sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(
             SELECT 1
             FROM workspace_forks f
             JOIN workspaces s ON s.org_id = f.org_id AND s.id = f.source_workspace_id
             JOIN workspaces w ON w.org_id = f.org_id AND w.id = f.target_workspace_id
                AND w.deleted_at IS NULL
             WHERE f.org_id = $1
               AND f.target_workspace_id = $2
               AND f.status = 'completed'
               AND left(f.idempotency_key, $3) = $4
               AND f.idempotency_key NOT LIKE 'review-workspace:%:platform:%')",
    )
    .bind(org_id)
    .bind(workspace_id)
    .bind(REVIEW_FORK_KEY_PREFIX.len() as i32)
    .bind(REVIEW_FORK_KEY_PREFIX)
    .fetch_one(db.conn())
    .await?)
}

/// Refuse an operation that a review workspace must not offer (fork,
/// transfer, pull request, merge, publish). A review workspace is a
/// lightweight copy whose only outputs are suggestions and artifacts; the
/// frontend hides these entry points and the routes refuse them here.
pub async fn ensure_not_review_workspace(
    db: &mut ScopedDb,
    org_id: Uuid,
    workspace_id: Uuid,
    operation: &'static str,
) -> Result<(), G5Error> {
    if is_review_workspace(db, org_id, workspace_id).await? {
        return Err(G5Error::ValidationError {
            details: json!({
                "code": "REVIEW_WORKSPACE_RESTRICTED",
                "operation": operation,
                "message": format!("{operation} is not available in a review workspace; publish suggestions with Push review instead"),
            }),
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocked_kinds_are_exactly_the_four_content_writes() {
        for kind in [
            WorkspaceMutationKind::DirectWorkspaceWrite,
            WorkspaceMutationKind::DirectAgentWrite,
            WorkspaceMutationKind::BackgroundContentWrite,
            WorkspaceMutationKind::WorkspaceOwnedCommentWrite,
        ] {
            assert!(kind.blocked_when_read_only(), "{kind:?} must be blocked");
        }
        for kind in [
            WorkspaceMutationKind::GitOperation,
            WorkspaceMutationKind::AdminWorkspaceAdministration,
            WorkspaceMutationKind::PerUserState,
        ] {
            assert!(
                !kind.blocked_when_read_only(),
                "{kind:?} must be allowed as an exception"
            );
        }
    }

    #[test]
    fn freezing_blocks_only_user_initiated_content_kinds() {
        // During `freezing` (drain), user-direct + comment writes are blocked,
        // but already-admitted agent writes are allowed to drain (spec [I10] O1).
        assert!(WorkspaceMutationKind::DirectWorkspaceWrite.blocked_when_freezing());
        assert!(WorkspaceMutationKind::WorkspaceOwnedCommentWrite.blocked_when_freezing());
        assert!(!WorkspaceMutationKind::DirectAgentWrite.blocked_when_freezing());
        assert!(!WorkspaceMutationKind::BackgroundContentWrite.blocked_when_freezing());
        for kind in [
            WorkspaceMutationKind::GitOperation,
            WorkspaceMutationKind::AdminWorkspaceAdministration,
            WorkspaceMutationKind::PerUserState,
        ] {
            assert!(!kind.blocked_when_freezing(), "{kind:?} must stay allowed");
        }
    }

    #[test]
    fn policy_state_treats_freezing_as_read_only_for_read_once_consumers() {
        let read_write = WorkspacePolicyState {
            mutation_policy: "read_write".to_string(),
            mutation_policy_transition: None,
        };
        assert!(!read_write.treated_read_only());
        assert!(!read_write.is_read_only());
        assert!(!read_write.is_freezing());

        let freezing = WorkspacePolicyState {
            mutation_policy: "read_write".to_string(),
            mutation_policy_transition: Some("freezing".to_string()),
        };
        assert!(freezing.treated_read_only());
        assert!(!freezing.is_read_only());
        assert!(freezing.is_freezing());

        let read_only = WorkspacePolicyState {
            mutation_policy: "read_only".to_string(),
            mutation_policy_transition: None,
        };
        assert!(read_only.treated_read_only());
        assert!(read_only.is_read_only());
        assert!(!read_only.is_freezing());
    }

    #[test]
    fn read_only_error_maps_to_structured_403() {
        let err = G5Error::WorkspaceReadOnly {
            operation: "chat.send_message",
            workspace_id: "ws-1".to_string(),
        };
        assert_eq!(err.error_code(), "WORKSPACE_READ_ONLY");
        assert_eq!(err.status_code().as_u16(), 403);
        let resp = err.to_error_response("req-1".to_string());
        assert_eq!(resp.details["operation"], "chat.send_message");
        assert_eq!(resp.details["workspaceId"], "ws-1");
    }
}
