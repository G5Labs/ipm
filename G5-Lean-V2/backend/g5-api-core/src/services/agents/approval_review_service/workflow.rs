//! Recovery of the approval-owned operation. This reconciles existing fork,
//! task and VM owners; it neither schedules provider retries nor owns their state.
use super::store::{CandidateEvidence, FormalReviewStore, Review};
use crate::db::scoped::scoped_db_for_system_org;
use crate::services::config_service::legacy_agent_launch_preferences;
use crate::services::workspace_messages::{MessageTarget, WorkspaceMessage};
use crate::services::workspace_vm_service::AgentReviewStart;
use crate::{
    errors::AppError,
    services::{AppState, WorkspaceVmService, WorkspaceVmServiceConfig},
};
use base64::Engine;
use chrono::{DateTime, Utc};
use futures_util::{stream, StreamExt};
use g5_authz::ScopedDb;
use g5_contracts::{
    types::{
        validate_agent_approval_markdown_v2, AgentApprovalOutcome, ApprovalReviewIdentity,
        ApprovalReviewResolutionV2, ChatAgentProvider, FormalApprovalInvocation, MergeSourceHead,
    },
    G5Error,
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{collections::HashMap, sync::Arc, time::Duration};
use uuid::Uuid;

enum ReconcileWork {
    Due(Result<Vec<(Uuid, Uuid)>, AppError>),
    Review(Uuid, Result<bool, AppError>),
}

pub fn spawn(
    state: Arc<AppState>,
    cancel: tokio_util::sync::CancellationToken,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut ticks = tokio::time::interval(Duration::from_secs(2));
        ticks.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        let slots = Arc::new(tokio::sync::Semaphore::new(owner_concurrency(&state)));
        let mut work = tokio::task::JoinSet::new();
        let mut discovery: Option<tokio::task::AbortHandle> = None;
        let mut in_flight: HashMap<Uuid, tokio::task::AbortHandle> = HashMap::new();
        loop {
            tokio::select! {
                _ = cancel.cancelled() => break,
                _ = ticks.tick(), if discovery.is_none() => {
                    // Even discovery may wait for a pool connection. Keep it
                    // outside the tick so completions and shutdown still run.
                    let state = state.clone();
                    discovery = Some(work.spawn(async move {
                        ReconcileWork::Due(due_reviews(&state).await)
                    }));
                }
                Some(completed) = work.join_next(), if !work.is_empty() => {
                    match completed {
                        Ok(ReconcileWork::Due(result)) => {
                            discovery = None;
                            match result {
                                Ok(pending) => for (org, id) in pending {
                                    if in_flight.contains_key(&id) {
                                        continue;
                                    }
                                    let state = state.clone();
                                    let slots = slots.clone();
                                    let handle = work.spawn(async move {
                                        let _permit = slots.acquire_owned().await
                                            .expect("review owner semaphore remains open");
                                        ReconcileWork::Review(id, reconcile_review(&state, org, id).await)
                                    });
                                    in_flight.insert(id, handle);
                                },
                                Err(error) => tracing::warn!(%error, "formal approval discovery deferred"),
                            }
                        }
                        Ok(ReconcileWork::Review(id, result)) => {
                            in_flight.remove(&id);
                            if let Err(error) = result {
                                tracing::warn!(review_id=%id, %error, "formal approval reconciliation deferred");
                            }
                        }
                        Err(error) => {
                            // A panicked task must not reserve its review for
                            // the rest of this process's lifetime.
                            if discovery.as_ref().is_some_and(|task| task.id() == error.id()) {
                                discovery = None;
                            }
                            in_flight.retain(|_, task| task.id() != error.id());
                            tracing::warn!(%error, "formal approval worker stopped; durable work remains recoverable");
                        }
                    }
                }
            }
        }
        work.shutdown().await;
    })
}

