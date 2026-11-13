//! Direct compare-and-swap landing of server-produced commits (P1.4).
//!
//! Every server-side producer of a commit (draft commit-all, fork creation,
//! the docs bootstrap, merges, retries, uploads, imports) lands it here. A cut
//! is prepared in memory from the current branch refs of the target's member
//! repositories, its changed members are applied to `refs/heads/<branch>`
//! under git's own ref locks ([`GitRepo::apply_branch_commits`]) while the
//! workspace commit fence is held, and a `workspace.branch_updated` hint is
//! published per changed member after the transaction commits. Nothing is
//! registered, staged, leased or replayed: a landing either lands or leaves
//! every ref where it was (design v6 §5.3), and projections converge from the
//! refs regardless of the hint (design v6 §9.6).
//!
//! Git acceptance under the fence is the commit point
//! (`g5.git.landing.commit-point`). Every database check that could refuse the
//! landing runs before the refs move, and a cut Git fails partway through is
//! restored before the fence is released. Once the fence is released the refs
//! are never retracted, so a fenced reader only ever sees a complete cut. If
//! COMMIT then fails, the refs stay ahead of the database, as they do after a
//! cancelled request. A draft commit's retry converges: its cut takes a member
//! whose ref already carries a commit with the tree the cut would write as
//! already landed (`find_already_landed_heads`).
//!
//! The "prepare, then land" split is only a validation boundary: preparation
//! resolves the target inventory and checks the producer's expectations
//! against the refs so the expensive part of a doomed request fails early;
//! landing repeats the check under the workspace fence and lets git decide.

use std::collections::{BTreeMap, BTreeSet};
use std::future::Future;
use std::pin::Pin;
use std::time::Duration;

use crate::services::git::repo_store::{OrgScope, ScopedRepoStorage, TenantRepoStorage};
use g5_authz::{ScopedDb, ScopedDbReopener};
use g5_contracts::error::{AuthorityCutMember, AuthorityCutMismatch, AuthorityCutStaleDetails};
use g5_contracts::types::{AuthorityTarget, WorkspaceHeadRepoInput, WorkspaceHeadTransitionKind};
use g5_scm::{BranchRefCutRefusal, GitRepo, RefCommit};
use sqlx::PgConnection;
use uuid::Uuid;

use crate::errors::{AppError, G5Error};
use crate::services::workspace_ops::enqueue_remote_tracking_integration_for_advance;
use crate::services::workspace_reader::{
    read_branch_head_oids, resolve_authority_inventory, resolve_authority_inventory_for_update,
    AuthorityInventorySnapshot, CommittedRepoReadView,
};
use crate::services::EventBus;
use crate::ws::events::BranchUpdateHint;
use crate::ws::{Channel, WsEvent};

pub const ZERO_GIT_SHA: &str = "0000000000000000000000000000000000000000";
const INTERACTIVE_FENCE_ACQUIRE_BOUND: Duration = Duration::from_secs(5);
const INTERACTIVE_FENCE_RETRY_INTERVAL: Duration = Duration::from_millis(250);
const AUTHORITY_CUT_STALE_PROTOCOL_PREFIX: &str = "AUTHORITY_CUT_STALE:";
/// Git refused to update a branch ref for a reason other than a stale
/// expectation (an invalid or missing commit object, a ref namespace
/// conflict, a lock failure). The landing is refused like a stale cut, but the
/// error is a git error, not a repairable conflict.
const BRANCH_REF_REFUSED_PROTOCOL_PREFIX: &str = "BRANCH_REF_REFUSED:";
/// Members of the cut that are refused because the branch ref carries another
/// head than the producer expected report this phrase; a few producers
/// classify such a refusal as a lost head race rather than an error.
pub const REGISTERED_TRANSITION_MISMATCH: &str =
    "does not match its registered authority transition";

/// One repository member of a cut being landed.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BranchCommitMember {
    pub repo_id: Uuid,
    pub workspace_id: Uuid,
    pub branch_name: String,
    /// The head `refs/heads/<branch_name>` carried when the cut was prepared
    /// (all zeros: the ref did not exist and the member is initialized).
    pub expected_old_head: String,
    /// The head the ref carries after the landing.
    pub proposed_new_head: String,
    pub transition: WorkspaceHeadTransitionKind,
}

impl BranchCommitMember {
    /// True when landing this member moves its branch ref.
    #[must_use]
    pub fn changes(&self) -> bool {
        self.expected_old_head != self.proposed_new_head
    }
}

/// How the workspace commit fence is acquired.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FenceMode {
    /// Wait for the fence (background and canonical producers).
    Blocking,
    /// Give up after a short bound (interactive inbound producers).
    InteractiveBounded,
}

/// A validated cut waiting to be landed.
///
/// Callers may inspect the members; only [`Self::land`] (and its variants)
/// consumes it. Dropping a prepared cut lands nothing and releases the
/// resources it holds.
#[must_use = "a prepared branch commit lands nothing until it is landed"]
pub struct PreparedBranchCommit {
    advance_id: Uuid,
    org_id: Uuid,
    workspace_id: Uuid,
    target: AuthorityTarget,
    members: Vec<BranchCommitMember>,
    actor: String,
    remote_tracking_completion: Option<crate::models::branch_integration_queue::PushAuthority>,
    additional_refs: Vec<RefCommit>,
    /// Changed members found already landed (see `find_already_landed_heads`),
    /// by `repo_id`, with the commit the producer prepared in
    /// place of the one the ref carries.
    already_landed: BTreeMap<Uuid, String>,
    already_landed_rule: AlreadyLanded,
}

impl PreparedBranchCommit {
    /// Identity of this landing. Consumers that keep bookkeeping about a
    /// landed cut key it by this id; it is minted here and never stored by
    /// the landing itself.
    #[must_use]
    pub fn advance_id(&self) -> Uuid {
        self.advance_id
    }

    #[must_use]
    pub fn target(&self) -> &AuthorityTarget {
        &self.target
    }

    #[must_use]
    pub fn workspace_id(&self) -> Uuid {
        self.workspace_id
    }

    #[must_use]
    pub fn org_id(&self) -> Uuid {
        self.org_id
    }

    #[must_use]
    pub fn members(&self) -> &[BranchCommitMember] {
        &self.members
    }

    pub fn changed_members(&self) -> impl Iterator<Item = &BranchCommitMember> {
        self.members.iter().filter(|member| member.changes())
    }

    #[must_use]
    pub fn member(&self, repo_id: Uuid) -> Option<&BranchCommitMember> {
        self.members.iter().find(|member| member.repo_id == repo_id)
    }

    /// Mark this landing as the completion of a remote-tracking merge run so
    /// the external-Git handoff enqueues a `push` trigger.
    // LEAN-TODO(P1.5): remote-tracking handoff triggers retire with remote tracking.
    pub fn with_remote_tracking_completion(
        mut self,
        authority: crate::models::branch_integration_queue::PushAuthority,
    ) -> Self {
        self.remote_tracking_completion = Some(authority);
        self
    }

    pub(crate) fn with_additional_refs(mut self, updates: Vec<RefCommit>) -> Self {
        self.additional_refs.extend(updates);
        self
    }

    pub async fn land(
        self,
        db: ScopedDb,
        event_bus: &EventBus,
        repo_storage: &TenantRepoStorage,
    ) -> Result<LandedBranchCommit, BranchCommitError> {
        self.land_with(db, event_bus, repo_storage, FenceMode::Blocking, |_, _| {
            Box::pin(async { Ok(()) })
        })
        .await
        .map(|(landed, ())| landed)
    }

    /// Land the cut without letting an interactive request wedge behind another
    /// same-workspace writer; [`BranchCommitError::FenceBusy`] reports the bound.
    pub async fn land_interactive_bounded(
        self,
        db: ScopedDb,
        event_bus: &EventBus,
        repo_storage: &TenantRepoStorage,
    ) -> Result<LandedBranchCommit, BranchCommitError> {
        self.land_with(
            db,
            event_bus,
            repo_storage,
            FenceMode::InteractiveBounded,
            |_, _| Box::pin(async { Ok(()) }),
        )
        .await
        .map(|(landed, ())| landed)
    }

