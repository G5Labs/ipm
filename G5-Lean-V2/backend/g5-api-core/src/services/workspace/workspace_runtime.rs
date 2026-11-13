//! Workspace Runtime facade.
//!
//! This is the only control-plane component that opens a dispatcher RPC stream.
//! Callers supply an already-authorized actor (or lifecycle authority), an exact
//! incarnation, a deadline and an idempotency key. The dispatcher repeats the
//! incarnation and owner checks at the other end of the host tunnel.

use crate::services::git::repo_store::{OrgScope, TenantRepoStorage};
use std::collections::HashMap;
use std::future::Future;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, Weak};
use std::time::Duration;

use chrono::{DateTime, Utc};
use g5_contracts::error::G5Error;
use g5_internal_api::stream_mux::{Channel, MuxStream};
use g5_internal_api::workspace_flush::{FlushRepoReceipt, SequencerState};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::{PgPool, Row};
use uuid::Uuid;

use crate::ws::preview_tunnel::TunnelRegistry;

/// One journal replay page (`g5.workspace.module-execution`): under the
/// 4 MiB reply line of the host relay and this API, so a backlog is read in
/// bounded pages by every consumer of the journal.
pub const JOURNAL_PAGE_LIMIT: u64 = 1024;
pub const JOURNAL_PAGE_MAX_BYTES: u64 = 2 * 1024 * 1024;

const MAX_RUNTIME_REPLY_BYTES: usize = 4 * 1024 * 1024;
const CHANGES_RPC_BUDGET_MS: u64 = 30_000;
/// An in-VM source rebase fetches, may replay twice and publishes; it stays
/// inside the `workspace.changes` module's 300 s wall clock.
const CHANGES_REBASE_BUDGET_MS: u64 = 240_000;
/// Budget for cancelling an abandoned `workspace.changes` execution, whose own
/// deadline has already passed.
const CHANGES_CANCEL_BUDGET_MS: u64 = 2_000;
const DATA_STREAM_OPEN_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);
/// Pauses before each resend of a runtime call whose dispatcher stream closed
/// without a reply. The host replaces a lost dispatcher connection within a
/// second or two; the resends span about three.
const CLOSED_STREAM_RESEND_DELAYS_MS: [u64; 4] = [100, 300, 900, 1_800];
/// Deadline a resend must still have left after its pause.
const MIN_RESEND_BUDGET_MS: u64 = 1_000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct WorkspaceRuntimeHandle {
    pub org_id: Uuid,
    pub workspace_id: Uuid,
    pub owner_user_id: Uuid,
    pub host_id: Uuid,
    pub vm_id: Uuid,
    pub epoch: u64,
    pub status: String,
    pub pending_transfer: bool,
}

/// Content-free reasons for owner input or the explicit preview-traffic exception.
/// G5 UI transport, RPC completion and work do not establish activity.
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkspaceActivityReason {
    Keyboard,
    Paste,
    Pointer,
    TerminalInput,
    PreviewTraffic,
}

impl WorkspaceActivityReason {
    fn as_str(self) -> &'static str {
        match self {
            Self::Keyboard => "keyboard",
            Self::Paste => "paste",
            Self::Pointer => "pointer",
            Self::TerminalInput => "terminal_input",
            Self::PreviewTraffic => "preview_traffic",
        }
    }
}

/// Live stream authorization and explicit activity recording. Opening a terminal
/// never renews activity; preview request admission is an intentional exception.
#[derive(Clone)]
pub struct WorkspaceActivityGuard {
    pool: PgPool,
    workspace_id: Uuid,
    owner_user_id: Uuid,
    vm_id: Uuid,
    epoch: u64,
    last_write_ms: Arc<AtomicU64>,
    membership: crate::services::identity::membership_activity::MembershipActivityGuard,
}

impl WorkspaceActivityGuard {
    /// Admit one bounded delivery or input batch using current authority.
    pub async fn authorize(&self) -> bool {
        let (org_id, user_id, revision) = self.membership.binding();
        matches!(
            tokio::time::timeout(std::time::Duration::from_secs(5), async {
                sqlx::query_scalar::<_, bool>(
                    "SELECT vm.status = 'running' AND vm.vm_id = $2 AND vm.epoch = $3
                        AND vm.lease_expires_at > clock_timestamp()
                        AND authz.lock_workspace_member_activity($1)
                        AND EXISTS (SELECT 1 FROM authz.lookup_workspace_runtime_authority($1) w
                            WHERE w.owner_user_id = $5 AND w.deleted_at IS NULL AND NOT w.pending_transfer)
                        AND EXISTS (SELECT 1 FROM org_memberships m
                            WHERE m.org_id = $4 AND m.user_id = $5
                              AND m.deactivated_at IS NULL AND m.activity_revision = $6)
                 FROM workspace_vms vm WHERE vm.workspace_id = $1",
                )
                .bind(self.workspace_id)
                .bind(self.vm_id.to_string())
                .bind(self.epoch as i64)
                .bind(org_id)
                .bind(user_id)
                .bind(revision)
                .fetch_optional(&self.pool)
                .await
            })
            .await,
            Ok(Ok(Some(true)))
        )
    }

    /// Count one owner input or admitted preview request. Database writes are already
    /// coalesced in memory per stream and defensively in SQL across streams.
    pub async fn record(&self, reason: WorkspaceActivityReason, correlation_id: &str) {
        let claimed_at = unix_time_ms();
        if !claim_activity_write(&self.last_write_ms, claimed_at) {
            return;
        }
        // Cancellation (including the terminal's bounded timeout) must also
        // release this claim so the next real input can try again.
        let mut claim = ActivityWriteClaim {
            last_write_ms: &self.last_write_ms,
            claimed_at,
            completed: false,
        };
        match record_activity(
            &self.pool,
            self.workspace_id,
            self.vm_id,
            self.epoch,
            self.owner_user_id,
            reason,
            correlation_id,
            None,
        )
        .await
        {
            Ok(()) => claim.completed = true,
            Err(error) => tracing::warn!(
                target: "g5_api::workspace_activity",
                event = "workspace_activity_write_failed",
                workspace_id = %self.workspace_id, vm_id = %self.vm_id,
                epoch = self.epoch, activity_reason = reason.as_str(), correlation_id,
                %error, "activity write failed; next eligible activity may retry"
            ),
        }
    }
}

/// One runtime call's authority: the exact incarnation it expects, the owner
/// or lifecycle authority it carries, its deadline and its idempotency key.
/// Code outside this crate can build only an owner request; lifecycle
/// authority skips the owner check, so its constructor is crate-private.
#[derive(Debug, Clone)]
pub struct RuntimeRequest {
    workspace_id: Uuid,
    expected_vm_id: Uuid,
    expected_epoch: u64,
    actor_user_id: Option<Uuid>,
    lifecycle: bool,
    deadline_ms: u64,
    idempotency_key: String,
}

impl RuntimeRequest {
    /// A request by `actor_user_id` on the incarnation in `handle`. The facade
    /// admits it only while the actor owns the workspace.
    #[must_use]
    pub fn owner(
        handle: &WorkspaceRuntimeHandle,
        actor_user_id: Uuid,
        deadline_ms: u64,
        idempotency_key: impl Into<String>,
    ) -> Self {
        Self {
            workspace_id: handle.workspace_id,
            expected_vm_id: handle.vm_id,
            expected_epoch: handle.epoch,
            actor_user_id: Some(actor_user_id),
            lifecycle: false,
            deadline_ms,
            idempotency_key: idempotency_key.into(),
        }
    }

    /// A lifecycle-authority request on one exact incarnation. It reaches the
    /// dispatcher's private admission, flush and event methods without an
    /// owner, so only lifecycle owners in this crate build one.
    pub(crate) fn lifecycle(
        workspace_id: Uuid,
        expected_vm_id: Uuid,
        expected_epoch: u64,
        deadline_ms: u64,
        idempotency_key: impl Into<String>,
    ) -> Self {
        Self {
            workspace_id,
            expected_vm_id,
            expected_epoch,
            actor_user_id: None,
            lifecycle: true,
            deadline_ms,
            idempotency_key: idempotency_key.into(),
        }
    }

