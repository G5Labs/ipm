//! Remote-Repo-Tracking merge handoff — Phase 4.A.
//!
//! Bridges the `branch_integration_worker`'s `Action::Diverged`
//! classification to the durable remote-tracking reconciliation workflow. When the
//! worker decides a (workspace, repository, remote branch) pair has
//! diverged from the upstream baseline, it calls
//! [`open_remote_tracking_merge_run`] to create a paused merge run
//! that operator UI (or a future Phase 4.B/4.D affordance) can resolve.
//!
//! Once the operator resolves the merge through a committed authority
//! advance, the completion flow binds that advance to the merge run, commits
//! the lifecycle metadata, and replays the shared advance notifier. Completion
//! never advances authority or owns a queue write.
//!
//! SOT:
//! - [`designs/REMOTE_REPO_TRACKING.md`](../../../../designs/REMOTE_REPO_TRACKING.md) §8
//! - [`remote_repo_tracking.arch_view.md`](../../../../views/arch_views/remote_repo_tracking.arch_view.md) D38–D41, D55, D60

use crate::services::git::repo_store::{ScopedRepoStorage, TenantRepoStorage};
use g5_authz::{ScopedDb, ScopedDbReopener};
use g5_contracts::types::WorkspaceRepoKey;
use g5_contracts::types::{AuthorityTarget, CommittedBindingSnapshot};
use g5_scm::GitRepo;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::PgConnection;
use uuid::Uuid;

use crate::errors::{AppError, G5Error};
use crate::services::advance_consumer::{
    consumer_receipt_result, load_consumer_input, record_consumer_receipt, register_consumer_input,
};
use crate::services::branch_commit::{
    acquire_workspace_commit_fence, BranchCommitMember, PreparedBranchCommit,
};
use crate::services::branch_integration_service::DivergedPhase;
use crate::services::workspace_merge_service::WorkspaceMergeRunSummary;
use crate::services::workspace_reader::resolve_published_snapshot;
use crate::services::EventBus;

pub(crate) const REMOTE_TRACKING_MERGE_COMPLETION_CONSUMER: &str =
    "remote_tracking_merge_completion";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RemoteTrackingMergeCompletionConsumerInput {
    pub(crate) merge_run_id: Uuid,
    pub(crate) workspace_id: Uuid,
    pub(crate) repo_id: Uuid,
    pub(crate) expected_run_status: String,
    pub(crate) expected_run_phase: String,
    pub(crate) expected_authority_tip: String,
    pub(crate) expected_watermark: Option<String>,
    pub(crate) synchronized_remote_tip: String,
    pub(crate) reconciled_tip: String,
    pub(crate) final_source_heads: serde_json::Value,
    pub(crate) confirmed_by: Uuid,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) authority_audit: Option<RemoteTrackingAuthorityAuditInput>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RemoteTrackingAuthorityAuditInput {
    pub(crate) org_id: Uuid,
    pub(crate) trigger_kind: String,
    pub(crate) old_workspace_tip: String,
    pub(crate) remote_tip: String,
}

#[derive(Debug, Clone)]
pub enum PreparedExternalRemoteTrackingCompletion {
    Ready {
        merge_run_id: Uuid,
        workspace_id: Uuid,
        repo_id: Uuid,
        opening_authority_tip: String,
        reviewed_remote_tip: String,
        current_authority_tip: String,
        resolution_task_id: Option<Uuid>,
        completion_advance_id: Uuid,
        requester: PreparedRequester,
    },
    AlreadyTerminal {
        merge_run_id: Uuid,
        status: String,
        completion_advance_id: Option<Uuid>,
        requester: PreparedRequester,
    },
}

/// The requester with the authority captured for it during preparation.
#[derive(Debug, Clone)]
pub enum PreparedRequester {
    Owner {
        acting_user_id: Uuid,
        push_authority: crate::models::branch_integration_queue::PushAuthority,
    },
    Automatic {
        trigger_kind: &'static str,
    },
}

/// Who asks to complete a remote-tracking divergence run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompletionRequester {
    /// The owner confirmed the resolution ("Mark resolved"). This is also an
    /// explicit request to publish the resolved tip under the owner's authority.
    Owner(Uuid),
    /// The branch integration worker proved that an accepted update resolved
    /// the divergence (g5.workspace.remote.divergence-auto-complete). It is not
    /// a publication request; publication follows the member's push policy
    /// (g5.workspace.remote.divergence-auto-publish). `trigger_kind` is the
    /// audit trigger of the queue wake being processed.
    Automatic { trigger_kind: &'static str },
}

pub struct CompleteRemoteTrackingMergeOutcome {
    pub completed: bool,
    pub projection_warnings: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, sqlx::FromRow)]
pub(crate) struct RemoteTrackingRebasePublishCheckpoint {
    pub(crate) id: Uuid,
    pub(crate) workspace_id: Uuid,
    pub(crate) repo_id: Uuid,
    pub(crate) source_branch_name: String,
    pub(crate) status: String,
    pub(crate) source_heads: serde_json::Value,
    pub(crate) target_heads: serde_json::Value,
    pub(crate) pre_start_source_heads: serde_json::Value,
}

impl RemoteTrackingRebasePublishCheckpoint {
    pub(crate) fn reviewed_remote_tip(&self) -> Result<&str, AppError> {
        self.source_heads
            .get("mirror_tip")
            .and_then(|value| value.as_str())
            .or_else(|| {
                self.target_heads
                    .get("baseline")
                    .and_then(|value| value.as_str())
            })
            .ok_or_else(|| {
                AppError::from_error(G5Error::Conflict {
                    reason: "Active reset-below-baseline approval checkpoint is missing its reviewed remote tip"
                        .to_string(),
                })
            })
    }
}

// ---------------------------------------------------------------------------
// Open
// ---------------------------------------------------------------------------

/// Open a `workspace_merge_runs` row of `source_kind = 'remote_tracking'`.
///
/// Per arch D55 + D60 the row is created in `waiting_for_human` status
/// from the moment it lands — the worker does not attempt any
/// auto-resolution. The phase comes from the classifier
/// ([`DivergedPhase`]) and is recorded so operators can distinguish a
/// normal divergence from a watermark violation.
///
/// `source_heads.workspace_tip` + `source_heads.mirror_tip` carry the
/// snapshot the worker observed under the per-branch lock.
/// `target_heads.baseline` records the synchronization watermark.
/// Both jsonb payloads MUST round-trip through subsequent reads — see
/// arch D60 (merge-run resume re-reads both heads for remote_tracking
/// runs).
///
/// Returns the merge run id. The unique index
/// `uq_active_remote_tracking_merge_per_repo` enforces at-most-one
/// active remote-tracking merge run per `repo_id`.
///
/// This is **idempotent**: if an active run already exists for the
/// `repo_id` (statuses `pending`/`running`/`waiting_for_human`/
/// `rolling_back` per the partial unique index), its id is returned instead of
/// inserting a duplicate — reflecting the "idempotent safety net" intent of
/// workspace_service D47 item 3 and the integration worker's own
/// `sync_paused_reason IS NULL` gate. A concurrent inserter that slips past the
/// pre-check and loses the `UNIQUE` race gets a typed `G5Error::Conflict`
/// instead of an opaque `Internal` 500 (a post-insert re-query is avoided
/// because the failed INSERT aborts the surrounding transaction).
#[allow(clippy::too_many_arguments)]
pub async fn open_remote_tracking_merge_run(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    repo_id: Uuid,
    remote_branch: &str,
    phase: DivergedPhase,
    workspace_tip: &str,
    mirror_tip: Option<&str>,
    baseline: &str,
) -> Result<Uuid, AppError> {
    use crate::errors::G5Error;

    let phase_str = match phase {
        DivergedPhase::WorkspaceRebasePreflight => "workspace_rebase_preflight",
        DivergedPhase::ResetBelowBaseline => "reset_below_baseline",
    };

    crate::services::workspace_mutation_policy::ensure_workspace_not_deleting(conn, workspace_id)
        .await?;

    // Idempotent pre-check: an active run already covers this repo child. Done
    // before the INSERT so the common duplicate case never aborts the caller's
    // transaction with a failed statement.
    let existing: Option<Uuid> = sqlx::query_scalar(
        r#"
        SELECT id
          FROM workspace_merge_runs
         WHERE repo_id = $1 AND workspace_id = $2
           AND source_kind = 'remote_tracking'
           AND status IN ('pending', 'running', 'waiting_for_human', 'rolling_back')
         LIMIT 1
        "#,
    )
    .bind(repo_id)
    .bind(workspace_id)
    .fetch_optional(&mut *conn)
    .await
    .map_err(AppError::from)?;
    if let Some(existing_id) = existing {
        return Ok(existing_id);
    }

    let source_heads = json!({
        "workspace_tip": workspace_tip,
        "mirror_tip": mirror_tip,
    });
    let target_heads = json!({
        "baseline": baseline,
    });
    let pre_start_source_heads = source_heads.clone();

    let insert_result = sqlx::query_scalar::<_, Uuid>(
        r#"
        INSERT INTO workspace_merge_runs (
            workspace_id,
            repo_id,
            source_kind,
            source_workspace_id,
            target_workspace_id,
            source_branch_name,
            status,
            phase,
            source_heads,
            target_heads,
            pre_start_source_heads
        ) VALUES (
            $1, $2,
            'remote_tracking',
            NULL,
            NULL,
            $3,
            'waiting_for_human',
            $4,
            $5,
            $6,
            $7
        )
        RETURNING id
        "#,
    )
    .bind(workspace_id)
    .bind(repo_id)
    .bind(remote_branch)
    .bind(phase_str)
    .bind(&source_heads)
    .bind(&target_heads)
    .bind(&pre_start_source_heads)
    .fetch_one(&mut *conn)
    .await;

    match insert_result {
        Ok(id) => Ok(id),
        // Concurrent inserter won the unique race between our pre-check and this
        // INSERT. Surface a typed conflict rather than an opaque 500.
        Err(sqlx::Error::Database(db_err))
            if db_err.code().as_deref() == Some("23505") =>
        {
            Err(AppError::from_error(G5Error::Conflict {
                reason: format!(
                    "an active remote-tracking merge run already exists for workspace repository {repo_id}"
                ),
            }))
        }
        Err(e) => Err(AppError::from(e)),
    }
}

