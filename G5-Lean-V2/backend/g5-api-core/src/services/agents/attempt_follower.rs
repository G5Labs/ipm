//! Attempt followers: live delivery from the guest journal and five-second
//! history batches (`g5.chat.live-delivery`, `g5.chat.history-batches`,
//! `g5.chat.live-tail-recovery`).
//!
//! The reconciler claims a live attempt and hands it to one follower task in
//! this process. The follower fetches journal pages (woken by the host's
//! module-event hint, otherwise by a short poll), binds every event to the
//! claimed task, attempt and VM incarnation, publishes its timeline row at
//! once, keeps the unpersisted tail in a bounded buffer that also serves the
//! tail read, and stores history in one transaction per batch. Only a
//! committed prefix is acknowledged to the guest, cumulatively. The database
//! stays the authority for task and control transitions; the journal stays
//! the durable recovery source until the batch commits.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use dashmap::DashMap;
use serde_json::json;
use tokio::sync::{Notify, Semaphore};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::errors::AppError;
use crate::maintenance::{scoped_db_for_maintenance_org, TenantMaintenanceDeps};
use crate::services::agents::segment_map::SegmentMap;
use crate::services::task_lifecycle::{
    canonical_exec_id, is_routine_stream_event, journal_event_from_replay, journal_timeline_entry,
    seed_segment_map, BatchApplyError, JournalBatchEvent, JournalTaskEvent, TaskLifecycleEvent,
};
use crate::services::workspace::workspace_runtime::{RuntimeRequest, WorkspaceRuntimeHandle};
use crate::services::AppState;
use crate::ws::events::TimelineEntryPayload;
use crate::ws::{Channel, WsEvent};

/// Routine output waits this long before its batch is stored; the owner's
/// explicit choice over a sub-second timer (`g5.chat.history-batches`).
pub const FLUSH_INTERVAL: Duration = Duration::from_secs(5);
/// Capacity safeguards that close a batch early, not a replacement cadence.
pub const BATCH_MAX_EVENTS: usize = 256;
pub const BATCH_MAX_BYTES: usize = 512 * 1024;
/// The unpersisted tail one follower retains; past it the buffer is dropped
/// and rebuilt from the journal rather than blocking the provider.
pub const BUFFER_MAX_EVENTS: usize = 4096;
pub const BUFFER_MAX_BYTES: usize = 8 * 1024 * 1024;
pub use crate::services::workspace::workspace_runtime::{
    JOURNAL_PAGE_LIMIT as PAGE_LIMIT, JOURNAL_PAGE_MAX_BYTES as PAGE_MAX_BYTES,
};

const ACTIVE_POLL: Duration = Duration::from_secs(1);
const IDLE_POLL: Duration = Duration::from_secs(2);
const STATUS_PROBE_INTERVAL: Duration = Duration::from_secs(10);
const CANCEL_REDELIVERY_INTERVAL: Duration = Duration::from_secs(2);
/// A committed cancel or pause is re-read on this cadence even without a
/// poke, so a lost poke or a failed read cannot strand it.
const CONTROL_RECONCILE_INTERVAL: Duration = Duration::from_secs(10);
const RPC_BUDGET: Duration = Duration::from_secs(15);
const MAX_BACKOFF: Duration = Duration::from_secs(10);
/// How long a history batch waits for its locks (the session's turn
/// allocation, the VM row, the task and attempt rows). A batch is background
/// work that is retried from the buffer, so it yields to a Stop or a send
/// rather than holding the session behind a stalled write
/// (`g5.chat.history-batches`).
const BATCH_LOCK_WAIT: Duration = Duration::from_secs(2);
/// Guest fetches in flight per process at most, so a burst of attempts
/// cannot monopolize the host tunnel while others wait; the effective limit
/// also leaves the pool room, see [`fetch_concurrency`].
const FETCH_CONCURRENCY: usize = 16;

/// Fetches in flight per process: at most [`FETCH_CONCURRENCY`], and at most
/// half the runtime pool (at least one), since every fetch pins a pooled
/// connection for its round trip. With the side calls' quarter, a host whose
/// guests stop answering pins at most three quarters of the pool, so the
/// default pool of 20 keeps connections free for requests, not only the 54
/// the deployed environments configure.
#[must_use]
pub fn fetch_concurrency(pool_max_connections: u32) -> usize {
    ((pool_max_connections / 2).max(1) as usize).min(FETCH_CONCURRENCY)
}

/// Guest calls beside the fetch — acknowledgements, status probes and
/// control redeliveries — in flight per process. Every guest call holds a
/// pooled connection as its incarnation fence for the whole round trip, so
/// these calls, none of which carries output, may hold at most a quarter of
/// the runtime pool (and at least one connection). A host whose guests
/// accept calls but stop answering then pins at most this many connections
/// plus the fetch permits, never the pool, and a stalled call delays only
/// bookkeeping, never a batch, the reconciler tick or another workspace's
/// dispatch (`g5.chat.journal`, `g5.chat.controls`).
#[must_use]
pub fn side_call_concurrency(pool_max_connections: u32) -> usize {
    (pool_max_connections / 4).max(1) as usize
}

/// A status probe that failed or timed out is repeated at a growing interval
/// up to this one, so a guest that accepts probes without answering is not
/// asked again the moment its previous answer gave up.
const MAX_STATUS_PROBE_INTERVAL: Duration = Duration::from_secs(60);

/// A live attempt as the reconciler claimed it.
#[derive(Debug, Clone)]
pub struct FollowedAttempt {
    pub org_id: Uuid,
    pub workspace_id: Uuid,
    pub task_id: Uuid,
    pub session_id: Option<Uuid>,
    pub attempt: i32,
    pub vm_id: Uuid,
    pub epoch: u64,
    /// The persisted contiguous prefix when the follower started.
    pub persisted_through: u64,
    pub start_confirmed: bool,
}

impl FollowedAttempt {
    #[must_use]
    pub fn exec_id(&self) -> String {
        canonical_exec_id(self.task_id, self.attempt, self.vm_id)
    }
}

/// Counters a follower keeps, read by tests and logged with each batch.
#[derive(Default, Debug)]
pub struct FollowerStats {
    pub flushes: AtomicU64,
    pub flushed_events: AtomicU64,
    pub acks: AtomicU64,
    pub ack_failures: AtomicU64,
    pub published: AtomicU64,
    pub fetches: AtomicU64,
    pub resyncs: AtomicU64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FollowerStatsSnapshot {
    pub flushes: u64,
    pub flushed_events: u64,
    pub acks: u64,
    pub ack_failures: u64,
    pub published: u64,
    pub fetches: u64,
    pub resyncs: u64,
    pub persisted_through: u64,
    pub fetched_through: u64,
    pub buffered: usize,
}

struct Buffered {
    event: JournalTaskEvent,
    received_at: DateTime<Utc>,
    message_id: Uuid,
    bytes: usize,
}

/// The follower's unpersisted tail, as the tail read sees it.
struct TailState {
    persisted_through: u64,
    fetched_through: u64,
    /// The highest sequence published live; a refetch after an overflow
    /// buffers older events again without publishing them twice.
    published_through: u64,
    /// The buffer holds every event from `persisted_through + 1` through
    /// `fetched_through`. False while it is being rebuilt after an overflow.
    complete: bool,
    entries: VecDeque<Buffered>,
    bytes: usize,
    first_unflushed_at: Option<Instant>,
    /// The previous batch closed on capacity or a backlog and left events
    /// behind: they were already waiting, so they drain without the interval.
    drain: bool,
}

/// What the tail buffer did with one bound event.
enum Admission {
    Buffered,
    /// This event filled the buffer: the tail is dropped and rebuilt from
    /// the journal once the stored prefix is gone.
    Overflowed,
    /// The buffer is already waiting for its rebuild.
    Rebuilding,
}

impl TailState {
    /// Buffer a bound event, or leave it for the rebuild when the buffer is
    /// full or already rebuilding.
    fn accept(&mut self, buffered: Buffered) -> Admission {
        if !self.complete {
            return Admission::Rebuilding;
        }
        if self.entries.len() >= BUFFER_MAX_EVENTS || self.bytes + buffered.bytes > BUFFER_MAX_BYTES
        {
            self.complete = false;
            return Admission::Overflowed;
        }
        if self.entries.is_empty() {
            self.first_unflushed_at = Some(Instant::now());
        }
        self.bytes += buffered.bytes;
        self.entries.push_back(buffered);
        Admission::Buffered
    }