    fn context(&self) -> Value {
        json!({
            "workspace_id": self.workspace_id,
            "expected_vm_id": self.expected_vm_id,
            "expected_epoch": self.expected_epoch,
            "actor_user_id": self.actor_user_id,
            "lifecycle": self.lifecycle.then_some(true),
            // The dispatcher reserves this for read-only inspection; the
            // facade sends none until it can validate a grant.
            "inspection_grant_id": Value::Null,
            "deadline_ms": self.deadline_ms,
            "idempotency_key": self.idempotency_key,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeRpcError {
    pub code: i64,
    pub message: String,
    pub data: Value,
}

#[derive(Deserialize)]
struct RpcReply {
    #[allow(dead_code)]
    jsonrpc: String,
    id: Value,
    result: Option<Value>,
    error: Option<RpcReplyError>,
}

#[derive(Deserialize)]
struct RpcReplyError {
    code: i64,
    message: String,
    #[serde(default)]
    data: Value,
}

/// The process-local facade over durable workspace attachment state and the
/// host-owned reverse tunnel. It deliberately has no method that implicitly
/// starts a VM: only the lifecycle service may create start intent and attach.
pub struct WorkspaceRuntime {
    pool: PgPool,
    tunnels: Arc<TunnelRegistry>,
    repo_storage: TenantRepoStorage,
    next_request: AtomicU64,
    changes_reads: ChangesReadCoalescers,
    /// Guest calls in flight per workspace, counted in this process so a stop
    /// can wait for them without a database lock
    /// (`g5.runtime.guest-call-admission`).
    in_flight: dashmap::DashMap<Uuid, Arc<InFlightCalls>>,
}

/// The calls in flight against one workspace's incarnation, the latest
/// deadline among them and the waiters for their end.
#[derive(Default)]
pub struct InFlightCalls {
    count: std::sync::atomic::AtomicUsize,
    latest_deadline_ms: AtomicU64,
    drained: tokio::sync::Notify,
}

/// One call in flight; dropping it ends the call's count.
pub struct InFlightCall {
    calls: Arc<InFlightCalls>,
}

impl Drop for InFlightCall {
    fn drop(&mut self) {
        if self.calls.count.fetch_sub(1, Ordering::AcqRel) == 1 {
            self.calls.drained.notify_waiters();
        }
    }
}

/// Past the latest in-flight deadline, a drain gives a call this long to
/// return before it reports the call as outliving its budget.
const IN_FLIGHT_DRAIN_GRACE: Duration = Duration::from_secs(1);

/// Methods the guest serializes behind its lifecycle gate; their admission
/// never held the fence through the round trip, so they are not counted.
fn lifecycle_gated(method: &str) -> bool {
    matches!(
        method,
        "admission.close" | "admission.open" | "workspace.flush"
    )
}

impl WorkspaceRuntime {
    #[must_use]
    pub fn new(
        pool: PgPool,
        tunnels: Arc<TunnelRegistry>,
        repo_storage: TenantRepoStorage,
    ) -> Self {
        Self {
            pool,
            tunnels,
            repo_storage,
            next_request: AtomicU64::new(1),
            changes_reads: ChangesReadCoalescers::default(),
            in_flight: dashmap::DashMap::new(),
        }
    }

    /// Count one call against the workspace's incarnation for as long as the
    /// returned guard lives, from the moment its admission is decided (under
    /// the row's share lock) so a stop or transfer draining under its row
    /// lock sees every admitted call and no call still waiting for the row.
    fn enter_call(&self, workspace_id: Uuid, deadline_ms: u64) -> InFlightCall {
        let calls = self.in_flight.entry(workspace_id).or_default().clone();
        calls.count.fetch_add(1, Ordering::AcqRel);
        calls
            .latest_deadline_ms
            .fetch_max(deadline_ms, Ordering::AcqRel);
        InFlightCall { calls }
    }

    /// Guest calls in flight against the workspace right now.
    #[must_use]
    pub fn calls_in_flight(&self, workspace_id: Uuid) -> usize {
        self.in_flight
            .get(&workspace_id)
            .map(|calls| calls.count.load(Ordering::Acquire))
            .unwrap_or(0)
    }

    /// Wait until no guest call is in flight against the workspace. The wait
    /// is bounded by the calls' own budgets: every call returns by its
    /// deadline, so a call still counted a grace period past the latest
    /// deadline outlived its budget and the drain reports false. A stop or
    /// transfer waits here before and again under its row lock; calls
    /// blocked on that lock are not yet counted, so the wait under the lock
    /// ends with the admitted calls (`g5.runtime.guest-call-admission`).
    pub async fn drain_in_flight(&self, workspace_id: Uuid) -> bool {
        let Some(calls) = self.in_flight.get(&workspace_id).map(|calls| calls.clone()) else {
            return true;
        };
        loop {
            let notified = calls.drained.notified();
            tokio::pin!(notified);
            // Register before reading the count, so an end between the read
            // and the wait is not missed.
            notified.as_mut().enable();
            if calls.count.load(Ordering::Acquire) == 0 {
                return true;
            }
            let latest = calls.latest_deadline_ms.load(Ordering::Acquire);
            let remaining = latest.saturating_sub(unix_time_ms());
            let bound = Duration::from_millis(remaining) + IN_FLIGHT_DRAIN_GRACE;
            if tokio::time::timeout(bound, notified).await.is_err()
                && calls.latest_deadline_ms.load(Ordering::Acquire) <= latest
            {
                return calls.count.load(Ordering::Acquire) == 0;
            }
        }
    }

    /// Hosts this API can currently invoke. Maintenance uses this before
    /// committing shared claims; absence is not evidence that a VM is lost.
    pub(crate) fn reachable_host_ids(&self) -> Vec<Uuid> {
        self.tunnels.reachable_host_ids()
    }

    /// Resolve the current healthy attachment from the durable authority. This
    /// never opens a host tunnel and never wakes a stopped workspace.
    pub async fn get(&self, workspace_id: Uuid) -> Result<Option<WorkspaceRuntimeHandle>, G5Error> {
        let mut conn = self.pool.acquire().await?;
        let row = sqlx::query(
            "SELECT vm.org_id, vm.workspace_id, workspace.owner_user_id,
                    vm.host_id, vm.vm_id, vm.epoch, vm.status,
                    workspace.pending_transfer
             FROM workspace_vms AS vm
             JOIN authz.lookup_workspace_runtime_authority($1) AS workspace
               ON workspace.org_id = vm.org_id
              AND workspace.workspace_id = vm.workspace_id
             WHERE vm.workspace_id = $1 AND workspace.deleted_at IS NULL",
        )
        .bind(workspace_id)
        .fetch_optional(&mut *conn)
        .await?;
        let Some(row) = row else {
            return Ok(None);
        };
        let status = row.try_get::<String, _>("status")?;
        if !matches!(status.as_str(), "starting" | "running" | "stopping") {
            return Ok(None);
        }
        // The authority lookup can wait for the workspace row, so the lease of
        // the incarnation it found is read after it (see `lease_running`).
        let lease_running = sqlx::query_scalar::<_, Option<bool>>(
            "SELECT lease_expires_at > clock_timestamp() FROM workspace_vms
             WHERE workspace_id = $1 AND vm_id = $2 AND epoch = $3",
        )
        .bind(workspace_id)
        .bind(row.try_get::<String, _>("vm_id")?)
        .bind(row.try_get::<i64, _>("epoch")?)
        .fetch_optional(&mut *conn)
        .await?
        .flatten();
        if lease_running != Some(true) {
            return Ok(None);
        }
        let vm_id = row
            .try_get::<String, _>("vm_id")?
            .parse::<Uuid>()
            .map_err(|_| internal("workspace_vms.vm_id is not a UUID"))?;
        let epoch = u64::try_from(row.try_get::<i64, _>("epoch")?)
            .map_err(|_| internal("workspace_vms.epoch is negative"))?;
        Ok(Some(WorkspaceRuntimeHandle {
            org_id: row.try_get("org_id")?,
            workspace_id: row.try_get("workspace_id")?,
            owner_user_id: row.try_get("owner_user_id")?,
            host_id: row.try_get("host_id")?,
            vm_id,
            epoch,
            status,
            pending_transfer: row.try_get("pending_transfer")?,
        }))
    }

    /// The durable lifecycle status of the workspace's incarnation, whether
    /// or not its lease is current: `None` when no incarnation row exists.
    /// `get` hides an incarnation whose lease lapsed; a caller deciding that
    /// a workspace has no live incarnation at all must look here.
    pub async fn durable_status(&self, workspace_id: Uuid) -> Result<Option<String>, G5Error> {
        Ok(sqlx::query_scalar::<_, String>(
            "SELECT status FROM workspace_vms WHERE workspace_id = $1",
        )
        .bind(workspace_id)
        .fetch_optional(&self.pool)
        .await?)
    }

    /// Resolve an owner-authorized running attachment. A caller cannot use this
    /// as a wake path; absent and transitional attachments fail closed.
    pub async fn owner_handle(
        &self,
        workspace_id: Uuid,
        actor_user_id: Uuid,
    ) -> Result<WorkspaceRuntimeHandle, G5Error> {
        let Some(handle) = self.get(workspace_id).await? else {
            return Err(G5Error::WorkspaceNotRunning);
        };
        if handle.owner_user_id != actor_user_id {
            return Err(G5Error::Forbidden {
                missing_scopes: Vec::new(),
            });
        }
        if !sqlx::query_scalar::<_, bool>("SELECT authz.lock_workspace_member_activity($1)")
            .bind(workspace_id)
            .fetch_one(&self.pool)
            .await?
        {
            return Err(G5Error::Forbidden {
                missing_scopes: Vec::new(),
            });
        }
        if handle.pending_transfer {
            return Err(G5Error::WorkspaceStopping);
        }
        match handle.status.as_str() {
            "running" => Ok(handle),
            "stopping" => Err(G5Error::WorkspaceStopping),
            _ => Err(G5Error::WorkspaceNotRunning),
        }
    }

    /// Read the timestamp authority used by idle admission. Callers retain
    /// owner admission; sampling this clock never wakes or renews a VM.
    pub async fn activity_clock(&self) -> Result<DateTime<Utc>, G5Error> {
        Ok(sqlx::query_scalar("SELECT clock_timestamp()")
            .fetch_one(&self.pool)
            .await?)
    }

    /// Record a separate, short-lived user gesture. Replayed reports retain
    /// their original event time and incarnation; neither retries nor a new
    /// VM can turn an old gesture into fresh activity. This never starts a VM.
    pub async fn record_user_activity(
        &self,
        handle: &WorkspaceRuntimeHandle,
        reason: WorkspaceActivityReason,
        correlation_id: &str,
        observed_at: DateTime<Utc>,
    ) -> Result<(), G5Error> {
        record_activity(
            &self.pool,
            handle.workspace_id,
            handle.vm_id,
            handle.epoch,
            handle.owner_user_id,
            reason,
            correlation_id,
            Some(observed_at),
        )
        .await?;
        Ok(())
    }

    /// Freeze an already captured owner attachment while admitting durable
    /// work. Call after the workspace commit fence, before queuing a task.
    pub async fn lock_owner_handle(
        &self,
        db: &mut g5_authz::ScopedDb,
        handle: &WorkspaceRuntimeHandle,
    ) -> Result<(), G5Error> {
        let running: Option<bool> = sqlx::query_scalar(
            "SELECT vm.status='running'
                    AND vm.vm_id=$2 AND vm.epoch=$3 AND NOT w.is_base
                    AND w.owner_user_id=$4 AND w.deleted_at IS NULL
                    AND NOT EXISTS (SELECT 1 FROM workspace_ownership_transfers t
                                    WHERE t.workspace_id=w.id AND t.status='pending')
             FROM workspaces w JOIN workspace_vms vm ON vm.workspace_id=w.id AND vm.org_id=w.org_id
             WHERE w.id=$1 FOR UPDATE OF w, vm",
        )
        .bind(handle.workspace_id)
        .bind(handle.vm_id.to_string())
        .bind(handle.epoch as i64)
        .bind(handle.owner_user_id)
        .fetch_optional(db.conn())
        .await?;
        if running != Some(true)
            || lease_running(db.conn(), handle.workspace_id).await? != Some(true)
        {
            return Err(G5Error::WorkspaceNotRunning);
        }
        crate::services::PermissionService::require_active_member_on_conn(
            db.conn(),
            handle.owner_user_id,
            handle.org_id,
            true,
        )
        .await?;
        Ok(())
    }

    /// Freeze the owner's workspace while admitting durable work bound to
    /// their Start consent `intent_id` (`g5.chat.start-and-send`,
    /// `g5.git.stopped-rebase.resolve-with-agent`): the consent is still
    /// theirs and live (recorded, waiting or starting, within its lifetime),
    /// the workspace is not Base, not deleted and not being transferred, and
    /// the owner is an active member. Call after the workspace commit fence,
    /// before queuing the task; the binding itself is settled against the
    /// same VM row by `bind_input_to_start_consent`. This never starts a VM.
    pub async fn lock_owner_pending_start(
        &self,
        db: &mut g5_authz::ScopedDb,
        workspace_id: Uuid,
        actor_user_id: Uuid,
        intent_id: Uuid,
    ) -> Result<(), G5Error> {
        let row: Option<(bool, Uuid)> = sqlx::query_as(
            "SELECT vm.start_intent ->> 'id' = $2
                    AND vm.start_intent ->> 'status' IN ('pending', 'queued_for_capacity', 'starting')
                    AND (vm.start_intent ->> 'expires_at')::timestamptz > now()
                    AND (vm.start_intent ->> 'owner_user_id')::uuid = $3
                    AND NOT w.is_base AND w.owner_user_id = $3 AND w.deleted_at IS NULL
                    AND NOT EXISTS (SELECT 1 FROM workspace_ownership_transfers t
                                    WHERE t.workspace_id = w.id AND t.status = 'pending'),
                    w.org_id
             FROM workspaces w JOIN workspace_vms vm ON vm.workspace_id = w.id AND vm.org_id = w.org_id
             WHERE w.id = $1 FOR UPDATE OF w, vm",
        )
        .bind(workspace_id)
        .bind(intent_id.to_string())
        .bind(actor_user_id)
        .fetch_optional(db.conn())
        .await?;
        let Some((live, org_id)) = row else {
            return Err(G5Error::WorkspaceNotRunning);
        };
        if !live {
            return Err(G5Error::Conflict {
                reason: "The Start ended before the agent's message was queued; nothing was sent"
                    .to_string(),
            });
        }
        crate::services::PermissionService::require_active_member_on_conn(
            db.conn(),
            actor_user_id,
            org_id,
            true,
        )
        .await?;
        Ok(())
    }

    /// Open a terminal stream after resolving current owner and incarnation
    /// authority. This has no ensure path: stopped and transitional workspaces
    /// fail before a tunnel is contacted. Preview streams open through
    /// [`Self::open_preview_stream`], which binds them to an incarnation.
    pub async fn open_owner_stream(
        &self,
        workspace_id: Uuid,
        actor_user_id: Uuid,
        channel: Channel,
    ) -> Result<(WorkspaceRuntimeHandle, MuxStream, WorkspaceActivityGuard), G5Error> {
        if channel != Channel::Pty {
            return Err(internal("unsupported workspace data stream"));
        }
        let handle = self.owner_handle(workspace_id, actor_user_id).await?;
        let (stream, activity) = self.open_data_stream(&handle, channel).await?;
        Ok((handle, stream, activity))
    }

    /// Open the HTTP stream to a preview's guest port on the incarnation the
    /// preview runs on (`g5.preview.transport`). If the workspace's current
    /// incarnation is another one, the stream is refused rather than opened
    /// there.
    pub async fn open_preview_stream(
        &self,
        workspace_id: Uuid,
        vm_id: Uuid,
        epoch: u64,
        port: u16,
        actor_user_id: Uuid,
    ) -> Result<(MuxStream, WorkspaceActivityGuard), G5Error> {
        let handle = self.owner_handle(workspace_id, actor_user_id).await?;
        if handle.vm_id != vm_id || handle.epoch != epoch {
            return Err(G5Error::ServiceUnavailable {
                message: "PREVIEW_NOT_READY".to_string(),
            });
        }
        let (stream, activity) = self.open_data_stream(&handle, Channel::Http).await?;
        stream
            .send(&g5_internal_api::stream_mux::encode_http_port(port))
            .await?;
        Ok((stream, activity))
    }

    async fn open_data_stream(
        &self,
        handle: &WorkspaceRuntimeHandle,
        channel: Channel,
    ) -> Result<(MuxStream, WorkspaceActivityGuard), G5Error> {
        let membership =
            crate::services::identity::membership_activity::MembershipActivityGuard::capture(
                &self.pool,
                handle.org_id,
                handle.owner_user_id,
            )
            .await?;
        let Some(tunnel) = self.tunnels.get(&handle.host_id) else {
            return Err(G5Error::ServiceUnavailable {
                message: "workspace host tunnel is unavailable".to_string(),
            });
        };
        let stream = tokio::time::timeout(
            DATA_STREAM_OPEN_TIMEOUT,
            tunnel.open_workspace_stream(handle.vm_id, channel),
        )
        .await
        .map_err(|_| G5Error::ServiceUnavailable {
            message: "workspace dispatcher stream timed out".to_string(),
        })??;
        let activity = WorkspaceActivityGuard {
            pool: self.pool.clone(),
            workspace_id: handle.workspace_id,
            owner_user_id: handle.owner_user_id,
            vm_id: handle.vm_id,
            epoch: handle.epoch,
            last_write_ms: Arc::new(AtomicU64::new(0)),
            membership,
        };
        if !activity.authorize().await {
            return Err(G5Error::Forbidden {
                missing_scopes: Vec::new(),
            });
        }
        Ok((stream, activity))
    }

    /// Call `method` on the incarnation in `handle` with `request`'s
    /// authority. A stream that closes before any reply — the host had no
    /// dispatcher connection to route it to, or the one carrying it was
    /// replaced — is resent a bounded number of times with the same
    /// parameters, idempotency key and deadline, each attempt behind the full
    /// admission check. The dispatcher answers a repeated key with the
    /// outcome it recorded instead of running the operation twice. Lifecycle
    /// gate methods are not resent here: their callers own that retry.
    pub(crate) async fn call(
        &self,
        handle: &WorkspaceRuntimeHandle,
        request: &RuntimeRequest,
        method: &str,
        mut params: Value,
    ) -> Result<Value, RuntimeRpcError> {
        if method == "workspace.flush" {
            tracing::warn!(workspace_id = %handle.workspace_id, vm_id = %handle.vm_id, "workspace flush runtime call entered");
        }
        if request.workspace_id != handle.workspace_id
            || request.expected_vm_id != handle.vm_id
            || request.expected_epoch != handle.epoch
        {
            return Err(runtime_error(409, "STALE_INCARNATION"));
        }
        if request.lifecycle {
            if request.actor_user_id.is_some() {
                return Err(runtime_error(403, "LIFECYCLE_AUTHORITY_REQUIRED"));
            }
        } else if request.actor_user_id != Some(handle.owner_user_id) {
            return Err(runtime_error(403, "OWNER_REQUIRED"));
        }
        if request.idempotency_key.is_empty() || request.idempotency_key.len() > 256 {
            return Err(runtime_error(-32602, "INVALID_IDEMPOTENCY_KEY"));
        }
        let Some(object) = params.as_object_mut() else {
            return Err(runtime_error(-32602, "INVALID_PARAMS"));
        };
        object.insert("context".to_string(), request.context());
        let id = format!(
            "runtime:{}",
            self.next_request.fetch_add(1, Ordering::Relaxed)
        );
        let mut encoded = serde_json::to_vec(&json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        }))
        .map_err(|_| runtime_error(-32603, "SERIALIZATION_FAILED"))?;
        encoded.push(b'\n');
        let mut resends = 0;
        loop {
            let error = match self.call_once(handle, request, method, &id, &encoded).await {
                Err(error) if resends_closed_stream(method, &error) => error,
                result => return result,
            };
            let Some(&delay_ms) = CLOSED_STREAM_RESEND_DELAYS_MS.get(resends) else {
                return Err(error);
            };
            if unix_time_ms().saturating_add(delay_ms + MIN_RESEND_BUDGET_MS) > request.deadline_ms
            {
                return Err(error);
            }
            resends += 1;
            tracing::warn!(
                workspace_id = %handle.workspace_id,
                vm_id = %handle.vm_id,
                method,
                resend = resends,
                "workspace runtime stream closed without a reply; resending under the same idempotency key"
            );
            tokio::time::sleep(std::time::Duration::from_millis(delay_ms)).await;
        }
    }

    /// One admitted attempt of [`Self::call`] on its own dispatcher stream.
    async fn call_once(
        &self,
        handle: &WorkspaceRuntimeHandle,
        request: &RuntimeRequest,
        method: &str,
        id: &str,
        encoded: &[u8],
    ) -> Result<Value, RuntimeRpcError> {
        let mut admission = self
            .pool
            .begin()
            .await
            .map_err(|_| runtime_error(503, "RUNTIME_AUTHORITY_UNAVAILABLE"))?;
        let workspace = sqlx::query(
            "SELECT org_id, owner_user_id, deleted_at, pending_transfer
             FROM authz.lookup_workspace_runtime_authority($1)",
        )
        .bind(handle.workspace_id)
        .fetch_optional(&mut *admission)
        .await
        .map_err(|_| runtime_error(503, "RUNTIME_AUTHORITY_UNAVAILABLE"))?
        .ok_or_else(|| runtime_error(409, "STALE_INCARNATION"))?;
        let vm = sqlx::query(
            "SELECT host_id, vm_id, epoch, status
             FROM workspace_vms WHERE workspace_id = $1 FOR SHARE",
        )
        .bind(handle.workspace_id)
        .fetch_optional(&mut *admission)
        .await
        .map_err(|_| runtime_error(503, "RUNTIME_AUTHORITY_UNAVAILABLE"))?
        .ok_or_else(|| runtime_error(409, "STALE_INCARNATION"))?;
        let lease_valid = lease_running(&mut admission, handle.workspace_id)
            .await
            .map_err(|_| runtime_error(503, "RUNTIME_AUTHORITY_UNAVAILABLE"))?
            .ok_or_else(|| runtime_error(503, "RUNTIME_AUTHORITY_UNAVAILABLE"))?;
        if method == "workspace.flush" {
            tracing::warn!(workspace_id = %handle.workspace_id, "workspace flush runtime authority read");
        }
        let current_owner = workspace
            .try_get::<Uuid, _>("owner_user_id")
            .map_err(|_| runtime_error(503, "RUNTIME_AUTHORITY_UNAVAILABLE"))?;
        let current_vm_id = vm
            .try_get::<Option<String>, _>("vm_id")
            .map_err(|_| runtime_error(503, "RUNTIME_AUTHORITY_UNAVAILABLE"))?;
        let current_epoch = vm
            .try_get::<i64, _>("epoch")
            .map_err(|_| runtime_error(503, "RUNTIME_AUTHORITY_UNAVAILABLE"))?;
        let current_status = vm
            .try_get::<String, _>("status")
            .map_err(|_| runtime_error(503, "RUNTIME_AUTHORITY_UNAVAILABLE"))?;
        if workspace
            .try_get::<Uuid, _>("org_id")
            .map_err(|_| runtime_error(503, "RUNTIME_AUTHORITY_UNAVAILABLE"))?
            != handle.org_id
            || workspace
                .try_get::<Option<chrono::DateTime<chrono::Utc>>, _>("deleted_at")
                .map_err(|_| runtime_error(503, "RUNTIME_AUTHORITY_UNAVAILABLE"))?
                .is_some()
            || current_owner != handle.owner_user_id
            || vm
                .try_get::<Option<Uuid>, _>("host_id")
                .map_err(|_| runtime_error(503, "RUNTIME_AUTHORITY_UNAVAILABLE"))?
                != Some(handle.host_id)
            || current_vm_id.as_deref() != Some(handle.vm_id.to_string().as_str())
            || current_epoch != handle.epoch as i64
        {
            return Err(runtime_error(409, "STALE_INCARNATION"));
        }
        if !lease_valid {
            return Err(runtime_error(409, "LEASE_EXPIRED"));
        }
        if request.lifecycle {
            if !matches!(current_status.as_str(), "starting" | "running" | "stopping") {
                return Err(runtime_error(409, "STALE_INCARNATION"));
            }
        } else {
            let active: bool =
                sqlx::query_scalar("SELECT authz.lock_workspace_member_activity($1)")
                    .bind(handle.workspace_id)
                    .fetch_one(&mut *admission)
                    .await
                    .map_err(|_| runtime_error(503, "RUNTIME_AUTHORITY_UNAVAILABLE"))?;
            if !active {
                return Err(runtime_error(403, "OWNER_REQUIRED"));
            }
            if workspace
                .try_get::<bool, _>("pending_transfer")
                .map_err(|_| runtime_error(503, "RUNTIME_AUTHORITY_UNAVAILABLE"))?
            {
                return Err(runtime_error(409, "WORKSPACE_STOPPING"));
            }
            match current_status.as_str() {
                "running" => {}
                "stopping" => return Err(runtime_error(409, "WORKSPACE_STOPPING")),
                _ => return Err(runtime_error(409, "WORKSPACE_NOT_RUNNING")),
            }
        }
        // The checks are done and nothing durable was written: the
        // transaction ends here, before the guest is asked, so the call holds
        // no pooled connection through its round trip. The call is counted
        // in process instead, while the row's share lock is still held: a
        // stop or transfer holding that row exclusively therefore sees every
        // call whose admission committed, and a call still waiting for the
        // row is not counted and is refused once the row is released
        // (`g5.runtime.guest-call-admission`). Callers that act on the reply
        // re-verify the incarnation when they apply it; the flush callback
        // repeats the owner/VM/epoch/lease checks before recording a receipt.
        let _in_flight = (!lifecycle_gated(method))
            .then(|| self.enter_call(handle.workspace_id, request.deadline_ms));
        admission
            .commit()
            .await
            .map_err(|_| runtime_error(503, "RUNTIME_AUTHORITY_UNAVAILABLE"))?;
        let timeout_ms = request.deadline_ms.saturating_sub(unix_time_ms());
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_millis(timeout_ms);
        let Some(tunnel) = self.tunnels.get(&handle.host_id) else {
            tracing::warn!(
                host_id = %handle.host_id,
                vm_id = %handle.vm_id,
                method,
                live_tunnels = self.tunnels.len(),
                "workspace runtime call has no tunnel for the host in this process"
            );
            return Err(runtime_error(503, "HOST_TUNNEL_UNAVAILABLE"));
        };
        let stream = tokio::time::timeout_at(deadline, tunnel.open_workspace_rpc(handle.vm_id))
            .await
            .map_err(|_| runtime_error(408, "DEADLINE_EXCEEDED"))?
            .map_err(|error| {
                tracing::warn!(
                    host_id = %handle.host_id,
                    vm_id = %handle.vm_id,
                    method,
                    %error,
                    "workspace runtime call could not open the host rpc stream"
                );
                runtime_error(503, "HOST_TUNNEL_UNAVAILABLE")
            })?;
        if method == "workspace.flush" {
            tracing::warn!(workspace_id = %handle.workspace_id, "workspace flush runtime rpc stream opened");
        }
        let (writer, mut reader) = stream.into_split();
        let sent = tokio::time::timeout_at(deadline, writer.send(encoded)).await;
        // Each API call owns exactly one request stream. Half-close it after
        // the complete request so the dispatcher can drain the reply and then
        // close its reverse direction, releasing both mux registries.
        writer.close().await;
        sent.map_err(|_| runtime_error(408, "DEADLINE_EXCEEDED"))?
            .map_err(|_| runtime_error(503, "HOST_TUNNEL_CLOSED"))?;
        if method == "workspace.flush" {
            tracing::warn!(workspace_id = %handle.workspace_id, "workspace flush runtime request sent");
        }
        let line = tokio::time::timeout(
            deadline.saturating_duration_since(tokio::time::Instant::now()),
            reader.read_line_with_limit(MAX_RUNTIME_REPLY_BYTES),
        )
        .await
        .map_err(|_| runtime_error(408, "DEADLINE_EXCEEDED"))?
        .ok_or_else(|| runtime_error(503, "HOST_TUNNEL_CLOSED"))?;
        let reply: RpcReply = serde_json::from_str(&line)
            .map_err(|_| runtime_error(-32603, "INVALID_RUNTIME_REPLY"))?;
        if reply.id.as_str() != Some(id) {
            return Err(runtime_error(-32603, "INVALID_RUNTIME_REPLY"));
        }
        if let Some(error) = reply.error {
            return Err(RuntimeRpcError {
                code: error.code,
                message: error.message,
                data: error.data,
            });
        }
        let result = reply
            .result
            .ok_or_else(|| runtime_error(-32603, "INVALID_RUNTIME_REPLY"))?;
        Ok(result)
    }

    /// Test support: a dispatcher call with lifecycle authority on the
    /// incarnation in `handle`, made the way lifecycle owners in this crate
    /// make it.
    #[cfg(feature = "test-helpers")]
    pub async fn lifecycle_call_for_test(
        &self,
        handle: &WorkspaceRuntimeHandle,
        method: &str,
        params: Value,
        idempotency_key: &str,
    ) -> Result<Value, RuntimeRpcError> {
        let request = RuntimeRequest::lifecycle(
            handle.workspace_id,
            handle.vm_id,
            handle.epoch,
            unix_time_ms().saturating_add(30_000),
            idempotency_key,
        );
        self.call(handle, &request, method, params).await
    }

    pub async fn file_read(
        &self,
        handle: &WorkspaceRuntimeHandle,
        request: &RuntimeRequest,
        repo_id: Uuid,
        path: &str,
    ) -> Result<Value, RuntimeRpcError> {
        self.call(
            handle,
            request,
            "fs.watch",
            json!({"repo_id": repo_id, "path": path}),
        )
        .await?;
        self.call(
            handle,
            request,
            "fs.read",
            json!({"repo_id": repo_id, "path": path}),
        )
        .await
    }

    /// One window of a file: `fs.read` with an offset and a length, for a
    /// file larger than one runtime reply carries. The reply names the whole
    /// file's size and hash next to the window's base64 bytes.
    pub async fn file_read_window(
        &self,
        handle: &WorkspaceRuntimeHandle,
        request: &RuntimeRequest,
        repo_id: Uuid,
        path: &str,
        offset: u64,
        length: u64,
    ) -> Result<Value, RuntimeRpcError> {
        self.call(
            handle,
            request,
            "fs.read",
            json!({"repo_id": repo_id, "path": path, "offset": offset, "length": length}),
        )
        .await
    }

    pub async fn file_list(
        &self,
        handle: &WorkspaceRuntimeHandle,
        request: &RuntimeRequest,
        repo_id: Uuid,
        path: &str,
    ) -> Result<Value, RuntimeRpcError> {
        self.call(
            handle,
            request,
            "fs.watch",
            json!({"repo_id": repo_id, "path": path}),
        )
        .await?;
        self.call(
            handle,
            request,
            "fs.list",
            json!({"repo_id": repo_id, "path": path}),
        )
        .await
    }

    /// The whole subtree under `path` in one `fs.tree` call, pruned by the
    /// checkout's `.gitignore` files except for `tracked` paths and bounded
    /// by `max_entries` entries and `max_bytes` of serialized entries (the
    /// reply says `truncated` past either). One watch on `path` covers the
    /// subtree for later change notifications (`g5.documents.read.live-tree`).
    #[allow(clippy::too_many_arguments)]
    pub async fn file_tree(
        &self,
        handle: &WorkspaceRuntimeHandle,
        request: &RuntimeRequest,
        repo_id: Uuid,
        path: &str,
        depth: i32,
        tracked: &std::collections::BTreeSet<String>,
        max_entries: usize,
        max_bytes: usize,
    ) -> Result<Value, RuntimeRpcError> {
        let watch = self
            .call(
                handle,
                request,
                "fs.watch",
                json!({"repo_id": repo_id, "path": path}),
            )
            .await?;
        if watch["watching"].as_bool() == Some(false) {
            // The pruned directory set still exceeded the native watch
            // limit: the listing below is live but will not refresh on
            // its own until the subtree shrinks.
            tracing::warn!(
                workspace_id = %handle.workspace_id,
                repo_id = %repo_id,
                path,
                "live file tree is served without a watch; the subtree exceeds the watch limit"
            );
        }
        self.call(
            handle,
            request,
            "fs.tree",
            json!({
                "repo_id": repo_id,
                "path": path,
                "depth": depth,
                "tracked": tracked,
                "max_entries": max_entries,
                "max_bytes": max_bytes,
            }),
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn file_write(
        &self,
        handle: &WorkspaceRuntimeHandle,
        request: &RuntimeRequest,
        repo_id: Uuid,
        path: &str,
        content: &str,
        encoding: &str,
        expected_hash: Option<&str>,
    ) -> Result<Value, RuntimeRpcError> {
        self.call(
            handle,
            request,
            "fs.write",
            json!({
                "repo_id": repo_id,
                "path": path,
                "content": content,
                "encoding": encoding,
                "expected_hash": expected_hash,
            }),
        )
        .await
    }

    pub async fn file_delete(
        &self,
        handle: &WorkspaceRuntimeHandle,
        request: &RuntimeRequest,
        repo_id: Uuid,
        path: &str,
        expected_hash: Option<&str>,
    ) -> Result<Value, RuntimeRpcError> {
        self.call(
            handle,
            request,
            "fs.delete",
            json!({
                "repo_id": repo_id,
                "path": path,
                "expected_hash": expected_hash,
            }),
        )
        .await
    }

    pub async fn file_rename(
        &self,
        handle: &WorkspaceRuntimeHandle,
        request: &RuntimeRequest,
        repo_id: Uuid,
        from: &str,
        to: &str,
        expected_hash: Option<&str>,
    ) -> Result<Value, RuntimeRpcError> {
        self.call(
            handle,
            request,
            "fs.rename",
            json!({
                "repo_id": repo_id,
                "from": from,
                "to": to,
                "expected_hash": expected_hash,
            }),
        )
        .await
    }

    /// Report the workspace change set without ever waking a VM. The current
    /// owner sees the live checkout only while its running incarnation remains
    /// authoritative; every reader and stopped owner sees the last durable
    /// branch/flush projection instead. `scope` is the caller's tenant scope,
    /// which names the durable projection's repositories
    /// (`g5.repo.storage-tenancy`).
    pub async fn workspace_changes(
        &self,
        scope: &(impl OrgScope + ?Sized),
        workspace_id: Uuid,
        actor_user_id: Uuid,
    ) -> Result<Value, G5Error> {
        let report = self
            .workspace_changes_report(scope, workspace_id, actor_user_id)
            .await?;
        if report["source"] == "live" && live_report_has_association_mismatch(&report) {
            return Err(G5Error::WorkspaceBranchMismatch { details: report });
        }
        Ok(report)
    }

    /// Return Changes data for display. Unlike mutation admission, this keeps
    /// an association mismatch in the report so the owner can see the affected
    /// member and invoke `switch_back`.
    pub async fn workspace_changes_report(
        &self,
        scope: &(impl OrgScope + ?Sized),
        workspace_id: Uuid,
        actor_user_id: Uuid,
    ) -> Result<Value, G5Error> {
        let workspace = sqlx::query(
            "SELECT authority.org_id, authority.owner_user_id,
                    EXISTS(
                        SELECT 1 FROM workspace_vms AS vm
                        WHERE vm.workspace_id = authority.workspace_id
                          AND vm.status = 'running'
                          AND vm.lease_expires_at > clock_timestamp()
                    ) AS attached
             FROM authz.lookup_workspace_runtime_authority($1) AS authority
             WHERE authority.deleted_at IS NULL",
        )
        .bind(workspace_id)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| G5Error::NotFound {
            resource: format!("workspace {workspace_id}"),
        })?;
        let org_id: Uuid = workspace.try_get("org_id")?;
        let owner_user_id: Uuid = workspace.try_get("owner_user_id")?;
        let attached: bool = workspace.try_get("attached")?;
        if owner_user_id == actor_user_id && attached {
            if let Some(handle) = self.get(workspace_id).await? {
                if handle.status == "running" && handle.owner_user_id == actor_user_id {
                    // The file tree and Changes panels request this together
                    // for every member after an edit; the VM admits only a
                    // couple of concurrent `workspace.changes` runs, so share
                    // one fresh run among concurrent readers.
                    let reads = self.changes_reads.for_key(ChangesReadKey {
                        workspace_id,
                        vm_id: handle.vm_id,
                        epoch: handle.epoch,
                        actor_user_id,
                    });
                    // One budget from arrival bounds both the wait behind other
                    // readers and this caller's own read; the read cancels its
                    // VM execution if it reaches that deadline.
                    let deadline = tokio::time::Instant::now()
                        + std::time::Duration::from_millis(CHANGES_RPC_BUDGET_MS);
                    let deadline_ms = unix_time_ms().saturating_add(CHANGES_RPC_BUDGET_MS);
                    return reads
                        .run(
                            deadline,
                            || {
                                self.invoke_workspace_changes(
                                    &handle,
                                    actor_user_id,
                                    "get",
                                    None,
                                    ChangesExtras {
                                        deadline_ms: Some(deadline_ms),
                                        ..ChangesExtras::default()
                                    },
                                )
                            },
                            || G5Error::ServiceUnavailable {
                                message: "workspace Changes query exceeded its deadline"
                                    .to_string(),
                            },
                        )
                        .await;
                }
            }
        }
        self.durable_workspace_changes(scope, org_id, workspace_id, actor_user_id)
            .await
    }

    /// Invoke an owner-only native Changes action on the current incarnation.
    pub async fn workspace_changes_action(
        &self,
        workspace_id: Uuid,
        actor_user_id: Uuid,
        operation: &str,
        repo_id: Option<Uuid>,
    ) -> Result<Value, G5Error> {
        if !matches!(operation, "push" | "update" | "switch_back") {
            return Err(G5Error::ValidationError {
                details: json!({"operation": "unsupported workspace Changes action"}),
            });
        }
        let handle = self.owner_handle(workspace_id, actor_user_id).await?;
        self.invoke_workspace_changes(
            &handle,
            actor_user_id,
            operation,
            repo_id,
            ChangesExtras::default(),
        )
        .await
    }

    /// Update every member from its parent branch (`workspace.changes`
    /// `update`). With `favor_base`, where a commit of the workspace
    /// collides with what the base already holds, the base's side of those
    /// paths is kept and the replay goes on; the outcome names them in
    /// `resolved`. That is a review copy's update: its rounds reach the
    /// source as the owner's decisions, which stand. Without it a collision
    /// fails the update and the branch stays where it was.
    pub async fn workspace_changes_update(
        &self,
        workspace_id: Uuid,
        actor_user_id: Uuid,
        repo_id: Option<Uuid>,
        favor_base: bool,
    ) -> Result<Value, G5Error> {
        let handle = self.owner_handle(workspace_id, actor_user_id).await?;
        self.invoke_workspace_changes(
            &handle,
            actor_user_id,
            "update",
            repo_id,
            ChangesExtras {
                review: None,
                message: None,
                favor_base,
                deadline_ms: None,
                paths: None,
                expected_blobs: None,
                push_head: None,
                rebase: None,
                adopt: None,
            },
        )
        .await
    }

    /// Commit every member's working tree on its shared branch as one
    /// commit with `message` (`workspace.changes` `commit`): the VM stages
    /// everything, untracked files included, and commits under the
    /// workspace owner's identity; a clean member commits nothing. The
    /// outcome reports `committed` and `head` per member.
    pub async fn workspace_changes_commit(
        &self,
        workspace_id: Uuid,
        actor_user_id: Uuid,
        repo_id: Option<Uuid>,
        message: &str,
    ) -> Result<Value, G5Error> {
        let handle = self.owner_handle(workspace_id, actor_user_id).await?;
        self.invoke_workspace_changes(
            &handle,
            actor_user_id,
            "commit",
            repo_id,
            ChangesExtras {
                review: None,
                message: Some(message),
                favor_base: false,
                deadline_ms: None,
                paths: None,
                expected_blobs: None,
                push_head: None,
                rebase: None,
                adopt: None,
            },
        )
        .await
    }

    /// Commit only the named repository-relative files, including new files.
    pub async fn workspace_changes_commit_paths(
        &self,
        workspace_id: Uuid,
        actor_user_id: Uuid,
        repo_id: Uuid,
        paths: &[String],
        expected_blobs: &std::collections::BTreeMap<String, String>,
        message: &str,
    ) -> Result<Value, G5Error> {
        let handle = self.owner_handle(workspace_id, actor_user_id).await?;
        self.invoke_workspace_changes(
            &handle,
            actor_user_id,
            "commit",
            Some(repo_id),
            ChangesExtras {
                review: None,
                message: Some(message),
                favor_base: false,
                deadline_ms: None,
                paths: Some(paths),
                expected_blobs: Some(expected_blobs),
                push_head: None,
                rebase: None,
                adopt: None,
            },
        )
        .await
    }

    /// Rebase one member in the owner VM of `handle` onto the captured target
    /// head and publish the result to G5 with the captured G5 head as its
    /// only lease (`g5.architecture.workspace_rebase`). It runs on that exact
    /// incarnation; the outcome reports `rebased`, `head` and `published`, or
    /// the member's refusal.
    pub async fn workspace_changes_rebase(
        &self,
        handle: &WorkspaceRuntimeHandle,
        actor_user_id: Uuid,
        repo_id: Uuid,
        expected_head: &str,
        onto_branch: &str,
        onto_head: &str,
    ) -> Result<Value, G5Error> {
        self.invoke_workspace_changes(
            handle,
            actor_user_id,
            "rebase",
            Some(repo_id),
            ChangesExtras {
                deadline_ms: Some(unix_time_ms().saturating_add(CHANGES_REBASE_BUDGET_MS)),
                rebase: Some(ChangesRebase {
                    expected_head,
                    onto_branch,
                    onto_head,
                }),
                ..ChangesExtras::default()
            },
        )
        .await
    }

    /// Reset one member in the owner VM of `handle` to the reviewed provider
    /// tip, pinned as divergence run `run_id`'s remote input, and publish it
    /// to G5 with the captured G5 head as its only lease
    /// (`g5.workspace.remote.adopt-provider-history`). The outcome reports
    /// `adopted`, `head` and `published`, or the member's refusal.
    pub async fn workspace_changes_adopt(
        &self,
        handle: &WorkspaceRuntimeHandle,
        actor_user_id: Uuid,
        repo_id: Uuid,
        run_id: Uuid,
        expected_head: &str,
        provider_tip: &str,
    ) -> Result<Value, G5Error> {
        self.invoke_workspace_changes(
            handle,
            actor_user_id,
            "adopt",
            Some(repo_id),
            ChangesExtras {
                deadline_ms: Some(unix_time_ms().saturating_add(CHANGES_REBASE_BUDGET_MS)),
                adopt: Some(ChangesAdopt {
                    run_id,
                    expected_head,
                    provider_tip,
                }),
                ..ChangesExtras::default()
            },
        )
        .await
    }

    pub async fn workspace_changes_push_head(
        &self,
        workspace_id: Uuid,
        actor_user_id: Uuid,
        repo_id: Uuid,
        head: &str,
    ) -> Result<Value, G5Error> {
        let handle = self.owner_handle(workspace_id, actor_user_id).await?;
        self.invoke_workspace_changes(
            &handle,
            actor_user_id,
            "push",
            Some(repo_id),
            ChangesExtras {
                push_head: Some(head),
                ..ChangesExtras::default()
            },
        )
        .await
    }

    /// Save the owner's working tree before PR evidence is captured. Both
    /// operations use one incarnation: a stop/transfer between commit and push
    /// must fail, never continue against a replacement VM. Retrying a clean
    /// commit still pushes, recovering a previous partial publication.
    pub async fn prepare_pull_request_source(
        &self,
        workspace_id: Uuid,
        actor_user_id: Uuid,
        expected_incarnation: Option<(Uuid, u64)>,
    ) -> Result<(), G5Error> {
        self.save_workspace_changes(
            workspace_id,
            actor_user_id,
            expected_incarnation,
            "Save workspace changes for pull request",
        )
        .await
    }

    pub async fn save_source_control(
        &self,
        workspace_id: Uuid,
        actor_user_id: Uuid,
        incarnation: (Uuid, u64),
    ) -> Result<(), G5Error> {
        self.save_workspace_changes(
            workspace_id,
            actor_user_id,
            Some(incarnation),
            "Save workspace changes",
        )
        .await
    }

    async fn save_workspace_changes(
        &self,
        workspace_id: Uuid,
        actor_user_id: Uuid,
        expected_incarnation: Option<(Uuid, u64)>,
        message: &str,
    ) -> Result<(), G5Error> {
        if matches!(
            self.durable_status(workspace_id).await?.as_deref(),
            None | Some("stopped")
        ) {
            if expected_incarnation.is_some() {
                return Err(G5Error::WorkspaceNotRunning);
            }
            // Flush snapshots preserve dirty files independently of branch
            // commits. Never silently create a PR that omits saved drafts.
            let rows: Vec<(Option<Value>,)> = sqlx::query_as(
                "SELECT flush_repos FROM authz.lookup_workspace_runtime_durable_members($1, $2) LIMIT 1",
            )
            .bind(workspace_id)
            .bind(actor_user_id)
            .fetch_all(&self.pool)
            .await?;
            if let Some((Some(value),)) = rows.into_iter().next() {
                let receipts: Vec<FlushRepoReceipt> =
                    serde_json::from_value(value).map_err(|error| {
                        internal(&format!("invalid durable flush receipt: {error}"))
                    })?;
                if receipts.iter().any(|repo| {
                    repo.dirty
                        || repo.local_head_oid != repo.branch_oid
                        || repo.sequencer != SequencerState::None
                }) {
                    return Err(G5Error::Conflict {
                        reason:
                            "Start the workspace to save its pending changes before creating the PR"
                                .to_string(),
                    });
                }
            }
            return Ok(());
        }
        let handle = self.owner_handle(workspace_id, actor_user_id).await?;
        if expected_incarnation.is_some_and(|expected| expected != (handle.vm_id, handle.epoch)) {
            return Err(G5Error::WorkspaceNotRunning);
        }
        for operation in ["commit", "push"] {
            let report = self
                .invoke_workspace_changes(
                    &handle,
                    actor_user_id,
                    operation,
                    None,
                    ChangesExtras {
                        review: None,
                        message: (operation == "commit").then_some(message),
                        favor_base: false,
                        deadline_ms: None,
                        paths: None,
                        expected_blobs: None,
                        push_head: None,
                        rebase: None,
                        adopt: None,
                    },
                )
                .await?;
            let failures: Vec<_> = report["members"]
                .as_array()
                .into_iter()
                .flatten()
                .filter(|member| {
                    member["errors"]
                        .as_array()
                        .is_some_and(|errors| !errors.is_empty())
                })
                .map(|member| format!("{}: {}", member["repo_id"], member["errors"]))
                .collect();
            if report["status"] != "succeeded" || !failures.is_empty() {
                return Err(G5Error::Conflict {
                    reason: format!("Could not {operation} workspace changes: {}. Saved commits are retained; resolve the issue and retry the save.", failures.join("; ")),
                });
            }
        }
        Ok(())
    }

    /// A named comparison on the current owner's running incarnation only.
    pub async fn review_inspect(
        &self,
        workspace_id: Uuid,
        actor_user_id: Uuid,
        repo_id: Uuid,
        mode: &str,
        pushed: &str,
        path: Option<&str>,
    ) -> Result<Value, G5Error> {
        let handle = self.owner_handle(workspace_id, actor_user_id).await?;
        self.invoke_workspace_changes(
            &handle,
            actor_user_id,
            "inspect",
            Some(repo_id),
            ChangesExtras {
                review: Some((mode, pushed, path)),
                ..ChangesExtras::default()
            },
        )
        .await
    }

    /// Operation-specific input of `workspace.changes` beyond the member
    /// selection.
    async fn invoke_workspace_changes(
        &self,
        handle: &WorkspaceRuntimeHandle,
        actor_user_id: Uuid,
        operation: &str,
        repo_id: Option<Uuid>,
        extras: ChangesExtras<'_>,
    ) -> Result<Value, G5Error> {
        let ChangesExtras {
            message,
            favor_base,
            deadline_ms,
            paths,
            expected_blobs,
            push_head,
            rebase,
            review,
            adopt,
        } = extras;
        let deadline_ms =
            deadline_ms.unwrap_or_else(|| unix_time_ms().saturating_add(CHANGES_RPC_BUDGET_MS));
        let (operation_id, exec_id, disposition) = {
            let operation_id = Uuid::new_v4();
            let exec_id = format!("operation.{operation_id}.1.{}", handle.vm_id);
            let mut changes_input = serde_json::Map::new();
            if let Some(repo_id) = repo_id {
                changes_input.insert("repo_id".to_string(), json!(repo_id));
            }
            if let Some(message) = message {
                changes_input.insert("message".to_string(), json!(message));
            }
            if let Some(paths) = paths {
                changes_input.insert("paths".to_string(), json!(paths));
            }
            if let Some(expected_blobs) = expected_blobs {
                changes_input.insert("expected_blobs".to_string(), json!(expected_blobs));
            }
            if let Some(push_head) = push_head {
                changes_input.insert("push_head".to_string(), json!(push_head));
            }
            if let Some(rebase) = rebase {
                changes_input.insert("expected_head".to_string(), json!(rebase.expected_head));
                changes_input.insert("onto_branch".to_string(), json!(rebase.onto_branch));
                changes_input.insert("onto_head".to_string(), json!(rebase.onto_head));
            }
            if let Some((mode, head, path)) = review {
                changes_input.insert("review_mode".into(), json!(mode));
                changes_input.insert("review_head".into(), json!(head));
                if let Some(path) = path {
                    changes_input.insert("review_path".into(), json!(path));
                }
            }
            if let Some(adopt) = adopt {
                changes_input.insert("run_id".to_string(), json!(adopt.run_id));
                changes_input.insert("expected_head".to_string(), json!(adopt.expected_head));
                changes_input.insert("provider_tip".to_string(), json!(adopt.provider_tip));
            }
            // Only ever sent as `true`: a VM whose catalog predates the field
            // refuses it with restart guidance below.
            if favor_base {
                changes_input.insert("favor_base".to_string(), json!(true));
            }
            let mut attempt = 0;
            let result = loop {
                let invoke = RuntimeRequest::owner(
                    handle,
                    actor_user_id,
                    deadline_ms,
                    format!("workspace-changes:{operation_id}:invoke:{attempt}"),
                );
                let result = self
                    .modules_invoke(
                        handle,
                        &invoke,
                        json!({
                            "exec_id": exec_id,
                            "module_id": "workspace.changes",
                            "version": "1.0.0",
                            "operation": operation,
                            "input_schema": format!("g5.workspace.changes.{operation}/v1"),
                            "input": Value::Object(changes_input.clone()),
                            "grant": {
                                "grant_id": operation_id,
                                "capabilities": ["workspace.read", "workspace.native_git"],
                            },
                        }),
                    )
                    .await;
                if matches!(
                    &result,
                    Err(RuntimeRpcError { code: 409, message, .. })
                        if matches!(message.as_str(), "MODULE_CONCURRENCY_EXHAUSTED" | "EXECUTION_SLOTS_EXHAUSTED")
                ) && attempt < 20
                    && unix_time_ms().saturating_add(100) < deadline_ms
                {
                    attempt += 1;
                    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                    continue;
                }
                break result;
            };
            match result {
                Ok(disposition) => (operation_id, exec_id, disposition),
                Err(error) if favor_base && rejects_unknown_field(&error, "favor_base") => {
                    tracing::warn!(
                        workspace_id = %handle.workspace_id,
                        vm_id = %handle.vm_id,
                        operation,
                        "workspace VM predates favor_base in workspace.changes input"
                    );
                    return Err(G5Error::Conflict {
                        reason: "this workspace's VM predates review-copy updates that keep the \
                                 source's side; stop and start the workspace, then retry"
                            .to_string(),
                    });
                }
                // An older guest cannot rebase in place; this never falls
                // back to a central rebase behind the running checkout.
                Err(error) if rebase.is_some() && error.message == "OPERATION_NOT_FOUND" => {
                    tracing::warn!(
                        workspace_id = %handle.workspace_id,
                        vm_id = %handle.vm_id,
                        "workspace VM predates in-VM source rebase"
                    );
                    return Err(G5Error::Conflict {
                        reason: "this workspace's VM predates rebasing in the workspace; stop and \
                                 start the workspace, then retry the rebase"
                            .to_string(),
                    });
                }
                // Adopting never falls back to a central reset behind the
                // running checkout either.
                Err(error) if adopt.is_some() && error.message == "OPERATION_NOT_FOUND" => {
                    tracing::warn!(
                        workspace_id = %handle.workspace_id,
                        vm_id = %handle.vm_id,
                        "workspace VM predates adopting provider history"
                    );
                    return Err(G5Error::Conflict {
                        reason: "this workspace's VM predates adopting provider history; stop \
                                 and start the workspace, then adopt again"
                            .to_string(),
                    });
                }
                Err(error) if paths.is_some() && rejects_unknown_field(&error, "paths") => {
                    tracing::warn!(
                        workspace_id = %handle.workspace_id,
                        vm_id = %handle.vm_id,
                        "workspace VM predates file-scoped commits"
                    );
                    return Err(G5Error::Conflict {
                        reason: "this workspace's VM predates automatic file commits; stop and \
                                 start the workspace, then retry Save or Accept"
                            .to_string(),
                    });
                }
                Err(error) => {
                    // A reply lost to the deadline may still have started the
                    // execution; release its module slot.
                    if error.code == 408 {
                        self.cancel_abandoned_changes(
                            handle,
                            actor_user_id,
                            operation_id,
                            &exec_id,
                        )
                        .await;
                    }
                    return Err(runtime_to_g5(error));
                }
            }
        };
        let request = |phase: &str| {
            RuntimeRequest::owner(
                handle,
                actor_user_id,
                deadline_ms,
                format!("workspace-changes:{operation_id}:{phase}"),
            )
        };
        let polled: Result<Value, G5Error> = async {
            let mut outcome = disposition.get("outcome").cloned();
            while outcome.is_none() {
                if unix_time_ms() >= deadline_ms {
                    return Err(G5Error::ServiceUnavailable {
                        message: "workspace Changes query exceeded its deadline".to_string(),
                    });
                }
                let status = self
                    .modules_status(handle, &request("status"), &exec_id)
                    .await
                    .map_err(runtime_to_g5)?;
                match status["status"].as_str() {
                    Some("finished") => outcome = status.get("outcome").cloned(),
                    Some("running") => {
                        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
                    }
                    Some("cancelled") => {
                        return Err(G5Error::Conflict {
                            reason: "workspace Changes query was cancelled".to_string(),
                        });
                    }
                    _ => {
                        return Err(G5Error::ServiceUnavailable {
                            message: "workspace Changes execution disappeared".to_string(),
                        });
                    }
                }
            }
            outcome.ok_or_else(|| G5Error::Internal {
                message: "workspace Changes outcome was absent".to_string(),
            })
        }
        .await;
        let outcome = match polled {
            Ok(outcome) => outcome,
            // Never leave an abandoned execution holding one of the module's
            // few concurrency slots.
            Err(error) => {
                self.cancel_abandoned_changes(handle, actor_user_id, operation_id, &exec_id)
                    .await;
                return Err(error);
            }
        };
        // The owner authorized this query on `modules.invoke`; draining and
        // acknowledging its journal is control-plane housekeeping. The
        // dispatcher keeps `events.replay` and `events.ack` private to the
        // lifecycle authority (protocol.rs), so the facade asserts that
        // authority here rather than the owner's actor, which the dispatcher
        // refuses with LIFECYCLE_AUTHORITY_REQUIRED.
        let housekeeping = |phase: &str| {
            RuntimeRequest::lifecycle(
                handle.workspace_id,
                handle.vm_id,
                handle.epoch,
                deadline_ms,
                format!("workspace-changes:{operation_id}:{phase}"),
            )
        };
        // Consume every page before acknowledging the whole prefix
        // (`g5.workspace.module-execution`).
        let mut from_seq = 1u64;
        let mut last_seq = None;
        let mut page = 0u32;
        loop {
            page += 1;
            let replay = self
                .modules_events_page(
                    handle,
                    &housekeeping(&format!("replay:{page}")),
                    &exec_id,
                    from_seq,
                    JOURNAL_PAGE_LIMIT,
                    JOURNAL_PAGE_MAX_BYTES,
                )
                .await
                .map_err(runtime_to_g5)?;
            let page_last = replay["events"]
                .as_array()
                .and_then(|events| events.last())
                .and_then(|event| event["seq"].as_u64());
            match page_last {
                Some(seq) => {
                    last_seq = Some(seq);
                    from_seq = seq.saturating_add(1);
                }
                None => break,
            }
            if !replay["has_more"].as_bool().unwrap_or(false) {
                break;
            }
        }
        if let Some(last_seq) = last_seq {
            self.modules_events_ack(handle, &housekeeping("ack"), &exec_id, last_seq)
                .await
                .map_err(runtime_to_g5)?;
        }
        if operation == "inspect" {
            if outcome["status"] != "succeeded" || !outcome["result"]["paths"].is_array() {
                return Err(G5Error::Conflict { reason: "Current workspace inspection failed. Check the repository association and restart an older workspace runtime.".into() });
            }
            return Ok(outcome);
        }
        if !outcome["result"]["members"].is_array() {
            return Err(G5Error::Conflict {
                reason: format!(
                    "workspace Changes query failed: {}",
                    outcome["detail"]["reason"]
                        .as_str()
                        .unwrap_or("invalid outcome")
                ),
            });
        }
        let report = json!({
            "workspace_id": handle.workspace_id,
            "source": "live",
            "attached": true,
            "vm_id": handle.vm_id,
            "epoch": handle.epoch,
            "status": outcome["status"],
            "members": outcome["result"]["members"],
        });
        Ok(report)
    }

    /// Best-effort `modules.cancel` for a `workspace.changes` execution this
    /// caller gave up on, with its own short budget.
    async fn cancel_abandoned_changes(
        &self,
        handle: &WorkspaceRuntimeHandle,
        actor_user_id: Uuid,
        operation_id: Uuid,
        exec_id: &str,
    ) {
        let request = RuntimeRequest::owner(
            handle,
            actor_user_id,
            unix_time_ms().saturating_add(CHANGES_CANCEL_BUDGET_MS),
            format!("workspace-changes:{operation_id}:cancel"),
        );
        if let Err(error) = self
            .modules_cancel(handle, &request, exec_id, "caller abandoned the query")
            .await
        {
            tracing::warn!(
                workspace_id = %handle.workspace_id,
                vm_id = %handle.vm_id,
                exec_id,
                error = %error.message,
                "could not cancel an abandoned workspace Changes execution"
            );
        }
    }

    async fn durable_workspace_changes(
        &self,
        scope: &(impl OrgScope + ?Sized),
        org_id: Uuid,
        workspace_id: Uuid,
        actor_user_id: Uuid,
    ) -> Result<Value, G5Error> {
        let rows = sqlx::query(
            "SELECT is_base, branch_name, repo_id, root_path, upstream_branch,
                    default_branch
             FROM authz.lookup_workspace_runtime_durable_members($1, $2)",
        )
        .bind(workspace_id)
        .bind(actor_user_id)
        .fetch_all(&self.pool)
        .await?;
        if rows.is_empty() {
            return Err(G5Error::NotFound {
                resource: format!("workspace {workspace_id}"),
            });
        }
        let selected_refs = rows
            .iter()
            .map(|row| {
                Ok((
                    row.try_get::<Uuid, _>("repo_id")?,
                    row.try_get::<String, _>("branch_name")?,
                ))
            })
            .collect::<Result<Vec<_>, sqlx::Error>>()?;
        let mut heads = crate::services::workspace_reader::read_branch_head_oids(
            &self.repo_storage.scoped(scope),
            org_id,
            &selected_refs,
        )
        .await?;
        let mut members = Vec::with_capacity(rows.len());
        for row in rows {
            let repo_id: Uuid = row.try_get("repo_id")?;
            let shared_branch: String = row.try_get("branch_name")?;
            let current_head = heads.remove(&(repo_id, shared_branch.clone())).flatten();
            members.push(durable_change_member(
                repo_id,
                row.try_get("root_path")?,
                shared_branch,
                row.try_get("upstream_branch")?,
                row.try_get("default_branch")?,
                current_head,
            ));
        }
        Ok(json!({
            "workspace_id": workspace_id,
            "source": "durable",
            "attached": false,
            "flush_id": null,
            "members": members,
        }))
    }

    pub async fn modules_describe(
        &self,
        handle: &WorkspaceRuntimeHandle,
        request: &RuntimeRequest,
        module_id: &str,
        version: Option<&str>,
    ) -> Result<Value, RuntimeRpcError> {
        self.call(
            handle,
            request,
            "modules.describe",
            json!({"module_id":module_id,"module_version":version}),
        )
        .await
    }

    pub async fn modules_invoke(
        &self,
        handle: &WorkspaceRuntimeHandle,
        request: &RuntimeRequest,
        params: Value,
    ) -> Result<Value, RuntimeRpcError> {
        self.call(handle, request, "modules.invoke", params).await
    }

    pub async fn modules_status(
        &self,
        handle: &WorkspaceRuntimeHandle,
        request: &RuntimeRequest,
        exec_id: &str,
    ) -> Result<Value, RuntimeRpcError> {
        self.call(
            handle,
            request,
            "modules.status",
            json!({"exec_id":exec_id}),
        )
        .await
    }

    pub async fn modules_cancel(
        &self,
        handle: &WorkspaceRuntimeHandle,
        request: &RuntimeRequest,
        exec_id: &str,
        reason: &str,
    ) -> Result<Value, RuntimeRpcError> {
        self.call(
            handle,
            request,
            "modules.cancel",
            json!({"exec_id":exec_id,"reason":reason}),
        )
        .await
    }

    pub async fn modules_input_respond(
        &self,
        handle: &WorkspaceRuntimeHandle,
        request: &RuntimeRequest,
        params: Value,
    ) -> Result<Value, RuntimeRpcError> {
        self.call(handle, request, "modules.input.respond", params)
            .await
    }

    pub async fn modules_background_end(
        &self,
        handle: &WorkspaceRuntimeHandle,
        request: &RuntimeRequest,
        params: Value,
    ) -> Result<Value, RuntimeRpcError> {
        self.call(handle, request, "modules.background.end", params)
            .await
    }

    pub async fn modules_input_steer(
        &self,
        handle: &WorkspaceRuntimeHandle,
        request: &RuntimeRequest,
        params: Value,
    ) -> Result<Value, RuntimeRpcError> {
        self.call(handle, request, "modules.input.steer", params)
            .await
    }

    pub async fn modules_events_replay(
        &self,
        handle: &WorkspaceRuntimeHandle,
        request: &RuntimeRequest,
        exec_id: &str,
        after_seq: u64,
    ) -> Result<Value, RuntimeRpcError> {
        self.call(
            handle,
            request,
            "events.replay",
            json!({"exec_id":exec_id,"from_seq":after_seq.saturating_add(1)}),
        )
        .await
    }

    /// One bounded journal page from `from_seq` (`g5.workspace.module-execution`).
    /// The reply carries `events`, and from guests with paging `has_more`
    /// and `next_seq`; an older guest returns everything it retains, which
    /// the caller treats as the last page.
    pub async fn modules_events_page(
        &self,
        handle: &WorkspaceRuntimeHandle,
        request: &RuntimeRequest,
        exec_id: &str,
        from_seq: u64,
        limit: u64,
        max_bytes: u64,
    ) -> Result<Value, RuntimeRpcError> {
        self.call(
            handle,
            request,
            "events.replay",
            json!({
                "exec_id": exec_id,
                "from_seq": from_seq.max(1),
                "limit": limit,
                "max_bytes": max_bytes,
            }),
        )
        .await
    }

    pub async fn modules_events_ack(
        &self,
        handle: &WorkspaceRuntimeHandle,
        request: &RuntimeRequest,
        exec_id: &str,
        through_seq: u64,
    ) -> Result<Value, RuntimeRpcError> {
        self.call(
            handle,
            request,
            "events.ack",
            json!({"exec_id":exec_id,"up_to_seq":through_seq}),
        )
        .await
    }

    pub async fn modules_outcomes_pending(
        &self,
        handle: &WorkspaceRuntimeHandle,
        request: &RuntimeRequest,
    ) -> Result<Value, RuntimeRpcError> {
        self.call(handle, request, "outcomes.pending", json!({}))
            .await
    }
}

/// The VM lease check, made once the caller holds the VM row. PostgreSQL
/// rechecks a row it waited for only if the row changed, and `now()` is when
/// the transaction began, so a lease that ran out while the caller waited
/// would pass a check made in the locking statement. `None` when the row has
/// no lease.
pub(crate) async fn lease_running(
    conn: &mut sqlx::PgConnection,
    workspace_id: Uuid,
) -> Result<Option<bool>, sqlx::Error> {
    Ok(sqlx::query_scalar::<_, Option<bool>>(
        "SELECT lease_expires_at > clock_timestamp() FROM workspace_vms WHERE workspace_id = $1",
    )
    .bind(workspace_id)
    .fetch_optional(conn)
    .await?
    .flatten())
}

/// Activity was authorized when its stream or request was admitted; the
/// write still requires that `owner_user_id` is the workspace's current
/// owner with no pending transfer, so revocation or an ownership change
/// mid-connection stops refreshing the idle clock immediately rather than at
/// the next admission.
#[allow(clippy::too_many_arguments)]
async fn record_activity(
    pool: &PgPool,
    workspace_id: Uuid,
    vm_id: Uuid,
    epoch: u64,
    owner_user_id: Uuid,
    reason: WorkspaceActivityReason,
    correlation_id: &str,
    observed_at: Option<DateTime<Utc>>,
) -> Result<(), sqlx::Error> {
    // None is reserved for input or preview traffic observed by the server,
    // never a browser report. Preserve its age across pool/lock waits without
    // comparing the API machine's wall clock to the database's clock.
    let received_at = std::time::Instant::now();
    // Keep the lifecycle lock order: workspace + membership, then VM. The
    // membership function takes FOR SHARE on the workspace, so calling it
    // after locking the VM would deadlock a concurrent Stop/transfer.
    let mut tx = pool.begin().await?;
    let (authorized, checked_at) = sqlx::query_as::<_, (bool, DateTime<Utc>)>(
        "SELECT authz.lock_workspace_member_activity($1), clock_timestamp()",
    )
    .bind(workspace_id)
    .fetch_one(&mut *tx)
    .await?;
    if !authorized {
        return Ok(());
    }
    let observed_at = activity_observation_time(observed_at, checked_at, received_at.elapsed());
    if observed_at < checked_at - chrono::Duration::seconds(30)
        || observed_at > checked_at + chrono::Duration::seconds(5)
    {
        tracing::info!(
            target: "g5_api::workspace_activity",
            event = "workspace_activity_rejected",
            %workspace_id, %vm_id, epoch, correlation_id,
            activity_reason = reason.as_str(), rejection_reason = "observation_outside_window",
            %observed_at, server_time = %checked_at,
            "activity observation rejected"
        );
        return Ok(());
    }
    let change = sqlx::query_as::<_, (Option<DateTime<Utc>>, DateTime<Utc>)>(
        "WITH previous AS MATERIALIZED (
             SELECT vm.workspace_id, vm.last_activity_at
             FROM workspace_vms vm
             WHERE vm.workspace_id = $1 FOR UPDATE
         )
         UPDATE workspace_vms AS vm
         SET last_activity_at = LEAST($5, clock_timestamp()), updated_at = clock_timestamp()
         FROM previous
         WHERE vm.workspace_id = previous.workspace_id AND vm.vm_id = $2 AND vm.epoch = $3
           AND vm.status = 'running'
           AND vm.lease_expires_at > clock_timestamp()
           AND EXISTS (
               SELECT 1 FROM authz.lookup_workspace_runtime_authority($1) AS workspace
               WHERE workspace.owner_user_id = $4
                 AND workspace.deleted_at IS NULL
                 AND NOT workspace.pending_transfer)
           AND $5 BETWEEN clock_timestamp() - interval '30 seconds'
                      AND clock_timestamp() + interval '5 seconds'
           AND (vm.last_activity_at IS NULL
                OR vm.last_activity_at < LEAST($5, clock_timestamp()) - interval '10 seconds')
         RETURNING previous.last_activity_at, vm.last_activity_at",
    )
    .bind(workspace_id)
    .bind(vm_id.to_string())
    .bind(epoch as i64)
    .bind(owner_user_id)
    .bind(observed_at)
    .fetch_optional(&mut *tx)
    .await?;
    tx.commit().await?;
    if let Some((previous, new)) = change {
        g5_api_foundation::activity::log_activity_change(
            workspace_id,
            vm_id,
            epoch,
            match reason {
                WorkspaceActivityReason::PreviewTraffic => "preview_traffic",
                _ => "user_activity",
            },
            reason.as_str(),
            correlation_id,
            previous,
            Some(new),
        );
    }
    Ok(())
}

fn activity_observation_time(
    browser_time: Option<DateTime<Utc>>,
    database_time: DateTime<Utc>,
    server_elapsed: std::time::Duration,
) -> DateTime<Utc> {
    browser_time.unwrap_or_else(|| {
        // All ages above the acceptance window are equivalent. Bound the
        // conversion, but keep expired input expired instead of re-dating it.
        database_time
            - chrono::Duration::milliseconds(server_elapsed.as_millis().min(31_000) as i64)
    })
}

struct ActivityWriteClaim<'a> {
    last_write_ms: &'a AtomicU64,
    claimed_at: u64,
    completed: bool,
}

impl Drop for ActivityWriteClaim<'_> {
    fn drop(&mut self) {
        if !self.completed {
            // Never clear a newer claim acquired by a concurrent input.
            let _ = self.last_write_ms.compare_exchange(
                self.claimed_at,
                0,
                Ordering::AcqRel,
                Ordering::Acquire,
            );
        }
    }
}