    /// Land the cut and run `in_tx` inside the landing transaction before the refs are applied; its output is
    /// returned with the landed cut.
    ///
    /// The landing takes the workspace fence first and every authority-adjacent
    /// row lock after it; a caller that already wrote in `db` (a fork inserting
    /// its rows) must have taken that same fence before writing. Domain
    /// bookkeeping that must commit atomically with the heads (a task
    /// transition, a merge-run completion, a provider receipt claim) runs in
    /// `in_tx`; an error rolls the whole landing back and leaves every ref
    /// where it was.
    pub async fn land_with<T, F>(
        self,
        db: ScopedDb,
        event_bus: &EventBus,
        repo_storage: &TenantRepoStorage,
        fence_mode: FenceMode,
        in_tx: F,
    ) -> Result<(LandedBranchCommit, T), BranchCommitError>
    where
        T: Send,
        F: for<'a> FnOnce(
            &'a mut ScopedDb,
            &'a [BranchCommitMember],
        )
            -> Pin<Box<dyn Future<Output = Result<T, AppError>> + Send + 'a>>,
    {
        self.land_with_admission(
            db,
            event_bus,
            repo_storage,
            fence_mode,
            |_| Box::pin(async { Ok(()) }),
            in_tx,
        )
        .await
    }

    pub(crate) async fn land_with_admission<T, A, F>(
        self,
        mut db: ScopedDb,
        event_bus: &EventBus,
        repo_storage: &TenantRepoStorage,
        fence_mode: FenceMode,
        admit: A,
        in_tx: F,
    ) -> Result<(LandedBranchCommit, T), BranchCommitError>
    where
        T: Send,
        A: for<'a> FnOnce(
            &'a mut ScopedDb,
        )
            -> Pin<Box<dyn Future<Output = Result<(), AppError>> + Send + 'a>>,
        F: for<'a> FnOnce(
            &'a mut ScopedDb,
            &'a [BranchCommitMember],
        )
            -> Pin<Box<dyn Future<Output = Result<T, AppError>> + Send + 'a>>,
    {
        let PreparedBranchCommit {
            advance_id,
            org_id,
            workspace_id,
            target,
            mut members,
            actor,
            remote_tracking_completion,
            additional_refs,
            mut already_landed,
            already_landed_rule,
        } = self;

        let repos = repo_storage.scoped(&db);
        let landing = async {
            configure_landing_transaction(db.conn()).await?;
            match fence_mode {
                FenceMode::Blocking => {
                    acquire_workspace_commit_fence(db.conn(), workspace_id).await?;
                }
                FenceMode::InteractiveBounded => {
                    if !acquire_workspace_commit_fence_bounded(db.conn(), workspace_id).await? {
                        return Err(BranchCommitError::FenceBusy { workspace_id });
                    }
                }
            }
            // Re-resolve the inventory under row locks and re-read the refs:
            // a member whose ref moved since preparation refuses the cut with
            // the current heads unless the ref carries the member's tree (the
            // member is already landed), and a member whose binding changed
            // identity refuses it outright.
            let inventory = resolve_authority_inventory_for_update(db.conn(), target.clone())
                .await
                .map_err(|error| protocol_error(error.to_string()))?;
            let heads = read_inventory_heads(&repos, &inventory).await?;
            if already_landed_rule == AlreadyLanded::Accept {
                recognize_already_landed_members(
                    &repos,
                    org_id,
                    &heads,
                    &mut members,
                    &mut already_landed,
                )
                .await?;
            }
            verify_members_against_inventory(&target, &members, &inventory, &heads)?;
            for member in &members {
                if let Some(prepared_head) = already_landed.get(&member.repo_id) {
                    tracing::info!(
                        %advance_id,
                        repo_id = %member.repo_id,
                        branch = %member.branch_name,
                        %prepared_head,
                        landed_head = %member.proposed_new_head,
                        "Branch ref already carries this member's tree; the landing keeps that commit and applies the member's database side"
                    );
                }
            }
            admit(&mut db).await.map_err(BranchCommitError::App)?;
            // Domain inputs registered against this landing (remote-tracking
            // metadata, merge completions) apply with the heads, then the
            // caller's own bookkeeping.
            let mut ref_effects =
                crate::services::advance_consumer::apply_registered_landing_consumers(
                    db.conn(),
                    advance_id,
                    &target,
                    &members,
                    &repos,
                    org_id,
                )
                .await
                .map_err(BranchCommitError::App)?;
            ref_effects.extend(additional_refs);
            let output = in_tx(&mut db, &members)
                .await
                .map_err(BranchCommitError::App)?;

            enqueue_remote_work_during_landing(
                db.conn(),
                advance_id,
                &members,
                remote_tracking_completion,
            )
            .await?;

            // Every database check that could refuse this landing runs now,
            // before Git moves a ref: deferred constraint triggers fire here
            // rather than at COMMIT (`g5.git.landing.commit-point`).
            sqlx::query("SET CONSTRAINTS ALL IMMEDIATE")
                .execute(db.conn())
                .await?;

            // Git acceptance is the landing's commit point: the refs are the
            // head authority, so the cut is applied to them by compare-and-swap
            // under the fence, and a refusal rolls the transaction back with
            // nothing landed anywhere.
            let projection_warnings =
                apply_member_ref_updates(&target, &members, &ref_effects, &repos, org_id)?;
            // Git has accepted the cut. Observation is best-effort and its
            // savepoint cannot poison the authoritative landing transaction.
            if members.iter().any(BranchCommitMember::changes) {
                super::workspace_revision_history::observe(
                    db.conn(),
                    &repos,
                    org_id,
                    workspace_id,
                    members
                        .iter()
                        .map(|m| g5_contracts::types::MergeSourceHead {
                            repo_id: m.repo_id.to_string(),
                            workspace_id: workspace_id.to_string(),
                            branch_name: m.branch_name.clone(),
                            head_commit: m.proposed_new_head.clone(),
                        })
                        .collect(),
                    chrono::Utc::now(),
                    &members
                        .iter()
                        .filter(|m| m.changes())
                        .map(|m| m.repo_id.to_string())
                        .collect::<Vec<_>>(),
                )
                .await;
            }

            #[cfg(feature = "test-helpers")]
            crate::services::git_test_barrier::clone_merge_test_barrier::wait_at(
                workspace_id,
                crate::services::git_test_barrier::clone_merge_test_barrier::Stage::RefsApplied,
            )
            .await;
            Ok((projection_warnings, output))
        };

        let (projection_warnings, output) = match landing.await {
            Ok(outcome) => outcome,
            Err(error) => {
                if let Err(rollback_error) = db.rollback().await {
                    tracing::warn!(
                        %advance_id,
                        %workspace_id,
                        %rollback_error,
                        "Branch commit landing rollback failed"
                    );
                }
                return Err(error);
            }
        };

        if let Err(commit_error) = db.commit().await {
            // Git accepted the cut under the fence, and only an infrastructure
            // failure can stop COMMIT now that every deferred check has run.
            // The refs keep the cut, exactly as after a cancelled request or
            // an unknown COMMIT outcome: refs are never retracted once the
            // fence is released, so a fenced reader never captures heads that
            // later disappear. The database side converges from the refs
            // (design v6 §9.6).
            tracing::warn!(
                %advance_id,
                %workspace_id,
                error = %commit_error,
                "Database COMMIT failed after Git accepted the cut; the refs keep the cut and the projections converge"
            );
            return Err(BranchCommitError::Database(protocol_error(
                commit_error.to_string(),
            )));
        }
        let committed_at = chrono::Utc::now();

        for member in members.iter().filter(|member| member.changes()) {
            event_bus.publish(
                &Channel::Workspace(workspace_id),
                &WsEvent::WorkspaceBranchUpdated {
                    workspace_id,
                    repo_id: member.repo_id,
                    branch: member.branch_name.clone(),
                    head_commit: member.proposed_new_head.clone(),
                    hint: Some(BranchUpdateHint {
                        ref_name: format!("refs/heads/{}", member.branch_name),
                        old: member.expected_old_head.clone(),
                        new: member.proposed_new_head.clone(),
                        actor: actor.clone(),
                    }),
                },
            );
        }

        Ok((
            LandedBranchCommit {
                advance_id,
                org_id,
                workspace_id,
                target,
                members,
                committed_at,
                projection_warnings,
            },
            output,
        ))
    }
}

/// A cut that landed: its refs carry `proposed_new_head`, its caches were
/// refreshed and its hints were published.
#[derive(Debug, Clone)]
pub struct LandedBranchCommit {
    pub advance_id: Uuid,
    pub org_id: Uuid,
    pub workspace_id: Uuid,
    pub target: AuthorityTarget,
    pub members: Vec<BranchCommitMember>,
    pub committed_at: chrono::DateTime<chrono::Utc>,
    /// Non-fatal follow-up work that remains pending (the convergence loop
    /// or the owning maintenance loop retries it).
    pub projection_warnings: Vec<String>,
}

/// What a landing owes to parties outside Git once it committed: today the
/// reviewers of the workspace. Git holds one at composition and never names
/// the notifier's own dependencies. A notification problem never fails the
/// caller: the landing already happened.
#[async_trait::async_trait]
pub trait LandedCommitNotifier: Send + Sync {
    async fn notify_after_landing(&self, landed: &LandedBranchCommit, actor_user_id: Uuid);
}

impl LandedBranchCommit {
    pub fn changed_members(&self) -> impl Iterator<Item = &BranchCommitMember> {
        self.members.iter().filter(|member| member.changes())
    }

    #[must_use]
    pub fn member(&self, repo_id: Uuid) -> Option<&BranchCommitMember> {
        self.members.iter().find(|member| member.repo_id == repo_id)
    }

    /// The exact landed version of one member, for a content read that must
    /// see what was just committed.
    pub fn read_view(
        &self,
        scope: &(impl OrgScope + ?Sized),
        repo_id: Uuid,
    ) -> Result<CommittedRepoReadView, G5Error> {
        let member = self
            .member(repo_id)
            .ok_or_else(|| G5Error::ValidationError {
                details: serde_json::json!({
                    "message": "Repository is not a member of the landed cut",
                    "advance_id": self.advance_id,
                    "repo_id": repo_id,
                }),
            })?;
        CommittedRepoReadView::from_landed_head(
            scope,
            self.org_id,
            member.repo_id,
            &member.proposed_new_head,
        )
    }
}

