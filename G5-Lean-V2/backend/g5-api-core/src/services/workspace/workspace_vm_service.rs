//! Durable workspace VM lifecycle.
//!
//! `workspace_vms` is the authority. Host commands and dispatcher RPCs are
//! effects of exact `(workspace_id, vm_id, epoch, operation)` records and may
//! therefore be retried after an ambiguous reply without creating a second
//! incarnation or teardown.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, TimeDelta, Utc};
use g5_auth::JwtMinter;
use g5_authz::{ScopedDb, ScopedDbReopener};
use g5_contracts::error::{G5Error, OwnerLimitKind};
use g5_contracts::types::worker_host::validate_base_image_ref;
use g5_contracts::types::ApprovalReviewIdentity;
use g5_contracts::GitAuthor;
use g5_internal_api::host_ctl::{
    method, workspace_hostname, CommandParams, CommandResult, SlotUnavailableReason, StopMode,
    VmImageSpec, VmIncarnationSpec, VmNetwork, VmReferenceRepoSpec, VmRepoSpec, VmRestoreRepoSpec,
    VmRestoreSpec, VmSequencer, VmSizeSpec, VmSpec, VmStart, VmStatus, VmStop, WorkspaceRootSpec,
    DISPATCHER_CONTRACT_VERSION, MODULE_CATALOG, MODULE_CATALOG_HASH, MODULE_CATALOG_VERSION,
    VM_SPEC_VERSION, WORKSPACE_HOSTNAME_ENV,
};
use g5_internal_api::workspace_flush::{FlushCompleteResponse, FlushRepoReceipt, SequencerState};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use sqlx::{PgConnection, PgPool, Postgres, Row, Transaction};
use uuid::Uuid;

use crate::services::git::repo_store::{ScopedRepoStorage, TenantRepoStorage};

use crate::errors::AppError;
use crate::services::worker_host_service::{
    PlacementBootRecord, PlacementOutcome, PlacementRequest, PlacementReservation,
};
use crate::services::workspace::workspace_owner_limit;
use crate::services::workspace_runtime::{
    canonical_exec_id, RuntimeRequest, RuntimeRpcError, WorkspaceRuntime,
};
use crate::services::workspace_vm_release::AgentWorkSettlement;
use crate::services::WorkerHostService;
use crate::ws::preview_tunnel::TunnelRegistry;
use tokio_util::sync::CancellationToken;

const START_INTENT_TTL_MINUTES: i64 = 15;
/// Capacity wait of an automatic continuation before its turns wait for the
/// owner again (`g5.lifecycle.start.automatic-continuation`).
const AUTOMATIC_CONTINUATION_TTL_MINUTES: i64 = 60;
/// Loss-caused automatic continuations a workspace gets between owner Starts.
const AUTOMATIC_CONTINUATION_LOSS_CAP: i64 = 2;
/// The refusal of an automatic start whose interrupted turns are gone: the
/// owner ended them between the verdict and the consent.
const AUTOMATIC_CONTINUATION_NO_WORK: &str =
    "automatic continuation has no interrupted turn left to run";
const START_ACQUISITION_SECS: i64 = 120;
/// How long a Start may wait for room on the host connection's outbound queue
/// while its row lock is held. The queue is bounded and drained by the control
/// stream, so this bounds the lock, not the host's reply.
const START_ENQUEUE_SECS: u64 = 5;
/// How long one Start attempt waits for the host's reply.
const START_REPLY_SECS: u64 = 125;
const LEASE_TTL_SECS: u64 = 300;
const INCARNATION_TOKEN_TTL_SECS: i64 = 24 * 60 * 60;
/// How long an owner, idle, transfer or ordinary Delete stop may spend saving
/// before it gives up and kills the VM (`g5.lifecycle.stop.window`).
const STOP_SAVE_WINDOW_SECS: i64 = 2 * 60;
/// The save window of a deadline stop once its owner nudge has run out.
const DEADLINE_STOP_SAVE_WINDOW_SECS: i64 = 10 * 60;
const DEFAULT_DEADLINE_STOP_SECS: i64 = 30 * 60;
/// Graceful shutdown after a complete save; nothing required is left in the
/// guest by then (`g5.lifecycle.stop.release`).
const CLEAN_STOP_GRACE_SECS: u64 = 30;
/// Time the host gets, beyond the grace, to end and reap the VM process.
const HOST_STOP_REAP_SECS: u64 = 30;
/// An admitted stop is claimed by the replica executing it: `stop_retry_at`
/// stays this far ahead while the executor lives, renewed every
/// `STOP_CLAIM_RENEW_SECS`. Each renewal replaces exactly the value the
/// executor wrote last, so a yield or another executor's claim ends the
/// renewals and, through `StopClaim::lost`, the execution. An executor that
/// dies (API crash, restart, panic) stops renewing, and the lifecycle
/// reconciler on any replica resumes the operation once the claim lapses.
const STOP_CLAIM_LEASE_SECS: i64 = 30;
const STOP_CLAIM_RENEW_SECS: u64 = 10;
/// Retention sent in `VmSpec.workspace_root`. Current hosts delete a root when
/// its VM is released and ignore the field; hosts from before that change
/// require it and keep the root this long, as they always did.
const LEGACY_HOST_RETAIN_HOURS: u32 = 24;
const DEFAULT_IDLE_RETRY_SECS: i64 = 5 * 60;
const IDLE_EXEMPTION_DIAGNOSTIC_INTERVAL: Duration = Duration::from_secs(60);

/// Owned by the lifecycle loop per organization, across service reconstruction.
/// Only diagnostic pagination/cadence lives here; Stop eligibility stays in SQL.
#[derive(Default)]
pub struct IdleExemptionDiagnostics {
    after_workspace: Option<Uuid>,
    next_scan_at: Option<tokio::time::Instant>,
}

impl IdleExemptionDiagnostics {
    fn begin_scan(&mut self, now: tokio::time::Instant) -> bool {
        if self.next_scan_at.is_some_and(|next| now < next) {
            return false;
        }
        self.next_scan_at = Some(now + IDLE_EXEMPTION_DIAGNOSTIC_INTERVAL);
        true
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StartKind {
    Resume,
    CompoundStart,
    /// The platform's own scoped consent after it ended compute while a chat
    /// turn was active (`g5.lifecycle.start.automatic-continuation`).
    AutomaticContinuation,
}

impl StartKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Resume => "resume",
            Self::CompoundStart => "compound_start",
            Self::AutomaticContinuation => "automatic_continuation",
        }
    }

    /// How long the consent waits for capacity. Nobody watches an automatic
    /// continuation, so it may wait longer than a person at the Start button.
    fn consent_ttl(self) -> TimeDelta {
        match self {
            Self::Resume | Self::CompoundStart => TimeDelta::minutes(START_INTENT_TTL_MINUTES),
            Self::AutomaticContinuation => TimeDelta::minutes(AUTOMATIC_CONTINUATION_TTL_MINUTES),
        }
    }

    /// Whether the owner gave this consent themselves. An owner's Start resets
    /// the loss-caused automatic continuation count.
    fn is_owner_consent(self) -> bool {
        !matches!(self, Self::AutomaticContinuation)
    }

    fn parse(value: &str) -> Result<Self, G5Error> {
        match value {
            "resume" => Ok(Self::Resume),
            "compound_start" => Ok(Self::CompoundStart),
            "automatic_continuation" | "rollout_continuation" => Ok(Self::AutomaticContinuation),
            _ => Err(internal("workspace start intent kind is invalid")),
        }
    }
}

/// How a Start request relates to the workspace's recorded start intent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum IntentAdmission {
    /// A new explicit Start; it may replace a settled intent.
    Fresh,
    /// Reconciliation retrying the exact intent it captured.
    ReplayOnly,
    /// A compound launch that owns at most the first intent: refused once
    /// another intent has replaced it.
    FirstOnly,
}