fn claim_activity_write(last_write_ms: &AtomicU64, now_ms: u64) -> bool {
    let mut previous = last_write_ms.load(Ordering::Acquire);
    loop {
        if now_ms.saturating_sub(previous) < 10_000 {
            return false;
        }
        match last_write_ms.compare_exchange_weak(
            previous,
            now_ms,
            Ordering::AcqRel,
            Ordering::Acquire,
        ) {
            Ok(_) => return true,
            Err(observed) => previous = observed,
        }
    }
}

fn live_report_has_association_mismatch(report: &Value) -> bool {
    const ASSOCIATION_ERRORS: &[&str] = &[
        "INVALID_BRANCH_NAME",
        "INVALID_UPSTREAM_BRANCH",
        "DETACHED_HEAD",
        "BRANCH_ASSOCIATION_MISMATCH",
        "MISSING_INTERNAL_BRANCH",
        "INTERRUPTED_SEQUENCER",
    ];
    report["members"]
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|member| member["errors"].as_array().into_iter().flatten())
        .filter_map(Value::as_str)
        .any(|error| ASSOCIATION_ERRORS.contains(&error))
}

/// Pushed metadata only; recovery receipts never authorize current-work browsing.
fn durable_change_member(
    repo_id: Uuid,
    root_path: String,
    shared_branch: String,
    upstream_branch: Option<String>,
    default_branch: String,
    current_head: Option<String>,
) -> Value {
    let errors = if current_head.is_none() {
        vec!["MISSING_INTERNAL_BRANCH"]
    } else {
        vec![]
    };
    json!({
        "repo_id": repo_id,
        "root_path": root_path,
        "head": shared_branch,
        "shared_branch": shared_branch,
        "upstream_branch": upstream_branch,
        "default_branch": default_branch,
        "detached_head": false,
        "foreign_head": false,
        "sequencer": null,
        "on_shared_branch": true,
        "internal_ref_present": current_head.is_some(),
        "internal_ref": current_head,
        "local_head_oid": null,
        "snapshot_oid": null,
        "dirty": null,
        "against_internal": {"ahead":null,"behind":null},
        "against_upstream": {"ahead":null,"behind":null},
        "conflicts": [],
        "errors": errors,
    })
}

