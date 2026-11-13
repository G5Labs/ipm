//! Remote integration worker. PostgreSQL owns policy, queued intent, leases and
//! exact operation receipts. Git owns the working head and synchronization
//! watermark. Transport runs outside pooled transactions; publication rechecks
//! the complete captured Git vector, current policy and both live leases.
//!
//! Queue cleanup compares the claimed request generation so a newer signal is
//! retained. Background work never bypasses auto-pull or pause policy. Explicit
//! review remains required for history rewrites below the synchronization mark.

use crate::services::git::repo_store::{RepoKey, ScopedRepoStorage, TenantRepoStorage};
use std::sync::Arc;
use std::time::Duration;

use sqlx::PgConnection;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use crate::auth::RepositoryOperation;
use crate::errors::AppError;
use crate::models::branch_integration_queue::PushAuthority;
use crate::models::branch_sync_run::status as run_status;
use crate::services::branch_commit::{
    acquire_workspace_commit_fence, map_branch_commit_error, prepare_branch_commit,
    LandedCommitNotifier, PreparedBranchCommit,
};
use crate::services::branch_integration_service::{
    classify_and_act, Action, ClassifyInput, Decision, DivergedPhase, PushMode, TipRelation,
    TriggerKind,
};
use crate::services::remote_integrations;
use crate::services::remote_tracking_merge::{
    apply_external_remote_tracking_merge_completion, open_remote_tracking_merge_run,
    prepare_external_remote_tracking_merge_completion,
    register_remote_tracking_merge_completion_consumer, CompletionRequester,
    PreparedExternalRemoteTrackingCompletion, RemoteTrackingAuthorityAuditInput,
    RemoteTrackingMergeCompletionConsumerInput,
};
use crate::services::workspace_reader::{resolve_authority_inventory, AuthorityInventorySnapshot};
use crate::services::EventBus;
use g5_auth::credential_scrub::scrub_opt;
use g5_contracts::error::G5Error;
use g5_contracts::types::{AuthorityTarget, WorkspaceHeadRepoInput, WorkspaceRepoKey};
use g5_scm::{GitAuth, GitRepo};
use std::collections::BTreeMap;

use crate::db::maintenance::{scoped_db_for_maintenance_org, TenantMaintenanceDeps};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Maximum time the worker waits for a single `git fetch` before
/// classifying it as `fetch_failed`.
const FETCH_TIMEOUT: Duration = Duration::from_secs(120);

/// How often the worker checks for queued integration work.
const SWEEP_INTERVAL: Duration = Duration::from_secs(5);

/// Internal representation of a missing Git synchronization watermark. The
/// sentinel is converted to None before Git classification or publication and
/// is never written to a Git ref or a SQL current-head field.
const ZERO_GIT_SHA_SENTINEL: &str = "0000000000000000000000000000000000000000";

/// The queue and remote-pair leases outlive every bounded transport operation.
/// Finalization still requires both leases to be live, so an unexpectedly slow
/// cycle fails closed and is retried by a fresh owner.
const CYCLE_LEASE_TTL_SECS: i64 = 600;

/// How long a queue row is deferred when its cycle could not acquire the
/// per-`(repo_id, remote_branch)` lease (another worker/cycle is
/// already integrating that branch). Short enough that the row is picked
/// up again on a subsequent sweep tick (the contender's commit releases
/// the xact lock), long enough that the SAME drain pass does not
/// busy-loop re-picking it. Per the design the deferred row must NOT be
/// starved (it must not inherit the full `CYCLE_LEASE_TTL_SECS` pickup
/// window) and must NOT get an audit row.
const BRANCH_LOCK_DEFER_SECS: i64 = 2;

/// Maximum number of queue rows processed in a single sweep tick.
const MAX_ROWS_PER_SWEEP: usize = 64;

/// Actor recorded on the `workspace.branch_updated` hints of the cuts this
/// worker lands.
const BRANCH_COMMIT_ACTOR: &str = "branch_integration";

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Constructor-style dependency bundle for the worker loop.
pub struct BranchIntegrationWorkerDeps {
    pub system_pool: sqlx::PgPool,
    pub tenant_pool: sqlx::PgPool,
    pub authz: Arc<dyn g5_authz::AuthzService>,
    pub repo_storage: TenantRepoStorage,
    pub repository_credentials: Arc<crate::auth::RepositoryCredentials>,
    pub event_bus: EventBus,
    /// Told after every landing the worker commits.
    pub landed_commit_notifier: Arc<dyn LandedCommitNotifier>,
}

impl BranchIntegrationWorkerDeps {
    fn tenant_deps(&self) -> TenantMaintenanceDeps {
        TenantMaintenanceDeps {
            system_pool: self.system_pool.clone(),
            tenant_pool: self.tenant_pool.clone(),
            authz: self.authz.clone(),
            repo_storage: self.repo_storage.clone(),
        }
    }
}