/// The Start intent of a fork's launch, derived from the chat the fork queued
/// so every replay of the fork names the same consent.
#[must_use]
pub fn fork_launch_intent_id(started_session_id: Uuid) -> Uuid {
    const NAMESPACE: Uuid = Uuid::from_u128(0x6f1e_7a2c_95d4_4c1b_8e3a_0b5d_9c27_f410);
    Uuid::new_v5(&NAMESPACE, started_session_id.as_bytes())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StopReason {
    User,
    Transfer,
    MemberDeactivated,
    /// Cleanup under a stop-by-user execution hold on an active membership
    /// (`g5.lifecycle.stop.by-user`).
    ExecutionHold,
    Idle,
    Drain,
    Rollout,
    Credential,
    Capacity,
    Delete,
    /// The owner left the organization or became a viewer, retiring their
    /// personal workspace (`g5.personal.retirement`).
    OwnerRemoved,
}

impl StopReason {
    fn as_str(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Transfer => "transfer",
            Self::MemberDeactivated => "member_deactivated",
            Self::ExecutionHold => "execution_hold",
            Self::Idle => "idle",
            Self::Drain => "drain",
            Self::Rollout => "rollout",
            Self::Credential => "credential",
            Self::Capacity => "capacity",
            Self::Delete => "delete",
            Self::OwnerRemoved => "owner_removed",
        }
    }

    /// Platform deadline stops warn the owner and keep the VM running until
    /// their nudge deadline before they start stopping.
    fn nudges_before_stopping(self) -> bool {
        matches!(
            self,
            Self::Drain | Self::Rollout | Self::Credential | Self::Capacity | Self::Delete
        )
    }

    /// Whether the turns this stop interrupts continue automatically once the
    /// workspace is placed again: only host operations do
    /// (`g5.lifecycle.start.automatic-continuation`).
    fn continues_automatically(self) -> bool {
        matches!(self, Self::Drain | Self::Rollout)
    }

    /// The save window a stop gets when its caller sets no deadline.
    fn default_save_window(self) -> TimeDelta {
        if self.nudges_before_stopping() {
            TimeDelta::seconds(DEADLINE_STOP_SAVE_WINDOW_SECS)
        } else {
            TimeDelta::seconds(STOP_SAVE_WINDOW_SECS)
        }
    }

    fn parse(value: &str) -> Result<Self, G5Error> {
        match value {
            "user" => Ok(Self::User),
            "transfer" => Ok(Self::Transfer),
            "member_deactivated" => Ok(Self::MemberDeactivated),
            "execution_hold" => Ok(Self::ExecutionHold),
            "idle" => Ok(Self::Idle),
            "drain" => Ok(Self::Drain),
            "rollout" => Ok(Self::Rollout),
            "credential" => Ok(Self::Credential),
            "capacity" => Ok(Self::Capacity),
            "delete" => Ok(Self::Delete),
            "owner_removed" => Ok(Self::OwnerRemoved),
            _ => Err(internal("workspace_vms.stop_reason is invalid")),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceVmView {
    pub org_id: Uuid,
    pub workspace_id: Uuid,
    pub owner_user_id: Uuid,
    pub status: String,
    pub host_id: Option<Uuid>,
    pub vm_id: Option<Uuid>,
    pub epoch: u64,
    pub image_ref: Option<String>,
    pub module_catalog_version: Option<String>,
    pub start_intent: Option<Value>,
    pub start_deadline_at: Option<DateTime<Utc>>,
    pub started_at: Option<DateTime<Utc>>,
    pub last_activity_at: Option<DateTime<Utc>>,
    pub stop_reason: Option<String>,
    pub stop_operation_id: Option<Uuid>,
    pub stop_deadline_at: Option<DateTime<Utc>>,
    pub pending_stop_reason: Option<String>,
    pub pending_stop_deadline_at: Option<DateTime<Utc>>,
    pub nudged_at: Option<DateTime<Utc>>,
    pub last_flush_id: Option<Uuid>,
    pub unflushed_loss_at: Option<DateTime<Utc>>,
    pub last_error: Option<Value>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum ResumeOutcome {
    Attached { vm: WorkspaceVmView },
    Starting { vm: WorkspaceVmView },
    QueuedForCapacity { vm: WorkspaceVmView },
}

/// The outcome of a formal review's readiness pass on its review workspace.
#[derive(Debug)]
pub(crate) enum AgentReviewStart {
    /// Compute is attached, starting or queued for capacity.
    Resumed,
    /// The review's Start consent ended before the review was sent; replays
    /// cannot revive it (`g5.lifecycle.start`). Carries a readable cause.
    Ended(&'static str),
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StopOutcome {
    pub operation_id: Uuid,
    pub status: String,
    pub flush_id: Option<Uuid>,
    pub unflushed_loss: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TransferOutcome {
    pub transfer_id: Uuid,
    pub workspace_id: Uuid,
    pub from_user_id: Uuid,
    pub to_user_id: Uuid,
    pub requested_by: Uuid,
    pub status: String,
    pub completed_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone)]
struct WorkspaceBootInput {
    org_id: Uuid,
    workspace_id: Uuid,
    owner_user_id: Uuid,
    /// The Start consent admission selected under the VM row lock: the one
    /// this call recorded, replayed or joined. Work bound to the Start binds
    /// to this id, never to one read from the row afterwards.
    consent_id: Uuid,
    /// Guest hostname: the workspace name as a hostname label.
    hostname: String,
    owner_git_name: String,
    owner_git_email: String,
    lifecycle_generation: u64,
    image_ref: String,
    image_digest: String,
    image_source: Value,
    image_oid_vector: Value,
    idle_timeout_secs: u64,
    region: String,
    repos: Vec<VmRepoSpec>,
    restore: Option<VmRestoreSpec>,
    reference_repos: Vec<VmReferenceRepoSpec>,
}

#[derive(Debug, Clone)]
struct CurrentStop {
    workspace_id: Uuid,
    host_id: Uuid,
    vm_id: Uuid,
    epoch: u64,
    reason: StopReason,
    requires_flush: bool,
    operation_id: Uuid,
    deadline_at: DateTime<Utc>,
    /// The claim admission wrote for this executor; renewals replace it.
    claimed_until: DateTime<Utc>,
    /// When this executor asked for that claim, before the database stamped
    /// it: the executor's own measure of the lease starts here, so it never
    /// runs later than the database's.
    claimed_at: tokio::time::Instant,
}

/// Every stop reports its stages, so an operator can see where an owner's
/// Stop stands (admitted, saving, host command, released) and which claim
/// renewals or yields preceded a reconciler resumption.
fn log_stop_phase(current: &CurrentStop, phase: &'static str) {
    tracing::info!(
        workspace_id = %current.workspace_id,
        operation_id = %current.operation_id,
        vm_id = %current.vm_id,
        reason = current.reason.as_str(),
        phase,
        "workspace stop phase"
    );
}

/// The executing replica's claim on an admitted stop. Admission writes
/// `stop_retry_at` `STOP_CLAIM_LEASE_SECS` ahead; this replaces that exact
/// value every `STOP_CLAIM_RENEW_SECS`. A renewal that finds another value
/// (a yield, a release, or a claim taken by another executor after this one
/// lapsed), or that has not succeeded by the time the lease ends because the
/// database failed or never answered, marks the claim lost, and the
/// execution holding it stops. Dropping the claim
/// (including when the executing task panics) stops the renewals.
struct StopClaim {
    renewer: tokio::task::JoinHandle<()>,
    lost: CancellationToken,
}

impl StopClaim {
    fn hold(service: WorkspaceVmService, current: CurrentStop) -> Self {
        let lost = CancellationToken::new();
        let signal = lost.clone();
        let renewer = tokio::spawn(async move {
            let mut ticks = tokio::time::interval(Duration::from_secs(STOP_CLAIM_RENEW_SECS));
            ticks.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
            ticks.tick().await;
            let mut expected = current.claimed_until;
            // The lease as this executor measures it: from the moment it asked
            // the database for the claim, so it ends no later than the stamped
            // one. A renewal that has not succeeded by then, whether it failed
            // or is still waiting on the database, means the claim is lost;
            // the executor must not work past the lease on the strength of a
            // reply that never came.
            let lease = Duration::from_secs(STOP_CLAIM_LEASE_SECS.unsigned_abs());
            let mut lease_ends = current.claimed_at + lease;
            loop {
                // A slow admission (a long row-lock wait) can leave less than
                // one renewal interval of lease: the wait for the next
                // renewal is bounded by the lease too.
                if tokio::time::timeout_at(lease_ends, ticks.tick())
                    .await
                    .is_err()
                {
                    log_stop_phase(&current, "claim_lost");
                    signal.cancel();
                    break;
                }
                let asked_at = tokio::time::Instant::now();
                let renewal = tokio::time::timeout_at(
                    lease_ends,
                    service.renew_stop_claim(&current, expected),
                )
                .await;
                match renewal {
                    Ok(Ok(Some(next))) => {
                        expected = next;
                        lease_ends = asked_at + lease;
                        log_stop_phase(&current, "claim_renewed");
                    }
                    Ok(Ok(None)) => {
                        log_stop_phase(&current, "claim_lost");
                        signal.cancel();
                        break;
                    }
                    Ok(Err(error)) => {
                        tracing::warn!(
                            workspace_id = %current.workspace_id,
                            operation_id = %current.operation_id,
                            %error,
                            "workspace stop claim renewal failed"
                        );
                        // The next attempt would come after the lease ends.
                        if tokio::time::Instant::now() + Duration::from_secs(STOP_CLAIM_RENEW_SECS)
                            >= lease_ends
                        {
                            log_stop_phase(&current, "claim_lost");
                            signal.cancel();
                            break;
                        }
                    }
                    Err(_lease_ended) => {
                        log_stop_phase(&current, "claim_lost");
                        signal.cancel();
                        break;
                    }
                }
            }
        });
        Self { renewer, lost }
    }

    /// Resolves once this executor no longer holds the claim.
    fn lost(&self) -> tokio_util::sync::WaitForCancellationFuture<'_> {
        self.lost.cancelled()
    }

    async fn end(mut self) {
        self.renewer.abort();
        let _ = (&mut self.renewer).await;
    }
}

impl Drop for StopClaim {
    fn drop(&mut self) {
        self.renewer.abort();
    }
}

enum StopAdmission {
    AlreadyStopped,
    Nudged,
    /// An idle stop found the workspace active again under the row lock.
    NotIdle,
    /// The same operation is being executed by a live claim holder; the
    /// repeated request reports progress instead of executing it again.
    Claimed,
    Ready(CurrentStop),
}

/// A running workspace is idle when its owner activity is older than the
/// idle timeout and no bounded work is in flight: a running agent or command
/// task, or a preview that has not yet reported ready
/// (`g5.lifecycle.idle`, `g5.lifecycle.idle.preview-startup`). Evaluated on
/// `workspace_vms AS vm` joined with `workspace_settings AS settings`.
const WORKSPACE_IDLE_WORK_EXEMPTIONS: &str = "ARRAY(
        SELECT DISTINCT CASE WHEN task.kind = 'preview' THEN 'preview_startup'
                            ELSE 'running_' || task.kind END FROM tasks task
        WHERE task.workspace_id = vm.workspace_id
          AND ((task.kind IN ('agent','command') AND task.status = 'running')
               OR (task.kind = 'preview'
                   AND task.status IN ('queued','running')
                   AND NOT EXISTS (
                       SELECT 1 FROM task_events event
                       WHERE event.task_id = task.id
                         AND event.metadata ->> 'kind' = 'module.preview.ready')))
    )";

/// One projection drives both selection and locked admission, including the
/// explanation. It does not turn task progress into owner activity.
fn idle_decision_query() -> String {
    format!("SELECT vm.workspace_id, vm.vm_id, vm.epoch, vm.last_activity_at,
                    settings.idle_timeout_secs,
                    COALESCE(vm.last_activity_at, vm.started_at, vm.updated_at) AS activity_since,
                    COALESCE(vm.last_activity_at, vm.started_at, vm.updated_at)
                      + make_interval(secs => settings.idle_timeout_secs) <= clock_timestamp() AS expired,
                    vm.idle_retry_after > clock_timestamp() AS retry_deferred,
                    {WORKSPACE_IDLE_WORK_EXEMPTIONS} AS work_exemptions
             FROM workspace_vms vm JOIN workspace_settings settings ON settings.workspace_id = vm.workspace_id
             WHERE vm.status = 'running'")
}

fn log_idle_decision(
    row: &sqlx::postgres::PgRow,
    correlation_id: &str,
    stage: &str,
) -> Result<bool, sqlx::Error> {
    let expired: bool = row.try_get("expired")?;
    let retry = row
        .try_get::<Option<bool>, _>("retry_deferred")?
        .unwrap_or(false);
    let mut exemptions: Vec<String> = row.try_get("work_exemptions")?;
    if !expired {
        exemptions.push("recent_activity_or_initialization".into());
    }
    if retry {
        exemptions.push("stop_retry_backoff".into());
    }
    let idle = exemptions.is_empty();
    // Selection does not log every active VM every sweep. Admission always
    // logs its recheck, including activity/work arriving after selection.
    if expired || stage == "admission" {
        tracing::info!(
            target: "g5_api::workspace_activity",
            event = "workspace_idle_decision", correlation_id, stage,
            workspace_id = %row.try_get::<Uuid, _>("workspace_id")?,
            vm_id = row.try_get::<Option<String>, _>("vm_id")?.as_deref().unwrap_or("none"),
            epoch = row.try_get::<i64, _>("epoch")?,
            last_activity_at = ?row.try_get::<Option<DateTime<Utc>>, _>("last_activity_at")?,
            idle_timeout_secs = row.try_get::<i32, _>("idle_timeout_secs")?,
            decision = if !idle { "exempt" } else if stage == "selection" { "candidate" } else { "stop" },
            exemptions = ?exemptions,
            "workspace idle stop evaluated"
        );
    }
    Ok(idle)
}

/// One running agent attempt a stop is settling, as the API identifies it to
/// the dispatcher.
#[derive(Debug, Clone)]
pub struct StoppedExecution {
    pub task_id: Uuid,
    pub attempt: i32,
    pub vm_id: Uuid,
    pub epoch: u64,
    pub exec_id: String,
}

/// One dispatcher replay record that Agents bound to its attempt: validated
/// before any connection is held, applied on the caller's transaction.
#[derive(Debug, Clone)]
pub struct ReplayRecord {
    /// The event ends the attempt.
    pub terminal: bool,
    /// The record as the dispatcher replayed it.
    pub(crate) event: Value,
}

/// How a stopped execution ended when its replay did not reach a terminal
/// event.
#[derive(Debug, Clone)]
pub enum StoppedExecutionOutcome {
    /// The dispatcher cancelled it before any work; `reason` is its own.
    Cancelled { reason: Value },
    /// It finished but its outcome never persisted; this is the in-memory
    /// outcome the dispatcher still holds.
    Recovered { outcome: Value },
    /// The dispatcher no longer knows it.
    Missing,
}

/// The Agents work a workspace's lifecycle touches beyond a release: a stop
/// quiesces running attempts and holds or retains queued work, a transfer
/// holds it for the new owner, and a scheduled stop leaves each live session
/// a notice. Every operation runs on the caller's connection, in its
/// transaction. Agents implements it and is installed at composition.
#[async_trait::async_trait]
pub trait WorkspaceAgentWork: AgentWorkSettlement {
    /// The non-running half of a stop at the task authority: previews are
    /// cancelled, agent work held or retained by `reason`.
    async fn prepare_tasks_for_stop(
        &self,
        db: &mut ScopedDb,
        workspace_id: Uuid,
        reason: &str,
        operation_id: Uuid,
    ) -> Result<(), AppError>;

    /// Mark the running attempts of one incarnation interrupted, returning
    /// each `(task_id, attempt, acked_event_seq)`.
    async fn mark_running_tasks_interrupted(
        &self,
        conn: &mut PgConnection,
        workspace_id: Uuid,
        vm_id: Uuid,
        epoch: u64,
        operation_id: Uuid,
        reason: &str,
    ) -> Result<Vec<(Uuid, i32, i64)>, sqlx::Error>;

    /// Record that the interrupts of a stop operation were committed.
    async fn commit_task_interrupts(
        &self,
        conn: &mut PgConnection,
        workspace_id: Uuid,
        operation_id: Uuid,
    ) -> Result<(), sqlx::Error>;

    /// Bind one dispatcher replay record to the attempt. Runs before any
    /// connection is held, so an invalid record is refused without one.
    fn validate_replay_event(
        &self,
        execution: &StoppedExecution,
        event: &Value,
    ) -> Result<ReplayRecord, AppError>;

    /// Apply a validated record on the caller's transaction, returning the
    /// sequence the dispatcher may now forget through.
    async fn apply_replay_event(
        &self,
        db: &mut ScopedDb,
        execution: &StoppedExecution,
        record: &ReplayRecord,
    ) -> Result<Option<u64>, AppError>;

    /// Apply one replay page as a single history batch
    /// (`g5.chat.history-batches`), returning the persisted prefix the
    /// dispatcher may now forget through.
    async fn apply_replay_batch(
        &self,
        db: &mut ScopedDb,
        execution: &StoppedExecution,
        records: &[ReplayRecord],
    ) -> Result<Option<u64>, AppError>;

    /// End this process's live follower of the execution before another
    /// consumer drains its journal.
    async fn retire_live_follower(&self, execution: &StoppedExecution);

    /// Settle an attempt whose replay reached no terminal event.
    async fn settle_stopped_execution(
        &self,
        db: &mut ScopedDb,
        execution: &StoppedExecution,
        outcome: StoppedExecutionOutcome,
    ) -> Result<(), AppError>;

    /// Hold logical work and queued input for the new owner.
    async fn hold_tasks_for_transfer(
        &self,
        db: &mut ScopedDb,
        workspace_id: Uuid,
    ) -> Result<(), AppError>;

    /// Leave a live session the notice of a scheduled stop.
    async fn append_lifecycle_notice(
        &self,
        conn: &mut PgConnection,
        session_id: Uuid,
        content: &str,
        operation_id: Uuid,
        stop_reason: &str,
        deadline_at: &str,
    ) -> Result<(), sqlx::Error>;

    /// What a platform-caused interruption of the workspace's last incarnation
    /// left for the lifecycle owner to continue
    /// (`g5.lifecycle.start.automatic-continuation`).
    async fn interrupted_work(
        &self,
        conn: &mut PgConnection,
        workspace_id: Uuid,
    ) -> Result<InterruptedWork, sqlx::Error>;

    /// Whether any agent turn an automatic continuation was recorded for is
    /// still there to run: queued, or held `vm_lost`. The owner may cancel
    /// the last one while the consent waits for capacity.
    async fn continuable_work_remains(
        &self,
        conn: &mut PgConnection,
        workspace_id: Uuid,
    ) -> Result<bool, sqlx::Error>;

    /// Leave every chat with interrupted work the platform notice of what
    /// happened to it (`g5.lifecycle.start.automatic-continuation`).
    async fn announce_automatic_continuation(
        &self,
        conn: &mut PgConnection,
        workspace_id: Uuid,
        content: &str,
        operation_id: Uuid,
        details: Value,
    ) -> Result<(), sqlx::Error>;
}

/// Agents' answer to [`WorkspaceAgentWork::interrupted_work`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InterruptedWork {
    /// An attempt of the ended incarnation has not settled yet; ask again.
    Unsettled,
    /// Nothing a platform interruption left to continue: no agent turn, or
    /// turns held on the owner's decision (their Stop, an input of unknown
    /// delivery, an unanswered clarification).
    None,
    /// Chat turns a lost VM left held `vm_lost`.
    VmLost,
    /// Chat turns a host drain or rollout stop left queued.
    HostStop,
}

/// What the owner is told after a decision.
#[derive(Debug, Clone, Copy)]
enum AutomaticContinuationNews {
    Started,
    NotStarted(&'static str),
}

impl InterruptedWork {
    fn cause(self) -> Option<&'static str> {
        match self {
            Self::VmLost => Some("vm_lost"),
            Self::HostStop => Some("host_stop"),
            Self::Unsettled | Self::None => None,
        }
    }
}

/// A guest call still counted past its budget blocks the operation rather
/// than being interleaved with it; the caller retries once it ends.
fn guest_calls_outlived_their_budget(
    runtime: &WorkspaceRuntime,
    workspace_id: Uuid,
    operation: &str,
) -> G5Error {
    let in_flight = runtime.calls_in_flight(workspace_id);
    tracing::warn!(%workspace_id, in_flight, operation, "guest calls outlived their budget; refusing");
    G5Error::ServiceUnavailable {
        message: format!(
            "{in_flight} guest call(s) still in flight past their budget; retry the {operation}"
        ),
    }
}

#[derive(Clone)]
pub struct WorkspaceVmService {
    system_pool: PgPool,
    operation_deadline: Option<DateTime<Utc>>,
    scope: ScopedDbReopener,
    hosts: Arc<TunnelRegistry>,
    minter: Arc<JwtMinter>,
    placement: WorkerHostService,
    runtime: Arc<WorkspaceRuntime>,
    agent_work: Arc<dyn WorkspaceAgentWork>,
    repo_storage: TenantRepoStorage,
    git_external_url: String,
    ws_hub: Arc<crate::ws::WsHub>,
}

pub struct WorkspaceVmServiceConfig {
    pub system_pool: PgPool,
    pub scope: ScopedDbReopener,
    pub hosts: Arc<TunnelRegistry>,
    pub minter: Arc<JwtMinter>,
    pub runtime: Arc<WorkspaceRuntime>,
    pub agent_work: Arc<dyn WorkspaceAgentWork>,
    pub repo_storage: TenantRepoStorage,
    pub git_external_url: String,
    pub ws_hub: Arc<crate::ws::WsHub>,
}

impl WorkspaceVmService {
    #[must_use]
    pub fn new(config: WorkspaceVmServiceConfig) -> Self {
        Self {
            system_pool: config.system_pool,
            operation_deadline: None,
            scope: config.scope,
            hosts: config.hosts,
            minter: config.minter,
            placement: WorkerHostService::new(),
            runtime: config.runtime,
            agent_work: config.agent_work,
            repo_storage: config.repo_storage,
            git_external_url: config.git_external_url,
            ws_hub: config.ws_hub,
        }
    }

    pub(crate) fn with_operation_deadline(mut self, deadline: DateTime<Utc>) -> Self {
        self.operation_deadline = Some(deadline);
        self
    }

    async fn open_db(&self) -> Result<ScopedDb, G5Error> {
        let mut db = self.scope.open().await.map_err(|error| {
            internal(&format!("failed to open workspace database scope: {error}"))
        })?;
        crate::db::operation_deadline::apply(&mut db, self.operation_deadline).await?;
        Ok(db)
    }

    pub async fn view(&self, workspace_id: Uuid) -> Result<WorkspaceVmView, G5Error> {
        let mut db = self.open_db().await?;
        let view = load_view(db.conn(), workspace_id, false).await?;
        rollback_db(db).await?;
        Ok(view)
    }

    /// The view a stop reports its outcome from. A retired personal
    /// workspace is already tombstoned when its stop runs
    /// (`g5.personal.retirement`), so this view still reads it; admission
    /// alone decides who may stop a tombstoned row.
    async fn stop_view(&self, workspace_id: Uuid) -> Result<WorkspaceVmView, G5Error> {
        let mut db = self.open_db().await?;
        let view = load_view(db.conn(), workspace_id, true).await?;
        rollback_db(db).await?;
        Ok(view)
    }

    pub(crate) async fn resume_agent_review(
        &self,
        review_id: Uuid,
    ) -> Result<AgentReviewStart, G5Error> {
        let mut db = self.open_db().await?;
        let admitted: Option<(Uuid,Uuid)>=sqlx::query_as("SELECT r.review_workspace_id,r.requested_by FROM agent_approval_reviews r JOIN workspaces w ON w.id=r.review_workspace_id AND w.org_id=r.org_id AND w.agent_approval_review_id=r.id WHERE r.id=$1 AND r.stage='start' AND w.deleted_at IS NULL")
            .bind(review_id).fetch_optional(db.conn()).await?;
        rollback_db(db).await?;
        let result = match admitted {
            Some((workspace, requester)) => {
                // The review owns at most its first intent: a replay never
                // overwrites a Start that replaced it (`g5.lifecycle.start`).
                self.resume_with(
                    workspace,
                    requester,
                    review_id,
                    StartKind::CompoundStart,
                    IntentAdmission::FirstOnly,
                )
                .await
            }
            None => Err(internal("review has no admitted VM authority")),
        };
        match result {
            Ok(_) => Ok(AgentReviewStart::Resumed),
            // Every pass replays the review's one intent. Once that consent
            // has ended, name what ended it instead of the refusal it causes.
            Err(error) => match self.agent_review_start_cause(review_id, false).await {
                Ok(Some(cause)) => Ok(AgentReviewStart::Ended(cause)),
                Ok(None) | Err(_) => Err(error),
            },
        }
    }

    /// What a review's workspace was still waiting for when its readiness
    /// deadline passed, if anything more specific than the deadline itself.
    pub(crate) async fn agent_review_deadline_cause(
        &self,
        review_id: Uuid,
    ) -> Result<Option<&'static str>, G5Error> {
        self.agent_review_start_cause(review_id, true).await
    }

    async fn agent_review_start_cause(
        &self,
        review_id: Uuid,
        at_deadline: bool,
    ) -> Result<Option<&'static str>, G5Error> {
        let mut db = self.open_db().await?;
        let row = sqlx::query(
            "SELECT w.id IS NULL OR w.deleted_at IS NOT NULL OR w.deletion_operation_id IS NOT NULL
                        AS deleted,
                    vm.status, vm.start_intent, vm.last_error
             FROM agent_approval_reviews r
             LEFT JOIN workspaces w ON w.id = r.review_workspace_id AND w.org_id = r.org_id
             LEFT JOIN workspace_vms vm ON vm.workspace_id = r.review_workspace_id
             WHERE r.id = $1",
        )
        .bind(review_id)
        .fetch_optional(db.conn())
        .await?;
        rollback_db(db).await?;
        let Some(row) = row else {
            return Ok(None);
        };
        let status = row.try_get::<Option<String>, _>("status")?;
        let intent = row.try_get::<Option<Value>, _>("start_intent")?;
        let last_error = row.try_get::<Option<Value>, _>("last_error")?;
        let ended = ended_review_start_cause(
            review_id,
            row.try_get("deleted")?,
            status.as_deref(),
            intent.as_ref(),
            last_error.as_ref(),
            Utc::now(),
        );
        Ok(ended.or_else(|| {
            at_deadline
                .then(|| {
                    waiting_review_start_cause(
                        status.as_deref(),
                        intent.as_ref(),
                        last_error.as_ref(),
                    )
                })
                .flatten()
        }))
    }

    pub(crate) async fn stop_agent_review(&self, review_id: Uuid) -> Result<(), G5Error> {
        let mut db = self.open_db().await?;
        let workspace: Option<Uuid>=sqlx::query_scalar("SELECT w.id FROM agent_approval_reviews r JOIN workspaces w ON w.id=r.review_workspace_id AND w.org_id=r.org_id AND w.agent_approval_review_id=r.id WHERE r.id=$1 AND r.stage IN ('effective','failed','cancelled') AND w.deleted_at IS NULL")
            .bind(review_id).fetch_optional(db.conn()).await?;
        rollback_db(db).await?;
        if let Some(workspace) = workspace {
            self.stop_as_system(workspace, review_id, StopReason::Delete, Some(Utc::now()))
                .await?;
        }
        Ok(())
    }

    pub async fn resume(
        &self,
        workspace_id: Uuid,
        actor_user_id: Uuid,
        intent_id: Uuid,
        kind: StartKind,
    ) -> Result<ResumeOutcome, G5Error> {
        self.resume_with(
            workspace_id,
            actor_user_id,
            intent_id,
            kind,
            IntentAdmission::Fresh,
        )
        .await
    }

    /// Start a forked workspace for the chat its fork queued, once. The
    /// intent is derived from that chat, so a replayed fork re-observes its
    /// own Start; after any other Start has replaced it, the replay starts
    /// nothing (`g5.lifecycle.start`: retries cannot revive superseded
    /// consent). Shared by the UI's fork and the workspace agent's.
    pub async fn launch_forked_workspace(
        &self,
        workspace_id: Uuid,
        actor_user_id: Uuid,
        started_session_id: Uuid,
    ) -> Result<ResumeOutcome, G5Error> {
        self.resume_with(
            workspace_id,
            actor_user_id,
            fork_launch_intent_id(started_session_id),
            StartKind::Resume,
            IntentAdmission::FirstOnly,
        )
        .await
    }

    /// Cancel `intent_id` while it still only waits (pending or queued for
    /// capacity), leaving a Start already admitted to a host alone. Used when
    /// the rest of a compound Start-and-send request fails after its consent
    /// was recorded (`g5.personal.start-and-send`).
    pub async fn cancel_waiting_start(
        &self,
        workspace_id: Uuid,
        intent_id: Uuid,
    ) -> Result<bool, G5Error> {
        let mut db = self.open_db().await?;
        // A consent another compound action has joined meanwhile carries
        // that action's queued input or preview and stays. The row lock
        // orders this against a binding in flight, which commits under the
        // same lock, so the bound-work read sees it
        // (`g5.preview.start-and-preview`).
        sqlx::query("SELECT 1 FROM workspace_vms WHERE workspace_id = $1 FOR UPDATE")
            .bind(workspace_id)
            .execute(db.conn())
            .await?;
        let joined = sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS (SELECT 1 FROM session_inputs input
                            WHERE input.start_consent_id = $1 AND input.delivery = 'queued')
                 OR EXISTS (SELECT 1 FROM tasks task
                            WHERE task.start_consent_id = $1 AND task.status = 'queued')",
        )
        .bind(intent_id)
        .fetch_one(db.conn())
        .await?;
        if joined {
            commit_db(db).await?;
            return Ok(false);
        }
        // A consent another action joined (`joined`) stays as well, whether
        // or not that action's work is bound through a column: a Start and
        // answer re-queues its held turn without one.
        let cancelled = sqlx::query(
            "UPDATE workspace_vms
             SET start_intent = jsonb_set(start_intent, '{status}', '\"cancelled\"'),
                 updated_at = now()
             WHERE workspace_id = $1 AND start_intent ->> 'id' = $2::text
               AND start_intent ->> 'status' IN ('pending', 'queued_for_capacity')
               AND NOT COALESCE((start_intent ->> 'joined')::boolean, false)",
        )
        .bind(workspace_id)
        .bind(intent_id)
        .execute(db.conn())
        .await?
        .rows_affected()
            == 1;
        commit_db(db).await?;
        Ok(cancelled)
    }

    /// Retry a queued start captured by reconciliation. It only replays that
    /// exact intent: if Stop cancelled it and another start replaced it in the
    /// meantime, the retry must not bring the old intent back.
    pub async fn resume_queued(
        &self,
        workspace_id: Uuid,
        actor_user_id: Uuid,
        intent_id: Uuid,
        kind: StartKind,
    ) -> Result<ResumeOutcome, G5Error> {
        self.resume_with(
            workspace_id,
            actor_user_id,
            intent_id,
            kind,
            IntentAdmission::ReplayOnly,
        )
        .await
    }

    async fn resume_with(
        &self,
        workspace_id: Uuid,
        actor_user_id: Uuid,
        intent_id: Uuid,
        kind: StartKind,
        admission: IntentAdmission,
    ) -> Result<ResumeOutcome, G5Error> {
        self.resume_recording(
            workspace_id,
            actor_user_id,
            intent_id,
            kind,
            admission,
            None,
        )
        .await
    }

    /// `resume_with`, with the automatic continuation decision a fresh
    /// automatic start records in the same transaction as its consent
    /// (`g5.lifecycle.start.automatic-continuation`).
    async fn resume_recording(
        &self,
        workspace_id: Uuid,
        actor_user_id: Uuid,
        intent_id: Uuid,
        kind: StartKind,
        admission: IntentAdmission,
        automatic_record: Option<&Value>,
    ) -> Result<ResumeOutcome, G5Error> {
        let mut selected = None;
        self.resume_recording_selecting(
            workspace_id,
            actor_user_id,
            intent_id,
            kind,
            admission,
            automatic_record,
            &mut selected,
        )
        .await
    }

    /// A compound action's Start (`g5.chat.start-and-send`,
    /// `g5.preview.start-and-preview`): the outcome, with the consent id
    /// admission selected under the VM row lock, which is the id the
    /// action's work binds to. It is `intent_id` when this call recorded the
    /// consent, or the id of the owner's Start it replayed or joined.
    pub async fn resume_for_binding(
        &self,
        workspace_id: Uuid,
        actor_user_id: Uuid,
        intent_id: Uuid,
        kind: StartKind,
    ) -> Result<(ResumeOutcome, Uuid), G5Error> {
        let mut selected = None;
        let outcome = self
            .resume_recording_selecting(
                workspace_id,
                actor_user_id,
                intent_id,
                kind,
                IntentAdmission::Fresh,
                None,
                &mut selected,
            )
            .await?;
        Ok((outcome, selected.unwrap_or(intent_id)))
    }

    #[allow(clippy::too_many_arguments)]
    async fn resume_recording_selecting(
        &self,
        workspace_id: Uuid,
        actor_user_id: Uuid,
        intent_id: Uuid,
        kind: StartKind,
        admission: IntentAdmission,
        automatic_record: Option<&Value>,
        selected_consent: &mut Option<Uuid>,
    ) -> Result<ResumeOutcome, G5Error> {
        let input = self
            .record_start_intent(
                workspace_id,
                actor_user_id,
                intent_id,
                kind,
                admission,
                automatic_record,
            )
            .await?;
        *selected_consent = Some(input.consent_id);
        let current = self.view(workspace_id).await?;
        if current.status == "running" {
            return Ok(ResumeOutcome::Attached { vm: current });
        }
        if current.status == "starting" {
            return Ok(ResumeOutcome::Starting { vm: current });
        }

        let placement = {
            let mut db = self.open_db().await?;
            let placement = self
                .placement
                .place(
                    &mut db,
                    PlacementRequest {
                        org_id: input.org_id,
                        workspace_id,
                        region: &input.region,
                        reachable_host_ids: &self.hosts.reachable_control_host_ids(),
                        excluded_host_ids: &[],
                        start_deadline_at: Utc::now() + TimeDelta::seconds(START_ACQUISITION_SECS),
                    },
                )
                .await
                .map_err(app_error)?;
            commit_db(db).await?;
            placement
        };
        let reservation = match placement {
            PlacementOutcome::Reserved(reservation) => {
                g5_api_foundation::activity::log_activity_change(
                    workspace_id,
                    reservation.vm_id,
                    reservation.epoch,
                    "initialization",
                    "vm_placement",
                    &input.consent_id.to_string(),
                    reservation.previous_activity_at,
                    None,
                );
                reservation
            }
            PlacementOutcome::NoCapacity | PlacementOutcome::QuotaExceeded { .. } => {
                return Ok(ResumeOutcome::QueuedForCapacity {
                    vm: self.view(workspace_id).await?,
                });
            }
            PlacementOutcome::OwnerLimitExceeded { limit } => {
                return Err(G5Error::OwnerLimitReached {
                    kind: OwnerLimitKind::RunningVms,
                    limit,
                });
            }
            PlacementOutcome::WorkspaceBusy => {
                let vm = self.view(workspace_id).await?;
                return match vm.status.as_str() {
                    "running" => Ok(ResumeOutcome::Attached { vm }),
                    "starting" => Ok(ResumeOutcome::Starting { vm }),
                    "stopping" => Err(G5Error::WorkspaceStopping),
                    _ => Err(G5Error::Conflict {
                        reason: "workspace placement changed concurrently".to_string(),
                    }),
                };
            }
            PlacementOutcome::StartIntentUnavailable => {
                return Err(G5Error::Conflict {
                    reason: "start intent is unavailable or expired".to_string(),
                });
            }
        };

        let spec = self.build_spec(&input, &reservation).await?;
        let spec_value = serde_json::to_value(&spec)?;
        let bound = {
            let mut db = self.open_db().await?;
            let bound = self
                .placement
                .bind_boot_record(
                    &mut db,
                    input.org_id,
                    workspace_id,
                    &reservation,
                    PlacementBootRecord {
                        image_ref: &input.image_ref,
                        image_digest: &input.image_digest,
                        image_source: &input.image_source,
                        image_oid_vector: &input.image_oid_vector,
                        module_catalog_version: catalog_version(),
                        boot_spec: &spec_value,
                        lease_ttl_secs: i32::try_from(LEASE_TTL_SECS).unwrap_or(i32::MAX),
                    },
                )
                .await
                .map_err(app_error)?;
            commit_db(db).await?;
            bound
        };
        if !bound {
            return Err(G5Error::Conflict {
                reason: "workspace incarnation changed before boot binding".to_string(),
            });
        }
        let token = self.minter.mint_workspace_incarnation(
            input.org_id,
            workspace_id,
            input.owner_user_id,
            reservation.vm_id,
            reservation.epoch,
            INCARNATION_TOKEN_TTL_SECS,
        )?;
        let command = CommandParams {
            // A start intent may be placed more than once after a host refuses
            // a slot. The host idempotency boundary is one concrete VM
            // incarnation, whose payload stays stable across transport retry.
            idempotency_key: format!(
                "start:{}:{}:{}",
                input.consent_id, reservation.vm_id, reservation.epoch
            ),
            budget_ms: 120_000,
            command: VmStart {
                workspace_id,
                vm_id: reservation.vm_id,
                spec,
                workspace_token: token,
            },
        };
        let Some(tunnel) = self.hosts.get(&reservation.host_id) else {
            self.release_start(&input, &reservation, false, "host_tunnel_unavailable", None)
                .await?;
            return Ok(ResumeOutcome::QueuedForCapacity {
                vm: self.view(workspace_id).await?,
            });
        };
        let Some(client) = tunnel.command_client() else {
            self.release_start(
                &input,
                &reservation,
                false,
                "host_control_unavailable",
                None,
            )
            .await?;
            return Ok(ResumeOutcome::QueuedForCapacity {
                vm: self.view(workspace_id).await?,
            });
        };
        // Stamp the send and enqueue the Start in one row-locked transaction
        // (`g5.runtime.start.settlement`): a release that reads the stamp
        // knows the host sees this Start before any command it sends later,
        // and a release that already cleared the incarnation makes the fence
        // fail, so nothing is sent for it.
        let reply_deadline = tokio::time::Instant::now() + Duration::from_secs(START_REPLY_SECS);
        let dispatched = {
            let mut db = self.open_db().await?;
            // Serialize with membership cleanup's stop admission, which takes
            // the workspace lock first, and admit only an active owner.
            sqlx::query("SELECT id FROM workspaces WHERE id = $1 FOR UPDATE")
                .bind(workspace_id)
                .execute(db.conn())
                .await?;
            crate::services::PermissionService::require_active_member_on_conn(
                db.conn(),
                actor_user_id,
                input.org_id,
                true,
            )
            .await?;
            let stamped = sqlx::query(
                "UPDATE workspace_vms
                 SET start_command_sent_at = now(), updated_at = now()
                 WHERE org_id = $1 AND workspace_id = $2 AND host_id = $3
                   AND vm_id = $4 AND epoch = $5 AND status = 'starting'
                   AND start_command_sent_at IS NULL",
            )
            .bind(input.org_id)
            .bind(workspace_id)
            .bind(reservation.host_id)
            .bind(reservation.vm_id.to_string())
            .bind(i64::try_from(reservation.epoch).unwrap_or(i64::MAX))
            .execute(db.conn())
            .await?
            .rows_affected()
                == 1;
            if !stamped {
                rollback_db(db).await?;
                None
            } else {
                match client
                    .dispatch(
                        method::VM_START,
                        &command,
                        Duration::from_secs(START_ENQUEUE_SECS),
                    )
                    .await
                {
                    Ok(pending) => {
                        commit_db(db).await?;
                        Some(Ok(pending))
                    }
                    Err(error) => {
                        rollback_db(db).await?;
                        Some(Err(error))
                    }
                }
            }
        };
        let pending = match dispatched {
            None => {
                // The incarnation was released or replaced while the spec was
                // built; whoever did so owns the row now.
                let vm = self.view(workspace_id).await?;
                return match vm.status.as_str() {
                    "running" => Ok(ResumeOutcome::Attached { vm }),
                    "starting" => Ok(ResumeOutcome::Starting { vm }),
                    "queued_for_capacity" => Ok(ResumeOutcome::QueuedForCapacity { vm }),
                    "stopping" => Err(G5Error::WorkspaceStopping),
                    _ => Err(G5Error::Conflict {
                        reason: "workspace placement changed before the start was sent".to_string(),
                    }),
                };
            }
            Some(Err(error)) => {
                tracing::warn!(%error, workspace_id = %workspace_id, host_id = %reservation.host_id, "workspace start could not be enqueued on the host connection");
                self.release_start(
                    &input,
                    &reservation,
                    false,
                    "host_control_unavailable",
                    None,
                )
                .await?;
                return Ok(ResumeOutcome::QueuedForCapacity {
                    vm: self.view(workspace_id).await?,
                });
            }
            Some(Ok(pending)) => pending,
        };
        // A timed-out reply leaves the outcome unknown; one retry keeps the
        // same identity on the same connection (`g5.runtime.control.replay`).
        let result = match pending.wait(reply_deadline).await {
            Err(crate::ws::host_ctl::HostCtlError::Timeout) => {
                client
                    .execute(
                        method::VM_START,
                        &command,
                        Duration::from_secs(START_REPLY_SECS),
                    )
                    .await
            }
            outcome => outcome,
        };
        match result {
            Ok(
                CommandResult::Started | CommandResult::AlreadyRunning | CommandResult::Accepted,
            ) => Ok(ResumeOutcome::Starting {
                vm: self.view(workspace_id).await?,
            }),
            Ok(
                refused @ (CommandResult::SlotUnavailable { .. }
                | CommandResult::Draining
                | CommandResult::WorkspaceBusy),
            ) => {
                // Keep the host's reason: a full disk needs different action
                // from busy slots, and both used to read as "no worker host".
                // A host still holding another VM of this workspace queues the
                // start the same way until that VM is released.
                let host_reason = match refused {
                    CommandResult::SlotUnavailable { reason } => reason.as_str(),
                    CommandResult::WorkspaceBusy => SlotUnavailableReason::WorkspaceBusy.as_str(),
                    _ => SlotUnavailableReason::Draining.as_str(),
                };
                tracing::warn!(
                    workspace_id = %workspace_id,
                    host_id = %reservation.host_id,
                    host_reason,
                    "workspace host refused the VM start; queued for capacity"
                );
                self.release_start(
                    &input,
                    &reservation,
                    true,
                    "host_capacity_refused",
                    Some(host_reason),
                )
                .await?;
                Ok(ResumeOutcome::QueuedForCapacity {
                    vm: self.view(workspace_id).await?,
                })
            }
            Ok(CommandResult::SpecRejected { reason }) => {
                self.fail_start(&input, &reservation, &format!("spec_rejected:{reason:?}"))
                    .await?;
                Err(G5Error::Conflict {
                    reason: format!("workspace boot spec was rejected: {reason:?}"),
                })
            }
            Ok(CommandResult::BootFailed { reason, .. }) => {
                self.fail_start(&input, &reservation, &format!("boot_failed:{reason}"))
                    .await?;
                Err(G5Error::ServiceUnavailable {
                    message: format!("workspace boot failed: {reason}"),
                })
            }
            Ok(other) => {
                self.release_start(&input, &reservation, true, "host_start_refused", None)
                    .await?;
                Err(G5Error::ServiceUnavailable {
                    message: format!("worker host refused workspace start: {other:?}"),
                })
            }
            Err(error) => {
                // Delivery is ambiguous. Keep the same durable incarnation so
                // the reconciler can query/retry it; never allocate another VM.
                tracing::warn!(%error, workspace_id = %workspace_id, vm_id = %reservation.vm_id, "workspace start reply ambiguous");
                Ok(ResumeOutcome::Starting {
                    vm: self.view(workspace_id).await?,
                })
            }
        }
    }

    /// The owner's Stop (`g5.lifecycle.stop`): refused unless `actor_user_id`
    /// is the workspace's current owner.
    pub async fn stop(
        &self,
        workspace_id: Uuid,
        actor_user_id: Uuid,
        operation_id: Uuid,
        reason: StopReason,
        deadline_at: Option<DateTime<Utc>>,
    ) -> Result<StopOutcome, G5Error> {
        self.stop_with_authority(
            workspace_id,
            Some(actor_user_id),
            operation_id,
            reason,
            deadline_at,
            false,
        )
        .await
    }

    pub(crate) async fn stop_for_deletion(
        &self,
        workspace_id: Uuid,
        actor: Uuid,
        operation_id: Uuid,
        force: bool,
    ) -> Result<StopOutcome, G5Error> {
        self.stop_with_authority(
            workspace_id,
            Some(actor),
            operation_id,
            if force {
                StopReason::Delete
            } else {
                StopReason::User
            },
            force.then(Utc::now),
            true,
        )
        .await
    }

    /// An organization administrator's capacity stop. The caller has proved
    /// the administrator's authority and the workspace's organization; the
    /// stop runs with system authority and the capacity reason.
    pub async fn stop_for_capacity(
        &self,
        workspace_id: Uuid,
        operation_id: Uuid,
        deadline_at: DateTime<Utc>,
    ) -> Result<StopOutcome, G5Error> {
        self.stop_with_authority(
            workspace_id,
            None,
            operation_id,
            StopReason::Capacity,
            Some(deadline_at),
            false,
        )
        .await
    }

    /// A stop the platform decides (transfer, deadlines, idle, drains),
    /// without an owner check. Crate-private, so a route cannot stop a
    /// workspace its caller does not own.
    pub(crate) async fn stop_as_system(
        &self,
        workspace_id: Uuid,
        operation_id: Uuid,
        reason: StopReason,
        deadline_at: Option<DateTime<Utc>>,
    ) -> Result<StopOutcome, G5Error> {
        self.stop_with_authority(workspace_id, None, operation_id, reason, deadline_at, false)
            .await
    }

    /// For integration tests that drive a platform stop directly.
    #[cfg(feature = "test-helpers")]
    pub async fn stop_as_system_for_tests(
        &self,
        workspace_id: Uuid,
        operation_id: Uuid,
        reason: StopReason,
        deadline_at: Option<DateTime<Utc>>,
    ) -> Result<StopOutcome, G5Error> {
        self.stop_as_system(workspace_id, operation_id, reason, deadline_at)
            .await
    }

    /// For integration tests: admit the owner's stop and then abandon it, the
    /// way an API process that dies right after admission does. Returns
    /// whether the stop was admitted for execution.
    #[cfg(feature = "test-helpers")]
    pub async fn admit_stop_and_abandon_for_tests(
        &self,
        workspace_id: Uuid,
        actor_user_id: Uuid,
        operation_id: Uuid,
        reason: StopReason,
    ) -> Result<bool, G5Error> {
        Ok(matches!(
            self.admit_stop(
                workspace_id,
                Some(actor_user_id),
                operation_id,
                reason,
                None,
                None,
                false,
            )
            .await?,
            StopAdmission::Ready(_)
        ))
    }

    /// Extend this executor's claim on a `stopping` row by replacing exactly
    /// the value it wrote last. Any other value (a yield, a release, a
    /// claim taken after this one lapsed) leaves the row alone and returns
    /// `None`, so a renewal can neither postpone a yield nor extend another
    /// executor's claim, whatever order the two transactions commit in.
    async fn renew_stop_claim(
        &self,
        current: &CurrentStop,
        expected: DateTime<Utc>,
    ) -> Result<Option<DateTime<Utc>>, G5Error> {
        let mut db = self.open_db().await?;
        let renewed = sqlx::query_scalar(
            "UPDATE workspace_vms
             SET stop_retry_at = now() + make_interval(secs => $5)
             WHERE workspace_id = $1 AND vm_id = $2 AND epoch = $3
               AND stop_operation_id = $4 AND status = 'stopping'
               AND stop_retry_at = $6
             RETURNING stop_retry_at",
        )
        .bind(current.workspace_id)
        .bind(current.vm_id.to_string())
        .bind(i64::try_from(current.epoch).unwrap_or(i64::MAX))
        .bind(current.operation_id)
        .bind(STOP_CLAIM_LEASE_SECS as f64)
        .bind(expected)
        .fetch_optional(db.conn())
        .await?;
        commit_db(db).await?;
        Ok(renewed)
    }

    async fn stop_with_authority(
        &self,
        workspace_id: Uuid,
        actor_user_id: Option<Uuid>,
        operation_id: Uuid,
        reason: StopReason,
        deadline_at: Option<DateTime<Utc>>,
        deleting: bool,
    ) -> Result<StopOutcome, G5Error> {
        let admission = self
            .admit_stop(
                workspace_id,
                actor_user_id,
                operation_id,
                reason,
                deadline_at,
                None,
                deleting,
            )
            .await?;
        let current = match admission {
            StopAdmission::Ready(current) => current,
            StopAdmission::Nudged | StopAdmission::NotIdle => {
                return Ok(StopOutcome {
                    operation_id,
                    status: "running".to_string(),
                    flush_id: None,
                    unflushed_loss: false,
                });
            }
            StopAdmission::AlreadyStopped => {
                let vm = self.stop_view(workspace_id).await?;
                return Ok(StopOutcome {
                    operation_id,
                    status: "stopped".to_string(),
                    flush_id: vm.last_flush_id,
                    unflushed_loss: vm.unflushed_loss_at.is_some(),
                });
            }
            StopAdmission::Claimed => {
                let vm = self.stop_view(workspace_id).await?;
                return Ok(StopOutcome {
                    operation_id,
                    status: vm.status,
                    flush_id: vm.last_flush_id,
                    unflushed_loss: vm.unflushed_loss_at.is_some(),
                });
            }
        };
        log_stop_phase(&current, "admitted");

        // Host tunnels are process-local. Every API replica runs the
        // lifecycle reconciler against the shared database, so a replica that
        // does not hold this host's tunnel would drive every RPC below into
        // HOST_TUNNEL_UNAVAILABLE and, through its retry backoff, keep the
        // replica that can reach the host from ever claiming the stop. Leave
        // the durable stop to the tunnel owner, also past the save window,
        // since only the tunnel owner can send the kill. With no tunnel
        // anywhere, the lease and host-loss rules release the VM.
        if self.hosts.get(&current.host_id).is_none() {
            log_stop_phase(&current, "deferred_to_tunnel_owner");
            self.defer_stop_to_tunnel_owner(&current).await?;
            return Ok(StopOutcome {
                operation_id,
                status: "stopping".to_string(),
                flush_id: None,
                unflushed_loss: false,
            });
        }

        // An admitted stop belongs to the workspace, not to the request that
        // asked for it. The execution runs on its own task: the caller waits
        // for it, but a caller that goes away (the owner's browser reloads,
        // the connection resets and the handler future is dropped) leaves it
        // running to completion under its claim. An API process death
        // interrupts it, and then the claim lapses within
        // `STOP_CLAIM_LEASE_SECS` and the reconciler resumes the operation.
        let service = self.clone();
        let execution = tokio::spawn(async move { service.execute_admitted_stop(current).await });
        match execution.await {
            Ok(outcome) => outcome,
            Err(join_error) => Err(internal(&format!(
                "workspace stop execution ended abnormally: {join_error}"
            ))),
        }
    }

    /// Run an admitted stop to its end (or to a yield) while holding and
    /// renewing the row's claim. An execution whose claim is lost stops at
    /// once and reports the row as it stands: the operation now belongs to
    /// whoever holds the claim, and every step it might have been in the
    /// middle of is keyed so that holder's replay lands on the same result.
    async fn execute_admitted_stop(&self, current: CurrentStop) -> Result<StopOutcome, G5Error> {
        let claim = StopClaim::hold(self.clone(), current.clone());
        let outcome = tokio::select! {
            outcome = self.run_admitted_stop(&current) => outcome,
            () = claim.lost() => {
                log_stop_phase(&current, "execution_stopped_without_claim");
                self.stop_view(current.workspace_id).await.map(|vm| StopOutcome {
                    operation_id: current.operation_id,
                    status: vm.status,
                    flush_id: vm.last_flush_id,
                    unflushed_loss: vm.unflushed_loss_at.is_some(),
                })
            }
        };
        claim.end().await;
        outcome
    }

    async fn run_admitted_stop(&self, current: &CurrentStop) -> Result<StopOutcome, G5Error> {
        let current = current.clone();
        let workspace_id = current.workspace_id;
        let operation_id = current.operation_id;
        let stopping = || StopOutcome {
            operation_id,
            status: "stopping".to_string(),
            flush_id: None,
            unflushed_loss: false,
        };
        // Saving happens only inside the save window. A stop resumed after it
        // calls the guest no more: each guest request would carry the expired
        // window as its deadline, and the guest may be why saving failed.
        let mut forced = false;
        if current.requires_flush && Utc::now() >= current.deadline_at {
            self.record_unflushed_loss(&current, &save_window_ended())
                .await?;
            forced = true;
        }
        let handle = if current.requires_flush && !forced {
            log_stop_phase(&current, "resolving_runtime");
            let resolved = match self.runtime.get(workspace_id).await {
                Ok(Some(handle)) => Ok(handle),
                Ok(None) => Err("durable stop incarnation is unavailable".to_string()),
                Err(error) => Err(error.to_string()),
            };
            match resolved {
                Ok(handle) => {
                    log_stop_phase(&current, "runtime_resolved");
                    Some(handle)
                }
                Err(detail) => {
                    let error = RuntimeRpcError {
                        code: 503,
                        message: detail,
                        data: Value::Null,
                    };
                    if !self.give_up_saving(&current, &error).await? {
                        return Ok(stopping());
                    }
                    forced = true;
                    None
                }
            }
        } else {
            None
        };
        if let Some(handle) = handle.as_ref() {
            // Refuse new writers first, but do not interrupt admitted module or
            // terminal writers until their durable task stop intent is stored.
            // A second close below performs cancellation and the actual drain.
            let request =
                lifecycle_request(&current, format!("stop:{operation_id}:admission-prepare"));
            log_stop_phase(&current, "preparing_admission");
            if let Err(error) = self
                .runtime
                .call(
                    handle,
                    &request,
                    "admission.close",
                    json!({"prepare_only":true,"operation_id":operation_id}),
                )
                .await
            {
                if !self.give_up_saving(&current, &error).await? {
                    return Ok(stopping());
                }
                forced = true;
            }
            if !forced {
                log_stop_phase(&current, "admission_prepared");
                log_stop_phase(&current, "quiescing_tasks");
                if let Err(error) = self.quiesce_agent_tasks(&current, handle).await {
                    if !self.give_up_saving(&current, &error).await? {
                        return Ok(stopping());
                    }
                    forced = true;
                }
            }
            if !forced {
                log_stop_phase(&current, "tasks_quiesced");
                let request =
                    lifecycle_request(&current, format!("stop:{operation_id}:admission-drain"));
                log_stop_phase(&current, "draining_admission");
                if let Err(error) = self
                    .runtime
                    .call(
                        handle,
                        &request,
                        "admission.close",
                        json!({"operation_id":operation_id}),
                    )
                    .await
                {
                    if !self.give_up_saving(&current, &error).await? {
                        return Ok(stopping());
                    }
                    forced = true;
                } else {
                    log_stop_phase(&current, "admission_drained");
                }
            }
        }

        let mut flush_id = self.completed_stop_flush(&current).await?;
        if current.requires_flush && flush_id.is_none() && !forced {
            let runtime_handle = handle
                .as_ref()
                .ok_or_else(|| internal("flush stop is missing its runtime handle"))?;
            let mut attempt = 0_u32;
            log_stop_phase(&current, "flushing_workspace");
            let flush = loop {
                attempt += 1;
                let request = lifecycle_request(&current, format!("stop:{operation_id}:flush"));
                match self
                    .runtime
                    .call(
                        runtime_handle,
                        &request,
                        "workspace.flush",
                        json!({"reason":"shutdown"}),
                    )
                    .await
                {
                    Ok(value) => break serde_json::from_value::<FlushCompleteResponse>(value).ok(),
                    Err(error)
                        if Utc::now() < current.deadline_at
                            && attempt < 6
                            && !is_deterministic_flush_error(&error) =>
                    {
                        tokio::time::sleep(Duration::from_millis(
                            100_u64.saturating_mul(1_u64 << attempt.min(5)),
                        ))
                        .await;
                    }
                    Err(error) => {
                        if !self.give_up_saving(&current, &error).await? {
                            return Ok(stopping());
                        }
                        forced = true;
                        break None;
                    }
                }
            };
            flush_id = flush.map(|receipt| receipt.flush_id);
            log_stop_phase(&current, "workspace_flushed");
        }

        // After a complete save nothing required remains in the guest, so a
        // short graceful shutdown is enough; a stop that gave up saving, or
        // one resumed past its window, kills at once. The host binds an
        // idempotency key to its payload, so each mode has its own key.
        let kill = forced || Utc::now() >= current.deadline_at;
        let (mode, grace_secs, key) = if kill {
            (StopMode::Kill, 0, "host-kill")
        } else {
            (StopMode::Graceful, CLEAN_STOP_GRACE_SECS, "host")
        };
        let command_wait = Duration::from_secs(grace_secs + HOST_STOP_REAP_SECS);
        // The claim keeps the reconciler away while this executor waits for
        // the host (so this step leaves `stop_retry_at` to the renewals); if
        // the executor dies mid-command, the reconciler resends the same key
        // within the claim lease and the host answers `Busy` or its recorded
        // result.
        let mut db = self.open_db().await?;
        sqlx::query(
            "UPDATE workspace_vms
             SET stop_command_sent_at = COALESCE(stop_command_sent_at, now()),
                 updated_at = now()
             WHERE workspace_id = $1 AND vm_id = $2 AND epoch = $3
               AND stop_operation_id = $4 AND status = 'stopping'",
        )
        .bind(workspace_id)
        .bind(current.vm_id.to_string())
        .bind(i64::try_from(current.epoch).unwrap_or(i64::MAX))
        .bind(current.operation_id)
        .execute(db.conn())
        .await?;
        commit_db(db).await?;

        let command = CommandParams {
            idempotency_key: format!("stop:{operation_id}:{key}"),
            budget_ms: (grace_secs + HOST_STOP_REAP_SECS - 5) * 1000,
            command: VmStop {
                workspace_id,
                vm_id: current.vm_id,
                mode,
                grace_secs,
            },
        };
        log_stop_phase(&current, "stopping_host_vm");
        let host_result = match self.hosts.get(&current.host_id) {
            Some(tunnel) => match tunnel.command_client() {
                Some(client) => {
                    client
                        .execute_with_retry(method::VM_STOP, &command, command_wait, 2)
                        .await
                }
                None => Err(crate::ws::host_ctl::HostCtlError::Closed),
            },
            None => Err(crate::ws::host_ctl::HostCtlError::Closed),
        };
        log_stop_phase(&current, "host_vm_stop_replied");
        match host_result {
            Ok(CommandResult::Stopped { .. } | CommandResult::NotFound) => {
                self.release_stopped(&current).await?;
                log_stop_phase(&current, "released");
            }
            Ok(CommandResult::Busy | CommandResult::Accepted) | Err(_) => {
                // Keep `stopping` and yield: the reconciler resends the same
                // command and the current-incarnation exit event also
                // performs release.
                log_stop_phase(&current, "yielded_host_vm_stop_retry");
                let mut db = self.open_db().await?;
                sqlx::query(
                    "UPDATE workspace_vms SET stop_retry_at = now() + interval '5 seconds', updated_at = now()
                     WHERE workspace_id = $1 AND vm_id = $2 AND epoch = $3
                       AND stop_operation_id = $4 AND status = 'stopping'",
                )
                .bind(workspace_id)
                .bind(current.vm_id.to_string())
                .bind(i64::try_from(current.epoch).unwrap_or(i64::MAX))
                .bind(current.operation_id)
                .execute(db.conn())
                .await?;
                commit_db(db).await?;
            }
            Ok(other) => {
                return Err(G5Error::ServiceUnavailable {
                    message: format!("worker host refused workspace stop: {other:?}"),
                });
            }
        }
        let view = self.stop_view(workspace_id).await?;
        Ok(StopOutcome {
            operation_id,
            status: view.status,
            flush_id: flush_id.or(view.last_flush_id),
            unflushed_loss: view.unflushed_loss_at.is_some(),
        })
    }

    /// Record the stop intent before delivering cancellation, then wait for
    /// each module identity to reach a durable non-running disposition.  A
    /// cancellation sent before `modules.invoke` is still authoritative at the
    /// dispatcher, so an API crash cannot launch work after stop admission.
    async fn quiesce_agent_tasks(
        &self,
        current: &CurrentStop,
        handle: &crate::services::workspace_runtime::WorkspaceRuntimeHandle,
    ) -> Result<(), RuntimeRpcError> {
        let mut db = self.open_db().await.map_err(|_| RuntimeRpcError {
            code: 503,
            message: "AGENT_STOP_PREPARE_FAILED".to_string(),
            data: Value::Null,
        })?;
        let rows = self
            .agent_work
            .mark_running_tasks_interrupted(
                db.conn(),
                current.workspace_id,
                current.vm_id,
                current.epoch,
                current.operation_id,
                current.reason.as_str(),
            )
            .await
            .map_err(|_| RuntimeRpcError {
                code: 503,
                message: "AGENT_STOP_PREPARE_FAILED".to_string(),
                data: Value::Null,
            })?;
        commit_db(db).await.map_err(|_| RuntimeRpcError {
            code: 503,
            message: "AGENT_STOP_PREPARE_FAILED".to_string(),
            data: Value::Null,
        })?;
        let mut executions = Vec::with_capacity(rows.len());
        for (task_id, attempt, acked_event_seq) in rows {
            let acked_event_seq = u64::try_from(acked_event_seq).map_err(|_| RuntimeRpcError {
                code: 503,
                message: "AGENT_STOP_PREPARE_FAILED".to_string(),
                data: Value::Null,
            })?;
            executions.push((task_id, attempt, acked_event_seq));
        }
        for (task_id, attempt, acked_event_seq) in executions {
            let exec_id = canonical_exec_id(task_id, attempt, current.vm_id);
            let cancel = lifecycle_request(
                current,
                format!("stop:{}:cancel:{exec_id}", current.operation_id),
            );
            self.runtime
                .modules_cancel(handle, &cancel, &exec_id, current.reason.as_str())
                .await?;
            loop {
                let status = lifecycle_request(
                    current,
                    format!("stop:{}:status:{exec_id}", current.operation_id),
                );
                let value = self
                    .runtime
                    .modules_status(handle, &status, &exec_id)
                    .await?;
                if value["status"].as_str() != Some("running") {
                    break;
                }
                if Utc::now() >= current.deadline_at {
                    return Err(RuntimeRpcError {
                        code: 408,
                        message: "AGENT_STOP_DEADLINE_EXCEEDED".to_string(),
                        data: json!({"exec_id":exec_id}),
                    });
                }
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
            self.settle_stopped_execution(
                current,
                handle,
                task_id,
                attempt,
                acked_event_seq,
                &exec_id,
            )
            .await?;
        }
        let mut db = self.open_db().await.map_err(|_| RuntimeRpcError {
            code: 503,
            message: "AGENT_STOP_COMMIT_FAILED".to_string(),
            data: Value::Null,
        })?;
        self.agent_work
            .commit_task_interrupts(db.conn(), current.workspace_id, current.operation_id)
            .await
            .map_err(|_| RuntimeRpcError {
                code: 503,
                message: "AGENT_STOP_COMMIT_FAILED".to_string(),
                data: Value::Null,
            })?;
        commit_db(db).await.map_err(|_| RuntimeRpcError {
            code: 503,
            message: "AGENT_STOP_COMMIT_FAILED".to_string(),
            data: Value::Null,
        })?;
        Ok(())
    }

    async fn settle_stopped_execution(
        &self,
        current: &CurrentStop,
        handle: &crate::services::workspace_runtime::WorkspaceRuntimeHandle,
        task_id: Uuid,
        attempt: i32,
        acked_event_seq: u64,
        exec_id: &str,
    ) -> Result<(), RuntimeRpcError> {
        let execution = StoppedExecution {
            task_id,
            attempt,
            vm_id: current.vm_id,
            epoch: current.epoch,
            exec_id: exec_id.to_string(),
        };
        // The live follower hands the journal over: the drain stores and
        // acknowledges the remaining pages as batches, then the persisted
        // prefix even when nothing is left, so retirement never leaves
        // acknowledged-but-unconfirmed data behind (`g5.chat.journal`).
        self.agent_work.retire_live_follower(&execution).await;
        let mut acked = acked_event_seq;
        let mut attempt_settled = false;
        let mut page_nonce = 0u64;
        loop {
            page_nonce += 1;
            let request = lifecycle_request(
                current,
                format!(
                    "stop:{}:events:{exec_id}:{acked}:{page_nonce}",
                    current.operation_id
                ),
            );
            let replay = self
                .runtime
                .modules_events_page(
                    handle,
                    &request,
                    exec_id,
                    acked.saturating_add(1),
                    crate::services::workspace::workspace_runtime::JOURNAL_PAGE_LIMIT,
                    crate::services::workspace::workspace_runtime::JOURNAL_PAGE_MAX_BYTES,
                )
                .await?;
            let events = replay["events"].as_array().ok_or_else(|| RuntimeRpcError {
                code: 503,
                message: "AGENT_STOP_REPLAY_INVALID".to_string(),
                data: json!({"exec_id":exec_id}),
            })?;
            let has_more = replay["has_more"].as_bool().unwrap_or(false);
            let mut records = Vec::with_capacity(events.len());
            for value in events {
                let record = self
                    .agent_work
                    .validate_replay_event(&execution, value)
                    .map_err(|error| RuntimeRpcError {
                        code: 503,
                        message: "AGENT_STOP_REPLAY_INVALID".to_string(),
                        data: json!({"exec_id":exec_id,"detail":error.to_string()}),
                    })?;
                let terminal = record.terminal;
                records.push(record);
                if terminal {
                    break;
                }
            }
            let page_terminal = records.last().is_some_and(|record| record.terminal);
            if !records.is_empty() {
                let mut db = self.open_db().await.map_err(|error| RuntimeRpcError {
                    code: 503,
                    message: "AGENT_STOP_REPLAY_COMMIT_FAILED".to_string(),
                    data: json!({"exec_id":exec_id,"detail":error.to_string()}),
                })?;
                let ack_through = self
                    .agent_work
                    .apply_replay_batch(&mut db, &execution, &records)
                    .await
                    .map_err(|error| RuntimeRpcError {
                        code: 503,
                        message: "AGENT_STOP_REPLAY_COMMIT_FAILED".to_string(),
                        data: json!({"exec_id":exec_id,"detail":error.to_string()}),
                    })?;
                commit_db(db).await.map_err(|error| RuntimeRpcError {
                    code: 503,
                    message: "AGENT_STOP_REPLAY_COMMIT_FAILED".to_string(),
                    data: json!({"exec_id":exec_id,"detail":error.to_string()}),
                })?;
                if let Some(through) = ack_through {
                    acked = acked.max(through);
                }
            }
            if acked > 0 {
                self.runtime
                    .modules_events_ack(
                        handle,
                        &lifecycle_request(
                            current,
                            format!("stop:{}:ack:{exec_id}:{acked}", current.operation_id),
                        ),
                        exec_id,
                        acked,
                    )
                    .await?;
            }
            if page_terminal {
                attempt_settled = true;
                break;
            }
            if !has_more || records.is_empty() {
                break;
            }
        }
        if attempt_settled {
            return Ok(());
        }

        let status = self
            .runtime
            .modules_status(
                handle,
                &lifecycle_request(
                    current,
                    format!(
                        "stop:{}:settled-status:{exec_id}:{acked}",
                        current.operation_id
                    ),
                ),
                exec_id,
            )
            .await?;
        if status["status"].as_str() == Some("cancelled") {
            return self
                .settle_stop_outcome(
                    &execution,
                    StoppedExecutionOutcome::Cancelled {
                        reason: status["reason"].clone(),
                    },
                )
                .await;
        }
        if status["status"].as_str() == Some("finished")
            && status["persistence"].as_str() == Some("failed")
        {
            let pending = self
                .runtime
                .modules_outcomes_pending(
                    handle,
                    &lifecycle_request(
                        current,
                        format!("stop:{}:pending:{exec_id}:{acked}", current.operation_id),
                    ),
                )
                .await?;
            let outcome = pending["pending"]
                .as_array()
                .into_iter()
                .flatten()
                .find(|entry| {
                    entry["exec_id"].as_str() == Some(exec_id)
                        && entry["source"].as_str() == Some("memory")
                })
                .and_then(|entry| entry.get("outcome"))
                .cloned()
                .ok_or_else(|| RuntimeRpcError {
                    code: 503,
                    message: "AGENT_STOP_OUTCOME_MISSING".to_string(),
                    data: json!({"exec_id":exec_id}),
                })?;
            return self
                .settle_stop_outcome(&execution, StoppedExecutionOutcome::Recovered { outcome })
                .await;
        }
        if status["status"].as_str() == Some("unknown") {
            return self
                .settle_stop_outcome(&execution, StoppedExecutionOutcome::Missing)
                .await;
        }
        Err(RuntimeRpcError {
            code: 503,
            message: "AGENT_STOP_OUTCOME_MISSING".to_string(),
            data: json!({"exec_id":exec_id,"status":status}),
        })
    }

    /// Settle one stopped execution at the task authority in its own
    /// committed transaction.
    async fn settle_stop_outcome(
        &self,
        execution: &StoppedExecution,
        outcome: StoppedExecutionOutcome,
    ) -> Result<(), RuntimeRpcError> {
        let exec_id = execution.exec_id.as_str();
        let recovery_failed = |error: String| RuntimeRpcError {
            code: 503,
            message: "AGENT_STOP_RECOVERY_FAILED".to_string(),
            data: json!({"exec_id":exec_id,"detail":error}),
        };
        let mut db = self
            .open_db()
            .await
            .map_err(|error| recovery_failed(error.to_string()))?;
        self.agent_work
            .settle_stopped_execution(&mut db, execution, outcome)
            .await
            .map_err(|error| recovery_failed(error.to_string()))?;
        commit_db(db)
            .await
            .map_err(|error| recovery_failed(error.to_string()))?;
        Ok(())
    }

    pub async fn transfer(
        &self,
        workspace_id: Uuid,
        actor_user_id: Uuid,
        to_user_id: Uuid,
        idempotency_key: &str,
    ) -> Result<TransferOutcome, G5Error> {
        validate_idempotency_key(idempotency_key)?;
        let digest = hex::encode(Sha256::digest(
            format!("{workspace_id}:{actor_user_id}:{to_user_id}").as_bytes(),
        ));
        // The owner's calls in flight end before ownership moves, as they
        // did under the admission share lock; the wait repeats under the
        // workspace row (`g5.runtime.guest-call-admission`).
        self.runtime.drain_in_flight(workspace_id).await;
        let (transfer_id, from_user_id) = {
            let mut db = self.open_db().await?;
            // Fork admission takes the owner-limit lock before any workspace
            // row lock; so does this, before the workspace row.
            workspace_owner_limit::lock_owner(db.conn(), to_user_id).await?;
            let workspace = sqlx::query(
                "SELECT org_id, owner_user_id, is_base
                 FROM workspaces WHERE id = $1 AND deleted_at IS NULL FOR UPDATE",
            )
            .bind(workspace_id)
            .fetch_optional(db.conn())
            .await?
            .ok_or_else(|| G5Error::NotFound {
                resource: format!("workspace {workspace_id}"),
            })?;
            if !self.runtime.drain_in_flight(workspace_id).await {
                return Err(guest_calls_outlived_their_budget(
                    &self.runtime,
                    workspace_id,
                    "transfer",
                ));
            }
            let org_id = workspace.try_get::<Uuid, _>("org_id")?;
            // A personal workspace stays bound to its user (`g5.personal.refusals`).
            crate::services::workspace::workspace_kind::refuse_personal(
                db.conn(),
                workspace_id,
                "ownership transfer",
            )
            .await?;
            let current_owner_user_id = workspace
                .try_get::<Option<Uuid>, _>("owner_user_id")?
                .ok_or_else(|| internal("workspace owner is unresolved"))?;
            lock_transfer_memberships(db.conn(), org_id, actor_user_id, to_user_id).await?;
            let outcome = if let Some(existing) = sqlx::query(
                "SELECT id, from_user_id, to_user_id, request_digest, status, completed_at
                 FROM workspace_ownership_transfers
                 WHERE workspace_id = $1 AND requested_by = $2 AND idempotency_key = $3
                 FOR UPDATE",
            )
            .bind(workspace_id)
            .bind(actor_user_id)
            .bind(idempotency_key)
            .fetch_optional(db.conn())
            .await?
            {
                let from_user_id = existing.try_get::<Uuid, _>("from_user_id")?;
                if existing.try_get::<String, _>("request_digest")? != digest
                    || existing.try_get::<Uuid, _>("to_user_id")? != to_user_id
                {
                    return Err(G5Error::Conflict {
                        reason: "IDEMPOTENCY_KEY_REUSED".to_string(),
                    });
                }
                let status = existing.try_get::<String, _>("status")?;
                if status == "completed" {
                    let outcome = TransferOutcome {
                        transfer_id: existing.try_get("id")?,
                        workspace_id,
                        from_user_id,
                        to_user_id,
                        requested_by: actor_user_id,
                        status,
                        completed_at: existing.try_get("completed_at")?,
                    };
                    commit_db(db).await?;
                    self.ws_hub
                        .revoke_workspace_access(
                            &self.system_pool,
                            self.scope.clone(),
                            workspace_id,
                            Some(from_user_id),
                        )
                        .await;
                    return Ok(outcome);
                }
                if status == "aborted" {
                    return Err(G5Error::Conflict {
                        reason: "previous transfer attempt was aborted".to_string(),
                    });
                }
                if current_owner_user_id != from_user_id {
                    let error = G5Error::Conflict {
                        reason: "workspace owner changed during transfer".to_string(),
                    };
                    abort_transfer_in_tx(db.conn(), existing.try_get("id")?, &error).await?;
                    commit_db(db).await?;
                    return Err(error);
                }
                let actor_role = sqlx::query_scalar::<_, String>(
                    "SELECT role FROM authz.lock_org_membership($1, $2)
                     WHERE deactivated_at IS NULL",
                )
                .bind(org_id)
                .bind(actor_user_id)
                .fetch_optional(db.conn())
                .await?;
                if actor_role.is_none()
                    || (actor_user_id != current_owner_user_id
                        && !matches!(actor_role.as_deref(), Some("admin")))
                {
                    let error = G5Error::Forbidden {
                        missing_scopes: Vec::new(),
                    };
                    abort_transfer_in_tx(db.conn(), existing.try_get("id")?, &error).await?;
                    commit_db(db).await?;
                    return Err(error);
                }
                let target_role = sqlx::query_scalar::<_, String>(
                    "SELECT role FROM authz.lock_org_membership($1, $2)
                     WHERE deactivated_at IS NULL",
                )
                .bind(org_id)
                .bind(to_user_id)
                .fetch_optional(db.conn())
                .await?;
                if !matches!(target_role.as_deref(), Some("admin" | "member")) {
                    let error = G5Error::ValidationError {
                        details: json!({"field":"toUserId","message":"target membership changed during transfer"}),
                    };
                    abort_transfer_in_tx(db.conn(), existing.try_get("id")?, &error).await?;
                    commit_db(db).await?;
                    return Err(error);
                }
                (existing.try_get("id")?, from_user_id)
            } else {
                if current_owner_user_id == to_user_id {
                    return Err(G5Error::ValidationError {
                        details: json!({"field":"toUserId","message":"target already owns the workspace"}),
                    });
                }
                let actor_role = sqlx::query_scalar::<_, String>(
                    "SELECT role FROM authz.lock_org_membership($1, $2)
                     WHERE deactivated_at IS NULL",
                )
                .bind(org_id)
                .bind(actor_user_id)
                .fetch_optional(db.conn())
                .await?;
                if actor_role.is_none()
                    || (actor_user_id != current_owner_user_id
                        && !matches!(actor_role.as_deref(), Some("admin")))
                {
                    return Err(G5Error::Forbidden {
                        missing_scopes: Vec::new(),
                    });
                }
                let target_role = sqlx::query_scalar::<_, String>(
                    "SELECT role FROM authz.lock_org_membership($1, $2)
                     WHERE deactivated_at IS NULL",
                )
                .bind(org_id)
                .bind(to_user_id)
                .fetch_optional(db.conn())
                .await?;
                if !matches!(target_role.as_deref(), Some("admin" | "member")) {
                    return Err(G5Error::ValidationError {
                        details: json!({"field":"toUserId","message":"target is not an eligible organization member"}),
                    });
                }
                // One pending transfer per workspace, decided under the
                // workspace lock (`g5.lifecycle.transfer.admission`).
                let another_pending = sqlx::query_scalar::<_, bool>(
                    "SELECT EXISTS (
                         SELECT 1 FROM workspace_ownership_transfers
                         WHERE workspace_id = $1 AND status = 'pending'
                     )",
                )
                .bind(workspace_id)
                .fetch_one(db.conn())
                .await?;
                if another_pending {
                    return Err(G5Error::Conflict {
                        reason: "another ownership transfer is pending for this workspace"
                            .to_string(),
                    });
                }
                // A new transfer counts for the recipient at once
                // (`g5.workspace.owner-limit`); a resumed one is not refused.
                workspace_owner_limit::ensure_below_limit(db.conn(), org_id, to_user_id).await?;
                let transfer_id = Uuid::new_v4();
                sqlx::query(
                    "INSERT INTO workspace_ownership_transfers(
                         id, org_id, workspace_id, from_user_id, to_user_id,
                         requested_by, idempotency_key, request_digest)
                     VALUES ($1,$2,$3,$4,$5,$6,$7,$8)",
                )
                .bind(transfer_id)
                .bind(org_id)
                .bind(workspace_id)
                .bind(current_owner_user_id)
                .bind(to_user_id)
                .bind(actor_user_id)
                .bind(idempotency_key)
                .bind(&digest)
                .execute(db.conn())
                .await?;
                // A pending transfer invalidates the old owner's drain even if
                // ownership later returns to that actor. Preserve the visible
                // marker for explicit recovery by the current owner.
                sqlx::query("UPDATE workspaces SET mutation_policy_transition_id=CASE WHEN mutation_policy_transition='freezing' THEN gen_random_uuid() ELSE NULL END WHERE id=$1")
                    .bind(workspace_id).execute(db.conn()).await?;
                (transfer_id, current_owner_user_id)
            };
            sqlx::query(
                "UPDATE workspace_vms
                 SET start_intent = CASE WHEN start_intent IS NULL THEN NULL
                     ELSE jsonb_set(start_intent, '{status}', '\"cancelled\"') END,
                     updated_at = now()
                 WHERE workspace_id = $1",
            )
            .bind(workspace_id)
            .execute(db.conn())
            .await?;
            commit_db(db).await?;
            outcome
        };

        let operation_id = transfer_id;
        let stopped = match self
            .stop_as_system(workspace_id, operation_id, StopReason::Transfer, None)
            .await
        {
            Ok(stopped) => stopped,
            Err(error) => {
                let mut db = self.open_db().await?;
                sqlx::query(
                    "UPDATE workspace_ownership_transfers
                     SET status = 'aborted', error = jsonb_build_object('message',$2)
                     WHERE id = $1 AND status = 'pending'",
                )
                .bind(transfer_id)
                .bind(error.to_string())
                .execute(db.conn())
                .await?;
                commit_db(db).await?;
                return Err(error);
            }
        };
        if stopped.status != "stopped" {
            return Ok(TransferOutcome {
                transfer_id,
                workspace_id,
                from_user_id,
                to_user_id,
                requested_by: actor_user_id,
                status: "pending".to_string(),
                completed_at: None,
            });
        }

        let mut db = self.open_db().await?;
        let workspace = sqlx::query(
            "SELECT org_id, owner_user_id FROM workspaces
             WHERE id = $1 AND deleted_at IS NULL FOR UPDATE",
        )
        .bind(workspace_id)
        .fetch_optional(db.conn())
        .await?
        .ok_or_else(|| G5Error::NotFound {
            resource: format!("workspace {workspace_id}"),
        })?;
        lock_transfer_memberships(
            db.conn(),
            workspace.try_get("org_id")?,
            actor_user_id,
            to_user_id,
        )
        .await?;
        let transfer = sqlx::query(
            "SELECT status, from_user_id, to_user_id, requested_by, request_digest, completed_at
             FROM workspace_ownership_transfers WHERE id = $1 FOR UPDATE",
        )
        .bind(transfer_id)
        .fetch_one(db.conn())
        .await?;
        let transfer_status = transfer.try_get::<String, _>("status")?;
        if transfer_status == "completed" {
            let outcome = TransferOutcome {
                transfer_id,
                workspace_id,
                from_user_id,
                to_user_id,
                requested_by: actor_user_id,
                status: transfer_status,
                completed_at: transfer.try_get("completed_at")?,
            };
            commit_db(db).await?;
            self.ws_hub
                .revoke_workspace_access(
                    &self.system_pool,
                    self.scope.clone(),
                    workspace_id,
                    Some(from_user_id),
                )
                .await;
            return Ok(outcome);
        }
        if transfer_status != "pending"
            || transfer.try_get::<Uuid, _>("from_user_id")? != from_user_id
            || transfer.try_get::<Uuid, _>("to_user_id")? != to_user_id
            || transfer.try_get::<Uuid, _>("requested_by")? != actor_user_id
            || transfer.try_get::<String, _>("request_digest")? != digest
        {
            let error = G5Error::Conflict {
                reason: "transfer operation changed during lifecycle drain".to_string(),
            };
            abort_transfer_in_tx(db.conn(), transfer_id, &error).await?;
            commit_db(db).await?;
            return Err(error);
        }
        if workspace.try_get::<Option<Uuid>, _>("owner_user_id")? != Some(from_user_id) {
            let error = G5Error::Conflict {
                reason: "workspace owner changed during transfer".to_string(),
            };
            abort_transfer_in_tx(db.conn(), transfer_id, &error).await?;
            commit_db(db).await?;
            return Err(error);
        }
        let org_id = workspace.try_get::<Uuid, _>("org_id")?;
        let actor_role = crate::services::PermissionService::require_active_member_on_conn(
            db.conn(),
            actor_user_id,
            org_id,
            false,
        )
        .await;
        if actor_role.is_err()
            || (actor_user_id != from_user_id
                && !matches!(actor_role, Ok(g5_contracts::types::enums::OrgRole::Admin)))
        {
            let error = G5Error::Forbidden {
                missing_scopes: Vec::new(),
            };
            abort_transfer_in_tx(db.conn(), transfer_id, &error).await?;
            commit_db(db).await?;
            return Err(error);
        }
        let target_valid = sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(SELECT 1 FROM authz.lock_org_membership($1, $2)
                           WHERE deactivated_at IS NULL
                             AND role IN ('admin','member'))",
        )
        .bind(workspace.try_get::<Uuid, _>("org_id")?)
        .bind(to_user_id)
        .fetch_one(db.conn())
        .await?;
        if !target_valid {
            let error = G5Error::ValidationError {
                details: json!({"field":"toUserId","message":"target membership changed during transfer"}),
            };
            abort_transfer_in_tx(db.conn(), transfer_id, &error).await?;
            commit_db(db).await?;
            return Err(error);
        }
        let vm_active = sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(SELECT 1 FROM workspace_vms
                           WHERE workspace_id = $1
                             AND status IN ('starting','running','stopping'))",
        )
        .bind(workspace_id)
        .fetch_one(db.conn())
        .await?;
        if vm_active {
            commit_db(db).await?;
            return Ok(TransferOutcome {
                transfer_id,
                workspace_id,
                from_user_id,
                to_user_id,
                requested_by: actor_user_id,
                status: "pending".to_string(),
                completed_at: None,
            });
        }
        self.agent_work
            .hold_tasks_for_transfer(&mut db, workspace_id)
            .await
            .map_err(app_error)?;
        sqlx::query(
            "UPDATE workspaces
             SET owner_user_id = $2, lifecycle_generation = lifecycle_generation + 1,
                 updated_at = now()
             WHERE id = $1 AND owner_user_id = $3",
        )
        .bind(workspace_id)
        .bind(to_user_id)
        .bind(from_user_id)
        .execute(db.conn())
        .await?;
        sqlx::query(
            "UPDATE workspace_vms SET start_intent = NULL, updated_at = now()
             WHERE workspace_id = $1",
        )
        .bind(workspace_id)
        .execute(db.conn())
        .await?;
        // The new owner has not let this workspace's agent change other
        // workspaces (`g5.security.agent-other-workspaces`).
        sqlx::query(
            "UPDATE workspace_settings
             SET agent_may_modify_other_workspaces = false, updated_by = $2, updated_at = now()
             WHERE workspace_id = $1 AND agent_may_modify_other_workspaces",
        )
        .bind(workspace_id)
        .bind(actor_user_id)
        .execute(db.conn())
        .await?;
        // Open/closed PRs inherit the new owner's default. A completed merge
        // must never stop the new owner's VM, even if their default is on.
        // This shares the workspace lock held by setting an explicit choice.
        sqlx::query(
            "UPDATE workspace_pull_requests
             SET stop_vm_after_merge=CASE WHEN status='merged' THEN false ELSE NULL END,
                 updated_at=now()
             WHERE source_workspace_id=$1 AND stop_vm_after_merge IS DISTINCT FROM
                 CASE WHEN status='merged' THEN false ELSE NULL END",
        )
        .bind(workspace_id)
        .execute(db.conn())
        .await?;
        let completed_at: DateTime<Utc> = sqlx::query_scalar(
            "UPDATE workspace_ownership_transfers
             SET status = 'completed', completed_at = now(), error = NULL
             WHERE id = $1 AND status = 'pending' RETURNING completed_at",
        )
        .bind(transfer_id)
        .fetch_one(db.conn())
        .await?;
        sqlx::query(
            "INSERT INTO notifications(
                 workspace_id, recipient_user_id, actor_user_id, category,
                 severity, title, message, actions)
             VALUES
                 ($1, $2, $4, 'workspace_lifecycle', 'info',
                  'Workspace ownership transferred',
                  'You transferred this workspace to another organization member.', '[]'::jsonb),
                 ($1, $3, $4, 'workspace_lifecycle', 'info',
                  'Workspace ownership transferred to you',
                  'You now own this workspace. Start it when you are ready to continue.', '[]'::jsonb)",
        )
        .bind(workspace_id)
        .bind(from_user_id)
        .bind(to_user_id)
        .bind(actor_user_id)
        .execute(db.conn())
        .await?;
        commit_db(db).await?;
        self.ws_hub
            .revoke_workspace_access(
                &self.system_pool,
                self.scope.clone(),
                workspace_id,
                Some(from_user_id),
            )
            .await;
        Ok(TransferOutcome {
            transfer_id,
            workspace_id,
            from_user_id,
            to_user_id,
            requested_by: actor_user_id,
            status: "completed".to_string(),
            completed_at: Some(completed_at),
        })
    }

    /// Discover unprocessed obligations and due admission retries. A stopped/held
    /// workspace drops out of the page; failures defer behind unattempted work.
    pub async fn reconcile_member_deactivation(&self, limit: i64) -> Result<u64, G5Error> {
        let mut db = self.open_db().await?;
        let rows = sqlx::query(
            "SELECT id, org_id, owner_user_id, cleanup_id FROM authz.pending_member_cleanup_workspaces($1)",
        )
        .bind(limit.clamp(1, 32))
        .fetch_all(db.conn())
        .await?;
        rollback_db(db).await?;
        let mut admitted = 0;
        for row in rows {
            let workspace_id: Uuid = row.try_get("id")?;
            let owner: Uuid = row.try_get("owner_user_id")?;
            let cleanup: Uuid = row.try_get("cleanup_id")?;
            match self
                .admit_stop(
                    workspace_id,
                    None,
                    cleanup,
                    StopReason::MemberDeactivated,
                    None,
                    Some((owner, cleanup)),
                    false,
                )
                .await
            {
                Ok(StopAdmission::Ready(current)) => {
                    // Stop's existing claim executor owns physical shutdown.
                    match self.defer_stop_to_tunnel_owner(&current).await {
                        Ok(()) => admitted += 1,
                        Err(error) => tracing::warn!(
                            %workspace_id, %error, "member cleanup stop deferral will retry"
                        ),
                    }
                }
                Ok(_) => admitted += 1,
                Err(error) => {
                    // Admission's transaction has rolled back. Persist scheduling
                    // separately so a repeatedly failing first page cannot starve
                    // later work, even after this worker restarts.
                    match self
                        .defer_member_cleanup_admission(workspace_id, owner, cleanup)
                        .await
                    {
                        Ok(true) => tracing::warn!(
                            %workspace_id, %error, "member cleanup admission deferred for retry"
                        ),
                        Ok(false) => {}
                        Err(retry_error) => tracing::warn!(
                            %workspace_id, %error, %retry_error,
                            "could not persist member cleanup admission retry"
                        ),
                    }
                }
            }
        }
        // Only scan settled memberships here. Pending ones never monopolize
        // this bounded completion page, including memberships without workspaces.
        let mut db = self.open_db().await?;
        let memberships: Vec<(Uuid, Uuid, Uuid)> = sqlx::query_as(
            "SELECT org_id, user_id, cleanup_id FROM authz.settled_member_cleanups($1)",
        )
        .bind(limit.clamp(1, 32))
        .fetch_all(db.conn())
        .await?;
        for (org, user, cleanup) in memberships {
            sqlx::query("SELECT authz.finish_member_cleanup($1,$2,$3)")
                .bind(org)
                .bind(user)
                .bind(cleanup)
                .execute(db.conn())
                .await?;
        }
        commit_db(db).await?;
        Ok(admitted)
    }

    async fn defer_member_cleanup_admission(
        &self,
        workspace_id: Uuid,
        expected_owner: Uuid,
        expected_cleanup: Uuid,
    ) -> Result<bool, G5Error> {
        let mut db = self.open_db().await?;
        let org: Option<Uuid> = sqlx::query_scalar(
            "SELECT org_id FROM workspaces WHERE id=$1 AND owner_user_id=$2 FOR UPDATE",
        )
        .bind(workspace_id)
        .bind(expected_owner)
        .fetch_optional(db.conn())
        .await?;
        let Some(org) = org else {
            rollback_db(db).await?;
            return Ok(false);
        };
        let cleanup: Option<Uuid> = sqlx::query_scalar(
            "SELECT deactivation_cleanup_id FROM authz.lock_org_membership($1,$2)",
        )
        .bind(org)
        .bind(expected_owner)
        .fetch_optional(db.conn())
        .await?
        .flatten();
        if cleanup != Some(expected_cleanup) {
            rollback_db(db).await?;
            return Ok(false);
        }
        // Another worker may have admitted the stop after this attempt failed.
        // Only defer obligations that still need admission under the locked owner
        // and cleanup generation; never replace membership or shutdown state.
        let updated = sqlx::query(
            "UPDATE workspaces w SET member_cleanup_retry_id=$2,
                 member_cleanup_retry_at=clock_timestamp()+interval '30 seconds'
             WHERE w.id=$1 AND (
                 EXISTS (SELECT 1 FROM workspace_vms vm WHERE vm.workspace_id=w.id AND (
                     vm.status IN ('starting','running')
                     OR vm.start_intent->>'status' IN ('pending','queued_for_capacity','starting')))
                 OR EXISTS (SELECT 1 FROM tasks t WHERE t.workspace_id=w.id AND t.status='queued'))",
        )
        .bind(workspace_id)
        .bind(expected_cleanup)
        .execute(db.conn())
        .await?
        .rows_affected();
        commit_db(db).await?;
        Ok(updated != 0)
    }

    /// Reconcile lease expiry and lifecycle crashes. Membership cleanup retains
    /// custody until termination is confirmed. Other expired incarnations use
    /// the existing logical release policy. A stop that had not reached the host
    /// command records the resulting unflushed loss; one that had reached it
    /// completes normally. Other active incarnations fail closed.
    pub async fn reconcile_expired_leases(&self) -> Result<u64, G5Error> {
        let mut db = self.open_db().await?;
        let updated_workspace_ids =
            crate::services::hosts::workspace_vm_release::release_expired_leases(
                self.agent_work.as_ref(),
                db.conn(),
            )
            .await?;
        commit_db(db).await?;
        Ok(updated_workspace_ids.len() as u64)
    }

    /// Retry durable lifecycle work after an API crash or an ambiguous host
    /// reply. The recorded operation and incarnation are reused verbatim.
    pub async fn reconcile_stopping(&self, limit: i64) -> Result<u64, G5Error> {
        let mut db = self.open_db().await?;
        let rows = sqlx::query(
            "SELECT workspace_id, stop_operation_id, stop_reason, stop_deadline_at,
                    stop_retry_at
             FROM workspace_vms
             WHERE status = 'stopping' AND stop_operation_id IS NOT NULL
               AND (stop_retry_at IS NULL OR stop_retry_at <= now())
             ORDER BY COALESCE(stop_retry_at, updated_at), workspace_id
             LIMIT $1",
        )
        .bind(limit.clamp(1, 32))
        .fetch_all(db.conn())
        .await?;
        rollback_db(db).await?;
        let mut reconciled = 0_u64;
        for row in rows {
            let workspace_id = row.try_get("workspace_id")?;
            let operation_id = row
                .try_get::<Option<Uuid>, _>("stop_operation_id")?
                .ok_or_else(|| internal("stopping workspace operation is absent"))?;
            let reason = StopReason::parse(
                row.try_get::<Option<String>, _>("stop_reason")?
                    .as_deref()
                    .ok_or_else(|| internal("stopping workspace reason is absent"))?,
            )?;
            let deadline_at = row.try_get("stop_deadline_at")?;
            tracing::info!(
                %workspace_id,
                %operation_id,
                reason = reason.as_str(),
                claim_lapsed_for_secs = row
                    .try_get::<Option<DateTime<Utc>>, _>("stop_retry_at")?
                    .map(|due| (Utc::now() - due).num_seconds()),
                "workspace stop resumed by the lifecycle reconciler"
            );
            match self
                .stop_as_system(workspace_id, operation_id, reason, deadline_at)
                .await
            {
                Ok(_) => reconciled += 1,
                Err(error) => tracing::warn!(
                    %workspace_id,
                    %operation_id,
                    %error,
                    "workspace stop reconciliation remains pending"
                ),
            }
        }
        Ok(reconciled)
    }

    /// Begin a scheduled stop only after its owner-visible nudge deadline.
    /// Until then the VM remains running and dispatcher admission stays open.
    pub async fn reconcile_pending_stops(&self, limit: i64) -> Result<u64, G5Error> {
        let mut db = self.open_db().await?;
        let rows = sqlx::query(
            "SELECT workspace_id, stop_operation_id, pending_stop_reason,
                    pending_stop_deadline_at
             FROM workspace_vms
             WHERE status IN ('starting','running')
               AND stop_operation_id IS NOT NULL
               AND pending_stop_reason IS NOT NULL
               AND pending_stop_deadline_at <= now()
             ORDER BY pending_stop_deadline_at, workspace_id
             LIMIT $1",
        )
        .bind(limit.clamp(1, 32))
        .fetch_all(db.conn())
        .await?;
        rollback_db(db).await?;
        let mut reconciled = 0_u64;
        for row in rows {
            let workspace_id = row.try_get("workspace_id")?;
            let operation_id = row
                .try_get::<Option<Uuid>, _>("stop_operation_id")?
                .ok_or_else(|| internal("pending workspace stop operation is absent"))?;
            let reason = StopReason::parse(
                row.try_get::<Option<String>, _>("pending_stop_reason")?
                    .as_deref()
                    .ok_or_else(|| internal("pending workspace stop reason is absent"))?,
            )?;
            let _nudge_deadline: DateTime<Utc> = row.try_get("pending_stop_deadline_at")?;
            match self
                // The nudge is complete. Admission now creates an independent
                // flush/harvest deadline instead of reusing the already-due
                // owner-visible nudge time as a zero-length RPC budget.
                .stop_as_system(workspace_id, operation_id, reason, None)
                .await
            {
                Ok(_) => reconciled += 1,
                Err(error) => tracing::warn!(
                    %workspace_id,
                    %operation_id,
                    %error,
                    "workspace pending stop reconciliation remains pending"
                ),
            }
        }
        Ok(reconciled)
    }

    /// Resume ownership transfers whose request returned while the durable
    /// stop operation was still in flight. The original actor, target and
    /// idempotency key are reused, so this cannot mint replacement authority.
    pub async fn reconcile_pending_transfers(
        &self,
        limit: i64,
    ) -> Result<Vec<TransferOutcome>, G5Error> {
        let mut db = self.open_db().await?;
        let rows = sqlx::query(
            "SELECT workspace_id, requested_by, to_user_id, idempotency_key
             FROM workspace_ownership_transfers
             WHERE status = 'pending'
             ORDER BY requested_at, id
             LIMIT $1",
        )
        .bind(limit.clamp(1, 32))
        .fetch_all(db.conn())
        .await?;
        rollback_db(db).await?;
        let mut completed = Vec::new();
        for row in rows {
            let workspace_id: Uuid = row.try_get("workspace_id")?;
            match self
                .transfer(
                    workspace_id,
                    row.try_get("requested_by")?,
                    row.try_get("to_user_id")?,
                    row.try_get::<String, _>("idempotency_key")?.as_str(),
                )
                .await
            {
                Ok(outcome) if outcome.status == "completed" => completed.push(outcome),
                Ok(_) | Err(G5Error::WorkspaceStopping) => {}
                Err(error) => tracing::warn!(
                    %workspace_id,
                    %error,
                    "workspace transfer reconciliation remains pending"
                ),
            }
        }
        Ok(completed)
    }

    /// Re-drive explicitly authorized starts in FIFO order. A queued row is
    /// only an effect of its durable owner/generation-bound start intent.
    pub async fn reconcile_start_queue(&self, limit: i64) -> Result<u64, G5Error> {
        let mut db = self.open_db().await?;
        let expired = sqlx::query(
            "SELECT org_id, workspace_id, host_id, vm_id, epoch,
                    start_command_sent_at IS NOT NULL AS start_sent
             FROM workspace_vms
             WHERE status = 'starting' AND start_deadline_at <= now()
               AND host_id = ANY($2::uuid[])
               AND start_intent ->> 'status' = 'starting'
               AND (start_intent ->> 'expires_at')::timestamptz > now()
             ORDER BY start_deadline_at, workspace_id
             LIMIT $1",
        )
        .bind(limit.clamp(1, 32))
        .bind(self.hosts.reachable_control_host_ids())
        .fetch_all(db.conn())
        .await?;
        rollback_db(db).await?;
        for row in expired {
            let org_id: Uuid = row.try_get("org_id")?;
            let workspace_id: Uuid = row.try_get("workspace_id")?;
            let host_id: Uuid = row.try_get("host_id")?;
            let vm_id = Uuid::parse_str(row.try_get::<String, _>("vm_id")?.as_str())
                .map_err(|_| internal("starting workspace VM identity is invalid"))?;
            let epoch = u64::try_from(row.try_get::<i64, _>("epoch")?)
                .map_err(|_| internal("starting workspace epoch is negative"))?;
            // Read before asking the host: a Start stamped as sent is ordered
            // before this status on the connection, or belongs to an earlier
            // connection whose every line the host read before this hello;
            // an unsent one is fenced from being sent by the release below
            // (`g5.runtime.start.settlement`).
            let start_sent: bool = row.try_get("start_sent")?;
            let command = CommandParams {
                idempotency_key: format!("start-status:{}", Uuid::new_v4()),
                budget_ms: 15_000,
                command: VmStatus {
                    workspace_id,
                    vm_id,
                },
            };
            let observed = match self
                .hosts
                .get(&host_id)
                .and_then(|tunnel| tunnel.command_client())
            {
                Some(client) => {
                    client
                        .execute(method::VM_STATUS, &command, Duration::from_secs(16))
                        .await
                }
                None => Err(crate::ws::host_ctl::HostCtlError::Closed),
            };
            match observed {
                Ok(CommandResult::NotFound | CommandResult::Stopped { .. }) => {
                    let mut db = self.open_db().await?;
                    self.placement
                        .release_placement(
                            &mut db,
                            &PlacementReservation {
                                host_id,
                                vm_id,
                                epoch,
                                occupancy_before: 0,
                                previous_activity_at: None,
                            },
                            org_id,
                            workspace_id,
                            start_sent,
                            "start_absent_on_host",
                            None,
                        )
                        .await
                        .map_err(app_error)?;
                    commit_db(db).await?;
                }
                Ok(CommandResult::Status { vm })
                    if vm.workspace_id == workspace_id
                        && vm.vm_id == vm_id
                        && vm.epoch == epoch =>
                {
                    self.defer_start_reconciliation(workspace_id, host_id, vm_id, epoch)
                        .await?;
                }
                Ok(other) => {
                    tracing::warn!(
                        %workspace_id,
                        %host_id,
                        %vm_id,
                        ?other,
                        "workspace start status remained ambiguous"
                    );
                    self.defer_start_reconciliation(workspace_id, host_id, vm_id, epoch)
                        .await?;
                }
                Err(error) => {
                    tracing::warn!(
                        %workspace_id,
                        %host_id,
                        %vm_id,
                        %error,
                        "workspace start status could not be observed"
                    );
                    self.defer_start_reconciliation(workspace_id, host_id, vm_id, epoch)
                        .await?;
                }
            }
        }

        let mut db = self.open_db().await?;
        sqlx::query(
            "UPDATE workspace_vms
             SET start_intent = jsonb_set(start_intent, '{status}', '\"expired\"'),
                 updated_at = now()
             WHERE status IN ('stopped','failed')
               AND start_intent ->> 'status' IN ('pending','queued_for_capacity')
               AND (start_intent ->> 'expires_at')::timestamptz <= now()",
        )
        .execute(db.conn())
        .await?;
        let rows = sqlx::query(
            "SELECT vm.workspace_id, vm.start_intent
             FROM workspace_vms AS vm
             JOIN workspaces AS workspace ON workspace.id = vm.workspace_id
             WHERE vm.status IN ('stopped','failed')
               AND vm.start_intent ->> 'status' IN ('pending','queued_for_capacity')
               AND (vm.start_intent ->> 'expires_at')::timestamptz > now()
               AND (vm.start_intent ->> 'owner_user_id')::uuid = workspace.owner_user_id
               AND (vm.start_intent ->> 'lifecycle_generation')::bigint = workspace.lifecycle_generation
               AND workspace.deleted_at IS NULL
               AND authz.workspace_execution_allowed(workspace.id)
               AND NOT EXISTS (
                   SELECT 1 FROM workspace_ownership_transfers transfer
                   WHERE transfer.workspace_id = workspace.id
                     AND transfer.status = 'pending'
               )
             ORDER BY (vm.start_intent ->> 'created_at')::timestamptz, vm.workspace_id
             LIMIT $1",
        )
        .bind(limit.clamp(1, 32))
        .fetch_all(db.conn())
        .await?;
        commit_db(db).await?;
        let mut reconciled = 0_u64;
        for row in rows {
            let workspace_id: Uuid = row.try_get("workspace_id")?;
            let intent: Value = row.try_get("start_intent")?;
            let intent_id = intent["id"]
                .as_str()
                .and_then(|value| Uuid::parse_str(value).ok())
                .ok_or_else(|| internal("workspace start intent id is invalid"))?;
            let owner_user_id = intent["owner_user_id"]
                .as_str()
                .and_then(|value| Uuid::parse_str(value).ok())
                .ok_or_else(|| internal("workspace start intent owner is invalid"))?;
            let kind = StartKind::parse(
                intent["kind"]
                    .as_str()
                    .ok_or_else(|| internal("workspace start intent kind is absent"))?,
            )?;
            match self
                .resume_queued(workspace_id, owner_user_id, intent_id, kind)
                .await
            {
                Ok(_) => reconciled += 1,
                Err(error) => tracing::warn!(
                    %workspace_id,
                    %intent_id,
                    %error,
                    "workspace start reconciliation remains pending"
                ),
            }
        }
        Ok(reconciled)
    }

    /// Start a workspace again for the chat turns a platform-caused
    /// interruption left behind (`g5.lifecycle.start.automatic-continuation`).
    /// Each ended incarnation is decided once; the decision is recorded on
    /// `workspace_vms.automatic_continuation`, with the consent when the
    /// decision is to start. A lost VM counts toward the loss cap, a drain
    /// does not. Returns the number of automatic starts recorded by this pass.
    pub async fn reconcile_automatic_continuations(&self, limit: i64) -> Result<u64, G5Error> {
        let mut db = self.open_db().await?;
        let rows = sqlx::query(
            "SELECT vm.workspace_id, workspace.owner_user_id, vm.epoch, vm.automatic_continuation,
                    vm.start_intent ->> 'status' AS intent_status,
                    vm.start_intent ->> 'kind' AS intent_kind,
                    vm.start_intent ? 'consumed_vm_id' AS intent_ran
             FROM workspace_vms AS vm
             JOIN workspaces AS workspace ON workspace.id = vm.workspace_id
             WHERE vm.status IN ('stopped','failed')
               AND workspace.deleted_at IS NULL
               AND NOT workspace.is_base
               AND workspace.owner_user_id IS NOT NULL
               AND COALESCE((SELECT (value #>> '{}')::boolean FROM configs
                             WHERE org_id = workspace.org_id AND scope = 'org'
                               AND scope_id = workspace.org_id AND key = $2), true)
               AND (vm.start_intent IS NULL
                    OR vm.start_intent ->> 'status' NOT IN ('pending','queued_for_capacity','starting'))
               AND (vm.automatic_continuation IS NULL
                    OR (vm.automatic_continuation ->> 'epoch')::bigint IS DISTINCT FROM vm.epoch)
               AND EXISTS (
                   SELECT 1 FROM tasks AS task
                   WHERE task.workspace_id = vm.workspace_id AND task.kind = 'agent'
                     AND task.status IN ('queued','running','waiting')
               )
             ORDER BY vm.updated_at, vm.workspace_id
             LIMIT $1",
        )
        .bind(limit.clamp(1, 32))
        // An organization that turned automatic continuation off leaves its
        // interrupted workspaces to their owners' Start; nothing is decided.
        .bind(crate::services::identity::org_admin_settings::AUTOMATIC_CONTINUATION_KEY)
        .fetch_all(db.conn())
        .await?;
        rollback_db(db).await?;
        let mut recorded = 0_u64;
        for row in rows {
            let workspace_id: Uuid = row.try_get("workspace_id")?;
            let owner_user_id: Uuid = row.try_get("owner_user_id")?;
            let epoch: i64 = row.try_get("epoch")?;
            let count = row
                .try_get::<Option<Value>, _>("automatic_continuation")?
                .and_then(|record| record["count"].as_i64())
                .unwrap_or(0);
            // The row's last consent, the owner's or the platform's, ended
            // without its VM ever running (it expired waiting for capacity, a
            // Stop cancelled it, or the boot failed): whoever gave it decided,
            // and the platform does not renew it with consent of its own. A
            // consent Ready consumed ran its VM (`consumed_vm_id`); a stop that
            // later cancels that record does not make it a lapsed one.
            let intent_status: Option<String> = row.try_get("intent_status")?;
            let intent_ran: bool = row
                .try_get::<Option<bool>, _>("intent_ran")?
                .unwrap_or(false);
            if !intent_ran
                && matches!(
                    intent_status.as_deref(),
                    Some("expired" | "cancelled" | "failed")
                )
            {
                let decided = self
                    .decide_automatic_continuation(
                        workspace_id,
                        automatic_continuation_record(
                            epoch,
                            "ended",
                            Some("consent_lapsed"),
                            count,
                            None,
                        ),
                    )
                    .await?;
                // The platform's own consent lapsed: say so. A cancelled one
                // was a Stop, and a lapsed owner Start is the owner's.
                let intent_kind: Option<String> = row.try_get("intent_kind")?;
                let why = match intent_status.as_deref() {
                    Some("expired") => Some("no host had room for it within an hour"),
                    Some("failed") => Some("the restarted workspace failed to boot"),
                    _ => None,
                };
                if let (true, Some("automatic_continuation"), Some(why)) =
                    (decided, intent_kind.as_deref(), why)
                {
                    self.announce_automatic_continuation(
                        workspace_id,
                        owner_user_id,
                        "restart",
                        AutomaticContinuationNews::NotStarted(why),
                    )
                    .await;
                }
                continue;
            }
            let mut db = self.open_db().await?;
            let work = self
                .agent_work
                .interrupted_work(db.conn(), workspace_id)
                .await?;
            rollback_db(db).await?;
            let Some(cause) = work.cause() else {
                if work == InterruptedWork::None {
                    // Nothing a platform interruption left behind: decide this
                    // incarnation once so it is not read again every pass.
                    self.decide_automatic_continuation(
                        workspace_id,
                        automatic_continuation_record(epoch, "none", None, count, None),
                    )
                    .await?;
                }
                continue;
            };
            let loss = work == InterruptedWork::VmLost;
            if loss && count >= AUTOMATIC_CONTINUATION_LOSS_CAP {
                if self
                    .decide_automatic_continuation(
                        workspace_id,
                        automatic_continuation_record(
                            epoch,
                            "cap_reached",
                            Some(cause),
                            count,
                            None,
                        ),
                    )
                    .await?
                {
                    tracing::warn!(
                        %workspace_id,
                        epoch,
                        count,
                        "automatic continuation refused: the workspace lost its VM too often since the owner's last Start"
                    );
                    self.announce_automatic_continuation(
                        workspace_id,
                        owner_user_id,
                        cause,
                        AutomaticContinuationNews::NotStarted(
                            "it has already been started again twice since your last Start",
                        ),
                    )
                    .await;
                }
                continue;
            }
            let intent_id = Uuid::new_v4();
            let record = automatic_continuation_record(
                epoch,
                "recorded",
                Some(cause),
                if loss { count + 1 } else { count },
                Some(intent_id),
            );
            match self
                .resume_recording(
                    workspace_id,
                    owner_user_id,
                    intent_id,
                    StartKind::AutomaticContinuation,
                    IntentAdmission::Fresh,
                    Some(&record),
                )
                .await
            {
                Ok(_) => {
                    recorded += 1;
                    tracing::info!(%workspace_id, %intent_id, cause, "automatic continuation started");
                    self.announce_automatic_continuation(
                        workspace_id,
                        owner_user_id,
                        cause,
                        AutomaticContinuationNews::Started,
                    )
                    .await;
                }
                Err(error) if self.start_intent_is_live(workspace_id, intent_id).await? => {
                    // Consent and decision are recorded; placement failed and
                    // the start queue retries it until the consent expires.
                    recorded += 1;
                    tracing::info!(%workspace_id, %intent_id, %error, "automatic continuation waits for placement");
                    self.announce_automatic_continuation(
                        workspace_id,
                        owner_user_id,
                        cause,
                        AutomaticContinuationNews::Started,
                    )
                    .await;
                }
                Err(error) if automatic_continuation_retryable(&error) => {
                    tracing::warn!(%workspace_id, %error, "automatic continuation deferred; retrying");
                }
                Err(G5Error::Conflict { reason }) if reason == AUTOMATIC_CONTINUATION_NO_WORK => {
                    // The owner ended the last interrupted turn meanwhile:
                    // their decision, decided as none and not announced.
                    self.decide_automatic_continuation(
                        workspace_id,
                        automatic_continuation_record(epoch, "none", None, count, None),
                    )
                    .await?;
                }
                Err(error) => {
                    // A definite refusal, before the consent was recorded or,
                    // as placement's owner limit does, after it, ending it.
                    let refusal = automatic_continuation_refusal(&error);
                    let decided = self
                        .refuse_automatic_continuation(
                            workspace_id,
                            intent_id,
                            automatic_continuation_record(
                                epoch,
                                &format!("refused:{refusal}"),
                                Some(cause),
                                count,
                                None,
                            ),
                        )
                        .await?;
                    tracing::warn!(%workspace_id, %error, "automatic continuation refused");
                    if decided {
                        self.announce_automatic_continuation(
                            workspace_id,
                            owner_user_id,
                            cause,
                            AutomaticContinuationNews::NotStarted(refusal_phrase(refusal)),
                        )
                        .await;
                    }
                }
            }
        }
        Ok(recorded)
    }

    /// Record a decision other than a start for one ended incarnation, once:
    /// the row must still be that incarnation, stopped or failed, undecided
    /// and without live consent, so an owner Start or a Stop committed since
    /// the scan wins and the pass reads the row again later.
    async fn decide_automatic_continuation(
        &self,
        workspace_id: Uuid,
        record: Value,
    ) -> Result<bool, G5Error> {
        let mut db = self.open_db().await?;
        let decided = sqlx::query(
            "UPDATE workspace_vms
             SET automatic_continuation = $2, updated_at = now()
             WHERE workspace_id = $1 AND status IN ('stopped','failed')
               AND epoch = ($2 ->> 'epoch')::bigint
               AND (automatic_continuation IS NULL
                    OR (automatic_continuation ->> 'epoch')::bigint IS DISTINCT FROM epoch)
               AND (start_intent IS NULL
                    OR start_intent ->> 'status' NOT IN ('pending','queued_for_capacity','starting'))",
        )
        .bind(workspace_id)
        .bind(&record)
        .execute(db.conn())
        .await?
        .rows_affected()
            == 1;
        commit_db(db).await?;
        Ok(decided)
    }

    /// Tell the owner what the platform did about the interruption: a notice
    /// in every chat with interrupted work and a workspace notification,
    /// stored and pushed live (`g5.lifecycle.start.automatic-continuation`).
    async fn announce_automatic_continuation(
        &self,
        workspace_id: Uuid,
        owner_user_id: Uuid,
        cause: &str,
        news: AutomaticContinuationNews,
    ) {
        // The decision is durable by now; a notice that cannot be written is
        // logged, not allowed to abort the pass.
        if let Err(error) = self
            .try_announce_automatic_continuation(workspace_id, owner_user_id, cause, news)
            .await
        {
            tracing::warn!(%workspace_id, %error, "automatic continuation notice not delivered");
        }
    }

    async fn try_announce_automatic_continuation(
        &self,
        workspace_id: Uuid,
        owner_user_id: Uuid,
        cause: &str,
        news: AutomaticContinuationNews,
    ) -> Result<(), G5Error> {
        let interruption = match cause {
            "host_stop" => "the workspace's host was drained",
            "vm_lost" => "the workspace VM was lost",
            _ => "the workspace was interrupted",
        };
        let (title, message, severity) = match news {
            AutomaticContinuationNews::Started => (
                "Workspace started again",
                format!("Interrupted: {interruption}. G5 started the workspace again and the interrupted chat continues."),
                "info",
            ),
            AutomaticContinuationNews::NotStarted(why) => (
                "Workspace stays stopped",
                format!("Interrupted: {interruption}. G5 did not start the workspace again: {why}. Press Start to continue."),
                "warning",
            ),
        };
        let operation_id = Uuid::new_v4();
        let mut db = self.open_db().await?;
        self.agent_work
            .announce_automatic_continuation(
                db.conn(),
                workspace_id,
                &message,
                operation_id,
                json!({"automatic_continuation": cause}),
            )
            .await?;
        let notification_id: Uuid = sqlx::query_scalar(
            "INSERT INTO notifications(
                 workspace_id, recipient_user_id, actor_user_id, category,
                 severity, title, message, actions)
             VALUES ($1, $2, NULL, 'workspace_lifecycle', $3, $4, $5, '[]'::jsonb)
             RETURNING id",
        )
        .bind(workspace_id)
        .bind(owner_user_id)
        .bind(severity)
        .bind(title)
        .bind(&message)
        .fetch_one(db.conn())
        .await?;
        commit_db(db).await?;
        let event = crate::ws::WsEvent::Notification {
            workspace_id,
            notification: crate::ws::events::NotificationPayload {
                id: notification_id,
                severity: severity.to_string(),
                category: "workspace_lifecycle".to_string(),
                title: title.to_string(),
                message,
                task_id: None,
                comment_id: None,
                reply_id: None,
                actions: Vec::new(),
                created_at: Utc::now(),
                expires_at: None,
            },
        };
        self.ws_hub
            .publish(&crate::ws::Channel::User(owner_user_id), &event);
        self.ws_hub.publish_to_user(
            &crate::ws::Channel::Workspace(workspace_id),
            owner_user_id,
            &event,
        );
        Ok(())
    }

    /// Record a refusal for the incarnation: over the consent's own record
    /// when that consent was recorded and then ended (placement's owner limit
    /// fails it in place), otherwise as a fresh fenced decision.
    async fn refuse_automatic_continuation(
        &self,
        workspace_id: Uuid,
        intent_id: Uuid,
        record: Value,
    ) -> Result<bool, G5Error> {
        let mut db = self.open_db().await?;
        let over_consent = sqlx::query(
            "UPDATE workspace_vms
             SET automatic_continuation = $2, updated_at = now()
             WHERE workspace_id = $1
               AND automatic_continuation ->> 'intent_id' = $3::text
               AND start_intent ->> 'id' = $3::text
               AND start_intent ->> 'status' NOT IN ('pending','queued_for_capacity','starting')",
        )
        .bind(workspace_id)
        .bind(&record)
        .bind(intent_id.to_string())
        .execute(db.conn())
        .await?
        .rows_affected()
            == 1;
        commit_db(db).await?;
        if over_consent {
            return Ok(true);
        }
        self.decide_automatic_continuation(workspace_id, record)
            .await
    }

    /// Whether `intent_id` is the row's consent and still waits or starts:
    /// placement may have ended it in place (the owner limit fails it).
    async fn start_intent_is_live(
        &self,
        workspace_id: Uuid,
        intent_id: Uuid,
    ) -> Result<bool, G5Error> {
        let mut db = self.open_db().await?;
        let matches = sqlx::query_scalar::<_, Option<bool>>(
            "SELECT start_intent ->> 'id' = $2::text
                    AND start_intent ->> 'status' IN ('pending','queued_for_capacity','starting')
             FROM workspace_vms WHERE workspace_id = $1",
        )
        .bind(workspace_id)
        .bind(intent_id.to_string())
        .fetch_optional(db.conn())
        .await?
        .flatten()
        .unwrap_or(false);
        rollback_db(db).await?;
        Ok(matches)
    }

    /// Initiate deadline-bound stop operations for hosts whose durable state
    /// has made them unavailable for new work.
    pub async fn reconcile_host_stop_triggers(&self, limit: i64) -> Result<u64, G5Error> {
        let mut db = self.open_db().await?;
        let rows = sqlx::query(
            "SELECT vm.workspace_id, 'drain'::text AS reason
             FROM workspace_vms AS vm
             JOIN worker_hosts AS host ON host.host_id = vm.host_id
             WHERE vm.status IN ('starting','running')
               AND host.status = 'draining'
             ORDER BY vm.workspace_id
             LIMIT $1",
        )
        .bind(limit.clamp(1, 32))
        .fetch_all(db.conn())
        .await?;
        rollback_db(db).await?;
        let mut admitted = 0_u64;
        for row in rows {
            let workspace_id: Uuid = row.try_get("workspace_id")?;
            let reason = StopReason::parse(row.try_get::<String, _>("reason")?.as_str())?;
            match self
                .stop_as_system(workspace_id, Uuid::new_v4(), reason, None)
                .await
            {
                Ok(_) => admitted += 1,
                Err(G5Error::WorkspaceStopping) => {}
                Err(error) => tracing::warn!(
                    %workspace_id,
                    %error,
                    "workspace host stop trigger remains pending"
                ),
            }
        }
        Ok(admitted)
    }

    /// Stop a retired personal workspace's compute without a save window
    /// (`g5.personal.retirement`). The membership change that retired it has
    /// already committed; this is its best-effort follow-up, and
    /// [`Self::reconcile_retired_personal`] retries it.
    pub async fn stop_retired_personal(
        &self,
        workspace_id: Uuid,
        operation_id: Uuid,
    ) -> Result<StopOutcome, G5Error> {
        self.stop_as_system(
            workspace_id,
            operation_id,
            StopReason::OwnerRemoved,
            Some(Utc::now()),
        )
        .await
    }

    /// Admit the stop of every retired personal workspace whose VM is still
    /// starting or running. Stops already admitted resume through
    /// [`Self::reconcile_stopping`].
    pub async fn reconcile_retired_personal(&self, limit: i64) -> Result<u64, G5Error> {
        let mut db = self.open_db().await?;
        let rows = sqlx::query_as::<_, (Uuid, Option<Uuid>)>(
            "SELECT vm.workspace_id, workspace.deletion_operation_id
             FROM workspace_vms AS vm
             JOIN workspaces AS workspace ON workspace.id = vm.workspace_id
             WHERE vm.status IN ('starting','running')
               AND workspace.deleted_at IS NOT NULL
               AND workspace.personal_user_id IS NOT NULL
             ORDER BY vm.updated_at, vm.workspace_id
             LIMIT $1",
        )
        .bind(limit.clamp(1, 32))
        .fetch_all(db.conn())
        .await?;
        rollback_db(db).await?;
        let mut admitted = 0_u64;
        for (workspace_id, operation_id) in rows {
            match self
                .stop_retired_personal(workspace_id, operation_id.unwrap_or_else(Uuid::new_v4))
                .await
            {
                Ok(_) => admitted += 1,
                Err(error) => {
                    tracing::warn!(%workspace_id, %error, "retired personal workspace stop deferred");
                }
            }
        }
        Ok(admitted)
    }

    pub async fn reconcile_idle(&self, limit: i64) -> Result<u64, G5Error> {
        let candidates = self.idle_candidates(limit).await?;
        let mut admitted = 0_u64;
        for workspace_id in candidates {
            match self
                .stop_as_system(workspace_id, Uuid::new_v4(), StopReason::Idle, None)
                .await
            {
                Ok(outcome) if outcome.status == "running" => {}
                Ok(_) => admitted += 1,
                Err(error) => {
                    self.mark_idle_retry(workspace_id, &error.to_string())
                        .await?;
                    tracing::warn!(%workspace_id, %error, "idle workspace stop deferred");
                }
            }
        }
        Ok(admitted)
    }

    /// Mark idle running workspaces for ordinary stop processing
    /// Stop admission rechecks the predicate
    /// under the row lock, so work that appears between selection and
    /// admission is not interrupted.
    pub async fn idle_candidates(&self, limit: i64) -> Result<Vec<Uuid>, G5Error> {
        let mut db = self.open_db().await?;
        let query = format!(
            "SELECT * FROM ({}) AS decision
             WHERE expired AND retry_deferred IS NOT TRUE AND cardinality(work_exemptions) = 0
             ORDER BY activity_since, workspace_id LIMIT $1",
            idle_decision_query(),
        );
        let rows = sqlx::query(&query)
            .bind(limit.clamp(1, 100))
            .fetch_all(db.conn())
            .await?;
        rollback_db(db).await?;
        let correlation_id = Uuid::new_v4().to_string();
        let mut candidates = Vec::with_capacity(rows.len());
        for row in rows {
            log_idle_decision(&row, &correlation_id, "selection")?;
            candidates.push(row.try_get("workspace_id")?);
        }
        Ok(candidates)
    }

    /// Sample one bounded page of expired workspaces, at most once per minute.
    /// Eligible candidates have a separate SQL limit, so an exempt population
    /// cannot starve Stop selection. The cursor rotates diagnostic coverage.
    /// Returns the number of rows examined, including non-exempt rows.
    pub async fn log_idle_exemptions(
        &self,
        diagnostics: &mut IdleExemptionDiagnostics,
        limit: i64,
    ) -> Result<usize, G5Error> {
        if !diagnostics.begin_scan(tokio::time::Instant::now()) {
            return Ok(0);
        }
        let limit = limit.clamp(1, 100);
        let mut db = self.open_db().await?;
        let rows = sqlx::query(&format!(
            "{} AND COALESCE(vm.last_activity_at, vm.started_at, vm.updated_at)
                     + make_interval(secs => settings.idle_timeout_secs) <= clock_timestamp()
                 AND ($1::uuid IS NULL OR vm.workspace_id > $1)
             ORDER BY vm.workspace_id LIMIT $2",
            idle_decision_query(),
        ))
        .bind(diagnostics.after_workspace)
        .bind(limit)
        .fetch_all(db.conn())
        .await?;
        rollback_db(db).await?;
        let correlation_id = Uuid::new_v4().to_string();
        for row in &rows {
            let work: Vec<String> = row.try_get("work_exemptions")?;
            let retry: Option<bool> = row.try_get("retry_deferred")?;
            if !work.is_empty() || retry == Some(true) {
                log_idle_decision(row, &correlation_id, "diagnostic")?;
            }
        }
        diagnostics.after_workspace = if rows.len() == limit as usize {
            rows.last()
                .map(|row| row.try_get("workspace_id"))
                .transpose()?
        } else {
            None
        };
        Ok(rows.len())
    }

    pub async fn mark_idle_retry(&self, workspace_id: Uuid, error: &str) -> Result<(), G5Error> {
        let mut db = self.open_db().await?;
        sqlx::query(
            "UPDATE workspace_vms
             SET idle_retry_after = now() + make_interval(secs => $2),
                 last_error = jsonb_set(
                     COALESCE(last_error, '{}'::jsonb),
                     '{latest}',
                     jsonb_build_object('reason','idle_stop_failed','detail',$3),
                     true),
                 updated_at = now()
             WHERE workspace_id = $1 AND status = 'running'",
        )
        .bind(workspace_id)
        .bind(i32::try_from(DEFAULT_IDLE_RETRY_SECS).unwrap_or(i32::MAX))
        .bind(error)
        .execute(db.conn())
        .await?;
        commit_db(db).await?;
        Ok(())
    }

    async fn record_start_intent(
        &self,
        workspace_id: Uuid,
        actor_user_id: Uuid,
        intent_id: Uuid,
        kind: StartKind,
        admission: IntentAdmission,
        automatic_record: Option<&Value>,
    ) -> Result<WorkspaceBootInput, G5Error> {
        let mut db = self.open_db().await?;
        // The VM checks out one committed cut (`g5.fork.source.cut`): hold the
        // commit fence while the member heads are read. It precedes every row
        // lock in this short, database-only transaction.
        if !crate::services::branch_commit::acquire_workspace_commit_fence_bounded(
            db.conn(),
            workspace_id,
        )
        .await?
        {
            return Err(crate::services::branch_commit::commit_fence_busy(
                "workspace_start",
            ));
        }
        let row = sqlx::query(
            "SELECT workspace.org_id, workspace.owner_user_id, workspace.is_base,
                    workspace.branch_name, workspace.lifecycle_generation,
                    COALESCE(settings.idle_timeout_secs, 28800) AS idle_timeout_secs,
                    COALESCE((SELECT value #>> '{}' FROM configs
                              WHERE org_id = workspace.org_id AND scope = 'org'
                                AND scope_id = workspace.org_id AND key = 'worker_host_region'),
                             'us-central1') AS region
             FROM workspaces AS workspace
             LEFT JOIN workspace_settings AS settings ON settings.workspace_id = workspace.id
             WHERE workspace.id = $1 AND workspace.deleted_at IS NULL
             FOR UPDATE OF workspace",
        )
        .bind(workspace_id)
        .fetch_optional(db.conn())
        .await?
        .ok_or_else(|| G5Error::NotFound {
            resource: format!("workspace {workspace_id}"),
        })?;
        let owner_user_id = row.try_get::<Option<Uuid>, _>("owner_user_id")?;
        if owner_user_id != Some(actor_user_id) {
            return Err(G5Error::Forbidden {
                missing_scopes: Vec::new(),
            });
        }
        if row.try_get::<bool, _>("is_base")? {
            return Err(G5Error::WorkspaceIsBase);
        }
        sqlx::query("SELECT g5_require_workspace_not_deleting($1)")
            .bind(workspace_id)
            .execute(db.conn())
            .await?;
        // Read after the workspace lock is held, in its own statement: a
        // transfer admitted while this Start waited for the lock is otherwise
        // invisible to a subquery evaluated on the earlier snapshot
        // (`g5.lifecycle.transfer.admission`).
        let pending_transfer = sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS (
                 SELECT 1 FROM workspace_ownership_transfers transfer
                 WHERE transfer.workspace_id = $1 AND transfer.status = 'pending'
             )",
        )
        .bind(workspace_id)
        .fetch_one(db.conn())
        .await?;
        if pending_transfer {
            return Err(G5Error::WorkspaceStopping);
        }
        let branch_name = row
            .try_get::<Option<String>, _>("branch_name")?
            .ok_or_else(|| G5Error::Conflict {
                reason: "workspace branch identity is unresolved".to_string(),
            })?;
        let org_id = row.try_get::<Uuid, _>("org_id")?;
        crate::services::PermissionService::require_active_member_on_conn(
            db.conn(),
            actor_user_id,
            org_id,
            true,
        )
        .await?;
        let org_default_image_ref = sqlx::query_scalar::<_, Option<String>>(
            "SELECT default_base_image_ref FROM orgs WHERE id = $1",
        )
        .bind(org_id)
        .fetch_optional(&self.system_pool)
        .await?
        .flatten()
        .ok_or_else(|| G5Error::Config("organization has no default workspace image".into()))?;
        let lifecycle_generation_i64 = row.try_get::<i64, _>("lifecycle_generation")?;
        let lifecycle_generation = u64::try_from(lifecycle_generation_i64)
            .map_err(|_| internal("workspace lifecycle generation is negative"))?;
        let owner_profile = sqlx::query("SELECT display_name, email FROM users WHERE id = $1")
            .bind(actor_user_id)
            .fetch_optional(&self.system_pool)
            .await?
            .ok_or_else(|| G5Error::NotFound {
                resource: format!("user {actor_user_id}"),
            })?;
        let owner_display_name = owner_profile.try_get::<Option<String>, _>("display_name")?;
        let owner_email = owner_profile.try_get::<Option<String>, _>("email")?;
        let owner_git =
            GitAuthor::from_profile(owner_display_name.as_deref(), owner_email.as_deref());
        let current = sqlx::query(
            "SELECT status, start_intent, automatic_continuation FROM workspace_vms
             WHERE workspace_id = $1 FOR UPDATE",
        )
        .bind(workspace_id)
        .fetch_optional(db.conn())
        .await?;
        if kind == StartKind::AutomaticContinuation {
            // Under the row lock. A fresh automatic start decides its ended
            // incarnation here, with its consent, in one transaction: the row
            // must still be that incarnation, stopped or failed, undecided and
            // without live consent, so an owner Start, a Stop or another pass
            // that got here first supersedes it. A replay must own the intent
            // already recorded (`g5.lifecycle.start.automatic-continuation`).
            let already_recorded = current
                .as_ref()
                .and_then(|row| {
                    row.try_get::<Option<Value>, _>("start_intent")
                        .ok()
                        .flatten()
                })
                .is_some_and(|intent| intent["id"].as_str() == Some(&intent_id.to_string()));
            match automatic_record {
                Some(record) if !already_recorded => {
                    let decided = sqlx::query(
                        "UPDATE workspace_vms
                         SET automatic_continuation = $2, updated_at = now()
                         WHERE workspace_id = $1 AND status IN ('stopped','failed')
                           AND epoch = ($2 ->> 'epoch')::bigint
                           AND (automatic_continuation IS NULL
                                OR (automatic_continuation ->> 'epoch')::bigint IS DISTINCT FROM epoch)
                           AND (start_intent IS NULL
                                OR start_intent ->> 'status'
                                   NOT IN ('pending','queued_for_capacity','starting'))",
                    )
                    .bind(workspace_id)
                    .bind(record)
                    .execute(db.conn())
                    .await?
                    .rows_affected()
                        == 1;
                    if !decided {
                        return Err(G5Error::Conflict {
                            reason: "the ended incarnation was decided or started meanwhile"
                                .to_string(),
                        });
                    }
                }
                Some(_) => {}
                None if already_recorded => {}
                None => {
                    return Err(G5Error::Conflict {
                        reason: "automatic continuation is not the recorded consent".to_string(),
                    });
                }
            }
            // The consent is scoped to the interrupted turns: once the owner
            // has cancelled the last of them, a start, first or replayed,
            // would only run an empty workspace. A replay cancels the consent
            // it was given; a fresh start records nothing, so the decision
            // written above and its loss count are rolled back with it.
            if !self
                .agent_work
                .continuable_work_remains(db.conn(), workspace_id)
                .await?
            {
                if already_recorded {
                    sqlx::query(
                        "UPDATE workspace_vms
                         SET start_intent = jsonb_set(start_intent, '{status}', '\"cancelled\"'),
                             updated_at = now()
                         WHERE workspace_id = $1 AND start_intent ->> 'id' = $2::text
                           AND start_intent ->> 'status' IN ('pending', 'queued_for_capacity')",
                    )
                    .bind(workspace_id)
                    .bind(intent_id.to_string())
                    .execute(db.conn())
                    .await?;
                    commit_db(db).await?;
                } else {
                    rollback_db(db).await?;
                }
                return Err(G5Error::Conflict {
                    reason: AUTOMATIC_CONTINUATION_NO_WORK.to_string(),
                });
            }
        }
        if current.is_none() {
            sqlx::query("INSERT INTO workspace_vms(org_id, workspace_id) VALUES ($1, $2)")
                .bind(org_id)
                .bind(workspace_id)
                .execute(db.conn())
                .await?;
        }
        let status = current
            .as_ref()
            .map(|value| value.try_get::<String, _>("status"))
            .transpose()?
            .unwrap_or_else(|| "stopped".to_string());
        if status == "stopping" {
            return Err(G5Error::WorkspaceStopping);
        }
        let existing = current
            .and_then(|value| value.try_get::<Option<Value>, _>("start_intent").ok())
            .flatten();
        let now = Utc::now();
        let intent_id_string = intent_id.to_string();
        let existing_intent_id = existing
            .as_ref()
            .and_then(|existing| existing.get("id"))
            .and_then(Value::as_str);
        if admission == IntentAdmission::ReplayOnly
            && existing_intent_id != Some(intent_id_string.as_str())
        {
            // Checked under the VM row lock: a newer intent replaced the one
            // reconciliation captured, so the old one stays gone.
            return Err(G5Error::Conflict {
                reason: "queued workspace start was superseded".to_string(),
            });
        }
        if admission == IntentAdmission::FirstOnly
            && existing_intent_id.is_some_and(|id| id != intent_id_string.as_str())
        {
            return Err(G5Error::Conflict {
                reason:
                    "the workspace was started separately since it was forked; start it explicitly"
                        .to_string(),
            });
        }
        // Whether this call records new consent. An exact replay retains the
        // intent already recorded and decides nothing new, so neither it nor
        // attaching to running compute resets a decision made since.
        let mut records_new_consent = false;
        let mut selected_consent = intent_id;
        if let Some(existing) = existing.as_ref() {
            let existing_id = existing.get("id").and_then(Value::as_str);
            let existing_uuid = existing_id.and_then(|id| Uuid::parse_str(id).ok());
            if existing_id == Some(intent_id_string.as_str()) {
                // Exact replay retains the original durable intent.
            } else if kind.is_owner_consent()
                && live_owner_start_intent(Some(existing), Some(actor_user_id), now).is_some()
                && existing["lifecycle_generation"].as_u64() == Some(lifecycle_generation)
                && !matches!(status.as_str(), "starting" | "running")
            {
                // The owner's own Start is already waiting: this Start joins
                // it instead of superseding it, so the work bound to the
                // first (a Start-and-send input, a pending preview) is not
                // withdrawn by the owner pressing Start again
                // (`g5.preview.start-and-preview`). The consent is marked
                // joined, so the request that recorded it can no longer
                // cancel it should its own work fail. Placement is retried
                // below.
                selected_consent = existing_uuid.unwrap_or(intent_id);
                sqlx::query(
                    "UPDATE workspace_vms
                     SET start_intent = jsonb_set(start_intent, '{joined}', 'true'),
                         updated_at = now()
                     WHERE workspace_id = $1",
                )
                .bind(workspace_id)
                .execute(db.conn())
                .await?;
            } else if matches!(status.as_str(), "starting" | "running") {
                // The owner's own Start, plain or compound, attaches to
                // compute already starting or running under their consent;
                // a compound action then binds to the starting consent or
                // runs at once.
                let same_resume_authority = kind.is_owner_consent()
                    && existing["owner_user_id"]
                        .as_str()
                        .and_then(|value| Uuid::parse_str(value).ok())
                        == Some(actor_user_id)
                    && existing["lifecycle_generation"].as_u64() == Some(lifecycle_generation);
                if !same_resume_authority {
                    return Err(G5Error::Conflict {
                        reason: "workspace already has an active start intent".to_string(),
                    });
                }
                // A compound action binds its work to the Start it attaches
                // to, so it attaches only to one the owner may bind to: the
                // platform's continuation, or the owner's own Start past its
                // bounded lifetime, is not; the action is refused with the
                // reason instead of binding to a consent admission would
                // then refuse as ended.
                if kind == StartKind::CompoundStart
                    && status == "starting"
                    && live_owner_start_intent(Some(existing), Some(actor_user_id), now).is_none()
                {
                    return Err(G5Error::Conflict {
                        reason: "the workspace is starting under a Start this action cannot wait for; try again once it is running".to_string(),
                    });
                }
                selected_consent = existing_uuid.unwrap_or(intent_id);
                // A consent attached to while starting is joined like a
                // waiting one: should placement be refused and the consent
                // return to the capacity queue, the request that recorded it
                // can no longer cancel it. Only a live consent is written to:
                // the row validator refuses an expired active intent.
                if status == "starting"
                    && live_owner_start_intent(Some(existing), Some(actor_user_id), now).is_some()
                {
                    sqlx::query(
                        "UPDATE workspace_vms
                         SET start_intent = jsonb_set(start_intent, '{joined}', 'true'),
                             updated_at = now()
                         WHERE workspace_id = $1",
                    )
                    .bind(workspace_id)
                    .execute(db.conn())
                    .await?;
                }
            } else {
                records_new_consent = true;
                let intent = start_intent_value(
                    intent_id,
                    kind,
                    actor_user_id,
                    lifecycle_generation,
                    "pending",
                    now,
                    existing_id,
                );
                sqlx::query(
                    "UPDATE workspace_vms SET start_intent = $2, updated_at = now()
                     WHERE workspace_id = $1",
                )
                .bind(workspace_id)
                .bind(&intent)
                .execute(db.conn())
                .await?;
            }
        } else {
            records_new_consent = true;
            let intent = start_intent_value(
                intent_id,
                kind,
                actor_user_id,
                lifecycle_generation,
                if matches!(status.as_str(), "starting" | "running") {
                    "starting"
                } else {
                    "pending"
                },
                now,
                None,
            );
            sqlx::query(
                "UPDATE workspace_vms SET start_intent = $2, updated_at = now()
                 WHERE workspace_id = $1",
            )
            .bind(workspace_id)
            .bind(&intent)
            .execute(db.conn())
            .await?;
        }
        if kind.is_owner_consent() && records_new_consent {
            // The owner's own Start ends the platform's continuation lineage:
            // the next loss starts a fresh count
            // (`g5.lifecycle.start.automatic-continuation`). A replay leaves a
            // decision made since the original admission in place.
            sqlx::query(
                "UPDATE workspace_vms SET automatic_continuation = NULL
                 WHERE workspace_id = $1 AND automatic_continuation IS NOT NULL",
            )
            .bind(workspace_id)
            .execute(db.conn())
            .await?;
        }
        let repo_scope = self.repo_storage.scoped(&db);
        let repos = load_vm_repos(
            db.tx(),
            &repo_scope,
            org_id,
            workspace_id,
            &branch_name,
            actor_user_id,
            &self.git_external_url,
        )
        .await?;
        let mut restore = load_restore(db.tx(), workspace_id).await?;
        if let Some(receipt) =
            restore.take_if(|restore| restore_is_superseded_by_authoritative_heads(restore, &repos))
        {
            tracing::info!(
                workspace_id = %workspace_id,
                flush_id = %receipt.flush_id,
                "resume restores from the moved authoritative heads; the fully published flush receipt is superseded"
            );
        }
        let mut reference_repos = load_reference_repos(
            db.tx(),
            &repo_scope,
            org_id,
            workspace_id,
            &repos,
            &self.git_external_url,
        )
        .await?;
        reference_repos.extend(
            load_personal_mount(
                db.tx(),
                &repo_scope,
                org_id,
                workspace_id,
                &repos,
                &self.git_external_url,
            )
            .await?,
        );
        let reference_pins = reference_repos
            .iter()
            .filter(|repo| repo.role.as_deref() == Some("code"))
            .map(|repo| (repo.repo_id, repo.commit.clone()))
            .collect::<Vec<_>>();
        let (image_ref, image_digest, image_source, image_oid_vector) = resolve_workspace_image(
            db.tx(),
            repo_scope.clone(),
            org_id,
            workspace_id,
            &org_default_image_ref,
            &repos,
            restore.as_ref(),
            &reference_pins,
        )
        .await?;
        commit_db(db).await?;
        Ok(WorkspaceBootInput {
            org_id,
            workspace_id,
            owner_user_id: actor_user_id,
            consent_id: selected_consent,
            hostname: workspace_hostname(&branch_name),
            owner_git_name: owner_git.name,
            owner_git_email: owner_git.email,
            lifecycle_generation,
            image_ref,
            image_digest,
            image_source,
            image_oid_vector,
            idle_timeout_secs: u64::try_from(row.try_get::<i32, _>("idle_timeout_secs")?)
                .unwrap_or(28800),
            region: row.try_get("region")?,
            repos,
            restore,
            reference_repos,
        })
    }

    async fn build_spec(
        &self,
        input: &WorkspaceBootInput,
        reservation: &PlacementReservation,
    ) -> Result<VmSpec, G5Error> {
        let mut db = self.open_db().await?;
        let size = sqlx::query(
            "SELECT slot_cpus, slot_mem_mib, scratch_disk_mib
             FROM worker_hosts WHERE org_id = $1 AND host_id = $2 AND slot_size_reported",
        )
        .bind(input.org_id)
        .bind(reservation.host_id)
        .fetch_optional(db.conn())
        .await?
        .ok_or_else(|| G5Error::ServiceUnavailable {
            message: "placed worker host disappeared".to_string(),
        })?;
        let spec = VmSpec {
            spec_version: VM_SPEC_VERSION,
            dispatcher_contract_version: DISPATCHER_CONTRACT_VERSION,
            incarnation: VmIncarnationSpec {
                workspace_id: input.workspace_id,
                vm_id: reservation.vm_id,
                epoch: reservation.epoch,
                org_id: input.org_id,
                owner_user_id: input.owner_user_id,
                lease_ttl_secs: LEASE_TTL_SECS,
                idle_timeout_secs: input.idle_timeout_secs,
            },
            image: VmImageSpec {
                image_ref: input.image_ref.clone(),
                digest: input.image_digest.clone(),
            },
            size: VmSizeSpec {
                cpus: u8::try_from(size.try_get::<i32, _>("slot_cpus")?)
                    .map_err(|_| internal("host slot CPU count is out of range"))?,
                mem_mib: u32::try_from(size.try_get::<i32, _>("slot_mem_mib")?)
                    .map_err(|_| internal("host slot memory is out of range"))?,
                scratch_disk_mib: u32::try_from(size.try_get::<i32, _>("scratch_disk_mib")?)
                    .map_err(|_| internal("host scratch disk size is out of range"))?,
            },
            workspace_root: WorkspaceRootSpec {
                retain_hours: LEGACY_HOST_RETAIN_HOURS,
                cache_generation: input.lifecycle_generation,
            },
            repos: input.repos.clone(),
            restore: input.restore.clone(),
            reference_repos: input.reference_repos.clone(),
            env: BTreeMap::from([
                (WORKSPACE_HOSTNAME_ENV.to_string(), input.hostname.clone()),
                (
                    "G5D_OWNER_GIT_NAME".to_string(),
                    input.owner_git_name.clone(),
                ),
                (
                    "G5D_OWNER_GIT_EMAIL".to_string(),
                    input.owner_git_email.clone(),
                ),
                ("G5D_MAX_PTY_STREAMS".to_string(), "8".to_string()),
                ("G5D_MODULE_UID".to_string(), "1000".to_string()),
                ("G5D_MODULE_GID".to_string(), "1000".to_string()),
            ]),
            network: VmNetwork::Passt,
            entrypoint: vec!["/opt/g5/bin/g5d".into(), "dispatch".into()],
            module_catalog_version: catalog_version().to_string(),
            module_catalog_hash: MODULE_CATALOG_HASH.to_string(),
        };
        rollback_db(db).await?;
        Ok(spec)
    }

    async fn defer_start_reconciliation(
        &self,
        workspace_id: Uuid,
        host_id: Uuid,
        vm_id: Uuid,
        epoch: u64,
    ) -> Result<(), G5Error> {
        let mut db = self.open_db().await?;
        sqlx::query(
            "UPDATE workspace_vms
             SET start_deadline_at = LEAST(
                     now() + interval '30 seconds',
                     lease_expires_at,
                     (start_intent ->> 'expires_at')::timestamptz),
                 updated_at = now()
             WHERE workspace_id = $1 AND host_id = $2 AND vm_id = $3
               AND epoch = $4 AND status = 'starting'",
        )
        .bind(workspace_id)
        .bind(host_id)
        .bind(vm_id.to_string())
        .bind(i64::try_from(epoch).unwrap_or(i64::MAX))
        .execute(db.conn())
        .await?;
        commit_db(db).await?;
        Ok(())
    }

    /// Release an unadmitted placement; `start_sent` is the Start-delivery
    /// state this caller observed (`g5.runtime.start.settlement`).
    async fn release_start(
        &self,
        input: &WorkspaceBootInput,
        reservation: &PlacementReservation,
        start_sent: bool,
        reason: &str,
        host_reason: Option<&str>,
    ) -> Result<(), G5Error> {
        let mut db = self.open_db().await?;
        self.placement
            .release_placement(
                &mut db,
                reservation,
                input.org_id,
                input.workspace_id,
                start_sent,
                reason,
                host_reason,
            )
            .await
            .map_err(app_error)?;
        commit_db(db).await?;
        Ok(())
    }

    async fn fail_start(
        &self,
        input: &WorkspaceBootInput,
        reservation: &PlacementReservation,
        reason: &str,
    ) -> Result<(), G5Error> {
        let mut db = self.open_db().await?;
        let released = crate::services::hosts::workspace_vm_release::release_failed_start(
            self.agent_work.as_ref(),
            db.conn(),
            input.org_id,
            input.workspace_id,
            reservation.host_id,
            reservation.vm_id,
            reservation.epoch,
            reason,
        )
        .await?;
        commit_db(db).await?;
        if !released {
            tracing::info!(
                workspace_id = %input.workspace_id,
                vm_id = %reservation.vm_id,
                epoch = reservation.epoch,
                reason,
                "workspace boot failure arrived for an incarnation already released"
            );
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    async fn admit_stop(
        &self,
        workspace_id: Uuid,
        actor_user_id: Option<Uuid>,
        operation_id: Uuid,
        reason: StopReason,
        deadline_at: Option<DateTime<Utc>>,
        cleanup: Option<(Uuid, Uuid)>,
        deleting: bool,
    ) -> Result<StopAdmission, G5Error> {
        let claimed_at = tokio::time::Instant::now();
        // No owner call is interleaved with its stop admission: the calls in
        // flight against the incarnation end first, bounded by their budgets,
        // before any row is taken; the wait repeats under the workspace row
        // below, where a new admission can no longer slip past it
        // (`g5.runtime.guest-call-admission`).
        self.runtime.drain_in_flight(workspace_id).await;
        let mut db = self.open_db().await?;
        // Cleanup and system stops may find the row already tombstoned, as a
        // retired personal workspace's stop does (`g5.personal.retirement`); an
        // owner's stop needs a live row.
        let workspace = sqlx::query(
            "SELECT org_id, owner_user_id, is_base, lifecycle_generation
             FROM workspaces
             WHERE id = $1 AND (deleted_at IS NULL OR $2)
             FOR UPDATE",
        )
        .bind(workspace_id)
        .bind(cleanup.is_some() || actor_user_id.is_none())
        .fetch_optional(db.conn())
        .await?
        .ok_or_else(|| G5Error::NotFound {
            resource: format!("workspace {workspace_id}"),
        })?;
        if deleting {
            let reserved: Option<Uuid> =
                sqlx::query_scalar("SELECT deletion_operation_id FROM workspaces WHERE id=$1")
                    .bind(workspace_id)
                    .fetch_one(db.conn())
                    .await?;
            if reserved != Some(operation_id) {
                return Err(G5Error::Conflict {
                    reason: "Deletion was superseded".into(),
                });
            }
        }
        if !self.runtime.drain_in_flight(workspace_id).await {
            return Err(guest_calls_outlived_their_budget(
                &self.runtime,
                workspace_id,
                "stop",
            ));
        }
        let owner_user_id = workspace.try_get::<Option<Uuid>, _>("owner_user_id")?;
        if actor_user_id.is_some() && actor_user_id != owner_user_id {
            return Err(G5Error::Forbidden {
                missing_scopes: Vec::new(),
            });
        }
        let mut reason = reason;
        if let Some((expected_owner, cleanup_id)) = cleanup {
            let membership = sqlx::query(
                "SELECT deactivation_cleanup_id, deactivated_at IS NOT NULL AS deactivated
                 FROM authz.lock_org_membership($1, $2)",
            )
            .bind(workspace.try_get::<Uuid, _>("org_id")?)
            .bind(expected_owner)
            .fetch_optional(db.conn())
            .await?;
            let current = match &membership {
                Some(row) => row.try_get::<Option<Uuid>, _>("deactivation_cleanup_id")?,
                None => None,
            };
            if owner_user_id != Some(expected_owner) || current != Some(cleanup_id) {
                return Err(G5Error::Conflict {
                    reason: "Membership cleanup was superseded".into(),
                });
            }
            // One execution hold serves deactivation and stop by user; the
            // stop records which one drove it, read under the membership lock.
            let deactivated = match &membership {
                Some(row) => row.try_get::<bool, _>("deactivated")?,
                None => false,
            };
            reason = if deactivated {
                StopReason::MemberDeactivated
            } else {
                StopReason::ExecutionHold
            };
            self.agent_work
                .prepare_tasks_for_stop(&mut db, workspace_id, reason.as_str(), operation_id)
                .await
                .map_err(app_error)?;
            sqlx::query(
                "UPDATE workspaces SET member_cleanup_retry_id=NULL, member_cleanup_retry_at=NULL
                 WHERE id=$1 AND member_cleanup_retry_id IS NOT NULL",
            )
            .bind(workspace_id)
            .execute(db.conn())
            .await?;
        }
        if workspace.try_get::<bool, _>("is_base")? {
            if cleanup.is_some() {
                commit_db(db).await?;
                return Ok(StopAdmission::AlreadyStopped);
            }
            return Err(G5Error::WorkspaceIsBase);
        }
        let Some(row) = sqlx::query(
            "SELECT workspace_id, status, host_id, vm_id, epoch,
                    stop_operation_id, stop_deadline_at,
                    stop_reason, started_at, stop_command_sent_at,
                    pending_stop_reason, pending_stop_deadline_at,
                    stop_retry_at > now() AS claimed
             FROM workspace_vms WHERE workspace_id = $1 FOR UPDATE",
        )
        .bind(workspace_id)
        .fetch_optional(db.conn())
        .await?
        else {
            commit_db(db).await?;
            return Ok(StopAdmission::AlreadyStopped);
        };
        let status = row.try_get::<String, _>("status")?;
        if matches!(status.as_str(), "stopped" | "failed") {
            // Stopping a workspace that only waits for capacity cancels that
            // wait, so a host freeing up later does not start it anyway.
            // A drain or rollout re-admitted after the VM already ended (the
            // host's exit event or another replica completed it) leaves an
            // automatic continuation alone: it is what those stops lead to.
            let decides = !reason.continues_automatically();
            sqlx::query(
                "UPDATE workspace_vms
                 SET start_intent = jsonb_set(start_intent, '{status}', '\"cancelled\"'),
                     updated_at = now()
                 WHERE workspace_id = $1
                   AND start_intent ->> 'status' IN ('pending', 'queued_for_capacity', 'starting')
                   AND ($2 OR start_intent ->> 'kind' <> 'automatic_continuation')",
            )
            .bind(workspace_id)
            .bind(decides)
            .execute(db.conn())
            .await?;
            if decides {
                // The Stop also decides the ended incarnation: the platform
                // does not start it again for the turns an interruption left
                // behind (`g5.lifecycle.start.automatic-continuation`). Written
                // under the VM row lock, it fences a continuation claimed
                // concurrently.
                sqlx::query(
                    "UPDATE workspace_vms
                     SET automatic_continuation = jsonb_build_object(
                             'epoch', epoch, 'outcome', 'stopped', 'cause', $2::text,
                             'count', COALESCE((automatic_continuation ->> 'count')::bigint, 0),
                             'at', now()),
                         updated_at = now()
                     WHERE workspace_id = $1",
                )
                .bind(workspace_id)
                .bind(reason.as_str())
                .execute(db.conn())
                .await?;
            }
            commit_db(db).await?;
            return Ok(StopAdmission::AlreadyStopped);
        }
        if status == "stopping" {
            if cleanup.is_some() {
                commit_db(db).await?;
                return Ok(StopAdmission::Claimed);
            }
            let existing = row.try_get::<Option<Uuid>, _>("stop_operation_id")?;
            if existing != Some(operation_id) {
                let now = Utc::now();
                if reason == StopReason::Delete
                    && deadline_at.is_some_and(|deadline| deadline <= now)
                {
                    let generation = workspace.try_get::<i64, _>("lifecycle_generation")? + 1;
                    sqlx::query(
                        "UPDATE workspaces
                         SET lifecycle_generation = $2, updated_at = now()
                         WHERE id = $1",
                    )
                    .bind(workspace_id)
                    .bind(generation)
                    .execute(db.conn())
                    .await?;
                    sqlx::query(
                        "UPDATE workspace_vms
                         SET stop_reason = 'delete', stop_operation_id = $2,
                             stop_generation = $3, stop_deadline_at = $4,
                             stop_retry_at = now(),
                             pending_stop_reason = NULL,
                             pending_stop_deadline_at = NULL, nudged_at = NULL,
                             updated_at = now()
                         WHERE workspace_id = $1 AND status = 'stopping'",
                    )
                    .bind(workspace_id)
                    .bind(operation_id)
                    .bind(generation)
                    .bind(deadline_at.unwrap_or(now))
                    .execute(db.conn())
                    .await?;
                    let claimed_until = claim_stop(db.conn(), workspace_id, operation_id).await?;
                    let current = CurrentStop {
                        workspace_id,
                        host_id: row
                            .try_get::<Option<Uuid>, _>("host_id")?
                            .ok_or_else(|| internal("stopping workspace host is absent"))?,
                        vm_id: row
                            .try_get::<Option<String>, _>("vm_id")?
                            .ok_or_else(|| internal("stopping workspace VM is absent"))?
                            .parse()
                            .map_err(|_| internal("stopping workspace VM id is invalid"))?,
                        epoch: u64::try_from(row.try_get::<i64, _>("epoch")?)
                            .map_err(|_| internal("workspace epoch is negative"))?,
                        reason,
                        requires_flush: row
                            .try_get::<Option<DateTime<Utc>>, _>("started_at")?
                            .is_some()
                            && row
                                .try_get::<Option<DateTime<Utc>>, _>("stop_command_sent_at")?
                                .is_none(),
                        operation_id,
                        deadline_at: deadline_at.unwrap_or(now),
                        claimed_until,
                        claimed_at,
                    };
                    commit_db(db).await?;
                    return Ok(StopAdmission::Ready(current));
                }
                return Err(G5Error::WorkspaceStopping);
            }
            let stored_reason = row
                .try_get::<Option<String>, _>("stop_reason")?
                .ok_or_else(|| internal("stopping workspace reason is absent"))?;
            let stored_reason = StopReason::parse(&stored_reason)?;
            if stored_reason != reason {
                return Err(G5Error::WorkspaceStopping);
            }
            // A repeated request (retry, reload, or the reconciler racing the
            // original executor) joins an execution whose claim or yield is
            // not yet due instead of running the same operation twice; a
            // lapsed one is resumed under a fresh claim, atomically with
            // this row lock.
            if row.try_get::<Option<bool>, _>("claimed")?.unwrap_or(false) {
                commit_db(db).await?;
                return Ok(StopAdmission::Claimed);
            }
            let claimed_until = claim_stop(db.conn(), workspace_id, operation_id).await?;
            let current = CurrentStop {
                workspace_id,
                host_id: row
                    .try_get::<Option<Uuid>, _>("host_id")?
                    .ok_or_else(|| internal("stopping workspace host is absent"))?,
                vm_id: row
                    .try_get::<Option<String>, _>("vm_id")?
                    .ok_or_else(|| internal("stopping workspace VM is absent"))?
                    .parse()
                    .map_err(|_| internal("stopping workspace VM id is invalid"))?,
                epoch: u64::try_from(row.try_get::<i64, _>("epoch")?)
                    .map_err(|_| internal("workspace epoch is negative"))?,
                reason: stored_reason,
                requires_flush: row
                    .try_get::<Option<DateTime<Utc>>, _>("started_at")?
                    .is_some()
                    && row
                        .try_get::<Option<DateTime<Utc>>, _>("stop_command_sent_at")?
                        .is_none(),
                operation_id,
                deadline_at: row
                    .try_get::<Option<DateTime<Utc>>, _>("stop_deadline_at")?
                    .ok_or_else(|| internal("stopping workspace deadline is absent"))?,
                claimed_until,
                claimed_at,
            };
            commit_db(db).await?;
            return Ok(StopAdmission::Ready(current));
        }
        if reason == StopReason::Idle {
            let decision = sqlx::query(&format!(
                "{} AND vm.workspace_id = $1",
                idle_decision_query()
            ))
            .bind(workspace_id)
            .fetch_optional(db.conn())
            .await?;
            let idle = match decision {
                Some(decision) => {
                    log_idle_decision(&decision, &operation_id.to_string(), "admission")?
                }
                None => false,
            };
            if !idle {
                commit_db(db).await?;
                return Ok(StopAdmission::NotIdle);
            }
        }
        let now = Utc::now();
        if reason.nudges_before_stopping() && deadline_at.is_none_or(|deadline| deadline > now) {
            let pending_reason = row.try_get::<Option<String>, _>("pending_stop_reason")?;
            let pending_deadline =
                row.try_get::<Option<DateTime<Utc>>, _>("pending_stop_deadline_at")?;
            let existing_operation = row.try_get::<Option<Uuid>, _>("stop_operation_id")?;
            match (pending_reason, pending_deadline) {
                (Some(pending_reason), Some(pending_deadline)) => {
                    if existing_operation != Some(operation_id)
                        || StopReason::parse(&pending_reason)? != reason
                    {
                        return Err(G5Error::WorkspaceStopping);
                    }
                    if now < pending_deadline {
                        commit_db(db).await?;
                        return Ok(StopAdmission::Nudged);
                    }
                }
                (None, None) => {
                    let nudge_deadline = deadline_at
                        .unwrap_or_else(|| now + TimeDelta::seconds(DEFAULT_DEADLINE_STOP_SECS));
                    append_deadline_stop_nudge(
                        self.agent_work.as_ref(),
                        db.tx(),
                        workspace_id,
                        owner_user_id.ok_or_else(|| internal("workspace owner is absent"))?,
                        reason,
                        operation_id,
                        nudge_deadline,
                    )
                    .await?;
                    sqlx::query(
                        "UPDATE workspace_vms
                         SET stop_reason = $2, stop_operation_id = $3,
                             stop_generation = $4,
                             pending_stop_reason = $2,
                             pending_stop_deadline_at = $5,
                             automatic_continuation = CASE WHEN $6 THEN jsonb_build_object(
                                     'epoch', epoch, 'outcome', 'stopped', 'cause', $2::text,
                                     'count', COALESCE((automatic_continuation ->> 'count')::bigint, 0),
                                     'at', now())
                                 ELSE automatic_continuation END,
                             nudged_at = now(), updated_at = now()
                         WHERE workspace_id = $1 AND status IN ('starting','running')",
                    )
                    .bind(workspace_id)
                    .bind(reason.as_str())
                    .bind(operation_id)
                    .bind(workspace.try_get::<i64, _>("lifecycle_generation")?)
                    .bind(nudge_deadline)
                    // A scheduled stop decides the incarnation when it is
                    // announced, so a VM lost during the nudge window is not
                    // started again against it.
                    .bind(!reason.continues_automatically())
                    .execute(db.conn())
                    .await?;
                    commit_db(db).await?;
                    return Ok(StopAdmission::Nudged);
                }
                _ => return Err(internal("pending workspace stop state is incomplete")),
            }
        }
        let generation = workspace.try_get::<i64, _>("lifecycle_generation")? + 1;
        let deadline_at = deadline_at.unwrap_or_else(|| now + reason.default_save_window());
        sqlx::query(
            "UPDATE workspaces SET lifecycle_generation = $2, updated_at = now()
             WHERE id = $1",
        )
        .bind(workspace_id)
        .bind(generation)
        .execute(db.conn())
        .await?;
        sqlx::query(
            "UPDATE workspace_vms
                 SET status = 'stopping', stop_reason = $2, stop_operation_id = $3,
                 stop_generation = $4, stop_deadline_at = $5,
                 stop_retry_at = now(),
                 pending_stop_reason = NULL, pending_stop_deadline_at = NULL,
                 nudged_at = NULL, stop_command_sent_at = NULL,
                 start_intent = CASE WHEN start_intent IS NULL THEN NULL
                    ELSE jsonb_set(start_intent, '{status}', '\"cancelled\"') END,
                 automatic_continuation = CASE WHEN $6 THEN jsonb_build_object(
                         'epoch', epoch, 'outcome', 'stopped', 'cause', $2::text,
                         'count', COALESCE((automatic_continuation ->> 'count')::bigint, 0),
                         'at', now())
                     ELSE automatic_continuation END,
                 updated_at = now()
             WHERE workspace_id = $1 AND status IN ('starting','running')",
        )
        .bind(workspace_id)
        .bind(reason.as_str())
        .bind(operation_id)
        .bind(generation)
        .bind(deadline_at)
        // Every stop but a host drain or rollout decides the incarnation: the
        // platform does not start it again for the turns it leaves behind,
        // even if the VM is lost before quiescence settles them
        // (`g5.lifecycle.start.automatic-continuation`).
        .bind(!reason.continues_automatically())
        .execute(db.conn())
        .await?;
        // The fencing trigger reads an immediate retry time on a Delete
        // supersession, so the claim is a second statement under the same
        // row lock; the row is never visible between the two.
        let claimed_until = claim_stop(db.conn(), workspace_id, operation_id).await?;
        self.agent_work
            .prepare_tasks_for_stop(&mut db, workspace_id, reason.as_str(), operation_id)
            .await
            .map_err(app_error)?;
        let current = CurrentStop {
            workspace_id,
            host_id: row
                .try_get::<Option<Uuid>, _>("host_id")?
                .ok_or_else(|| internal("active workspace host is absent"))?,
            vm_id: row
                .try_get::<Option<String>, _>("vm_id")?
                .ok_or_else(|| internal("active workspace VM is absent"))?
                .parse()
                .map_err(|_| internal("workspace VM id is invalid"))?,
            epoch: u64::try_from(row.try_get::<i64, _>("epoch")?)
                .map_err(|_| internal("workspace epoch is negative"))?,
            reason,
            requires_flush: status == "running",
            operation_id,
            deadline_at,
            claimed_until,
            claimed_at,
        };
        commit_db(db).await?;
        Ok(StopAdmission::Ready(current))
    }

    async fn record_unflushed_loss(
        &self,
        current: &CurrentStop,
        error: &RuntimeRpcError,
    ) -> Result<(), G5Error> {
        // Giving up ends the save window for good: a retry after a busy or
        // unanswered host command, or after an API restart, kills again
        // instead of saving or shutting down gracefully. A stop whose save
        // already completed (an API restart between the flush and the host
        // command, say) lost nothing, so it records no loss.
        let saved = self.completed_stop_flush(current).await?.is_some();
        let mut db = self.open_db().await?;
        sqlx::query(
            "UPDATE workspace_vms
             SET unflushed_loss_at = CASE WHEN $6 THEN unflushed_loss_at
                                          ELSE COALESCE(unflushed_loss_at, now()) END,
                 stop_deadline_at = LEAST(stop_deadline_at, now()),
                 last_error = CASE WHEN $6 THEN last_error
                                   ELSE COALESCE(last_error, '{}'::jsonb) ||
                                       jsonb_build_object(
                                           'reason','deadline_unflushed_loss','detail',$5)
                              END,
                 updated_at = now()
             WHERE workspace_id = $1 AND vm_id = $2 AND epoch = $3
               AND stop_operation_id = $4 AND status = 'stopping'",
        )
        .bind(current_workspace_id(current))
        .bind(current.vm_id.to_string())
        .bind(i64::try_from(current.epoch).unwrap_or(i64::MAX))
        .bind(current.operation_id)
        .bind(&error.message)
        .bind(saved)
        .execute(db.conn())
        .await?;
        commit_db(db).await?;
        Ok(())
    }

    /// A save step failed. Inside the save window, a failure that a retry
    /// may fix is left for the reconciler to retry; past the window, or for a
    /// failure no retry fixes, the stop records the loss and goes on to kill
    /// the VM (`g5.lifecycle.stop.force`). Returns whether it gave up.
    async fn give_up_saving(
        &self,
        current: &CurrentStop,
        error: &RuntimeRpcError,
    ) -> Result<bool, G5Error> {
        if is_deterministic_flush_error(error) || Utc::now() >= current.deadline_at {
            tracing::warn!(
                workspace_id = %current.workspace_id,
                vm_id = %current.vm_id,
                operation_id = %current.operation_id,
                reason = ?current.reason,
                error = %error.message,
                "workspace stop gave up saving; the VM will be killed"
            );
            self.record_unflushed_loss(current, error).await?;
            return Ok(true);
        }
        self.defer_stop_transport(current, &error.message).await?;
        Ok(false)
    }

    async fn completed_stop_flush(&self, current: &CurrentStop) -> Result<Option<Uuid>, G5Error> {
        let mut db = self.open_db().await?;
        let flush_id = sqlx::query_scalar(
            "SELECT flush_id FROM workspace_flushes
             WHERE workspace_id = $1 AND vm_id = $2 AND epoch = $3
               AND request_key = $4 AND complete_at IS NOT NULL
               AND error IS NULL AND pruned_at IS NULL",
        )
        .bind(current.workspace_id)
        .bind(current.vm_id.to_string())
        .bind(i64::try_from(current.epoch).unwrap_or(i64::MAX))
        .bind(format!("stop:{}:flush", current.operation_id))
        .fetch_optional(db.conn())
        .await?;
        rollback_db(db).await?;
        Ok(flush_id)
    }

    /// This replica holds no tunnel for the VM's host. Record that the stop
    /// is waiting for the tunnel owner and leave it due at once, so the
    /// owning replica's next reconciler tick claims it. A later due time
    /// would let this replica's own ticks keep pushing it out of the owner's
    /// reach.
    async fn defer_stop_to_tunnel_owner(&self, current: &CurrentStop) -> Result<(), G5Error> {
        tracing::info!(
            workspace_id = %current.workspace_id,
            vm_id = %current.vm_id,
            host_id = %current.host_id,
            operation_id = %current.operation_id,
            live_tunnels = self.hosts.len(),
            "workspace stop waits for the API replica holding the host tunnel"
        );
        let mut db = self.open_db().await?;
        sqlx::query(
            "UPDATE workspace_vms
             SET stop_retry_at = now(),
                 last_error = COALESCE(last_error, '{}'::jsonb) ||
                     jsonb_build_object(
                         'reason', 'stop_awaiting_tunnel_owner',
                         'detail', $5::text),
                 updated_at = now()
             WHERE workspace_id = $1 AND vm_id = $2 AND epoch = $3
               AND stop_operation_id = $4 AND status = 'stopping'",
        )
        .bind(current.workspace_id)
        .bind(current.vm_id.to_string())
        .bind(i64::try_from(current.epoch).unwrap_or(i64::MAX))
        .bind(current.operation_id)
        .bind(current.host_id.to_string())
        .execute(db.conn())
        .await?;
        commit_db(db).await?;
        Ok(())
    }

    async fn defer_stop_transport(
        &self,
        current: &CurrentStop,
        detail: &str,
    ) -> Result<(), G5Error> {
        tracing::warn!(
            workspace_id = %current.workspace_id,
            vm_id = %current.vm_id,
            host_id = %current.host_id,
            operation_id = %current.operation_id,
            detail,
            "workspace stop transport failed; retrying"
        );
        let mut db = self.open_db().await?;
        sqlx::query(
            "UPDATE workspace_vms
             SET stop_retry_at = now() + interval '5 seconds',
                 last_error = COALESCE(last_error, '{}'::jsonb) ||
                     jsonb_build_object(
                         'reason', 'stop_transport_retry', 'detail', $5),
                 updated_at = now()
             WHERE workspace_id = $1 AND vm_id = $2 AND epoch = $3
               AND stop_operation_id = $4 AND status = 'stopping'",
        )
        .bind(current.workspace_id)
        .bind(current.vm_id.to_string())
        .bind(i64::try_from(current.epoch).unwrap_or(i64::MAX))
        .bind(current.operation_id)
        .bind(detail)
        .execute(db.conn())
        .await?;
        commit_db(db).await?;
        Ok(())
    }

    async fn release_stopped(&self, current: &CurrentStop) -> Result<(), G5Error> {
        let mut db = self.open_db().await?;
        let released = crate::services::hosts::workspace_vm_release::release_stopped(
            self.agent_work.as_ref(),
            db.conn(),
            current.host_id,
            current_workspace_id(current),
            current.vm_id,
            current.epoch,
            current.operation_id,
        )
        .await?;
        commit_db(db).await?;
        if !released {
            log_stop_phase(current, "already_released");
        }
        Ok(())
    }
}

fn automatic_continuation_record(
    epoch: i64,
    outcome: &str,
    cause: Option<&str>,
    count: i64,
    intent_id: Option<Uuid>,
) -> Value {
    let mut record = json!({
        "epoch": epoch,
        "outcome": outcome,
        "count": count,
        "at": Utc::now(),
    });
    if let Some(cause) = cause {
        record["cause"] = Value::String(cause.to_string());
    }
    if let Some(intent_id) = intent_id {
        record["intent_id"] = Value::String(intent_id.to_string());
    }
    record
}

/// A failure to record the automatic consent that the next pass should try
/// again: anything but a definite refusal of this workspace's start (its
/// owner, state or configuration), since a busy fence, a lost connection or
/// a pool timeout says nothing about the continuation itself.
fn automatic_continuation_retryable(error: &G5Error) -> bool {
    match error {
        G5Error::ValidationError { details } => {
            details["code"].as_str() == Some("WORKSPACE_COMMIT_FENCE_BUSY")
        }
        G5Error::Forbidden { .. }
        | G5Error::WorkspaceStopping
        | G5Error::WorkspaceIsBase
        | G5Error::NotFound { .. }
        | G5Error::OwnerLimitReached { .. }
        | G5Error::Conflict { .. }
        | G5Error::Config(_) => false,
        _ => true,
    }
}

/// What the owner is told for a refusal code.
fn refusal_phrase(code: &str) -> &'static str {
    match code {
        "owner_authority" => "its owner may not start workspaces here any more",
        "transfer_or_stop" => "it is being transferred or stopped",
        "base_workspace" => "it is a Base workspace",
        "missing" => "it no longer exists",
        "owner_limit" => "its owner already runs as many workspace VMs as the organization allows",
        "conflict" => "another start or stop got there first",
        "configuration" => "the organization has no workspace image configured",
        _ => "an error prevented it",
    }
}