pub fn runtime_to_g5(error: RuntimeRpcError) -> G5Error {
    // A refusal from the dispatcher or the admission fence is otherwise
    // indistinguishable in logs from an authorization 403/409 raised by the
    // API itself; name the runtime code so an operator reading the API log
    // can tell which side refused.
    if matches!(error.code, 403 | 409) {
        tracing::warn!(
            target: "g5_audit",
            event = "workspace_runtime.refused",
            runtime_code = error.code,
            runtime_error = %error.message,
            "workspace runtime call refused"
        );
    }
    match error.message.as_str() {
        "WORKSPACE_NOT_RUNNING" | "STALE_INCARNATION" | "LEASE_EXPIRED" => {
            G5Error::WorkspaceNotRunning
        }
        "WORKSPACE_STOPPING" => G5Error::WorkspaceStopping,
        "OWNER_REQUIRED" | "LIFECYCLE_AUTHORITY_REQUIRED" => G5Error::Forbidden {
            missing_scopes: Vec::new(),
        },
        _ if error.code == 409 => G5Error::Conflict {
            reason: error.message,
        },
        _ => G5Error::ServiceUnavailable {
            message: format!("workspace runtime call failed: {}", error.message),
        },
    }
}

/// Operation-specific input of `workspace.changes`: commit message and paths,
/// or whether `update` keeps the base's side where a review copy collides.
#[derive(Clone, Copy, Debug, Default)]
struct ChangesExtras<'a> {
    review: Option<(&'a str, &'a str, Option<&'a str>)>,
    message: Option<&'a str>,
    favor_base: bool,
    /// The caller's deadline, when it is earlier than one fresh budget.
    deadline_ms: Option<u64>,
    paths: Option<&'a [String]>,
    expected_blobs: Option<&'a std::collections::BTreeMap<String, String>>,
    push_head: Option<&'a str>,
    rebase: Option<ChangesRebase<'a>>,
    adopt: Option<ChangesAdopt<'a>>,
}