/// Runs the worker loop until `cancel` fires.
///
/// Designed to be spawned as an independent Tokio task by
/// [`crate::maintenance::MaintenanceRunner::spawn_all`].
pub async fn run_branch_integration_worker(
    deps: BranchIntegrationWorkerDeps,
    cancel: CancellationToken,
) {
    let mut interval = tokio::time::interval(SWEEP_INTERVAL);
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    loop {
        tokio::select! {
            _ = interval.tick() => {
                match do_sweep(&deps).await {
                    Ok(processed) if processed > 0 => {
                        info!(
                            processed,
                            "branch_integration_worker: sweep completed"
                        );
                    }
                    Ok(_) => {
                        debug!("branch_integration_worker: no work");
                    }
                    Err(e) => {
                        error!(error = %e, "branch_integration_worker: sweep failed");
                    }
                }
            }
            _ = cancel.cancelled() => {
                info!("branch_integration_worker: shutting down");
                break;
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Sweep — per-org draining (remote-pair serialization happens inside the cycle)
// ---------------------------------------------------------------------------

async fn do_sweep(deps: &BranchIntegrationWorkerDeps) -> Result<usize, AppError> {
    // No global replica-wide lock here (arch view D7): cross-branch concurrency
    // comes from running multiple worker instances. Queue pickup uses
    // `FOR UPDATE SKIP LOCKED`, while a durable generation-fenced lease admits
    // exactly one cycle for each `(repo_id, remote_branch)` pair.
    drain_all_orgs(deps).await
}

async fn drain_all_orgs(deps: &BranchIntegrationWorkerDeps) -> Result<usize, AppError> {
    let tenant_deps = deps.tenant_deps();
    let org_ids = crate::db::maintenance::maintenance_org_ids(&deps.system_pool)
        .await
        .map_err(AppError::from)?;

    let mut processed = 0;
    for org_id in org_ids {
        match drain_one_org(deps, &tenant_deps, org_id).await {
            Ok(n) => processed += n,
            Err(e) => {
                warn!(
                    org_id = %org_id,
                    error = %e,
                    "branch_integration_worker: org sweep failed"
                );
            }
        }
        if processed >= MAX_ROWS_PER_SWEEP {
            break;
        }
    }
    Ok(processed)
}

async fn drain_one_org(
    deps: &BranchIntegrationWorkerDeps,
    tenant_deps: &TenantMaintenanceDeps,
    org_id: Uuid,
) -> Result<usize, AppError> {
    let mut processed = 0;
    let mut deferred = 0;
    // Hard upper bound on total pickups (completed + failed + deferred) per
    // org per tick. Bounds the worst case where every ready row keeps
    // deferring on the remote-pair lease and its short re-pick window
    // expires before the drain pass finishes — without this, a fast loop
    // could re-pick the same deferred rows repeatedly. A later sweep tick
    // resumes the work.
    let mut pickups = 0usize;
    let max_pickups = MAX_ROWS_PER_SWEEP.saturating_mul(2);

    loop {
        if processed >= MAX_ROWS_PER_SWEEP || pickups >= max_pickups {
            break;
        }

        // Phase 1: short transaction to grab one queue row.
        let picked = {
            let mut db = scoped_db_for_maintenance_org(tenant_deps, org_id).await?;
            let picked = grab_one_queue_row(db.conn()).await?;
            db.commit().await.map_err(AppError::from)?;
            picked
        };

        let Some(picked) = picked else {
            break;
        };
        pickups += 1;

        // A landed resolution clears its divergence pause before the claim
        // loads the cycle state, so this same wake then runs as an ordinary
        // cycle under the member's push policy. A proven completion that could
        // not be applied keeps the wake for a bounded number of backed-off
        // retries; a terminal failure, or the last retry, is recorded on the
        // run for the owner and the wake proceeds as it would while paused.
        if let AutoCompletion::Unapplied {
            merge_run_id,
            error,
            retryable,
        } = auto_complete_proven_divergence(deps, tenant_deps, org_id, &picked).await
        {
            let mut db = scoped_db_for_maintenance_org(tenant_deps, org_id).await?;
            let retry_scheduled =
                retryable && defer_auto_completion_retry(db.conn(), &picked, &error).await?;
            if !retry_scheduled {
                record_auto_completion_failure(db.conn(), merge_run_id, &error).await?;
            }
            db.commit().await.map_err(AppError::from)?;
            if retry_scheduled {
                deferred += 1;
                continue;
            }
        }

        // Phase 2: load the exact DB cut and claim the shared remote pair in a
        // short transaction. No database connection survives this block.
        let claim = {
            let mut db = scoped_db_for_maintenance_org(tenant_deps, org_id).await?;
            let repos = deps.repo_storage.scoped(&db);
            let claim = claim_queue_row(deps, &repos, db.conn(), &picked).await?;
            db.commit().await.map_err(AppError::from)?;
            claim
        };
        let CycleClaimOutcome::Claimed {
            state,
            lease,
            approved_rebase_proof,
            resume_pause_only,
            reclassify_checkpoint,
        } = claim
        else {
            match claim {
                CycleClaimOutcome::Completed => processed += 1,
                CycleClaimOutcome::Deferred => deferred += 1,
                CycleClaimOutcome::Claimed { .. } => unreachable!("matched above"),
            }
            continue;
        };

        // Phase 3: bounded credential and Git work with no open database
        // transaction. The durable pair lease serializes a shared upstream.
        let expected_state = (*state).clone();
        let prepared = match prepare_claimed_cycle(
            deps,
            &picked,
            *state,
            *approved_rebase_proof,
            resume_pause_only,
            reclassify_checkpoint,
        )
        .await
        {
            Ok(prepared) => prepared,
            Err(error) if is_stale_prepared_cut(&error) => {
                // A concurrent cycle moved the workspace between capture and
                // preparation. Retry with fresh state on the next sweep rather
                // than after lease expiry.
                debug!(
                    workspace_repo = %picked.key,
                    %error,
                    "branch_integration_worker: prepared cut went stale; retrying promptly"
                );
                abandon_cycle(deps, org_id, &picked, &lease, &[]).await;
                deferred += 1;
                continue;
            }
            Err(error) => {
                error!(
                    workspace_repo = %picked.key,
                    error = %error,
                    "branch_integration_worker: remote preparation failed; leases will expire for retry"
                );
                processed += 1;
                continue;
            }
        };

        let co_members = prepared.co_members().to_vec();

        // Refresh every owner after bounded remote work. If any owner was
        // lost or expired, this transaction rolls back and the stale worker
        // never enters finalization; what it still owns is released so the
        // row retries promptly.
        let renewed = {
            let mut db = scoped_db_for_maintenance_org(tenant_deps, org_id).await?;
            if renew_cycle_leases(db.conn(), &picked, &lease).await?
                && renew_co_member_leases(db.conn(), picked.owner_token, &co_members).await?
            {
                db.commit().await.map_err(AppError::from)?;
                true
            } else {
                drop(db);
                false
            }
        };
        if !renewed {
            abandon_cycle(deps, org_id, &picked, &lease, &co_members).await;
            deferred += 1;
            continue;
        }

        // Phase 4: reacquire a short transaction, fence both lease owners and
        // the exact DB cut, and register any landing consumers. The branch
        // commit always lands from a separately opened untouched transaction
        // after that registration commits.
        let cycle_result: Result<ProcessOutcome, AppError> = {
            let mut db = scoped_db_for_maintenance_org(tenant_deps, org_id).await?;
            let repos = deps.repo_storage.scoped(&db);
            match validate_cycle_claim_for_finalization(
                &repos,
                db.conn(),
                &picked,
                &lease,
                &expected_state,
                &co_members,
            )
            .await?
            {
                CycleClaimValidation::LostLease => {
                    drop(db);
                    abandon_cycle(deps, org_id, &picked, &lease, &co_members).await;
                    Ok(ProcessOutcome::DeferredLeaseHeld)
                }
                CycleClaimValidation::StaleState => {
                    if !release_remote_pair_lease(db.conn(), &lease).await? {
                        return Err(AppError::from_error(G5Error::Internal {
                            message: "Remote-pair lease disappeared while deferring a stale cycle"
                                .to_string(),
                        }));
                    }
                    for co in &co_members {
                        release_remote_pair_lease(db.conn(), &co.lease).await?;
                    }
                    defer_queue_row_for_pair_lease(db.conn(), &picked).await?;
                    db.commit().await.map_err(AppError::from)?;
                    Ok(ProcessOutcome::DeferredLeaseHeld)
                }
                CycleClaimValidation::Valid => 'valid: {
                    let mut completed =
                        apply_prepared_cycle(deps, db.conn(), &picked, prepared).await?;
                    if let Some(advance) = completed.authority_advance.take() {
                        let application_scope = db.reopener();
                        db.commit().await.map_err(AppError::from)?;
                        let application_db =
                            application_scope.open().await.map_err(AppError::from)?;
                        let advance = advance.with_claim(remote_integrations::PublicationClaim {
                            key: picked.key,
                            policy: expected_state.policy.clone(),
                            owner_token: picked.owner_token,
                            lease_generation: lease.generation,
                            requested_at: picked.requested_at,
                            queue_advance_id: picked.advance_id,
                            trigger_kind: picked.trigger_kind.as_audit_str().into(),
                            push_authority: picked.explicit_authority(),
                            automatic_import: !matches!(
                                picked.trigger_kind,
                                TriggerKind::Manual
                                    | TriggerKind::Push
                                    | TriggerKind::ApprovedRebasePublish
                            ),
                            co_members: co_members.iter().map(CoMember::claim).collect(),
                        });
                        #[cfg(feature = "test-helpers")]
                        crate::services::git_test_barrier::clone_merge_test_barrier::wait(
                            picked.key.workspace_id,
                        )
                        .await;
                        let landed = match advance
                            .land(application_db, &deps.event_bus, &deps.repo_storage)
                            .await
                        {
                            Ok(landed) => landed,
                            Err(error) => {
                                let error = AppError::from(error);
                                if is_refused_landing(&error) {
                                    // Another writer moved the workspace or the
                                    // claim changed. Retry with fresh state on the
                                    // next sweep rather than after lease expiry.
                                    debug!(
                                        workspace_repo = %picked.key,
                                        %error,
                                        "branch_integration_worker: landing refused; retrying promptly"
                                    );
                                    abandon_cycle(deps, org_id, &picked, &lease, &co_members).await;
                                    break 'valid Ok(ProcessOutcome::DeferredLeaseHeld);
                                }
                                let co_leases: Vec<_> =
                                    co_members.iter().map(|co| &co.lease).collect();
                                release_leases(deps, org_id, &co_leases).await;
                                return Err(error);
                            }
                        };
                        let committed_advance_id = landed.advance_id;
                        deps.landed_commit_notifier
                            .notify_after_landing(&landed, expected_state.acting_user_id)
                            .await;
                        for warning in landed.projection_warnings {
                            warn!(
                                advance_id = %committed_advance_id,
                                %warning,
                                "branch_integration_worker: committed authority projection remains pending"
                            );
                        }
                        let mut cleanup_db =
                            application_scope.open().await.map_err(AppError::from)?;
                        // Co-members' own queue rows stay; their cycles find
                        // the provider tip already imported.
                        for co in &co_members {
                            release_remote_pair_lease(cleanup_db.conn(), &co.lease).await?;
                        }
                        if release_remote_pair_lease(cleanup_db.conn(), &lease).await? {
                            if picked.trigger_kind == TriggerKind::ApprovedRebasePublish {
                                rearm_committed_approved_rebase_projection(
                                    cleanup_db.conn(),
                                    &picked,
                                    expected_state.org_id,
                                    committed_advance_id,
                                )
                                .await?;
                            } else {
                                delete_queue_row(cleanup_db.conn(), &picked).await?;
                                sqlx::query("UPDATE remote_integration_queue SET locked_by = NULL, locked_until = NULL
                                             WHERE workspace_id = $1 AND repo_id = $2 AND locked_by = $3")
                                    .bind(picked.key.workspace_id).bind(picked.key.repo_id).bind(picked.owner_token)
                                    .execute(cleanup_db.conn()).await?;
                            }
                            cleanup_db.commit().await.map_err(AppError::from)?;
                        } else {
                            warn!(
                                workspace_repo = %picked.key,
                                "branch_integration_worker: committed authority but lease ownership changed before queue cleanup; leaving the row for exact-cut reconciliation"
                            );
                        }
                    } else {
                        if !release_remote_pair_lease(db.conn(), &lease).await? {
                            return Err(AppError::from_error(G5Error::Internal {
                                message:
                                    "Remote-pair lease disappeared during fenced cycle finalization"
                                        .to_string(),
                            }));
                        }
                        let refs = apply_cycle_watermarks(
                            &deps.repo_storage.scoped(&db),
                            &expected_state,
                            &completed.tracking_ref_writes,
                            completed.provider_tip.as_deref(),
                        )?;
                        if let Err(error) = db.commit().await {
                            if matches!(
                                &error,
                                g5_authz::ScopedDbError::ClearFailed(_)
                                    | g5_authz::ScopedDbError::CommitFailed(sqlx::Error::Database(
                                        _
                                    ))
                            ) {
                                rollback_cycle_watermarks(&refs);
                            }
                            return Err(AppError::from(error));
                        }
                    }
                    Ok(ProcessOutcome::Completed(Box::new(completed)))
                }
            }
        };

        match cycle_result {
            Ok(ProcessOutcome::Completed(_completed)) => {
                processed += 1;
            }
            Ok(ProcessOutcome::DeferredLeaseHeld) => {
                // Not counted as processed work — the cycle did nothing but
                // reschedule the row. The short `BRANCH_LOCK_DEFER_SECS`
                // window stamped on the row keeps this same drain pass from
                // re-picking it (so no busy loop); a later sweep retries.
                deferred += 1;
            }
            Err(e) => {
                warn!(
                    workspace_repo = %picked.key,
                    error = %e,
                    "branch_integration_worker: cycle failed (will retry after lease expiry)"
                );
                processed += 1;
            }
        }
    }

    if deferred > 0 {
        debug!(
            org_id = %org_id,
            deferred,
            "branch_integration_worker: rows deferred on the per-(repo_id, remote_branch) \
             lease; will retry on a later sweep"
        );
    }

    Ok(processed)
}

/// Wakes retried after a proven but unapplied automatic completion, before
/// the failure is surfaced on the run and the wake proceeds paused.
const AUTO_COMPLETION_MAX_ATTEMPTS: i32 = 5;

#[derive(Debug)]
enum AutoCompletion {
    /// No open divergence run, not proven, or already completed.
    Settled,
    /// The proof passed (or could not be evaluated) but the completion was not
    /// applied. `retryable` failures may succeed on a later sweep: the cut moved
    /// between preparation and apply, or a service or database was briefly
    /// unavailable. Anything else is terminal.
    Unapplied {
        merge_run_id: Uuid,
        error: String,
        retryable: bool,
    },
}

fn is_retryable_completion_failure(error: &AppError) -> bool {
    matches!(
        error.inner,
        G5Error::Conflict { .. }
            | G5Error::ServiceUnavailable { .. }
            | G5Error::DatabaseRetryable { .. }
    )
}

/// Complete the member's open divergence run when the current workspace head
/// passes the same completion proof as an owner's "Mark resolved"
/// (g5.workspace.remote.divergence-auto-complete). A head that does not pass
/// leaves the run open. Failures here never fail the cycle.
async fn auto_complete_proven_divergence(
    deps: &BranchIntegrationWorkerDeps,
    tenant_deps: &TenantMaintenanceDeps,
    org_id: Uuid,
    picked: &PickedQueueRow,
) -> AutoCompletion {
    // Only an accepted update or an owner request can carry a resolution.
    if matches!(
        picked.trigger_kind,
        TriggerKind::BackgroundInbound | TriggerKind::ApprovedRebasePublish
    ) {
        return AutoCompletion::Settled;
    }
    let requester = CompletionRequester::Automatic {
        trigger_kind: picked.trigger_kind.as_audit_str(),
    };
    let unapplied = |merge_run_id: Uuid, error: AppError, stage: &str| {
        let retryable = is_retryable_completion_failure(&error);
        warn!(
            workspace_repo = %picked.key,
            %merge_run_id,
            error = %error,
            retryable,
            stage,
            "branch_integration_worker: automatic divergence completion was not applied"
        );
        AutoCompletion::Unapplied {
            merge_run_id,
            error: error.inner.to_string(),
            retryable,
        }
    };

    let mut db = match scoped_db_for_maintenance_org(tenant_deps, org_id).await {
        Ok(db) => db,
        Err(error) => {
            // The claim that follows needs the same database and retries
            // through its own lease expiry.
            warn!(workspace_repo = %picked.key, error = %error,
                "branch_integration_worker: could not check for a divergence run");
            return AutoCompletion::Settled;
        }
    };
    let merge_run_id: Option<Uuid> = match sqlx::query_scalar(
        r#"
        SELECT run.id
        FROM workspace_merge_runs run
        JOIN remote_integrations integration ON
            (integration.org_id, integration.workspace_id, integration.repo_id)
                = (run.org_id, run.workspace_id, run.repo_id)
        WHERE run.org_id = $1 AND run.workspace_id = $2 AND run.repo_id = $3
          AND run.source_kind = 'remote_tracking'
          AND run.phase = 'workspace_rebase_preflight'
          AND run.status IN ('waiting_for_human', 'running')
          AND integration.sync_paused_reason = 'diverged'
        "#,
    )
    .bind(picked.key.org_id)
    .bind(picked.key.workspace_id)
    .bind(picked.key.repo_id)
    .fetch_optional(db.conn())
    .await
    {
        Ok(run) => run,
        Err(error) => {
            warn!(workspace_repo = %picked.key, error = %error,
                "branch_integration_worker: could not check for a divergence run");
            return AutoCompletion::Settled;
        }
    };
    let Some(merge_run_id) = merge_run_id else {
        return AutoCompletion::Settled;
    };
    let repos = deps.repo_storage.scoped(&db);
    let prepared = prepare_external_remote_tracking_merge_completion(
        db.conn(),
        merge_run_id,
        &repos,
        requester,
    )
    .await;
    drop(db);
    let prepared = match prepared {
        Ok(prepared @ PreparedExternalRemoteTrackingCompletion::Ready { .. }) => prepared,
        Ok(PreparedExternalRemoteTrackingCompletion::AlreadyTerminal { .. }) => {
            return AutoCompletion::Settled;
        }
        // Not (yet) proven: the head has not advanced, does not descend from
        // the captured provider tip, or is not a linear rebase.
        Err(error)
            if matches!(
                error.inner,
                G5Error::Conflict { .. } | G5Error::ValidationError { .. }
            ) =>
        {
            debug!(
                workspace_repo = %picked.key,
                %merge_run_id,
                reason = %error,
                "branch_integration_worker: divergence resolution not proven; run stays open"
            );
            return AutoCompletion::Settled;
        }
        Err(error) => return unapplied(merge_run_id, error, "prepare"),
    };
    let db = match scoped_db_for_maintenance_org(tenant_deps, org_id).await {
        Ok(db) => db,
        Err(error) => return unapplied(merge_run_id, error, "apply"),
    };
    match apply_external_remote_tracking_merge_completion(
        db,
        prepared,
        &deps.event_bus,
        &deps.repo_storage,
    )
    .await
    {
        Ok(outcome) => {
            if outcome.completed {
                info!(
                    workspace_repo = %picked.key,
                    %merge_run_id,
                    trigger = picked.trigger_kind.as_audit_str(),
                    "branch_integration_worker: divergence resolution proven; run completed automatically"
                );
            }
            AutoCompletion::Settled
        }
        Err(error) => unapplied(merge_run_id, error, "apply"),
    }
}

/// Keep the wake for another automatic-completion attempt with exponential
/// backoff (15 s doubling, capped at 10 min). Returns `false` once the wake has
/// used its attempts, so the caller surfaces the failure instead.
async fn defer_auto_completion_retry(
    conn: &mut PgConnection,
    picked: &PickedQueueRow,
    error: &str,
) -> Result<bool, AppError> {
    // A dedicated counter: queue pickups also bump `attempts`.
    let attempts: Option<i32> = sqlx::query_scalar(
        "UPDATE remote_integration_queue
            SET auto_completion_attempts = auto_completion_attempts + 1,
                last_error = $4,
                locked_by = NULL,
                locked_until = now()
                    + make_interval(secs => LEAST(15 * power(2, auto_completion_attempts), 600)::double precision)
          WHERE workspace_id = $1 AND repo_id = $2 AND locked_by = $3
            AND auto_completion_attempts + 1 < $5
          RETURNING auto_completion_attempts",
    )
    .bind(picked.key.workspace_id)
    .bind(picked.key.repo_id)
    .bind(picked.owner_token)
    .bind(scrub_opt(Some(error.to_string())))
    .bind(AUTO_COMPLETION_MAX_ATTEMPTS)
    .fetch_optional(conn)
    .await?;
    Ok(attempts.is_some())
}

/// Surface an automatic completion that will not be retried on the open run,
/// where the owner sees it and can confirm manually with the same proof. A
/// later successful completion clears it.
async fn record_auto_completion_failure(
    conn: &mut PgConnection,
    merge_run_id: Uuid,
    error: &str,
) -> Result<(), AppError> {
    let detail: String = error.chars().take(300).collect();
    sqlx::query(
        "UPDATE workspace_merge_runs
            SET blocking_reason = $2, updated_at = now()
          WHERE id = $1 AND source_kind = 'remote_tracking'
            AND status IN ('waiting_for_human', 'running')",
    )
    .bind(merge_run_id)
    .bind(scrub_opt(Some(format!(
        "Automatic completion failed: {detail}. Mark resolved to retry."
    ))))
    .execute(conn)
    .await?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Queue row pickup
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct PickedQueueRow {
    key: WorkspaceRepoKey,
    trigger_kind: TriggerKind,
    requested_at: chrono::DateTime<chrono::Utc>,
    advance_id: Option<Uuid>,
    requested_by_user_id: Option<Uuid>,
    lifecycle_generation: Option<i64>,
    owner_token: Uuid,
}

impl PickedQueueRow {
    fn explicit_authority(&self) -> Option<PushAuthority> {
        Some(PushAuthority {
            requested_by_user_id: self.requested_by_user_id?,
            lifecycle_generation: self.lifecycle_generation?,
        })
    }

    fn is_explicit(&self) -> bool {
        matches!(
            self.trigger_kind,
            TriggerKind::Push | TriggerKind::ApprovedRebasePublish
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RemotePairLease {
    repo_id: Uuid,
    remote_branch: String,
    owner_token: Uuid,
    generation: i64,
}

/// Result of one claimed cycle, distinguishing committed work from a cycle
/// whose generation-fenced remote-pair lease was lost or already held.
/// A projection that MUST run only after the cycle's DB transaction commits.
/// Canonical and tracking refs are derived from the committed database cut;
/// neither is permitted to select current authority.
#[derive(Debug, PartialEq, Eq, Clone)]
struct PendingBaselineRefWrite {
    repo_id: Uuid,
    local_branch_name: String,
    expected_watermark: Option<String>,
    new_baseline: String,
}

#[derive(Default)]
struct CompletedCycle {
    authority_advance: Option<super::PreparedRemotePublication>,
    tracking_ref_writes: Vec<PendingBaselineRefWrite>,
    provider_tip: Option<String>,
}

enum ProcessOutcome {
    /// The cycle ran to a terminal classification (or vanished/paused); commit
    /// its DB effects, then run the carried post-commit ref advances.
    Completed(Box<CompletedCycle>),
    /// Another cycle holds the `(repo_id, remote_branch)` lease;
    /// this cycle wrote no audit row and deleted no queue row. The queue
    /// row was re-armed with a short `locked_until` so a later sweep tick
    /// retries once the holder commits.
    DeferredLeaseHeld,
}

/// Claim one shared remote pair in a short transaction. A takeover is allowed
/// only after expiry and always increments the generation, fencing any stale
/// worker that resumes after process or network delay.
async fn try_claim_remote_pair_lease(
    conn: &mut PgConnection,
    org_id: Uuid,
    repo_id: Uuid,
    remote_branch: &str,
    owner_token: Uuid,
) -> Result<Option<RemotePairLease>, AppError> {
    let generation: Option<i64> = sqlx::query_scalar(
        r#"
        INSERT INTO remote_ref_leases (
            repo_id, remote_branch, org_id, owner_token,
            lease_generation, lease_expires_at
        ) VALUES (
            $1, $2, $3, $4, 1,
            now() + make_interval(secs => $5)
        )
        ON CONFLICT (repo_id, remote_branch) DO UPDATE
           SET org_id = EXCLUDED.org_id,
               owner_token = EXCLUDED.owner_token,
               lease_generation =
                   remote_ref_leases.lease_generation + 1,
               lease_expires_at = EXCLUDED.lease_expires_at,
               updated_at = now()
         WHERE remote_ref_leases.lease_expires_at <= now()
        RETURNING lease_generation
        "#,
    )
    .bind(repo_id)
    .bind(remote_branch)
    .bind(org_id)
    .bind(owner_token)
    .bind(CYCLE_LEASE_TTL_SECS as f64)
    .fetch_optional(&mut *conn)
    .await
    .map_err(AppError::from)?;

    Ok(generation.map(|generation| RemotePairLease {
        repo_id,
        remote_branch: remote_branch.to_string(),
        owner_token,
        generation,
    }))
}

async fn release_remote_pair_lease(
    conn: &mut PgConnection,
    lease: &RemotePairLease,
) -> Result<bool, AppError> {
    let released = sqlx::query(
        r#"
        DELETE FROM remote_ref_leases
        WHERE repo_id = $1
          AND remote_branch = $2
          AND owner_token = $3
          AND lease_generation = $4
          AND lease_expires_at > now()
        "#,
    )
    .bind(lease.repo_id)
    .bind(&lease.remote_branch)
    .bind(lease.owner_token)
    .bind(lease.generation)
    .execute(&mut *conn)
    .await
    .map_err(AppError::from)?
    .rows_affected()
        == 1;
    Ok(released)
}

async fn renew_cycle_leases(
    conn: &mut PgConnection,
    picked: &PickedQueueRow,
    lease: &RemotePairLease,
) -> Result<bool, AppError> {
    let renewed: bool = sqlx::query_scalar(
        r#"
        WITH renewed_pair AS (
            UPDATE remote_ref_leases
               SET lease_expires_at = now() + make_interval(secs => $5),
                   updated_at = now()
             WHERE repo_id = $1
               AND remote_branch = $2
               AND owner_token = $3
               AND lease_generation = $4
               AND lease_expires_at > now()
             RETURNING repo_id
        ),
        renewed_queue AS (
            UPDATE remote_integration_queue
               SET locked_until = now() + make_interval(secs => $5)
             WHERE workspace_id = $6 AND repo_id = $7
               AND locked_by = $3
               AND locked_until > now()
               AND EXISTS (SELECT 1 FROM renewed_pair)
             RETURNING repo_id
        )
        SELECT EXISTS (SELECT 1 FROM renewed_queue)
        "#,
    )
    .bind(lease.repo_id)
    .bind(&lease.remote_branch)
    .bind(lease.owner_token)
    .bind(lease.generation)
    .bind(CYCLE_LEASE_TTL_SECS as f64)
    .bind(picked.key.workspace_id)
    .bind(picked.key.repo_id)
    .fetch_one(&mut *conn)
    .await
    .map_err(AppError::from)?;
    Ok(renewed)
}

enum CycleClaimValidation {
    Valid,
    StaleState,
    LostLease,
}

/// Fence finalization with both durable owners, then lock and compare the
/// database-selected branch state that remote preparation observed.
async fn validate_cycle_claim_for_finalization(
    repos: &ScopedRepoStorage,
    conn: &mut PgConnection,
    picked: &PickedQueueRow,
    lease: &RemotePairLease,
    expected_state: &CycleState,
    co_members: &[CoMember],
) -> Result<CycleClaimValidation, AppError> {
    #[derive(sqlx::FromRow)]
    struct OwnedQueueClaim {
        trigger_kind: String,
        requested_at: chrono::DateTime<chrono::Utc>,
        advance_id: Option<Uuid>,
        requested_by_user_id: Option<Uuid>,
        lifecycle_generation: Option<i64>,
    }

    acquire_workspace_commit_fence(conn, expected_state.workspace_id).await?;
    let owned = sqlx::query_as::<_, OwnedQueueClaim>(
        r#"SELECT q.trigger_kind, q.requested_at, q.advance_id, q.requested_by_user_id, q.lifecycle_generation FROM remote_integration_queue q
        JOIN remote_ref_leases p ON p.repo_id = q.repo_id AND p.remote_branch = $4
        WHERE q.workspace_id = $1 AND q.repo_id = $2 AND q.locked_by = $3 AND q.locked_until > now()
          AND p.owner_token = $3 AND p.lease_generation = $5 AND p.lease_expires_at > now()
        FOR UPDATE OF q, p"#,
    )
    .bind(picked.key.workspace_id)
    .bind(picked.key.repo_id)
    .bind(picked.owner_token)
    .bind(&lease.remote_branch)
    .bind(lease.generation)
    .fetch_optional(&mut *conn)
    .await?;
    let Some(owned) = owned else {
        return Ok(CycleClaimValidation::LostLease);
    };
    if parse_trigger_kind(&owned.trigger_kind) != Some(picked.trigger_kind)
        || owned.requested_at != picked.requested_at
        || owned.advance_id != picked.advance_id
        || owned.requested_by_user_id != picked.requested_by_user_id
        || owned.lifecycle_generation != picked.lifecycle_generation
    {
        return Ok(CycleClaimValidation::StaleState);
    }
    if remote_integrations::load(conn, picked.key, true)
        .await?
        .is_none()
    {
        return Ok(CycleClaimValidation::StaleState);
    }
    let current = load_cycle_state(repos, conn, picked.key).await?;
    if current.as_ref() != Some(expected_state) {
        return Ok(CycleClaimValidation::StaleState);
    }
    for co in co_members {
        let owned: Option<Uuid> = sqlx::query_scalar(
            "SELECT repo_id FROM remote_ref_leases
             WHERE repo_id = $1 AND remote_branch = $2 AND owner_token = $3
               AND lease_generation = $4 AND lease_expires_at > now()
             FOR UPDATE",
        )
        .bind(co.lease.repo_id)
        .bind(&co.lease.remote_branch)
        .bind(co.lease.owner_token)
        .bind(co.lease.generation)
        .fetch_optional(&mut *conn)
        .await?;
        if owned.is_none()
            || load_cycle_state(repos, conn, co.state.key).await?.as_ref() != Some(&co.state)
        {
            return Ok(CycleClaimValidation::StaleState);
        }
    }
    Ok(CycleClaimValidation::Valid)
}

/// Extend each co-member's lease with the primary's, under the same token.
async fn renew_co_member_leases(
    conn: &mut PgConnection,
    owner_token: Uuid,
    co_members: &[CoMember],
) -> Result<bool, AppError> {
    for co in co_members {
        let renewed = sqlx::query(
            "UPDATE remote_ref_leases
                SET lease_expires_at = now() + make_interval(secs => $5), updated_at = now()
              WHERE repo_id = $1 AND remote_branch = $2 AND owner_token = $3
                AND lease_generation = $4 AND lease_expires_at > now()",
        )
        .bind(co.lease.repo_id)
        .bind(&co.lease.remote_branch)
        .bind(owner_token)
        .bind(co.lease.generation)
        .bind(CYCLE_LEASE_TTL_SECS as f64)
        .execute(&mut *conn)
        .await?
        .rows_affected()
            == 1;
        if !renewed {
            return Ok(false);
        }
    }
    Ok(true)
}

/// A landing refused because the workspace or the claim moved on: the next
/// sweep retries it with fresh state.
fn is_refused_landing(error: &AppError) -> bool {
    matches!(
        error.inner,
        G5Error::AuthorityCutStale { .. } | G5Error::Conflict { .. } | G5Error::GitError { .. }
    )
}

/// A cut prepared against heads another writer has since moved. Other
/// preparation failures may persist, so they still wait for lease expiry.
fn is_stale_prepared_cut(error: &AppError) -> bool {
    matches!(error.inner, G5Error::AuthorityCutStale { .. })
}

/// Give up this cycle without waiting for lease expiry: release every lease it
/// still owns and re-arm its queue row for a prompt retry.
async fn abandon_cycle(
    deps: &BranchIntegrationWorkerDeps,
    org_id: Uuid,
    picked: &PickedQueueRow,
    lease: &RemotePairLease,
    co_members: &[CoMember],
) {
    let abandoned = async {
        let mut db = scoped_db_for_maintenance_org(&deps.tenant_deps(), org_id).await?;
        release_remote_pair_lease(db.conn(), lease).await?;
        for co in co_members {
            release_remote_pair_lease(db.conn(), &co.lease).await?;
        }
        defer_queue_row_for_pair_lease(db.conn(), picked).await?;
        db.commit().await.map_err(AppError::from)
    }
    .await;
    if let Err(error) = abandoned {
        warn!(workspace_repo = %picked.key, %error, "branch_integration_worker: could not release an abandoned cycle; leases will expire");
    }
}

/// Re-arms a queue row whose remote-pair lease was unavailable or whose
/// prepared state became stale. The row is not deleted and no audit row is
/// written because the cycle did not publish work.
async fn defer_queue_row_for_pair_lease(
    conn: &mut PgConnection,
    picked: &PickedQueueRow,
) -> Result<(), AppError> {
    sqlx::query("UPDATE remote_integration_queue SET locked_by = NULL, locked_until = now() + make_interval(secs => $4)
                 WHERE workspace_id = $1 AND repo_id = $2 AND locked_by = $3")
        .bind(picked.key.workspace_id).bind(picked.key.repo_id).bind(picked.owner_token)
        .bind(BRANCH_LOCK_DEFER_SECS as f64).execute(conn).await?;
    Ok(())
}

async fn grab_one_queue_row(conn: &mut PgConnection) -> Result<Option<PickedQueueRow>, AppError> {
    #[derive(sqlx::FromRow)]
    struct ClaimedQueueRow {
        org_id: Uuid,
        workspace_id: Uuid,
        repo_id: Uuid,
        trigger_kind: String,
        requested_at: chrono::DateTime<chrono::Utc>,
        advance_id: Option<Uuid>,
        requested_by_user_id: Option<Uuid>,
        lifecycle_generation: Option<i64>,
    }

    let owner_token = Uuid::new_v4();
    let row = sqlx::query_as::<_, ClaimedQueueRow>(
        r#"
        WITH picked AS (
            SELECT q.workspace_id, q.repo_id
            FROM remote_integration_queue q
            JOIN remote_integrations i USING (org_id, workspace_id, repo_id)
            JOIN workspaces w ON w.id = q.workspace_id AND w.org_id = q.org_id
            WHERE (q.locked_until IS NULL OR q.locked_until < now())
              AND q.org_id = ANY(authz.current_org_ids()) AND w.deleted_at IS NULL
              AND (q.trigger_kind <> 'approved_rebase_publish' OR NOT EXISTS (
                  SELECT 1 FROM remote_integration_queue prior_q
                  JOIN workspace_merge_runs prior_jmr
                    ON (prior_jmr.workspace_id, prior_jmr.repo_id) = (prior_q.workspace_id, prior_q.repo_id)
                   AND prior_jmr.source_kind = 'remote_tracking' AND prior_jmr.phase = 'reset_below_baseline'
                   AND prior_jmr.status IN ('waiting_for_human', 'running')
                  JOIN workspace_merge_runs current_jmr
                    ON (current_jmr.workspace_id, current_jmr.repo_id) = (q.workspace_id, q.repo_id)
                   AND current_jmr.source_kind = 'remote_tracking' AND current_jmr.phase = 'reset_below_baseline'
                   AND current_jmr.status IN ('waiting_for_human', 'running')
                  WHERE prior_q.trigger_kind = 'approved_rebase_publish'
                    AND prior_q.workspace_id = q.workspace_id AND prior_q.repo_id <> q.repo_id
                    AND (prior_q.requested_at, prior_q.repo_id) < (q.requested_at, q.repo_id)
              ))
            ORDER BY q.requested_at, q.workspace_id, q.repo_id LIMIT 1 FOR UPDATE OF q SKIP LOCKED
        )
        UPDATE remote_integration_queue q SET locked_by = $1,
            locked_until = now() + make_interval(secs => $2), attempts = attempts + 1, last_attempt_at = now()
        FROM picked WHERE (q.workspace_id, q.repo_id) = (picked.workspace_id, picked.repo_id)
        RETURNING q.org_id, q.workspace_id, q.repo_id, q.trigger_kind, q.requested_at, q.advance_id, q.requested_by_user_id, q.lifecycle_generation
        "#,
    ).bind(owner_token).bind(CYCLE_LEASE_TTL_SECS as f64).fetch_optional(&mut *conn).await?;
    let Some(ClaimedQueueRow {
        org_id,
        workspace_id,
        repo_id,
        trigger_kind: kind,
        requested_at,
        advance_id,
        requested_by_user_id,
        lifecycle_generation,
    }) = row
    else {
        return Ok(None);
    };
    let trigger_kind = parse_trigger_kind(&kind).ok_or_else(|| {
        AppError::from_error(G5Error::Internal {
            message: format!("Unknown remote integration trigger {kind}"),
        })
    })?;
    Ok(Some(PickedQueueRow {
        key: WorkspaceRepoKey {
            org_id,
            workspace_id,
            repo_id,
        },
        trigger_kind,
        requested_at,
        advance_id,
        requested_by_user_id,
        lifecycle_generation,
        owner_token,
    }))
}

fn parse_trigger_kind(s: &str) -> Option<TriggerKind> {
    match s {
        "commit" => Some(TriggerKind::Commit),
        "manual" => Some(TriggerKind::Manual),
        "workspace_open" => Some(TriggerKind::WorkspaceOpen),
        "background_inbound" => Some(TriggerKind::BackgroundInbound),
        "push" => Some(TriggerKind::Push),
        "approved_rebase_publish" => Some(TriggerKind::ApprovedRebasePublish),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Cycle state — exact DB authority loaded before any remote I/O
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(dead_code)]
struct CycleState {
    key: WorkspaceRepoKey,
    /// The tracked repository, keyed in the per-organization scope that
    /// loaded this state (`g5.repo.storage-tenancy`).
    repo_key: RepoKey,
    policy: remote_integrations::PublicationPolicy,
    workspace_id: Uuid,
    repo_id: Uuid,
    org_id: Uuid,
    remote_branch: String,
    local_branch_name: String,
    authority_inventory: Option<AuthorityInventorySnapshot>,
    authority_heads: BTreeMap<Uuid, String>,
    workspace_tip: String,
    baseline: String,
    sync_paused_reason: Option<String>,
    git_url: String,
    acting_user_id: Uuid,
    lifecycle_generation: i64,
    transfer_pending: bool,
    dirty_files: i32,
    push_mode: String,
    auto_pull_enabled: bool,
    /// Auto-pull is held by a partly merged multi-repository workspace PR.
    automatic_import_held: bool,
    org_binding_mismatch: bool,
}

impl CycleState {
    fn push_authority(&self) -> PushAuthority {
        PushAuthority {
            requested_by_user_id: self.acting_user_id,
            lifecycle_generation: self.lifecycle_generation,
        }
    }

    fn authority_inventory(&self) -> Result<&AuthorityInventorySnapshot, AppError> {
        self.authority_inventory.as_ref().ok_or_else(|| {
            AppError::from_error(G5Error::Internal {
                message: format!(
                    "Remote-tracking cycle {} has no authority inventory outside its org-binding mismatch terminal path",
                    self.key
                ),
            })
        })
    }
}

async fn load_cycle_state(
    repos: &ScopedRepoStorage,
    conn: &mut PgConnection,
    key: WorkspaceRepoKey,
) -> Result<Option<CycleState>, AppError> {
    let repo_key = repos.key(key.org_id, key.repo_id)?;
    let Some(policy) = remote_integrations::load(conn, key, false).await? else {
        return Ok(None);
    };
    let acting_user_id = policy.acting_user_id.ok_or_else(|| {
        AppError::from_error(G5Error::Conflict {
            reason: "Remote integration requires a current workspace owner".into(),
        })
    })?;
    let inventory = resolve_authority_inventory(
        conn,
        AuthorityTarget::Workspace {
            workspace_id: key.workspace_id,
        },
    )
    .await?;
    let memberships = crate::services::workspace_repositories::load_workspace_repositories(
        conn,
        key.workspace_id,
        false,
    )
    .await?;
    let versions =
        crate::services::workspace_repositories::read_repository_versions(repos, memberships)
            .await?;
    let heads: BTreeMap<_, _> = versions
        .into_iter()
        .map(|version| (version.member.repo_id, version.head_commit))
        .collect();
    let workspace_tip = heads.get(&key.repo_id).cloned().ok_or_else(|| {
        AppError::from_error(G5Error::Conflict {
            reason: "Remote integration repository left the workspace".into(),
        })
    })?;
    let path = repos.path(&repo_key)?;
    let baseline = GitRepo::open(&path)?
        .read_sync_watermark(&policy.local_branch)?
        .unwrap_or_else(|| ZERO_GIT_SHA_SENTINEL.into());
    let automatic_import_held =
        super::automatic_import_held(conn, key.workspace_id, key.repo_id).await?;
    Ok(Some(CycleState {
        repo_key,
        key,
        policy: policy.publication_policy(),
        workspace_id: key.workspace_id,
        repo_id: key.repo_id,
        org_id: key.org_id,
        remote_branch: policy.remote_branch,
        local_branch_name: policy.local_branch,
        authority_inventory: Some(inventory),
        authority_heads: heads,
        workspace_tip,
        baseline,
        sync_paused_reason: policy.sync_paused_reason,
        git_url: policy.git_url,
        acting_user_id,
        lifecycle_generation: policy.lifecycle_generation,
        transfer_pending: policy.transfer_pending,
        dirty_files: 0,
        push_mode: policy.push_mode,
        auto_pull_enabled: policy.auto_pull_enabled,
        automatic_import_held,
        org_binding_mismatch: false,
    }))
}

// ---------------------------------------------------------------------------
// Per-row processing
// ---------------------------------------------------------------------------

enum CycleClaimOutcome {
    Completed,
    Deferred,
    Claimed {
        state: Box<CycleState>,
        lease: RemotePairLease,
        approved_rebase_proof: Box<Option<Result<ApprovedRebasePublishProof, String>>>,
        resume_pause_only: bool,
        /// A provider-rewrite checkpoint that fresh inputs may prove false.
        reclassify_checkpoint: bool,
    },
}

/// Load the exact database-selected cycle state and claim its shared remote
/// pair. The caller commits this short transaction before any credential or Git
/// operation begins.
async fn claim_queue_row(
    deps: &BranchIntegrationWorkerDeps,
    repos: &ScopedRepoStorage,
    conn: &mut PgConnection,
    picked: &PickedQueueRow,
) -> Result<CycleClaimOutcome, AppError> {
    let Some(state) = load_cycle_state(repos, conn, picked.key).await? else {
        delete_queue_row(conn, picked).await?;
        return Ok(CycleClaimOutcome::Completed);
    };

    if picked.is_explicit()
        && (picked.explicit_authority() != Some(state.push_authority()) || state.transfer_pending)
    {
        // Never reinterpret an unattributed or former owner's request as consent
        // from the new owner. Keep the cancellation visible without pausing pulls.
        let mut audit_state = state.clone();
        if let Some(actor) = picked.requested_by_user_id {
            audit_state.acting_user_id = actor;
        }
        write_push_failed_audit(conn, deps, &audit_state, picked.trigger_kind, None,
            Some("Explicit push authorization expired or is missing; request Push to Remote again as the current owner".into())).await?;
        delete_queue_row(conn, picked).await?;
        return Ok(CycleClaimOutcome::Completed);
    }

    // Cross-org corruption must terminalize before remote-pair coordination.
    // The lease has a composite (org_id, repo_id) FK, so attempting to claim it
    // would roll this transaction back and leave the queue row retrying forever.
    if state.org_binding_mismatch {
        write_org_binding_mismatch_audit(conn, deps, &state, picked.trigger_kind).await?;
        return Ok(CycleClaimOutcome::Completed);
    }

    // A provider-rewrite checkpoint is re-evaluated against fresh Git inputs:
    // one opened for what is now proven a workspace rewrite (or convergence)
    // is closed by the cycle (`g5.governance.remote.workspace-rewrite`).
    let reclassify_checkpoint = state.sync_paused_reason.as_deref() == Some("watermark_violation")
        && picked.trigger_kind != TriggerKind::ApprovedRebasePublish
        && is_unapproved_rewrite_checkpoint(conn, state.key).await?;

    // Re-check passive eligibility after claim and before the remote-pair
    // lease or any Git I/O. Either automatic direction admits background work;
    // an explicit trigger does not depend on either automatic toggle.
    if picked.trigger_kind == TriggerKind::BackgroundInbound
        && ((!state.auto_pull_enabled && state.push_mode != "auto")
            || (state.sync_paused_reason.is_some() && !reclassify_checkpoint))
    {
        delete_queue_row(conn, picked).await?;
        return Ok(CycleClaimOutcome::Completed);
    }

    let resume_pause_only = state.sync_paused_reason.as_deref() == Some("diverged")
        && matches!(picked.trigger_kind, TriggerKind::Push | TriggerKind::Manual)
        && !has_active_remote_tracking_merge_run(conn, state.key).await?;

    let Some(lease) = try_claim_remote_pair_lease(
        conn,
        state.org_id,
        state.repo_id,
        &state.remote_branch,
        picked.owner_token,
    )
    .await?
    else {
        debug!(
            workspace_repo = %picked.key,
            repo_id = %state.repo_id,
            remote_branch = %state.remote_branch,
            "branch_integration_worker: deferring because another cycle owns the remote-pair lease",
        );
        defer_queue_row_for_pair_lease(conn, picked).await?;
        return Ok(CycleClaimOutcome::Deferred);
    };

    let approved_rebase_proof = if picked.trigger_kind == TriggerKind::ApprovedRebasePublish {
        Some(load_approved_rebase_publish_proof(conn, state.key).await)
    } else {
        None
    };

    Ok(CycleClaimOutcome::Claimed {
        state: Box::new(state),
        lease,
        approved_rebase_proof: Box::new(approved_rebase_proof),
        resume_pause_only,
        reclassify_checkpoint,
    })
}

enum PreparedCycle {
    Paused(Box<CycleState>),
    AuthRequired {
        state: Box<CycleState>,
        message: String,
    },
    FetchFailed {
        state: Box<CycleState>,
        mirror_tip: Option<String>,
        message: String,
    },
    Decision {
        state: Box<CycleState>,
        prepared: Box<PreparedDecision>,
    },
    ApprovedRebase {
        state: Box<CycleState>,
        outcome: ApprovedRebaseRemoteOutcome,
    },
}

struct PreparedDecision {
    input: ClassifyInput,
    decision: Decision,
    push_result: Option<Result<(), String>>,
    resume_pause_only: bool,
    reclassify_checkpoint: bool,
    authority_advance: Option<super::PreparedRemotePublication>,
    co_members: Vec<CoMember>,
}

impl PreparedCycle {
    /// Members co-landed with this cycle's import, whose leases the cycle holds.
    fn co_members(&self) -> &[CoMember] {
        match self {
            PreparedCycle::Decision { prepared, .. } => &prepared.co_members,
            _ => &[],
        }
    }
}

/// A member whose due import lands in the importing cycle's cut
/// (`g5.governance.remote.pull`). The cycle holds its remote-pair lease under
/// the primary owner token until the landing commits or is refused.
#[derive(Debug, Clone)]
struct CoMember {
    state: CycleState,
    lease: RemotePairLease,
    /// Its own queued manual pull made it eligible; otherwise auto-pull did.
    explicit: bool,
    provider_tip: String,
}

impl CoMember {
    fn claim(&self) -> super::CoMemberClaim {
        super::CoMemberClaim {
            key: self.state.key,
            policy: self.state.policy.clone(),
            lease_generation: self.lease.generation,
            automatic_import: !self.explicit,
        }
    }
}

/// Claim, without waiting, the remote-pair leases of the workspace's other
/// members whose import is due: bound, unpaused, eligible under their own
/// policy and captured at the primary's Git vector. A busy lease or an
/// unreadable member is skipped; it never blocks the primary import.
async fn claim_co_members(
    deps: &BranchIntegrationWorkerDeps,
    primary: &CycleState,
    owner_token: Uuid,
) -> Result<Vec<(CycleState, RemotePairLease, bool)>, AppError> {
    let mut db = scoped_db_for_maintenance_org(&deps.tenant_deps(), primary.org_id).await?;
    let repos = deps.repo_storage.scoped(&db);
    let candidates: Vec<(Uuid, bool)> = sqlx::query_as(
        "SELECT i.repo_id, EXISTS (
                SELECT 1 FROM remote_integration_queue q
                WHERE q.workspace_id = i.workspace_id AND q.repo_id = i.repo_id
                  AND q.trigger_kind = 'manual') AS manual_requested
         FROM remote_integrations i
         WHERE i.org_id = $1 AND i.workspace_id = $2 AND i.repo_id <> $3
           AND i.sync_paused_reason IS NULL
         ORDER BY i.repo_id",
    )
    .bind(primary.org_id)
    .bind(primary.workspace_id)
    .bind(primary.repo_id)
    .fetch_all(db.conn())
    .await?;
    let mut claimed = Vec::new();
    for (repo_id, manual_requested) in candidates {
        let key = WorkspaceRepoKey {
            org_id: primary.org_id,
            workspace_id: primary.workspace_id,
            repo_id,
        };
        let state = match load_cycle_state(&repos, db.conn(), key).await {
            Ok(Some(state)) => state,
            Ok(None) => continue,
            Err(error) => {
                // A failed read may have aborted the transaction; stop here and
                // keep only what was already claimed.
                debug!(workspace_repo = %key, %error, "branch_integration_worker: co-member skipped");
                break;
            }
        };
        let eligible =
            manual_requested || (state.auto_pull_enabled && !state.automatic_import_held);
        if !eligible
            || state.sync_paused_reason.is_some()
            || state.authority_heads != primary.authority_heads
        {
            continue;
        }
        match try_claim_remote_pair_lease(
            db.conn(),
            primary.org_id,
            repo_id,
            &state.remote_branch,
            owner_token,
        )
        .await
        {
            Ok(Some(lease)) => claimed.push((state, lease, manual_requested)),
            Ok(None) => {}
            Err(error) => {
                debug!(workspace_repo = %key, %error, "branch_integration_worker: co-member lease unavailable");
                break;
            }
        }
    }
    match db.commit().await {
        Ok(()) => Ok(claimed),
        // Nothing was claimed if the transaction rolled back.
        Err(error) => {
            debug!(%error, "branch_integration_worker: co-member claim rolled back");
            Ok(Vec::new())
        }
    }
}

/// Fetch each claimed co-member and keep only strict fast-forwards. A member
/// whose credentials, fetch or history do not admit an import is returned for
/// release; its own cycle records that outcome.
async fn prepare_co_members(
    deps: &BranchIntegrationWorkerDeps,
    claimed: Vec<(CycleState, RemotePairLease, bool)>,
) -> (Vec<CoMember>, Vec<RemotePairLease>) {
    let mut kept = Vec::new();
    let mut released = Vec::new();
    for (state, lease, explicit) in claimed {
        let Ok(auth) = resolve_worker_auth(deps, &state, RepositoryOperation::ReadGit).await else {
            released.push(lease);
            continue;
        };
        let prepared = crate::services::remote_integrations::prepare_pull(
            &deps.repo_storage,
            crate::services::remote_integrations::PullInput {
                key: state.repo_key,
                git_url: state.git_url.clone(),
                remote_branch: state.remote_branch.clone(),
                local_head: state.workspace_tip.clone(),
                watermark: (state.baseline != ZERO_GIT_SHA_SENTINEL)
                    .then(|| state.baseline.clone()),
                observed_remote_tip: None,
            },
            auth,
        )
        .await;
        match prepared {
            Ok(prepared) if matches!(prepared.relation, TipRelation::MirrorAhead) => {
                kept.push(CoMember {
                    state,
                    lease,
                    explicit,
                    provider_tip: prepared.remote_tip,
                });
            }
            _ => released.push(lease),
        }
    }
    (kept, released)
}

/// Best-effort release of leases this cycle owns; an expired or taken lease
/// is already free of this cycle.
async fn release_leases(
    deps: &BranchIntegrationWorkerDeps,
    org_id: Uuid,
    leases: &[&RemotePairLease],
) {
    if leases.is_empty() {
        return;
    }
    let released = async {
        let mut db = scoped_db_for_maintenance_org(&deps.tenant_deps(), org_id).await?;
        for lease in leases {
            release_remote_pair_lease(db.conn(), lease).await?;
        }
        db.commit().await.map_err(AppError::from)
    }
    .await;
    if let Err(error) = released {
        warn!(%error, "branch_integration_worker: lease release failed; leases will expire");
    }
}

enum ApprovedRebaseRemoteOutcome {
    Ready(Box<PreparedApprovedRebasePublish>),
    Failed {
        merge_run_id: Option<Uuid>,
        message: String,
    },
}

struct PreparedApprovedRebasePublish {
    proof: ApprovedRebasePublishProof,
    normalized_tip: String,
    authority_advance: PreparedBranchCommit,
}

async fn prepare_claimed_cycle(
    deps: &BranchIntegrationWorkerDeps,
    picked: &PickedQueueRow,
    state: CycleState,
    approved_rebase_proof: Option<Result<ApprovedRebasePublishProof, String>>,
    resume_pause_only: bool,
    reclassify_checkpoint: bool,
) -> Result<PreparedCycle, AppError> {
    if picked.is_explicit() {
        if let Err(message) = validate_push_authority(deps, &state, picked).await {
            return Ok(PreparedCycle::FetchFailed {
                state: Box::new(state),
                mirror_tip: None,
                message,
            });
        }
    }
    if picked.trigger_kind == TriggerKind::ApprovedRebasePublish {
        let outcome = prepare_approved_rebase_publish(deps, &state, approved_rebase_proof).await;
        return Ok(PreparedCycle::ApprovedRebase {
            state: Box::new(state),
            outcome,
        });
    }
    if state.sync_paused_reason.is_some() && !resume_pause_only && !reclassify_checkpoint {
        return Ok(PreparedCycle::Paused(Box::new(state)));
    }

    // Cycle-start marker. This + the per-action `INFO` log at the end
    // of `execute_decision` give operators a grep-able pair around
    // every queue row the worker touches.
    info!(
        workspace_repo = %state.key,
        workspace_id = %state.workspace_id,
        repo_id = %state.repo_id,
        remote_branch = %state.remote_branch,
        local_branch_name = %state.local_branch_name,
        workspace_tip = %state.workspace_tip,
        baseline = %state.baseline,
        dirty_files = state.dirty_files,
        trigger = picked.trigger_kind.as_audit_str(),
        "branch_integration_worker: cycle start",
    );

    // Resolve repository credentials as the workspace owner. The credential
    // boundary selects App or personal access and rechecks membership.
    let auth = match resolve_worker_auth(deps, &state, RepositoryOperation::ReadGit).await {
        Ok(auth) => auth,
        Err(error) => {
            let message = error.to_string();
            if !is_credential_denial(&error) {
                return Ok(PreparedCycle::FetchFailed {
                    state: Box::new(state),
                    mirror_tip: None,
                    message,
                });
            }
            warn!(
                workspace_repo = %state.key,
                error = %message,
                "branch_integration_worker: auth_required before git transport",
            );
            return Ok(PreparedCycle::AuthRequired {
                state: Box::new(state),
                message,
            });
        }
    };

    let prepared_pull = crate::services::remote_integrations::prepare_pull_classified(
        &deps.repo_storage,
        crate::services::remote_integrations::PullInput {
            key: state.repo_key,
            git_url: state.git_url.clone(),
            remote_branch: state.remote_branch.clone(),
            local_head: state.workspace_tip.clone(),
            watermark: (state.baseline != ZERO_GIT_SHA_SENTINEL).then(|| state.baseline.clone()),
            observed_remote_tip: None,
        },
        auth.clone(),
    )
    .await;
    let (mirror_tip_opt, tip_relation) = match prepared_pull {
        Ok(prepared) => (Some(prepared.remote_tip), prepared.relation),
        Err(failure) => {
            let message = failure.error.to_string();
            if failure.credential_refused || is_git_auth_failure(&message) {
                return Ok(PreparedCycle::AuthRequired {
                    state: Box::new(state),
                    message,
                });
            }
            return Ok(PreparedCycle::FetchFailed {
                state: Box::new(state),
                mirror_tip: None,
                message,
            });
        }
    };

    let input = ClassifyInput {
        workspace_tip: state.workspace_tip.clone(),
        mirror_tip: mirror_tip_opt.clone(),
        baseline: state.baseline.clone(),
        dirty_files: state.dirty_files,
        trigger_kind: picked.trigger_kind,
        push_mode: PushMode::from_db(&state.push_mode),
        // A held member records the provider observation without importing.
        auto_pull_enabled: state.auto_pull_enabled && !state.automatic_import_held,
        publication_allowed: !state.policy.is_base,
        tip_relation,
    };

    // A genuine provider rewrite keeps its approval checkpoint unchanged.
    if reclassify_checkpoint && tip_relation == TipRelation::WatermarkViolation {
        return Ok(PreparedCycle::Paused(Box::new(state)));
    }
    let decision = classify_and_act(&input);
    // An import also lands every other member whose import is due in the
    // same cut (`g5.governance.remote.pull`).
    let co_members = if decision.action == Action::Imported {
        let claimed = match claim_co_members(deps, &state, picked.owner_token).await {
            Ok(claimed) => claimed,
            Err(error) => {
                debug!(workspace_repo = %state.key, %error, "branch_integration_worker: no co-members claimed");
                Vec::new()
            }
        };
        let (kept, released) = prepare_co_members(deps, claimed).await;
        release_leases(deps, state.org_id, &released.iter().collect::<Vec<_>>()).await;
        kept
    } else {
        Vec::new()
    };
    let authority_advance = if decision.action == Action::Imported {
        let tip = mirror_tip_opt
            .as_ref()
            .expect("import requires a fetched tip");
        let convergence = |member: &CycleState, tip: &str, trigger: &str| {
            super::convergence::PreparedRemoteTrackingConvergence {
                workspace_id: member.workspace_id,
                repo_id: member.repo_id,
                remote_branch: member.remote_branch.clone(),
                expected_tracking_mode: "remote_tracking".into(),
                expected_remote_branch: Some(member.remote_branch.clone()),
                expected_old_head: member.workspace_tip.clone(),
                proposed_new_head: tip.to_owned(),
                expected_baseline_commit: (member.baseline != ZERO_GIT_SHA_SENTINEL)
                    .then(|| member.baseline.clone()),
                expected_git_url: member.git_url.clone(),
                expected_sync_paused_reason: member.sync_paused_reason.clone(),
                acting_user_id: member.acting_user_id,
                trigger_kind: trigger.into(),
                status: super::convergence::RemoteTrackingConvergenceStatus::Imported,
            }
        };
        let mut proposed = BTreeMap::from([(state.repo_id, tip.clone())]);
        let mut inputs = vec![convergence(&state, tip, picked.trigger_kind.as_audit_str())];
        for co in &co_members {
            proposed.insert(co.state.repo_id, co.provider_tip.clone());
            let trigger = if co.explicit {
                TriggerKind::Manual.as_audit_str()
            } else {
                picked.trigger_kind.as_audit_str()
            };
            inputs.push(convergence(&co.state, &co.provider_tip, trigger));
        }
        let prepared = async {
            let db = scoped_db_for_maintenance_org(&deps.tenant_deps(), state.org_id).await?;
            let scope = db.reopener();
            db.rollback().await.map_err(AppError::from)?;
            super::prepare_pull_cut(
                &scope,
                &deps.repo_storage,
                state.authority_inventory()?.target.clone(),
                &state.authority_heads,
                &proposed,
                &inputs,
            )
            .await
            .map_err(crate::services::branch_commit::map_branch_commit_app_error)
        }
        .await;
        match prepared {
            Ok(prepared) => Some(prepared),
            Err(error) => {
                release_leases(
                    deps,
                    state.org_id,
                    &co_members.iter().map(|co| &co.lease).collect::<Vec<_>>(),
                )
                .await;
                return Err(error);
            }
        }
    } else {
        None
    };
    let push_result = if decision.action == Action::Pushed {
        Some(
            push_workspace_tip(
                deps,
                &state,
                picked,
                mirror_tip_opt
                    .as_deref()
                    .expect("push requires fetched provider tip"),
                input.tip_relation == TipRelation::WorkspaceRewritten,
            )
            .await,
        )
    } else {
        None
    };

    Ok(PreparedCycle::Decision {
        state: Box::new(state),
        prepared: Box::new(PreparedDecision {
            input,
            decision,
            push_result,
            resume_pause_only,
            reclassify_checkpoint,
            authority_advance,
            co_members,
        }),
    })
}

async fn apply_prepared_cycle(
    deps: &BranchIntegrationWorkerDeps,
    conn: &mut PgConnection,
    picked: &PickedQueueRow,
    prepared: PreparedCycle,
) -> Result<CompletedCycle, AppError> {
    let completed = match prepared {
        PreparedCycle::Paused(state) => {
            let reason = state.sync_paused_reason.clone().unwrap_or_default();
            // A background recheck of a genuine provider rewrite is silent.
            let has_active_run = picked.trigger_kind != TriggerKind::BackgroundInbound
                && (reason == "diverged" || reason == "watermark_violation")
                && has_active_remote_tracking_merge_run(conn, state.key).await?;
            if has_active_run {
                info!(
                    workspace_repo = %picked.key,
                    reason = %reason,
                    "branch_integration_worker: paused with an active merge run; writing merge_active audit"
                );
                write_merge_active_audit(conn, &state, picked.trigger_kind).await?;
            } else {
                debug!(
                    workspace_repo = %picked.key,
                    reason = %reason,
                    "branch_integration_worker: skipping paused row already audited by its originating path"
                );
            }
            delete_queue_row(conn, picked).await?;
            Ok(CompletedCycle::default())
        }
        PreparedCycle::AuthRequired { state, message } => {
            write_auth_required_audit(conn, deps, &state, picked.trigger_kind, Some(message))
                .await?;
            Ok(CompletedCycle::default())
        }
        PreparedCycle::FetchFailed {
            state,
            mirror_tip,
            message,
        } => {
            write_fetch_failed_audit(
                conn,
                deps,
                &state,
                picked.trigger_kind,
                mirror_tip,
                Some(message),
            )
            .await?;
            Ok(CompletedCycle::default())
        }
        PreparedCycle::Decision { state, prepared } => {
            execute_decision(deps, conn, &state, picked, *prepared).await
        }
        PreparedCycle::ApprovedRebase { state, outcome } => {
            apply_approved_rebase_publish(deps, conn, picked, *state, outcome).await
        }
    }?;
    // Keep authority-producing queue signals until their cut lands. Metadata-only
    // terminal classifications can clear the signal in this transaction.
    if completed.authority_advance.is_none() {
        delete_queue_row(conn, picked).await?;
    }
    Ok(completed)
}

// ---------------------------------------------------------------------------
// Mirror fetch
// ---------------------------------------------------------------------------

fn is_git_auth_failure(message: &str) -> bool {
    let message = message.trim();
    if message.ends_with("; code=Auth (-16)") {
        return true;
    }

    // Clone's G5Error conversion retains only the bare library message;
    // ls-remote/fetch retain Display's class/code suffix and wrapper context.
    let message = message.strip_prefix("Git error: ").unwrap_or(message);
    let diagnostic = message
        .strip_suffix("; class=Http (34)")
        .or_else(|| message.strip_suffix("; class=Net (12)"))
        .or_else(|| message.strip_suffix("; class=Ssh (23)"))
        .unwrap_or(message);
    let diagnostic = if diagnostic.starts_with("ls-remote connect to '")
        || diagnostic.starts_with("Single-ref fetch of '")
    {
        diagnostic
            .rsplit_once("' failed: ")
            .map_or(diagnostic, |(_, error)| error)
    } else {
        diagnostic
    };

    const AUTH_DIAGNOSTICS: &[&str] = &[
        "unexpected http status code: 401",
        "unexpected http status code: 403",
        "too many redirects or authentication replays",
        "unexpected authentication failure",
        "server authentication required but no callback set",
        "proxy authentication required but no callback set",
        "server requires authentication that we do not support",
        "proxy requires authentication that we do not support",
        "authentication required but no callback set",
        "authentication callback returned unsupported credentials type",
        "failed to respond to authentication challenge",
        "negotiate authentication is not supported",
        "'Negotiate' authentication is not supported",
        "'NTLM' authentication is not supported",
        "'Basic' authentication is not supported",
    ];
    AUTH_DIAGNOSTICS.contains(&diagnostic)
        || diagnostic.starts_with("remote rejected authentication: ")
        || diagnostic.starts_with("ntlm authentication failed: ")
}

fn is_credential_denial(error: &G5Error) -> bool {
    matches!(error, G5Error::Unauthorized | G5Error::Forbidden { .. })
        || matches!(error, G5Error::Conflict { reason } if reason.starts_with("Repository credentials: "))
}

async fn resolve_worker_auth(
    deps: &BranchIntegrationWorkerDeps,
    state: &CycleState,
    operation: RepositoryOperation,
) -> Result<GitAuth, G5Error> {
    deps.repository_credentials
        .resolve(
            state.org_id,
            state.key.repo_id,
            state.acting_user_id,
            &state.git_url,
            operation,
        )
        .await
}

// ---------------------------------------------------------------------------
// Action execution
// ---------------------------------------------------------------------------

// LEAN-TODO(P1.5): retained remote-tracking merge and handoff ordering.
async fn execute_decision(
    deps: &BranchIntegrationWorkerDeps,
    conn: &mut PgConnection,
    state: &CycleState,
    picked: &PickedQueueRow,
    prepared: PreparedDecision,
) -> Result<CompletedCycle, AppError> {
    let PreparedDecision {
        input,
        decision,
        mut push_result,
        resume_pause_only,
        reclassify_checkpoint,
        authority_advance,
        co_members,
    } = prepared;
    if reclassify_checkpoint {
        obsolete_false_rewrite_checkpoint(conn, state.key).await?;
    }
    let mirror_tip = input.mirror_tip.clone();
    // Publish provider observations with the fenced cycle result. They do
    // not select the shared workspace head or its content projections.
    let mut completed = CompletedCycle {
        provider_tip: mirror_tip.clone(),
        ..Default::default()
    };

    match &decision.action {
        Action::Imported => {
            // The shared landing owns the branch, watermark, provider ref and
            // audit receipt. Its final admission repeats the policy/lease and
            // complete Git vector checks after this transaction closes.
            completed.authority_advance = Some(authority_advance.ok_or_else(|| {
                AppError::from_error(G5Error::Internal {
                    message: "Imported action is missing its prepared publication".into(),
                })
            })?);
            if let Some(tip) = mirror_tip.as_ref() {
                refresh_advisory(conn, state.key, tip).await?;
            }
            for co in &co_members {
                refresh_advisory(conn, co.state.key, &co.provider_tip).await?;
            }
        }
        Action::Noop => {
            info!(
                workspace_repo = %state.key,
                remote_branch = %state.remote_branch,
                workspace_tip = %state.workspace_tip,
                mirror_tip = ?mirror_tip,
                "branch_integration_worker: noop — nothing to pick up from remote",
            );
            if decision.refresh_advisory {
                if let Some(tip) = mirror_tip.as_ref() {
                    refresh_advisory(conn, state.key, tip).await?;
                }
            }
            // §6.1/§5.2 invariant: workspace tip == mirror tip ⇒ baseline == tip.
            // When the tips are equal but the watermark lags, catch `baseline`
            // up to the tip — the co-tracking inherited-commit convergence
            // (design §3.3 D90/C7): a peer pushed the identical commit this
            // workspace already had, so the tip reached the shared remote
            // without THIS workspace pushing or importing (`head == mirror ==
            // tip`, but `baseline` still at the old sync point). The advisory is
            // already advanced above; baseline must reach the tip too or the
            // read-side `isBehind = advisory != baseline` stays falsely lit
            // (C16 — the Noop-path analogue of the push C30 / import C31
            // convergence; worker leaf D6).
            let mut noop_new_baseline: Option<String> = None;
            let mut tips_converged = false;
            if let Some(tip) = mirror_tip.as_ref() {
                if tip == &state.workspace_tip {
                    tips_converged = true;
                    if tip != &state.baseline {
                        record_push_converged(conn, state.key, tip).await?;
                        noop_new_baseline = Some(tip.clone());
                    }
                    completed.tracking_ref_writes.push(PendingBaselineRefWrite {
                        repo_id: state.repo_id,
                        expected_watermark: (state.baseline != ZERO_GIT_SHA_SENTINEL)
                            .then(|| state.baseline.clone()),
                        local_branch_name: state.local_branch_name.clone(),
                        new_baseline: tip.clone(),
                    });
                } else if input.tip_relation == TipRelation::WorkspaceAhead
                    && tip != &state.baseline
                {
                    // Manual policy left a workspace that already contains a
                    // newer provider tip (e.g. a resolved divergence rebased onto
                    // it) unpublished. That tip is the newest shared point, so the
                    // watermark catches up to it; otherwise the read-side
                    // `isBehind = advisory != baseline` reports "behind" for a
                    // workspace that is only ahead.
                    noop_new_baseline = Some(tip.clone());
                    completed.tracking_ref_writes.push(PendingBaselineRefWrite {
                        repo_id: state.repo_id,
                        expected_watermark: (state.baseline != ZERO_GIT_SHA_SENTINEL)
                            .then(|| state.baseline.clone()),
                        local_branch_name: state.local_branch_name.clone(),
                        new_baseline: tip.clone(),
                    });
                }
            }
            write_audit_row(
                conn,
                state,
                picked.trigger_kind,
                &decision,
                mirror_tip.as_deref(),
                noop_new_baseline.as_deref(),
                None,
                None,
            )
            .await?;
            if resume_pause_only && tips_converged {
                clear_resumed_divergence_pause(conn, state.key).await?;
            }
        }
        Action::Fetched => {
            if let Some(tip) = mirror_tip.as_ref() {
                refresh_advisory(conn, state.key, tip).await?;
            }
            write_audit_row(
                conn,
                state,
                picked.trigger_kind,
                &decision,
                mirror_tip.as_deref(),
                None,
                None,
                None,
            )
            .await?;
        }
        Action::Pushed => {
            info!(
                workspace_repo = %state.key,
                remote_branch = %state.remote_branch,
                local_branch_name = %state.local_branch_name,
                workspace_tip = %state.workspace_tip,
                "branch_integration_worker: pushing local tip to remote",
            );
            let push_result = push_result.take().ok_or_else(|| {
                AppError::from_error(G5Error::Internal {
                    message: "Pushed action is missing its remote result".to_string(),
                })
            })?;
            match push_result {
                Ok(()) => {
                    completed.provider_tip = Some(state.workspace_tip.clone());
                    info!(
                        workspace_repo = %state.key,
                        workspace_tip = %state.workspace_tip,
                        "branch_integration_worker: push succeeded",
                    );
                    // The push fast-forwarded the remote to our workspace tip,
                    // so workspace_tip == mirror_tip now. Per design §6.2 (the
                    // clean + dirty push rows: "on a push, advance baseline")
                    // and the §6.1 watermark invariant (workspace tip ==
                    // mirror tip ⇒ baseline == workspace tip), advance the
                    // synchronization watermark AND the UI advisory to the
                    // just-pushed tip.
                    //
                    // C30 regression: the old code refreshed the advisory to
                    // the *pre-push* `mirror_tip` (the remote's value BEFORE
                    // this push) and never advanced baseline. That left
                    // `remote_tip_advisory_sha` strictly below `head_commit`,
                    // so the client's `isBehind` (advisory != head) kept the
                    // branch flagged "behind — merge required" after a
                    // successful push and the UI never acknowledged the push.
                    record_push_converged(conn, state.key, &state.workspace_tip).await?;
                    // Publish the synchronization ref under the workspace
                    // fence. SQL records the operation outcome, not Git's tip.
                    completed.tracking_ref_writes.push(PendingBaselineRefWrite {
                        repo_id: state.repo_id,
                        expected_watermark: (state.baseline != ZERO_GIT_SHA_SENTINEL)
                            .then(|| state.baseline.clone()),
                        local_branch_name: state.local_branch_name.clone(),
                        new_baseline: state.workspace_tip.clone(),
                    });
                    write_audit_row(
                        conn,
                        state,
                        picked.trigger_kind,
                        &decision,
                        // Remote is now at our tip: new_mirror_tip (also bound
                        // to the audit advisory_sha column) and new_baseline
                        // are both the pushed tip.
                        Some(&state.workspace_tip),
                        Some(&state.workspace_tip),
                        None,
                        None,
                    )
                    .await?;
                    if resume_pause_only {
                        clear_resumed_divergence_pause(conn, state.key).await?;
                    }
                }
                Err(msg) => {
                    warn!(
                        workspace_repo = %state.key,
                        error = %msg,
                        "branch_integration_worker: push_failed",
                    );
                    let scrubbed = scrub_opt(Some(msg));
                    write_push_failed_audit(
                        conn,
                        deps,
                        state,
                        picked.trigger_kind,
                        mirror_tip.as_deref(),
                        scrubbed,
                    )
                    .await?;
                }
            }
        }
        Action::DivergedPauseOnly => {
            set_sync_paused_reason(conn, state.key, "diverged").await?;
            warn!(
                workspace_repo = %state.key,
                workspace_id = %state.workspace_id,
                workspace_tip = %state.workspace_tip,
                mirror_tip = ?mirror_tip,
                baseline = %state.baseline,
                "branch_integration_worker: automatic auto-push divergence paused without opening reconciliation",
            );
            write_audit_row(
                conn,
                state,
                picked.trigger_kind,
                &decision,
                mirror_tip.as_deref(),
                None,
                None,
                None,
            )
            .await?;
        }
        Action::Diverged { phase } => {
            let reason = match phase {
                DivergedPhase::WorkspaceRebasePreflight => "diverged",
                DivergedPhase::ResetBelowBaseline => "watermark_violation",
            };
            set_sync_paused_reason(conn, state.key, reason).await?;

            // Phase 4.A: open a `workspace_merge_runs` row of
            // `source_kind = 'remote_tracking'` so operators can resolve
            // the divergence through the existing merge handoff workflow
            // (arch D55, D60).
            let merge_run_id = open_remote_tracking_merge_run(
                conn,
                state.workspace_id,
                state.repo_id,
                &state.remote_branch,
                *phase,
                &state.workspace_tip,
                mirror_tip.as_deref(),
                &state.baseline,
            )
            .await?;

            warn!(
                workspace_repo = %state.key,
                workspace_id = %state.workspace_id,
                phase = ?phase,
                reason = %reason,
                workspace_tip = %state.workspace_tip,
                mirror_tip = ?mirror_tip,
                baseline = %state.baseline,
                merge_run_id = %merge_run_id,
                "branch_integration_worker: diverged — opened merge run, sync paused until operator resolves",
            );

            write_audit_row(
                conn,
                state,
                picked.trigger_kind,
                &decision,
                mirror_tip.as_deref(),
                None,
                None,
                Some(merge_run_id),
            )
            .await?;
        }
    }

    Ok(completed)
}

/// Publish the captured workspace tip, leased on the fetched provider tip.
/// `replace` is set only for a classified workspace rewrite against that
/// unchanged tip (`g5.governance.remote.workspace-rewrite`).
async fn push_workspace_tip(
    deps: &BranchIntegrationWorkerDeps,
    state: &CycleState,
    picked: &PickedQueueRow,
    provider_tip: &str,
    replace: bool,
) -> Result<(), String> {
    // Backstop for `g5.workspace.remote.base-parity`: the classifier never
    // yields a push for Base, and no executor publishes Base either.
    if state.policy.is_base {
        return Err("Base never publishes to its provider".to_string());
    }
    let provider_tip = provider_tip.to_owned();
    let local_path = deps
        .repo_storage
        .path(&state.repo_key)
        .map_err(|error| format!("local mirror path not configured: {error}"))?;

    validate_push_authority(deps, state, picked).await?;
    let operation = if picked.is_explicit() {
        RepositoryOperation::WriteGit
    } else {
        RepositoryOperation::AutoPush
    };
    let auth = resolve_worker_auth(deps, state, operation)
        .await
        .map_err(|error| error.to_string())?;
    validate_push_authority(deps, state, picked).await?;

    let captured_tip = state.workspace_tip.clone();
    let remote_branch = state.remote_branch.clone();
    let git_url = state.git_url.clone();
    let local_path_clone = local_path.clone();

    let git = crate::git_runtime::network(FETCH_TIMEOUT).map_err(|e| e.to_string())?;
    let join = tokio::time::timeout(
        FETCH_TIMEOUT,
        tokio::task::spawn_blocking(move || -> Result<(), String> {
            let repo = GitRepo::open(&local_path_clone).map_err(|e| e.to_string())?;
            repo.ensure_remote_url("origin", &git_url)
                .map_err(|e| e.to_string())?;
            repo.push_existing_commit(
                &git,
                "origin",
                &captured_tip,
                &remote_branch,
                &provider_tip,
                replace,
                &auth,
            )
            .map_err(|e| e.to_string())
        }),
    )
    .await;

    match join {
        Ok(Ok(Ok(()))) => Ok(()),
        Ok(Ok(Err(msg))) => Err(msg),
        Ok(Err(join_err)) => Err(format!("push task panicked: {join_err}")),
        Err(_) => Err("git push timed out".to_string()),
    }
}

/// Recheck the admitted request before resolving write credentials and again
/// before transport. Credentials always belong to the captured actor; a transfer
/// after admission cannot switch a running push to the recipient's account.
async fn validate_push_authority(
    deps: &BranchIntegrationWorkerDeps,
    state: &CycleState,
    picked: &PickedQueueRow,
) -> Result<(), String> {
    let mut db = scoped_db_for_maintenance_org(&deps.tenant_deps(), state.org_id)
        .await
        .map_err(|error| error.to_string())?;
    if picked.is_explicit() {
        let expected = picked
            .explicit_authority()
            .ok_or("Explicit push has no authorizing owner; request it again")?;
        let current =
            PushAuthority::capture(db.conn(), state.workspace_id, expected.requested_by_user_id)
                .await
                .map_err(|error| error.to_string())?;
        if current != expected || current != state.push_authority() {
            return Err("Explicit push authorization expired; request it again".into());
        }
    }
    let current = remote_integrations::load(db.conn(), state.key, false)
        .await
        .map_err(|error| error.to_string())?;
    if current
        .as_ref()
        .is_none_or(|current| current.publication_policy() != state.policy)
    {
        return Err("Remote publication policy changed; request publication again".into());
    }
    let owned: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM remote_integration_queue
         WHERE workspace_id=$1 AND repo_id=$2 AND locked_by=$3 AND locked_until > now()
           AND requested_at=$4 AND advance_id IS NOT DISTINCT FROM $5 AND trigger_kind=$6
           AND requested_by_user_id IS NOT DISTINCT FROM $7 AND lifecycle_generation IS NOT DISTINCT FROM $8)",
    ).bind(state.workspace_id).bind(state.repo_id).bind(picked.owner_token)
        .bind(picked.requested_at).bind(picked.advance_id).bind(picked.trigger_kind.as_audit_str())
        .bind(picked.requested_by_user_id).bind(picked.lifecycle_generation)
        .fetch_one(db.conn()).await.map_err(|error| error.to_string())?;
    db.commit().await.map_err(|error| error.to_string())?;
    if !owned {
        return Err("Remote publication request was replaced; retry the current request".into());
    }
    Ok(())
}

struct ApprovedRebasePublishProof {
    merge_run_id: Uuid,
    expected_run_status: String,
    source_heads: serde_json::Value,
    approved_workspace_tip: String,
    expected_authority_tip: String,
    expected_remote_tip: String,
}

async fn load_approved_rebase_publish_proof(
    conn: &mut PgConnection,
    key: WorkspaceRepoKey,
) -> Result<ApprovedRebasePublishProof, String> {
    #[derive(sqlx::FromRow)]
    struct Row {
        id: Uuid,
        status: String,
        source_heads: Option<serde_json::Value>,
        target_heads: Option<serde_json::Value>,
    }

    let row: Option<Row> = sqlx::query_as(
        r#"
        SELECT id, status, source_heads, target_heads
        FROM workspace_merge_runs
        WHERE workspace_id = $1 AND repo_id = $2 AND org_id = $3
          AND source_kind = 'remote_tracking'
          AND phase = 'reset_below_baseline'
          AND status IN ('waiting_for_human', 'running')
        ORDER BY updated_at DESC NULLS LAST, started_at DESC NULLS LAST, id DESC
        LIMIT 1
        FOR UPDATE
        "#,
    )
    .bind(key.workspace_id)
    .bind(key.repo_id)
    .bind(key.org_id)
    .fetch_optional(&mut *conn)
    .await
    .map_err(|e| format!("failed to load approved rebase merge run: {e}"))?;

    let row = row.ok_or_else(|| {
        "approved_rebase_publish requires an active reset-below-baseline merge run".to_string()
    })?;
    let target_heads = row
        .target_heads
        .as_ref()
        .ok_or_else(|| "approved rebase merge run is missing target_heads".to_string())?;
    let source_heads = row
        .source_heads
        .ok_or_else(|| "approved rebase merge run is missing source_heads".to_string())?;
    let approved_workspace_tip = target_heads
        .get("approved_workspace_tip")
        .and_then(|value| value.as_str())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "approved rebase merge run is missing approved_workspace_tip".to_string())?
        .to_string();
    let expected_authority_tip = target_heads
        .get("expected_authority_tip")
        .and_then(|value| value.as_str())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "approved rebase merge run is missing expected_authority_tip".to_string())?
        .to_string();
    let expected_remote_tip = target_heads
        .get("expected_remote_tip")
        .and_then(|value| value.as_str())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "approved rebase merge run is missing expected_remote_tip".to_string())?
        .to_string();

    Ok(ApprovedRebasePublishProof {
        merge_run_id: row.id,
        expected_run_status: row.status,
        source_heads,
        approved_workspace_tip,
        expected_authority_tip,
        expected_remote_tip,
    })
}

async fn block_workspace_approved_rebase_publish_set(
    conn: &mut PgConnection,
    state: &CycleState,
    reason: &str,
) -> Result<(), AppError> {
    let scrubbed = scrub_opt(Some(reason.to_string()));
    sqlx::query(
        r#"
        UPDATE workspace_merge_runs jmr
           SET status = 'waiting_for_human',
               blocking_reason = $2,
               updated_at = now()
         WHERE jmr.workspace_id = $1
           AND jmr.source_kind = 'remote_tracking'
           AND jmr.phase = 'reset_below_baseline'
           AND jmr.status IN ('waiting_for_human', 'running')
        "#,
    )
    .bind(state.workspace_id)
    .bind(scrubbed)
    .execute(&mut *conn)
    .await
    .map_err(AppError::from)?;

    sqlx::query(
        r#"
        DELETE FROM remote_integration_queue q
        WHERE q.workspace_id = $1 AND q.trigger_kind = 'approved_rebase_publish'
        "#,
    )
    .bind(state.workspace_id)
    .execute(&mut *conn)
    .await
    .map_err(AppError::from)?;
    Ok(())
}

async fn finish_approved_rebase_publish_failed(
    deps: &BranchIntegrationWorkerDeps,
    conn: &mut PgConnection,
    picked: &PickedQueueRow,
    state: &CycleState,
    _merge_run_id: Option<Uuid>,
    msg: String,
) -> Result<CompletedCycle, AppError> {
    block_workspace_approved_rebase_publish_set(conn, state, &msg).await?;
    write_push_failed_audit(
        conn,
        deps,
        state,
        picked.trigger_kind,
        None,
        scrub_opt(Some(msg)),
    )
    .await?;
    delete_queue_row(conn, picked).await?;
    Ok(CompletedCycle::default())
}

async fn prepare_approved_rebase_publish(
    deps: &BranchIntegrationWorkerDeps,
    state: &CycleState,
    proof: Option<Result<ApprovedRebasePublishProof, String>>,
) -> ApprovedRebaseRemoteOutcome {
    let mut merge_run_id = None;

    if state.policy.is_base {
        return ApprovedRebaseRemoteOutcome::Failed {
            merge_run_id,
            message: "Base never publishes to its provider; accept the provider history instead"
                .to_string(),
        };
    }
    if state.sync_paused_reason.as_deref() != Some("watermark_violation") {
        return ApprovedRebaseRemoteOutcome::Failed {
            merge_run_id,
            message: format!(
                "approved_rebase_publish requires sync_paused_reason=watermark_violation, found {:?}",
                state.sync_paused_reason
            ),
        };
    }

    let proof = match proof {
        Some(Ok(proof)) => proof,
        Some(Err(message)) => {
            return ApprovedRebaseRemoteOutcome::Failed {
                merge_run_id,
                message,
            };
        }
        None => {
            return ApprovedRebaseRemoteOutcome::Failed {
                merge_run_id,
                message: "approved_rebase_publish cycle is missing its claimed merge-run proof"
                    .to_string(),
            };
        }
    };
    merge_run_id = Some(proof.merge_run_id);

    if proof.expected_authority_tip != state.workspace_tip {
        return ApprovedRebaseRemoteOutcome::Failed {
            merge_run_id,
            message: format!(
                "approved authority tip changed: reviewed {}, current {}",
                proof.expected_authority_tip, state.workspace_tip
            ),
        };
    }

    let repo_path = match deps.repo_storage.path(&state.repo_key) {
        Ok(repo_path) => repo_path,
        Err(_) => {
            return ApprovedRebaseRemoteOutcome::Failed {
                merge_run_id,
                message: "local mirror path not configured".to_string(),
            };
        }
    };
    let candidate_tip = proof.approved_workspace_tip.clone();
    let authority_tip = proof.expected_authority_tip.clone();
    let remote_tip = proof.expected_remote_tip.clone();
    let normalized_tip = match tokio::time::timeout(
        FETCH_TIMEOUT,
        tokio::task::spawn_blocking(move || {
            GitRepo::open(&repo_path)
                .and_then(|repo| {
                    repo.normalize_reviewed_publication_tip(
                        &candidate_tip,
                        &authority_tip,
                        &remote_tip,
                    )
                })
                .map_err(|error| error.to_string())
        }),
    )
    .await
    {
        Ok(Ok(Ok(normalized_tip))) => normalized_tip,
        Ok(Ok(Err(message))) => {
            return ApprovedRebaseRemoteOutcome::Failed {
                merge_run_id,
                message,
            };
        }
        Ok(Err(join_error)) => {
            return ApprovedRebaseRemoteOutcome::Failed {
                merge_run_id,
                message: format!("approved publication normalization task panicked: {join_error}"),
            };
        }
        Err(_) => {
            return ApprovedRebaseRemoteOutcome::Failed {
                merge_run_id,
                message: "approved publication normalization timed out".to_string(),
            };
        }
    };

    let authority_inventory = match state.authority_inventory() {
        Ok(authority_inventory) => authority_inventory,
        Err(error) => {
            return ApprovedRebaseRemoteOutcome::Failed {
                merge_run_id,
                message: error.to_string(),
            };
        }
    };
    let exact_repos = match authority_inventory
        .repos
        .iter()
        .map(|repo| {
            let expected_old_head = state
                .authority_heads
                .get(&repo.repo_id)
                .cloned()
                .ok_or_else(|| {
                    format!(
                        "Materialized workspace authority member {} has no exact Git SHA",
                        repo.repo_id
                    )
                })?;
            Ok(WorkspaceHeadRepoInput {
                repo_id: repo.repo_id,
                proposed_new_head: if repo.repo_id == state.repo_id {
                    normalized_tip.clone()
                } else {
                    expected_old_head.clone()
                },
                expected_old_head,
            })
        })
        .collect::<Result<Vec<_>, String>>()
    {
        Ok(exact_repos) => exact_repos,
        Err(message) => {
            return ApprovedRebaseRemoteOutcome::Failed {
                merge_run_id,
                message,
            };
        }
    };
    let preparation_context =
        match scoped_db_for_maintenance_org(&deps.tenant_deps(), state.org_id).await {
            Ok(preparation_context) => preparation_context,
            Err(error) => {
                return ApprovedRebaseRemoteOutcome::Failed {
                    merge_run_id,
                    message: error.to_string(),
                };
            }
        };
    let preparation_scope = preparation_context.reopener();
    if let Err(error) = preparation_context.rollback().await {
        return ApprovedRebaseRemoteOutcome::Failed {
            merge_run_id,
            message: error.to_string(),
        };
    }
    // LEAN-TODO(P1.5): retained remote-tracking merge and handoff ordering.
    // Landing this cut completes the remote-tracking merge run, so the
    // landing's external-Git handoff enqueues `push` instead of `commit`.
    let authority_advance = match prepare_branch_commit(
        &preparation_scope,
        &deps.repo_storage,
        authority_inventory.target.clone(),
        exact_repos,
        BRANCH_COMMIT_ACTOR,
    )
    .await
    {
        Ok(authority_advance) => {
            authority_advance.with_remote_tracking_completion(state.push_authority())
        }
        Err(error) => {
            return ApprovedRebaseRemoteOutcome::Failed {
                merge_run_id,
                message: map_branch_commit_error(error).to_string(),
            };
        }
    };
    let selected_repo = authority_advance.member(state.repo_id);
    if selected_repo.is_none_or(|repo| {
        repo.expected_old_head != proof.expected_authority_tip
            || repo.proposed_new_head != normalized_tip
    }) {
        return ApprovedRebaseRemoteOutcome::Failed {
            merge_run_id,
            message: "prepared approved publication did not preserve its reviewed exact cut"
                .to_string(),
        };
    }

    ApprovedRebaseRemoteOutcome::Ready(Box::new(PreparedApprovedRebasePublish {
        proof,
        normalized_tip,
        authority_advance,
    }))
}

// LEAN-TODO(P1.5): retained remote-tracking merge and handoff ordering.
async fn apply_approved_rebase_publish(
    deps: &BranchIntegrationWorkerDeps,
    conn: &mut PgConnection,
    picked: &PickedQueueRow,
    state: CycleState,
    outcome: ApprovedRebaseRemoteOutcome,
) -> Result<CompletedCycle, AppError> {
    let prepared = match outcome {
        ApprovedRebaseRemoteOutcome::Ready(prepared) => *prepared,
        ApprovedRebaseRemoteOutcome::Failed {
            merge_run_id,
            message,
        } => {
            return finish_approved_rebase_publish_failed(
                deps,
                conn,
                picked,
                &state,
                merge_run_id,
                message,
            )
            .await;
        }
    };
    register_remote_tracking_merge_completion_consumer(
        conn,
        &prepared.authority_advance,
        &RemoteTrackingMergeCompletionConsumerInput {
            merge_run_id: prepared.proof.merge_run_id,
            workspace_id: state.workspace_id,
            repo_id: state.repo_id,
            expected_watermark: (state.baseline != ZERO_GIT_SHA_SENTINEL)
                .then(|| state.baseline.clone()),
            synchronized_remote_tip: prepared.proof.expected_remote_tip.clone(),
            expected_run_status: prepared.proof.expected_run_status.clone(),
            expected_run_phase: "reset_below_baseline".to_string(),
            expected_authority_tip: prepared.proof.expected_authority_tip.clone(),
            reconciled_tip: prepared.normalized_tip.clone(),
            final_source_heads: prepared.proof.source_heads.clone(),
            confirmed_by: state.acting_user_id,
            authority_audit: Some(RemoteTrackingAuthorityAuditInput {
                org_id: state.org_id,
                trigger_kind: picked.trigger_kind.as_audit_str().to_string(),
                old_workspace_tip: prepared.proof.expected_authority_tip.clone(),
                remote_tip: prepared.proof.expected_remote_tip.clone(),
            }),
        },
    )
    .await
    .map_err(AppError::from)?;

    Ok(CompletedCycle {
        authority_advance: Some(super::PreparedRemotePublication::new(
            prepared.authority_advance,
        )),
        tracking_ref_writes: Vec::new(),
        provider_tip: None,
    })
}

/// Observations remain transient and never participate in a publication fence.
async fn refresh_advisory(
    conn: &mut PgConnection,
    key: WorkspaceRepoKey,
    tip: &str,
) -> Result<(), AppError> {
    if let Some(policy) = remote_integrations::load(conn, key, false).await? {
        remote_integrations::observe_tip(conn, &policy, tip).await?;
    }
    Ok(())
}

async fn record_push_converged(
    conn: &mut PgConnection,
    key: WorkspaceRepoKey,
    tip: &str,
) -> Result<(), AppError> {
    refresh_advisory(conn, key, tip).await?;
    sqlx::query(
        "UPDATE remote_integrations SET last_sync_at = now(), updated_at = now()
                 WHERE workspace_id = $1 AND repo_id = $2 AND org_id = $3",
    )
    .bind(key.workspace_id)
    .bind(key.repo_id)
    .bind(key.org_id)
    .execute(conn)
    .await?;
    Ok(())
}

async fn set_sync_paused_reason(
    conn: &mut PgConnection,
    key: WorkspaceRepoKey,
    reason: &str,
) -> Result<(), AppError> {
    sqlx::query(
        "UPDATE remote_integrations SET sync_paused_reason = $4, updated_at = now()
                 WHERE org_id = $1 AND workspace_id = $2 AND repo_id = $3",
    )
    .bind(key.org_id)
    .bind(key.workspace_id)
    .bind(key.repo_id)
    .bind(reason)
    .execute(conn)
    .await?;
    Ok(())
}

/// Clear only the stale pause that authorized this explicit resumption.
/// Exact head and reason guards keep auth/fetch/push failures, head races, and
/// any newer operator pause untouched.
async fn clear_resumed_divergence_pause(
    conn: &mut PgConnection,
    key: WorkspaceRepoKey,
) -> Result<(), AppError> {
    let cleared = sqlx::query("UPDATE remote_integrations SET sync_paused_reason = NULL, updated_at = now()
                 WHERE org_id = $1 AND workspace_id = $2 AND repo_id = $3 AND sync_paused_reason = 'diverged'")
        .bind(key.org_id).bind(key.workspace_id).bind(key.repo_id).execute(conn).await?;
    if cleared.rows_affected() != 1 {
        return Err(AppError::from_error(G5Error::Conflict {
            reason: "Remote integration pause changed before explicit convergence".into(),
        }));
    }
    Ok(())
}

/// Whether the repository's only active run is an unapproved provider-rewrite
/// checkpoint. An approved publication in flight keeps its checkpoint.
async fn is_unapproved_rewrite_checkpoint(
    conn: &mut PgConnection,
    key: WorkspaceRepoKey,
) -> Result<bool, AppError> {
    Ok(sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM workspace_merge_runs
            WHERE workspace_id = $1 AND repo_id = $2 AND org_id = $3 AND source_kind = 'remote_tracking'
              AND phase = 'reset_below_baseline' AND status = 'waiting_for_human')
          AND NOT EXISTS (SELECT 1 FROM workspace_merge_runs
            WHERE workspace_id = $1 AND repo_id = $2 AND org_id = $3 AND source_kind = 'remote_tracking'
              AND status IN ('pending', 'running', 'waiting_for_human', 'rolling_back')
              AND NOT (phase = 'reset_below_baseline' AND status = 'waiting_for_human'))
          AND NOT EXISTS (SELECT 1 FROM remote_integration_queue
            WHERE workspace_id = $1 AND trigger_kind = 'approved_rebase_publish')",
    )
    .bind(key.workspace_id)
    .bind(key.repo_id)
    .bind(key.org_id)
    .fetch_one(conn)
    .await?)
}

/// Close a provider-rewrite checkpoint that fresh classification proved to be
/// a workspace rewrite or convergence. The run keeps its audit history; the
/// pause clears only if it is still the one this cycle captured.
async fn obsolete_false_rewrite_checkpoint(
    conn: &mut PgConnection,
    key: WorkspaceRepoKey,
) -> Result<(), AppError> {
    sqlx::query(
        "UPDATE workspace_merge_runs SET status = 'cancelled', blocking_reason = 'reclassified_workspace_rewrite',
                completed_at = now(), updated_at = now()
          WHERE workspace_id = $1 AND repo_id = $2 AND org_id = $3 AND source_kind = 'remote_tracking'
            AND phase = 'reset_below_baseline' AND status = 'waiting_for_human'",
    )
    .bind(key.workspace_id)
    .bind(key.repo_id)
    .bind(key.org_id)
    .execute(&mut *conn)
    .await?;
    let cleared = sqlx::query(
        "UPDATE remote_integrations SET sync_paused_reason = NULL, updated_at = now()
          WHERE org_id = $1 AND workspace_id = $2 AND repo_id = $3
            AND sync_paused_reason = 'watermark_violation'",
    )
    .bind(key.org_id)
    .bind(key.workspace_id)
    .bind(key.repo_id)
    .execute(&mut *conn)
    .await?;
    if cleared.rows_affected() != 1 {
        return Err(AppError::from_error(G5Error::Conflict {
            reason: "Remote integration pause changed before checkpoint reclassification".into(),
        }));
    }
    Ok(())
}

async fn has_active_remote_tracking_merge_run(
    conn: &mut PgConnection,
    key: WorkspaceRepoKey,
) -> Result<bool, AppError> {
    Ok(sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM workspace_merge_runs
        WHERE workspace_id = $1 AND repo_id = $2 AND org_id = $3 AND source_kind = 'remote_tracking'
          AND status IN ('pending', 'running', 'waiting_for_human', 'rolling_back'))",
    )
    .bind(key.workspace_id)
    .bind(key.repo_id)
    .bind(key.org_id)
    .fetch_one(conn)
    .await?)
}

// ---------------------------------------------------------------------------
// Audit-row writes
// ---------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
async fn write_audit_row(
    conn: &mut PgConnection,
    state: &CycleState,
    trigger_kind: TriggerKind,
    decision: &Decision,
    new_mirror_tip: Option<&str>,
    new_baseline: Option<&str>,
    override_error: Option<&str>,
    merge_run_id: Option<Uuid>,
) -> Result<(), AppError> {
    let error_detail = override_error
        .map(|s| s.to_string())
        .or_else(|| decision.audit_error_detail.map(str::to_string));
    let error_detail = scrub_opt(error_detail);
    if new_baseline.is_none()
        && merge_run_id.is_none()
        && extend_repeated_background_run(
            conn,
            state,
            trigger_kind,
            decision.audit_status,
            new_mirror_tip,
            error_detail.as_deref(),
        )
        .await?
    {
        return Ok(());
    }

    sqlx::query(
        r#"
        INSERT INTO remote_sync_runs (
            org_id, workspace_id, repo_id, acting_user_id,
            kind, trigger_kind, status,
            old_workspace_tip, new_workspace_tip,
            old_mirror_tip, new_mirror_tip,
            new_baseline, error_detail,
            merge_run_id,
            started_at, completed_at
        ) VALUES (
            $1, $2, $12, $3,
            'integration', $4, $5,
            $6, $7,
            NULL, $8,
            $9, $10,
            $11,
            now(), now()
        )
        "#,
    )
    .bind(state.org_id)
    .bind(state.workspace_id)
    .bind(state.acting_user_id)
    .bind(trigger_kind.as_audit_str())
    .bind(decision.audit_status)
    .bind(state.workspace_tip.clone())
    .bind(state.workspace_tip.clone())
    .bind(new_mirror_tip)
    .bind(new_baseline)
    .bind(error_detail)
    .bind(merge_run_id)
    .bind(state.repo_id)
    .execute(&mut *conn)
    .await
    .map_err(AppError::from)?;
    Ok(())
}

/// Extends the membership's latest integration run instead of adding a row
/// when a background pass repeats its observation exactly: same outcome, tips,
/// error and acting user, with no watermark or merge-run effect. The run's
/// `started_at` keeps the first observation and `completed_at` becomes the
/// latest, so an idle or persistently failing integration adds no rows while
/// the latest status every reader consults stays current
/// (`g5.governance.remote.sync-history`).
async fn extend_repeated_background_run(
    conn: &mut PgConnection,
    state: &CycleState,
    trigger_kind: TriggerKind,
    status: &str,
    new_mirror_tip: Option<&str>,
    error_detail: Option<&str>,
) -> Result<bool, AppError> {
    if trigger_kind != TriggerKind::BackgroundInbound
        || ![
            run_status::NOOP,
            run_status::FETCHED,
            run_status::FETCH_FAILED,
        ]
        .contains(&status)
    {
        return Ok(false);
    }
    let extended = sqlx::query(
        r#"
        UPDATE remote_sync_runs SET completed_at = now()
        WHERE id = (
                SELECT id FROM remote_sync_runs
                WHERE org_id = $1 AND workspace_id = $2 AND repo_id = $3 AND kind = 'integration'
                ORDER BY started_at DESC, id DESC LIMIT 1
            )
          AND trigger_kind = $4 AND status = $5 AND acting_user_id = $6
          AND old_workspace_tip IS NOT DISTINCT FROM $7
          AND new_workspace_tip IS NOT DISTINCT FROM $7
          AND new_mirror_tip IS NOT DISTINCT FROM $8
          AND error_detail IS NOT DISTINCT FROM $9
          AND new_baseline IS NULL AND merge_run_id IS NULL
        "#,
    )
    .bind(state.org_id)
    .bind(state.workspace_id)
    .bind(state.repo_id)
    .bind(trigger_kind.as_audit_str())
    .bind(status)
    .bind(state.acting_user_id)
    .bind(&state.workspace_tip)
    .bind(new_mirror_tip)
    .bind(error_detail)
    .execute(&mut *conn)
    .await?;
    Ok(extended.rows_affected() == 1)
}

/// Writes the `merge_active` short-circuit audit row for a queue pickup
/// that found the branch paused with an active `remote_tracking` merge
/// run (`sync_paused_reason ∈ {'diverged', 'watermark_violation'}`).
///
/// Per design §8.2 + test-view D39: a commit landing while a merge run is
/// active is allowed and re-enqueues, but the worker must NOT fetch or
/// push — it records exactly one `remote_sync_runs` row with
/// `status='dirty_skipped'`, `error_detail='merge_active'` so the audit
/// trail shows why the queued work was deferred, then the caller deletes
/// the queue row. No tip/baseline movement: `new_workspace_tip ==
/// old_workspace_tip`, all mirror/baseline columns NULL. `merge_run_id`
/// is left NULL here — the run id was already stamped on the `diverged`
/// audit row written when the run was opened; this short-circuit row only
/// signals "blocked by an in-progress merge."
///
/// `'merge_active'` is a fixed literal (no caller-supplied free text), so
/// no credential scrub is required.
async fn write_merge_active_audit(
    conn: &mut PgConnection,
    state: &CycleState,
    trigger_kind: TriggerKind,
) -> Result<(), AppError> {
    sqlx::query(
        r#"
        INSERT INTO remote_sync_runs (
            org_id, workspace_id, repo_id, acting_user_id,
            kind, trigger_kind, status,
            old_workspace_tip, new_workspace_tip, error_detail,
            started_at, completed_at
        ) VALUES (
            $1, $2, $6, $3,
            'integration', $4, 'dirty_skipped',
            $5, $5, 'merge_active',
            now(), now()
        )
        "#,
    )
    .bind(state.org_id)
    .bind(state.workspace_id)
    .bind(state.acting_user_id)
    .bind(trigger_kind.as_audit_str())
    .bind(state.workspace_tip.clone())
    .bind(state.repo_id)
    .execute(&mut *conn)
    .await
    .map_err(AppError::from)?;
    Ok(())
}

async fn write_auth_required_audit(
    conn: &mut PgConnection,
    _deps: &BranchIntegrationWorkerDeps,
    state: &CycleState,
    trigger_kind: TriggerKind,
    error_msg: Option<String>,
) -> Result<(), AppError> {
    let scrubbed = scrub_opt(error_msg);
    set_sync_paused_reason(conn, state.key, "auth_required").await?;
    sqlx::query(
        r#"
        INSERT INTO remote_sync_runs (
            org_id, workspace_id, repo_id, acting_user_id,
            kind, trigger_kind, status,
            old_workspace_tip, new_workspace_tip, error_detail,
            started_at, completed_at
        ) VALUES (
            $1, $2, $7, $3,
            'integration', $4, 'auth_required',
            $5, $5, $6,
            now(), now()
        )
        "#,
    )
    .bind(state.org_id)
    .bind(state.workspace_id)
    .bind(state.acting_user_id)
    .bind(trigger_kind.as_audit_str())
    .bind(state.workspace_tip.clone())
    .bind(scrubbed)
    .bind(state.repo_id)
    .execute(&mut *conn)
    .await
    .map_err(AppError::from)?;
    Ok(())
}

async fn write_org_binding_mismatch_audit(
    conn: &mut PgConnection,
    deps: &BranchIntegrationWorkerDeps,
    state: &CycleState,
    trigger_kind: TriggerKind,
) -> Result<(), AppError> {
    warn!(
        workspace_repo = %state.key,
        "branch_integration_worker: org binding mismatch before git transport"
    );
    set_sync_paused_reason(conn, state.key, "org_binding_mismatch").await?;
    write_fetch_failed_audit(
        conn,
        deps,
        state,
        trigger_kind,
        None,
        Some("org_binding_mismatch".to_string()),
    )
    .await
}

async fn write_fetch_failed_audit(
    conn: &mut PgConnection,
    _deps: &BranchIntegrationWorkerDeps,
    state: &CycleState,
    trigger_kind: TriggerKind,
    new_mirror_tip: Option<String>,
    error_msg: Option<String>,
) -> Result<(), AppError> {
    let scrubbed = scrub_opt(error_msg);
    if extend_repeated_background_run(
        conn,
        state,
        trigger_kind,
        run_status::FETCH_FAILED,
        new_mirror_tip.as_deref(),
        scrubbed.as_deref(),
    )
    .await?
    {
        return Ok(());
    }
    sqlx::query(
        r#"
        INSERT INTO remote_sync_runs (
            org_id, workspace_id, repo_id, acting_user_id,
            kind, trigger_kind, status,
            old_workspace_tip, new_workspace_tip, new_mirror_tip, error_detail,
            started_at, completed_at
        ) VALUES (
            $1, $2, $8, $3,
            'integration', $4, 'fetch_failed',
            $5, $5, $6, $7,
            now(), now()
        )
        "#,
    )
    .bind(state.org_id)
    .bind(state.workspace_id)
    .bind(state.acting_user_id)
    .bind(trigger_kind.as_audit_str())
    .bind(state.workspace_tip.clone())
    .bind(new_mirror_tip)
    .bind(scrubbed)
    .bind(state.repo_id)
    .execute(&mut *conn)
    .await
    .map_err(AppError::from)?;
    Ok(())
}

async fn write_push_failed_audit(
    conn: &mut PgConnection,
    _deps: &BranchIntegrationWorkerDeps,
    state: &CycleState,
    trigger_kind: TriggerKind,
    new_mirror_tip: Option<&str>,
    error_msg: Option<String>,
) -> Result<(), AppError> {
    sqlx::query(
        r#"
        INSERT INTO remote_sync_runs (
            org_id, workspace_id, repo_id, acting_user_id,
            kind, trigger_kind, status,
            old_workspace_tip, new_workspace_tip, new_mirror_tip, error_detail,
            started_at, completed_at
        ) VALUES (
            $1, $2, $8, $3,
            'integration', $4, 'push_failed',
            $5, $5, $6, $7,
            now(), now()
        )
        "#,
    )
    .bind(state.org_id)
    .bind(state.workspace_id)
    .bind(state.acting_user_id)
    .bind(trigger_kind.as_audit_str())
    .bind(state.workspace_tip.clone())
    .bind(new_mirror_tip)
    .bind(error_msg)
    .bind(state.repo_id)
    .execute(&mut *conn)
    .await
    .map_err(AppError::from)?;
    Ok(())
}

async fn delete_queue_row(
    conn: &mut PgConnection,
    picked: &PickedQueueRow,
) -> Result<(), AppError> {
    sqlx::query("DELETE FROM remote_integration_queue WHERE workspace_id = $1 AND repo_id = $2
        AND locked_by = $3 AND requested_at = $4 AND advance_id IS NOT DISTINCT FROM $5 AND trigger_kind = $6 AND requested_by_user_id IS NOT DISTINCT FROM $7 AND lifecycle_generation IS NOT DISTINCT FROM $8")
        .bind(picked.key.workspace_id).bind(picked.key.repo_id).bind(picked.owner_token)
        .bind(picked.requested_at).bind(picked.advance_id).bind(picked.trigger_kind.as_audit_str())
        .bind(picked.requested_by_user_id).bind(picked.lifecycle_generation)
        .execute(conn).await?;
    Ok(())
}

/// Hand a landed reset-approval cycle to the queue's ordinary fast-forward
/// projection without dropping a newer per-repo approval signal.
///
/// The landing's remote-tracking follow-up has already attempted the same
/// advance-bound `push` handoff (the cut is marked as a remote-tracking
/// completion). The canonical enqueue here closes the worker's landing/cleanup
/// boundary even when that follow-up only reported a projection warning. Queue
/// precedence preserves a newer active approval generation; exact queue
/// ownership fences the subsequent unlock from a replacement worker.
// LEAN-TODO(P1.5): retained remote-tracking merge and handoff ordering.
async fn rearm_committed_approved_rebase_projection(
    conn: &mut PgConnection,
    picked: &PickedQueueRow,
    _org_id: Uuid,
    committed_advance_id: Uuid,
) -> Result<(), AppError> {
    crate::models::branch_integration_queue::enqueue_branch_integration_for_advance_with_trigger(
        conn,
        picked.key,
        committed_advance_id,
        crate::models::branch_integration_queue::trigger_kind::PUSH,
        picked.explicit_authority(),
    )
    .await
    .map_err(AppError::from)?;

    let updated = sqlx::query(
        r#"
        UPDATE remote_integration_queue
           SET locked_by = NULL,
               locked_until = NULL,
               last_error = NULL
         WHERE workspace_id = $1 AND repo_id = $3
           AND locked_by = $2
        "#,
    )
    .bind(picked.key.workspace_id)
    .bind(picked.owner_token)
    .bind(picked.key.repo_id)
    .execute(&mut *conn)
    .await
    .map_err(AppError::from)?;
    if updated.rows_affected() != 1 {
        return Err(AppError::from_error(G5Error::Conflict {
            reason: format!(
                "approved rebase projection queue ownership changed for repo row {}",
                picked.key
            ),
        }));
    }
    Ok(())
}

/// Fence every captured member, then publish the synchronization watermark.
fn apply_cycle_watermarks(
    repos: &ScopedRepoStorage,
    state: &CycleState,
    writes: &[PendingBaselineRefWrite],
    provider_tip: Option<&str>,
) -> Result<Vec<g5_scm::RefCommit>, AppError> {
    let mut refs = Vec::new();
    for member in &state.authority_inventory()?.repos {
        let oid = state.authority_heads.get(&member.repo_id).ok_or_else(|| {
            AppError::from_error(G5Error::Conflict {
                reason: "Captured member has no Git head".into(),
            })
        })?;
        let path = repos.repo_path(state.org_id, member.repo_id)?;
        refs.push(g5_scm::RefCommit {
            repo_path: path,
            ref_name: format!("refs/heads/{}", member.branch_name),
            expected_old: Some(oid.clone()),
            new_oid: Some(oid.clone()),
        });
    }
    for write in writes {
        let path = repos.repo_path(state.org_id, write.repo_id)?;
        refs.push(GitRepo::sync_watermark_update(
            path,
            &write.local_branch_name,
            write.expected_watermark.clone(),
            Some(write.new_baseline.clone()),
        ));
    }
    if let Some(tip) = provider_tip {
        let path = repos.path(&state.repo_key)?;
        refs.push(GitRepo::provider_observation_update(
            path,
            state.workspace_id,
            &state.remote_branch,
            tip,
        )?);
    }
    let outcomes = GitRepo::apply_ref_commits(&refs).map_err(|error| {
        AppError::from_error(G5Error::Conflict {
            reason: format!("Remote synchronization cut refused: {error:?}"),
        })
    })?;
    Ok(refs
        .into_iter()
        .zip(outcomes)
        .filter_map(|(update, outcome)| {
            (outcome == g5_scm::BranchRefUpdate::Applied).then_some(update)
        })
        .collect())
}

fn rollback_cycle_watermarks(refs: &[g5_scm::RefCommit]) {
    for update in refs.iter().rev() {
        let reverse = g5_scm::RefCommit {
            repo_path: update.repo_path.clone(),
            ref_name: update.ref_name.clone(),
            expected_old: update.new_oid.clone(),
            new_oid: update.expected_old.clone(),
        };
        if let Err(error) = GitRepo::apply_ref_commits(&[reverse]) {
            warn!(ref_name = %update.ref_name, ?error, "Synchronization rollback refused; retaining a concurrent Git write");
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn git_auth_classification_uses_terminal_transport_diagnostics() {
        for diagnostic in [
            "Git error: ls-remote connect to 'https://example.test/repo.git' failed: credentials rejected; class=Net (12); code=Auth (-16)",
            "Git error: unexpected http status code: 401",
            "Git error: unexpected http status code: 403",
            "Git error: too many redirects or authentication replays",
            "Git error: authentication required but no callback set",
            "Git error: server requires authentication that we do not support",
            "unexpected http status code: 403; class=Http (34)",
            "Git error: ls-remote connect to 'https://example.test/repo.git' failed: unexpected http status code: 401; class=Http (34)",
            "Git error: Single-ref fetch of 'main' from 'origin' failed: too many redirects or authentication replays; class=Http (34)",
        ] {
            assert!(is_git_auth_failure(diagnostic), "{diagnostic}");
        }
        for diagnostic in [
            "Git error: ls-remote connect to '/tmp/401-403-authentication.git' failed: failed to resolve path: No such file or directory; class=Os (2); code=NotFound (-3)",
            "Git error: failed to resolve path '/tmp/authentication': No such file or directory",
            "Git error: unexpected http status code: 404; class=Http (34)",
            "Git error: unexpected http status code: 503; class=Http (34)",
            "Git error: fetch 'https://example.test/401.git': unexpected http status code: 404; class=Http (34)",
            "Git error: invalid URL 'unexpected http status code: 403'; class=Http (34)",
            "Git error: invalid path 'credentials; code=Auth (-16)'",
            "Not found: remote branch 'authentication' on 'https://example.test/403.git'",
            "git fetch timed out",
        ] {
            assert!(!is_git_auth_failure(diagnostic), "{diagnostic}");
        }
    }

    #[test]
    fn git_auth_classification_preserves_transport_diagnostics() {
        use git2::{ErrorClass, ErrorCode};

        for (class, diagnostic) in [
            (ErrorClass::Http, "unexpected http status code: 401"),
            (ErrorClass::Http, "unexpected http status code: 403"),
            (
                ErrorClass::Http,
                "too many redirects or authentication replays",
            ),
            (ErrorClass::Http, "unexpected authentication failure"),
            (
                ErrorClass::Http,
                "server authentication required but no callback set",
            ),
            (
                ErrorClass::Http,
                "proxy authentication required but no callback set",
            ),
            (
                ErrorClass::Http,
                "server requires authentication that we do not support",
            ),
            (
                ErrorClass::Http,
                "proxy requires authentication that we do not support",
            ),
            (
                ErrorClass::Http,
                "failed to respond to authentication challenge",
            ),
            (
                ErrorClass::Http,
                "'Negotiate' authentication is not supported",
            ),
            (ErrorClass::Http, "'NTLM' authentication is not supported"),
            (ErrorClass::Http, "'Basic' authentication is not supported"),
            (
                ErrorClass::Ssh,
                "authentication required but no callback set",
            ),
            (
                ErrorClass::Ssh,
                "authentication callback returned unsupported credentials type",
            ),
            (
                ErrorClass::Ssh,
                "remote rejected authentication: credentials rejected",
            ),
            (ErrorClass::Net, "negotiate authentication is not supported"),
            (
                ErrorClass::Net,
                "ntlm authentication failed: invalid credentials",
            ),
        ] {
            let error = git2::Error::new(ErrorCode::GenericError, class, diagnostic);
            for message in [
                diagnostic.to_string(),
                error.to_string(),
                format!("Git error: ls-remote connect to 'https://example.test/repo.git' failed: {error}"),
                format!("Git error: Single-ref fetch of 'main' from 'origin' failed: {error}"),
                g5_contracts::G5Error::GitError {
                    message: error.message().to_owned(),
                }.to_string(),
            ] {
                assert!(is_git_auth_failure(&message), "{message}");
            }
        }

        let error = git2::Error::new(ErrorCode::Auth, ErrorClass::Net, "credentials rejected");
        assert!(is_git_auth_failure(&format!(
            "Git error: ls-remote connect to 'https://example.test/repo.git' failed: {error}"
        )));
    }

    #[test]
    fn git_auth_classification_ignores_repository_identity() {
        for diagnostic in [
            "Git error: ls-remote connect to '/tmp/401-403-authentication.git' failed: failed to resolve path: No such file or directory; class=Os (2); code=NotFound (-3)",
            "Git error: failed to resolve path '/tmp/authentication': No such file or directory",
            "Git error: unexpected http status code: 404; class=Http (34)",
            "Git error: unexpected http status code: 503; class=Http (34)",
            "Git error: ls-remote connect to 'https://example.test/401.git' failed: unexpected http status code: 404; class=Http (34)",
            "Git error: Single-ref fetch of 'authentication' from 'origin' failed: unexpected http status code: 503; class=Http (34)",
            "Git error: invalid URL 'unexpected http status code: 403'; class=Http (34)",
            "Git error: invalid path 'credentials; code=Auth (-16)'",
            "Git error: ls-remote connect to 'https://example.test/ntlm authentication failed: invalid' failed: failed to resolve address; class=Net (12)",
            "Not found: remote branch 'authentication' on 'https://example.test/403.git'",
            "git fetch timed out",
        ] {
            assert!(!is_git_auth_failure(diagnostic), "{diagnostic}");
        }
    }

    #[test]
    fn load_cycle_state_filters_only_live_workspaces() {
        let source = include_str!("worker.rs");
        let load_start = source
            .find("async fn load_cycle_state")
            .expect("load_cycle_state exists");
        let process_start = source
            .find("async fn process_queue_row")
            .expect("process_queue_row exists");
        let load_cycle_state = &source[load_start..process_start];

        assert!(include_str!("../remote_integrations.rs").contains("w.deleted_at IS NULL"));
        assert!(
            !load_cycle_state.contains("wb.deleted_at"),
            "workspace_branches has no deleted_at column; only workspaces is soft-deleted"
        );
    }

    #[test]
    fn passive_pickup_regates_on_auto_pull_before_network_io() {
        // Option 1 (G5 #1198): the worker re-reads auto_pull_enabled at pickup
        // and drops a background_inbound cycle whose child was toggled off,
        // BEFORE the branch lock / any git I/O. Guards against a future edit
        // silently dropping either half of the gate.
        let source = include_str!("worker.rs");
        // load_cycle_state must surface the current setting.
        let load_start = source
            .find("async fn load_cycle_state")
            .expect("load_cycle_state exists");
        assert!(
            source[load_start..].contains("wbr.auto_pull_enabled        AS auto_pull_enabled"),
            "load_cycle_state must SELECT the current auto_pull_enabled"
        );
        // claim_queue_row must short-circuit disabled passive rows before the
        // shared remote-pair lease is taken.
        let proc_start = source
            .find("async fn claim_queue_row")
            .expect("claim_queue_row exists");
        let proc = &source[proc_start..];
        let gate = proc
            .find("TriggerKind::BackgroundInbound")
            .expect("passive re-check present");
        let lock = proc
            .find("try_claim_remote_pair_lease")
            .expect("remote-pair lease acquisition present");
        assert!(
            gate < lock,
            "the background_inbound auto_pull re-check must run before any remote-pair lease or git I/O"
        );
        assert!(proc[gate..lock].contains("!state.auto_pull_enabled"));
    }

    #[test]
    fn parse_trigger_kind_recognizes_all_variants() {
        assert_eq!(parse_trigger_kind("commit"), Some(TriggerKind::Commit));
        assert_eq!(parse_trigger_kind("manual"), Some(TriggerKind::Manual));
        assert_eq!(
            parse_trigger_kind("workspace_open"),
            Some(TriggerKind::WorkspaceOpen)
        );
        assert_eq!(
            parse_trigger_kind("background_inbound"),
            Some(TriggerKind::BackgroundInbound)
        );
        assert_eq!(parse_trigger_kind("push"), Some(TriggerKind::Push));
        assert_eq!(
            parse_trigger_kind("approved_rebase_publish"),
            Some(TriggerKind::ApprovedRebasePublish)
        );
        assert_eq!(parse_trigger_kind("ping"), None);
        assert_eq!(parse_trigger_kind(""), None);
    }

    #[test]
    fn approved_rebase_pickup_serializes_workspace_queue_rows() {
        let source = include_str!("worker.rs");
        let start = source
            .find("async fn grab_one_queue_row")
            .expect("queue pickup function exists");
        let end = source
            .find("fn parse_trigger_kind")
            .expect("parse_trigger_kind follows queue pickup");
        let pickup = &source[start..end];

        assert!(
            pickup.contains("prior_q.trigger_kind = 'approved_rebase_publish'")
                && pickup.contains("prior_q.workspace_id = q.workspace_id")
                && pickup.contains("(current_jmr.workspace_id, current_jmr.repo_id) = (q.workspace_id, q.repo_id)")
                && pickup.contains("w.deleted_at IS NULL")
                && pickup
                    .contains("(prior_q.requested_at, prior_q.repo_id)")
                && pickup.contains("< (q.requested_at, q.repo_id)")
                && pickup.contains("FOR UPDATE OF q SKIP LOCKED"),
            "later approved publish rows for a workspace must not be pickable while an earlier row exists"
        );
    }

    #[test]
    fn queue_pickup_and_state_load_exclude_tombstoned_workspaces() {
        let source = include_str!("worker.rs");
        for helper in ["async fn grab_one_queue_row", "async fn load_cycle_state"] {
            let start = source.find(helper).expect("helper exists");
            let body = &source[start..];
            let end = body.find("\nasync fn ").unwrap_or(body.len());
            let helper_body = &body[..end];
            assert!(
                helper_body.contains("w.deleted_at IS NULL")
                    || (helper_body.contains("remote_integrations::load(")
                        && include_str!("../remote_integrations.rs")
                            .contains("w.deleted_at IS NULL")),
                "{helper} must require a live owning workspace"
            );
        }
        let state_load_start = source
            .find("async fn load_cycle_state")
            .expect("state-load helper exists");
        let state_load = &source[state_load_start..];
        let state_load_end = state_load
            .find("\n// ---------------------------------------------------------------------------\n// Per-row processing")
            .unwrap_or(state_load.len());
        assert!(
            state_load[..state_load_end].contains("org_id: key.org_id"),
            "cycle audit identity must remain on the repo-local row that authorized pickup"
        );
    }

    #[test]
    fn watermark_updates_are_fenced_and_rolled_back_with_failed_commits() {
        let production = include_str!("worker.rs")
            .split_once("#[cfg(test)]")
            .unwrap()
            .0;
        let apply = production
            .find("let refs = apply_cycle_watermarks(")
            .unwrap();
        let commit = production[apply..].find("db.commit().await").unwrap() + apply;
        assert!(apply < commit);
        assert!(production[commit..].contains("rollback_cycle_watermarks(&refs)"));
        assert!(production.contains("GitRepo::apply_ref_commits(&refs)"));
        assert!(!production.contains("write_baseline_ref_best_effort"));
    }

    #[test]
    fn remote_git_preparation_cannot_hold_a_database_connection() {
        let source = include_str!("worker.rs");
        let production = &source[..source
            .find("#[cfg(test)]")
            .expect("test module follows production worker code")];
        for helper in [
            "async fn prepare_claimed_cycle(",
            "async fn push_workspace_tip(",
            "async fn prepare_approved_rebase_publish(",
        ] {
            let start = production.find(helper).expect("remote helper exists");
            let signature = &production[start
                ..production[start..]
                    .find(") ->")
                    .map(|offset| start + offset)
                    .expect("remote helper has a return type")];
            assert!(
                !signature.contains("PgConnection") && !signature.contains("ScopedDb"),
                "{helper} must not carry a database connection over remote work"
            );
        }
        assert!(
            !production.contains("pg_try_advisory_xact_lock"),
            "remote integration must use the durable pair lease, not a transaction-scoped lock"
        );
    }

    #[test]
    fn pair_lease_is_generation_fenced_at_claim_renew_finalize_and_release() {
        let source = include_str!("worker.rs");
        for helper in [
            "async fn try_claim_remote_pair_lease(",
            "async fn renew_cycle_leases(",
            "async fn validate_cycle_claim_for_finalization(",
            "async fn release_remote_pair_lease(",
        ] {
            let start = source.find(helper).expect("pair-lease helper exists");
            let body = &source[start..];
            let end = body.find("\nasync fn ").unwrap_or(body.len());
            let helper_body = &body[..end];
            assert!(
                helper_body.contains("owner_token")
                    && helper_body.contains("lease_generation")
                    && helper_body.contains("lease_expires_at"),
                "{helper} must validate owner, generation, and expiry"
            );
        }
    }

    #[test]
    fn finalization_takes_workspace_fence_before_cycle_row_locks() {
        let source = include_str!("worker.rs");
        let start = source
            .find("async fn validate_cycle_claim_for_finalization(")
            .expect("cycle finalization validator exists");
        let body = &source[start..];
        let end = body
            .find("\nasync fn ")
            .expect("another helper follows cycle finalization validation");
        let helper = &body[..end];
        let fence = helper
            .find("acquire_workspace_commit_fence(")
            .expect("validator acquires the shared workspace commit fence");
        let lease_rows = helper
            .find("FOR UPDATE OF q, p")
            .expect("validator locks queue and pair lease rows");
        let branch_rows = helper
            .find("remote_integrations::load(conn, picked.key, true)")
            .expect("validator locks parent and child branch rows");

        assert!(
            fence < lease_rows && lease_rows < branch_rows,
            "workspace fence must precede lease and branch row locks"
        );
    }

    #[test]
    fn authority_cycles_commit_registration_before_fresh_landing() {
        let source = include_str!("worker.rs");
        let start = source
            .find("CycleClaimValidation::Valid =>")
            .expect("valid finalization arm exists");
        let arm = &source[start
            ..source[start..]
                .find("Ok(ProcessOutcome::Completed(Box::new(completed)))")
                .map(|offset| start + offset)
                .expect("completed outcome closes valid arm")];
        let registration = arm
            .find("apply_prepared_cycle(")
            .expect("cycle registers its consumers");
        let registration_commit = arm
            .find("db.commit()")
            .expect("registration transaction commits");
        let fresh_application = arm
            .find("let application_db =")
            .expect("fresh application transaction opens");
        let landing = arm.find(".land(").expect("branch commit lands");
        let lease_cleanup = arm
            .find("release_remote_pair_lease(cleanup_db.conn()")
            .expect("lease cleanup follows committed authority");

        assert!(
            registration < registration_commit
                && registration_commit < fresh_application
                && fresh_application < landing
                && landing < lease_cleanup,
            "authority cycles must commit consumer registration, land from an untouched transaction, and clean up coordination only after the landing commits"
        );
        assert!(
            !arm.contains("release_remote_pair_lease(db.conn(), &lease).await? {\n                            delete_queue_row"),
            "authority registration must not release coordination or delete its queue before landing"
        );
    }

    #[test]
    fn approved_reset_uses_exact_cut_landing_and_push_handoff() {
        let source = include_str!("worker.rs");
        let production = source
            .split_once("\n#[cfg(test)]")
            .map(|(production, _)| production)
            .expect("branch integration test module follows production code");
        let prepare_start = source
            .find("async fn prepare_approved_rebase_publish(")
            .expect("approved reset preparation exists");
        let apply_start = source
            .find("async fn apply_approved_rebase_publish(")
            .expect("approved reset application exists");
        let prepare = &source[prepare_start..apply_start];
        let apply_end = source[apply_start..]
            .find("\nasync fn refresh_advisory(")
            .map(|offset| apply_start + offset)
            .expect("advisory helper follows approved reset application");
        let apply = &source[apply_start..apply_end];

        assert!(
            prepare.contains("proof.expected_authority_tip != state.workspace_tip")
                && prepare.contains("normalize_reviewed_publication_tip")
                && prepare.contains("state.authority_inventory")
                && prepare.contains("prepare_branch_commit(")
                && prepare.contains(".with_remote_tracking_completion(state.push_authority())")
                && !prepare.contains("push_refspec_with_lease")
                && !prepare.contains("ls_remote_ref"),
            "approved reset must bind A/L/R, normalize without ref movement, and prepare the captured complete cut as a remote-tracking completion"
        );
        assert!(
            apply.contains("register_remote_tracking_merge_completion_consumer")
                && apply.contains("authority_audit: Some(RemoteTrackingAuthorityAuditInput")
                && apply.split_whitespace().collect::<String>().contains("PreparedRemotePublication::new(prepared.authority_advance,)")
                && !apply.contains("stage_remote_tracking_merge_completion_for_advance")
                && !apply.contains("write_approved_rebase_authority_audit")
                && !apply.contains("delete_queue_row")
                && !apply.contains("record_push_converged")
                && !apply.contains("tracking_ref_writes.push"),
            "approved reset application must register lifecycle and audit reconciliation, then leave publication and the push handoff to the branch commit landing"
        );
        assert!(
            !production.contains("push_workspace_tip_with_lease")
                && !production.contains("push_refspec_with_lease"),
            "branch integration must not retain a force-with-lease authority publication path"
        );

        let handoff_start = source
            .find("async fn rearm_committed_approved_rebase_projection(")
            .expect("approved reset post-landing handoff exists");
        let handoff = &source[handoff_start..];
        let handoff_end = handoff.find("\n/// ").unwrap_or(handoff.len());
        let handoff = &handoff[..handoff_end];
        assert!(
            handoff.contains("enqueue_branch_integration_for_advance_with_trigger")
                && handoff.contains("trigger_kind::PUSH")
                && handoff.contains("locked_by = NULL")
                && handoff.contains("locked_by = $2")
                && !handoff.contains("SET trigger_kind = 'push'"),
            "a committed approval must apply canonical queue precedence before unlocking the claimed row"
        );
    }

    #[test]
    fn policy_updates_do_not_duplicate_git_authority() {
        let production = include_str!("worker.rs")
            .split_once("#[cfg(test)]")
            .unwrap()
            .0;
        assert!(!production.contains("SET head_commit"));
        assert!(!production.contains("SET baseline_commit"));
        assert!(!production.contains("SET remote_tip_advisory_sha"));
        assert!(production.contains("UPDATE remote_integrations SET last_sync_at"));
    }

    #[test]
    fn approved_rebase_failure_blocks_workspace_publish_set() {
        let source = include_str!("worker.rs");
        let start = source
            .find("async fn block_workspace_approved_rebase_publish_set")
            .expect("approved publish set failure helper exists");
        let end = source
            .find("async fn complete_approved_rebase_merge_run")
            .expect("completion helper follows set failure helper");
        let helper = &source[start..end];

        assert!(
            helper.contains("UPDATE workspace_merge_runs jmr")
                && helper.contains("DELETE FROM remote_integration_queue q")
                && helper.contains("q.trigger_kind = 'approved_rebase_publish'")
                && helper.contains("workspace_id = $1"),
            "a failed approved publish repo must block sibling merge runs and remove sibling queue rows"
        );
    }
}