// ---------------------------------------------------------------------------
// Complete
// ---------------------------------------------------------------------------

/// Capture and prove an externally resolved remote-tracking completion.
///
/// This is a read/preparation phase. It selects only exact database-bound
/// identities and SHAs, then verifies immutable Git ancestry without holding
/// the short lifecycle application transaction.
pub async fn prepare_external_remote_tracking_merge_completion(
    conn: &mut PgConnection,
    merge_run_id: Uuid,
    repo_storage: &ScopedRepoStorage,
    requester: CompletionRequester,
) -> Result<PreparedExternalRemoteTrackingCompletion, AppError> {
    use crate::errors::G5Error;

    #[derive(sqlx::FromRow)]
    // LEAN-TODO(P1.5): retained remote-tracking merge and handoff ordering.
    struct CompletionContext {
        status: String,
        phase: String,
        workspace_id: Uuid,
        repo_id: Option<Uuid>,
        pre_start_source_heads: serde_json::Value,
        source_heads: serde_json::Value,
        resolution_task_id: Option<Uuid>,
        completion_advance_id: Option<Uuid>,
    }

    let context: CompletionContext = sqlx::query_as(
        r#"
        SELECT run.status,
               run.phase,
               run.workspace_id,
               run.repo_id,
               run.pre_start_source_heads,
               run.source_heads,
               run.resolution_task_id,
               run.completion_advance_id
        FROM workspace_merge_runs run

        WHERE run.id = $1
          AND run.source_kind = 'remote_tracking'
        "#,
    )
    .bind(merge_run_id)
    .fetch_optional(&mut *conn)
    .await
    .map_err(AppError::from)?
    .ok_or_else(|| {
        AppError::from_error(G5Error::NotFound {
            resource: format!("remote_tracking merge run {merge_run_id}"),
        })
    })?;

    let prepared_requester = match requester {
        CompletionRequester::Owner(acting_user_id) => PreparedRequester::Owner {
            acting_user_id,
            push_authority: crate::models::branch_integration_queue::PushAuthority::capture(
                conn,
                context.workspace_id,
                acting_user_id,
            )
            .await?,
        },
        CompletionRequester::Automatic { trigger_kind } => {
            PreparedRequester::Automatic { trigger_kind }
        }
    };
    if !matches!(context.status.as_str(), "waiting_for_human" | "running") {
        return Ok(PreparedExternalRemoteTrackingCompletion::AlreadyTerminal {
            merge_run_id,
            status: context.status,
            completion_advance_id: context.completion_advance_id,
            requester: prepared_requester,
        });
    }
    if context.phase == "reset_below_baseline" {
        return Err(AppError::from_error(G5Error::Conflict {
            reason: "reset_below_baseline runs require approved rebase publication".to_string(),
        }));
    }
    // Automatic completion covers divergence runs only; any other phase keeps
    // its explicit human decision.
    if matches!(requester, CompletionRequester::Automatic { .. })
        && context.phase != "workspace_rebase_preflight"
    {
        return Err(AppError::from_error(G5Error::Conflict {
            reason: format!(
                "automatic completion does not apply to {} runs",
                context.phase
            ),
        }));
    }

    let repo_id = context.repo_id.ok_or_else(|| {
        AppError::from_error(G5Error::Conflict {
            reason: "Remote merge run lost its repository binding".into(),
        })
    })?;
    let snapshot = crate::services::workspace_reader::resolve_published_snapshot_on(
        conn,
        repo_storage,
        AuthorityTarget::Workspace {
            workspace_id: context.workspace_id,
        },
    )
    .await?;
    let current_authority_tip = snapshot
        .repo(repo_id)
        .map_err(|reason| {
            AppError::from_error(G5Error::Conflict {
                reason: reason.to_string(),
            })
        })?
        .head_commit
        .clone();
    let input_heads = if context.resolution_task_id.is_some() {
        &context.source_heads
    } else {
        &context.pre_start_source_heads
    };
    let opening_authority_tip = input_heads
        .get("workspace_tip")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| {
            AppError::from_error(G5Error::ValidationError {
                details: json!({
                    "message": "remote_tracking merge run is missing its opening authority SHA",
                    "merge_run_id": merge_run_id,
                }),
            })
        })?
        .to_string();
    let reviewed_remote_tip = input_heads
        .get("mirror_tip")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| {
            AppError::from_error(G5Error::ValidationError {
                details: json!({
                    "message": "remote_tracking merge run is missing its reviewed remote SHA",
                    "merge_run_id": merge_run_id,
                }),
            })
        })?
        .to_string();
    if current_authority_tip == opening_authority_tip {
        return Err(AppError::from_error(G5Error::Conflict {
            reason: "Remote-tracking completion requires an advanced authority head".to_string(),
        }));
    }

    // LEAN-TODO(P1.5): the branch ref is the head authority; there is no
    // registry row behind the advance that moved it, so the completion
    // receipt is keyed by a freshly minted advance id.
    let completion_advance_id = Uuid::new_v4();

    let repo_path = repo_storage
        .repo_path(snapshot.org_id, repo_id)
        .map_err(AppError::from_error)?;
    let repo = GitRepo::open(&repo_path).map_err(AppError::from)?;
    if current_authority_tip != reviewed_remote_tip
        && !repo
            .is_ancestor_of(&reviewed_remote_tip, &current_authority_tip)
            .map_err(AppError::from)?
    {
        return Err(AppError::from_error(G5Error::Conflict {
            reason: format!(
                "Current authority SHA {current_authority_tip} does not descend from reviewed remote SHA {reviewed_remote_tip}"
            ),
        }));
    }

    if context.resolution_task_id.is_some()
        && !repo
            .is_linear_descendant_of(&reviewed_remote_tip, &current_authority_tip)
            .map_err(AppError::from)?
    {
        return Err(AppError::from_error(G5Error::Conflict {
            reason: "Agent resolution must rebase workspace commits onto the captured remote tip without merge commits".into(),
        }));
    }

    Ok(PreparedExternalRemoteTrackingCompletion::Ready {
        requester: prepared_requester,
        merge_run_id,
        workspace_id: context.workspace_id,
        repo_id,
        opening_authority_tip,
        reviewed_remote_tip,
        current_authority_tip,
        resolution_task_id: context.resolution_task_id,
        completion_advance_id,
    })
}