fn automatic_continuation_refusal(error: &G5Error) -> &'static str {
    match error {
        G5Error::Forbidden { .. } => "owner_authority",
        G5Error::WorkspaceStopping => "transfer_or_stop",
        G5Error::WorkspaceIsBase => "base_workspace",
        G5Error::NotFound { .. } => "missing",
        G5Error::OwnerLimitReached { .. } => "owner_limit",
        G5Error::Conflict { .. } => "conflict",
        G5Error::Config(_) => "configuration",
        _ => "error",
    }
}

async fn commit_db(db: ScopedDb) -> Result<(), G5Error> {
    crate::db::durable_commit::commit_durable_scope(db).await
}

async fn rollback_db(db: ScopedDb) -> Result<(), G5Error> {
    db.rollback().await.map_err(|error| {
        internal(&format!(
            "failed to close workspace database scope: {error}"
        ))
    })
}

/// Why a formal review's Start consent ended without the review being sent,
/// or `None` while that consent is live or the workspace has compute. The
/// cause is the review's readable failure; each ends one attempt, which Retry
/// replaces with another under fresh consent.
fn ended_review_start_cause(
    review_id: Uuid,
    workspace_deleted: bool,
    vm_status: Option<&str>,
    intent: Option<&Value>,
    last_error: Option<&Value>,
    now: DateTime<Utc>,
) -> Option<&'static str> {
    if workspace_deleted {
        return Some("review workspace was deleted before the review started");
    }
    // Compute another Start brought up is not the review's either.
    if intent.is_some_and(|intent| intent["id"].as_str() != Some(review_id.to_string().as_str())) {
        return Some("another start replaced the review workspace's start");
    }
    if matches!(vm_status, Some("starting" | "running" | "stopping")) {
        return None;
    }
    let intent = intent?;
    let expired = || {
        intent["expires_at"]
            .as_str()
            .and_then(|value| DateTime::parse_from_rfc3339(value).ok())
            .is_none_or(|expires_at| expires_at <= now)
    };
    match intent["status"].as_str()? {
        "pending" | "queued_for_capacity" if !expired() => None,
        "pending" | "queued_for_capacity" | "expired" => Some(capacity_wait_cause(last_error)),
        "cancelled" => Some("review workspace was stopped before it was ready"),
        "failed" if last_error.is_some_and(|error| error["reason"] == "owner_vm_limit") => {
            Some("the requester's running VM limit was reached; stop one of their workspaces")
        }
        "failed" => Some("review workspace failed to boot"),
        "consumed" => Some("review workspace stopped before the review was sent"),
        _ => None,
    }
}