/// Why a cut did not land.
#[derive(Debug, thiserror::Error)]
pub enum BranchCommitError {
    #[error("Workspace commit fence for {workspace_id} remained busy")]
    FenceBusy { workspace_id: Uuid },
    #[error(transparent)]
    Database(#[from] sqlx::Error),
    #[error("{0}")]
    App(AppError),
}

impl From<BranchCommitError> for AppError {
    fn from(error: BranchCommitError) -> Self {
        match error {
            BranchCommitError::FenceBusy { workspace_id } => G5Error::Conflict {
                reason: format!(
                    "workspace commit fence for {workspace_id} remained busy; retry shortly"
                ),
            }
            .into(),
            BranchCommitError::Database(error) => map_branch_commit_app_error(error),
            BranchCommitError::App(error) => error,
        }
    }
}

impl From<BranchCommitError> for G5Error {
    fn from(error: BranchCommitError) -> Self {
        AppError::from(error).inner
    }
}

/// True when git refused the cut's branch ref update for a reason other than
/// a stale expectation: nothing landed in git or in the database and the
/// durable rows the cut was initializing keep their unmaterialized state.
#[must_use]
pub fn is_branch_ref_refusal(error: &BranchCommitError) -> bool {
    matches!(
        error,
        BranchCommitError::Database(sqlx::Error::Protocol(reason))
            if reason.starts_with(BRANCH_REF_REFUSED_PROTOCOL_PREFIX)
    )
}

/// Decode a landing's protocol errors into the route contract every producer
/// relies on: a stale cut is a typed [`G5Error::AuthorityCutStale`] conflict
/// with the current heads, a git refusal is a git error, and everything else
/// keeps its ordinary mapping.
#[must_use]
pub fn map_branch_commit_error(error: sqlx::Error) -> G5Error {
    match error {
        sqlx::Error::Protocol(reason) => {
            if let Some(encoded) = reason.strip_prefix(AUTHORITY_CUT_STALE_PROTOCOL_PREFIX) {
                match serde_json::from_str(encoded) {
                    Ok(details) => G5Error::AuthorityCutStale {
                        details: Box::new(details),
                    },
                    Err(parse_error) => G5Error::Internal {
                        message: format!(
                            "Malformed authority-cut stale protocol details: {parse_error}"
                        ),
                    },
                }
            } else if let Some(message) = reason.strip_prefix(BRANCH_REF_REFUSED_PROTOCOL_PREFIX) {
                G5Error::GitError {
                    message: message.to_string(),
                }
            } else {
                G5Error::Conflict { reason }
            }
        }
        other => G5Error::from(other),
    }
}

#[must_use]
pub fn map_branch_commit_app_error(error: sqlx::Error) -> AppError {
    AppError::from(map_branch_commit_error(error))
}

/// Serialize every writer of one workspace's heads and head-adjacent rows for
/// the rest of the transaction.
pub async fn acquire_workspace_commit_fence(
    conn: &mut PgConnection,
    workspace_id: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "SELECT pg_advisory_xact_lock(\
             hashtextextended('g5:workspace-commit:' || $1::text, 0)\
         )",
    )
    .bind(workspace_id)
    .execute(&mut *conn)
    .await?;
    Ok(())
}

/// Capture a coherent cut concurrently with other readers, while excluding
/// writers until the authorized read transaction releases its snapshot.
pub async fn acquire_workspace_read_fence(
    conn: &mut PgConnection,
    workspace_id: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query("SELECT pg_advisory_xact_lock_shared(hashtextextended('g5:workspace-commit:' || $1::text, 0))")
        .bind(workspace_id).execute(conn).await?;
    Ok(())
}

/// Try the same fence without blocking. The SQL key expression must remain
/// byte-identical to the blocking acquisition so both contend in the same
/// advisory-lock namespace.
pub async fn try_acquire_workspace_commit_fence(
    conn: &mut PgConnection,
    workspace_id: Uuid,
) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT pg_try_advisory_xact_lock(\
             hashtextextended('g5:workspace-commit:' || $1::text, 0)\
         )",
    )
    .bind(workspace_id)
    .fetch_one(&mut *conn)
    .await
}

/// Bound the fence for an interactive inbound producer. A false result means
/// the bound elapsed without acquiring any authority-adjacent lock.
pub async fn acquire_workspace_commit_fence_bounded(
    conn: &mut PgConnection,
    workspace_id: Uuid,
) -> Result<bool, sqlx::Error> {
    let deadline = std::time::Instant::now() + INTERACTIVE_FENCE_ACQUIRE_BOUND;
    loop {
        if try_acquire_workspace_commit_fence(conn, workspace_id).await? {
            return Ok(true);
        }
        let now = std::time::Instant::now();
        if now >= deadline {
            return Ok(false);
        }
        let Some(remaining) = deadline.checked_duration_since(now) else {
            continue;
        };
        tokio::time::sleep(INTERACTIVE_FENCE_RETRY_INTERVAL.min(remaining)).await;
    }
}

/// The retryable refusal for an interactive operation that could not take a
/// workspace's commit fence within its bound.
#[must_use]
pub fn commit_fence_busy(operation: &str) -> G5Error {
    G5Error::ValidationError {
        details: serde_json::json!({
            "code": "WORKSPACE_COMMIT_FENCE_BUSY",
            "operation": operation,
            "message": "Another operation is committing this workspace; retry shortly.",
        }),
    }
}

/// Prepare one complete cut of the target from the producer's vector.
///
/// `repos` names every materialized member of the target with the head its
/// ref carried when the producer computed its result (`expected_old_head`)
/// and the head it should carry afterwards; an unborn member is initialized
/// by naming an all-zero expectation. Members whose expectation disagrees
/// with the ref refuse the cut as [`G5Error::AuthorityCutStale`] with the
/// current heads.
pub async fn prepare_branch_commit(
    db: &ScopedDbReopener,
    repo_storage: &TenantRepoStorage,
    target: AuthorityTarget,
    repos: Vec<WorkspaceHeadRepoInput>,
    actor: &str,
) -> Result<PreparedBranchCommit, sqlx::Error> {
    prepare_branch_commit_with_transition(
        db,
        repo_storage,
        target,
        repos,
        actor,
        WorkspaceHeadTransitionKind::Advance,
    )
    .await
}

/// [`prepare_branch_commit`] with every changed member typed `transition`
/// (a server-selected rebase or a task retry reseed) instead of `advance`.
pub async fn prepare_branch_commit_with_transition(
    db: &ScopedDbReopener,
    repo_storage: &TenantRepoStorage,
    target: AuthorityTarget,
    repos: Vec<WorkspaceHeadRepoInput>,
    actor: &str,
    transition: WorkspaceHeadTransitionKind,
) -> Result<PreparedBranchCommit, sqlx::Error> {
    capture_branch_commit(
        db,
        repo_storage,
        target,
        repos,
        actor,
        transition,
        AlreadyLanded::Refuse,
    )
    .await
}

async fn capture_branch_commit(
    db: &ScopedDbReopener,
    repo_storage: &TenantRepoStorage,
    target: AuthorityTarget,
    repos: Vec<WorkspaceHeadRepoInput>,
    actor: &str,
    transition: WorkspaceHeadTransitionKind,
    already_landed_rule: AlreadyLanded,
) -> Result<PreparedBranchCommit, sqlx::Error> {
    let scoped = repo_storage.scoped(db);
    let mut capture_db = db
        .open()
        .await
        .map_err(|error| protocol_error(error.to_string()))?;
    let prepared = prepare_cut_in_tx(
        capture_db.conn(),
        &scoped,
        target,
        repos,
        actor,
        transition,
        already_landed_rule,
    )
    .await;
    capture_db
        .rollback()
        .await
        .map_err(|error| protocol_error(error.to_string()))?;
    prepared
}

/// Prepare a cut inside the caller's transaction, for producers that inserted
/// the target's rows in that same transaction (workspace creation, fork).
///
/// No connected draft adapter runs here: a target whose rows are being
/// created has no draft yet.
pub async fn prepare_branch_commit_in_tx(
    db: &mut ScopedDb,
    repo_storage: &TenantRepoStorage,
    target: AuthorityTarget,
    repos: Vec<WorkspaceHeadRepoInput>,
    actor: &str,
    transition: WorkspaceHeadTransitionKind,
) -> Result<PreparedBranchCommit, sqlx::Error> {
    let scoped = repo_storage.scoped(&*db);
    prepare_cut_in_tx(
        db.conn(),
        &scoped,
        target,
        repos,
        actor,
        transition,
        AlreadyLanded::Refuse,
    )
    .await
}