/// Commit lifecycle-only remote-tracking completion against the exact
/// previously prepared authority state, then replay the shared notifier.
#[allow(clippy::too_many_arguments)]
pub async fn apply_external_remote_tracking_merge_completion(
    mut db: ScopedDb,
    prepared: PreparedExternalRemoteTrackingCompletion,
    event_bus: &EventBus,
    repo_storage: &TenantRepoStorage,
) -> Result<CompleteRemoteTrackingMergeOutcome, AppError> {
    use crate::errors::G5Error;

    let (completed, completion_advance_id) = match prepared {
        PreparedExternalRemoteTrackingCompletion::AlreadyTerminal {
            merge_run_id,
            status,
            completion_advance_id,
            requester,
        } => {
            let current: (String, Option<Uuid>) = sqlx::query_as(
                r#"
                SELECT status, completion_advance_id
                FROM workspace_merge_runs
                WHERE id = $1
                  AND source_kind = 'remote_tracking'
                FOR UPDATE
                "#,
            )
            .bind(merge_run_id)
            .fetch_optional(db.conn())
            .await
            .map_err(AppError::from)?
            .ok_or_else(|| {
                AppError::from_error(G5Error::NotFound {
                    resource: format!("remote_tracking merge run {merge_run_id}"),
                })
            })?;
            if current.0 != status || current.1 != completion_advance_id {
                return Err(AppError::from_error(G5Error::Conflict {
                    reason: format!(
                        "Remote-tracking merge run {merge_run_id} changed before completion replay"
                    ),
                }));
            }
            if status == "completed" {
                request_owner_publication(&mut db, merge_run_id, &requester).await?;
            }
            (false, completion_advance_id)
        }
        PreparedExternalRemoteTrackingCompletion::Ready {
            merge_run_id,
            workspace_id,
            repo_id,
            opening_authority_tip,
            reviewed_remote_tip,
            current_authority_tip,
            resolution_task_id,
            completion_advance_id,
            requester,
        } => {
            if let PreparedRequester::Owner {
                acting_user_id,
                push_authority,
            } = &requester
            {
                if crate::models::branch_integration_queue::PushAuthority::capture(
                    db.conn(),
                    workspace_id,
                    *acting_user_id,
                )
                .await?
                    != *push_authority
                {
                    return Err(AppError::from_error(G5Error::Conflict { reason: "Workspace lifecycle changed during merge completion; request completion again".into() }));
                }
            }
            #[derive(sqlx::FromRow)]
            // LEAN-TODO(P1.5): retained remote-tracking merge and handoff ordering.
            struct LockedCompletion {
                status: String,
                phase: String,
                workspace_id: Uuid,
                repo_id: Uuid,
                org_id: Uuid,
                pre_start_source_heads: serde_json::Value,
                source_heads: serde_json::Value,
                resolution_task_id: Option<Uuid>,
                sync_paused_reason: Option<String>,
            }

            // Revalidate the attempt inputs and current Git cut before accepting
            // completion; neither policy nor SQL contains a cached local head.
            let locked: LockedCompletion = sqlx::query_as(
                r#"
                SELECT run.status,
                       run.phase,
                       run.workspace_id,
                       run.repo_id,
                       run.org_id,
                       run.pre_start_source_heads,
                       run.source_heads,
                       run.resolution_task_id,
                       integration.sync_paused_reason
                FROM workspace_merge_runs run
                JOIN remote_integrations integration ON
                    (integration.org_id, integration.workspace_id, integration.repo_id) = (run.org_id, run.workspace_id, run.repo_id)
                WHERE run.id = $1
                  AND run.source_kind = 'remote_tracking'
                  AND run.repo_id = $2
                FOR UPDATE OF run, integration
                "#,
            )
            .bind(merge_run_id)
            .bind(repo_id)
            .fetch_optional(db.conn())
            .await
            .map_err(AppError::from)?
            .ok_or_else(|| {
                AppError::from_error(G5Error::Conflict {
                    reason: format!(
                        "Remote-tracking merge run {merge_run_id} lost its exact authority evidence"
                    ),
                })
            })?;

            let locked_heads = if locked.resolution_task_id.is_some() {
                &locked.source_heads
            } else {
                &locked.pre_start_source_heads
            };
            let locked_opening_tip = locked_heads
                .get("workspace_tip")
                .and_then(serde_json::Value::as_str);
            let locked_remote_tip = locked_heads
                .get("mirror_tip")
                .and_then(serde_json::Value::as_str);
            // The run finished between preparation and this lock (the owner and
            // the worker raced): replay it rather than report a conflict.
            if locked.status == "completed" {
                request_owner_publication(&mut db, merge_run_id, &requester).await?;
                db.commit().await.map_err(AppError::from)?;
                return Ok(CompleteRemoteTrackingMergeOutcome {
                    completed: false,
                    projection_warnings: Vec::new(),
                });
            }
            // Automatic completion applies only while the member is still
            // paused for this divergence run; otherwise there is nothing to do.
            if matches!(requester, PreparedRequester::Automatic { .. })
                && (locked.phase != "workspace_rebase_preflight"
                    || locked.sync_paused_reason.as_deref() != Some("diverged"))
            {
                db.rollback().await.map_err(AppError::from)?;
                return Ok(CompleteRemoteTrackingMergeOutcome {
                    completed: false,
                    projection_warnings: Vec::new(),
                });
            }
            if !matches!(locked.status.as_str(), "waiting_for_human" | "running")
                || locked.phase == "reset_below_baseline"
                || locked.workspace_id != workspace_id
                || locked.repo_id != repo_id
                || locked.resolution_task_id != resolution_task_id
                || locked_opening_tip != Some(opening_authority_tip.as_str())
                || locked_remote_tip != Some(reviewed_remote_tip.as_str())
            {
                return Err(AppError::from_error(G5Error::Conflict {
                    reason: format!(
                        "Remote-tracking merge run {merge_run_id} changed after completion preparation"
                    ),
                }));
            }

            let repos = repo_storage.scoped(&db);
            let snapshot = resolve_published_snapshot(
                &mut db,
                repo_storage,
                AuthorityTarget::Workspace { workspace_id },
            )
            .await?;
            if snapshot
                .repo(repo_id)
                .map_err(|reason| {
                    AppError::from_error(G5Error::Conflict {
                        reason: reason.to_string(),
                    })
                })?
                .head_commit
                != current_authority_tip
            {
                return Err(AppError::from_error(G5Error::Conflict {
                    reason: "Remote merge completion head changed".into(),
                }));
            }
            let refs = snapshot
                .repos
                .iter()
                .map(|member| {
                    Ok(g5_scm::RefCommit {
                        repo_path: repos.repo_path(snapshot.org_id, member.repo_id)?,
                        ref_name: format!("refs/heads/{}", member.branch_name),
                        expected_old: Some(member.head_commit.clone()),
                        new_oid: Some(member.head_commit.clone()),
                    })
                })
                .collect::<Result<Vec<_>, G5Error>>()?;
            GitRepo::apply_ref_commits(&refs).map_err(|error| {
                AppError::from_error(G5Error::Conflict {
                    reason: format!("Remote merge completion cut changed: {error:?}"),
                })
            })?;
            let run_updated = sqlx::query(
                r#"
                UPDATE workspace_merge_runs
                SET status = 'completed',
                    completed_at = now(),
                    confirmed_by = $2,
                    completion_advance_id = $3,
                    blocking_reason = NULL,
                    updated_at = now()
                WHERE id = $1
                  AND source_kind = 'remote_tracking'
                  AND status IN ('waiting_for_human', 'running')
                "#,
            )
            .bind(merge_run_id)
            .bind(match &requester {
                PreparedRequester::Owner { acting_user_id, .. } => Some(*acting_user_id),
                PreparedRequester::Automatic { .. } => None,
            })
            .bind(completion_advance_id)
            .execute(db.conn())
            .await
            .map_err(AppError::from)?;
            let child_updated = sqlx::query(
                r#"
                UPDATE remote_integrations
                SET sync_paused_reason = NULL,
                    updated_at = now()
                WHERE repo_id = $1 AND workspace_id = $2
                "#,
            )
            .bind(repo_id)
            .bind(workspace_id)
            .execute(db.conn())
            .await
            .map_err(AppError::from)?;
            if run_updated.rows_affected() != 1 || child_updated.rows_affected() != 1 {
                return Err(AppError::from_error(G5Error::Conflict {
                    reason: format!(
                        "Remote-tracking merge run {merge_run_id} changed during completion application"
                    ),
                }));
            }
            match &requester {
                // An owner confirmation is an explicit publication request, like
                // the server-side clean reconciliation: upgrade the member's
                // external-Git handoff to `push` with the completion. The landing
                // that moved the head already queued its `commit` wake keyed by
                // that landing's advance, which the precedence clause keeps.
                PreparedRequester::Owner { push_authority, .. } => {
                    crate::models::branch_integration_queue::enqueue_authorized_branch_integration(
                        db.conn(),
                        WorkspaceRepoKey {
                            org_id: locked.org_id,
                            workspace_id,
                            repo_id,
                        },
                        crate::models::branch_integration_queue::trigger_kind::PUSH,
                        chrono::Utc::now(),
                        *push_authority,
                    )
                    .await
                    .map_err(AppError::from)?;
                }
                // Automatic completion requests no publication. Record why
                // tracking resumed; the worker's cycle for the same wake then
                // publishes only if the member's push policy allows it.
                PreparedRequester::Automatic { trigger_kind } => {
                    let audited = sqlx::query(
                        r#"
                        INSERT INTO remote_sync_runs (
                            org_id, workspace_id, repo_id, acting_user_id,
                            kind, trigger_kind, status,
                            old_workspace_tip, new_workspace_tip,
                            merge_run_id, started_at, completed_at
                        )
                        SELECT $1, $2, $3, workspace.owner_user_id,
                               'integration', $4, 'resolved',
                               $5, $6,
                               $7, now(), now()
                        FROM workspaces workspace
                        WHERE workspace.id = $2
                          AND workspace.owner_user_id IS NOT NULL
                        "#,
                    )
                    .bind(locked.org_id)
                    .bind(workspace_id)
                    .bind(repo_id)
                    .bind(*trigger_kind)
                    .bind(&opening_authority_tip)
                    .bind(&current_authority_tip)
                    .bind(merge_run_id)
                    .execute(db.conn())
                    .await
                    .map_err(AppError::from)?;
                    if audited.rows_affected() != 1 {
                        // Not a race: retrying cannot supply an owner.
                        return Err(AppError::from_error(G5Error::Internal {
                            message: format!(
                                "Workspace {workspace_id} has no owner to attribute the automatic completion audit"
                            ),
                        }));
                    }
                }
            }
            (true, Some(completion_advance_id))
        }
    };

    db.commit().await.map_err(AppError::from)?;

    // The refs already carry the completed head; projections converge from
    // them (design v6 §9.6) and no notifier replay is needed.
    let _ = (completion_advance_id, event_bus, repo_storage);

    Ok(CompleteRemoteTrackingMergeOutcome {
        completed,
        projection_warnings: Vec::new(),
    })
}

/// A late owner confirmation of an already-completed run keeps its meaning as
/// an explicit publication request (g5.workspace.remote.divergence-auto-publish):
/// queue the owner's authorized push. Automatic requesters publish nothing.
async fn request_owner_publication(
    db: &mut ScopedDb,
    merge_run_id: Uuid,
    requester: &PreparedRequester,
) -> Result<(), AppError> {
    use crate::errors::G5Error;

    let PreparedRequester::Owner {
        acting_user_id,
        push_authority,
    } = requester
    else {
        return Ok(());
    };
    let key: (Uuid, Uuid, Option<Uuid>) = sqlx::query_as(
        "SELECT org_id, workspace_id, repo_id FROM workspace_merge_runs
         WHERE id = $1 AND source_kind = 'remote_tracking'",
    )
    .bind(merge_run_id)
    .fetch_one(db.conn())
    .await
    .map_err(AppError::from)?;
    let (org_id, workspace_id, Some(repo_id)) = key else {
        return Ok(());
    };
    if crate::models::branch_integration_queue::PushAuthority::capture(
        db.conn(),
        workspace_id,
        *acting_user_id,
    )
    .await?
        != *push_authority
    {
        return Err(AppError::from_error(G5Error::Conflict {
            reason: "Workspace lifecycle changed during merge completion; request completion again"
                .into(),
        }));
    }
    crate::models::branch_integration_queue::enqueue_authorized_branch_integration(
        db.conn(),
        WorkspaceRepoKey {
            org_id,
            workspace_id,
            repo_id,
        },
        crate::models::branch_integration_queue::trigger_kind::PUSH,
        chrono::Utc::now(),
        *push_authority,
    )
    .await
    .map_err(AppError::from)?;
    Ok(())
}