async fn due_reviews(state: &Arc<AppState>) -> Result<Vec<(Uuid, Uuid)>, AppError> {
    let mut pending = Vec::new();
    for org in crate::maintenance::maintenance_org_ids(&state.system_pool).await? {
        let mut db = scoped_db_for_system_org(state, org, &["*"]).await?;
        let ids = FormalReviewStore::due(&mut db, org).await?;
        db.rollback().await?;
        pending.extend(ids.into_iter().map(|id| (org, id)));
    }
    Ok(pending)
}

fn owner_concurrency(state: &AppState) -> usize {
    // Each owner operation may need a connection in addition to its fence.
    ((state
        .tenant_pool
        .options()
        .get_max_connections()
        .saturating_sub(1))
        / 2)
    .clamp(1, 4) as usize
}

async fn reconcile_review(state: &Arc<AppState>, org: Uuid, id: Uuid) -> Result<bool, AppError> {
    let mut claim = scoped_db_for_system_org(state, org, &["*"]).await?;
    let locked = FormalReviewStore::try_lock_owner(&mut claim, id).await?;
    if !locked {
        return Ok(false);
    }
    let outcome = drive(state, org, id).await;
    claim.rollback().await?;
    match outcome {
        Ok(()) => Ok(true),
        Err(error) => {
            tracing::warn!(review_id=%id, %error, "formal review remains recoverable");
            Ok(false)
        }
    }
}

/// Drive one discovered pass to completion for explicit callers and tests.
/// The production loop tracks work independently instead of awaiting this batch.
pub async fn run_once(state: &Arc<AppState>) -> Result<u64, AppError> {
    let pending = due_reviews(state).await?;
    let mut work = stream::iter(pending)
        .map(|(org, id)| reconcile_review(state, org, id))
        .buffer_unordered(owner_concurrency(state));
    let mut count = 0;
    while let Some(result) = work.next().await {
        if result? {
            count += 1;
        }
    }
    Ok(count)
}

async fn load(state: &Arc<AppState>, org: Uuid, id: Uuid) -> Result<Review, AppError> {
    let mut db = scoped_db_for_system_org(state, org, &["*"]).await?;
    let row = FormalReviewStore::load(&mut db, org, id).await?;
    db.rollback().await?;
    Ok(row)
}

async fn drive(state: &Arc<AppState>, org: Uuid, id: Uuid) -> Result<(), AppError> {
    let mut review = load(state, org, id).await?;
    if ["effective", "failed", "cancelled"].contains(&review.stage.as_str()) {
        return cleanup(state, &review).await;
    }
    let mut db = scoped_db_for_system_org(state, org, &["*"]).await?;
    let request = FormalReviewStore::request_state(&mut db, &review, false).await?;
    db.rollback().await?;
    if request != "pending" {
        return terminalize(state, &review, "cancelled", None).await;
    }
    if review.stage == "capture" && review.claimed_at.is_none() {
        let mut db = scoped_db_for_system_org(state, org, &["*"]).await?;
        let claimed = FormalReviewStore::first_claim(&mut db, &review).await?;
        let Some((claimed_at, deadline_at)) = claimed else {
            db.rollback().await?;
            return Ok(());
        };
        commit_review(db).await?;
        review.claimed_at = Some(claimed_at);
        review.deadline_at = deadline_at;
    }
    // Read acceptance before checking the readiness deadline: acceptance is a
    // durable task fact and advances the budget even after a reconciler crash.
    if review.stage == "start" && review.task_id.is_some() {
        let mut db = scoped_db_for_system_org(state, org, &["*"]).await?;
        let accepted = FormalReviewStore::input_accepted_at(&mut db, review.task_id).await?;
        if let Some(at) = accepted {
            FormalReviewStore::advance(
                &mut db,
                &review,
                "execute",
                at + chrono::Duration::minutes(45),
            )
            .await?;
            commit_review(db).await?;
            return Ok(());
        }
        db.rollback().await?;
    }
    if Utc::now() >= review.deadline_at {
        let cause = deadline_cause(state, &review).await;
        return terminalize(state, &review, "failed", Some(cause)).await;
    }
    let budget = review
        .deadline_at
        .signed_duration_since(Utc::now())
        .to_std()
        .unwrap_or_default();
    let result = tokio::time::timeout(budget, step(state, &review)).await;
    match result {
        Ok(Ok(())) => Ok(()),
        Ok(Err(error)) => {
            if matches!(&error.inner,G5Error::Internal{message} if message=="review request was cancelled")
            {
                return terminalize(state, &review, "cancelled", None).await;
            }
            // A lost COMMIT acknowledgement can have advanced the durable
            // stage. Observe that fact before deciding to retry or fail.
            let current = load(state, org, id).await?;
            if progressed(&review, &current) {
                return Ok(());
            }
            if Utc::now() >= review.deadline_at {
                let cause = deadline_cause(state, &review).await;
                return terminalize(state, &review, "failed", Some(cause)).await;
            }
            // Definite operation/validation errors fail visibly; only safe
            // typed transport, database retry or stopping errors may retry within this same stage budget.
            let transient = matches!(
                &error.inner,
                G5Error::ServiceUnavailable { .. }
                    | G5Error::DatabaseRetryable { .. }
                    | G5Error::WorkspaceStopping
            );
            if transient {
                let mut db = scoped_db_for_system_org(state, org, &["*"]).await?;
                let attempts = FormalReviewStore::record_transient(&mut db, &review).await?;
                commit_review(db).await?;
                if attempts < 3 {
                    return Ok(());
                }
            }
            terminalize(state, &review, "failed", Some(&error.inner.to_string())).await
        }
        Err(_) => {
            let current = load(state, org, id).await?;
            if progressed(&review, &current) {
                return Ok(());
            }
            let cause = deadline_cause(state, &review).await;
            terminalize(state, &review, "failed", Some(cause)).await
        }
    }
}