    /// Forget what a committed batch stored.
    fn drop_persisted(&mut self, through: u64, reason: FlushReason) {
        self.persisted_through = self.persisted_through.max(through);
        while self.entries.front().is_some_and(|b| b.event.seq <= through) {
            if let Some(b) = self.entries.pop_front() {
                self.bytes = self.bytes.saturating_sub(b.bytes);
            }
        }
        if self.entries.is_empty() {
            self.first_unflushed_at = None;
            self.bytes = 0;
            self.drain = false;
        } else {
            self.first_unflushed_at = Some(Instant::now());
            self.drain = matches!(reason, "capacity" | "backlog");
        }
    }

    /// After an overflow, once the stored prefix is gone: refetch from the
    /// persisted prefix. True when a rebuild starts.
    fn begin_rebuild(&mut self) -> bool {
        if self.complete || !self.entries.is_empty() {
            return false;
        }
        self.fetched_through = self.persisted_through;
        self.complete = true;
        self.first_unflushed_at = None;
        true
    }

    /// A refused batch: start again from what the database holds.
    fn resync(&mut self, persisted: u64) {
        self.persisted_through = persisted;
        self.fetched_through = persisted;
        self.entries.clear();
        self.bytes = 0;
        self.complete = true;
        self.first_unflushed_at = None;
        self.drain = false;
    }
}

/// What a tail read returns for a live attempt.
#[derive(Debug, Clone)]
pub struct TailSnapshot {
    pub task_id: Uuid,
    pub attempt: i32,
    pub persisted_through: u64,
    pub fetched_through: u64,
    pub complete: bool,
    pub entries: Vec<TimelineEntryPayload>,
}

struct Follower {
    attempt: FollowedAttempt,
    exec_id: String,
    wake: Arc<Notify>,
    control_changed: AtomicBool,
    tail: Mutex<TailState>,
    stats: FollowerStats,
    finished: AtomicBool,
    stop: CancellationToken,
    /// The highest committed prefix still to acknowledge (0: none) and
    /// whether an acknowledgement task is running: one per follower, the
    /// latest prefix winning, since acknowledgements are cumulative.
    ack_pending: AtomicU64,
    ack_running: AtomicBool,
    /// Test-only fault injection: the next N control-intent reads fail.
    #[cfg(feature = "test-helpers")]
    inject_control_read_failures: std::sync::atomic::AtomicU32,
}

/// This process's followers, keyed by canonical execution id.
pub struct FollowerRegistry {
    followers: DashMap<String, Arc<Follower>>,
    fetch_permits: Arc<Semaphore>,
    fetch_limit: usize,
    /// Shared by followers and the retry scan; see [`side_call_concurrency`].
    side_call_permits: Arc<Semaphore>,
    side_call_limit: usize,
    shutdown: CancellationToken,
    /// One acknowledgement retry scan runs at a time (`agent_tasks`).
    ack_retry_in_flight: AtomicBool,
}

impl Default for FollowerRegistry {
    fn default() -> Self {
        Self::for_pool(16)
    }
}

impl FollowerRegistry {
    /// A registry whose guest calls are bounded for a runtime pool of
    /// `pool_max_connections`: fetches by [`fetch_concurrency`], side calls
    /// (acknowledgements, probes, redeliveries) by [`side_call_concurrency`].
    #[must_use]
    pub fn for_pool(pool_max_connections: u32) -> Self {
        Self::with_limits(
            fetch_concurrency(pool_max_connections),
            side_call_concurrency(pool_max_connections),
        )
    }

    /// A registry with explicit bounds on fetches and side calls in flight.
    #[must_use]
    pub fn with_limits(fetches: usize, calls: usize) -> Self {
        Self {
            followers: DashMap::new(),
            fetch_permits: Arc::new(Semaphore::new(fetches.max(1))),
            fetch_limit: fetches.max(1),
            side_call_permits: Arc::new(Semaphore::new(calls.max(1))),
            side_call_limit: calls.max(1),
            shutdown: CancellationToken::new(),
            ack_retry_in_flight: AtomicBool::new(false),
        }
    }

    /// How many fetches may run at once in this process.
    #[must_use]
    pub fn fetch_limit(&self) -> usize {
        self.fetch_limit
    }

    /// The process-wide permits for guest calls beside the fetch (the retry
    /// scan takes one per acknowledgement too).
    #[must_use]
    pub fn side_call_permits(&self) -> Arc<Semaphore> {
        self.side_call_permits.clone()
    }

    /// How many side calls may run at once in this process.
    #[must_use]
    pub fn side_call_limit(&self) -> usize {
        self.side_call_limit
    }

    /// Follow `attempt` in this process unless it already is. Returns whether
    /// a follower was started.
    pub fn ensure(&self, state: &Arc<AppState>, attempt: FollowedAttempt) -> bool {
        let exec_id = attempt.exec_id();
        let entry = match self.followers.entry(exec_id.clone()) {
            dashmap::mapref::entry::Entry::Occupied(_) => return false,
            dashmap::mapref::entry::Entry::Vacant(entry) => entry,
        };
        let wake = state.live_hints.subscribe(&exec_id);
        let follower = Arc::new(Follower {
            exec_id: exec_id.clone(),
            wake,
            control_changed: AtomicBool::new(true),
            tail: Mutex::new(TailState {
                persisted_through: attempt.persisted_through,
                fetched_through: attempt.persisted_through,
                published_through: attempt.persisted_through,
                complete: true,
                entries: VecDeque::new(),
                bytes: 0,
                first_unflushed_at: None,
                drain: false,
            }),
            stats: FollowerStats::default(),
            finished: AtomicBool::new(false),
            stop: self.shutdown.child_token(),
            ack_pending: AtomicU64::new(0),
            ack_running: AtomicBool::new(false),
            #[cfg(feature = "test-helpers")]
            inject_control_read_failures: std::sync::atomic::AtomicU32::new(0),
            attempt,
        });
        entry.insert(follower.clone());
        let state = state.clone();
        let permits = self.fetch_permits.clone();
        let shutdown = follower.stop.clone();
        tokio::spawn(async move {
            run(state.clone(), follower.clone(), permits, shutdown).await;
            follower.finished.store(true, Ordering::Release);
            state.followers.followers.remove(&follower.exec_id);
            state.live_hints.release(&follower.exec_id);
        });
        true
    }

    /// Tasks whose attempt this process follows; the reconciler's claim
    /// leaves them out so its batch goes to attempts nobody follows.
    #[must_use]
    pub fn followed_task_ids(&self) -> Vec<Uuid> {
        self.followers
            .iter()
            .map(|entry| entry.value().attempt.task_id)
            .collect()
    }

    /// The `(task, attempt)` pairs this process follows. A task's later
    /// attempt (a resumed native session) is a different execution and must
    /// be claimed even while the earlier attempt's follower is ending.
    #[must_use]
    pub fn followed_attempts(&self) -> Vec<(Uuid, i32)> {
        self.followers
            .iter()
            .map(|entry| (entry.value().attempt.task_id, entry.value().attempt.attempt))
            .collect()
    }

    #[must_use]
    pub fn is_following(&self, exec_id: &str) -> bool {
        self.followers.contains_key(exec_id)
    }

    /// A control fact changed (cancel or pause requested, a delivery
    /// failed): the follower re-reads the task and redelivers.
    pub fn poke(&self, exec_id: &str) {
        if let Some(follower) = self.followers.get(exec_id) {
            follower.control_changed.store(true, Ordering::Release);
            follower.wake.notify_one();
        }
    }