/// Stage remote-tracking completion metadata in the landing transaction. This
/// helper never commits, moves refs, or enqueues projections; the landing
/// publishes the hint and the convergence loop derives the projections.
pub(crate) async fn stage_remote_tracking_merge_completion_for_advance(
    conn: &mut PgConnection,
    merge_run_id: Uuid,
    reconciled_tip: &str,
    _expected_authority_tip: &str,
    completion_advance_id: Uuid,
    confirmed_by: Option<Uuid>,
    final_source_heads: Option<&serde_json::Value>,
) -> Result<Uuid, crate::errors::G5Error> {
    use crate::errors::G5Error;

    #[derive(sqlx::FromRow)]
    struct UpdatedRun {
        workspace_id: Uuid,
        repo_id: Option<Uuid>,
    }

    let updated: Option<UpdatedRun> = sqlx::query_as(
        r#"
        UPDATE workspace_merge_runs
           SET status = 'completed',
               phase = 'post_rebase_alignment',
               source_heads = COALESCE($2, source_heads),
               target_heads = COALESCE(target_heads, '{}'::jsonb)
                   || jsonb_build_object('reconciled_tip', $3::text),
               confirmed_by = COALESCE(confirmed_by, $4),
               completion_advance_id = $5,
               blocking_reason = NULL,
               completed_at = now(),
               updated_at = now()
         WHERE id = $1
           AND source_kind = 'remote_tracking'
           AND status IN ('waiting_for_human', 'running')
        RETURNING workspace_id, repo_id
        "#,
    )
    .bind(merge_run_id)
    .bind(final_source_heads)
    .bind(reconciled_tip)
    .bind(confirmed_by)
    .bind(completion_advance_id)
    .fetch_optional(&mut *conn)
    .await?;

    let updated = updated.ok_or_else(|| G5Error::Conflict {
        reason: format!(
            "Remote-tracking merge run {merge_run_id} is not active for authority publication"
        ),
    })?;
    let repo_id = updated.repo_id.ok_or_else(|| G5Error::ValidationError {
        details: json!({
            "message": "remote_tracking merge run is missing repo_id",
            "merge_run_id": merge_run_id,
        }),
    })?;

    let child_updated = sqlx::query(
        r#"
        UPDATE remote_integrations
           SET sync_paused_reason = NULL, last_sync_at = now(), updated_at = now()
         WHERE repo_id = $1 AND workspace_id = $2
        "#,
    )
    .bind(repo_id)
    .bind(updated.workspace_id)
    .execute(&mut *conn)
    .await?;
    if child_updated.rows_affected() != 1 {
        return Err(G5Error::Conflict {
            reason: format!(
                "Remote-tracking repository {repo_id} changed before authority publication"
            ),
        });
    }

    Ok(repo_id)
}

// LEAN-TODO(P1.5): retained remote-tracking merge and handoff ordering.
pub(crate) async fn register_remote_tracking_merge_completion_consumer(
    conn: &mut PgConnection,
    cut: &PreparedBranchCommit,
    input: &RemoteTrackingMergeCompletionConsumerInput,
) -> Result<(), G5Error> {
    if *cut.target()
        != (AuthorityTarget::Workspace {
            workspace_id: input.workspace_id,
        })
    {
        return Err(G5Error::Conflict {
            reason: format!(
                "Remote-tracking completion {} targets the wrong workspace cut",
                input.merge_run_id
            ),
        });
    }
    let operation_repo = cut
        .members()
        .iter()
        .find(|repo| repo.repo_id == input.repo_id)
        .ok_or_else(|| G5Error::ValidationError {
            details: json!({
                "merge_run_id": input.merge_run_id,
                "repo_id": input.repo_id,
                "message": "Remote-tracking completion is outside the prepared workspace cut",
            }),
        })?;
    if operation_repo.expected_old_head != input.expected_authority_tip
        || operation_repo.proposed_new_head != input.reconciled_tip
    {
        return Err(G5Error::Conflict {
            reason: format!(
                "Remote-tracking completion {} does not match its prepared exact member",
                input.merge_run_id
            ),
        });
    }
    if input.authority_audit.as_ref().is_some_and(|audit| {
        audit.org_id.is_nil()
            || audit.trigger_kind.trim().is_empty()
            || audit.old_workspace_tip != input.expected_authority_tip
            || audit.remote_tip.trim().is_empty()
    }) {
        return Err(G5Error::ValidationError {
            details: json!({
                "merge_run_id": input.merge_run_id,
                "message": "Remote-tracking authority audit does not match the prepared exact cut",
            }),
        });
    }
    register_consumer_input(
        conn,
        cut.advance_id(),
        REMOTE_TRACKING_MERGE_COMPLETION_CONSUMER,
        input,
    )
    .await?;
    Ok(())
}

/// Complete the remote-tracking merge run whose completion input was
/// registered against this landing, in the landing transaction.
// LEAN-TODO(P1.5): retained remote-tracking merge and handoff ordering.
pub(crate) async fn apply_remote_tracking_merge_completion_consumer(
    conn: &mut PgConnection,
    advance_id: Uuid,
    target: &AuthorityTarget,
    members: &[BranchCommitMember],
) -> Result<bool, G5Error> {
    if consumer_receipt_result::<bool>(conn, advance_id, REMOTE_TRACKING_MERGE_COMPLETION_CONSUMER)
        .await?
        .is_some()
    {
        return Ok(false);
    }
    let Some(input) = load_consumer_input::<RemoteTrackingMergeCompletionConsumerInput>(
        conn,
        advance_id,
        REMOTE_TRACKING_MERGE_COMPLETION_CONSUMER,
    )
    .await?
    else {
        return Ok(false);
    };
    if *target
        != (AuthorityTarget::Workspace {
            workspace_id: input.workspace_id,
        })
        || !members.iter().any(|member| {
            member.repo_id == input.repo_id && member.proposed_new_head == input.reconciled_tip
        })
    {
        return Err(G5Error::Conflict {
            reason: format!(
                "Remote-tracking completion consumer {} does not match the landed workspace cut",
                input.merge_run_id
            ),
        });
    }

    let current: Option<(String, String, Option<Uuid>)> = sqlx::query_as(
        r#"
        SELECT status, phase, repo_id
        FROM workspace_merge_runs
        WHERE id = $1
          AND source_kind = 'remote_tracking'
        FOR UPDATE
        "#,
    )
    .bind(input.merge_run_id)
    .fetch_optional(&mut *conn)
    .await?;
    let current = current.ok_or_else(|| G5Error::Conflict {
        reason: format!(
            "Remote-tracking merge run {} disappeared before receipt reconciliation",
            input.merge_run_id
        ),
    })?;
    if current.0 != input.expected_run_status
        || current.1 != input.expected_run_phase
        || current.2 != Some(input.repo_id)
    {
        return Err(G5Error::Conflict {
            reason: format!(
                "Remote-tracking merge run {} changed before receipt reconciliation",
                input.merge_run_id
            ),
        });
    }

    stage_remote_tracking_merge_completion_for_advance(
        conn,
        input.merge_run_id,
        &input.reconciled_tip,
        &input.reconciled_tip,
        advance_id,
        Some(input.confirmed_by),
        Some(&input.final_source_heads),
    )
    .await?;
    if let Some(audit) = input.authority_audit.as_ref() {
        sqlx::query(
            r#"
            INSERT INTO remote_sync_runs (
                org_id, repo_id, acting_user_id, workspace_id,
                kind, trigger_kind, status,
                old_workspace_tip, new_workspace_tip,
                old_mirror_tip, new_mirror_tip,
                merge_run_id,
                started_at, completed_at
            ) VALUES (
                $1, $2, $3, $9,
                'integration', $4, 'imported',
                $5, $6,
                $7, $7,
                $8,
                now(), now()
            )
            "#,
        )
        .bind(audit.org_id)
        .bind(input.repo_id)
        .bind(input.confirmed_by)
        .bind(audit.trigger_kind.trim())
        .bind(&audit.old_workspace_tip)
        .bind(&input.reconciled_tip)
        .bind(&audit.remote_tip)
        .bind(input.merge_run_id)
        .bind(input.workspace_id)
        .execute(&mut *conn)
        .await?;
    }
    record_consumer_receipt(
        conn,
        advance_id,
        REMOTE_TRACKING_MERGE_COMPLETION_CONSUMER,
        Some(&true),
    )
    .await?;
    Ok(true)
}

pub(crate) async fn load_remote_tracking_rebase_publish_checkpoint(
    conn: &mut PgConnection,
    key: WorkspaceRepoKey,
) -> Result<RemoteTrackingRebasePublishCheckpoint, AppError> {
    let checkpoint = sqlx::query_as::<_, RemoteTrackingRebasePublishCheckpoint>(
        r#"
        SELECT
            id,
            workspace_id,
            repo_id,
            source_branch_name,
            status,
            source_heads,
            target_heads,
            pre_start_source_heads
        FROM workspace_merge_runs
        WHERE repo_id = $1 AND workspace_id = $2 AND org_id = $3
          AND source_kind = 'remote_tracking'
          AND phase = 'reset_below_baseline'
          AND status IN ('waiting_for_human', 'running')
        ORDER BY started_at DESC NULLS LAST, id DESC
        LIMIT 1
        "#,
    )
    .bind(key.repo_id)
    .bind(key.workspace_id)
    .bind(key.org_id)
    .fetch_optional(&mut *conn)
    .await
    .map_err(AppError::from)?;

    checkpoint.ok_or_else(|| {
        AppError::from_error(G5Error::Conflict {
            reason: format!(
                "No active reset-below-baseline remote-tracking approval checkpoint for repo row {key}"
            ),
        })
    })
}