/// The cause a passed stage deadline records. Before the review is sent,
/// readiness names what its workspace was still waiting for, such as VM
/// capacity; a probe that cannot answer quickly never delays the failure.
async fn deadline_cause(state: &Arc<AppState>, review: &Review) -> &'static str {
    const EXCEEDED: &str = "stage deadline exceeded";
    if review.stage != "start" || review.task_id.is_some() {
        return EXCEEDED;
    }
    let probe_deadline = Utc::now() + chrono::Duration::seconds(10);
    let probe = async {
        let db = operation_scope(state, review, probe_deadline).await?;
        let vm = vm_service(state, &db, probe_deadline)?;
        db.rollback().await?;
        Ok::<_, AppError>(vm.agent_review_deadline_cause(review.id).await?)
    };
    match tokio::time::timeout(Duration::from_secs(10), probe).await {
        Ok(Ok(Some(cause))) => cause,
        _ => EXCEEDED,
    }
}

fn progressed(prior: &Review, current: &Review) -> bool {
    current.stage != prior.stage
        || current.task_id != prior.task_id
        || current.target_heads != prior.target_heads
        || current.invocation != prior.invocation
        || current.artifact_id != prior.artifact_id
}

fn vm_service(
    state: &Arc<AppState>,
    db: &ScopedDb,
    deadline: DateTime<Utc>,
) -> Result<WorkspaceVmService, AppError> {
    Ok(WorkspaceVmService::new(WorkspaceVmServiceConfig {
        system_pool: state.system_pool.clone(),
        scope: db.reopener(),
        hosts: state.preview_tunnels.clone(),
        minter: state
            .jwt_minter
            .clone()
            .ok_or_else(|| failure("workspace signing unavailable"))?,
        runtime: state.workspace_runtime.clone(),
        agent_work: state.agent_work.clone(),
        repo_storage: state.repo_storage.clone(),
        git_external_url: state.git_external_url.clone(),
        ws_hub: state.event_bus.ws_hub().clone(),
    })
    .with_operation_deadline(deadline))
}

/// Cancellation of a future does not synchronously cancel a SQLx query on
/// PostgreSQL. Bound the server wait too, so its transaction releases locks
/// before terminalization needs the same request row.
async fn operation_scope(
    state: &Arc<AppState>,
    review: &Review,
    deadline: DateTime<Utc>,
) -> Result<ScopedDb, AppError> {
    let mut db = scoped_db_for_system_org(state, review.org_id, &["*"]).await?;
    crate::db::operation_deadline::apply(&mut db, Some(deadline)).await?;
    Ok(db)
}
async fn stage_scope(state: &Arc<AppState>, review: &Review) -> Result<ScopedDb, AppError> {
    operation_scope(state, review, review.deadline_at).await
}