/// What a review workspace whose Start consent is still live was waiting
/// for when the review's readiness deadline passed.
fn waiting_review_start_cause(
    vm_status: Option<&str>,
    intent: Option<&Value>,
    last_error: Option<&Value>,
) -> Option<&'static str> {
    match (
        vm_status,
        intent.and_then(|intent| intent["status"].as_str()),
    ) {
        (Some("starting"), _) => Some("review workspace was still starting at the start deadline"),
        (Some("stopped" | "failed") | None, Some("queued_for_capacity")) => {
            Some(capacity_wait_cause(last_error))
        }
        _ => None,
    }
}

/// Why placement kept a review workspace queued, from the reason it recorded.
fn capacity_wait_cause(last_error: Option<&Value>) -> &'static str {
    if last_error.is_some_and(|error| error["reason"] == "org_vm_quota") {
        "no VM became available for the review workspace before the start deadline: \
         the organization's running VM quota stayed full"
    } else {
        "no VM became available for the review workspace before the start deadline: \
         no host had a free slot"
    }
}

/// The Start consent on a VM row that work may still bind to or join: one
/// the owner recorded (Resume or a compound action; an automatic
/// continuation is the platform's own scoped consent), still `pending`,
/// `queued_for_capacity` or `starting`, and not past its bounded lifetime.
/// Returns its id. One predicate serves the input binding, the preview
/// binding and a compound action joining the Start already under way
/// (`g5.lifecycle.start`, `g5.preview.start-and-preview`).
pub fn live_owner_start_intent(
    intent: Option<&Value>,
    owner_user_id: Option<Uuid>,
    now: DateTime<Utc>,
) -> Option<Uuid> {
    let intent = intent?;
    if !matches!(
        intent["status"].as_str(),
        Some("pending" | "queued_for_capacity" | "starting")
    ) || !matches!(intent["kind"].as_str(), Some("resume" | "compound_start"))
    {
        return None;
    }
    if let Some(owner) = owner_user_id {
        if intent["owner_user_id"]
            .as_str()
            .and_then(|value| Uuid::parse_str(value).ok())
            != Some(owner)
        {
            return None;
        }
    }
    // An active intent past its bounded lifetime is dead whatever its
    // status: the row validator (`g5_validate_runtime_intent_binding`)
    // refuses to carry one, so nothing may bind to or join it.
    let expires_at = intent["expires_at"]
        .as_str()
        .and_then(|value| DateTime::parse_from_rfc3339(value).ok())?;
    if expires_at <= now {
        return None;
    }
    intent["id"]
        .as_str()
        .and_then(|id| Uuid::parse_str(id).ok())
}