    /// The unpersisted tail of the attempt, with the persisted watermark.
    #[must_use]
    pub fn tail(&self, task_id: Uuid, attempt: i32, vm_id: Uuid) -> Option<TailSnapshot> {
        let exec_id = canonical_exec_id(task_id, attempt, vm_id);
        let follower = self.followers.get(&exec_id)?;
        let tail = follower.tail.lock().ok()?;
        Some(TailSnapshot {
            task_id,
            attempt,
            persisted_through: tail.persisted_through,
            fetched_through: tail.fetched_through,
            complete: tail.complete,
            entries: tail
                .entries
                .iter()
                .map(|buffered| {
                    journal_timeline_entry(
                        &buffered.event,
                        buffered.message_id,
                        follower.attempt.session_id,
                        buffered.received_at,
                        true,
                    )
                })
                .collect(),
        })
    }

    #[must_use]
    pub fn stats(&self, exec_id: &str) -> Option<FollowerStatsSnapshot> {
        let follower = self.followers.get(exec_id)?;
        let tail = follower.tail.lock().ok()?;
        let stats = &follower.stats;
        Some(FollowerStatsSnapshot {
            flushes: stats.flushes.load(Ordering::Relaxed),
            flushed_events: stats.flushed_events.load(Ordering::Relaxed),
            acks: stats.acks.load(Ordering::Relaxed),
            ack_failures: stats.ack_failures.load(Ordering::Relaxed),
            published: stats.published.load(Ordering::Relaxed),
            fetches: stats.fetches.load(Ordering::Relaxed),
            resyncs: stats.resyncs.load(Ordering::Relaxed),
            persisted_through: tail.persisted_through,
            fetched_through: tail.fetched_through,
            buffered: tail.entries.len(),
        })
    }

    /// Make the follower's next `count` control-intent reads fail, as a
    /// transient database error would (tests only).
    #[cfg(feature = "test-helpers")]
    pub fn fail_control_reads(&self, exec_id: &str, count: u32) {
        if let Some(follower) = self.followers.get(exec_id) {
            follower
                .inject_control_read_failures
                .store(count, Ordering::Release);
        }
    }

    /// Claim the one acknowledgement retry scan slot; false while a scan
    /// is still running.
    pub fn begin_ack_retry(&self) -> bool {
        self.ack_retry_in_flight
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
    }

    pub fn end_ack_retry(&self) {
        self.ack_retry_in_flight.store(false, Ordering::Release);
    }

    /// Whether an acknowledgement retry scan is running.
    #[must_use]
    pub fn ack_retry_running(&self) -> bool {
        self.ack_retry_in_flight.load(Ordering::Acquire)
    }

    /// Stop every follower; used at process shutdown.
    pub fn shutdown(&self) {
        self.shutdown.cancel();
    }