async fn step(state: &Arc<AppState>, review: &Review) -> Result<(), AppError> {
    match review.stage.as_str() {
        "capture" => {
            if review.target_heads.is_none() {
                let mut db = stage_scope(state, review).await?;
                crate::db::scoped::ensure_user_workspace_access_until_commit(
                    &state.system_pool,
                    &mut db,
                    review.requested_by,
                    review.org_id,
                    review.source_workspace_id,
                )
                .await?;
                crate::db::scoped::ensure_user_workspace_access_until_commit(
                    &state.system_pool,
                    &mut db,
                    review.requested_by,
                    review.org_id,
                    review.target_workspace_id,
                )
                .await?;
                let heads = state
                    .approval_config_service
                    .capture_formal_review_heads(&mut db, review.target_workspace_id)
                    .await?;
                FormalReviewStore::record_target(&mut db, review, json!(heads)).await?;
                commit_review(db).await?;
            }
            let db = stage_scope(state, review).await?;
            let fork = state
                .workspace_fork_service
                .scoped(db.reopener())
                .with_operation_deadline(review.deadline_at);
            db.rollback().await?;
            fork.fork_for_approval(review.id).await?;
            let (source_heads, repository_names) = fork.approval_source_heads(review.id).await?;
            let refreshed = load(state, review.org_id, review.id).await?;
            let target_heads: Vec<MergeSourceHead> = serde_json::from_value(
                refreshed
                    .target_heads
                    .ok_or_else(|| failure("target cut absent"))?,
            )?;
            if source_heads
                .iter()
                .map(|h| &h.repo_id)
                .collect::<std::collections::BTreeSet<_>>()
                != target_heads
                    .iter()
                    .map(|h| &h.repo_id)
                    .collect::<std::collections::BTreeSet<_>>()
            {
                return Err(failure("source and target repository vectors differ"));
            }
            let mut db = stage_scope(state, review).await?;
            let (definition, version) =
                FormalReviewStore::definition(&mut db, review.approval_request_id).await?;
            let output_repo = Uuid::parse_str(
                &source_heads
                    .first()
                    .ok_or_else(|| failure("source cut empty"))?
                    .repo_id,
            )
            .map_err(|_| failure("invalid source repository"))?;
            let workspace_base_heads = if review.invocation.is_none() {
                Some(
                    crate::services::git::workspace_revision_history::saved_base(
                        db.conn(),
                        review.source_workspace_id,
                    )
                    .await?
                    .ok_or_else(|| failure("source workspace has no saved fork base"))?,
                )
            } else {
                None
            };
            let repos = state.repo_storage.scoped(&db);
            db.rollback().await?;
            let mut resolution = ApprovalReviewResolutionV2 {
                assigned_at: None,
                merge_base: None,
                schema_version:
                    g5_contracts::types::APPROVAL_REVIEW_RESOLUTION_SCHEMA_VERSION_CURRENT,
                workspace_base_heads,
                repository_names,
                resolution_id: ApprovalReviewIdentity::new(review.id).resolution_id(),
                source_workspace_id: review.source_workspace_id,
                source_heads,
                target_workspace_id: review.target_workspace_id,
                target_heads,
                definition_id: definition,
                definition_version: version,
                output_doc_id: ApprovalReviewIdentity::new(review.id).output_doc_id(),
                output_repo_id: output_repo,
                output_doc_path: ApprovalReviewIdentity::new(review.id)
                    .output_doc_path(review.source_workspace_id),
            };
            if review.invocation.is_none() {
                let mut merge_bases = Vec::new();
                for source in &resolution.source_heads {
                    let parent = resolution
                        .target_heads
                        .iter()
                        .find(|h| h.repo_id == source.repo_id)
                        .ok_or_else(|| failure("Parent repository missing"))?;
                    let repo_id = Uuid::parse_str(&source.repo_id)
                        .map_err(|_| failure("invalid source repository"))?;
                    let path = repos
                        .repo_path(review.org_id, repo_id)
                        .map_err(|_| failure("Repository unavailable"))?;
                    let source_sha = source.head_commit.clone();
                    let parent_sha = parent.head_commit.clone();
                    let workspace_base = resolution
                        .workspace_base_heads
                        .as_ref()
                        .and_then(|heads| heads.iter().find(|h| h.repo_id == source.repo_id))
                        .map(|h| h.head_commit.clone());
                    let comparison = tokio::task::spawn_blocking(move || {
                        g5_scm::GitRepo::open(&path)?.pushed_comparison(
                            &source_sha,
                            &parent_sha,
                            workspace_base.as_deref(),
                            250,
                        )
                    })
                    .await
                    .map_err(failure)??;
                    merge_bases.push(g5_contracts::types::ApprovalMergeBaseRepository {
                        repo_id: source.repo_id.clone(),
                        repo_name: resolution.repository_names[&source.repo_id].clone(),
                        git_sha: comparison.merge_base,
                    });
                }
                resolution.merge_base = Some(g5_contracts::types::ApprovalMergeBaseCut {
                    repositories: merge_bases,
                });
                // Placeholder; record_assignment replaces it with the database
                // clock when it stores the immutable invocation.
                resolution.assigned_at =
                    Some(Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Micros, true));
            }
            let invocation = if let Some(frozen) = &review.invocation {
                let frozen: FormalApprovalInvocation = serde_json::from_value(frozen.clone())?;
                if frozen.resolution.source_heads != resolution.source_heads
                    || frozen.resolution.target_heads != resolution.target_heads
                {
                    return Err(failure("retry fork differs from frozen assignment"));
                }
                frozen
            } else {
                FormalApprovalInvocation::render(
                    resolution,
                    review.definition_snapshot["name"]
                        .as_str()
                        .unwrap_or("Approval"),
                    review.definition_snapshot["agent_prompt"]
                        .as_str()
                        .ok_or_else(|| failure("pinned policy absent"))?,
                )
                .map_err(failure)?
            };
            fork.pin_approval_evidence(review.id, &invocation.resolution)
                .await?;
            let mut db = stage_scope(state, review).await?;
            FormalReviewStore::record_assignment(&mut db, review, &invocation).await?;
            FormalReviewStore::advance(
                &mut db,
                review,
                "start",
                Utc::now() + chrono::Duration::minutes(15),
            )
            .await?;
            commit_review(db).await?;
        }
        "start" => {
            if review.task_id.is_some() {
                let mut db = stage_scope(state, review).await?;
                let status = FormalReviewStore::task_status(&mut db, review).await?;
                db.rollback().await?;
                if ["failed", "cancelled"].contains(&status.as_str()) {
                    return Err(task_failure(state, review).await?);
                }
                return Ok(());
            }
            let db = stage_scope(state, review).await?;
            let vm = vm_service(state, &db, review.deadline_at)?;
            db.rollback().await?;
            if let AgentReviewStart::Ended(cause) = vm.resume_agent_review(review.id).await? {
                // A definite end of this attempt's consent: fail with its cause
                // rather than the replay's refusal, leaving Retry available.
                return terminalize(state, review, "failed", Some(cause)).await;
            }
            if vm.view(review.review_workspace_id).await?.status != "running" {
                return Ok(());
            }
            let invocation: FormalApprovalInvocation = serde_json::from_value(
                review
                    .invocation
                    .clone()
                    .ok_or_else(|| failure("assignment absent"))?,
            )?;
            let provider: ChatAgentProvider = review.definition_snapshot["agent_harness"]
                .as_str()
                .ok_or_else(|| failure("pinned harness absent"))?
                .parse()
                .map_err(|_| failure("unsupported pinned harness"))?;
            // Formal definitions omit Mode: native policy is fixed separately.
            // Supply a fixed admission selector before shared validation;
            // ordinary modes cannot override it and account authority stays shared.
            let permission_mode = match provider {
                ChatAgentProvider::ClaudeCode => "default",
                ChatAgentProvider::Codex => "never/read-only",
            };
            let mut db = stage_scope(state, review).await?;
            let sent = state
                .workspace_messenger
                .send(
                    &mut db,
                    WorkspaceMessage {
                        org_id: review.org_id,
                        workspace_id: review.review_workspace_id,
                        actor_user_id: review.requested_by,
                        target: MessageTarget::NewSession {
                            title: Some("Formal approval review".into()),
                            scope_type: Some("workspace".into()),
                            scope_ref: None,
                        },
                        content: invocation.user_prompt.clone(),
                        agent_launch: None,
                        legacy_preferences: legacy_agent_launch_preferences(
                            Some(provider),
                            review.definition_snapshot["agent_model"].as_str(),
                            Some(permission_mode),
                            review.definition_snapshot["agent_effort"].as_str(),
                        ),
                        attachment_digest: None,
                        request_key: Some(ApprovalReviewIdentity::new(review.id).operation_key()),
                        workbench: None,
                    },
                )
                .await?;
            state
                .task_service
                .bind_formal_approval(&mut db, review.id, sent.queued.task_id, &invocation)
                .await?;
            FormalReviewStore::bind_task(&mut db, review, sent.queued.task_id).await?;
            commit_review(db).await?;
        }
        "execute" => {
            let mut db = stage_scope(state, review).await?;
            let status = FormalReviewStore::task_status(&mut db, review).await?;
            if status == "succeeded" {
                FormalReviewStore::advance(
                    &mut db,
                    review,
                    "harvest",
                    Utc::now() + chrono::Duration::minutes(2),
                )
                .await?;
            } else if ["failed", "cancelled", "waiting"].contains(&status.as_str()) {
                return Err(task_failure(state, review).await?);
            }
            commit_review(db).await?;
        }
        "harvest" => {
            let invocation: FormalApprovalInvocation = serde_json::from_value(
                review
                    .invocation
                    .clone()
                    .ok_or_else(|| failure("assignment absent"))?,
            )?;
            let mut db = stage_scope(state, review).await?;
            let (outcome, execution) = state
                .task_service
                .formal_approval_receipt(
                    &mut db,
                    review.org_id,
                    review
                        .task_id
                        .ok_or_else(|| failure("review task absent"))?,
                )
                .await?;
            let artifact = &outcome["result"]["approval_artifact"];
            if artifact["resolutionId"] != invocation.resolution.resolution_id
                || artifact["contractVersion"] != invocation.contract_version
            {
                return Err(failure("output provenance mismatch"));
            }
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(
                    artifact["contentBase64"]
                        .as_str()
                        .ok_or_else(|| failure("fresh output missing"))?,
                )
                .map_err(failure)?;
            let parsed = validate_agent_approval_markdown_v2(&bytes, &invocation.resolution)
                .map_err(failure)?;
            let digest = format!("{:x}", Sha256::digest(&bytes));
            if artifact["sha256"] != digest {
                return Err(failure("output digest mismatch"));
            }
            let document = std::str::from_utf8(&bytes).map_err(failure)?;
            let result = match parsed.outcome {
                AgentApprovalOutcome::Approved => "approved",
                AgentApprovalOutcome::Rejected => "rejected",
                AgentApprovalOutcome::Deferred => {
                    return Err(failure("operational outcomes are not agent verdicts"))
                }
            };
            let artifact_id = ApprovalReviewIdentity::new(review.id).artifact_id();
            FormalReviewStore::record_candidate(
                &mut db,
                review,
                CandidateEvidence {
                    id: artifact_id,
                    result,
                    document,
                    digest: &digest,
                    execution,
                    summary: &parsed.summary,
                },
            )
            .await?;
            FormalReviewStore::advance(
                &mut db,
                review,
                "seal",
                Utc::now() + chrono::Duration::minutes(2),
            )
            .await?;
            commit_review(db).await?;
        }
        "seal" => {
            let mut db = stage_scope(state, review).await?;
            let request = FormalReviewStore::request_state(&mut db, review, true).await?;
            if request != "pending" {
                return Err(failure("review request was cancelled"));
            }
            state
                .workspace_service
                .seal_agent_review(&mut db, review.id)
                .await?;
            let invocation: FormalApprovalInvocation = serde_json::from_value(
                review
                    .invocation
                    .clone()
                    .ok_or_else(|| failure("assignment absent"))?,
            )?;
            FormalReviewStore::settle(&mut db, review, &invocation).await?;
            commit_review(db).await?;
        }
        _ => return Err(failure("unknown review stage")),
    }
    Ok(())
}