pub(crate) async fn refresh_remote_tracking_rebase_publish_checkpoint(
    conn: &mut PgConnection,
    captured: &RemoteTrackingRebasePublishCheckpoint,
    live_remote_tip: &str,
    acting_user_id: Uuid,
) -> Result<Uuid, AppError> {
    let current = sqlx::query_as::<_, RemoteTrackingRebasePublishCheckpoint>(
        r#"
        SELECT
            id,
            workspace_id,
            repo_id,
            source_branch_name,
            status,
            source_heads,
            target_heads,
            pre_start_source_heads
        FROM workspace_merge_runs
        WHERE id = $1
          AND repo_id = $2 AND workspace_id = $3
          AND source_kind = 'remote_tracking'
          AND phase = 'reset_below_baseline'
          AND status IN ('waiting_for_human', 'running')
        FOR UPDATE
        "#,
    )
    .bind(captured.id)
    .bind(captured.repo_id)
    .bind(captured.workspace_id)
    .fetch_optional(&mut *conn)
    .await
    .map_err(AppError::from)?
    .ok_or_else(|| {
        AppError::from_error(G5Error::Conflict {
            reason:
                "The rebase-publication approval checkpoint changed before it could be refreshed"
                    .to_string(),
        })
    })?;

    if current != *captured {
        return Err(AppError::from_error(G5Error::Conflict {
            reason: "The rebase-publication approval checkpoint evidence changed before it could be refreshed"
                .to_string(),
        }));
    }

    let reviewed_remote_tip = captured.reviewed_remote_tip()?.to_string();
    if reviewed_remote_tip == live_remote_tip {
        return Err(AppError::from_error(G5Error::Conflict {
            reason: "The provider tip no longer requires an approval checkpoint refresh"
                .to_string(),
        }));
    }
    let reviewed_workspace_tip = captured
        .source_heads
        .get("workspace_tip")
        .and_then(|value| value.as_str())
        .ok_or_else(|| {
            AppError::from_error(G5Error::Conflict {
                reason: "The rebase-publication approval checkpoint is missing its reviewed workspace candidate"
                    .to_string(),
            })
        })?;
    let baseline = captured
        .target_heads
        .get("baseline")
        .and_then(|value| value.as_str())
        .ok_or_else(|| {
            AppError::from_error(G5Error::Conflict {
                reason: "The rebase-publication approval checkpoint is missing its baseline"
                    .to_string(),
            })
        })?;

    crate::services::workspace_mutation_policy::ensure_workspace_not_deleting(
        conn,
        captured.workspace_id,
    )
    .await?;

    let terminalized = sqlx::query(
        r#"
        UPDATE workspace_merge_runs
           SET status = 'cancelled',
               blocking_reason = 'provider_descendant_advanced',
               confirmed_by = $2,
               completed_at = now(),
               updated_at = now()
         WHERE id = $1
           AND status IN ('waiting_for_human', 'running')
        "#,
    )
    .bind(captured.id)
    .bind(acting_user_id)
    .execute(&mut *conn)
    .await
    .map_err(AppError::from)?;
    if terminalized.rows_affected() != 1 {
        return Err(AppError::from_error(G5Error::Conflict {
            reason:
                "The rebase-publication approval checkpoint changed before it could be terminalized"
                    .to_string(),
        }));
    }

    let source_heads = json!({
        "workspace_tip": reviewed_workspace_tip,
        "mirror_tip": live_remote_tip,
    });
    let target_heads = json!({
        "baseline": baseline,
        "refresh_reason": "provider_descendant_advanced",
        "previous_remote_tip": reviewed_remote_tip,
        "supersedes_merge_run_id": captured.id,
    });
    let pre_start_source_heads = source_heads.clone();

    sqlx::query_scalar::<_, Uuid>(
        r#"
        INSERT INTO workspace_merge_runs (
            workspace_id,
            repo_id,
            source_kind,
            source_workspace_id,
            target_workspace_id,
            source_branch_name,
            status,
            phase,
            source_heads,
            target_heads,
            pre_start_source_heads,
            started_at
        ) VALUES (
            $1, $2,
            'remote_tracking',
            NULL,
            NULL,
            $3,
            'waiting_for_human',
            'reset_below_baseline',
            $4,
            $5,
            $6,
            now()
        )
        RETURNING id
        "#,
    )
    .bind(captured.workspace_id)
    .bind(captured.repo_id)
    .bind(&captured.source_branch_name)
    .bind(&source_heads)
    .bind(&target_heads)
    .bind(&pre_start_source_heads)
    .fetch_one(&mut *conn)
    .await
    .map_err(AppError::from)
}

pub async fn approve_remote_tracking_rebase_publish(
    conn: &mut PgConnection,
    expected_merge_run_id: Uuid,
    key: WorkspaceRepoKey,
    approved_workspace_tip: &str,
    expected_authority_tip: &str,
    expected_remote_tip: &str,
    acting_user_id: Uuid,
) -> Result<Uuid, AppError> {
    use crate::errors::G5Error;

    #[derive(sqlx::FromRow)]
    struct Row {
        id: Uuid,
        source_heads: Option<serde_json::Value>,
        target_heads: Option<serde_json::Value>,
    }

    let row: Option<Row> = sqlx::query_as(
        r#"
        SELECT id, source_heads, target_heads
        FROM workspace_merge_runs
        WHERE id = $1
          AND repo_id = $2 AND workspace_id = $3 AND org_id = $4
          AND source_kind = 'remote_tracking'
          AND phase = 'reset_below_baseline'
          AND status IN ('waiting_for_human', 'running')
        FOR UPDATE
        "#,
    )
    .bind(expected_merge_run_id)
    .bind(key.repo_id)
    .bind(key.workspace_id)
    .bind(key.org_id)
    .fetch_optional(&mut *conn)
    .await
    .map_err(AppError::from)?;

    let row = row.ok_or_else(|| {
        AppError::from_error(G5Error::Conflict {
            reason: format!(
                "No active reset-below-baseline remote-tracking merge run {expected_merge_run_id} for repo row {key}"
            ),
        })
    })?;

    let reviewed_remote_tip = row
        .source_heads
        .as_ref()
        .and_then(|heads| heads.get("mirror_tip"))
        .and_then(|value| value.as_str())
        .or_else(|| {
            row.target_heads
                .as_ref()
                .and_then(|heads| heads.get("baseline"))
                .and_then(|value| value.as_str())
        })
        .ok_or_else(|| {
            AppError::from_error(G5Error::Conflict {
                reason: "Active reset-below-baseline merge run is missing reviewed remote tip"
                    .to_string(),
            })
        })?;

    if reviewed_remote_tip != expected_remote_tip {
        return Err(AppError::from_error(G5Error::Conflict {
            reason: format!(
                "Remote tip changed since reset review: expected {expected_remote_tip}, found {reviewed_remote_tip}"
            ),
        }));
    }

    let mut target_heads = row.target_heads.unwrap_or_else(|| json!({}));
    if !target_heads.is_object() {
        return Err(AppError::from_error(G5Error::Conflict {
            reason: "Active reset-below-baseline merge run has non-object target_heads".to_string(),
        }));
    }
    if let Some(obj) = target_heads.as_object_mut() {
        obj.insert(
            "approved_workspace_tip".to_string(),
            serde_json::Value::String(approved_workspace_tip.to_string()),
        );
        obj.insert(
            "expected_authority_tip".to_string(),
            serde_json::Value::String(expected_authority_tip.to_string()),
        );
        obj.insert(
            "expected_remote_tip".to_string(),
            serde_json::Value::String(expected_remote_tip.to_string()),
        );
    }

    sqlx::query(
        r#"
        UPDATE workspace_merge_runs
           SET target_heads = $2,
               confirmed_by = $3,
               blocking_reason = NULL,
               updated_at = now()
         WHERE id = $1
        "#,
    )
    .bind(row.id)
    .bind(&target_heads)
    .bind(acting_user_id)
    .execute(&mut *conn)
    .await
    .map_err(AppError::from)?;

    Ok(row.id)
}

// ---------------------------------------------------------------------------
// List active runs for a branch
// ---------------------------------------------------------------------------

/// Filter for [`list_remote_tracking_merge_runs_for_branch`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MergeRunStatusFilter {
    /// Default. Only runs in an open/waiting/pending/rolling-back state —
    /// these are the actionable runs the merge-required UI needs.
    Active,
    /// All runs, including completed/cancelled/failed — for audit views.
    All,
}

/// List `workspace_merge_runs` rows with `source_kind = 'remote_tracking'`
/// bound to the selected canonical workspace/repository membership. When
/// `include_coordinated_reset` is true, the result also contains every active
/// reset-below-baseline run in the authorized workspace. That opt-in expansion
/// lets the workspace-level approved-publication action display and bind its
/// complete checkpoint set without changing ordinary branch-scoped callers.
///
/// Newest-first by `started_at`. The default `Active` filter excludes
/// terminal states (`completed`, `cancelled`, `failed`); `All` returns
/// the union plus terminals.
pub async fn list_remote_tracking_merge_runs_for_branch(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    repo_id: Uuid,
    filter: MergeRunStatusFilter,
    include_coordinated_reset: bool,
) -> Result<Vec<WorkspaceMergeRunSummary>, AppError> {
    #[derive(sqlx::FromRow)]
    struct Row {
        id: Uuid,
        workspace_id: Uuid,
        source_kind: String,
        source_workspace_id: Option<Uuid>,
        target_workspace_id: Option<Uuid>,
        resolution_task_id: Option<Uuid>,
        resolution_session_id: Option<Uuid>,
        resolution_task_status: Option<String>,
        source_branch_name: String,
        status: String,
        phase: String,
        blocking_reason: Option<String>,
        repo_id: Option<Uuid>,
        source_heads: Option<serde_json::Value>,
        target_heads: Option<serde_json::Value>,
        started_at: Option<chrono::DateTime<chrono::Utc>>,
    }

    let active_statuses: &[&str] = &["waiting_for_human", "running", "pending", "rolling_back"];

    // Build the status predicate inline so we keep one query path and let
    // PostgreSQL plan the IN(...) list. `ANY($2::text[])` would also work
    // but requires bind-array support; the simpler form is plenty fast at
    // the cardinality this endpoint sees.
    let rows: Vec<Row> = match filter {
        MergeRunStatusFilter::Active => sqlx::query_as(
            r#"
            SELECT
                jmr.id,
                jmr.workspace_id,
                jmr.source_kind,
                jmr.source_workspace_id,
                jmr.target_workspace_id,
                jmr.resolution_task_id,
                resolution.chat_session_id AS resolution_session_id,
                resolution.status AS resolution_task_status,
                jmr.source_branch_name,
                jmr.status,
                jmr.phase,
                jmr.blocking_reason,
                jmr.repo_id,
                jmr.source_heads,
                jmr.target_heads,
                jmr.started_at
            FROM workspace_merge_runs jmr
            LEFT JOIN tasks resolution ON resolution.id = jmr.resolution_task_id
              AND resolution.org_id = jmr.org_id AND resolution.workspace_id = jmr.workspace_id
            WHERE jmr.workspace_id = $4 AND (
                    jmr.repo_id = $1
                    OR (
                        $3
                        AND jmr.workspace_id = $4
                        AND jmr.phase = 'reset_below_baseline'
                    )
                  )
              AND jmr.source_kind = 'remote_tracking'
              AND jmr.status = ANY($2)
            ORDER BY jmr.started_at DESC NULLS LAST, jmr.id DESC
            "#,
        )
        .bind(repo_id)
        .bind(active_statuses)
        .bind(include_coordinated_reset)
        .bind(workspace_id)
        .fetch_all(&mut *conn)
        .await
        .map_err(AppError::from)?,
        MergeRunStatusFilter::All => sqlx::query_as(
            r#"
            SELECT
                jmr.id,
                jmr.workspace_id,
                jmr.source_kind,
                jmr.source_workspace_id,
                jmr.target_workspace_id,
                jmr.resolution_task_id,
                resolution.chat_session_id AS resolution_session_id,
                resolution.status AS resolution_task_status,
                jmr.source_branch_name,
                jmr.status,
                jmr.phase,
                jmr.blocking_reason,
                jmr.repo_id,
                jmr.source_heads,
                jmr.target_heads,
                jmr.started_at
            FROM workspace_merge_runs jmr
            LEFT JOIN tasks resolution ON resolution.id = jmr.resolution_task_id
              AND resolution.org_id = jmr.org_id AND resolution.workspace_id = jmr.workspace_id
            WHERE jmr.workspace_id = $3 AND (
                    jmr.repo_id = $1
                    OR (
                        $2
                        AND jmr.workspace_id = $3
                        AND jmr.phase = 'reset_below_baseline'
                        AND jmr.status = ANY($4)
                    )
                  )
              AND jmr.source_kind = 'remote_tracking'
            ORDER BY jmr.started_at DESC NULLS LAST, jmr.id DESC
            "#,
        )
        .bind(repo_id)
        .bind(include_coordinated_reset)
        .bind(workspace_id)
        .bind(active_statuses)
        .fetch_all(&mut *conn)
        .await
        .map_err(AppError::from)?,
    };

    Ok(rows
        .into_iter()
        .map(|r| WorkspaceMergeRunSummary {
            id: r.id,
            workspace_id: r.workspace_id,
            source_kind: r.source_kind,
            source_workspace_id: r.source_workspace_id,
            target_workspace_id: r.target_workspace_id,
            resolution_task_id: r.resolution_task_id,
            resolution_session_id: r.resolution_session_id,
            resolution_task_status: r.resolution_task_status,
            source_branch_name: r.source_branch_name,
            status: r.status,
            phase: r.phase,
            blocking_reason: r.blocking_reason,
            repo_id: r.repo_id,
            source_heads: r.source_heads,
            target_heads: r.target_heads,
            started_at: r.started_at,
            approval_admission: None,
        })
        .collect())
}