/// Captured inputs of an in-VM provider-history adoption.
#[derive(Clone, Copy, Debug)]
struct ChangesAdopt<'a> {
    run_id: Uuid,
    expected_head: &'a str,
    provider_tip: &'a str,
}

/// Captured inputs of an in-VM source rebase.
#[derive(Clone, Copy, Debug)]
struct ChangesRebase<'a> {
    expected_head: &'a str,
    onto_branch: &'a str,
    onto_head: &'a str,
}

/// The VM's strict input validator refused `field` as unknown: its catalog
/// predates the field.
fn rejects_unknown_field(error: &RuntimeRpcError, field: &str) -> bool {
    error.code == -32602
        && error.message == "INVALID_INPUT"
        && error
            .data
            .get("reason")
            .and_then(Value::as_str)
            .is_some_and(|reason| reason.starts_with(&format!("unknown field `{field}`")))
}

/// Whether a call that failed with `error` may be resent unchanged: its
/// dispatcher stream closed before any reply, and `method` is not one of the
/// lifecycle gate methods whose stop, fork and transfer callers own retry.
fn resends_closed_stream(method: &str, error: &RuntimeRpcError) -> bool {
    error.code == 503
        && error.message == "HOST_TUNNEL_CLOSED"
        && !matches!(
            method,
            "admission.close" | "admission.open" | "workspace.flush"
        )
}