fn start_intent_value(
    intent_id: Uuid,
    kind: StartKind,
    owner_user_id: Uuid,
    lifecycle_generation: u64,
    status: &str,
    now: DateTime<Utc>,
    supersedes_intent_id: Option<&str>,
) -> Value {
    let mut intent = json!({
        "id": intent_id,
        "kind": kind.as_str(),
        "owner_user_id": owner_user_id,
        "lifecycle_generation": lifecycle_generation,
        "status": status,
        "created_at": now,
        "expires_at": now + kind.consent_ttl(),
        "attempts": 0,
    });
    if let Some(supersedes) = supersedes_intent_id {
        intent["supersedes_intent_id"] = Value::String(supersedes.to_string());
    }
    intent
}

async fn append_deadline_stop_nudge(
    agent_work: &dyn WorkspaceAgentWork,
    tx: &mut Transaction<'_, Postgres>,
    workspace_id: Uuid,
    owner_user_id: Uuid,
    reason: StopReason,
    operation_id: Uuid,
    deadline_at: DateTime<Utc>,
) -> Result<(), G5Error> {
    let (title, message) = if reason == StopReason::Delete {
        (
            "Workspace force deletion started",
            "The workspace owner started force deletion. Unflushed runtime changes may be lost."
                .to_string(),
        )
    } else {
        (
            "Workspace shutdown scheduled",
            format!(
                "This workspace must stop for {} maintenance by {}. Save any important work before the deadline.",
                reason.as_str(),
                deadline_at.to_rfc3339()
            ),
        )
    };
    sqlx::query(
        "INSERT INTO notifications(
             workspace_id, recipient_user_id, actor_user_id, category,
             severity, title, message, actions)
         VALUES ($1, $2, NULL, 'workspace_lifecycle', 'warning', $3, $4,
                 jsonb_build_array(jsonb_build_object(
                     'kind', 'workspace_stop', 'operationId', $5::text,
                     'deadlineAt', $6::text)))",
    )
    .bind(workspace_id)
    .bind(owner_user_id)
    .bind(title)
    .bind(&message)
    .bind(operation_id)
    .bind(deadline_at.to_rfc3339())
    .execute(&mut **tx)
    .await?;

    // No session row lock here: journal projection inserts a session's chat
    // rows and then updates its session row, so holding the row across the
    // notice's ordinal allocation deadlocks with a projection that already
    // allocated the same ordinal. The notice insert is per-operation
    // idempotent and tolerates a session deleted after this read.
    let session_ids = sqlx::query_scalar::<_, Uuid>(
        "SELECT session.id
         FROM chat_sessions AS session
         WHERE session.workspace_id = $1
           AND EXISTS (
               SELECT 1 FROM tasks AS task
               WHERE task.chat_session_id = session.id
                 AND task.kind = 'agent'
                 AND task.status IN ('queued', 'running', 'waiting')
           )
         ORDER BY session.id",
    )
    .bind(workspace_id)
    .fetch_all(&mut **tx)
    .await?;
    for session_id in session_ids {
        agent_work
            .append_lifecycle_notice(
                tx,
                session_id,
                &message,
                operation_id,
                reason.as_str(),
                &deadline_at.to_rfc3339(),
            )
            .await?;
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn resolve_workspace_image(
    tx: &mut Transaction<'_, Postgres>,
    repo_storage: ScopedRepoStorage,
    org_id: Uuid,
    workspace_id: Uuid,
    org_default_image_ref: &str,
    repos: &[VmRepoSpec],
    restore: Option<&VmRestoreSpec>,
    reference_pins: &[(Uuid, String)],
) -> Result<(String, String, Value, Value), G5Error> {
    let (formal, personal): (bool, bool) = sqlx::query_as(
        "SELECT agent_approval_review_id IS NOT NULL, personal_user_id IS NOT NULL
         FROM workspaces WHERE id=$1",
    )
    .bind(workspace_id)
    .fetch_one(&mut **tx)
    .await?;
    // A personal workspace takes its image from the reference code checkouts
    // at their pinned commits, never its private repository, and the
    // organization default when it has none (`g5.workspace.identity.reference`).
    if formal || (personal && reference_pins.is_empty()) {
        let (reference, digest, source) =
            select_workspace_image(BTreeMap::new(), org_default_image_ref)?;
        return Ok((reference, digest, source, json!([])));
    }
    let code_repo_ids = if personal {
        reference_pins.iter().map(|(repo_id, _)| *repo_id).collect()
    } else {
        sqlx::query_scalar::<_, Uuid>(
            "SELECT repo_id FROM workspace_repos
             WHERE workspace_id = $1 AND role = 'code'
             ORDER BY repo_id",
        )
        .bind(workspace_id)
        .fetch_all(&mut **tx)
        .await?
    };
    if code_repo_ids.is_empty() {
        return Err(image_resolution_error(
            "IMAGE_INVALID",
            "workspace has no code repository members",
        ));
    }

    let restore_heads = restore
        .map(|restore| {
            restore
                .repos
                .iter()
                .map(|repo| (repo.repo_id, repo.local_head_oid.clone()))
                .collect::<BTreeMap<_, _>>()
        })
        .unwrap_or_default();
    let branch_heads = repos
        .iter()
        .map(|repo| (repo.repo_id, repo.branch_oid.clone()))
        .chain(reference_pins.iter().cloned())
        .collect::<BTreeMap<_, _>>();
    let mut resolved_from = Vec::with_capacity(code_repo_ids.len());
    let mut selected = BTreeMap::<String, Vec<(Uuid, String)>>::new();
    for repo_id in code_repo_ids {
        let oid = restore_heads
            .get(&repo_id)
            .or_else(|| branch_heads.get(&repo_id))
            .cloned()
            .ok_or_else(|| {
                image_resolution_error(
                    "IMAGE_INVALID",
                    format!("code repository {repo_id} has no committed restore head"),
                )
            })?;
        resolved_from.push(json!({"repo_id":repo_id,"oid":oid}));
        let storage = repo_storage.clone();
        let key = repo_storage.key(org_id, repo_id)?;
        let oid_for_read = oid.clone();
        let bytes = tokio::task::spawn_blocking(move || {
            crate::services::workspace_reader::read_file_at_commit_sync(
                &storage,
                &key,
                &oid_for_read,
                ".g5/workspace.yaml",
            )
        })
        .await
        .map_err(|error| internal(&format!("workspace image reader task failed: {error}")))?;
        let bytes = match bytes {
            Ok(bytes) => bytes,
            Err(G5Error::NotFound { .. }) => continue,
            Err(error) => {
                return Err(image_resolution_error(
                    "IMAGE_INVALID",
                    format!("cannot read .g5/workspace.yaml from {repo_id}@{oid}: {error}"),
                ))
            }
        };
        let Some(base_image) = workspace_config_base_image(&bytes, repo_id)? else {
            continue;
        };
        selected.entry(base_image).or_default().push((repo_id, oid));
    }

    let (image_ref, image_digest, source) =
        select_workspace_image(selected, org_default_image_ref)?;
    Ok((image_ref, image_digest, source, Value::Array(resolved_from)))
}

fn workspace_config_base_image(bytes: &[u8], repo_id: Uuid) -> Result<Option<String>, G5Error> {
    if bytes.len() > 1024 * 1024 {
        return Err(image_resolution_error(
            "IMAGE_INVALID",
            format!(".g5/workspace.yaml in {repo_id} exceeds 1 MiB"),
        ));
    }
    let document: serde_yaml::Value = serde_yaml::from_slice(bytes).map_err(|error| {
        image_resolution_error(
            "IMAGE_INVALID",
            format!("invalid .g5/workspace.yaml in {repo_id}: {error}"),
        )
    })?;
    let Some(base_image) = document.get("base_image") else {
        return Ok(None);
    };
    let base_image = base_image.as_str().map(str::trim).ok_or_else(|| {
        image_resolution_error(
            "IMAGE_INVALID",
            format!("base_image in {repo_id} must be a string"),
        )
    })?;
    validate_base_image_ref(base_image).map_err(|error| {
        image_resolution_error(
            "IMAGE_INVALID",
            format!("invalid base_image in {repo_id}: {error:?}"),
        )
    })?;
    Ok(Some(base_image.to_string()))
}

fn select_workspace_image(
    selected: BTreeMap<String, Vec<(Uuid, String)>>,
    org_default_image_ref: &str,
) -> Result<(String, String, Value), G5Error> {
    if selected.len() > 1 {
        return Err(image_resolution_error(
            "IMAGE_AMBIGUOUS",
            format!(
                "code repositories select conflicting base images: {}",
                selected.keys().cloned().collect::<Vec<_>>().join(", ")
            ),
        ));
    }
    let image_ref = selected
        .keys()
        .next()
        .cloned()
        .unwrap_or_else(|| org_default_image_ref.to_string());
    let image_digest = validate_base_image_ref(&image_ref).map_err(|error| {
        image_resolution_error(
            "IMAGE_INVALID",
            format!("invalid resolved workspace image: {error:?}"),
        )
    })?;
    let source = if selected.is_empty() {
        json!({"kind":"org_default"})
    } else {
        json!({"kind":"workspace_config","path":".g5/workspace.yaml"})
    };
    Ok((image_ref, image_digest, source))
}

fn image_resolution_error(code: &'static str, message: impl Into<String>) -> G5Error {
    G5Error::ValidationError {
        details: json!({"code":code,"message":message.into()}),
    }
}

/// A personal workspace's reference checkouts: its organization's Base
/// members, each pinned at Base's current durable branch head
/// (`g5.workspace.identity.reference`). Every other workspace has none of
/// these; it may get its owner's personal repository instead
/// ([`load_personal_mount`]). A Base
/// member whose root would collide with a member's root refuses the Start
/// rather than shadowing the private repository.
async fn load_reference_repos(
    tx: &mut Transaction<'_, Postgres>,
    repo_storage: &ScopedRepoStorage,
    org_id: Uuid,
    workspace_id: Uuid,
    members: &[VmRepoSpec],
    git_external_url: &str,
) -> Result<Vec<VmReferenceRepoSpec>, G5Error> {
    use crate::services::workspace_repositories::load_workspace_repositories;
    let base_id = sqlx::query_scalar::<_, Uuid>(
        "SELECT base.id FROM workspaces AS personal
         JOIN workspaces AS base
           ON base.org_id = personal.org_id AND base.is_base AND base.deleted_at IS NULL
         WHERE personal.id = $1 AND personal.personal_user_id IS NOT NULL
         ORDER BY base.created_at, base.id
         LIMIT 1",
    )
    .bind(workspace_id)
    .fetch_optional(&mut **tx)
    .await?;
    let Some(base_id) = base_id else {
        return Ok(Vec::new());
    };
    let rows = match load_workspace_repositories(tx, base_id, false).await {
        Ok(rows) => rows,
        Err(G5Error::NotFound { .. }) => return Ok(Vec::new()),
        Err(error) => return Err(error),
    };
    let refs = rows
        .iter()
        .map(|row| (row.repo_id, row.branch_name.clone()))
        .collect::<Vec<_>>();
    let heads =
        crate::services::workspace_reader::read_branch_head_oids(repo_storage, org_id, &refs)
            .await?;
    let mut references = Vec::with_capacity(rows.len());
    for row in rows {
        let overlaps = members.iter().any(|member| {
            let (member, reference) = (
                std::path::Path::new(&member.root_path),
                std::path::Path::new(&row.root_path),
            );
            member.starts_with(reference) || reference.starts_with(member)
        });
        if overlaps || members.iter().any(|member| member.repo_id == row.repo_id) {
            return Err(G5Error::Conflict {
                reason: format!(
                    "Base repository root {} collides with the personal repository",
                    row.root_path
                ),
            });
        }
        // An unborn Base member has nothing to check out.
        let Some(commit) = heads
            .get(&(row.repo_id, row.branch_name.clone()))
            .cloned()
            .flatten()
            .filter(|value| valid_oid(value))
        else {
            continue;
        };
        references.push(VmReferenceRepoSpec {
            repo_id: row.repo_id,
            git_url: format!(
                "{}/git/{}",
                git_external_url.trim_end_matches('/'),
                row.repo_id
            ),
            root_path: row.root_path,
            role: Some(row.role),
            branch: row.branch_name,
            commit,
        });
    }
    Ok(references)
}

/// The current owner's personal repository, mounted in a workspace that is
/// neither personal nor a formal review workspace, whose execution reads only
/// the reviewed content (`g5.agent-approval.execution`)
/// (`g5.personal.mount`): on the personal branch at its durable
/// head, at the personal workspace's own root, with no role, so neither image
/// selection nor the docs alias sees it. Absent when the owner has no live
/// personal workspace, its branch is unborn, or a member's root overlaps the
/// mount's: an implicit convenience never fails a Start.
async fn load_personal_mount(
    tx: &mut Transaction<'_, Postgres>,
    repo_storage: &ScopedRepoStorage,
    org_id: Uuid,
    workspace_id: Uuid,
    members: &[VmRepoSpec],
    git_external_url: &str,
) -> Result<Option<VmReferenceRepoSpec>, G5Error> {
    let row = sqlx::query_as::<_, (Uuid, String, String)>(
        "SELECT member.repo_id, member.root_path, personal.name
         FROM workspaces AS workspace
         JOIN workspaces AS personal
           ON personal.org_id = workspace.org_id
          AND personal.personal_user_id = workspace.owner_user_id
          AND personal.deleted_at IS NULL
         JOIN workspace_repos AS member
           ON member.org_id = personal.org_id AND member.workspace_id = personal.id
         WHERE workspace.id = $1
           AND workspace.personal_user_id IS NULL
           AND workspace.agent_approval_review_id IS NULL
           AND NOT workspace.is_base
         ORDER BY member.repo_id
         LIMIT 1",
    )
    .bind(workspace_id)
    .fetch_optional(&mut **tx)
    .await?;
    let Some((repo_id, root_path, branch)) = row else {
        return Ok(None);
    };
    let mount = std::path::Path::new(&root_path);
    if members.iter().any(|member| {
        let member_root = std::path::Path::new(&member.root_path);
        member.repo_id == repo_id
            || member_root.starts_with(mount)
            || mount.starts_with(member_root)
    }) {
        tracing::warn!(
            %workspace_id,
            root_path,
            "a member overlaps the personal repository's root; starting without the mount"
        );
        return Ok(None);
    }
    let heads = crate::services::workspace_reader::read_branch_head_oids(
        repo_storage,
        org_id,
        &[(repo_id, branch.clone())],
    )
    .await?;
    let Some(commit) = heads
        .get(&(repo_id, branch.clone()))
        .cloned()
        .flatten()
        .filter(|value| valid_oid(value))
    else {
        return Ok(None);
    };
    Ok(Some(VmReferenceRepoSpec {
        repo_id,
        root_path,
        role: None,
        git_url: format!("{}/git/{repo_id}", git_external_url.trim_end_matches('/')),
        branch,
        commit,
    }))
}

async fn load_vm_repos(
    tx: &mut Transaction<'_, Postgres>,
    repo_storage: &ScopedRepoStorage,
    org_id: Uuid,
    workspace_id: Uuid,
    branch_name: &str,
    actor_user_id: Uuid,
    git_external_url: &str,
) -> Result<Vec<VmRepoSpec>, G5Error> {
    use crate::services::workspace_repositories::{
        load_workspace_repositories, readable_parent_workspace,
    };
    let rows = load_workspace_repositories(tx, workspace_id, false).await?;
    // Parent access only controls comparison and update operations. It must
    // not prevent a new owner from starting or editing an otherwise valid cut.
    let formal: Option<Uuid> =
        sqlx::query_scalar("SELECT agent_approval_review_id FROM workspaces WHERE id=$1")
            .bind(workspace_id)
            .fetch_one(&mut **tx)
            .await?;
    let upstreams = if let Some(review) = formal {
        rows.iter()
            .map(|repo| {
                (
                    repo.repo_id,
                    ApprovalReviewIdentity::new(review).base_branch(),
                )
            })
            .collect()
    } else {
        match readable_parent_workspace(tx, workspace_id, actor_user_id).await {
            Ok(parent_id) => match load_workspace_repositories(tx, parent_id, false).await {
                Ok(parent)
                    if rows.len() == parent.len()
                        && rows.iter().all(|member| {
                            parent
                                .iter()
                                .any(|upstream| upstream.repo_id == member.repo_id)
                        }) =>
                {
                    parent
                        .into_iter()
                        .map(|repo| (repo.repo_id, repo.branch_name))
                        .collect::<BTreeMap<_, _>>()
                }
                Ok(_) | Err(G5Error::NotFound { .. }) => BTreeMap::new(),
                Err(error) => return Err(error),
            },
            Err(G5Error::DiffGraphUnavailable { .. }) => BTreeMap::new(),
            Err(error) => return Err(error),
        }
    };
    if rows.is_empty() {
        return Err(G5Error::Conflict {
            reason: "workspace has no repository members".to_string(),
        });
    }
    let members = rows
        .iter()
        .map(|row| (row.repo_id, row.branch_name.clone()))
        .collect::<Vec<_>>();
    let heads =
        crate::services::workspace_reader::read_branch_head_oids(repo_storage, org_id, &members)
            .await?;
    // The owner's other workspaces push the private repository's branch
    // (`g5.personal.mount-push`), so the personal VM follows it.
    let follow_branch = sqlx::query_scalar::<_, bool>(
        "SELECT personal_user_id IS NOT NULL FROM workspaces WHERE id = $1",
    )
    .bind(workspace_id)
    .fetch_optional(&mut **tx)
    .await?
    .unwrap_or(false);
    rows.into_iter()
        .map(|row| {
            let repo_id = row.repo_id;
            let oid = heads
                .get(&(repo_id, branch_name.to_string()))
                .cloned()
                .flatten()
                .filter(|value| valid_oid(value))
                .ok_or_else(|| G5Error::Conflict {
                    reason: format!(
                        "repository {repo_id} has no authoritative {branch_name} branch head"
                    ),
                })?;
            Ok(VmRepoSpec {
                repo_id,
                root_path: row.root_path,
                role: Some(row.role),
                git_url: format!("{}/git/{repo_id}", git_external_url.trim_end_matches('/')),
                branch: branch_name.to_string(),
                branch_oid: oid,
                upstream_branch: upstreams.get(&repo_id).cloned(),
                follow_branch,
            })
        })
        .collect()
}

/// Whether a mount push may move the owner's personal branch now
/// (`g5.personal.mount-push`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PersonalMountPush {
    Admitted,
    /// The pusher is personal, or the target is not its owner's live
    /// personal workspace.
    NotTheOwners,
    /// Starting, stopping, or stopped with unsaved Git work.
    PersonalBusy,
}