    /// End the follower of an execution another consumer takes over (the VM
    /// stop drain), waiting briefly for a batch in flight to finish so the
    /// two never store concurrently.
    pub async fn retire(&self, exec_id: &str) {
        let Some(follower) = self
            .followers
            .get(exec_id)
            .map(|entry| entry.value().clone())
        else {
            return;
        };
        follower.stop.cancel();
        follower.wake.notify_one();
        let deadline = Instant::now() + Duration::from_secs(5);
        while !follower.finished.load(Ordering::Acquire) && Instant::now() < deadline {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }

    /// Wait until no follower of `exec_id` runs in this process (it settled
    /// the attempt, lost its incarnation or was retired). Tests and drains
    /// use it; returns whether that happened within `timeout`.
    pub async fn wait_finished(&self, exec_id: &str, timeout: Duration) -> bool {
        let deadline = Instant::now() + timeout;
        while self.followers.contains_key(exec_id) {
            if Instant::now() >= deadline {
                return false;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        true
    }

    /// Wait until the follower persisted at least `seq` (or finished).
    pub async fn wait_persisted(&self, exec_id: &str, seq: u64, timeout: Duration) -> bool {
        let deadline = Instant::now() + timeout;
        loop {
            match self.followers.get(exec_id) {
                None => return true,
                Some(follower) => {
                    let persisted = follower
                        .tail
                        .lock()
                        .map(|tail| tail.persisted_through)
                        .unwrap_or(0);
                    if persisted >= seq {
                        return true;
                    }
                }
            }
            if Instant::now() >= deadline {
                return false;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }

    /// Wake a follower (tests and the reconciler use this instead of waiting
    /// for its poll).
    pub fn wake(&self, exec_id: &str) {
        if let Some(follower) = self.followers.get(exec_id) {
            follower.wake.notify_one();
        }
    }
}

struct FlushOutcome {
    ack_through: Option<u64>,
    terminal: bool,
    stored: u64,
    reason: FlushReason,
}

enum FlushError {
    /// The batch was refused as inconsistent with the persisted prefix
    /// (gap, watermark moved, duplicate row): rebuild from the journal.
    Resync(String),
    /// A transient failure: keep the buffer and retry.
    Retry(String),
}

fn deps(state: &AppState) -> TenantMaintenanceDeps {
    TenantMaintenanceDeps {
        system_pool: state.system_pool.clone(),
        tenant_pool: state.tenant_pool.clone(),
        authz: state.authz.clone(),
        repo_storage: state.repo_storage.clone(),
    }
}

fn lifecycle_request(attempt: &FollowedAttempt, key: String) -> RuntimeRequest {
    RuntimeRequest::lifecycle(
        attempt.workspace_id,
        attempt.vm_id,
        attempt.epoch,
        unix_time_ms().saturating_add(RPC_BUDGET.as_millis() as u64),
        key,
    )
}

fn unix_time_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

/// The next receipt time: now, or one microsecond after the previous one
/// when the clock has not moved, so receipt times stay strictly increasing.
fn next_receipt(last: &mut DateTime<Utc>) -> DateTime<Utc> {
    let now = Utc::now();
    let next = if now > *last {
        now
    } else {
        *last + chrono::Duration::microseconds(1)
    };
    *last = next;
    next
}

fn event_bytes(event: &JournalTaskEvent) -> usize {
    event.kind.len()
        + serde_json::to_vec(&event.payload)
            .map(|bytes| bytes.len())
            .unwrap_or(0)
}

/// Why the next batch should be stored now, if it should.
fn flush_reason(tail: &TailState, backlog: bool, now: Instant) -> Option<&'static str> {
    if tail.entries.is_empty() {
        return None;
    }
    if tail
        .entries
        .iter()
        .any(|buffered| !is_routine_stream_event(&buffered.event.kind))
    {
        return Some("boundary");
    }
    if tail.entries.len() >= BATCH_MAX_EVENTS || tail.bytes >= BATCH_MAX_BYTES {
        return Some("capacity");
    }
    if backlog || tail.drain {
        return Some("backlog");
    }
    match tail.first_unflushed_at {
        Some(first) if now.duration_since(first) >= FLUSH_INTERVAL => Some("interval"),
        _ => None,
    }
}

/// The next batch: the buffer's prefix up to capacity, ending at the first
/// terminal event.
fn next_batch(tail: &TailState) -> Vec<JournalBatchEvent> {
    let mut batch = Vec::new();
    let mut bytes = 0usize;
    for buffered in &tail.entries {
        if batch.len() >= BATCH_MAX_EVENTS || (bytes >= BATCH_MAX_BYTES && !batch.is_empty()) {
            break;
        }
        bytes += buffered.bytes;
        let terminal = buffered.event.terminal;
        batch.push(JournalBatchEvent {
            event: buffered.event.clone(),
            received_at: buffered.received_at,
            message_id: Some(buffered.message_id),
        });
        if terminal {
            break;
        }
    }
    batch
}

/// Why the follower stores a batch now (`g5.chat.history-batches`).
type FlushReason = &'static str;

/// Cancel or pause intent the owner committed, redelivered to the guest
/// until the attempt settles (`g5.chat.controls`). The route delivers it
/// first; the follower reconciles the durable intent on every poke, on a
/// fixed cadence, and again after a read that failed, so a transient
/// database error cannot strand a committed Stop.
struct ControlDelivery {
    reason: Option<String>,
    read_pending: bool,
    last_read: Option<Instant>,
    last_delivery: Option<Instant>,
    /// The redelivery in flight: one at a time, on its own task, so a slow
    /// guest answer never holds the live reader.
    in_flight: Option<tokio::task::JoinHandle<()>>,
}

impl ControlDelivery {
    fn new() -> Self {
        Self {
            reason: None,
            read_pending: true,
            last_read: None,
            last_delivery: None,
            in_flight: None,
        }
    }

    fn abort(&mut self) {
        if let Some(task) = self.in_flight.take() {
            task.abort();
        }
    }

    async fn reconcile(&mut self, follower: &Follower, deps: &TenantMaintenanceDeps) {
        if follower.control_changed.swap(false, Ordering::AcqRel) {
            self.read_pending = true;
        }
        let due = self
            .last_read
            .map(|at| at.elapsed() >= CONTROL_RECONCILE_INTERVAL)
            .unwrap_or(true);
        if !self.read_pending && !due {
            return;
        }
        match read_cancel_reason(follower, deps).await {
            Ok(reason) => {
                self.reason = reason;
                self.read_pending = false;
                self.last_read = Some(Instant::now());
            }
            Err(error) => {
                // Keep the read pending: the next wake, poll or cadence
                // repeats it, so a committed intent is not lost to one
                // transient failure.
                self.read_pending = true;
                tracing::warn!(
                    target: "g5_api::live_relay",
                    task_id = %follower.attempt.task_id,
                    attempt = follower.attempt.attempt,
                    %error,
                    "control intent read failed; it stays pending"
                );
            }
        }
    }

    fn deliver(
        &mut self,
        state: &Arc<AppState>,
        follower: &Arc<Follower>,
        handle: &WorkspaceRuntimeHandle,
    ) {
        let Some(reason) = self.reason.clone() else {
            return;
        };
        if self
            .in_flight
            .as_ref()
            .is_some_and(|task| !task.is_finished())
        {
            return;
        }
        let due = self
            .last_delivery
            .map(|at| at.elapsed() >= CANCEL_REDELIVERY_INTERVAL)
            .unwrap_or(true);
        if !due {
            return;
        }
        self.last_delivery = Some(Instant::now());
        let state = state.clone();
        let follower = follower.clone();
        let handle = handle.clone();
        self.in_flight = Some(tokio::spawn(async move {
            let attempt = &follower.attempt;
            let exec_id = &follower.exec_id;
            // One of the bounded side-call permits: the call pins a pooled
            // connection until the guest answers.
            let permits = state.followers.side_call_permits();
            let _permit = permits.acquire().await;
            let result = state
                .workspace_runtime
                .modules_cancel(
                    &handle,
                    &lifecycle_request(attempt, format!("agent-cancel:{exec_id}")),
                    exec_id,
                    &reason,
                )
                .await;
            match result {
                Ok(disposition) => tracing::info!(
                    target: "g5_api::live_relay",
                    task_id = %attempt.task_id,
                    attempt = attempt.attempt,
                    disposition = %disposition["disposition"].as_str().unwrap_or("unknown"),
                    reason,
                    "cancellation redelivered"
                ),
                Err(error) => tracing::warn!(
                    task_id = %attempt.task_id,
                    attempt = attempt.attempt,
                    error = %error.message,
                    "cancellation redelivery failed; retrying"
                ),
            }
        }));
    }
}

/// What one fetch cycle of the live reader did.
struct FetchOutcome {
    fetched_any: bool,
    backlog: bool,
    failed: bool,
}

/// One attempt's follower loop: the live reader (fetch, bind, publish,
/// buffer), the history writer (one batch in flight), control delivery and
/// the guest status probe, each a step of the loop below.
struct FollowerLoop {
    state: Arc<AppState>,
    follower: Arc<Follower>,
    deps: TenantMaintenanceDeps,
    permits: Arc<Semaphore>,
    shutdown: CancellationToken,
    segments: SegmentMap,
    control: ControlDelivery,
    fetch_nonce: u64,
    backoff: Duration,
    last_received_at: DateTime<Utc>,
    /// When the last status probe returned (not when it started), so a probe
    /// that ran out its budget is not followed by another at once.
    last_probe: Instant,
    /// The current probe cadence: the base interval, doubled after a failed
    /// probe up to [`MAX_STATUS_PROBE_INTERVAL`], reset by a successful one.
    probe_interval: Duration,
    /// The status probe in flight, on its own task under a side-call permit.
    probe: Option<tokio::task::JoinHandle<Result<bool, AppError>>>,
    flush_in_flight: Option<tokio::task::JoinHandle<Result<FlushOutcome, FlushError>>>,
}

/// Why the loop ends.
enum Exit {
    Shutdown,
    IncarnationGone,
    Settled,
    Superseded,
}

async fn run(
    state: Arc<AppState>,
    follower: Arc<Follower>,
    permits: Arc<Semaphore>,
    shutdown: CancellationToken,
) {
    let deps = deps(&state);
    let segments = match seed_segments(&state, &deps, &follower.attempt).await {
        Ok(segments) => segments,
        Err(error) => {
            tracing::warn!(task_id = %follower.attempt.task_id, attempt = follower.attempt.attempt, %error,
                "attempt follower could not seed its segment map; the reconciler will retry");
            return;
        }
    };
    let mut lp = FollowerLoop {
        state,
        follower,
        deps,
        permits,
        shutdown,
        segments,
        control: ControlDelivery::new(),
        fetch_nonce: 0,
        backoff: Duration::from_millis(500),
        last_received_at: Utc::now(),
        last_probe: Instant::now(),
        probe_interval: STATUS_PROBE_INTERVAL,
        probe: None,
        flush_in_flight: None,
    };
    let exit = lp.run().await;
    lp.control.abort();
    if let Some(probe) = lp.probe.take() {
        probe.abort();
    }
    if let Some(flush) = lp.flush_in_flight.take() {
        let _ = flush.await;
    }
    let attempt = &lp.follower.attempt;
    match exit {
        Exit::Settled => tracing::info!(
            target: "g5_api::live_relay",
            task_id = %attempt.task_id,
            attempt = attempt.attempt,
            "attempt settled; follower ending"
        ),
        Exit::Superseded => tracing::debug!(
            target: "g5_api::live_relay",
            task_id = %attempt.task_id,
            attempt = attempt.attempt,
            "attempt settled or superseded elsewhere; follower ending"
        ),
        Exit::IncarnationGone | Exit::Shutdown => {}
    }
}

impl FollowerLoop {
    async fn run(&mut self) -> Exit {
        loop {
            if self.shutdown.is_cancelled() {
                return Exit::Shutdown;
            }
            self.control.reconcile(&self.follower, &self.deps).await;
            let handle = match self
                .state
                .workspace_runtime
                .get(self.follower.attempt.workspace_id)
                .await
            {
                Ok(Some(handle))
                    if handle.vm_id == self.follower.attempt.vm_id
                        && handle.epoch == self.follower.attempt.epoch =>
                {
                    handle
                }
                // Lost or replaced incarnation: the reconciler settles it
                // from the database once this follower is gone.
                Ok(_) => return Exit::IncarnationGone,
                Err(error) => {
                    tracing::debug!(%error, "attempt follower could not read the runtime handle");
                    self.back_off().await;
                    continue;
                }
            };
            self.control.deliver(&self.state, &self.follower, &handle);

            let fetch = self.fetch_pages(&handle).await;
            if fetch.failed {
                // The fetch permit is already released, so a stalled
                // runtime does not hold a slot other followers could use.
                self.back_off().await;
            }
            self.maybe_rebuild_tail().await;
            self.maybe_start_flush(&handle, fetch.backlog);

            if !fetch.fetched_any && self.flush_in_flight.is_none() {
                match attempt_superseded(&self.deps, &self.follower.attempt).await {
                    Ok(true) => return Exit::Superseded,
                    Ok(false) => {}
                    Err(error) => {
                        tracing::debug!(%error, "attempt follower could not read its attempt")
                    }
                }
                if self.probe.is_none() && self.probe_due() {
                    let state = self.state.clone();
                    let follower = self.follower.clone();
                    let handle = handle.clone();
                    self.probe = Some(tokio::spawn(async move {
                        // One of the bounded side-call permits: the probe
                        // pins a pooled connection until the guest answers.
                        let permits = state.followers.side_call_permits();
                        let _permit = permits.acquire().await;
                        let deps = deps(&state);
                        probe_status(&state, &deps, &follower.attempt, &handle, &follower.exec_id)
                            .await
                    }));
                }
            }

            if let Some(exit) = self.wait(fetch.fetched_any || fetch.backlog).await {
                return exit;
            }
        }
    }

    async fn back_off(&mut self) {
        tokio::time::sleep(self.backoff).await;
        self.backoff = (self.backoff * 2).min(MAX_BACKOFF);
    }

    fn probe_due(&self) -> bool {
        let buffered = self
            .follower
            .tail
            .lock()
            .map(|t| !t.entries.is_empty())
            .unwrap_or(false);
        !buffered && self.last_probe.elapsed() >= self.probe_interval
    }

    /// The probe returned: re-arm from now, backing off after a failure.
    fn on_probe_outcome(
        &mut self,
        joined: Result<Result<bool, AppError>, tokio::task::JoinError>,
    ) -> Option<Exit> {
        self.last_probe = Instant::now();
        match joined {
            Ok(Ok(true)) => Some(Exit::Settled),
            Ok(Ok(false)) => {
                self.probe_interval = STATUS_PROBE_INTERVAL;
                None
            }
            Ok(Err(error)) => {
                self.probe_interval = (self.probe_interval * 2).min(MAX_STATUS_PROBE_INTERVAL);
                tracing::warn!(
                    target: "g5_api::live_relay",
                    task_id = %self.follower.attempt.task_id,
                    attempt = self.follower.attempt.attempt,
                    %error,
                    next_probe_s = self.probe_interval.as_secs(),
                    "attempt follower status probe failed"
                );
                None
            }
            Err(join_error) => {
                self.probe_interval = (self.probe_interval * 2).min(MAX_STATUS_PROBE_INTERVAL);
                tracing::error!(%join_error, "status probe task panicked");
                None
            }
        }
    }

    /// The live reader: every page behind the fetched cursor, bound to the
    /// claimed identity, published at once and buffered for history.
    async fn fetch_pages(&mut self, handle: &WorkspaceRuntimeHandle) -> FetchOutcome {
        let attempt = self.follower.attempt.clone();
        let exec_id = self.follower.exec_id.clone();
        let mut outcome = FetchOutcome {
            fetched_any: false,
            backlog: false,
            failed: false,
        };
        let fetched_through = self
            .follower
            .tail
            .lock()
            .map(|t| t.fetched_through)
            .unwrap_or(0);
        let permits = self.permits.clone();
        let _permit = permits.acquire().await;
        let mut from = fetched_through.saturating_add(1);
        loop {
            self.fetch_nonce = self.fetch_nonce.wrapping_add(1);
            self.follower.stats.fetches.fetch_add(1, Ordering::Relaxed);
            let page = self
                .state
                .workspace_runtime
                .modules_events_page(
                    handle,
                    &lifecycle_request(
                        &attempt,
                        format!("agent-events:{exec_id}:{from}:{}", self.fetch_nonce),
                    ),
                    &exec_id,
                    from,
                    PAGE_LIMIT,
                    PAGE_MAX_BYTES,
                )
                .await;
            let page = match page {
                Ok(page) => page,
                Err(error) => {
                    tracing::warn!(task_id = %attempt.task_id, attempt = attempt.attempt, error = %error.message,
                        "attempt follower could not fetch the journal; retrying");
                    outcome.failed = true;
                    return outcome;
                }
            };
            self.backoff = Duration::from_millis(500);
            let events = page["events"].as_array().cloned().unwrap_or_default();
            let has_more = page["has_more"].as_bool().unwrap_or(false);
            let mut published: Vec<TimelineEntryPayload> = Vec::new();
            let mut stop = false;
            for value in &events {
                let event = match journal_event_from_replay(
                    attempt.task_id,
                    attempt.attempt,
                    attempt.vm_id,
                    attempt.epoch,
                    &exec_id,
                    value,
                )
                .and_then(|event| event.validate_identity().map(|()| event))
                {
                    Ok(event) => event,
                    Err(error) => {
                        tracing::warn!(%error, "attempt follower refused a replay record");
                        stop = true;
                        break;
                    }
                };
                if event.seq < from {
                    continue;
                }
                if event.seq != from {
                    // The guest skipped a sequence: refetch from the cursor
                    // rather than publishing past a gap.
                    tracing::warn!(
                        expected = from,
                        received = event.seq,
                        "attempt follower saw a journal gap; refetching"
                    );
                    stop = true;
                    break;
                }
                from = from.saturating_add(1);
                let terminal = event.terminal;
                if let Some(entry) = self.admit(event) {
                    published.push(entry);
                }
                if terminal {
                    // Nothing follows the terminal event.
                    stop = true;
                    break;
                }
            }
            let count = published.len() as u64;
            self.publish_live(published);
            if count > 0 {
                outcome.fetched_any = true;
                self.follower
                    .stats
                    .published
                    .fetch_add(count, Ordering::Relaxed);
                tracing::debug!(
                    target: "g5_api::live_relay",
                    task_id = %attempt.task_id,
                    attempt = attempt.attempt,
                    events = count,
                    through = from.saturating_sub(1),
                    "journal page published live"
                );
            }
            if stop || !has_more {
                return outcome;
            }
            outcome.backlog = true;
        }
    }

    /// Bind one bound event: its receipt time, owning segment and buffer
    /// slot. Returns the live copy to publish, or none when the copy was
    /// already published before a rebuild.
    fn admit(&mut self, event: JournalTaskEvent) -> Option<TimelineEntryPayload> {
        let attempt = &self.follower.attempt;
        // Each event gets its own receipt time, strictly increasing across
        // pages: the stored row's `created_at` orders the timeline.
        let received_at = next_receipt(&mut self.last_received_at);
        if event.kind == "input.shipped" {
            self.segments.observe_shipped(&event.payload);
        }
        let message_id = self.segments.assign(&event.kind, &event.payload);
        let bytes = event_bytes(&event);
        let entry =
            journal_timeline_entry(&event, message_id, attempt.session_id, received_at, true);
        let seq = event.seq;
        let mut publish = true;
        if let Ok(mut tail) = self.follower.tail.lock() {
            if seq <= tail.published_through {
                publish = false;
            } else {
                tail.published_through = seq;
            }
            tail.fetched_through = seq;
            if let Admission::Overflowed = tail.accept(Buffered {
                event,
                received_at,
                message_id,
                bytes,
            }) {
                // Overflow: the tail is rebuilt from the journal once
                // history catches up; live delivery goes on.
                self.follower.stats.resyncs.fetch_add(1, Ordering::Relaxed);
                tracing::warn!(
                    target: "g5_api::live_relay",
                    task_id = %attempt.task_id,
                    attempt = attempt.attempt,
                    buffered = tail.entries.len(),
                    "unpersisted tail overflowed; it will be rebuilt from the journal"
                );
            }
        }
        publish.then_some(entry)
    }

    fn publish_live(&self, entries: Vec<TimelineEntryPayload>) {
        let task_id = self.follower.attempt.task_id;
        for entry in entries {
            self.state.event_bus.publish(
                &Channel::Task(task_id),
                &WsEvent::TaskTimeline {
                    task_id,
                    entry: entry.clone(),
                },
            );
            if let Some(session_id) = self.follower.attempt.session_id {
                self.state.event_bus.publish(
                    &Channel::Chat(session_id),
                    &WsEvent::ChatTimeline {
                        session_id,
                        task_id,
                        entry,
                    },
                );
            }
        }
    }

    /// After an overflow the buffer's contiguous prefix is still stored;
    /// once it is empty the tail is rebuilt from the persisted prefix,
    /// refetching what the guest retains beyond it (published copies are
    /// not sent twice). The segment map advanced while events were
    /// discarded, so it is reseeded from the persisted rows first: a
    /// refetched event is then assigned exactly as a restart would.
    async fn maybe_rebuild_tail(&mut self) {
        if self.flush_in_flight.is_some() {
            return;
        }
        let rebuild = self
            .follower
            .tail
            .lock()
            .map(|mut tail| tail.begin_rebuild())
            .unwrap_or(false);
        if !rebuild {
            return;
        }
        match seed_segments(&self.state, &self.deps, &self.follower.attempt).await {
            Ok(seeded) => {
                self.segments = seeded;
                self.follower.wake.notify_one();
            }
            Err(error) => {
                tracing::warn!(%error, "segment map reseed failed; retrying the rebuild");
                if let Ok(mut tail) = self.follower.tail.lock() {
                    tail.complete = false;
                }
            }
        }
    }

    /// The history writer: store a batch when one is due and none is in
    /// flight. The batch runs on its own task so fetching and publishing
    /// never wait on the database.
    fn maybe_start_flush(&mut self, handle: &WorkspaceRuntimeHandle, backlog: bool) {
        if self.flush_in_flight.is_some() {
            return;
        }
        let due = self.follower.tail.lock().ok().and_then(|tail| {
            flush_reason(&tail, backlog, Instant::now()).map(|reason| {
                (
                    reason,
                    next_batch(&tail),
                    tail.first_unflushed_at
                        .map(|first| first.elapsed().as_millis() as u64)
                        .unwrap_or(0),
                )
            })
        });
        let Some((reason, batch, first_age_ms)) = due else {
            return;
        };
        if batch.is_empty() {
            return;
        }
        self.flush_in_flight = Some(tokio::spawn(flush(
            self.state.clone(),
            self.follower.clone(),
            handle.clone(),
            self.segments.clone(),
            batch,
            reason,
            first_age_ms,
        )));
    }

    /// Wait for a hint, the poll, the batch timer or the batch in flight.
    async fn wait(&mut self, active: bool) -> Option<Exit> {
        let poll = if active { ACTIVE_POLL } else { IDLE_POLL };
        let flush_deadline = self.follower.tail.lock().ok().and_then(|tail| {
            tail.first_unflushed_at
                .map(|first| first + FLUSH_INTERVAL)
                .filter(|_| self.flush_in_flight.is_none())
        });
        let sleep_for = flush_deadline
            .map(|deadline| deadline.saturating_duration_since(Instant::now()).min(poll))
            .unwrap_or(poll);
        let wake = self.follower.wake.clone();
        let shutdown = self.shutdown.clone();
        let flush_in_flight = &mut self.flush_in_flight;
        let probe = &mut self.probe;
        let flush_wait = async {
            match flush_in_flight.as_mut() {
                Some(flush) => flush.await,
                None => std::future::pending().await,
            }
        };
        let probe_wait = async {
            match probe.as_mut() {
                Some(probe) => probe.await,
                None => std::future::pending().await,
            }
        };
        enum Woken {
            Flush(Result<Result<FlushOutcome, FlushError>, tokio::task::JoinError>),
            Probe(Result<Result<bool, AppError>, tokio::task::JoinError>),
        }
        let woken = tokio::select! {
            _ = wake.notified() => return None,
            _ = tokio::time::sleep(sleep_for) => return None,
            _ = shutdown.cancelled() => return Some(Exit::Shutdown),
            joined = flush_wait => Woken::Flush(joined),
            joined = probe_wait => Woken::Probe(joined),
        };
        match woken {
            Woken::Flush(joined) => {
                self.flush_in_flight = None;
                self.on_flush_outcome(joined).await
            }
            Woken::Probe(joined) => {
                self.probe = None;
                self.on_probe_outcome(joined)
            }
        }
    }

    async fn on_flush_outcome(
        &mut self,
        joined: Result<Result<FlushOutcome, FlushError>, tokio::task::JoinError>,
    ) -> Option<Exit> {
        let attempt = self.follower.attempt.clone();
        match joined {
            Ok(Ok(outcome)) => {
                if let (Ok(mut tail), Some(through)) =
                    (self.follower.tail.lock(), outcome.ack_through)
                {
                    tail.drop_persisted(through, outcome.reason);
                }
                self.follower.stats.flushes.fetch_add(1, Ordering::Relaxed);
                self.follower
                    .stats
                    .flushed_events
                    .fetch_add(outcome.stored, Ordering::Relaxed);
                if outcome.terminal {
                    return Some(Exit::Settled);
                }
                // Drain a ready backlog batch by batch.
                self.follower.wake.notify_one();
                None
            }
            Ok(Err(FlushError::Resync(reason))) => {
                self.follower.stats.resyncs.fetch_add(1, Ordering::Relaxed);
                tracing::warn!(
                    target: "g5_api::live_relay",
                    task_id = %attempt.task_id,
                    attempt = attempt.attempt,
                    %reason,
                    "history batch refused; rebuilding the tail from the journal"
                );
                match read_persisted_through(&self.deps, &attempt).await {
                    Ok(persisted) => {
                        if let Ok(mut tail) = self.follower.tail.lock() {
                            tail.resync(persisted);
                        }
                        match seed_segments(&self.state, &self.deps, &attempt).await {
                            Ok(seeded) => self.segments = seeded,
                            Err(error) => tracing::warn!(%error, "segment map reseed failed"),
                        }
                        if is_attempt_settled(&self.deps, &attempt)
                            .await
                            .unwrap_or(false)
                        {
                            return Some(Exit::Settled);
                        }
                    }
                    Err(error) => tracing::warn!(%error, "could not read the persisted prefix"),
                }
                self.back_off().await;
                None
            }
            Ok(Err(FlushError::Retry(reason))) => {
                tracing::warn!(
                    target: "g5_api::live_relay",
                    task_id = %attempt.task_id,
                    attempt = attempt.attempt,
                    %reason,
                    "history batch failed; retrying"
                );
                self.back_off().await;
                None
            }
            Err(join_error) => {
                tracing::error!(%join_error, "history batch task panicked");
                self.back_off().await;
                None
            }
        }
    }
}

/// The history writer's unit of work: store one batch and publish its
/// post-commit updates. The guest acknowledgement is journal-retention
/// bookkeeping, done by its own task, so a slow guest never delays the
/// next batch; the reconciler's retry scan covers a failed one.
async fn flush(
    state: Arc<AppState>,
    follower: Arc<Follower>,
    handle: WorkspaceRuntimeHandle,
    mut segments: SegmentMap,
    batch: Vec<JournalBatchEvent>,
    reason: FlushReason,
    first_age_ms: u64,
) -> Result<FlushOutcome, FlushError> {
    let attempt = &follower.attempt;
    let deps = deps(&state);
    let batch_len = batch.len() as u64;
    let batch_bytes: usize = batch.iter().map(|entry| event_bytes(&entry.event)).sum();
    let started = Instant::now();
    let mut db = scoped_db_for_maintenance_org(&deps, attempt.org_id)
        .await
        .map_err(|error| FlushError::Retry(error.to_string()))?;
    sqlx::query(&format!(
        "SET LOCAL lock_timeout = '{}ms'",
        BATCH_LOCK_WAIT.as_millis()
    ))
    .execute(db.conn())
    .await
    .map_err(|error| FlushError::Retry(error.to_string()))?;
    let result = state
        .task_service
        .apply_journal_batch(&mut db, &mut segments, batch)
        .await
        .map_err(|error| match error {
            BatchApplyError::Inconsistent(reason) => FlushError::Resync(reason),
            BatchApplyError::Failed(error) => FlushError::Retry(error.to_string()),
        })?;
    db.commit()
        .await
        .map_err(|error| FlushError::Retry(error.to_string()))?;
    let commit_ms = started.elapsed().as_millis() as u64;
    for (channel, event) in &result.live {
        state.event_bus.publish(channel, event);
    }
    if let Some(through) = result.ack_through {
        request_ack(&state, &follower, &handle, through);
    }
    tracing::info!(
        target: "g5_api::live_relay",
        task_id = %attempt.task_id,
        attempt = attempt.attempt,
        reason,
        batch_events = batch_len,
        batch_bytes,
        stored = result.applied,
        duplicates = result.duplicates,
        persisted_through = result.ack_through.unwrap_or(0),
        oldest_unflushed_ms = first_age_ms,
        commit_ms,
        task_status = %result.task_status,
        terminal = result.terminal,
        "history batch committed"
    );
    Ok(FlushOutcome {
        ack_through: result.ack_through,
        terminal: result.terminal,
        stored: result.applied,
        reason,
    })
}

/// Ask for the committed prefix to be acknowledged. One acknowledgement
/// task runs per follower and always sends the latest prefix, so a slow
/// guest answer never queues one call per batch.
fn request_ack(
    state: &Arc<AppState>,
    follower: &Arc<Follower>,
    handle: &WorkspaceRuntimeHandle,
    through: u64,
) {
    follower.ack_pending.fetch_max(through, Ordering::AcqRel);
    if follower
        .ack_running
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_ok()
    {
        tokio::spawn(acknowledge_pending(
            state.clone(),
            follower.clone(),
            handle.clone(),
        ));
    }
}

/// Acknowledgement cleanup: tell the guest the committed prefix and record
/// its confirmation (`g5.chat.journal`), under the process-wide
/// acknowledgement bound. Cumulative, so a lost one is covered by the
/// next, or by the reconciler's retry scan.
async fn acknowledge_pending(
    state: Arc<AppState>,
    follower: Arc<Follower>,
    handle: WorkspaceRuntimeHandle,
) {
    loop {
        let through = follower.ack_pending.swap(0, Ordering::AcqRel);
        if through == 0 {
            follower.ack_running.store(false, Ordering::Release);
            // A prefix requested between the swap and the release above
            // belongs to this task unless a new one has started.
            if follower.ack_pending.load(Ordering::Acquire) == 0
                || follower
                    .ack_running
                    .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
                    .is_err()
            {
                return;
            }
            continue;
        }
        let permits = state.followers.side_call_permits();
        let _permit = permits.acquire().await;
        acknowledge(&state, &follower, &handle, through).await;
    }
}

async fn acknowledge(
    state: &Arc<AppState>,
    follower: &Arc<Follower>,
    handle: &WorkspaceRuntimeHandle,
    through: u64,
) {
    let attempt = &follower.attempt;
    let exec_id = &follower.exec_id;
    let deps = deps(state);
    let started = Instant::now();
    let acked = state
        .workspace_runtime
        .modules_events_ack(
            handle,
            &lifecycle_request(attempt, format!("agent-ack:{exec_id}:{through}")),
            exec_id,
            through,
        )
        .await;
    match acked {
        Ok(_) => {
            follower.stats.acks.fetch_add(1, Ordering::Relaxed);
            tracing::debug!(
                target: "g5_api::live_relay",
                task_id = %attempt.task_id,
                attempt = attempt.attempt,
                through,
                ack_ms = started.elapsed().as_millis() as u64,
                "journal prefix acknowledged"
            );
            if let Err(error) = record_journal_ack(state, &deps, attempt, through).await {
                tracing::debug!(%error, "journal acknowledgement record failed; the retry scan covers it");
            }
        }
        Err(error) => {
            follower.stats.ack_failures.fetch_add(1, Ordering::Relaxed);
            tracing::warn!(
                target: "g5_api::live_relay",
                task_id = %attempt.task_id,
                attempt = attempt.attempt,
                through,
                error = %error.message,
                "journal acknowledgement failed; the retry scan will repeat it"
            );
        }
    }
}

async fn seed_segments(
    state: &Arc<AppState>,
    deps: &TenantMaintenanceDeps,
    attempt: &FollowedAttempt,
) -> Result<SegmentMap, AppError> {
    let _ = state;
    let mut db = scoped_db_for_maintenance_org(deps, attempt.org_id).await?;
    let segments = seed_segment_map(&mut db, attempt.task_id, attempt.attempt).await?;
    db.commit().await?;
    Ok(segments)
}

async fn read_cancel_reason(
    follower: &Follower,
    deps: &TenantMaintenanceDeps,
) -> Result<Option<String>, AppError> {
    let attempt = &follower.attempt;
    #[cfg(feature = "test-helpers")]
    {
        let remaining = follower
            .inject_control_read_failures
            .load(Ordering::Acquire);
        if remaining > 0 {
            follower
                .inject_control_read_failures
                .store(remaining - 1, Ordering::Release);
            return Err(AppError::from_error(
                g5_contracts::error::G5Error::ServiceUnavailable {
                    message: "injected control read failure".to_string(),
                },
            ));
        }
    }
    let mut db = scoped_db_for_maintenance_org(deps, attempt.org_id).await?;
    let row: Option<(bool, Option<String>, String)> = sqlx::query_as(
        "SELECT COALESCE((spec #>> '{lifecycle,cancel_requested}')::boolean, false),
                spec #>> '{lifecycle,interrupt_reason}', status
         FROM tasks WHERE id = $1",
    )
    .bind(attempt.task_id)
    .fetch_optional(db.conn())
    .await?;
    db.commit().await?;
    Ok(row.and_then(|(cancel, interrupt, status)| {
        if status != "running" {
            None
        } else if cancel {
            Some("owner_cancel".to_string())
        } else {
            interrupt.map(|_| "interrupted".to_string())
        }
    }))
}

/// Whether the attempt ended, or the task moved on to a later attempt.
async fn attempt_superseded(
    deps: &TenantMaintenanceDeps,
    attempt: &FollowedAttempt,
) -> Result<bool, AppError> {
    let mut db = scoped_db_for_maintenance_org(deps, attempt.org_id).await?;
    let row: Option<(i32, bool)> = sqlx::query_as(
        "SELECT task.attempt, a.ended_at IS NOT NULL
         FROM tasks AS task
         JOIN task_attempts AS a ON a.task_id = task.id AND a.attempt = $2
         WHERE task.id = $1",
    )
    .bind(attempt.task_id)
    .bind(attempt.attempt)
    .fetch_optional(db.conn())
    .await?;
    db.commit().await?;
    Ok(match row {
        Some((current, ended)) => ended || current != attempt.attempt,
        None => true,
    })
}

/// The attempt's current start confirmation: dispatch may confirm it after
/// the follower started, and only a confirmed execution the guest does not
/// know is a lost one.
async fn start_confirmed(
    deps: &TenantMaintenanceDeps,
    attempt: &FollowedAttempt,
) -> Result<bool, AppError> {
    let mut db = scoped_db_for_maintenance_org(deps, attempt.org_id).await?;
    let confirmed: bool = sqlx::query_scalar(
        "SELECT start_confirmed FROM task_attempts WHERE task_id = $1 AND attempt = $2",
    )
    .bind(attempt.task_id)
    .bind(attempt.attempt)
    .fetch_one(db.conn())
    .await?;
    db.commit().await?;
    Ok(confirmed)
}

async fn read_persisted_through(
    deps: &TenantMaintenanceDeps,
    attempt: &FollowedAttempt,
) -> Result<u64, AppError> {
    let mut db = scoped_db_for_maintenance_org(deps, attempt.org_id).await?;
    let acked: i64 = sqlx::query_scalar(
        "SELECT acked_event_seq FROM task_attempts WHERE task_id = $1 AND attempt = $2",
    )
    .bind(attempt.task_id)
    .bind(attempt.attempt)
    .fetch_one(db.conn())
    .await?;
    db.commit().await?;
    Ok(u64::try_from(acked).unwrap_or(0))
}

async fn is_attempt_settled(
    deps: &TenantMaintenanceDeps,
    attempt: &FollowedAttempt,
) -> Result<bool, AppError> {
    let mut db = scoped_db_for_maintenance_org(deps, attempt.org_id).await?;
    let ended: bool = sqlx::query_scalar(
        "SELECT ended_at IS NOT NULL FROM task_attempts WHERE task_id = $1 AND attempt = $2",
    )
    .bind(attempt.task_id)
    .bind(attempt.attempt)
    .fetch_one(db.conn())
    .await?;
    db.commit().await?;
    Ok(ended)
}

/// Record the guest's confirmed acknowledgement (`g5.chat.journal`).
pub(crate) async fn record_journal_ack(
    state: &AppState,
    deps: &TenantMaintenanceDeps,
    attempt: &FollowedAttempt,
    through: u64,
) -> Result<(), AppError> {
    let mut db = scoped_db_for_maintenance_org(deps, attempt.org_id).await?;
    state
        .task_service
        .record_journal_acknowledgement(&mut db, attempt.task_id, attempt.attempt, through)
        .await?;
    db.commit().await?;
    Ok(())
}

/// Ask the guest about an execution that produced nothing for a while.
/// Returns whether the attempt settled (and the follower should end).
async fn probe_status(
    state: &Arc<AppState>,
    deps: &TenantMaintenanceDeps,
    attempt: &FollowedAttempt,
    handle: &WorkspaceRuntimeHandle,
    exec_id: &str,
) -> Result<bool, AppError> {
    let status = state
        .workspace_runtime
        .modules_status(
            handle,
            &lifecycle_request(
                attempt,
                format!("agent-status:{exec_id}:{}", unix_time_ms()),
            ),
            exec_id,
        )
        .await
        .map_err(|error| {
            AppError::from_error(g5_contracts::error::G5Error::ServiceUnavailable {
                message: format!("workspace runtime call failed: {}", error.message),
            })
        })?;
    let event = if status["status"].as_str() == Some("cancelled") {
        Some(TaskLifecycleEvent::StartDisposition {
            task_id: attempt.task_id,
            attempt: attempt.attempt,
            vm_id: attempt.vm_id,
            epoch: attempt.epoch,
            disposition: json!({
                "disposition": "cancelled",
                "reason": status["reason"],
            }),
        })
    } else if status["status"].as_str() == Some("finished")
        && status["persistence"].as_str() == Some("failed")
    {
        let pending = state
            .workspace_runtime
            .modules_outcomes_pending(
                handle,
                &lifecycle_request(
                    attempt,
                    format!("agent-pending:{exec_id}:{}", unix_time_ms()),
                ),
            )
            .await
            .map_err(|error| {
                AppError::from_error(g5_contracts::error::G5Error::ServiceUnavailable {
                    message: format!("workspace runtime call failed: {}", error.message),
                })
            })?;
        let outcome = pending["pending"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|entry| {
                entry["exec_id"].as_str() == Some(exec_id)
                    && entry["source"].as_str() == Some("memory")
            })
            .and_then(|entry| entry.get("outcome"))
            .cloned();
        outcome.map(|outcome| TaskLifecycleEvent::RecoveredOutcome {
            task_id: attempt.task_id,
            attempt: attempt.attempt,
            vm_id: attempt.vm_id,
            epoch: attempt.epoch,
            outcome,
        })
    } else if status["status"].as_str() == Some("unknown")
        && start_confirmed(deps, attempt)
            .await
            .unwrap_or(attempt.start_confirmed)
    {
        Some(TaskLifecycleEvent::VmLost {
            task_id: attempt.task_id,
            attempt: attempt.attempt,
            vm_id: attempt.vm_id,
            epoch: attempt.epoch,
            evidence: json!({"reason":"confirmed_execution_missing"}),
        })
    } else {
        None
    };
    let Some(event) = event else {
        return Ok(false);
    };
    let mut db = scoped_db_for_maintenance_org(deps, attempt.org_id).await?;
    let result = state.task_service.apply(&mut db, event).await?;
    db.commit().await?;
    Ok(result.terminal)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The two bounds together leave connections free for requests on the
    /// default pool of 20 as on the deployed 54, and never exceed the fixed
    /// fetch cap.
    #[test]
    fn guest_call_bounds_leave_the_pool_room() {
        for pool in [20u32, 54] {
            let pinned = fetch_concurrency(pool) + side_call_concurrency(pool);
            assert!(
                pinned <= (pool as usize) * 3 / 4,
                "pool {pool}: fetches and side calls may pin {pinned} connections"
            );
            assert!(fetch_concurrency(pool) <= FETCH_CONCURRENCY);
        }
        assert_eq!(fetch_concurrency(20), 10);
        assert_eq!(side_call_concurrency(20), 5);
        assert_eq!(fetch_concurrency(54), 16);
        assert_eq!(side_call_concurrency(54), 13);
        // A tiny pool still allows one of each.
        assert_eq!(fetch_concurrency(2), 1);
        assert_eq!(side_call_concurrency(2), 1);
        let registry = FollowerRegistry::for_pool(8);
        assert_eq!((registry.fetch_limit(), registry.side_call_limit()), (4, 2));
    }

    fn event(seq: u64, kind: &str, terminal: bool) -> Buffered {
        let task_id = Uuid::from_u128(1);
        let vm_id = Uuid::from_u128(2);
        let event = JournalTaskEvent {
            task_id,
            attempt: 1,
            vm_id,
            epoch: 1,
            exec_id: canonical_exec_id(task_id, 1, vm_id),
            seq,
            kind: kind.to_string(),
            payload: json!({"text_delta": "x"}),
            terminal,
        };
        let bytes = event_bytes(&event);
        Buffered {
            event,
            received_at: Utc::now(),
            message_id: Uuid::from_u128(3),
            bytes,
        }
    }

    fn tail(entries: Vec<Buffered>, first_age: Option<Duration>) -> TailState {
        let bytes = entries.iter().map(|b| b.bytes).sum();
        TailState {
            persisted_through: 0,
            fetched_through: entries.last().map(|b| b.event.seq).unwrap_or(0),
            published_through: entries.last().map(|b| b.event.seq).unwrap_or(0),
            complete: true,
            entries: entries.into(),
            bytes,
            first_unflushed_at: first_age.map(|age| Instant::now() - age),
            drain: false,
        }
    }

    #[test]
    fn routine_output_waits_five_seconds_not_half_a_second() {
        let entries = vec![
            event(1, "message.delta", false),
            event(2, "message.delta", false),
        ];
        let half = tail(entries, Some(Duration::from_millis(500)));
        assert_eq!(flush_reason(&half, false, Instant::now()), None);
        let entries = vec![event(1, "message.delta", false)];
        let five = tail(entries, Some(Duration::from_secs(5)));
        assert_eq!(flush_reason(&five, false, Instant::now()), Some("interval"));
    }

    #[test]
    fn boundaries_capacity_and_backlog_flush_at_once() {
        let entries = vec![
            event(1, "message.delta", false),
            event(2, "invocation.outcome", true),
        ];
        let boundary = tail(entries, Some(Duration::from_millis(1)));
        assert_eq!(
            flush_reason(&boundary, false, Instant::now()),
            Some("boundary")
        );
        let entries = (1..=BATCH_MAX_EVENTS as u64)
            .map(|seq| event(seq, "message.delta", false))
            .collect();
        let full = tail(entries, Some(Duration::from_millis(1)));
        assert_eq!(flush_reason(&full, false, Instant::now()), Some("capacity"));
        let entries = vec![event(1, "tool.started", false)];
        let backlog = tail(entries, Some(Duration::from_millis(1)));
        assert_eq!(
            flush_reason(&backlog, true, Instant::now()),
            Some("backlog")
        );
        assert_eq!(
            flush_reason(&tail(Vec::new(), None), true, Instant::now()),
            None
        );
        let mut leftover = tail(
            vec![event(1, "message.delta", false)],
            Some(Duration::from_millis(1)),
        );
        leftover.drain = true;
        assert_eq!(
            flush_reason(&leftover, false, Instant::now()),
            Some("backlog"),
            "what a capacity batch left behind was already waiting"
        );
    }

    #[test]
    fn a_batch_is_bounded_and_ends_at_the_terminal_event() {
        let mut entries: Vec<Buffered> = (1..=(BATCH_MAX_EVENTS as u64 + 10))
            .map(|seq| event(seq, "message.delta", false))
            .collect();
        let state = tail(std::mem::take(&mut entries), Some(Duration::from_secs(6)));
        assert_eq!(next_batch(&state).len(), BATCH_MAX_EVENTS);
        let entries = vec![
            event(1, "message.delta", false),
            event(2, "invocation.outcome", true),
            event(3, "message.delta", false),
        ];
        let state = tail(entries, Some(Duration::from_secs(6)));
        let batch = next_batch(&state);
        assert_eq!(batch.len(), 2);
        assert!(batch[1].event.terminal);
    }
}
