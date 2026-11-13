//! Own the SQL task and its Git input pins as one retryable resolution attempt.
use crate::{errors::AppError, services::AppState};
use g5_authz::ScopedDb;
use g5_contracts::G5Error;
use serde_json::json;
use uuid::Uuid;

pub(crate) struct ResolutionInputs {
    pub run_id: Uuid,
    pub org_id: Uuid,
    pub workspace_id: Uuid,
    pub repo_id: Uuid,
    pub actor_user_id: Uuid,
    pub root_path: String,
    pub local_branch: String,
    pub local_tip: String,
    pub remote_tip: String,
}

/// The caller holds the workspace fence and has revalidated authorization,
/// runtime ownership and captured heads. The row lock below is the durable
/// arbiter for pins: a committed task freezes them; without a task, any pins
/// are abandoned preparation and may be repaired. This also handles an unknown
/// commit outcome: a retry waits for the original transaction to finish first.
pub(crate) async fn enqueue_resolution(
    state: &AppState,
    db: &mut ScopedDb,
    input: ResolutionInputs,
    agent_launch: Option<&g5_contracts::types::AgentLaunchSelectionV1>,
) -> Result<(), AppError> {
    let actor_user_id = input.actor_user_id;
    let attempt: Option<(Option<Uuid>,)> = sqlx::query_as(
        "SELECT resolution_task_id FROM workspace_merge_runs
         WHERE id=$1 AND org_id=$2 AND workspace_id=$3 AND repo_id=$4
           AND source_kind='remote_tracking' AND phase='workspace_rebase_preflight'
           AND status IN ('waiting_for_human', 'running') FOR UPDATE",
    )
    .bind(input.run_id)
    .bind(input.org_id)
    .bind(input.workspace_id)
    .bind(input.repo_id)
    .fetch_optional(db.conn())
    .await?;
    match attempt {
        Some((Some(_),)) => return Ok(()),
        Some((None,)) => {}
        None => {
            return Err(G5Error::Conflict {
                reason: "Resolution attempt is no longer open".into(),
            }
            .into())
        }
    }
    // The locked row proves that no committed task owns these pins. Capture
    // abandoned preparation, including an incomplete pair, and have the Git
    // operation recheck both values under its own ref locks before replacing.
    let expected_pins = (|| {
        let path = state
            .repo_storage
            .repo_path(&*db, input.org_id, input.repo_id)?;
        g5_scm::GitRepo::open(&path)?.resolution_pin_state(input.workspace_id, input.run_id)
    })()
    .map_err(|error| {
        crate::services::git_conflict_resolution::resolution_pin_error(
            input.repo_id,
            input.workspace_id,
            input.run_id,
            g5_scm::ResolutionPinError::Refused {
                stage: g5_contracts::error::GitOperationStage::InspectRefs,
                error,
            },
        )
    })?;
    let queued = crate::services::git_conflict_resolution::enqueue(
        state,
        db,
        crate::services::git_conflict_resolution::ResolutionRequest {
            id: input.run_id,
            org_id: input.org_id,
            workspace_id: input.workspace_id,
            actor_user_id,
            repositories: vec![
                crate::services::git_conflict_resolution::ResolutionRepository {
                    repo_id: input.repo_id,
                    root_path: input.root_path,
                    local_branch: input.local_branch,
                    local_tip: input.local_tip.clone(),
                    remote_tip: input.remote_tip.clone(),
                    expected_pins: Some(expected_pins),
                },
            ],
            purpose: crate::services::git_conflict_resolution::ResolutionPurpose::Provider,
            agent_launch,
        },
    )
    .await?;
    // Preserve the initial waiting observation in pre_start_source_heads. These
    // are the immutable inputs of the one resolution attempt, matching its pins
    // and prompt even if the provider advanced while the run was waiting.
    sqlx::query("UPDATE workspace_merge_runs SET resolution_task_id=$2, source_heads=$3, updated_at=now() WHERE id=$1")
        .bind(input.run_id)
        .bind(queued.task_id)
        .bind(json!({"workspace_tip": input.local_tip, "mirror_tip": input.remote_tip}))
        .execute(db.conn())
        .await?;
    Ok(())
}

/// Pin the inputs of an in-VM adoption on its divergence run
/// (`g5.workspace.remote.adopt-provider-history`): the replaced head stays
/// recoverable and the checkout fetches the reviewed tip from the remote pin.
/// The caller holds the workspace fence and the run row, which no agent task
/// owns, so any earlier pins are abandoned preparation and are replaced only
/// if they are still what was observed.
pub(crate) fn pin_adoption_inputs(
    state: &AppState,
    db: &ScopedDb,
    input: &ResolutionInputs,
) -> Result<(), AppError> {
    let refused = |error| {
        crate::services::git_conflict_resolution::resolution_pin_error(
            input.repo_id,
            input.workspace_id,
            input.run_id,
            g5_scm::ResolutionPinError::Refused {
                stage: g5_contracts::error::GitOperationStage::InspectRefs,
                error,
            },
        )
    };
    let path = state
        .repo_storage
        .repo_path(db, input.org_id, input.repo_id)?;
    let repo = g5_scm::GitRepo::open(&path).map_err(refused)?;
    let observed = repo
        .resolution_pin_state(input.workspace_id, input.run_id)
        .map_err(refused)?;
    repo.replace_resolution_inputs(
        input.workspace_id,
        input.run_id,
        &input.local_tip,
        &input.remote_tip,
        &observed,
    )
    .map_err(|error| {
        crate::services::git_conflict_resolution::resolution_pin_error(
            input.repo_id,
            input.workspace_id,
            input.run_id,
            error,
        )
    })?;
    Ok(())
}