async fn terminalize(
    state: &Arc<AppState>,
    review: &Review,
    stage: &str,
    cause: Option<&str>,
) -> Result<(), AppError> {
    let mut db = scoped_db_for_system_org(state, review.org_id, &["*"]).await?;
    FormalReviewStore::terminalize(&mut db, review, stage, cause).await?;
    // Failure ends this attempt, not the person's still-unaddressed request.
    // Human cancellation has already closed the request under the same row lock.
    // Cleanup is separate due work so disposal cannot hold this operation open.
    commit_review(db).await
}

async fn cleanup(state: &Arc<AppState>, review: &Review) -> Result<(), AppError> {
    if review.cleanup_due_at.is_some_and(|due| due > Utc::now()) {
        return Ok(());
    }
    let cleanup_deadline = Utc::now() + chrono::Duration::minutes(5);
    let attempt = async {
        let mut db = operation_scope(state, review, cleanup_deadline).await?;
        let has_vm = FormalReviewStore::has_vm(&mut db, review).await?;
        let vm = if has_vm {
            Some(vm_service(state, &db, cleanup_deadline)?)
        } else {
            None
        };
        let fork = state
            .workspace_fork_service
            .scoped(db.reopener())
            .with_operation_deadline(cleanup_deadline);
        db.rollback().await?;
        if let Some(vm) = vm {
            vm.stop_agent_review(review.id).await?;
        }
        let mut db = operation_scope(state, review, cleanup_deadline).await?;
        state
            .workspace_service
            .delete_agent_review(&mut db, review.id)
            .await?;
        commit_review(db).await?;
        fork.retire_approval_fork(review.id).await?;
        Ok::<(), AppError>(())
    };
    let result = tokio::time::timeout(Duration::from_secs(5 * 60), attempt)
        .await
        .map_err(|_| failure("cleanup attempt deadline exceeded"))
        .and_then(|result| result);
    let mut db = scoped_db_for_system_org(state, review.org_id, &["*"]).await?;
    let error = result.err().map(|error| error.inner.to_string());
    FormalReviewStore::record_cleanup(&mut db, review, error.as_deref()).await?;
    commit_review(db).await?;
    Ok(())
}
pub(super) fn failure(message: impl std::fmt::Display) -> AppError {
    G5Error::Internal {
        message: message.to_string(),
    }
    .into()
}

async fn task_failure(state: &Arc<AppState>, review: &Review) -> Result<AppError, AppError> {
    let mut db = scoped_db_for_system_org(state, review.org_id, &["*"]).await?;
    let outcome = FormalReviewStore::latest_task_outcome(&mut db, review).await?;
    db.rollback().await?;
    let code = outcome
        .as_ref()
        .and_then(|o| {
            o.pointer("/result/code")
                .and_then(Value::as_str)
                .or_else(|| o.get("reason").and_then(Value::as_str))
        })
        .unwrap_or("agent_dispatch_failed");
    let cause = match code {
        "approval_output_invalid" => {
            "output_validation: assigned review output is missing or invalid".into()
        }
        code if code.contains("auth") => format!("Provider authentication failed ({code})"),
        code => format!("Agent execution failed ({code})"),
    };
    Ok(failure(cause))
}

async fn commit_review(db: ScopedDb) -> Result<(), AppError> {
    crate::db::durable_commit::commit_durable_scope(db)
        .await
        .map_err(Into::into)
}