// ---------------------------------------------------------------------------
// Start in-app merge resolution (G3)
// ---------------------------------------------------------------------------

/// Captured operation evidence joined to its current integration policy and
/// workspace during start-time validation.
#[derive(Debug, Clone, sqlx::FromRow)]
struct StartContextRow {
    org_id: Uuid,
    workspace_id: Uuid,
    status: String,
    /// Divergence phase (`workspace_rebase_preflight` | `reset_below_baseline`).
    /// The server-side direct_clean fast path is gated to
    /// `workspace_rebase_preflight` — a `reset_below_baseline` watermark violation
    /// is a history rewrite that design §8 forbids silently reconciling.
    phase: String,
    resolution_task_id: Option<Uuid>,
    repo_id: Uuid,
    local_branch_name: String,
    remote_branch: Option<String>,
    #[sqlx(skip)]
    local_tip: String,
    #[sqlx(skip)]
    watermark: Option<String>,
    source_heads: Option<serde_json::Value>,
}

#[derive(Debug, Clone)]
struct CapturedRemoteTrackingMergeStart {
    ctx: StartContextRow,
    workspace_snapshot: CommittedBindingSnapshot,
    previous_mirror_tip: Option<String>,
    policy: crate::services::remote_integrations::RemoteIntegration,
}

/// Compare the authority-bearing parts of two committed workspace reads.
/// `snapshot_at` records when branch refs were observed, so two reads of the
/// same committed cut necessarily carry different timestamps. Provider poll
/// observations are attached diagnostics rather than workspace authority.
fn same_workspace_authority_cut(
    captured: &CommittedBindingSnapshot,
    current: &CommittedBindingSnapshot,
) -> bool {
    captured.target == current.target
        && captured.org_id == current.org_id
        && captured.workspace_id == current.workspace_id
        && captured.source == current.source
        && captured.repos.len() == current.repos.len()
        && captured
            .repos
            .iter()
            .zip(&current.repos)
            .all(|(captured, current)| {
                captured.repo_id == current.repo_id
                    && captured.workspace_id == current.workspace_id
                    && captured.branch_name == current.branch_name
                    && captured.canonical_ref == current.canonical_ref
                    && captured.head_commit == current.head_commit
            })
}

async fn capture_remote_tracking_merge_start(
    conn: &mut PgConnection,
    repo_storage: &ScopedRepoStorage,
    merge_run_id: Uuid,
) -> Result<CapturedRemoteTrackingMergeStart, AppError> {
    let ctx: Option<StartContextRow> = sqlx::query_as(
        r#"
        SELECT
            jmr.org_id,
            jmr.workspace_id,
            jmr.status,
            jmr.phase,
            jmr.resolution_task_id,
            jmr.repo_id,
            CASE WHEN w.is_base THEN r.default_branch ELSE w.branch_name END AS local_branch_name,
            integration.remote_branch,
            jmr.source_heads                 AS source_heads
        FROM workspace_merge_runs jmr
        JOIN remote_integrations integration ON (integration.org_id, integration.workspace_id, integration.repo_id) = (jmr.org_id, jmr.workspace_id, jmr.repo_id)
        JOIN workspaces w ON w.id = jmr.workspace_id AND w.org_id = jmr.org_id
        JOIN repos r ON r.id = jmr.repo_id AND r.org_id = jmr.org_id
        WHERE jmr.id = $1
          AND jmr.source_kind = 'remote_tracking'
        "#,
    )
    .bind(merge_run_id)
    .fetch_optional(&mut *conn)
    .await
    .map_err(AppError::from)?;
    let mut ctx = ctx.ok_or_else(|| {
        AppError::from_error(G5Error::NotFound {
            resource: format!("remote_tracking merge run {merge_run_id}"),
        })
    })?;
    validate_remote_tracking_merge_start_status(&ctx)?;
    if ctx.remote_branch.is_none() {
        return Err(AppError::from_error(G5Error::ValidationError {
            details: json!({
                "message": "remote integration has no selected remote branch",
                "merge_run_id": merge_run_id,
            })
            .to_string()
            .into(),
        }));
    }

    let workspace_snapshot = crate::services::workspace_reader::resolve_published_snapshot_on(
        conn,
        repo_storage,
        AuthorityTarget::Workspace {
            workspace_id: ctx.workspace_id,
        },
    )
    .await
    .map_err(AppError::from)?;
    let tracked_repo = workspace_snapshot.repo(ctx.repo_id).map_err(|message| {
        AppError::from_error(G5Error::Conflict {
            reason: format!(
                "Remote-tracking workspace {} has no exact authority member for repo {}: {message}",
                ctx.workspace_id, ctx.repo_id
            ),
        })
    })?;
    ctx.local_tip = tracked_repo.head_commit.clone();
    let path = repo_storage
        .repo_path(workspace_snapshot.org_id, ctx.repo_id)
        .map_err(AppError::from_error)?;
    ctx.watermark = GitRepo::open(&path)?.read_sync_watermark(&ctx.local_branch_name)?;

    let previous_mirror_tip = ctx
        .source_heads
        .as_ref()
        .and_then(|value| value.get("mirror_tip"))
        .and_then(|value| value.as_str())
        .map(str::to_owned);
    let policy = crate::services::remote_integrations::load(
        conn,
        WorkspaceRepoKey {
            org_id: ctx.org_id,
            workspace_id: ctx.workspace_id,
            repo_id: ctx.repo_id,
        },
        false,
    )
    .await?
    .ok_or_else(|| {
        AppError::from_error(G5Error::Conflict {
            reason: "Remote integration was removed".into(),
        })
    })?;
    Ok(CapturedRemoteTrackingMergeStart {
        ctx,
        workspace_snapshot,
        previous_mirror_tip,
        policy,
    })
}

fn validate_remote_tracking_merge_start_status(ctx: &StartContextRow) -> Result<(), AppError> {
    if ctx.resolution_task_id.is_some() || ctx.status == "running" {
        return Err(AppError::from_error(G5Error::TaskNotEligible {
            current_status: ctx.status.clone(),
            requested_status: "remote_tracking_merge_start".to_string(),
        }));
    }
    if !(ctx.status == "waiting_for_human" || ctx.status == "pending") {
        return Err(AppError::from_error(G5Error::TaskNotEligible {
            current_status: ctx.status.clone(),
            requested_status: "remote_tracking_merge_start".to_string(),
        }));
    }
    Ok(())
}

fn resolve_remote_tracking_mirror_tip(
    repo_storage: &TenantRepoStorage,
    captured: &CapturedRemoteTrackingMergeStart,
) -> Result<String, AppError> {
    let remote_branch = captured.ctx.remote_branch.as_deref().ok_or_else(|| {
        AppError::from_error(G5Error::ValidationError {
            details: json!({
                "message": "remote_tracking merge capture is missing its exact remote branch",
                "workspace_id": captured.ctx.workspace_id,
                "repo_id": captured.ctx.repo_id,
            })
            .to_string()
            .into(),
        })
    })?;
    // Reconciliation is bound to the captured operation input, never a mutable
    // remote-tracking ref or a later provider observation.
    let _ = (repo_storage, remote_branch);
    captured.previous_mirror_tip.clone().ok_or_else(|| {
        AppError::from_error(G5Error::Conflict {
            reason: "Remote reconciliation has no captured remote tip".into(),
        })
    })
}