async fn prepare_cut_in_tx(
    conn: &mut PgConnection,
    repo_storage: &ScopedRepoStorage,
    target: AuthorityTarget,
    mut repos: Vec<WorkspaceHeadRepoInput>,
    actor: &str,
    transition: WorkspaceHeadTransitionKind,
    already_landed_rule: AlreadyLanded,
) -> Result<PreparedBranchCommit, sqlx::Error> {
    validate_repo_inputs(&repos)?;
    repos.sort_by_key(|repo| repo.repo_id);
    let inventory = resolve_authority_inventory(conn, target.clone())
        .await
        .map_err(|error| protocol_error(error.to_string()))?;
    let heads = read_inventory_heads(repo_storage, &inventory).await?;
    let landed_heads = match already_landed_rule {
        AlreadyLanded::Accept => {
            find_already_landed_heads(
                repo_storage,
                inventory.org_id,
                already_landed_candidates(&inventory, &heads, &repos),
            )
            .await?
        }
        AlreadyLanded::Refuse => BTreeMap::new(),
    };
    let members = build_members(
        &target,
        &inventory,
        &heads,
        &repos,
        transition,
        &landed_heads,
    )?;
    let already_landed = replaced_prepared_heads(&members, &repos);
    Ok(PreparedBranchCommit {
        advance_id: Uuid::new_v4(),
        org_id: inventory.org_id,
        workspace_id: inventory.workspace_id,
        target,
        members,
        actor: actor.to_string(),
        remote_tracking_completion: None,
        additional_refs: Vec::new(),
        already_landed,
        already_landed_rule,
    })
}

/// One unborn member of a workspace and the commit that materializes it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InitialWorkspaceHeadInput {
    pub repo_id: Uuid,
    pub proposed_new_head: String,
}

/// Prepare the first materialization of one or more unborn members of a
/// workspace, inside the caller's transaction.
///
/// Every named Base member must be unborn (no branch ref); already materialized
/// siblings stay unchanged members of the same cut and unrelated unborn Base
/// inventory stays outside it. Missing working refs require explicit repair.
pub async fn prepare_initial_branch_commit_in_tx(
    db: &mut ScopedDb,
    repo_storage: &TenantRepoStorage,
    target: AuthorityTarget,
    initial_heads: Vec<InitialWorkspaceHeadInput>,
    actor: &str,
) -> Result<PreparedBranchCommit, sqlx::Error> {
    let repo_storage = &repo_storage.scoped(&*db);
    let conn = db.conn();
    if !matches!(target, AuthorityTarget::Workspace { .. }) {
        return Err(protocol_error(
            "First repository materialization is valid only for workspace authority",
        ));
    }
    if initial_heads.is_empty() {
        return Err(protocol_error(
            "First materialization must select at least one unborn repository member",
        ));
    }
    let candidate_repo_ids = initial_heads
        .iter()
        .map(|candidate| candidate.repo_id)
        .collect::<BTreeSet<_>>();
    if candidate_repo_ids.len() != initial_heads.len() {
        return Err(protocol_error(
            "First materialization cannot name one repository member twice",
        ));
    }
    if initial_heads
        .iter()
        .any(|candidate| !is_git_sha(&candidate.proposed_new_head))
    {
        return Err(protocol_error(
            "First materialization must propose exact non-zero Git SHAs",
        ));
    }
    let inventory = resolve_authority_inventory(conn, target.clone())
        .await
        .map_err(|error| protocol_error(error.to_string()))?;
    if initial_heads.iter().any(|candidate| {
        inventory
            .repos
            .iter()
            .all(|row| row.repo_id != candidate.repo_id)
    }) {
        return Err(protocol_error(
            "First materialization names a repository outside the target authority inventory",
        ));
    }
    let heads = read_inventory_heads(repo_storage, &inventory).await?;
    let candidate_heads = initial_heads
        .iter()
        .map(|candidate| (candidate.repo_id, candidate.proposed_new_head.clone()))
        .collect::<BTreeMap<_, _>>();
    let mut repos = Vec::new();
    for row in &inventory.repos {
        let current = heads
            .get(&(row.repo_id, row.branch_name.clone()))
            .cloned()
            .flatten();
        match (candidate_heads.get(&row.repo_id), current) {
            (Some(proposed), current) => {
                if current.is_none() && !row.may_be_unborn {
                    return Err(protocol_error(
                        "Only Base repositories may be initialized; a missing working ref must be repaired explicitly",
                    ));
                }
                // A ref that already carries the candidate is an idempotent
                // replay; a ref at any other head refuses the cut as stale.
                repos.push(WorkspaceHeadRepoInput {
                    repo_id: row.repo_id,
                    expected_old_head: ZERO_GIT_SHA.to_string(),
                    proposed_new_head: proposed.clone(),
                });
            }
            (None, Some(current)) => repos.push(WorkspaceHeadRepoInput {
                repo_id: row.repo_id,
                expected_old_head: current.clone(),
                proposed_new_head: current,
            }),
            (None, None) => {}
        }
    }
    let members = build_members(
        &target,
        &inventory,
        &heads,
        &repos,
        WorkspaceHeadTransitionKind::Advance,
        &BTreeMap::new(),
    )?;
    Ok(PreparedBranchCommit {
        advance_id: Uuid::new_v4(),
        org_id: inventory.org_id,
        workspace_id: inventory.workspace_id,
        target,
        members,
        actor: actor.to_string(),
        remote_tracking_completion: None,
        additional_refs: Vec::new(),
        already_landed: BTreeMap::new(),
        already_landed_rule: AlreadyLanded::Refuse,
    })
}

/// Prepare the first materialization of unborn members without holding a
/// caller transaction (a short capture transaction is opened and closed).
pub async fn prepare_initial_branch_commit(
    db: &ScopedDbReopener,
    repo_storage: &TenantRepoStorage,
    target: AuthorityTarget,
    initial_heads: Vec<InitialWorkspaceHeadInput>,
    actor: &str,
) -> Result<PreparedBranchCommit, sqlx::Error> {
    let mut capture_db = db
        .open()
        .await
        .map_err(|error| protocol_error(error.to_string()))?;
    let prepared = prepare_initial_branch_commit_in_tx(
        &mut capture_db,
        repo_storage,
        target,
        initial_heads,
        actor,
    )
    .await;
    capture_db
        .rollback()
        .await
        .map_err(|error| protocol_error(error.to_string()))?;
    prepared
}

fn validate_repo_inputs(inputs: &[WorkspaceHeadRepoInput]) -> Result<(), sqlx::Error> {
    let mut repo_ids = BTreeSet::new();
    for input in inputs {
        if !repo_ids.insert(input.repo_id) {
            return Err(protocol_error(format!(
                "Repository {} appears twice in one workspace head cut",
                input.repo_id
            )));
        }
        let expected_ok =
            input.expected_old_head == ZERO_GIT_SHA || is_git_sha(&input.expected_old_head);
        if !expected_ok || !is_git_sha(&input.proposed_new_head) {
            return Err(protocol_error(format!(
                "Repository {} head cut must name exact Git SHAs (expected {}, proposed {})",
                input.repo_id, input.expected_old_head, input.proposed_new_head
            )));
        }
    }
    Ok(())
}

async fn read_inventory_heads(
    repo_storage: &ScopedRepoStorage,
    inventory: &AuthorityInventorySnapshot,
) -> Result<BTreeMap<(Uuid, String), Option<String>>, sqlx::Error> {
    let members = inventory
        .repos
        .iter()
        .map(|row| (row.repo_id, row.branch_name.clone()))
        .collect::<Vec<_>>();
    read_branch_head_oids(repo_storage, inventory.org_id, &members)
        .await
        .map_err(|error| protocol_error(error.to_string()))
}

/// Whether a cut takes a member whose ref already carries its tree as landed
/// (`find_already_landed_heads`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AlreadyLanded {
    /// The producer reports the landed members (draft commit-all), so its
    /// retry converges on the ref's commit.
    Accept,
    /// The producer records the commit it prepared (a merge commit, a worker
    /// checkpoint, a remote tip), so such a member refuses the cut as stale.
    Refuse,
}

/// A changed member whose branch ref carries neither the cut's expectation nor
/// its result.
struct AlreadyLandedCandidate {
    repo_id: Uuid,
    current_head: String,
    proposed_new_head: String,
}

/// The recovery rule of a landing that moves refs before its database
/// transaction commits (P1.4).
///
/// A refused COMMIT whose ref restore failed, a partial cross-repository ref
/// write, or a request cancelled or crashed after its ref write leaves a
/// member's ref ahead of the database. When a changed member's ref already
/// carries a commit whose tree is the tree the cut would write, the member is
/// already landed: the cut takes the ref's commit as the member's result, so
/// its compare-and-swap finds the ref applied and writes nothing, and its
/// database side (draft rollover and retirement, head caches, registered
/// consumers) runs with that commit. Commit SHAs embed timestamps and differ
/// between attempts; the tree of recomputed content does not. A ref at any
/// other head still refuses the cut as stale.
///
/// Only a cut prepared with [`AlreadyLanded::Accept`] applies the rule, at
/// preparation and again under the landing's fence.
///
/// Returns, by repository, the ref commit of every candidate that qualifies.
async fn find_already_landed_heads(
    repo_storage: &ScopedRepoStorage,
    org_id: Uuid,
    candidates: Vec<AlreadyLandedCandidate>,
) -> Result<BTreeMap<Uuid, String>, sqlx::Error> {
    if candidates.is_empty() {
        return Ok(BTreeMap::new());
    }
    let lookups = candidates
        .into_iter()
        .filter_map(|candidate| {
            repo_storage
                .repo_path(org_id, candidate.repo_id)
                .ok()
                .map(|repo_path| (candidate, repo_path))
        })
        .collect::<Vec<_>>();
    tokio::task::spawn_blocking(move || {
        lookups
            .into_iter()
            .filter(|(candidate, repo_path)| {
                // A commit that cannot be read is not an earlier landing.
                GitRepo::open(repo_path)
                    .and_then(|repo| {
                        Ok::<bool, G5Error>(
                            repo.commit_tree_id(&candidate.current_head)?
                                == repo.commit_tree_id(&candidate.proposed_new_head)?,
                        )
                    })
                    .unwrap_or(false)
            })
            .map(|(candidate, _)| (candidate.repo_id, candidate.current_head))
            .collect()
    })
    .await
    .map_err(|error| protocol_error(format!("Landed tree comparison failed: {error}")))
}