/// Admit a mount push into `personal_id` inside the caller's push admission
/// transaction `tx`, which already holds that workspace's commit fence and
/// the pusher's VM row. This share-locks the personal VM row until the caller
/// ends `tx`, after Git committed or refused the ref, so Stop and flush
/// settlement wait for the push, and Start waits on the fence. Under those
/// locks the committed lifecycle state read here stays valid: the push lands
/// while the personal VM runs, or while it is stopped with a clean, fully
/// published saved state ([`source_rebase_execution`]).
pub async fn admit_personal_mount_push(
    state: &std::sync::Arc<crate::services::AppState>,
    tx: &mut sqlx::PgConnection,
    claims: &g5_auth::WorkspaceIncarnationClaims,
    personal_id: Uuid,
) -> Result<PersonalMountPush, G5Error> {
    sqlx::query("SELECT 1 FROM workspace_vms WHERE workspace_id = $1 FOR SHARE")
        .bind(personal_id)
        .fetch_optional(&mut *tx)
        .await?;
    let mut db =
        crate::db::scoped::scoped_db_for_system_org(state, claims.org_id, &["workspaces:read"])
            .await
            .map_err(|error| error.inner)?;
    // Kinds and the personal binding are immutable; recheck them anyway.
    let kinds: Option<(Option<Uuid>, bool)> = sqlx::query_as(
        "SELECT personal.personal_user_id, pusher.personal_user_id IS NOT NULL
         FROM workspaces AS personal, workspaces AS pusher
         WHERE personal.id = $1 AND personal.deleted_at IS NULL AND pusher.id = $2",
    )
    .bind(personal_id)
    .bind(claims.workspace_id)
    .fetch_optional(db.conn())
    .await?;
    let outcome = if kinds != Some((Some(claims.owner_user_id), false)) {
        PersonalMountPush::NotTheOwners
    } else {
        match source_rebase_execution(db.conn(), personal_id).await {
            Ok(_) => PersonalMountPush::Admitted,
            Err(G5Error::Conflict { .. }) => PersonalMountPush::PersonalBusy,
            Err(error) => return Err(error),
        }
    };
    rollback_db(db).await?;
    Ok(outcome)
}