/// Start explicit resolution of an automatically detected divergence. Passive
/// sync can pause without a workflow; the owner action creates or reuses one
/// under the same workspace fence used by integration and resolution starts.
pub async fn start_remote_tracking_resolution(
    state: &crate::services::AppState,
    scope: &ScopedDbReopener,
    key: WorkspaceRepoKey,
    actor_user_id: Uuid,
    agent_launch: Option<&g5_contracts::types::AgentLaunchSelectionV1>,
) -> Result<WorkspaceMergeRunSummary, AppError> {
    let mut db = scope.open().await.map_err(AppError::from)?;
    acquire_workspace_commit_fence(db.conn(), key.workspace_id).await?;
    crate::services::workspace_mutation_policy::ensure_workspace_owner(&mut db, key.workspace_id)
        .await?;
    let active = list_remote_tracking_merge_runs_for_branch(
        db.conn(),
        key.workspace_id,
        key.repo_id,
        MergeRunStatusFilter::Active,
        false,
    )
    .await?;
    let run_id = if let Some(run) = active.into_iter().next() {
        if run.phase == "reset_below_baseline" {
            // A concurrent refresh discovered a rewrite. Surface its approval
            // checkpoint instead of interpreting Resolve as rewrite approval.
            db.commit().await.map_err(AppError::from)?;
            return Ok(run);
        }
        run.id
    } else {
        let policy = super::load(db.conn(), key, true).await?.ok_or_else(|| {
            AppError::from_error(G5Error::Conflict {
                reason: "Remote integration was removed".into(),
            })
        })?;
        if policy.is_base {
            // Base has no VM to run the resolution agent (g5.workspace.remote.base-parity).
            return Err(G5Error::Conflict {
                reason: "Base has no VM to resolve divergence; fork it to reconcile the history"
                    .into(),
            }
            .into());
        }
        if policy.sync_paused_reason.as_deref() != Some("diverged") {
            return Err(G5Error::Conflict {
                reason: "This workspace repository has no provider divergence to resolve".into(),
            }
            .into());
        }
        let snapshot = resolve_published_snapshot(
            &mut db,
            &state.repo_storage,
            AuthorityTarget::Workspace {
                workspace_id: key.workspace_id,
            },
        )
        .await?;
        let member = snapshot
            .repo(key.repo_id)
            .map_err(|reason| G5Error::Conflict {
                reason: reason.to_string(),
            })?;
        let path = state.repo_storage.repo_path(&db, key.org_id, key.repo_id)?;
        let git = GitRepo::open(&path)?;
        let watermark = git.read_sync_watermark(&policy.local_branch)?;
        let provider_tip = git
            .read_provider_observation(key.workspace_id, &policy.remote_branch)?
            .ok_or_else(|| G5Error::Conflict {
                reason: "Provider observation is missing; fetch from remote before resolving"
                    .into(),
            })?;
        // Retain the observed inputs so a failed agent launch still leaves a
        // usable manual-resolution record. The shared start path fetches fresh
        // provider input and validates policy, Git heads, ownership, the running
        // VM and launch settings before admitting an agent task.
        open_remote_tracking_merge_run(
            db.conn(),
            key.workspace_id,
            key.repo_id,
            &policy.remote_branch,
            DivergedPhase::WorkspaceRebasePreflight,
            &member.head_commit,
            Some(&provider_tip),
            watermark
                .as_deref()
                .unwrap_or(crate::services::branch_commit::ZERO_GIT_SHA),
        )
        .await?
    };
    db.commit().await.map_err(AppError::from)?;
    start_remote_tracking_merge(state, scope, run_id, actor_user_id, agent_launch).await
}

/// Reconcile provider changes in an ordinary workspace
/// agent. The normal task dispatcher owns launch, retries, events and recovery.
pub async fn start_remote_tracking_merge(
    state: &crate::services::AppState,
    scope: &ScopedDbReopener,
    merge_run_id: Uuid,
    actor_user_id: Uuid,
    agent_launch: Option<&g5_contracts::types::AgentLaunchSelectionV1>,
) -> Result<WorkspaceMergeRunSummary, AppError> {
    start_divergence_run(
        state,
        scope,
        merge_run_id,
        actor_user_id,
        DivergenceStart::Agent(agent_launch),
    )
    .await
}

/// What an explicit owner start does with a divergence run once its inputs
/// are fresh, fenced and revalidated in the running owner VM.
#[derive(Clone, Copy)]
enum DivergenceStart<'a> {
    /// Hand the run to a resolution agent.
    Agent(Option<&'a g5_contracts::types::AgentLaunchSelectionV1>),
    /// Reset the checkout to the reviewed provider tip
    /// (`g5.workspace.remote.adopt-provider-history`).
    Adopt {
        provider_tip: &'a str,
        workspace_head: &'a str,
    },
}

/// Adopt the reviewed provider history for the repository's open divergence
/// run in an ordinary workspace (`g5.workspace.remote.adopt-provider-history`).
/// The owner VM resets the member branch to `provider_tip` and publishes it to
/// G5 under a lease on `workspace_head`; nothing reaches the provider. The
/// worker then completes the run through the ordinary completion proof.
pub async fn adopt_remote_tracking_provider_history(
    state: &crate::services::AppState,
    scope: &ScopedDbReopener,
    key: WorkspaceRepoKey,
    actor_user_id: Uuid,
    provider_tip: &str,
    workspace_head: &str,
) -> Result<WorkspaceMergeRunSummary, AppError> {
    let run_id = adoptable_divergence_run(scope, key).await?;
    start_divergence_run(
        state,
        scope,
        run_id,
        actor_user_id,
        DivergenceStart::Adopt {
            provider_tip,
            workspace_head,
        },
    )
    .await
}

/// The repository's open divergence run that adopting may settle: one no
/// agent is resolving and that is not a provider-rewrite checkpoint.
pub(crate) async fn adoptable_divergence_run(
    scope: &ScopedDbReopener,
    key: WorkspaceRepoKey,
) -> Result<Uuid, AppError> {
    let mut db = scope.open().await.map_err(AppError::from)?;
    let active = list_remote_tracking_merge_runs_for_branch(
        db.conn(),
        key.workspace_id,
        key.repo_id,
        MergeRunStatusFilter::Active,
        false,
    )
    .await?;
    db.rollback().await.map_err(AppError::from)?;
    let Some(run) = active.into_iter().next() else {
        return Err(G5Error::Conflict {
            reason:
                "This repository has no open divergence to settle by adopting the provider history"
                    .into(),
        }
        .into());
    };
    if run.phase == "reset_below_baseline" {
        return Err(G5Error::Conflict {
            reason: "The provider rewrote its history; review its rewrite approval instead".into(),
        }
        .into());
    }
    if run.resolution_task_id.is_some() || run.status == "running" {
        return Err(G5Error::Conflict {
            reason: "An agent is already resolving this divergence".into(),
        }
        .into());
    }
    Ok(run.id)
}