/// The changed members of a producer's cut whose ref is at neither head.
fn already_landed_candidates(
    inventory: &AuthorityInventorySnapshot,
    heads: &BTreeMap<(Uuid, String), Option<String>>,
    inputs: &[WorkspaceHeadRepoInput],
) -> Vec<AlreadyLandedCandidate> {
    inventory
        .repos
        .iter()
        .filter_map(|row| {
            let input = inputs.iter().find(|input| input.repo_id == row.repo_id)?;
            let current_head = heads
                .get(&(row.repo_id, row.branch_name.clone()))
                .cloned()
                .flatten()?;
            (input.expected_old_head != input.proposed_new_head
                && current_head != input.expected_old_head
                && current_head != input.proposed_new_head)
                .then(|| AlreadyLandedCandidate {
                    repo_id: row.repo_id,
                    current_head,
                    proposed_new_head: input.proposed_new_head.clone(),
                })
        })
        .collect()
}

/// The prepared commit each already-landed member of a freshly built cut
/// replaced, by repository ID.
fn replaced_prepared_heads(
    members: &[BranchCommitMember],
    inputs: &[WorkspaceHeadRepoInput],
) -> BTreeMap<Uuid, String> {
    members
        .iter()
        .filter_map(|member| {
            let input = inputs
                .iter()
                .find(|input| input.repo_id == member.repo_id)?;
            (input.proposed_new_head != member.proposed_new_head)
                .then(|| (member.repo_id, input.proposed_new_head.clone()))
        })
        .collect()
}

/// Apply the recovery rule under the landing's fence to the changed members
/// whose ref moved after the cut was prepared.
async fn recognize_already_landed_members(
    repo_storage: &ScopedRepoStorage,
    org_id: Uuid,
    heads: &BTreeMap<(Uuid, String), Option<String>>,
    members: &mut [BranchCommitMember],
    already_landed: &mut BTreeMap<Uuid, String>,
) -> Result<(), sqlx::Error> {
    let candidates = members
        .iter()
        .filter(|member| member.changes())
        .filter_map(|member| {
            let current_head = heads
                .get(&(member.repo_id, member.branch_name.clone()))
                .cloned()
                .flatten()?;
            (current_head != member.expected_old_head && current_head != member.proposed_new_head)
                .then(|| AlreadyLandedCandidate {
                    repo_id: member.repo_id,
                    current_head,
                    proposed_new_head: member.proposed_new_head.clone(),
                })
        })
        .collect();
    let landed_heads = find_already_landed_heads(repo_storage, org_id, candidates).await?;
    for member in members.iter_mut() {
        if let Some(landed_head) = landed_heads.get(&member.repo_id) {
            already_landed
                .entry(member.repo_id)
                .or_insert_with(|| member.proposed_new_head.clone());
            member.proposed_new_head = landed_head.clone();
        }
    }
    Ok(())
}

fn build_members(
    target: &AuthorityTarget,
    inventory: &AuthorityInventorySnapshot,
    heads: &BTreeMap<(Uuid, String), Option<String>>,
    inputs: &[WorkspaceHeadRepoInput],
    transition: WorkspaceHeadTransitionKind,
    already_landed: &BTreeMap<Uuid, String>,
) -> Result<Vec<BranchCommitMember>, sqlx::Error> {
    if inputs.iter().any(|input| {
        inventory
            .repos
            .iter()
            .all(|row| input.repo_id != row.repo_id)
    }) {
        return Err(protocol_error(
            "A workspace head cut names a repository outside the target inventory",
        ));
    }
    if !matches!(target, AuthorityTarget::Workspace { .. }) {
        if let Some(input) = inputs
            .iter()
            .find(|input| input.expected_old_head == ZERO_GIT_SHA)
        {
            return Err(protocol_error(format!(
                "Repository {} cannot be initialized by a {} cut: first repository materialization is valid only for workspace authority",
                input.repo_id,
                target.kind().as_str()
            )));
        }
    }
    let mut members = Vec::with_capacity(inventory.repos.len());
    let mut mismatches = Vec::new();
    for row in &inventory.repos {
        let current = heads
            .get(&(row.repo_id, row.branch_name.clone()))
            .cloned()
            .flatten();
        let input = inputs.iter().find(|input| input.repo_id == row.repo_id);
        match (input, current) {
            (Some(input), Some(current)) => {
                // A ref already at the proposed head is an idempotent replay
                // of the same result, exactly as git's compare-and-swap treats
                // it. A ref at a commit with the proposed tree is an earlier
                // landing of the same content, and the member takes that
                // commit (`find_already_landed_heads`). Any other head refuses
                // the cut as stale.
                let proposed_new_head =
                    if input.expected_old_head == current || input.proposed_new_head == current {
                        input.proposed_new_head.clone()
                    } else if already_landed.get(&row.repo_id) == Some(&current) {
                        current
                    } else {
                        mismatches.push(AuthorityCutMismatch {
                            repo_id: row.repo_id.to_string(),
                            expected_old: input.expected_old_head.clone(),
                            current_head: current,
                        });
                        continue;
                    };
                members.push(BranchCommitMember {
                    repo_id: row.repo_id,
                    workspace_id: row.workspace_id,
                    branch_name: row.branch_name.clone(),
                    expected_old_head: input.expected_old_head.clone(),
                    proposed_new_head,
                    transition: if input.expected_old_head == input.proposed_new_head {
                        WorkspaceHeadTransitionKind::Advance
                    } else if input.expected_old_head == ZERO_GIT_SHA {
                        WorkspaceHeadTransitionKind::Initialize
                    } else {
                        transition
                    },
                });
            }
            (Some(input), None) => {
                if !row.may_be_unborn {
                    return Err(protocol_error(
                        "Working workspace branch is missing; implicit recreation is forbidden",
                    ));
                }
                if input.expected_old_head != ZERO_GIT_SHA {
                    mismatches.push(AuthorityCutMismatch {
                        repo_id: row.repo_id.to_string(),
                        expected_old: input.expected_old_head.clone(),
                        current_head: String::new(),
                    });
                    continue;
                }
                members.push(BranchCommitMember {
                    repo_id: row.repo_id,
                    workspace_id: row.workspace_id,
                    branch_name: row.branch_name.clone(),
                    expected_old_head: ZERO_GIT_SHA.to_string(),
                    proposed_new_head: input.proposed_new_head.clone(),
                    transition: WorkspaceHeadTransitionKind::Initialize,
                });
            }
            (None, Some(_)) => {
                return Err(protocol_error(format!(
                    "Repository {} {REGISTERED_TRANSITION_MISMATCH}: a materialized member is missing from the complete cut",
                    row.repo_id
                )));
            }
            (None, None) => {
                if !row.may_be_unborn {
                    return Err(protocol_error(
                        "Working workspace branch is missing; a complete cut cannot omit it",
                    ));
                }
                // An unborn Base member outside the cut stays unborn.
            }
        }
    }
    if !mismatches.is_empty() {
        return Err(stale_cut_error(target, inventory, heads, mismatches)?);
    }
    if members.is_empty() {
        return Err(protocol_error(
            "A workspace head cut must name at least one repository member",
        ));
    }
    Ok(members)
}

fn stale_cut_error(
    target: &AuthorityTarget,
    inventory: &AuthorityInventorySnapshot,
    heads: &BTreeMap<(Uuid, String), Option<String>>,
    mismatches: Vec<AuthorityCutMismatch>,
) -> Result<sqlx::Error, sqlx::Error> {
    let details = AuthorityCutStaleDetails {
        workspace_id: target.workspace_id().to_string(),
        target_kind: target.kind().as_str().to_string(),
        target_id: target.target_id().to_string(),
        current_cut: inventory
            .repos
            .iter()
            .map(|row| AuthorityCutMember {
                repo_id: row.repo_id.to_string(),
                workspace_id: row.workspace_id.to_string(),
                branch_name: row.branch_name.clone(),
                current_head: heads
                    .get(&(row.repo_id, row.branch_name.clone()))
                    .cloned()
                    .flatten()
                    .unwrap_or_else(|| ZERO_GIT_SHA.to_string()),
            })
            .collect(),
        mismatches,
    };
    let encoded = serde_json::to_string(&details).map_err(|error| {
        protocol_error(format!("failed to encode stale authority cut: {error}"))
    })?;
    Ok(protocol_error(format!(
        "{AUTHORITY_CUT_STALE_PROTOCOL_PREFIX}{encoded}"
    )))
}