fn runtime_error(code: i64, message: &str) -> RuntimeRpcError {
    RuntimeRpcError {
        code,
        message: message.to_string(),
        data: Value::Null,
    }
}

fn internal(message: &str) -> G5Error {
    G5Error::Internal {
        message: message.to_string(),
    }
}

/// Identity of one owner's live Changes reads against one VM incarnation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct ChangesReadKey {
    workspace_id: Uuid,
    vm_id: Uuid,
    epoch: u64,
    actor_user_id: Uuid,
}

/// Per-key coalescers, held only while some reader is using them, so no
/// report outlives its concurrent readers.
#[derive(Default)]
struct ChangesReadCoalescers {
    by_key: Mutex<HashMap<ChangesReadKey, Weak<FreshRunCoalescer<Value>>>>,
}

impl ChangesReadCoalescers {
    fn for_key(&self, key: ChangesReadKey) -> Arc<FreshRunCoalescer<Value>> {
        let mut by_key = self.by_key.lock().unwrap_or_else(|p| p.into_inner());
        by_key.retain(|_, weak| weak.strong_count() > 0);
        if let Some(existing) = by_key.get(&key).and_then(Weak::upgrade) {
            return existing;
        }
        let coalescer = Arc::new(FreshRunCoalescer::default());
        by_key.insert(key, Arc::downgrade(&coalescer));
        coalescer
    }
}