async fn start_divergence_run(
    state: &crate::services::AppState,
    scope: &ScopedDbReopener,
    merge_run_id: Uuid,
    actor_user_id: Uuid,
    action: DivergenceStart<'_>,
) -> Result<WorkspaceMergeRunSummary, AppError> {
    // A run already handed to an agent or completed answers an agent start
    // idempotently; adopting would discard history someone else is settling.
    let settled = |summary: WorkspaceMergeRunSummary| match action {
        DivergenceStart::Agent(_) => Ok(summary),
        DivergenceStart::Adopt { .. } => Err(AppError::from_error(G5Error::Conflict {
            reason: "This divergence is already being resolved or was resolved".into(),
        })),
    };
    let mut db = scope.open().await.map_err(AppError::from)?;
    let initial = remote_merge_summary(db.conn(), merge_run_id).await?;
    db.rollback().await.map_err(AppError::from)?;
    if initial.resolution_task_id.is_some() || initial.status == "completed" {
        return settled(initial);
    }
    let mut capture_db = scope.open().await.map_err(AppError::from)?;
    crate::services::workspace_mutation_policy::ensure_workspace_owner(
        &mut capture_db,
        initial.workspace_id,
    )
    .await?;
    let capture_repos = state.repo_storage.scoped(&capture_db);
    let mut captured =
        match capture_remote_tracking_merge_start(capture_db.conn(), &capture_repos, merge_run_id)
            .await
        {
            Ok(captured) => captured,
            Err(error) => {
                let latest = remote_merge_summary(capture_db.conn(), merge_run_id).await?;
                if latest.resolution_task_id.is_some() || latest.status == "completed" {
                    capture_db.rollback().await.map_err(AppError::from)?;
                    return settled(latest);
                }
                return Err(error);
            }
        };
    capture_db.rollback().await.map_err(AppError::from)?;
    // A waiting run has not started reconciliation yet. Capture a fresh provider
    // tip through the same pull abstraction as Base and background integration;
    // mutable origin refs and cached observations are never start authority.
    let auth = state
        .repository_credentials
        .resolve(
            captured.ctx.org_id,
            captured.ctx.repo_id,
            actor_user_id,
            &captured.policy.git_url,
            crate::auth::RepositoryOperation::ReadGit,
        )
        .await?;
    let remote = crate::services::remote_integrations::prepare_pull(
        &state.repo_storage,
        crate::services::remote_integrations::PullInput {
            key: TenantRepoStorage::key(scope, captured.ctx.org_id, captured.ctx.repo_id)?,
            git_url: captured.policy.git_url.clone(),
            remote_branch: captured.policy.remote_branch.clone(),
            local_head: captured.ctx.local_tip.clone(),
            watermark: captured.ctx.watermark.clone(),
            observed_remote_tip: None,
        },
        auth,
    )
    .await?;
    if remote.relation
        == crate::services::branch_integration_service::TipRelation::WatermarkViolation
    {
        // A paused divergence is not polled again. Supersede its evidence with
        // an explicit approval checkpoint so a provider rewrite cannot leave
        // the owner stuck behind a Resolve action that can never succeed.
        let mut db = scope.open().await.map_err(AppError::from)?;
        acquire_workspace_commit_fence(db.conn(), initial.workspace_id)
            .await
            .map_err(AppError::from)?;
        sqlx::query("SELECT id FROM workspace_merge_runs WHERE id=$1 FOR UPDATE")
            .bind(merge_run_id)
            .fetch_one(db.conn())
            .await?;
        let latest = remote_merge_summary(db.conn(), merge_run_id).await?;
        if latest.resolution_task_id.is_some() || latest.status == "completed" {
            db.commit().await.map_err(AppError::from)?;
            return settled(latest);
        }
        crate::services::workspace_mutation_policy::ensure_workspace_owner(
            &mut db,
            initial.workspace_id,
        )
        .await?;
        validate_start_policy(db.conn(), &captured).await?;
        let repos = state.repo_storage.scoped(&db);
        let current = capture_remote_tracking_merge_start(db.conn(), &repos, merge_run_id).await?;
        if !same_workspace_authority_cut(&captured.workspace_snapshot, &current.workspace_snapshot)
            || captured.ctx.phase != current.ctx.phase
            || captured.ctx.watermark != current.ctx.watermark
        {
            return Err(AppError::from_error(G5Error::Conflict {
                reason: "Remote rewrite inputs changed during preparation".into(),
            }));
        }
        sqlx::query("UPDATE workspace_merge_runs SET status='cancelled', blocking_reason='provider_rewrote_before_resolution', completed_at=now(), updated_at=now() WHERE id=$1")
            .bind(merge_run_id).execute(db.conn()).await?;
        let watermark = captured.ctx.watermark.as_deref().ok_or_else(|| {
            AppError::from_error(G5Error::Conflict {
                reason: "Remote rewrite has no captured synchronization watermark".into(),
            })
        })?;
        let checkpoint = open_remote_tracking_merge_run(
            db.conn(),
            initial.workspace_id,
            captured.ctx.repo_id,
            &captured.policy.remote_branch,
            DivergedPhase::ResetBelowBaseline,
            &captured.ctx.local_tip,
            Some(&remote.remote_tip),
            watermark,
        )
        .await?;
        sqlx::query("UPDATE remote_integrations SET sync_paused_reason='watermark_violation', updated_at=now() WHERE workspace_id=$1 AND repo_id=$2")
            .bind(initial.workspace_id).bind(captured.ctx.repo_id).execute(db.conn()).await?;
        let summary = remote_merge_summary(db.conn(), checkpoint).await?;
        db.commit().await.map_err(AppError::from)?;
        return match action {
            DivergenceStart::Agent(_) => Ok(summary),
            DivergenceStart::Adopt { .. } => Err(AppError::from_error(G5Error::Conflict {
                reason: "The provider rewrote its history; review its rewrite approval instead"
                    .into(),
            })),
        };
    }
    captured.previous_mirror_tip = Some(remote.remote_tip);
    // No ensure/start call: resolving conflicts is an owner action in an
    // already-running workspace. Resolve the handle before taking SQL locks.
    let handle = state
        .workspace_runtime
        .owner_handle(initial.workspace_id, actor_user_id)
        .await
        .map_err(AppError::from)?;
    let mut db = scope.open().await.map_err(AppError::from)?;
    acquire_workspace_commit_fence(db.conn(), initial.workspace_id)
        .await
        .map_err(AppError::from)?;
    sqlx::query("SELECT id FROM workspace_merge_runs WHERE id=$1 FOR UPDATE")
        .bind(merge_run_id)
        .fetch_one(db.conn())
        .await?;
    let current = remote_merge_summary(db.conn(), merge_run_id).await?;
    if current.resolution_task_id.is_some() || current.status == "completed" {
        db.commit().await.map_err(AppError::from)?;
        return settled(current);
    }
    crate::services::workspace_mutation_policy::ensure_workspace_owner(
        &mut db,
        initial.workspace_id,
    )
    .await?;
    state
        .workspace_runtime
        .lock_owner_handle(&mut db, &handle)
        .await?;
    let repos = state.repo_storage.scoped(&db);
    let current_capture =
        capture_remote_tracking_merge_start(db.conn(), &repos, merge_run_id).await?;
    validate_start_policy(db.conn(), &captured).await?;
    if !same_workspace_authority_cut(
        &captured.workspace_snapshot,
        &current_capture.workspace_snapshot,
    ) || current_capture.ctx.phase != captured.ctx.phase
        || current_capture.ctx.watermark != captured.ctx.watermark
    {
        return Err(AppError::from_error(G5Error::Conflict {
            reason: "Conflict resolution inputs changed during preparation".into(),
        }));
    }
    if captured.ctx.phase != "workspace_rebase_preflight" {
        return Err(AppError::from_error(G5Error::Conflict {
            reason: "Remote history rewrites require their explicit approval checkpoint".into(),
        }));
    }
    let remote_tip = resolve_remote_tracking_mirror_tip(&state.repo_storage, &captured)?;
    let member = captured
        .workspace_snapshot
        .repo(captured.ctx.repo_id)
        .map_err(|reason| {
            AppError::from_error(G5Error::Conflict {
                reason: reason.to_string(),
            })
        })?;
    let inputs = crate::services::remote_integrations::ResolutionInputs {
        run_id: merge_run_id,
        org_id: captured.ctx.org_id,
        workspace_id: initial.workspace_id,
        repo_id: captured.ctx.repo_id,
        actor_user_id,
        root_path: member.root_path.clone(),
        local_branch: captured.ctx.local_branch_name.clone(),
        local_tip: captured.ctx.local_tip.clone(),
        remote_tip,
    };
    let (provider_tip, workspace_head) = match action {
        DivergenceStart::Agent(agent_launch) => {
            crate::services::remote_integrations::enqueue_resolution(
                state,
                &mut db,
                inputs,
                agent_launch,
            )
            .await?;
            let summary = remote_merge_summary(db.conn(), merge_run_id).await?;
            db.commit().await.map_err(AppError::from)?;
            return Ok(summary);
        }
        DivergenceStart::Adopt {
            provider_tip,
            workspace_head,
        } => (provider_tip, workspace_head),
    };
    if inputs.remote_tip != provider_tip || inputs.local_tip != workspace_head {
        return Err(AppError::from_error(G5Error::ValidationError {
            details: json!({
                "code": "PROVIDER_HISTORY_CHANGED",
                "operation": "accept_provider_history",
                "message": "The provider branch or this workspace changed since you reviewed it. Review the provider history again.",
            }),
        }));
    }
    // The pins keep the replaced head recoverable and carry the reviewed tip
    // to the checkout. The guest push takes the fence itself, so it runs
    // after this transaction commits; its lease refuses any landing between.
    crate::services::remote_integrations::pin_adoption_inputs(state, &db, &inputs)?;
    db.commit().await.map_err(AppError::from)?;
    let report = state
        .workspace_runtime
        .workspace_changes_adopt(
            &handle,
            actor_user_id,
            inputs.repo_id,
            merge_run_id,
            &inputs.local_tip,
            &inputs.remote_tip,
        )
        .await?;
    let member = &report["members"][0];
    if member["adopted"] != true {
        let code = member["errors"][0].as_str().unwrap_or("ADOPT_FAILED");
        return Err(adoption_refusal(code, &inputs.root_path));
    }
    let mut db = scope.open().await.map_err(AppError::from)?;
    let summary = remote_merge_summary(db.conn(), merge_run_id).await?;
    db.rollback().await.map_err(AppError::from)?;
    Ok(summary)
}

/// Name a refused in-VM adoption in the owner's terms. Nothing changed.
fn adoption_refusal(code: &str, repository: &str) -> AppError {
    let action = match code {
        "INTERNAL_BRANCH_MOVED" | "PROVIDER_INPUT_MOVED" => {
            return AppError::from_error(G5Error::ValidationError {
                details: json!({
                    "code": "PROVIDER_HISTORY_CHANGED",
                    "operation": "accept_provider_history",
                    "message": format!("{repository} changed while adopting the provider history. Review it again."),
                }),
            });
        }
        "DIRTY_WORKING_TREE" | "STATUS_UNAVAILABLE" => {
            "has uncommitted changes in the workspace; save or discard them, then adopt again"
        }
        "UNPUSHED_COMMITS" => {
            "has workspace commits that are not saved to G5; push or discard them, then adopt again"
        }
        "INTERRUPTED_SEQUENCER" => {
            "has a Git merge or rebase in progress in the workspace; finish or abort it, then adopt again"
        }
        "DETACHED_HEAD" | "BRANCH_ASSOCIATION_MISMATCH" => {
            "is not on the workspace branch in the workspace; switch back to it, then adopt again"
        }
        _ => "could not adopt the provider history in the workspace; try again",
    };
    AppError::from_error(G5Error::Conflict {
        reason: format!("Adopting stopped: {repository} {action} ({code}). Nothing changed."),
    })
}

async fn remote_merge_summary(
    conn: &mut PgConnection,
    run_id: Uuid,
) -> Result<WorkspaceMergeRunSummary, AppError> {
    let (workspace_id, repo_id): (Uuid, Uuid) = sqlx::query_as(
        "SELECT workspace_id, repo_id FROM workspace_merge_runs WHERE id=$1 AND source_kind='remote_tracking'")
        .bind(run_id).fetch_optional(&mut *conn).await?.ok_or_else(|| AppError::from_error(G5Error::NotFound {
            resource: format!("remote merge {run_id}"),
        }))?;
    list_remote_tracking_merge_runs_for_branch(
        conn,
        workspace_id,
        repo_id,
        MergeRunStatusFilter::All,
        false,
    )
    .await?
    .into_iter()
    .find(|run| run.id == run_id)
    .ok_or_else(|| {
        AppError::from_error(G5Error::NotFound {
            resource: format!("remote merge {run_id}"),
        })
    })
}

async fn validate_start_policy(
    conn: &mut PgConnection,
    captured: &CapturedRemoteTrackingMergeStart,
) -> Result<(), AppError> {
    let current = crate::services::remote_integrations::load(conn, captured.policy.key(), true)
        .await?
        .ok_or_else(|| {
            AppError::from_error(G5Error::Conflict {
                reason: "Remote integration was removed".into(),
            })
        })?;
    if current.publication_policy() != captured.policy.publication_policy() {
        return Err(AppError::from_error(G5Error::Conflict {
            reason: "Remote integration changed during reconciliation preparation".into(),
        }));
    }
    Ok(())
}