fn verify_members_against_inventory(
    target: &AuthorityTarget,
    members: &[BranchCommitMember],
    inventory: &AuthorityInventorySnapshot,
    heads: &BTreeMap<(Uuid, String), Option<String>>,
) -> Result<(), BranchCommitError> {
    let mut mismatches = Vec::new();
    for member in members {
        let row = inventory
            .repos
            .iter()
            .find(|row| row.repo_id == member.repo_id)
            .ok_or_else(|| {
                protocol_error(format!(
                    "Repository {} left the target inventory before its cut landed",
                    member.repo_id
                ))
            })?;
        if row.workspace_id != member.workspace_id || row.branch_name != member.branch_name {
            return Err(protocol_error(format!(
                "Repository {} {REGISTERED_TRANSITION_MISMATCH}: its binding changed before the cut landed",
                member.repo_id
            ))
            .into());
        }
        let current = heads
            .get(&(row.repo_id, row.branch_name.clone()))
            .cloned()
            .flatten();
        let expected_current = if member.expected_old_head == ZERO_GIT_SHA {
            None
        } else {
            Some(member.expected_old_head.clone())
        };
        if current != expected_current
            && current.as_deref() != Some(member.proposed_new_head.as_str())
        {
            mismatches.push(AuthorityCutMismatch {
                repo_id: member.repo_id.to_string(),
                expected_old: member.expected_old_head.clone(),
                current_head: current.unwrap_or_default(),
            });
        }
    }
    let members_in_cut = members
        .iter()
        .map(|member| member.repo_id)
        .collect::<BTreeSet<_>>();
    for row in &inventory.repos {
        if members_in_cut.contains(&row.repo_id) {
            continue;
        }
        if heads
            .get(&(row.repo_id, row.branch_name.clone()))
            .cloned()
            .flatten()
            .is_some()
        {
            return Err(protocol_error(format!(
                "Repository {} {REGISTERED_TRANSITION_MISMATCH}: it materialized after the cut was prepared",
                row.repo_id
            ))
            .into());
        }
        if !row.may_be_unborn {
            return Err(protocol_error(format!("Working repository {REGISTERED_TRANSITION_MISMATCH}: it is missing from the complete cut")).into());
        }
    }
    if mismatches.is_empty() {
        Ok(())
    } else {
        Err(stale_cut_error(target, inventory, heads, mismatches)?.into())
    }
}

/// Put back the refs a cut wrote before Git failed partway through it. This
/// runs while the landing still holds the workspace fence, so no fenced reader
/// ever sees the partial cut. Each ref is restored by its own compare-and-swap
/// from the landed head (a created ref is deleted only while it still names
/// the landed head), so a ref some other writer has moved since is left as it
/// is and reported: the restore can never overwrite someone else's write.
fn restore_applied_branch_refs(applied: &[RefCommit]) -> Vec<String> {
    let mut unresolved = Vec::new();
    for update in applied.iter().rev() {
        let rollback = RefCommit {
            repo_path: update.repo_path.clone(),
            ref_name: update.ref_name.clone(),
            expected_old: update.new_oid.clone(),
            new_oid: update.expected_old.clone(),
        };
        if let Err(error) = GitRepo::apply_ref_commits(&[rollback]) {
            unresolved.push(format!(
                "{} in {}: {error:?}",
                update.ref_name,
                update.repo_path.display()
            ));
        }
    }
    unresolved
}

/// Apply every changed member to its branch ref as one cut under git's ref
/// locks. A ref already at its result is an idempotent replay; a ref at any
/// other head refuses the whole cut before anything is written.
fn apply_member_ref_updates(
    target: &AuthorityTarget,
    members: &[BranchCommitMember],
    additional_refs: &[RefCommit],
    repo_storage: &ScopedRepoStorage,
    org_id: Uuid,
) -> Result<Vec<String>, BranchCommitError> {
    let mut updates = Vec::new();
    let mut changed = Vec::new();
    for member in members {
        let Ok(repo_path) = repo_storage.repo_path(org_id, member.repo_id) else {
            return Err(protocol_error(format!(
                "{BRANCH_REF_REFUSED_PROTOCOL_PREFIX}Repository {} is unavailable for branch ref application",
                member.repo_id
            ))
            .into());
        };
        updates.push(RefCommit {
            repo_path,
            ref_name: format!("refs/heads/{}", member.branch_name),
            expected_old: (member.expected_old_head != ZERO_GIT_SHA)
                .then(|| member.expected_old_head.clone()),
            new_oid: Some(member.proposed_new_head.clone()),
        });
        changed.push(member);
    }
    updates.extend_from_slice(additional_refs);
    if updates.is_empty() {
        return Ok(Vec::new());
    }
    match GitRepo::apply_ref_commits(&updates) {
        Ok(_) => Ok(Vec::new()),
        Err(BranchRefCutRefusal::Stale { mismatches }) => {
            if let Some(mismatch) = mismatches
                .iter()
                .find(|mismatch| mismatch.index >= members.len())
            {
                return Err(protocol_error(format!("{BRANCH_REF_REFUSED_PROTOCOL_PREFIX}Synchronization ref {} changed from {:?} to {:?}",
                    updates[mismatch.index].ref_name, updates[mismatch.index].expected_old, mismatch.current)).into());
            }
            let details = AuthorityCutStaleDetails {
                workspace_id: target.workspace_id().to_string(),
                target_kind: target.kind().as_str().to_string(),
                target_id: target.target_id().to_string(),
                current_cut: members
                    .iter()
                    .map(|member| AuthorityCutMember {
                        repo_id: member.repo_id.to_string(),
                        workspace_id: member.workspace_id.to_string(),
                        branch_name: member.branch_name.clone(),
                        current_head: repo_storage
                            .repo_path(org_id, member.repo_id)
                            .ok()
                            .and_then(|repo_path| GitRepo::open(&repo_path).ok())
                            .and_then(|repo| repo.read_branch_oid(&member.branch_name).ok())
                            .flatten()
                            .unwrap_or_default(),
                    })
                    .collect(),
                mismatches: mismatches
                    .iter()
                    .map(|mismatch| {
                        let member = changed[mismatch.index];
                        tracing::warn!(
                            repo_id = %member.repo_id,
                            branch = %member.branch_name,
                            current = mismatch.current.as_deref().unwrap_or("<absent>"),
                            expected = %member.expected_old_head,
                            "Branch ref is authoritative and the cut's expectation is stale"
                        );
                        AuthorityCutMismatch {
                            repo_id: member.repo_id.to_string(),
                            expected_old: member.expected_old_head.clone(),
                            current_head: mismatch.current.clone().unwrap_or_default(),
                        }
                    })
                    .collect(),
            };
            let encoded = serde_json::to_string(&details).map_err(|error| {
                protocol_error(format!(
                    "failed to encode refused branch ref update: {error}"
                ))
            })?;
            Err(protocol_error(format!("{AUTHORITY_CUT_STALE_PROTOCOL_PREFIX}{encoded}")).into())
        }
        Err(BranchRefCutRefusal::Failed {
            index,
            error,
            written,
        }) => {
            let mut message = match index {
                Some(index) if index < members.len() => {
                    let member = &members[index];
                    format!(
                        "Branch ref refs/heads/{} of repository {} was not updated to {}: {error}",
                        member.branch_name, member.repo_id, member.proposed_new_head
                    )
                }
                Some(index) => {
                    let update = &updates[index];
                    format!(
                        "Git ref {} in {} was not updated to {:?}: {error}",
                        update.ref_name,
                        update.repo_path.display(),
                        update.new_oid
                    )
                }
                None => format!("Branch refs were not updated: {error}"),
            };
            if !written.is_empty() {
                // The landing still holds the fence: put the partial cut back
                // before any reader can capture it (`g5.git.landing.commit-point`).
                let written_refs = written
                    .iter()
                    .map(|index| updates[*index].clone())
                    .collect::<Vec<_>>();
                let unresolved = restore_applied_branch_refs(&written_refs);
                if unresolved.is_empty() {
                    message.push_str("; the refs git had already written were restored");
                } else {
                    tracing::error!(
                        unresolved = ?unresolved,
                        "A partial branch-ref cut could not be fully restored"
                    );
                    message.push_str(&format!(
                        "; these refs git had already written could not be restored: {}",
                        unresolved.join(", ")
                    ));
                }
            }
            Err(protocol_error(format!("{BRANCH_REF_REFUSED_PROTOCOL_PREFIX}{message}")).into())
        }
    }
}

async fn configure_landing_transaction(conn: &mut PgConnection) -> Result<(), sqlx::Error> {
    sqlx::query("SET LOCAL statement_timeout = '15s'")
        .execute(&mut *conn)
        .await?;
    sqlx::query("SET LOCAL idle_in_transaction_session_timeout = '15s'")
        .execute(&mut *conn)
        .await?;
    Ok(())
}

/// Queue the remote handoff in the same database transaction as publication.
/// The worker cannot see the queue row until Git has accepted the complete cut.
async fn enqueue_remote_work_during_landing(
    conn: &mut PgConnection,
    advance_id: Uuid,
    members: &[BranchCommitMember],
    remote_tracking_completion: Option<crate::models::branch_integration_queue::PushAuthority>,
) -> Result<(), sqlx::Error> {
    let trigger = if remote_tracking_completion.is_some() {
        crate::models::branch_integration_queue::trigger_kind::PUSH
    } else {
        crate::models::branch_integration_queue::trigger_kind::COMMIT
    };
    for member in members.iter().filter(|member| member.changes()) {
        enqueue_remote_tracking_integration_for_advance(
            conn,
            member.workspace_id,
            member.repo_id,
            advance_id,
            trigger,
            remote_tracking_completion,
        )
        .await?;
    }
    Ok(())
}