/// Runs a read at most once at a time and shares a successful result only
/// with callers that arrived before that run started, so every caller sees
/// state at least as new as its own arrival. Callers arriving during a run
/// wait and share one follow-up run. A failed run is not shared: each waiter
/// then makes its own attempt, still one at a time. Each caller's deadline
/// bounds its wait behind other readers; the read must enforce the same
/// deadline itself, so it can cancel remote work it abandons instead of being
/// dropped mid-flight.
struct FreshRunCoalescer<T> {
    /// Monotonic stamps for arrivals and run starts.
    seq: AtomicU64,
    /// The latest successful result and the stamp of the run that produced it.
    last: tokio::sync::Mutex<Option<(u64, T)>>,
}

impl<T> Default for FreshRunCoalescer<T> {
    fn default() -> Self {
        Self {
            seq: AtomicU64::new(0),
            last: tokio::sync::Mutex::new(None),
        }
    }
}

impl<T: Clone> FreshRunCoalescer<T> {
    async fn run<E, F, Fut>(
        &self,
        deadline: tokio::time::Instant,
        read: F,
        timed_out: impl FnOnce() -> E,
    ) -> Result<T, E>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = Result<T, E>>,
    {
        let arrived = self.seq.fetch_add(1, Ordering::SeqCst) + 1;
        let Ok(mut last) = tokio::time::timeout_at(deadline, self.last.lock()).await else {
            return Err(timed_out());
        };
        if let Some((started, value)) = last.as_ref() {
            if *started > arrived {
                return Ok(value.clone());
            }
        }
        let started = self.seq.fetch_add(1, Ordering::SeqCst) + 1;
        let result = read().await;
        if let Ok(value) = &result {
            *last = Some((started, value.clone()));
        }
        result
    }
}