/// Where an explicit source rebase executes (`g5.architecture.workspace_rebase`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourceRebaseExecution {
    /// The running owner VM executes every rebase, even one Git could land
    /// centrally, so its checkout is never left behind G5.
    Vm,
    /// Stopped, with a recovery receipt (if any) that holds nothing of the
    /// VM's own: a central landing is what the next start restores.
    Central,
    /// The owner's Start is under way (recorded, waiting or starting, within
    /// its lifetime). Nothing lands centrally; only a resolution task bound
    /// to that Start may be admitted (`g5.git.stopped-rebase.resolve-with-agent`).
    Starting,
}

/// Cancel an automatic continuation of `workspace_id` that still only waits,
/// inside the caller's transaction: Agents calls it when the last turn the
/// consent was recorded for is cancelled
/// (`g5.lifecycle.start.automatic-continuation`).
pub async fn cancel_waiting_automatic_continuation_in_tx(
    conn: &mut sqlx::PgConnection,
    workspace_id: Uuid,
) -> Result<u64, sqlx::Error> {
    Ok(sqlx::query(
        "UPDATE workspace_vms
         SET start_intent = jsonb_set(start_intent, '{status}', '\"cancelled\"'),
             updated_at = now()
         WHERE workspace_id = $1
           AND start_intent ->> 'kind' = 'automatic_continuation'
           AND start_intent ->> 'status' IN ('pending', 'queued_for_capacity')",
    )
    .bind(workspace_id)
    .execute(conn)
    .await?
    .rows_affected())
}