fn is_git_sha(value: &str) -> bool {
    value.len() == 40
        && value.as_bytes().iter().any(|byte| *byte != b'0')
        && value
            .as_bytes()
            .iter()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(byte))
}

fn protocol_error(message: impl Into<String>) -> sqlx::Error {
    sqlx::Error::Protocol(message.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::workspace_reader::AuthorityInventoryRepo;

    #[test]
    fn auxiliary_ref_failures_refuse_the_whole_cut_without_panicking() {
        let temp = tempfile::tempdir().unwrap();
        let org_id = Uuid::new_v4();
        let storage = TenantRepoStorage::local_for_test(temp.path().join("repos"))
            .scoped(&crate::services::git::repo_store::TestOrgScope(org_id));
        let repo_id = Uuid::new_v4();
        let workspace_id = Uuid::new_v4();
        let path = storage.repo_path(org_id, repo_id).unwrap();
        let repo = git2::Repository::init_bare(&path).unwrap();
        let tree = repo
            .find_tree(repo.treebuilder(None).unwrap().write().unwrap())
            .unwrap();
        let signature = git2::Signature::now("Test", "test@example.com").unwrap();
        let old = repo
            .commit(
                Some("refs/heads/main"),
                &signature,
                &signature,
                "initial",
                &tree,
                &[],
            )
            .unwrap();
        let new = repo
            .commit(
                None,
                &signature,
                &signature,
                "next",
                &tree,
                &[&repo.find_commit(old).unwrap()],
            )
            .unwrap();
        let member = BranchCommitMember {
            repo_id,
            workspace_id,
            branch_name: "main".into(),
            expected_old_head: old.to_string(),
            proposed_new_head: new.to_string(),
            transition: WorkspaceHeadTransitionKind::Advance,
        };
        let auxiliary_ref = "refs/test/auxiliary";
        for lock_failure in [false, true] {
            if lock_failure {
                std::fs::create_dir_all(path.join("refs/test")).unwrap();
                std::fs::write(
                    path.join("refs/test/auxiliary.lock"),
                    "held by another writer",
                )
                .unwrap();
            }
            let result = apply_member_ref_updates(
                &AuthorityTarget::Workspace { workspace_id },
                std::slice::from_ref(&member),
                &[RefCommit {
                    repo_path: path.clone(),
                    ref_name: auxiliary_ref.into(),
                    expected_old: None,
                    new_oid: Some(if lock_failure {
                        new.to_string()
                    } else {
                        "a".repeat(40)
                    }),
                }],
                &storage,
                org_id,
            );
            let error = result.unwrap_err();
            assert!(is_branch_ref_refusal(&error), "{error:?}");
            assert!(error.to_string().contains(auxiliary_ref), "{error}");
            assert_eq!(repo.refname_to_id("refs/heads/main").unwrap(), old);
            assert!(repo.find_reference(auxiliary_ref).is_err());
        }
    }

    #[test]
    fn stale_and_refused_protocol_errors_decode_to_the_route_contract() {
        let stale = protocol_error(format!(
            "{AUTHORITY_CUT_STALE_PROTOCOL_PREFIX}{{\"workspaceId\":\"w\",\"targetKind\":\"workspace\",\"targetId\":\"w\",\"currentCut\":[],\"mismatches\":[]}}"
        ));
        assert!(matches!(
            map_branch_commit_error(stale),
            G5Error::AuthorityCutStale { .. }
        ));
        let refused = protocol_error(format!("{BRANCH_REF_REFUSED_PROTOCOL_PREFIX}x"));
        assert!(matches!(
            map_branch_commit_error(refused),
            G5Error::GitError { message } if message == "x"
        ));
        let other = protocol_error("plain conflict");
        assert!(matches!(
            map_branch_commit_error(other),
            G5Error::Conflict { reason } if reason == "plain conflict"
        ));
    }

    #[test]
    fn repo_inputs_must_name_exact_shas_once() {
        let repo_id = Uuid::new_v4();
        let ok = vec![WorkspaceHeadRepoInput {
            repo_id,
            expected_old_head: ZERO_GIT_SHA.to_string(),
            proposed_new_head: "a".repeat(40),
        }];
        assert!(validate_repo_inputs(&ok).is_ok());
        let duplicate = vec![ok[0].clone(), ok[0].clone()];
        assert!(validate_repo_inputs(&duplicate).is_err());
        let zero_proposed = vec![WorkspaceHeadRepoInput {
            repo_id,
            expected_old_head: "a".repeat(40),
            proposed_new_head: ZERO_GIT_SHA.to_string(),
        }];
        assert!(validate_repo_inputs(&zero_proposed).is_err());
    }

    fn inventory_repo(
        repo_id: Uuid,
        branch_name: &str,
        _old_cache: Option<&str>,
        _old_status: &str,
    ) -> AuthorityInventoryRepo {
        AuthorityInventoryRepo {
            workspace_id: Uuid::nil(),
            repo_id,
            branch_name: branch_name.to_string(),
            is_base: true,
            may_be_unborn: true,
        }
    }

    fn workspace_inventory(
        workspace_id: Uuid,
        repos: Vec<AuthorityInventoryRepo>,
    ) -> AuthorityInventorySnapshot {
        AuthorityInventorySnapshot {
            target: AuthorityTarget::Workspace { workspace_id },
            org_id: Uuid::new_v4(),
            workspace_id,
            repos: repos
                .into_iter()
                .map(|mut repo| {
                    repo.workspace_id = workspace_id;
                    repo
                })
                .collect(),
        }
    }

    /// The branch refs of the inventory members, in inventory order; `None`
    /// for a member without a ref.
    fn branch_refs(
        inventory: &AuthorityInventorySnapshot,
        refs: &[Option<&str>],
    ) -> BTreeMap<(Uuid, String), Option<String>> {
        inventory
            .repos
            .iter()
            .zip(refs)
            .map(|(row, head)| {
                (
                    (row.repo_id, row.branch_name.clone()),
                    head.map(str::to_string),
                )
            })
            .collect()
    }

    /// A route that tolerates a refused initial cut must not swallow a stale
    /// one: the two decode differently.
    #[test]
    fn a_refused_branch_ref_update_decodes_to_a_git_error_not_a_stale_cut() {
        let refused = protocol_error(format!(
            "{BRANCH_REF_REFUSED_PROTOCOL_PREFIX}Branch ref refs/heads/feat/x of repository r was not updated to b: namespace conflict"
        ));
        assert!(is_branch_ref_refusal(&BranchCommitError::Database(
            protocol_error(format!("{BRANCH_REF_REFUSED_PROTOCOL_PREFIX}x"))
        )));
        assert!(!is_branch_ref_refusal(&BranchCommitError::Database(
            protocol_error(format!("{AUTHORITY_CUT_STALE_PROTOCOL_PREFIX}{{}}"))
        )));
        match map_branch_commit_error(refused) {
            G5Error::GitError { message } => {
                assert!(
                    message.starts_with("Branch ref refs/heads/feat/x"),
                    "{message}"
                );
            }
            other => panic!("a refused ref update is a git error: {other:?}"),
        }
    }

    /// A landing consumer's retryable database failure keeps its typed
    /// variant through the landing, so callers still apply their retry budget.
    #[test]
    fn retryable_database_errors_survive_the_landing() {
        let retryable = AppError::from_error(G5Error::DatabaseRetryable {
            message: "Database error: error returned from database: deadlock detected".to_string(),
        });
        match AppError::from(BranchCommitError::App(retryable)).inner {
            G5Error::DatabaseRetryable { message } => {
                assert!(message.contains("deadlock detected"));
            }
            other => panic!("retryable victim must stay DatabaseRetryable: {other:?}"),
        }
    }

    #[test]
    fn repo_inputs_require_lowercase_git_shas() {
        let valid = WorkspaceHeadRepoInput {
            repo_id: Uuid::new_v4(),
            expected_old_head: "a".repeat(40),
            proposed_new_head: "b".repeat(40),
        };
        assert!(validate_repo_inputs(std::slice::from_ref(&valid)).is_ok());
        assert!(validate_repo_inputs(&[WorkspaceHeadRepoInput {
            expected_old_head: "A".repeat(40),
            ..valid.clone()
        }])
        .is_err());
        assert!(validate_repo_inputs(&[WorkspaceHeadRepoInput {
            proposed_new_head: "B".repeat(40),
            ..valid
        }])
        .is_err());
    }

    #[test]
    fn complete_cut_validation_rejects_missing_or_stale_repositories() {
        let workspace_id = Uuid::new_v4();
        let first = Uuid::new_v4();
        let second = Uuid::new_v4();
        let (a, b, c, d) = (
            "a".repeat(40),
            "b".repeat(40),
            "c".repeat(40),
            "d".repeat(40),
        );
        let inventory = workspace_inventory(
            workspace_id,
            vec![
                inventory_repo(first, "main", Some(&a), "materialized"),
                inventory_repo(second, "main", Some(&b), "materialized"),
            ],
        );
        let refs = branch_refs(&inventory, &[Some(&a), Some(&b)]);
        let build = |inputs: &[WorkspaceHeadRepoInput]| {
            build_members(
                &inventory.target,
                &inventory,
                &refs,
                inputs,
                WorkspaceHeadTransitionKind::Advance,
                &BTreeMap::new(),
            )
        };
        let complete = vec![
            WorkspaceHeadRepoInput {
                repo_id: first,
                expected_old_head: a.clone(),
                proposed_new_head: c,
            },
            WorkspaceHeadRepoInput {
                repo_id: second,
                expected_old_head: b.clone(),
                proposed_new_head: b,
            },
        ];
        assert!(build(&complete).is_ok());
        let missing = build(&complete[..1])
            .expect_err("a cut that omits a materialized member must be refused");
        assert!(
            missing.to_string().contains(REGISTERED_TRANSITION_MISMATCH),
            "{missing}"
        );
        let mut stale = complete;
        stale[0].expected_old_head = d;
        assert!(build(&stale).is_err());
    }

    #[test]
    fn stale_cut_error_carries_the_complete_current_vector_and_mismatches() {
        let workspace_id = Uuid::new_v4();
        let first = Uuid::new_v4();
        let second = Uuid::new_v4();
        let (a, b, c) = ("a".repeat(40), "b".repeat(40), "c".repeat(40));
        let inventory = workspace_inventory(
            workspace_id,
            vec![
                inventory_repo(first, "main", Some(&a), "materialized"),
                inventory_repo(second, "docs", Some(&b), "materialized"),
            ],
        );
        let refs = branch_refs(&inventory, &[Some(&a), Some(&b)]);
        let inputs = vec![
            WorkspaceHeadRepoInput {
                repo_id: first,
                expected_old_head: c.clone(),
                proposed_new_head: "d".repeat(40),
            },
            WorkspaceHeadRepoInput {
                repo_id: second,
                expected_old_head: b,
                proposed_new_head: "e".repeat(40),
            },
        ];
        let protocol = build_members(
            &inventory.target,
            &inventory,
            &refs,
            &inputs,
            WorkspaceHeadTransitionKind::Advance,
            &BTreeMap::new(),
        )
        .expect_err("one stale member must reject the complete cut");
        let G5Error::AuthorityCutStale { details } = map_branch_commit_app_error(protocol).inner
        else {
            panic!("stale complete-cut conflict must retain its typed details")
        };
        assert_eq!(details.workspace_id, workspace_id.to_string());
        assert_eq!(details.target_kind, "workspace");
        assert_eq!(details.target_id, workspace_id.to_string());
        assert_eq!(details.current_cut.len(), 2);
        assert_eq!(details.mismatches.len(), 1);
        assert_eq!(details.mismatches[0].repo_id, first.to_string());
        assert_eq!(details.mismatches[0].expected_old, c);
        assert_eq!(details.mismatches[0].current_head, a);
        for (index, row) in inventory.repos.iter().enumerate() {
            assert_eq!(details.current_cut[index].repo_id, row.repo_id.to_string());
            assert_eq!(
                details.current_cut[index].workspace_id,
                workspace_id.to_string()
            );
        }
    }

    #[test]
    fn workspace_cut_types_an_exact_unborn_member_as_initialization() {
        let workspace_id = Uuid::new_v4();
        let initializing = Uuid::new_v4();
        let materialized = Uuid::new_v4();
        let pending = Uuid::new_v4();
        let a = "a".repeat(40);
        let inventory = workspace_inventory(
            workspace_id,
            vec![
                inventory_repo(initializing, "workspace/canonical", None, "unborn"),
                inventory_repo(
                    materialized,
                    "workspace/canonical",
                    Some(&a),
                    "materialized",
                ),
                inventory_repo(pending, "workspace/canonical", None, "unborn"),
            ],
        );
        let refs = branch_refs(&inventory, &[None, Some(&a), None]);
        let inputs = vec![
            WorkspaceHeadRepoInput {
                repo_id: initializing,
                expected_old_head: ZERO_GIT_SHA.to_string(),
                proposed_new_head: "b".repeat(40),
            },
            WorkspaceHeadRepoInput {
                repo_id: materialized,
                expected_old_head: a.clone(),
                proposed_new_head: a,
            },
        ];
        let members = build_members(
            &inventory.target,
            &inventory,
            &refs,
            &inputs,
            WorkspaceHeadTransitionKind::Advance,
            &BTreeMap::new(),
        )
        .expect("an exactly unborn member initializes beside its materialized sibling");
        let transition = |repo_id: Uuid| {
            members
                .iter()
                .find(|member| member.repo_id == repo_id)
                .map(|member| member.transition)
        };
        assert_eq!(
            transition(initializing),
            Some(WorkspaceHeadTransitionKind::Initialize)
        );
        assert_eq!(
            transition(materialized),
            Some(WorkspaceHeadTransitionKind::Advance)
        );
        assert_eq!(
            transition(pending),
            None,
            "an unborn member outside the cut stays outside it"
        );
    }

    /// The other half of the initialization rule: a task cut that names
    /// an exactly unborn member is refused, and so is its replay onto a ref
    /// that already carries the proposed commit, while the same target still
    /// builds a cut that initializes nothing.
    #[test]
    fn task_cuts_refuse_to_initialize_an_unborn_member() {
        let workspace_id = Uuid::new_v4();
        let initializing = Uuid::new_v4();
        let materialized = Uuid::new_v4();
        let pending = Uuid::new_v4();
        let a = "a".repeat(40);
        let b = "b".repeat(40);
        let target = AuthorityTarget::Task {
            workspace_id,
            task_id: Uuid::new_v4(),
        };
        let kind = target.kind().as_str();
        let inventory = AuthorityInventorySnapshot {
            target: target.clone(),
            ..workspace_inventory(
                workspace_id,
                vec![
                    inventory_repo(initializing, "scope/branch", None, "unborn"),
                    inventory_repo(materialized, "scope/branch", Some(&a), "materialized"),
                    inventory_repo(pending, "scope/branch", None, "unborn"),
                ],
            )
        };
        let initialize = WorkspaceHeadRepoInput {
            repo_id: initializing,
            expected_old_head: ZERO_GIT_SHA.to_string(),
            proposed_new_head: b.clone(),
        };
        let unchanged = WorkspaceHeadRepoInput {
            repo_id: materialized,
            expected_old_head: a.clone(),
            proposed_new_head: a.clone(),
        };
        let build = |refs: &[Option<&str>], inputs: &[WorkspaceHeadRepoInput]| {
            build_members(
                &target,
                &inventory,
                &branch_refs(&inventory, refs),
                inputs,
                WorkspaceHeadTransitionKind::Advance,
                &BTreeMap::new(),
            )
        };

        let first = build(
            &[None, Some(&a), None],
            &[initialize.clone(), unchanged.clone()],
        )
        .expect_err("a task cut must not initialize an unborn member")
        .to_string();
        assert!(
            first.contains(&format!(
                "Repository {initializing} cannot be initialized by a {kind} cut"
            )) && first.contains("valid only for workspace authority"),
            "{first}"
        );
        let replay = build(
            &[Some(&b), Some(&a), None],
            &[initialize.clone(), unchanged.clone()],
        )
        .expect_err("a task cut must not replay an initialization either")
        .to_string();
        assert!(
            replay.contains("valid only for workspace authority"),
            "{replay}"
        );

        let members = build(&[None, Some(&a), None], std::slice::from_ref(&unchanged))
            .expect("the same target still builds a cut that initializes nothing");
        assert_eq!(
            members
                .iter()
                .map(|member| (member.repo_id, member.transition))
                .collect::<Vec<_>>(),
            vec![(materialized, WorkspaceHeadTransitionKind::Advance)]
        );
    }

    #[test]
    fn landed_read_view_selects_the_exact_member() {
        let workspace_id = Uuid::new_v4();
        let selected_repo_id = Uuid::new_v4();
        let selected_head = "a".repeat(40);
        let member = |repo_id: Uuid, expected: String, proposed: String| BranchCommitMember {
            repo_id,
            workspace_id,
            branch_name: "main".to_string(),
            expected_old_head: expected,
            proposed_new_head: proposed,
            transition: WorkspaceHeadTransitionKind::Advance,
        };
        let org_id = Uuid::new_v4();
        let landed = LandedBranchCommit {
            advance_id: Uuid::new_v4(),
            org_id,
            workspace_id,
            target: AuthorityTarget::Workspace { workspace_id },
            members: vec![
                member(Uuid::new_v4(), "b".repeat(40), "c".repeat(40)),
                member(selected_repo_id, "d".repeat(40), selected_head.clone()),
            ],
            committed_at: chrono::Utc::now(),
            projection_warnings: Vec::new(),
        };

        let view = landed
            .read_view(
                &crate::services::git::repo_store::TestOrgScope(org_id),
                selected_repo_id,
            )
            .expect("select the landed member");

        assert_eq!(view.repo_id(), selected_repo_id);
        assert_eq!(view.head_commit(), selected_head);
    }
}