fn unix_time_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

/// The dispatcher's execution id of one task attempt on one VM.
pub fn canonical_exec_id(task_id: Uuid, attempt: i32, vm_id: Uuid) -> String {
    format!("task.{task_id}.{attempt}.{vm_id}")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The guest dispatcher rejects unknown context fields, so both request
    /// kinds must keep sending exactly these keys.
    #[test]
    fn context_does_not_mix_actor_and_lifecycle_authority() {
        let (workspace_id, vm_id, owner) = (Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4());
        assert_eq!(
            RuntimeRequest::lifecycle(workspace_id, vm_id, 4, 1_000, "stop:one").context(),
            json!({
                "workspace_id": workspace_id,
                "expected_vm_id": vm_id,
                "expected_epoch": 4,
                "actor_user_id": null,
                "lifecycle": true,
                "inspection_grant_id": null,
                "deadline_ms": 1_000,
                "idempotency_key": "stop:one",
            })
        );
        let handle = WorkspaceRuntimeHandle {
            org_id: Uuid::new_v4(),
            workspace_id,
            owner_user_id: owner,
            host_id: Uuid::new_v4(),
            vm_id,
            epoch: 4,
            status: "running".to_string(),
            pending_transfer: false,
        };
        assert_eq!(
            RuntimeRequest::owner(&handle, owner, 2_000, "edit:one").context(),
            json!({
                "workspace_id": workspace_id,
                "expected_vm_id": vm_id,
                "expected_epoch": 4,
                "actor_user_id": owner,
                "lifecycle": null,
                "inspection_grant_id": null,
                "deadline_ms": 2_000,
                "idempotency_key": "edit:one",
            })
        );
    }

    #[test]
    fn stream_activity_claims_at_most_one_database_write_per_window() {
        let last = AtomicU64::new(1_000);
        assert!(!claim_activity_write(&last, 1_001));
        assert!(!claim_activity_write(&last, 10_999));
        assert!(claim_activity_write(&last, 11_000));
        assert!(!claim_activity_write(&last, 20_999));
        assert!(claim_activity_write(&last, 21_000));
    }

    #[test]
    fn server_activity_uses_database_time_and_keeps_waits_in_its_age() {
        // Deliberately unrelated to this API process's current wall clock.
        let database_time = "2020-01-01T00:00:00Z".parse::<DateTime<Utc>>().unwrap();
        assert_eq!(
            activity_observation_time(None, database_time, std::time::Duration::from_secs(2)),
            database_time - chrono::Duration::seconds(2),
        );
        assert!(
            activity_observation_time(None, database_time, std::time::Duration::MAX)
                < database_time - chrono::Duration::seconds(30)
        );
        let original = database_time - chrono::Duration::seconds(10);
        assert_eq!(
            activity_observation_time(
                Some(original),
                database_time,
                std::time::Duration::from_secs(2)
            ),
            original,
            "browser reports retain their captured database-calibrated observation",
        );
    }

    #[test]
    fn failed_or_cancelled_activity_claim_releases_only_its_own_window() {
        let last = AtomicU64::new(11_000);
        drop(ActivityWriteClaim {
            last_write_ms: &last,
            claimed_at: 11_000,
            completed: false,
        });
        assert!(
            claim_activity_write(&last, 11_001),
            "next actual input may retry immediately"
        );
        drop(ActivityWriteClaim {
            last_write_ms: &last,
            claimed_at: 11_000,
            completed: false,
        });
        assert_eq!(last.load(Ordering::Acquire), 11_001, "newer claim survives");
        drop(ActivityWriteClaim {
            last_write_ms: &last,
            claimed_at: 11_001,
            completed: true,
        });
        assert!(
            !claim_activity_write(&last, 11_002),
            "success retains coalescing"
        );
    }

    impl<T: Clone> FreshRunCoalescer<T> {
        /// `run` with a deadline no test reaches.
        async fn run_test<E, F, Fut>(&self, read: F) -> Result<T, E>
        where
            F: FnOnce() -> Fut,
            Fut: Future<Output = Result<T, E>>,
        {
            let far = tokio::time::Instant::now() + std::time::Duration::from_secs(3600);
            self.run(far, read, || panic!("unexpected coalescer timeout"))
                .await
        }
    }

    #[tokio::test]
    async fn a_reader_queued_behind_a_stalled_run_gives_up_waiting_at_its_deadline() {
        let coalescer = Arc::new(FreshRunCoalescer::<u32>::default());
        let (release_tx, release_rx) = tokio::sync::oneshot::channel::<()>();
        let leader = {
            let coalescer = coalescer.clone();
            tokio::spawn(async move {
                coalescer
                    .run_test(|| async move {
                        release_rx.await.unwrap();
                        Ok::<_, &str>(1)
                    })
                    .await
            })
        };
        stamped(&coalescer, 2).await;

        let deadline = tokio::time::Instant::now() + std::time::Duration::from_millis(50);
        let follower = coalescer
            .run(deadline, || async { Ok::<_, &str>(2) }, || "timed out")
            .await;
        assert_eq!(follower, Err("timed out"));
        assert!(tokio::time::Instant::now() >= deadline);

        // The running read is not cut off: it enforces its own deadline so it
        // can cancel remote work, and here it completes after release.
        release_tx.send(()).unwrap();
        assert_eq!(leader.await.unwrap(), Ok(1));
    }

    /// Wait until `count` arrivals/starts have been stamped, so spawned readers
    /// are known to be queued behind the in-flight run.
    async fn stamped(coalescer: &FreshRunCoalescer<u32>, count: u64) {
        while coalescer.seq.load(Ordering::SeqCst) < count {
            tokio::task::yield_now().await;
        }
    }

    #[tokio::test]
    async fn readers_arriving_mid_run_share_one_follow_up_run() {
        let coalescer = Arc::new(FreshRunCoalescer::<u32>::default());
        let runs = Arc::new(AtomicU64::new(0));
        let (release_tx, release_rx) = tokio::sync::oneshot::channel::<()>();

        let leader = {
            let (coalescer, runs) = (coalescer.clone(), runs.clone());
            tokio::spawn(async move {
                coalescer
                    .run_test(|| async move {
                        let run = runs.fetch_add(1, Ordering::SeqCst) + 1;
                        release_rx.await.unwrap();
                        Ok::<_, ()>(run as u32)
                    })
                    .await
            })
        };
        // Leader arrived and started.
        stamped(&coalescer, 2).await;

        let followers: Vec<_> = (0..5)
            .map(|_| {
                let (coalescer, runs) = (coalescer.clone(), runs.clone());
                tokio::spawn(async move {
                    coalescer
                        .run_test(|| async move {
                            Ok::<_, ()>((runs.fetch_add(1, Ordering::SeqCst) + 1) as u32)
                        })
                        .await
                })
            })
            .collect();
        stamped(&coalescer, 7).await;
        release_tx.send(()).unwrap();

        assert_eq!(leader.await.unwrap(), Ok(1));
        for follower in followers {
            // A mid-run arrival never receives the run that started before it.
            assert_eq!(follower.await.unwrap(), Ok(2));
        }
        assert_eq!(runs.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn a_read_after_completion_runs_again() {
        let coalescer = FreshRunCoalescer::<u32>::default();
        let runs = AtomicU64::new(0);
        let read = || async { Ok::<_, ()>((runs.fetch_add(1, Ordering::SeqCst) + 1) as u32) };
        assert_eq!(coalescer.run_test(read).await, Ok(1));
        assert_eq!(coalescer.run_test(read).await, Ok(2));
    }

    #[tokio::test]
    async fn a_failed_run_is_not_shared() {
        let coalescer = Arc::new(FreshRunCoalescer::<u32>::default());
        let attempts = Arc::new(AtomicU64::new(0));
        let (release_tx, release_rx) = tokio::sync::oneshot::channel::<()>();

        let leader = {
            let coalescer = coalescer.clone();
            tokio::spawn(async move {
                coalescer
                    .run_test(|| async move {
                        release_rx.await.unwrap();
                        Err::<u32, &str>("down")
                    })
                    .await
            })
        };
        stamped(&coalescer, 2).await;
        let followers: Vec<_> = (0..3)
            .map(|_| {
                let (coalescer, attempts) = (coalescer.clone(), attempts.clone());
                tokio::spawn(async move {
                    coalescer
                        .run_test(|| async move {
                            attempts.fetch_add(1, Ordering::SeqCst);
                            Ok::<u32, &str>(7)
                        })
                        .await
                })
            })
            .collect();
        stamped(&coalescer, 5).await;
        release_tx.send(()).unwrap();

        assert_eq!(leader.await.unwrap(), Err("down"));
        for follower in followers {
            assert_eq!(follower.await.unwrap(), Ok(7));
        }
        // The first waiter re-ran after the failure; the rest shared its success.
        assert_eq!(attempts.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn changes_coalescers_live_only_while_held() {
        let registry = ChangesReadCoalescers::default();
        let key = ChangesReadKey {
            workspace_id: Uuid::new_v4(),
            vm_id: Uuid::new_v4(),
            epoch: 3,
            actor_user_id: Uuid::new_v4(),
        };
        let first = registry.for_key(key);
        assert!(Arc::ptr_eq(&first, &registry.for_key(key)));
        let other = registry.for_key(ChangesReadKey { epoch: 4, ..key });
        assert!(!Arc::ptr_eq(&first, &other));
        drop(first);
        drop(other);
        let fresh = registry.for_key(key);
        assert_eq!(registry.by_key.lock().unwrap().len(), 1);
        assert_eq!(fresh.seq.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn live_changes_rejects_only_branch_association_errors() {
        assert!(!live_report_has_association_mismatch(&json!({
            "members": [{"errors":["STATUS_UNAVAILABLE"]}]
        })));
        assert!(live_report_has_association_mismatch(&json!({
            "members": [{"errors":["BRANCH_ASSOCIATION_MISMATCH"]}]
        })));
    }

    #[test]
    fn durable_changes_exposes_only_published_metadata() {
        let member = durable_change_member(
            Uuid::new_v4(),
            "".to_string(),
            "workspace/demo".to_string(),
            Some("main".to_string()),
            "main".to_string(),
            Some("a".repeat(40)),
        );
        assert_eq!(member["internal_ref"], "a".repeat(40));
        assert_eq!(member["on_shared_branch"], true);
        for field in ["dirty", "local_head_oid", "snapshot_oid", "sequencer"] {
            assert!(
                member[field].is_null(),
                "{field} is private current-work metadata"
            );
        }
        assert!(member["against_internal"]["ahead"].is_null());
    }
}