/// Cancel a Start of `workspace_id` that still only waits (pending or queued
/// for capacity), inside the caller's transaction.
pub async fn cancel_waiting_start_in_tx(
    conn: &mut sqlx::PgConnection,
    workspace_id: Uuid,
) -> Result<u64, sqlx::Error> {
    Ok(sqlx::query(
        "UPDATE workspace_vms
         SET start_intent = jsonb_set(start_intent, '{status}', '\"cancelled\"'),
             updated_at = now()
         WHERE workspace_id = $1
           AND start_intent ->> 'status' IN ('pending', 'queued_for_capacity')",
    )
    .bind(workspace_id)
    .execute(conn)
    .await?
    .rows_affected())
}

/// Select where a source rebase may execute under current lifecycle state.
/// A start in progress, a stop or a recovery receipt with dirty, unpublished
/// or foreign state refuses rather than permitting a central landing that
/// restore would discard. A caller that lands centrally rechecks this under
/// the workspace commit fence, which Start takes before capturing its heads.
pub async fn source_rebase_execution(
    conn: &mut sqlx::PgConnection,
    workspace_id: Uuid,
) -> Result<SourceRebaseExecution, G5Error> {
    match source_rebase_state(conn, workspace_id).await? {
        SourceRebaseExecution::Starting => Err(G5Error::Conflict {
            reason: "The workspace is starting or stopping; retry the rebase once it is running"
                .into(),
        }),
        execution => Ok(execution),
    }
}

/// `source_rebase_execution` that reports a Start under way as
/// [`SourceRebaseExecution::Starting`] instead of refusing, for the compound
/// Start and resolve with agent, which binds its resolution task to that
/// Start. A dirty or unpublished receipt refuses in every stopped state: the
/// VM restores that work, and the rebase must then run there.
pub async fn source_rebase_state(
    conn: &mut sqlx::PgConnection,
    workspace_id: Uuid,
) -> Result<SourceRebaseExecution, G5Error> {
    let row = sqlx::query(
        "SELECT workspace.branch_name, vm.status,
                COALESCE(vm.start_intent ->> 'status' IN ('pending','queued_for_capacity','starting')
                         AND (vm.start_intent ->> 'expires_at')::timestamptz > now(), false) AS start_pending,
                receipt.repos AS receipt
         FROM workspaces AS workspace
         LEFT JOIN workspace_vms AS vm ON vm.workspace_id = workspace.id
         LEFT JOIN workspace_flushes AS receipt
           ON receipt.org_id = vm.org_id AND receipt.flush_id = vm.last_flush_id
          AND receipt.complete_at IS NOT NULL AND receipt.pruned_at IS NULL
         WHERE workspace.id = $1 AND workspace.deleted_at IS NULL",
    )
    .bind(workspace_id)
    .fetch_optional(&mut *conn)
    .await?
    .ok_or_else(|| G5Error::NotFound {
        resource: format!("workspace {workspace_id}"),
    })?;
    let status: Option<String> = row.try_get("status")?;
    if status.as_deref() == Some("running") {
        return Ok(SourceRebaseExecution::Vm);
    }
    let start_pending = row.try_get::<bool, _>("start_pending")?;
    if !matches!(
        status.as_deref(),
        None | Some("stopped") | Some("failed") | Some("starting")
    ) || (status.as_deref() == Some("starting") && !start_pending)
    {
        return Err(G5Error::Conflict {
            reason: "The workspace is starting or stopping; retry the rebase once it is running"
                .into(),
        });
    }
    let branch: String = row
        .try_get::<Option<String>, _>("branch_name")?
        .ok_or_else(|| internal("the Base workspace owns no rebase branch"))?;
    let receipt: Option<Value> = row.try_get("receipt")?;
    let receipts: Vec<FlushRepoReceipt> = receipt
        .map(serde_json::from_value)
        .transpose()
        .map_err(|error| internal(&format!("invalid durable flush receipt: {error}")))?
        .unwrap_or_default();
    if receipts
        .iter()
        .any(|receipt| !flush_receipt_is_published_clean(receipt, &branch))
    {
        return Err(G5Error::Conflict {
            reason: "The stopped workspace kept uncommitted, unpublished or in-progress Git work; \
                     start the workspace, then retry the rebase so it runs there"
                .into(),
        });
    }
    Ok(if start_pending {
        SourceRebaseExecution::Starting
    } else {
        SourceRebaseExecution::Central
    })
}

/// Whether a flush receipt left nothing of the VM's own behind: the checkout
/// sat on the shared `branch`, no sequencer was open, no file was dirty, and
/// the local branch was exactly the published head. Such a receipt is only a
/// pointer to the published head, so restoring from it and restoring from
/// the head are the same state.
pub fn flush_receipt_is_published_clean(receipt: &FlushRepoReceipt, branch: &str) -> bool {
    matches!(receipt.sequencer, SequencerState::None)
        && !receipt.dirty
        && receipt.head_ref.as_deref() == Some(format!("refs/heads/{branch}").as_str())
        && receipt.local_branch_oid == receipt.branch_oid
        && receipt.local_head_oid == receipt.local_branch_oid
}

/// Whether a restore receipt should be dropped in favour of the authoritative
/// branch heads.
///
/// The host restores a repo with a receipt to the receipt's local branch and
/// only fetches the moved server head as `origin/<branch>`. When a head moved
/// while the VM was stopped (a pull request landed on this workspace, a
/// review workspace was rebased onto its source at Reopen), a receipt that
/// carried nothing of the VM's own would resume a stale checkout behind its
/// own origin. Such a receipt is only a pointer to the published heads, so
/// restoring from the heads instead loses nothing. The receipt is immutable
/// (the host verifies its OIDs against the flush refs) and the host requires
/// the restore vector to cover every repo, so this is all or nothing: the
/// receipt is superseded only when *every* repo is fully published and at
/// least one head moved; a receipt with unpushed commits, dirty files, an
/// open sequencer or a foreign checkout anywhere is kept verbatim.
fn restore_is_superseded_by_authoritative_heads(
    restore: &VmRestoreSpec,
    repos: &[VmRepoSpec],
) -> bool {
    let mut moved = false;
    for receipt in &restore.repos {
        let Some(repo) = repos.iter().find(|repo| repo.repo_id == receipt.repo_id) else {
            return false;
        };
        let published_clean = matches!(receipt.sequencer, VmSequencer::None)
            && !receipt.dirty
            && receipt.head_ref.as_deref() == Some(format!("refs/heads/{}", repo.branch).as_str())
            && receipt.local_branch_oid == receipt.branch_oid
            && receipt.local_head_oid == receipt.local_branch_oid;
        if !published_clean {
            return false;
        }
        moved |= repo.branch_oid != receipt.branch_oid;
    }
    moved
}

async fn load_restore(
    tx: &mut Transaction<'_, Postgres>,
    workspace_id: Uuid,
) -> Result<Option<VmRestoreSpec>, G5Error> {
    let row = sqlx::query(
        "SELECT receipt.flush_id, receipt.repos
         FROM workspace_vms AS vm
         JOIN workspace_flushes AS receipt
           ON receipt.org_id = vm.org_id AND receipt.flush_id = vm.last_flush_id
         WHERE vm.workspace_id = $1 AND receipt.complete_at IS NOT NULL
           AND receipt.pruned_at IS NULL",
    )
    .bind(workspace_id)
    .fetch_optional(&mut **tx)
    .await?;
    let Some(row) = row else {
        return Ok(None);
    };
    let receipts: Vec<FlushRepoReceipt> = serde_json::from_value(row.try_get("repos")?)
        .map_err(|error| internal(&format!("invalid durable flush receipt: {error}")))?;
    let repos = receipts
        .into_iter()
        .map(|receipt| VmRestoreRepoSpec {
            repo_id: receipt.repo_id,
            branch_oid: receipt.branch_oid,
            local_branch_oid: receipt.local_branch_oid,
            local_head_oid: receipt.local_head_oid,
            head_ref: receipt.head_ref,
            sequencer: match receipt.sequencer {
                SequencerState::None => VmSequencer::None,
                SequencerState::Rebase => VmSequencer::Rebase,
                SequencerState::Merge => VmSequencer::Merge,
                SequencerState::CherryPick => VmSequencer::CherryPick,
                SequencerState::Revert => VmSequencer::Revert,
            },
            snapshot_oid: receipt.snapshot_oid,
            dirty: receipt.dirty,
        })
        .collect();
    Ok(Some(VmRestoreSpec {
        flush_id: row.try_get("flush_id")?,
        repos,
    }))
}

async fn load_view(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    include_retired_personal: bool,
) -> Result<WorkspaceVmView, G5Error> {
    let row = sqlx::query(
        "SELECT workspace.org_id, workspace.id AS workspace_id, workspace.owner_user_id,
                -- A stopped or failed VM whose new start waits for capacity
                -- reads as queued.
                CASE WHEN COALESCE(vm.status, 'stopped') IN ('stopped', 'failed')
                          AND vm.start_intent ->> 'status' = 'queued_for_capacity'
                     THEN 'queued_for_capacity'
                     ELSE COALESCE(vm.status, 'stopped') END AS status,
                vm.host_id, vm.vm_id,
                COALESCE(vm.epoch, 0) AS epoch, vm.image_ref, vm.module_catalog_version,
                vm.start_intent, vm.start_deadline_at, vm.started_at, vm.last_activity_at,
                vm.stop_reason, vm.stop_operation_id, vm.stop_deadline_at,
                vm.pending_stop_reason, vm.pending_stop_deadline_at, vm.nudged_at,
                vm.last_flush_id, vm.unflushed_loss_at, vm.last_error
         FROM workspaces AS workspace
         LEFT JOIN workspace_vms AS vm ON vm.workspace_id = workspace.id
         WHERE workspace.id = $1
           AND (workspace.deleted_at IS NULL
                OR ($2 AND workspace.personal_user_id IS NOT NULL))",
    )
    .bind(workspace_id)
    .bind(include_retired_personal)
    .fetch_optional(conn)
    .await?
    .ok_or_else(|| G5Error::NotFound {
        resource: format!("workspace {workspace_id}"),
    })?;
    Ok(WorkspaceVmView {
        org_id: row.try_get("org_id")?,
        workspace_id: row.try_get("workspace_id")?,
        owner_user_id: row
            .try_get::<Option<Uuid>, _>("owner_user_id")?
            .ok_or_else(|| internal("workspace owner is unresolved"))?,
        status: row.try_get("status")?,
        host_id: row.try_get("host_id")?,
        vm_id: row
            .try_get::<Option<String>, _>("vm_id")?
            .map(|value| value.parse::<Uuid>())
            .transpose()
            .map_err(|_| internal("workspace VM id is invalid"))?,
        epoch: u64::try_from(row.try_get::<i64, _>("epoch")?)
            .map_err(|_| internal("workspace epoch is negative"))?,
        image_ref: row.try_get("image_ref")?,
        module_catalog_version: row.try_get("module_catalog_version")?,
        start_intent: row.try_get("start_intent")?,
        start_deadline_at: row.try_get("start_deadline_at")?,
        started_at: row.try_get("started_at")?,
        last_activity_at: row.try_get("last_activity_at")?,
        stop_reason: row.try_get("stop_reason")?,
        stop_operation_id: row.try_get("stop_operation_id")?,
        stop_deadline_at: row.try_get("stop_deadline_at")?,
        pending_stop_reason: row.try_get("pending_stop_reason")?,
        pending_stop_deadline_at: row.try_get("pending_stop_deadline_at")?,
        nudged_at: row.try_get("nudged_at")?,
        last_flush_id: row.try_get("last_flush_id")?,
        unflushed_loss_at: row.try_get("unflushed_loss_at")?,
        last_error: row.try_get("last_error")?,
    })
}

async fn lock_transfer_memberships(
    conn: &mut PgConnection,
    org_id: Uuid,
    actor: Uuid,
    recipient: Uuid,
) -> Result<(), G5Error> {
    let mut members = vec![actor, recipient];
    members.sort_unstable();
    members.dedup();
    for user in members {
        sqlx::query("SELECT role FROM authz.lock_org_membership($1,$2)")
            .bind(org_id)
            .bind(user)
            .execute(&mut *conn)
            .await?;
    }
    Ok(())
}

async fn abort_transfer_in_tx(
    conn: &mut PgConnection,
    transfer_id: Uuid,
    error: &G5Error,
) -> Result<(), G5Error> {
    sqlx::query(
        "UPDATE workspace_ownership_transfers
         SET status = 'aborted', error = jsonb_build_object('message',$2)
         WHERE id = $1 AND status = 'pending'",
    )
    .bind(transfer_id)
    .bind(error.to_string())
    .execute(conn)
    .await?;
    Ok(())
}

/// Claim an admitted stop for the executor about to run it: the reconciler
/// leaves the row alone until `STOP_CLAIM_LEASE_SECS` pass without a
/// renewal. Runs under the admission row lock, so the claim and the
/// decision to execute are one step. Returns the claim the renewals start
/// from.
async fn claim_stop(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    operation_id: Uuid,
) -> Result<DateTime<Utc>, G5Error> {
    sqlx::query_scalar(
        "UPDATE workspace_vms
         SET stop_retry_at = now() + make_interval(secs => $3), updated_at = now()
         WHERE workspace_id = $1 AND stop_operation_id = $2 AND status = 'stopping'
         RETURNING stop_retry_at",
    )
    .bind(workspace_id)
    .bind(operation_id)
    .bind(STOP_CLAIM_LEASE_SECS as f64)
    .fetch_optional(conn)
    .await?
    .ok_or_else(|| internal("admitted workspace stop is no longer stopping"))
}

fn lifecycle_request(current: &CurrentStop, idempotency_key: String) -> RuntimeRequest {
    RuntimeRequest::lifecycle(
        current_workspace_id(current),
        current.vm_id,
        current.epoch,
        current.deadline_at.timestamp_millis().max(0) as u64,
        idempotency_key,
    )
}

fn current_workspace_id(current: &CurrentStop) -> Uuid {
    current.workspace_id
}

/// The loss detail of a stop that reaches its window's end before saving.
/// It is the error every guest request of that stop would now return.
fn save_window_ended() -> RuntimeRpcError {
    RuntimeRpcError {
        code: 408,
        message: "DEADLINE_EXCEEDED".to_string(),
        data: Value::Null,
    }
}

fn is_deterministic_flush_error(error: &RuntimeRpcError) -> bool {
    matches!(
        error.message.as_str(),
        "FLUSH_TOO_LARGE"
            | "FLUSH_MEMORY_IGNORED"
            | "FLUSH_MEMBER_VECTOR_MISMATCH"
            | "FLUSH_INCOMPLETE_VECTOR"
            | "FLUSH_COMPLETION_CONFLICT"
            | "FLUSH_INVALID_PROOF"
            | "FLUSH_CLEAN_MISMATCH"
            | "FLUSH_REPO_NOT_FOUND"
            | "FLUSH_BRANCH_MISMATCH"
            | "FLUSH_ASSOCIATION_MISMATCH"
            | "FLUSH_REF_MISMATCH"
            | "FLUSH_HEAD_UNREACHABLE"
            | "FLUSH_NOT_COMMIT"
    )
}

fn catalog_version() -> &'static str {
    debug_assert_eq!(
        serde_json::from_slice::<Value>(MODULE_CATALOG)
            .ok()
            .and_then(|value| value["catalog_version"].as_str().map(str::to_owned))
            .as_deref(),
        Some(MODULE_CATALOG_VERSION)
    );
    MODULE_CATALOG_VERSION
}

fn valid_oid(value: &str) -> bool {
    matches!(value.len(), 40 | 64) && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn validate_idempotency_key(value: &str) -> Result<(), G5Error> {
    if value.is_empty()
        || value.len() > 256
        || value.trim() != value
        || value.chars().any(char::is_control)
    {
        return Err(G5Error::ValidationError {
            details: json!({"field":"idempotencyKey","message":"invalid idempotency key"}),
        });
    }
    Ok(())
}

fn app_error(error: AppError) -> G5Error {
    error.inner
}

fn internal(message: &str) -> G5Error {
    G5Error::Internal {
        message: message.to_string(),
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn only_definite_refusals_decide_an_automatic_continuation() {
        use g5_contracts::error::G5Error;
        assert!(!super::automatic_continuation_retryable(
            &G5Error::Forbidden {
                missing_scopes: Vec::new(),
            }
        ));
        assert!(!super::automatic_continuation_retryable(
            &G5Error::Conflict {
                reason: "superseded".to_string(),
            }
        ));
        assert!(super::automatic_continuation_retryable(
            &G5Error::Internal {
                message: "connection reset".to_string(),
            }
        ));
        assert!(super::automatic_continuation_retryable(
            &G5Error::DatabaseRetryable {
                message: "deadlock".to_string(),
            }
        ));
        assert!(super::automatic_continuation_retryable(
            &G5Error::ValidationError {
                details: serde_json::json!({"code": "WORKSPACE_COMMIT_FENCE_BUSY"}),
            }
        ));
        assert!(!super::automatic_continuation_retryable(
            &G5Error::ValidationError {
                details: serde_json::json!({"code": "OTHER"}),
            }
        ));
    }

    use super::*;

    #[test]
    fn idle_exemption_diagnostics_do_not_repeat_each_reconciliation_tick() {
        let mut diagnostics = IdleExemptionDiagnostics::default();
        let start = tokio::time::Instant::now();
        assert!(diagnostics.begin_scan(start));
        diagnostics.after_workspace = Some(Uuid::new_v4());
        let cursor = diagnostics.after_workspace;
        for seconds in (2..60).step_by(2) {
            assert!(!diagnostics.begin_scan(start + Duration::from_secs(seconds)));
            assert_eq!(diagnostics.after_workspace, cursor);
        }
        assert!(diagnostics.begin_scan(start + Duration::from_secs(60)));
        assert_eq!(diagnostics.after_workspace, cursor);
        assert!(!diagnostics.begin_scan(start + Duration::from_secs(60)));
    }

    fn restore_repo(repo_id: Uuid, oid: &str) -> VmRestoreRepoSpec {
        VmRestoreRepoSpec {
            repo_id,
            branch_oid: oid.to_string(),
            local_branch_oid: oid.to_string(),
            local_head_oid: oid.to_string(),
            head_ref: Some("refs/heads/ws".to_string()),
            sequencer: VmSequencer::None,
            snapshot_oid: oid.to_string(),
            dirty: false,
        }
    }

    fn vm_repo(repo_id: Uuid, oid: &str) -> VmRepoSpec {
        VmRepoSpec {
            repo_id,
            root_path: "repo".to_string(),
            role: None,
            git_url: "https://git.example/repo".to_string(),
            branch: "ws".to_string(),
            branch_oid: oid.to_string(),
            upstream_branch: Some("main".to_string()),
            follow_branch: false,
        }
    }

    #[test]
    fn a_fully_published_receipt_is_superseded_only_when_a_head_moved() {
        let first = Uuid::new_v4();
        let second = Uuid::new_v4();
        let old = "a".repeat(40);
        let new = "b".repeat(40);
        let local = "c".repeat(40);
        let restore = |second_repo: VmRestoreRepoSpec| VmRestoreSpec {
            flush_id: Uuid::new_v4(),
            repos: vec![restore_repo(first, &old), second_repo],
        };
        let moved = vec![vm_repo(first, &new), vm_repo(second, &old)];
        let unmoved = vec![vm_repo(first, &old), vm_repo(second, &old)];

        // Every repo fully published and one head moved: restore from heads.
        assert!(restore_is_superseded_by_authoritative_heads(
            &restore(restore_repo(second, &old)),
            &moved
        ));
        // Nothing moved: the receipt stays (identical state either way).
        assert!(!restore_is_superseded_by_authoritative_heads(
            &restore(restore_repo(second, &old)),
            &unmoved
        ));
        // Anything the VM owned anywhere keeps the whole receipt verbatim,
        // even though the other repo's head moved.
        for owned in [
            VmRestoreRepoSpec {
                local_branch_oid: local.clone(),
                local_head_oid: local.clone(),
                snapshot_oid: local.clone(),
                ..restore_repo(second, &old)
            },
            VmRestoreRepoSpec {
                snapshot_oid: local.clone(),
                dirty: true,
                ..restore_repo(second, &old)
            },
            VmRestoreRepoSpec {
                head_ref: Some("refs/heads/experiment".to_string()),
                ..restore_repo(second, &old)
            },
            VmRestoreRepoSpec {
                sequencer: VmSequencer::Rebase,
                ..restore_repo(second, &old)
            },
        ] {
            assert!(!restore_is_superseded_by_authoritative_heads(
                &restore(owned),
                &moved
            ));
        }
        // A receipt for a repo the spec no longer lists is left alone.
        assert!(!restore_is_superseded_by_authoritative_heads(
            &restore(restore_repo(Uuid::new_v4(), &old)),
            &moved
        ));
    }

    /// `g5.agents.admission.vm-slots`: the VM accepts every turn a default
    /// workspace admits, and the task-less bounded operations keep slots of
    /// their own. Otherwise a refused turn sits reserved and unconfirmed while
    /// holding a workspace slot, and the Changes panel gives up on a busy VM.
    #[test]
    fn shipped_catalog_admits_the_default_workspace_slots() {
        let catalog: Value = serde_json::from_slice(MODULE_CATALOG).unwrap();
        let concurrency = |module_id: &str| {
            catalog["modules"]
                .as_array()
                .unwrap()
                .iter()
                .find(|module| module["module_id"] == module_id)
                .and_then(|module| module["limits"]["concurrency"].as_i64())
                .unwrap_or_else(|| panic!("the catalog declares {module_id} concurrency"))
        };
        let agent_turns = i64::from(crate::models::workspace::DEFAULT_BOUNDED_EXECUTION_SLOTS);
        assert!(concurrency("agent.turn") >= agent_turns);
        let operations = concurrency("workspace.changes") + concurrency("workspace.diagnostics");
        assert!(
            catalog["bounded_execution_slots"].as_i64().unwrap() >= agent_turns + operations,
            "the VM total must cover the default workspace slots plus task-less operations"
        );
    }

    #[test]
    fn immutable_image_ref_is_required() {
        assert!(validate_base_image_ref("registry.example/g5@sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa").is_ok());
        assert!(validate_base_image_ref("registry.example/g5:latest").is_err());
    }

    #[test]
    fn workspace_config_image_is_trimmed_and_must_be_immutable() {
        let repo_id = Uuid::new_v4();
        let image = format!("registry.example/g5@sha256:{}", "a".repeat(64));
        assert_eq!(
            workspace_config_base_image(
                format!("base_image: '  {image}  '\npreview:\n  command: npm run dev\n").as_bytes(),
                repo_id,
            )
            .unwrap(),
            Some(image)
        );
        assert_eq!(
            workspace_config_base_image(b"preview:\n  command: npm run dev\n", repo_id).unwrap(),
            None
        );
        let error =
            workspace_config_base_image(b"base_image: registry.example/g5:latest\n", repo_id)
                .unwrap_err();
        assert!(matches!(error, G5Error::ValidationError { .. }));
    }

    #[test]
    fn workspace_image_selection_rejects_conflicting_code_repo_configs() {
        let first = format!("registry.example/first@sha256:{}", "a".repeat(64));
        let second = format!("registry.example/second@sha256:{}", "b".repeat(64));
        let mut selected = BTreeMap::new();
        selected.insert(first, vec![(Uuid::new_v4(), "c".repeat(40))]);
        selected.insert(second, vec![(Uuid::new_v4(), "d".repeat(40))]);
        let error = select_workspace_image(
            selected,
            &format!("registry.example/default@sha256:{}", "e".repeat(64)),
        )
        .unwrap_err();
        match error {
            G5Error::ValidationError { details } => {
                assert_eq!(details["code"], "IMAGE_AMBIGUOUS");
            }
            other => panic!("expected IMAGE_AMBIGUOUS, got {other:?}"),
        }
    }

    #[test]
    fn retired_restart_is_not_a_lifecycle_kind_or_reason() {
        assert!(StartKind::parse("restart").is_err());
        assert!(StopReason::parse("restart").is_err());
        assert!(serde_json::from_str::<StartKind>(r#""restart""#).is_err());
        assert!(serde_json::from_str::<StopReason>(r#""restart""#).is_err());
    }

    #[test]
    fn owner_idle_and_transfer_stops_save_for_two_minutes_without_a_nudge() {
        for reason in [
            StopReason::User,
            StopReason::Transfer,
            StopReason::Idle,
            StopReason::ExecutionHold,
        ] {
            assert!(!reason.nudges_before_stopping());
            assert_eq!(reason.default_save_window(), TimeDelta::minutes(2));
        }
        for reason in [
            StopReason::Drain,
            StopReason::Rollout,
            StopReason::Credential,
            StopReason::Capacity,
            StopReason::Delete,
        ] {
            assert!(reason.nudges_before_stopping());
            assert_eq!(reason.default_save_window(), TimeDelta::minutes(10));
        }
    }

    #[test]
    fn workspace_vm_view_uses_browser_contract_field_names() {
        let id = Uuid::nil();
        let value = serde_json::to_value(WorkspaceVmView {
            org_id: id,
            workspace_id: id,
            owner_user_id: id,
            status: "running".to_string(),
            host_id: Some(id),
            vm_id: Some(id),
            epoch: 1,
            image_ref: None,
            module_catalog_version: None,
            start_intent: None,
            start_deadline_at: None,
            started_at: None,
            last_activity_at: None,
            stop_reason: None,
            stop_operation_id: None,
            stop_deadline_at: None,
            pending_stop_reason: None,
            pending_stop_deadline_at: None,
            nudged_at: None,
            last_flush_id: None,
            unflushed_loss_at: None,
            last_error: None,
        })
        .unwrap();
        assert_eq!(value["workspaceId"], id.to_string());
        assert_eq!(value["hostId"], id.to_string());
        assert!(value.get("workspace_id").is_none());
    }

    #[test]
    fn a_review_start_ends_only_once_its_one_consent_is_dead() {
        use chrono::{TimeDelta, Utc};
        use serde_json::json;
        let review = uuid::Uuid::new_v4();
        let now = Utc::now();
        let intent = |status: &str, expires: TimeDelta| json!({"id": review, "status": status, "expires_at": now + expires});
        let cause = |status: Option<&str>,
                     intent: Option<serde_json::Value>,
                     error: Option<serde_json::Value>| {
            super::ended_review_start_cause(
                review,
                false,
                status,
                intent.as_ref(),
                error.as_ref(),
                now,
            )
        };
        let live = TimeDelta::minutes(5);
        assert_eq!(
            cause(
                Some("stopped"),
                Some(intent("queued_for_capacity", live)),
                None
            ),
            None
        );
        assert_eq!(
            cause(Some("failed"), Some(intent("pending", live)), None),
            None
        );
        assert_eq!(
            cause(Some("starting"), Some(intent("starting", live)), None),
            None
        );
        assert_eq!(
            cause(Some("running"), Some(intent("consumed", live)), None),
            None
        );
        assert_eq!(
            cause(Some("stopping"), Some(intent("cancelled", live)), None),
            None
        );
        assert_eq!(cause(None, None, None), None);
        let lapsed = cause(
            Some("stopped"),
            Some(intent("queued_for_capacity", -live)),
            None,
        );
        assert!(lapsed.is_some_and(|c| c.contains("no host had a free slot")));
        assert_eq!(
            cause(Some("stopped"), Some(intent("expired", -live)), None),
            lapsed
        );
        assert!(
            cause(Some("stopped"), Some(intent("cancelled", live)), None)
                .is_some_and(|c| c.contains("stopped before it was ready"))
        );
        assert!(cause(Some("failed"), Some(intent("failed", live)), None)
            .is_some_and(|c| c.contains("failed to boot")));
        assert!(cause(
            Some("stopped"),
            Some(intent("failed", live)),
            Some(json!({"reason": "owner_vm_limit", "limit": 2}))
        )
        .is_some_and(|c| c.contains("running VM limit")));
        assert!(cause(Some("failed"), Some(intent("consumed", live)), None)
            .is_some_and(|c| c.contains("stopped before the review was sent")));
        let other =
            json!({"id": uuid::Uuid::new_v4(), "status": "pending", "expires_at": now + live});
        assert!(cause(Some("stopped"), Some(other.clone()), None)
            .is_some_and(|c| c.contains("replaced")));
        assert!(cause(Some("running"), Some(other), None).is_some_and(|c| c.contains("replaced")));
        assert!(
            super::ended_review_start_cause(review, true, Some("running"), None, None, now)
                .is_some_and(|c| c.contains("deleted"))
        );
    }

    #[test]
    fn a_review_start_deadline_names_what_its_workspace_still_waited_for() {
        use serde_json::json;
        let queued = json!({"status": "queued_for_capacity"});
        let quota = json!({"reason": "org_vm_quota", "limit": 4});
        let host = json!({"reason": "no_ready_host"});
        let wait = super::waiting_review_start_cause;
        assert!(wait(Some("stopped"), Some(&queued), Some(&quota))
            .is_some_and(|c| c.contains("running VM quota stayed full")));
        assert!(wait(Some("failed"), Some(&queued), Some(&host))
            .is_some_and(|c| c.contains("no host had a free slot")));
        assert!(
            wait(Some("starting"), Some(&json!({"status": "starting"})), None)
                .is_some_and(|c| c.contains("still starting"))
        );
        assert_eq!(
            wait(Some("running"), Some(&json!({"status": "consumed"})), None),
            None
        );
        assert_eq!(
            wait(Some("stopped"), Some(&json!({"status": "pending"})), None),
            None
        );
        assert_eq!(wait(None, None, None), None);
    }
}
