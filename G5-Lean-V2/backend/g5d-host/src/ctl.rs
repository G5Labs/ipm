//! Host side of the versioned worker-host `ctl` stream.
//!
//! The control session owns protocol negotiation, host-monotonic command
//! budgets, idempotent retained command results, heartbeat traffic, drain
//! state, and durable VM-event replay. VM boot/stop execution is supplied by
//! the workspace-VM host implementation; this module does not simulate it.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Weak};
use std::time::{Duration, Instant};

use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use chrono::Utc;
use g5_internal_api::host_ctl::{
    method, CommandParams, CommandResult, CurrentIncarnation, HeartbeatAck, HelloDisposition,
    HostDrain, HostDrained, HostHeartbeat, HostHello, HostReject, LeaseRenew, LeaseRenewResult,
    RootsPurge, RpcMessage, RpcNotification, RpcRequest, RpcResponse, VmEvent, VmEventAck,
    VmEventKind, VmFileChanged, VmModuleEvents, VmStart, VmStatus, VmStop, MAX_CTL_MESSAGE_BYTES,
    MAX_INFLIGHT_COMMANDS, PROTOCOL_VERSION,
};
use g5_internal_api::stream_mux::{MuxReader, MuxStream, MuxWriter};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use tokio::sync::{mpsc, oneshot, watch, Mutex, OwnedSemaphorePermit, Semaphore};
use tokio::task::JoinHandle;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct JournalState {
    #[serde(default)]
    last_seq: HashMap<Uuid, u64>,
    #[serde(default)]
    events: Vec<VmEvent>,
}

impl JournalState {
    fn validate(&self) -> Result<()> {
        let mut previous = HashMap::<Uuid, u64>::new();
        for event in &self.events {
            event.validate().map_err(anyhow::Error::msg)?;
            let last = previous.entry(event.vm_id).or_default();
            if event.seq <= *last {
                return Err(anyhow!(
                    "event journal sequence is not increasing for vm {}",
                    event.vm_id
                ));
            }
            *last = event.seq;
            if self.last_seq.get(&event.vm_id).copied().unwrap_or_default() < event.seq {
                return Err(anyhow!(
                    "event journal watermark trails event {} for vm {}",
                    event.seq,
                    event.vm_id
                ));
            }
        }
        Ok(())
    }
}

/// File-backed VM event journal. Sequence watermarks survive acknowledgement,
/// so restarting the host never reuses `(vm_id, seq)`.
pub struct EventJournal {
    path: PathBuf,
    state: Mutex<JournalState>,
    changed: watch::Sender<u64>,
    /// Test-only: fail every `record` as if the durable write failed, so the
    /// custody owners' journal-failure paths are exercised deterministically.
    #[cfg(any(test, feature = "test-helpers"))]
    fail_writes: std::sync::atomic::AtomicBool,
    /// Test-only: park every `record` until released, so a test can place a
    /// concurrent operation exactly inside a journal write.
    #[cfg(any(test, feature = "test-helpers"))]
    hold_writes: watch::Sender<bool>,
    #[cfg(any(test, feature = "test-helpers"))]
    held_writers: std::sync::atomic::AtomicUsize,
}

impl EventJournal {
    pub async fn open(path: impl Into<PathBuf>) -> Result<Self> {
        let path = path.into();
        let state = match tokio::fs::read(&path).await {
            Ok(bytes) => serde_json::from_slice::<JournalState>(&bytes)
                .with_context(|| format!("decode host event journal {}", path.display()))?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => JournalState {
                last_seq: HashMap::new(),
                events: Vec::new(),
            },
            Err(error) => {
                return Err(error)
                    .with_context(|| format!("read host event journal {}", path.display()));
            }
        };
        state.validate()?;
        let (changed, _) = watch::channel(0);
        Ok(Self {
            path,
            state: Mutex::new(state),
            changed,
            #[cfg(any(test, feature = "test-helpers"))]
            fail_writes: std::sync::atomic::AtomicBool::new(false),
            #[cfg(any(test, feature = "test-helpers"))]
            hold_writes: watch::channel(false).0,
            #[cfg(any(test, feature = "test-helpers"))]
            held_writers: std::sync::atomic::AtomicUsize::new(0),
        })
    }

    /// Test-only: park subsequent `record` calls (`true`) or release them.
    #[cfg(any(test, feature = "test-helpers"))]
    pub fn hold_writes(&self, hold: bool) {
        self.hold_writes.send_replace(hold);
    }

    /// Test-only: how many `record` calls are parked right now.
    #[cfg(any(test, feature = "test-helpers"))]
    pub fn held_writers(&self) -> usize {
        self.held_writers.load(std::sync::atomic::Ordering::Acquire)
    }

    /// Test-only: make every subsequent `record` fail.
    #[cfg(any(test, feature = "test-helpers"))]
    pub fn fail_writes(&self, fail: bool) {
        self.fail_writes
            .store(fail, std::sync::atomic::Ordering::Release);
    }

    pub async fn record(
        &self,
        workspace_id: Uuid,
        vm_id: Uuid,
        kind: VmEventKind,
        log_tail: Option<String>,
    ) -> Result<VmEvent> {
        Ok(self
            .record_batch(workspace_id, vm_id, vec![(kind, log_tail)])
            .await?
            .remove(0))
    }

    /// Append related facts in one durable write. A failed boot's diagnostics
    /// and proven exit must survive restart together, with distinct sequence
    /// identities understood by older control planes.
    pub(crate) async fn record_batch(
        &self,
        workspace_id: Uuid,
        vm_id: Uuid,
        facts: Vec<(VmEventKind, Option<String>)>,
    ) -> Result<Vec<VmEvent>> {
        // Test-only parking (G5-Lean#626): a test can place a concurrent
        // operation exactly inside a journal write; batch and single-event
        // records park alike.
        #[cfg(any(test, feature = "test-helpers"))]
        {
            let mut held = self.hold_writes.subscribe();
            if *held.borrow_and_update() {
                self.held_writers
                    .fetch_add(1, std::sync::atomic::Ordering::AcqRel);
                while *held.borrow_and_update() {
                    if held.changed().await.is_err() {
                        break;
                    }
                }
                self.held_writers
                    .fetch_sub(1, std::sync::atomic::Ordering::AcqRel);
            }
        }
        let mut state = self.state.lock().await;
        let mut next = state.clone();
        let mut events = Vec::with_capacity(facts.len());
        for (kind, log_tail) in facts {
            let seq = next
                .last_seq
                .get(&vm_id)
                .copied()
                .unwrap_or_default()
                .checked_add(1)
                .ok_or_else(|| anyhow!("event sequence exhausted for vm {vm_id}"))?;
            let event = VmEvent {
                workspace_id,
                vm_id,
                seq,
                kind,
                at: Utc::now(),
                log_tail,
            };
            event.validate().map_err(anyhow::Error::msg)?;
            next.last_seq.insert(vm_id, seq);
            next.events.push(event.clone());
            events.push(event);
        }
        #[cfg(any(test, feature = "test-helpers"))]
        if self.fail_writes.load(std::sync::atomic::Ordering::Acquire) {
            return Err(anyhow!("event journal write failing (test)"));
        }
        let before = std::mem::replace(&mut *state, next);
        if let Err(error) = persist_state(&self.path, &state).await {
            *state = before;
            return Err(error);
        }
        drop(state);
        self.changed
            .send_modify(|version| *version = version.wrapping_add(1));
        Ok(events)
    }

    pub async fn pending(&self) -> Vec<VmEvent> {
        self.state.lock().await.events.clone()
    }

    pub async fn acknowledge(&self, ack: &VmEventAck) -> Result<()> {
        let mut state = self.state.lock().await;
        let before = state.clone();
        state
            .events
            .retain(|event| event.vm_id != ack.vm_id || event.seq > ack.through_seq);
        if state.events.len() == before.events.len() {
            return Ok(());
        }
        if let Err(error) = persist_state(&self.path, &state).await {
            *state = before;
            return Err(error);
        }
        Ok(())
    }

    fn subscribe(&self) -> watch::Receiver<u64> {
        self.changed.subscribe()
    }
}

async fn persist_state(path: &Path, state: &JournalState) -> Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| anyhow!("host event journal path has no parent: {}", path.display()))?;
    tokio::fs::create_dir_all(parent)
        .await
        .with_context(|| format!("create host event journal directory {}", parent.display()))?;
    let bytes = serde_json::to_vec(state).context("encode host event journal")?;
    let temp = path.with_extension(format!("tmp-{}", Uuid::new_v4()));
    let write_result = async {
        use tokio::io::AsyncWriteExt;

        let mut file = tokio::fs::File::create(&temp)
            .await
            .with_context(|| format!("create host event journal temp {}", temp.display()))?;
        file.write_all(&bytes)
            .await
            .with_context(|| format!("write host event journal temp {}", temp.display()))?;
        file.sync_all()
            .await
            .with_context(|| format!("sync host event journal temp {}", temp.display()))?;
        tokio::fs::rename(&temp, path).await.with_context(|| {
            format!(
                "replace host event journal {} with {}",
                path.display(),
                temp.display()
            )
        })?;
        // The file fsync protects its contents; syncing the directory protects
        // the rename itself across a host crash.
        let parent = parent.to_path_buf();
        tokio::task::spawn_blocking(move || {
            std::fs::File::open(&parent)
                .with_context(|| format!("open host event journal directory {}", parent.display()))?
                .sync_all()
                .with_context(|| format!("sync host event journal directory {}", parent.display()))
        })
        .await
        .context("join host event journal directory sync")??;
        Ok(())
    }
    .await;
    if write_result.is_err() {
        let _ = tokio::fs::remove_file(&temp).await;
    }
    write_result
}

#[derive(Clone)]
struct CachedResult {
    payload_hash: [u8; 32],
    result: CommandResult,
    completed_at: Instant,
}

struct CommandExecution {
    cache_key: (String, String),
    payload_hash: [u8; 32],
    _slot: OwnedSemaphorePermit,
}

type ExecutionGateMap = HashMap<(String, String), Weak<CommandExecution>>;

/// Host-owned command admission and idempotency state.
pub struct HostCommandProcessor {
    draining: AtomicBool,
    drain_id: Mutex<Option<Uuid>>,
    retained: Mutex<HashMap<(String, String), CachedResult>>,
    execution_gates: Mutex<ExecutionGateMap>,
    command_slots: Arc<Semaphore>,
    workspace_runtime: Option<Arc<crate::workspace_vm::WorkspaceVmRuntime>>,
}

impl Default for HostCommandProcessor {
    fn default() -> Self {
        Self {
            draining: AtomicBool::new(false),
            drain_id: Mutex::new(None),
            retained: Mutex::new(HashMap::new()),
            execution_gates: Mutex::new(HashMap::new()),
            command_slots: Arc::new(Semaphore::new(MAX_INFLIGHT_COMMANDS)),
            workspace_runtime: None,
        }
    }
}

impl HostCommandProcessor {
    #[must_use]
    pub fn with_workspace_runtime(
        mut self,
        runtime: Arc<crate::workspace_vm::WorkspaceVmRuntime>,
    ) -> Self {
        self.workspace_runtime = Some(runtime);
        self
    }

    #[must_use]
    pub fn is_draining(&self) -> bool {
        self.draining.load(Ordering::Acquire)
    }

    pub async fn drain_id(&self) -> Option<Uuid> {
        *self.drain_id.lock().await
    }

    pub async fn self_drain(&self) {
        self.draining.store(true, Ordering::Release);
    }

    /// Register a `vm.start`'s incarnation with the runtime the moment the
    /// session dequeues it, before any later command on the connection can be
    /// evaluated (`g5.runtime.start.settlement`). Other methods, and a Start
    /// without an installed runtime or with undecodable parameters, register
    /// nothing; `execute` reports those.
    pub async fn admit_request(
        &self,
        request: &RpcRequest,
    ) -> Option<crate::workspace_vm::PendingStartGuard> {
        if request.method != method::VM_START {
            return None;
        }
        let runtime = self.workspace_runtime.as_ref()?;
        let command = request.params::<CommandParams<VmStart>>().ok()?;
        Some(runtime.register_start(&command.command).await)
    }

    /// Deduplicate before allocating execution capacity. Active retries are
    /// transient Busy responses, never waiting tasks; changed payloads conflict
    /// even before the original execution has completed. The permit belongs to
    /// the processor, so reconnect cannot multiply the host's command budget.
    async fn admit_execution(
        &self,
        request: &RpcRequest,
        queued_at: Instant,
    ) -> Result<std::result::Result<Arc<CommandExecution>, CommandResult>> {
        let (idempotency_key, budget_ms) = command_metadata(&request.params)?;
        if queued_at.elapsed() > Duration::from_millis(budget_ms) {
            return Ok(Err(CommandResult::Expired));
        }
        let payload_hash: [u8; 32] = Sha256::digest(
            serde_json::to_vec(&request.params)
                .context("encode command payload for idempotency")?,
        )
        .into();
        let cache_key = (request.method.clone(), idempotency_key);
        let mut gates = self.execution_gates.lock().await;
        gates.retain(|_, execution| execution.strong_count() != 0);
        if let Some(active) = gates.get(&cache_key).and_then(Weak::upgrade) {
            return Ok(Err(if active.payload_hash == payload_hash {
                CommandResult::Busy
            } else {
                CommandResult::IdempotencyConflict
            }));
        }
        let Ok(slot) = self.command_slots.clone().try_acquire_owned() else {
            return Ok(Err(CommandResult::Busy));
        };
        let execution = Arc::new(CommandExecution {
            cache_key: cache_key.clone(),
            payload_hash,
            _slot: slot,
        });
        gates.insert(cache_key, Arc::downgrade(&execution));
        Ok(Ok(execution))
    }

    pub async fn execute(
        &self,
        request: &RpcRequest,
        queued_at: Instant,
        retention: Duration,
    ) -> Result<CommandResult> {
        match self.admit_execution(request, queued_at).await? {
            Ok(execution) => {
                self.execute_admitted(request, queued_at, retention, execution)
                    .await
            }
            Err(result) => Ok(result),
        }
    }

    async fn execute_admitted(
        &self,
        request: &RpcRequest,
        queued_at: Instant,
        retention: Duration,
        execution: Arc<CommandExecution>,
    ) -> Result<CommandResult> {
        let (_, budget_ms) = command_metadata(&request.params)?;
        let cache_key = &execution.cache_key;
        let payload_hash = execution.payload_hash;
        let mut retained = self.retained.lock().await;
        retained.retain(|_, entry| entry.completed_at.elapsed() <= retention);
        if let Some(previous) = retained.get(cache_key) {
            return Ok(if previous.payload_hash == payload_hash {
                previous.result.clone()
            } else {
                CommandResult::IdempotencyConflict
            });
        }

        let result = if queued_at.elapsed() > Duration::from_millis(budget_ms) {
            CommandResult::Expired
        } else {
            match request.method.as_str() {
                method::HOST_DRAIN => {
                    let command = request.params::<CommandParams<HostDrain>>()?;
                    command.validate().map_err(anyhow::Error::msg)?;
                    self.draining.store(true, Ordering::Release);
                    *self.drain_id.lock().await = Some(command.command.drain_id);
                    CommandResult::Accepted
                }
                method::VM_START if self.is_draining() => CommandResult::Draining,
                method::VM_START => {
                    let command = request.params::<CommandParams<VmStart>>()?;
                    command.validate().map_err(anyhow::Error::msg)?;
                    let Some(runtime) = &self.workspace_runtime else {
                        return Err(anyhow!("workspace VM runtime is not installed"));
                    };
                    drop(retained);
                    let result = runtime.start(command.command).await;
                    retained = self.retained.lock().await;
                    result
                }
                method::VM_STOP => {
                    let command = request.params::<CommandParams<VmStop>>()?;
                    command.validate().map_err(anyhow::Error::msg)?;
                    let Some(runtime) = &self.workspace_runtime else {
                        return Err(anyhow!("workspace VM runtime is not installed"));
                    };
                    drop(retained);
                    let result = runtime.stop(command.command).await;
                    retained = self.retained.lock().await;
                    result
                }
                method::VM_STATUS => {
                    let command = request.params::<CommandParams<VmStatus>>()?;
                    command.validate().map_err(anyhow::Error::msg)?;
                    let Some(runtime) = &self.workspace_runtime else {
                        return Err(anyhow!("workspace VM runtime is not installed"));
                    };
                    drop(retained);
                    let result = runtime.status(command.command).await;
                    retained = self.retained.lock().await;
                    result
                }
                method::ROOTS_PURGE => {
                    // Roots are deleted when their VM is released, so nothing
                    // is left to purge; an older control plane counts
                    // `not_found` as purged.
                    let command = request.params::<CommandParams<RootsPurge>>()?;
                    command.validate().map_err(anyhow::Error::msg)?;
                    CommandResult::NotFound
                }
                command => CommandResult::Unsupported {
                    command: command.to_string(),
                },
            }
        };
        retained.insert(
            cache_key.clone(),
            CachedResult {
                payload_hash,
                result: result.clone(),
                completed_at: Instant::now(),
            },
        );
        Ok(result)
    }
}

fn command_metadata(params: &Value) -> Result<(String, u64)> {
    let key = params
        .get("idempotency_key")
        .and_then(Value::as_str)
        .filter(|key| !key.is_empty() && key.len() <= 256)
        .ok_or_else(|| anyhow!("invalid idempotency key"))?;
    let budget_ms = params
        .get("budget_ms")
        .and_then(Value::as_u64)
        .filter(|budget| *budget > 0)
        .ok_or_else(|| anyhow!("invalid command budget"))?;
    Ok((key.to_string(), budget_ms))
}

#[async_trait]
pub trait HostControlState: Send + Sync {
    async fn hello(&self) -> HostHello;
    async fn heartbeat(&self) -> HostHeartbeat;
    async fn active_vm_count(&self) -> usize;
    async fn reconcile_current_incarnations(&self, generation: u64, current: &[CurrentIncarnation]);
}

pub(crate) struct PendingLeaseRenewal {
    pub(crate) renewal: LeaseRenew,
    pub(crate) response: oneshot::Sender<Result<LeaseRenewResult, String>>,
}

/// Bridges a dispatcher's private `lease.renew` request onto the host ctl
/// stream. The broker survives tunnel reconnects; a caller times out rather
/// than treating a disconnected control plane as a successful renewal.
pub struct LeaseRenewalBroker {
    sender: mpsc::Sender<PendingLeaseRenewal>,
    receiver: Mutex<mpsc::Receiver<PendingLeaseRenewal>>,
    file_sender: mpsc::Sender<VmFileChanged>,
    file_receiver: Mutex<mpsc::Receiver<VmFileChanged>>,
    file_overflow: Mutex<HashMap<(Uuid, Uuid, Uuid), VmFileChanged>>,
    /// Executions with new module events per bound incarnation, coalesced
    /// until the ctl session drains them (`g5.runtime.module-event-hints`).
    /// Keyed by (workspace, VM, epoch) of the dispatcher connection.
    hint_pending: Mutex<HashMap<(Uuid, Uuid, u64), Vec<String>>>,
    hint_wake: tokio::sync::Notify,
}

/// How long a drained hint waits for more events of the same VM to join it,
/// so a stream of deltas costs at most twenty hints a second.
const MODULE_EVENT_HINT_COALESCE: Duration = Duration::from_millis(50);

impl Default for LeaseRenewalBroker {
    fn default() -> Self {
        let (sender, receiver) = mpsc::channel(MAX_INFLIGHT_COMMANDS * 2);
        let (file_sender, file_receiver) = mpsc::channel(256);
        Self {
            sender,
            receiver: Mutex::new(receiver),
            file_sender,
            file_receiver: Mutex::new(file_receiver),
            file_overflow: Mutex::new(HashMap::new()),
            hint_pending: Mutex::new(HashMap::new()),
            hint_wake: tokio::sync::Notify::new(),
        }
    }
}

impl LeaseRenewalBroker {
    pub async fn renew(&self, renewal: LeaseRenew) -> Result<LeaseRenewResult> {
        let (response, reply) = oneshot::channel();
        self.sender
            .send(PendingLeaseRenewal { renewal, response })
            .await
            .map_err(|_| anyhow!("host ctl lease-renewal bridge closed"))?;
        let result = tokio::time::timeout(Duration::from_secs(30), reply)
            .await
            .map_err(|_| anyhow!("host ctl lease-renewal bridge timed out"))?
            .map_err(|_| anyhow!("host ctl lease-renewal response dropped"))?;
        result.map_err(anyhow::Error::msg)
    }

    pub(crate) async fn next(&self) -> Option<PendingLeaseRenewal> {
        self.receiver.lock().await.recv().await
    }

    async fn publish_file_change(&self, event: VmFileChanged) {
        if self.file_sender.try_send(event.clone()).is_ok() {
            return;
        }
        self.file_overflow.lock().await.insert(
            (event.workspace_id, event.vm_id, event.repo_id),
            VmFileChanged {
                path: String::new(),
                rescan_required: true,
                ..event
            },
        );
    }

    async fn next_file_change(&self) -> Option<VmFileChanged> {
        let pending = {
            let mut overflow = self.file_overflow.lock().await;
            let key = overflow.keys().next().copied();
            key.and_then(|key| overflow.remove(&key))
        };
        match pending {
            Some(event) => Some(event),
            None => self.file_receiver.lock().await.recv().await,
        }
    }

    /// Note that `exec_id` on the bound incarnation journaled a module
    /// event. Identity comes from the connection, never from the frame.
    /// Beyond the hint's capacity an execution is dropped from this hint,
    /// which only delays the control plane's fetch to its poll.
    pub(crate) async fn publish_module_hint(
        &self,
        workspace_id: Uuid,
        vm_id: Uuid,
        epoch: u64,
        exec_id: &str,
    ) {
        let mut pending = self.hint_pending.lock().await;
        let ids = pending.entry((workspace_id, vm_id, epoch)).or_default();
        if !ids.iter().any(|known| known == exec_id)
            && ids.len() < g5_internal_api::host_ctl::MAX_MODULE_EVENT_HINT_EXECUTIONS
        {
            ids.push(exec_id.to_string());
        }
        drop(pending);
        self.hint_wake.notify_one();
    }

    /// The next coalesced hint: waits for an execution to be noted, lets a
    /// short window of further events join it, then takes one incarnation's
    /// set. Other incarnations' sets follow on the next call without a wait.
    pub(crate) async fn next_module_hint(&self) -> VmModuleEvents {
        loop {
            if self.hint_pending.lock().await.is_empty() {
                self.hint_wake.notified().await;
                tokio::time::sleep(MODULE_EVENT_HINT_COALESCE).await;
            }
            let mut pending = self.hint_pending.lock().await;
            let key = pending.keys().next().copied();
            if let Some(key) = key {
                if let Some(exec_ids) = pending.remove(&key) {
                    if exec_ids.is_empty() {
                        continue;
                    }
                    return VmModuleEvents {
                        workspace_id: key.0,
                        vm_id: key.1,
                        epoch: key.2,
                        exec_ids,
                    };
                }
            }
        }
    }
}

/// JSON-RPC error returned to a dispatcher whose `lease.renew` names an
/// incarnation other than the one its connection is bound to. The code and
/// message are the dispatcher's own for an incarnation mismatch.
pub(crate) const STALE_INCARNATION_CODE: i64 = 409;
pub(crate) const STALE_INCARNATION_MESSAGE: &str = "STALE_INCARNATION";

/// A renewal may only name the incarnation the dispatcher connection is
/// bound to. The guest chooses the frame's tuple, so a renewal relayed on
/// its own say-so would let one VM renew, and obtain the workspace token
/// of, any sibling on the host (`g5.runtime.lease`).
fn lease_renewal_is_bound(
    renewal: &LeaseRenew,
    workspace_id: Uuid,
    vm_id: Uuid,
    epoch: u64,
) -> bool {
    renewal.workspace_id == workspace_id && renewal.vm_id == vm_id && renewal.epoch == epoch
}

/// Consume dispatcher control traffic that must never escape onto an API
/// method-reply stream. Returns `true` for a handled ready/renewal frame.
///
/// `workspace_id`, `vm_id` and `epoch` are the incarnation the dispatcher
/// connection is bound to: the `VmStart` command on the host's own control
/// connection, the live runtime record on a control-plane relay. They, not
/// the frame, identify the VM for `file.changed`, and a `lease.renew` naming
/// any other incarnation is refused with `STALE_INCARNATION` before it can
/// reach the control plane.
pub async fn handle_dispatcher_control_message(
    line: &str,
    dispatcher_writer: &Mutex<MuxWriter>,
    renewals: &LeaseRenewalBroker,
    _journal: &EventJournal,
    workspace_id: Uuid,
    vm_id: Uuid,
    epoch: u64,
) -> bool {
    let Ok(value) = serde_json::from_str::<Value>(line) else {
        return false;
    };
    if value.get("method").and_then(Value::as_str) == Some("runtime.ready") {
        return true;
    }
    match value.get("method").and_then(Value::as_str) {
        // The payload is never forwarded: the control plane fetches the
        // journal, which stays the source and ordering authority. The hint
        // names the bound incarnation, whatever the frame claims.
        Some("module.event") => {
            if let Some(exec_id) = value["params"]["exec_id"].as_str() {
                renewals
                    .publish_module_hint(workspace_id, vm_id, epoch, exec_id)
                    .await;
            }
            return true;
        }
        // A lagged live stream is irrelevant to a consumer that replays
        // from its watermark; consume it so it is not forwarded or logged.
        Some("module.events.lagged") => return true,
        _ => {}
    }
    if value.get("method").and_then(Value::as_str) == Some("file.changed") {
        let params = value.get("params").cloned().unwrap_or(Value::Null);
        let Some(repo_id) = params["repo_id"]
            .as_str()
            .and_then(|value| Uuid::parse_str(value).ok())
        else {
            return true;
        };
        let Some(path) = params["path"].as_str() else {
            return true;
        };
        let event = VmFileChanged {
            workspace_id,
            vm_id,
            repo_id,
            path: path.to_string(),
            rescan_required: params["rescan_required"].as_bool().unwrap_or(false),
        };
        renewals.publish_file_change(event).await;
        return true;
    }
    if value.get("method").and_then(Value::as_str) != Some(method::LEASE_RENEW) {
        return false;
    }
    let id = value.get("id").cloned().unwrap_or(Value::Null);
    let response = match value
        .get("params")
        .cloned()
        .ok_or_else(|| "missing lease renewal params".to_string())
        .and_then(|params| serde_json::from_value::<LeaseRenew>(params).map_err(|e| e.to_string()))
    {
        Ok(renewal) if !lease_renewal_is_bound(&renewal, workspace_id, vm_id, epoch) => {
            tracing::warn!(
                %workspace_id,
                %vm_id,
                epoch,
                claimed_workspace_id = %renewal.workspace_id,
                claimed_vm_id = %renewal.vm_id,
                claimed_epoch = renewal.epoch,
                "refused a dispatcher lease renewal for another incarnation"
            );
            serde_json::json!({"jsonrpc":"2.0","id":id,"error":{"code":STALE_INCARNATION_CODE,"message":STALE_INCARNATION_MESSAGE}})
        }
        Ok(renewal) => match renewals.renew(renewal).await {
            Ok(result) => serde_json::json!({"jsonrpc":"2.0","id":id,"result":result}),
            Err(error) => {
                serde_json::json!({"jsonrpc":"2.0","id":id,"error":{"code":503,"message":error.to_string()}})
            }
        },
        Err(error) => {
            serde_json::json!({"jsonrpc":"2.0","id":id,"error":{"code":-32602,"message":error}})
        }
    };
    let mut encoded = match serde_json::to_vec(&response) {
        Ok(encoded) => encoded,
        Err(_) => return true,
    };
    encoded.push(b'\n');
    let _ = dispatcher_writer.lock().await.send(&encoded).await;
    true
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionOutcome {
    Disconnected,
    Rejected(HostReject),
}

/// A control session must not leave its read pump detached after any writer,
/// protocol, or journal failure returns from the session.
struct AbortOnDrop(JoinHandle<()>);

impl Drop for AbortOnDrop {
    fn drop(&mut self) {
        self.0.abort();
    }
}

/// Run one negotiated control-stream lifetime. Unacknowledged VM events are
/// replayed immediately after acceptance, before heartbeat or other traffic.
pub async fn serve_control_stream(
    stream: MuxStream,
    state: Arc<dyn HostControlState>,
    journal: Arc<EventJournal>,
    commands: Arc<HostCommandProcessor>,
    renewals: Arc<LeaseRenewalBroker>,
) -> Result<SessionOutcome> {
    let (writer, mut reader) = stream.into_split();
    let hello = state.hello().await;
    if hello.protocol_version != PROTOCOL_VERSION {
        return Err(anyhow!(
            "host state supplied protocol version {}, expected {}",
            hello.protocol_version,
            PROTOCOL_VERSION
        ));
    }
    send_request(
        &writer,
        &RpcRequest::new("hello", method::HOST_HELLO, &hello)?,
    )
    .await?;
    let response = read_response(&mut reader).await?;
    let disposition = response.result_as::<HelloDisposition>()?;
    let accepted = match disposition {
        HelloDisposition::Accept(accept) if accept.accepted => accept,
        HelloDisposition::Reject(reject) if !reject.accepted => {
            commands.self_drain().await;
            return Ok(SessionOutcome::Rejected(reject));
        }
        _ => return Err(anyhow!("invalid host hello disposition")),
    };
    if accepted.protocol_version != PROTOCOL_VERSION {
        commands.self_drain().await;
        return Err(anyhow!(
            "control plane selected unsupported protocol version {}",
            accepted.protocol_version
        ));
    }
    state
        .reconcile_current_incarnations(
            accepted.incarnation_generation,
            &accepted.current_incarnations,
        )
        .await;

    let (incoming_tx, mut incoming_rx) = mpsc::channel(MAX_INFLIGHT_COMMANDS * 2);
    let _reader_task = AbortOnDrop(tokio::spawn(async move {
        while let Some(line) = reader.read_line_with_limit(MAX_CTL_MESSAGE_BYTES).await {
            let received_at = Instant::now();
            let message = serde_json::from_str::<RpcMessage>(&line)
                .context("decode control-plane ctl message");
            if incoming_tx.send((received_at, message)).await.is_err() {
                return;
            }
        }
    }));

    let mut sent_events = HashSet::new();
    // Subscribe before the initial snapshot. An append racing with replay then
    // either appears in that snapshot or leaves the watch receiver changed.
    let mut journal_changed = journal.subscribe();
    replay_events(&writer, &journal, &mut sent_events).await?;
    let heartbeat_secs = accepted.config.heartbeat_secs.max(1);
    let retention = Duration::from_secs(accepted.config.result_retention_secs.max(1));
    let mut heartbeat = tokio::time::interval(Duration::from_secs(heartbeat_secs));
    heartbeat.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    heartbeat.tick().await;
    let mut heartbeat_seq = 0_u64;
    let mut next_source_refresh = Instant::now();
    let mut source_refresh_id = None::<String>;
    let mut renewal_seq = 0_u64;
    let mut pending_renewals =
        HashMap::<String, oneshot::Sender<Result<LeaseRenewResult, String>>>::new();
    let mut drained_reported = false;
    let (result_tx, mut result_rx) = mpsc::channel(MAX_INFLIGHT_COMMANDS);

    loop {
        tokio::select! {
            incoming = incoming_rx.recv() => {
                let Some((received_at, message)) = incoming else {
                    return Ok(SessionOutcome::Disconnected);
                };
                let message = message?;
                match message {
                    RpcMessage::Request(request) => {
                        let execution = match commands.admit_execution(&request, received_at).await? {
                            Ok(execution) => execution,
                            Err(result) => {
                                send_response(&writer, &RpcResponse::result(request.id, &result)?).await?;
                                continue;
                            }
                        };
                        let result_tx = result_tx.clone();
                        let commands = commands.clone();
                        // Registered here, in dequeue order, so a status or
                        // stop dequeued later always finds the Start.
                        let pending_start = commands.admit_request(&request).await;
                        tokio::spawn(async move {
                            let id = request.id.clone();
                            // Keep the admission through result delivery as well:
                            // a stalled session must not accumulate unbounded
                            // completed tasks waiting on its bounded channel.
                            let result = commands.execute_admitted(&request, received_at, retention, execution.clone()).await;
                            if let Some(pending_start) = pending_start {
                                pending_start.settle().await;
                            }
                            let _ = result_tx.send((id, result)).await;
                            drop(execution);
                        });
                    }
                    RpcMessage::Notification(notification)
                        if notification.method == method::VM_EVENT_ACK =>
                    {
                        journal.acknowledge(&notification.params::<VmEventAck>()?).await?;
                    }
                    RpcMessage::Response(response)
                        if response.id.starts_with("heartbeat:") =>
                    {
                        let ack = response.result_as::<HeartbeatAck>()?;
                        if source_refresh_id.as_deref() == Some(response.id.as_str()) {
                            source_refresh_id = None;
                            if let Some(runtime) = commands.workspace_runtime.clone() {
                                let assignment = ack.source_preparation.clone().filter(|a| a.host_id == hello.host_id);
                                tokio::spawn(async move {
                                    if let Some(assignment) = assignment { runtime.prepare_sources(assignment).await; }
                                    else { runtime.cancel_source_preparation().await; }
                                });
                            }
                        }
                        // Off the control stream: a stale process-mode
                        // incarnation whose exit is not yet proven retries its
                        // stop until proof, and heartbeats must keep flowing
                        // meanwhile. The state serializes rosters by generation.
                        let state = state.clone();
                        tokio::spawn(async move {
                            state
                                .reconcile_current_incarnations(
                                    ack.incarnation_generation,
                                    &ack.current_incarnations,
                                )
                                .await;
                        });
                    }
                    RpcMessage::Response(response)
                        if response.id.starts_with("lease:") =>
                    {
                        if let Some(reply) = pending_renewals.remove(&response.id) {
                            let result = response
                                .result_as::<LeaseRenewResult>()
                                .map_err(|error| error.to_string());
                            let _ = reply.send(result);
                        }
                    }
                    _ => return Err(anyhow!("unexpected ctl message")),
                }
            }
            renewal = renewals.next() => {
                let Some(renewal) = renewal else {
                    return Err(anyhow!("lease-renewal bridge closed"));
                };
                renewal_seq = renewal_seq.wrapping_add(1);
                let id = format!("lease:{renewal_seq}");
                let request = RpcRequest::new(id.clone(), method::LEASE_RENEW, &renewal.renewal)?;
                pending_renewals.insert(id, renewal.response);
                send_request(&writer, &request).await?;
            }
            file_change = renewals.next_file_change() => {
                let Some(file_change) = file_change else {
                    return Err(anyhow!("file-change bridge closed"));
                };
                send_notification(
                    &writer,
                    &RpcNotification::new(method::VM_FILE_CHANGED, &file_change)?,
                )
                .await?;
            }
            hint = renewals.next_module_hint() => {
                // Only a control plane that enabled hints in its accept
                // understands the notification; otherwise the hint is
                // drained and dropped (`g5.runtime.module-event-hints`).
                if accepted.config.module_event_hints_enabled && hint.validate().is_ok() {
                    send_notification(
                        &writer,
                        &RpcNotification::new(method::VM_MODULE_EVENTS, &hint)?,
                    )
                    .await?;
                }
            }
            completed = result_rx.recv() => {
                let Some((id, result)) = completed else {
                    return Err(anyhow!("host command result channel closed"));
                };
                send_response(&writer, &RpcResponse::result(id, &result?)?).await?;
            }
            changed = journal_changed.changed() => {
                changed.map_err(|_| anyhow!("event journal notifier closed"))?;
                replay_events(&writer, &journal, &mut sent_events).await?;
            }
            _ = heartbeat.tick() => {
                heartbeat_seq = heartbeat_seq.wrapping_add(1);
                let id = format!("heartbeat:{heartbeat_seq}");
                let mut report = state.heartbeat().await;
                if accepted.config.source_preparation_enabled && Instant::now() >= next_source_refresh {
                    report.refresh_source_preparation = true;
                    source_refresh_id = Some(id.clone());
                    next_source_refresh = Instant::now() + Duration::from_secs(60);
                }
                let request = RpcRequest::new(id, method::HOST_HEARTBEAT, &report)?;
                send_request(&writer, &request).await?;
            }
        }

        if commands.is_draining() && !drained_reported && state.active_vm_count().await == 0 {
            if let Some(drain_id) = commands.drain_id().await {
                send_notification(
                    &writer,
                    &RpcNotification::new(method::HOST_DRAINED, &HostDrained { drain_id })?,
                )
                .await?;
                drained_reported = true;
            }
        }
    }
}

async fn replay_events(
    writer: &MuxWriter,
    journal: &EventJournal,
    sent: &mut HashSet<(Uuid, u64)>,
) -> Result<()> {
    for event in journal.pending().await {
        if sent.insert((event.vm_id, event.seq)) {
            send_notification(writer, &RpcNotification::new(method::VM_EVENT, &event)?).await?;
        }
    }
    Ok(())
}

async fn read_response(reader: &mut MuxReader) -> Result<RpcResponse> {
    let line = reader
        .read_line_with_limit(MAX_CTL_MESSAGE_BYTES)
        .await
        .ok_or_else(|| anyhow!("ctl stream closed during negotiation"))?;
    match serde_json::from_str::<RpcMessage>(&line).context("decode ctl negotiation response")? {
        RpcMessage::Response(response) => Ok(response),
        _ => Err(anyhow!("expected ctl negotiation response")),
    }
}

async fn send_request(writer: &MuxWriter, request: &RpcRequest) -> Result<()> {
    send_message(writer, &RpcMessage::Request(request.clone())).await
}

async fn send_response(writer: &MuxWriter, response: &RpcResponse) -> Result<()> {
    send_message(writer, &RpcMessage::Response(response.clone())).await
}

async fn send_notification(writer: &MuxWriter, notification: &RpcNotification) -> Result<()> {
    send_message(writer, &RpcMessage::Notification(notification.clone())).await
}

async fn send_message(writer: &MuxWriter, message: &RpcMessage) -> Result<()> {
    let mut encoded = serde_json::to_vec(message).context("encode ctl message")?;
    encoded.push(b'\n');
    writer.send(&encoded).await.map_err(anyhow::Error::msg)
}

pub(crate) fn disk_free_mib(path: &Path) -> u64 {
    #[cfg(unix)]
    {
        use std::ffi::CString;
        use std::os::unix::ffi::OsStrExt;

        let Ok(path) = CString::new(path.as_os_str().as_bytes()) else {
            return 0;
        };
        // SAFETY: `stat` is initialized before the call and read only when
        // `statvfs` reports success for the NUL-terminated path.
        let mut stat: libc::statvfs = unsafe { std::mem::zeroed() };
        if unsafe { libc::statvfs(path.as_ptr(), &mut stat) } != 0 {
            return 0;
        }
        #[allow(clippy::unnecessary_cast)]
        let unit = if stat.f_frsize != 0 {
            stat.f_frsize as u64
        } else {
            stat.f_bsize as u64
        };
        #[allow(clippy::unnecessary_cast)]
        return (stat.f_bavail as u64).saturating_mul(unit) / (1024 * 1024);
    }
    #[cfg(not(unix))]
    {
        let _ = path;
        0
    }
}

pub(crate) fn system_load() -> f64 {
    #[cfg(unix)]
    {
        let mut load = [0.0_f64; 1];
        // SAFETY: `load` points to one writable f64 and the count is one.
        if unsafe { libc::getloadavg(load.as_mut_ptr(), 1) } == 1 {
            return load[0];
        }
    }
    0.0
}

pub(crate) fn memory_free_mib() -> u64 {
    #[cfg(target_os = "linux")]
    {
        if let Ok(contents) = std::fs::read_to_string("/proc/meminfo") {
            if let Some(kib) = contents.lines().find_map(|line| {
                let value = line.strip_prefix("MemAvailable:")?;
                value.split_whitespace().next()?.parse::<u64>().ok()
            }) {
                return kib / 1024;
            }
        }
    }
    0
}

#[cfg(test)]
mod tests {
    use super::*;
    use g5_internal_api::host_ctl::{
        HostAccept, HostCtlConfig, HostRejectReason, HostVmState, LeaseGrant, RpcError,
        SUPPORTED_PROTOCOL_VERSIONS,
    };
    use g5_internal_api::stream_mux::{Channel, Mux};
    use tempfile::TempDir;

    struct EmptyState {
        host_id: Uuid,
        reconciliations: Mutex<Vec<(u64, Vec<CurrentIncarnation>)>>,
    }

    #[async_trait]
    impl HostControlState for EmptyState {
        async fn hello(&self) -> HostHello {
            HostHello {
                host_id: self.host_id,
                protocol_version: PROTOCOL_VERSION,
                capabilities: vec!["isolation:cloud-hypervisor".to_string()],
                slots_total: 2,
                slot_size: Some(g5_internal_api::host_ctl::VmSizeSpec {
                    cpus: 2,
                    mem_mib: 4096,
                    scratch_disk_mib: 10240,
                }),
                inventory: Vec::new(),
                retained_roots: Vec::new(),
                runtime: None,
            }
        }

        async fn heartbeat(&self) -> HostHeartbeat {
            HostHeartbeat {
                refresh_source_preparation: false,
                slots_in_use: 0,
                boots_in_progress: 0,
                load: 0.0,
                memory_free_mib: 1_024,
                disk_free_mib: 4_096,
                retained_roots: Vec::new(),
                vms: Vec::new(),
            }
        }

        async fn active_vm_count(&self) -> usize {
            0
        }

        async fn reconcile_current_incarnations(
            &self,
            generation: u64,
            current: &[CurrentIncarnation],
        ) {
            self.reconciliations
                .lock()
                .await
                .push((generation, current.to_vec()));
        }
    }

    async fn journal(temp: &TempDir) -> Arc<EventJournal> {
        Arc::new(
            EventJournal::open(temp.path().join("vm-events.json"))
                .await
                .unwrap(),
        )
    }

    #[tokio::test]
    async fn dispatcher_file_changes_use_the_bounded_ephemeral_bridge() {
        let temp = TempDir::new().unwrap();
        let journal = journal(&temp).await;
        let broker = LeaseRenewalBroker::default();
        let workspace_id = Uuid::new_v4();
        let vm_id = Uuid::new_v4();
        let repo_id = Uuid::new_v4();
        let (local_io, peer_io) = tokio::io::duplex(4096);
        let local = Mux::new(local_io, true);
        let mut peer = Mux::new(peer_io, false);
        let stream = local.open(Channel::Rpc).await.unwrap();
        let _accepted = peer.accept().await.unwrap();
        let writer = Mutex::new(stream.into_split().0);

        assert!(
            handle_dispatcher_control_message(
                &serde_json::json!({
                    "jsonrpc": "2.0",
                    "method": "file.changed",
                    "params": {
                        "repo_id": repo_id,
                        "path": "src/main.rs",
                        "rescan_required": false,
                    },
                })
                .to_string(),
                &writer,
                &broker,
                &journal,
                workspace_id,
                vm_id,
                1,
            )
            .await
        );
        assert!(journal.pending().await.is_empty());
        assert_eq!(
            broker.next_file_change().await,
            Some(VmFileChanged {
                workspace_id,
                vm_id,
                repo_id,
                path: "src/main.rs".into(),
                rescan_required: false,
            })
        );
    }

    /// A dispatcher RPC stream as the host sees it. Every half stays alive
    /// for the test's lifetime: a dropped half resets the stream and the
    /// guest would read EOF instead of the host's reply.
    struct DispatcherRpcPair {
        _host: Mux,
        _guest: Mux,
        _host_reader: MuxReader,
        _guest_writer: MuxWriter,
        host_writer: Mutex<MuxWriter>,
        guest_reader: MuxReader,
    }

    async fn dispatcher_rpc_pair() -> DispatcherRpcPair {
        let (local_io, peer_io) = tokio::io::duplex(4096);
        let host = Mux::new(local_io, true);
        let mut guest = Mux::new(peer_io, false);
        let stream = host.open(Channel::Rpc).await.unwrap();
        let accepted = guest.accept().await.unwrap();
        let (host_writer, host_reader) = stream.into_split();
        let (guest_writer, guest_reader) = accepted.into_split();
        DispatcherRpcPair {
            _host: host,
            _guest: guest,
            _host_reader: host_reader,
            _guest_writer: guest_writer,
            host_writer: Mutex::new(host_writer),
            guest_reader,
        }
    }

    fn lease_renew_frame(renewal: &LeaseRenew) -> String {
        serde_json::json!({
            "jsonrpc": "2.0",
            "id": format!("lease:{}", renewal.seq),
            "method": method::LEASE_RENEW,
            "params": renewal,
        })
        .to_string()
    }

    /// `g5.runtime.lease` ([G5Labs/G5-Lean#524]): a guest-authored renewal
    /// naming another incarnation on the host is refused on the stream and
    /// never reaches the host ctl bridge, whichever of workspace, VM or
    /// epoch it forges.
    #[tokio::test]
    async fn lease_renewals_for_another_incarnation_are_refused_before_the_bridge() {
        let temp = TempDir::new().unwrap();
        let journal = journal(&temp).await;
        let broker = LeaseRenewalBroker::default();
        let workspace_id = Uuid::new_v4();
        let vm_id = Uuid::new_v4();
        let epoch = 3;
        let mut pair = dispatcher_rpc_pair().await;
        let writer = &pair.host_writer;
        let guest = &mut pair.guest_reader;

        let forged = [
            (
                "workspace",
                LeaseRenew {
                    workspace_id: Uuid::new_v4(),
                    vm_id,
                    epoch,
                    seq: 7,
                },
            ),
            (
                "vm",
                LeaseRenew {
                    workspace_id,
                    vm_id: Uuid::new_v4(),
                    epoch,
                    seq: 7,
                },
            ),
            (
                "epoch",
                LeaseRenew {
                    workspace_id,
                    vm_id,
                    epoch: epoch + 1,
                    seq: 7,
                },
            ),
            (
                "stale epoch",
                LeaseRenew {
                    workspace_id,
                    vm_id,
                    epoch: epoch - 1,
                    seq: 7,
                },
            ),
            (
                "sibling tuple with a sequence jump",
                LeaseRenew {
                    workspace_id: Uuid::new_v4(),
                    vm_id: Uuid::new_v4(),
                    epoch,
                    seq: i64::MAX as u64 - 1,
                },
            ),
        ];
        for (forged_field, renewal) in forged {
            let handled = handle_dispatcher_control_message(
                &lease_renew_frame(&renewal),
                writer,
                &broker,
                &journal,
                workspace_id,
                vm_id,
                epoch,
            )
            .await;
            assert!(
                handled,
                "a refused lease.renew is still consumed ({forged_field})"
            );
            let reply: Value = serde_json::from_str(&guest.read_line().await.unwrap()).unwrap();
            assert_eq!(
                reply["id"],
                format!("lease:{}", renewal.seq),
                "{forged_field}"
            );
            assert_eq!(
                reply["error"]["code"], STALE_INCARNATION_CODE,
                "{forged_field}"
            );
            assert_eq!(
                reply["error"]["message"], STALE_INCARNATION_MESSAGE,
                "{forged_field}"
            );
            assert!(reply.get("result").is_none(), "{forged_field}");
            assert!(
                matches!(
                    broker.receiver.lock().await.try_recv(),
                    Err(mpsc::error::TryRecvError::Empty)
                ),
                "a forged {forged_field} never reaches the host ctl bridge"
            );
        }
        assert!(journal.pending().await.is_empty());
    }

    /// The authorized path: a renewal naming the incarnation the stream is
    /// bound to reaches the bridge unchanged and its grant is answered
    /// under the dispatcher's request ID.
    #[tokio::test]
    async fn lease_renewals_for_the_bound_incarnation_are_relayed_and_answered() {
        let temp = TempDir::new().unwrap();
        let journal = journal(&temp).await;
        let broker = Arc::new(LeaseRenewalBroker::default());
        let workspace_id = Uuid::new_v4();
        let vm_id = Uuid::new_v4();
        let epoch = 3;
        let mut pair = dispatcher_rpc_pair().await;
        let writer = &pair.host_writer;
        let guest = &mut pair.guest_reader;
        let renewal = LeaseRenew {
            workspace_id,
            vm_id,
            epoch,
            seq: 7,
        };

        let control_plane = tokio::spawn({
            let broker = broker.clone();
            let renewal = renewal.clone();
            async move {
                let pending = broker.next().await.expect("the renewal reaches the bridge");
                assert_eq!(pending.renewal, renewal);
                pending
                    .response
                    .send(Ok(LeaseRenewResult::Granted(LeaseGrant {
                        stale: false,
                        seq: 7,
                        token: "renewed-token".into(),
                        lease_ttl_secs: 300,
                        incarnation_generation: 1,
                    })))
                    .unwrap();
            }
        });
        assert!(
            handle_dispatcher_control_message(
                &lease_renew_frame(&renewal),
                writer,
                &broker,
                &journal,
                workspace_id,
                vm_id,
                epoch,
            )
            .await
        );
        control_plane.await.unwrap();
        let reply: Value = serde_json::from_str(&guest.read_line().await.unwrap()).unwrap();
        assert_eq!(reply["id"], "lease:7");
        assert!(reply.get("error").is_none());
        assert_eq!(reply["result"]["seq"], 7);
        assert_eq!(reply["result"]["token"], "renewed-token");
        assert_eq!(reply["result"]["lease_ttl_secs"], 300);
    }

    #[tokio::test]
    async fn saturated_file_change_bridge_coalesces_to_one_repo_rescan() {
        let broker = LeaseRenewalBroker::default();
        let workspace_id = Uuid::new_v4();
        let vm_id = Uuid::new_v4();
        let repo_id = Uuid::new_v4();
        for index in 0..256 {
            broker
                .publish_file_change(VmFileChanged {
                    workspace_id,
                    vm_id,
                    repo_id,
                    path: format!("src/{index}.rs"),
                    rescan_required: false,
                })
                .await;
        }
        broker
            .publish_file_change(VmFileChanged {
                workspace_id,
                vm_id,
                repo_id,
                path: "src/overflow.rs".into(),
                rescan_required: false,
            })
            .await;

        assert_eq!(
            broker.next_file_change().await,
            Some(VmFileChanged {
                workspace_id,
                vm_id,
                repo_id,
                path: String::new(),
                rescan_required: true,
            })
        );
    }

    #[tokio::test]
    async fn event_journal_survives_restart_and_ack_preserves_sequence() {
        let temp = TempDir::new().unwrap();
        let workspace_id = Uuid::new_v4();
        let vm_id = Uuid::new_v4();
        let first = journal(&temp).await;
        assert_eq!(
            first
                .record(workspace_id, vm_id, VmEventKind::Booted, None)
                .await
                .unwrap()
                .seq,
            1
        );
        drop(first);

        let reopened = journal(&temp).await;
        assert_eq!(reopened.pending().await.len(), 1);
        reopened
            .acknowledge(&VmEventAck {
                vm_id,
                through_seq: 1,
            })
            .await
            .unwrap();
        drop(reopened);

        let reopened = journal(&temp).await;
        assert!(reopened.pending().await.is_empty());
        assert_eq!(
            reopened
                .record(
                    workspace_id,
                    vm_id,
                    VmEventKind::Exited {
                        code: Some(0),
                        signal: None,
                    },
                    None,
                )
                .await
                .unwrap()
                .seq,
            2
        );
    }

    #[tokio::test]
    async fn cumulative_ack_never_drops_a_later_event() {
        let temp = TempDir::new().unwrap();
        let journal = journal(&temp).await;
        let workspace_id = Uuid::new_v4();
        let vm_id = Uuid::new_v4();
        journal
            .record(workspace_id, vm_id, VmEventKind::Booted, None)
            .await
            .unwrap();
        let second = journal
            .record(
                workspace_id,
                vm_id,
                VmEventKind::Exited {
                    code: Some(0),
                    signal: None,
                },
                None,
            )
            .await
            .unwrap();

        journal
            .acknowledge(&VmEventAck {
                vm_id,
                through_seq: 1,
            })
            .await
            .unwrap();
        assert_eq!(journal.pending().await, vec![second]);
    }

    #[tokio::test]
    async fn terminal_event_batch_is_atomic_and_survives_partial_ack() {
        let temp = TempDir::new().unwrap();
        let workspace_id = Uuid::new_v4();
        let vm_id = Uuid::new_v4();
        let first = journal(&temp).await;
        // Validation failure on the second fact must not consume a sequence
        // or leave the first fact visible in memory or on disk.
        assert!(first
            .record_batch(
                workspace_id,
                vm_id,
                vec![
                    (
                        VmEventKind::BootFailed {
                            reason: "failed".into()
                        },
                        None
                    ),
                    (
                        VmEventKind::Exited {
                            code: None,
                            signal: None
                        },
                        Some("x".repeat(g5_internal_api::host_ctl::MAX_LOG_TAIL_BYTES + 1))
                    ),
                ],
            )
            .await
            .is_err());
        assert!(first.pending().await.is_empty());
        assert!(journal(&temp).await.pending().await.is_empty());
        // Force the atomic rename to fail without relying on Unix permissions
        // (these tests can run as root). No fact or watermark may escape.
        tokio::fs::create_dir(&first.path).await.unwrap();
        assert!(first
            .record_batch(
                workspace_id,
                vm_id,
                vec![
                    (
                        VmEventKind::BootFailed {
                            reason: "failed".into()
                        },
                        None
                    ),
                    (
                        VmEventKind::Exited {
                            code: None,
                            signal: None
                        },
                        None
                    ),
                ],
            )
            .await
            .is_err());
        assert!(first.pending().await.is_empty());
        tokio::fs::remove_dir(&first.path).await.unwrap();
        let facts = first
            .record_batch(
                workspace_id,
                vm_id,
                vec![
                    (
                        VmEventKind::BootFailed {
                            reason: "failed".into(),
                        },
                        Some("diagnostics".into()),
                    ),
                    (
                        VmEventKind::Exited {
                            code: None,
                            signal: None,
                        },
                        None,
                    ),
                ],
            )
            .await
            .unwrap();
        assert_eq!(
            facts.iter().map(|event| event.seq).collect::<Vec<_>>(),
            vec![1, 2]
        );
        drop(first);
        let reopened = journal(&temp).await;
        assert_eq!(reopened.pending().await, facts);
        reopened
            .acknowledge(&VmEventAck {
                vm_id,
                through_seq: 1,
            })
            .await
            .unwrap();
        drop(reopened);
        let reopened = journal(&temp).await;
        assert_eq!(reopened.pending().await, vec![facts[1].clone()]);
        reopened
            .acknowledge(&VmEventAck {
                vm_id,
                through_seq: 2,
            })
            .await
            .unwrap();
        drop(reopened);
        let reopened = journal(&temp).await;
        assert_eq!(
            reopened
                .record(workspace_id, vm_id, VmEventKind::Booted, None)
                .await
                .unwrap()
                .seq,
            3
        );
    }

    #[tokio::test]
    async fn command_budget_and_idempotency_are_host_monotonic() {
        let processor = HostCommandProcessor::default();
        let drain_id = Uuid::new_v4();
        let request = RpcRequest::new(
            "one",
            method::HOST_DRAIN,
            &CommandParams {
                idempotency_key: "same-key".to_string(),
                budget_ms: 60_000,
                command: HostDrain { drain_id },
            },
        )
        .unwrap();
        let expired = processor
            .execute(
                &request,
                Instant::now() - Duration::from_secs(61),
                Duration::from_secs(600),
            )
            .await
            .unwrap();
        assert_eq!(expired, CommandResult::Expired);
        assert!(!processor.is_draining());

        let replay = processor
            .execute(&request, Instant::now(), Duration::from_secs(600))
            .await
            .unwrap();
        assert_eq!(replay, CommandResult::Accepted);
        assert!(processor.is_draining());

        let conflicting = RpcRequest::new(
            "two",
            method::HOST_DRAIN,
            &CommandParams {
                idempotency_key: "same-key".to_string(),
                budget_ms: 60_001,
                command: HostDrain { drain_id },
            },
        )
        .unwrap();
        assert_eq!(
            processor
                .execute(&conflicting, Instant::now(), Duration::from_secs(600))
                .await
                .unwrap(),
            CommandResult::IdempotencyConflict
        );

        let accepted_request = RpcRequest::new(
            "three",
            method::HOST_DRAIN,
            &CommandParams {
                idempotency_key: "accepted-key".to_string(),
                budget_ms: 1_000,
                command: HostDrain { drain_id },
            },
        )
        .unwrap();
        assert_eq!(
            processor
                .execute(&accepted_request, Instant::now(), Duration::from_secs(600))
                .await
                .unwrap(),
            CommandResult::Accepted
        );
        assert_eq!(processor.drain_id().await, Some(drain_id));
        assert_eq!(
            processor
                .execute(&accepted_request, Instant::now(), Duration::from_secs(600))
                .await
                .unwrap(),
            CommandResult::Accepted,
            "a retry with the same payload and idempotency key returns the retained result"
        );
    }

    #[tokio::test]
    async fn reject_self_drains_without_discarding_journal() {
        let temp = TempDir::new().unwrap();
        let journal = journal(&temp).await;
        let event = journal
            .record(Uuid::new_v4(), Uuid::new_v4(), VmEventKind::Booted, None)
            .await
            .unwrap();
        let commands = Arc::new(HostCommandProcessor::default());
        let state = Arc::new(EmptyState {
            host_id: Uuid::new_v4(),
            reconciliations: Mutex::new(Vec::new()),
        });
        let (host_io, api_io) = tokio::io::duplex(64 * 1024);
        let host_mux = Mux::new(host_io, false);
        let mut api_mux = Mux::new(api_io, true);
        let control = host_mux.open(Channel::Ctl).await.unwrap();
        let session = tokio::spawn(serve_control_stream(
            control,
            state,
            journal.clone(),
            commands.clone(),
            Arc::new(LeaseRenewalBroker::default()),
        ));
        let mut peer = api_mux.accept().await.unwrap();
        let hello: RpcMessage = serde_json::from_str(&peer.read_line().await.unwrap()).unwrap();
        let hello_id = match hello {
            RpcMessage::Request(request) => request.id,
            _ => panic!("expected hello request"),
        };
        send_message(
            &peer.into_split().0,
            &RpcMessage::Response(
                RpcResponse::result(
                    hello_id,
                    &HelloDisposition::Reject(HostReject {
                        accepted: false,
                        reason: HostRejectReason::UnsupportedVersion,
                        supported: SUPPORTED_PROTOCOL_VERSIONS.to_vec(),
                    }),
                )
                .unwrap(),
            ),
        )
        .await
        .unwrap();
        let outcome = session.await.unwrap().unwrap();
        assert!(matches!(outcome, SessionOutcome::Rejected(_)));
        assert!(commands.is_draining());
        assert_eq!(journal.pending().await, vec![event]);
    }

    #[tokio::test]
    async fn accepted_session_replays_event_before_heartbeat_and_acks_it() {
        let temp = TempDir::new().unwrap();
        let journal = journal(&temp).await;
        let event = journal
            .record(Uuid::new_v4(), Uuid::new_v4(), VmEventKind::Booted, None)
            .await
            .unwrap();
        let commands = Arc::new(HostCommandProcessor::default());
        let state = Arc::new(EmptyState {
            host_id: Uuid::new_v4(),
            reconciliations: Mutex::new(Vec::new()),
        });
        let (host_io, api_io) = tokio::io::duplex(64 * 1024);
        let host_mux = Mux::new(host_io, false);
        let mut api_mux = Mux::new(api_io, true);
        let control = host_mux.open(Channel::Ctl).await.unwrap();
        let session = tokio::spawn(serve_control_stream(
            control,
            state,
            journal.clone(),
            commands,
            Arc::new(LeaseRenewalBroker::default()),
        ));
        let mut peer = api_mux.accept().await.unwrap();
        let hello: RpcMessage = serde_json::from_str(&peer.read_line().await.unwrap()).unwrap();
        let hello_id = match hello {
            RpcMessage::Request(request) => request.id,
            _ => panic!("expected hello request"),
        };
        let (peer_writer, mut peer_reader) = peer.into_split();
        send_message(
            &peer_writer,
            &RpcMessage::Response(
                RpcResponse::result(
                    hello_id,
                    &HelloDisposition::Accept(HostAccept {
                        accepted: true,
                        protocol_version: PROTOCOL_VERSION,
                        incarnation_generation: 7,
                        current_incarnations: Vec::new(),
                        config: HostCtlConfig {
                            heartbeat_secs: 60,
                            result_retention_secs: 600,
                            source_preparation_enabled: false,
                            module_event_hints_enabled: false,
                        },
                    }),
                )
                .unwrap(),
            ),
        )
        .await
        .unwrap();
        let replay: RpcMessage =
            serde_json::from_str(&peer_reader.read_line().await.unwrap()).unwrap();
        match replay {
            RpcMessage::Notification(notification) => {
                assert_eq!(notification.method, method::VM_EVENT);
                assert_eq!(notification.params::<VmEvent>().unwrap(), event);
            }
            _ => panic!("event must be first after acceptance"),
        }
        send_message(
            &peer_writer,
            &RpcMessage::Notification(
                RpcNotification::new(
                    method::VM_EVENT_ACK,
                    &VmEventAck {
                        vm_id: event.vm_id,
                        through_seq: event.seq,
                    },
                )
                .unwrap(),
            ),
        )
        .await
        .unwrap();
        tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                if journal.pending().await.is_empty() {
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        session.abort();
    }

    #[tokio::test]
    async fn host_drain_reports_drained_after_the_retained_command_result() {
        let temp = TempDir::new().unwrap();
        let commands = Arc::new(HostCommandProcessor::default());
        let state = Arc::new(EmptyState {
            host_id: Uuid::new_v4(),
            reconciliations: Mutex::new(Vec::new()),
        });
        let (host_io, api_io) = tokio::io::duplex(64 * 1024);
        let host_mux = Mux::new(host_io, false);
        let mut api_mux = Mux::new(api_io, true);
        let control = host_mux.open(Channel::Ctl).await.unwrap();
        let session = tokio::spawn(serve_control_stream(
            control,
            state,
            journal(&temp).await,
            commands,
            Arc::new(LeaseRenewalBroker::default()),
        ));
        let peer = api_mux.accept().await.unwrap();
        let (peer_writer, mut peer_reader) = peer.into_split();
        let hello: RpcMessage =
            serde_json::from_str(&peer_reader.read_line().await.unwrap()).unwrap();
        let hello_id = match hello {
            RpcMessage::Request(request) => request.id,
            _ => panic!("expected hello request"),
        };
        send_message(
            &peer_writer,
            &RpcMessage::Response(
                RpcResponse::result(
                    hello_id,
                    &HelloDisposition::Accept(HostAccept {
                        accepted: true,
                        protocol_version: PROTOCOL_VERSION,
                        incarnation_generation: 1,
                        current_incarnations: Vec::new(),
                        config: HostCtlConfig {
                            heartbeat_secs: 60,
                            result_retention_secs: 600,
                            source_preparation_enabled: false,
                            module_event_hints_enabled: false,
                        },
                    }),
                )
                .unwrap(),
            ),
        )
        .await
        .unwrap();

        let drain_id = Uuid::new_v4();
        send_request(
            &peer_writer,
            &RpcRequest::new(
                "drain",
                method::HOST_DRAIN,
                &CommandParams {
                    idempotency_key: "drain-key".to_string(),
                    budget_ms: 1_000,
                    command: HostDrain { drain_id },
                },
            )
            .unwrap(),
        )
        .await
        .unwrap();

        let result: RpcMessage = serde_json::from_str(
            &tokio::time::timeout(Duration::from_secs(2), peer_reader.read_line())
                .await
                .unwrap()
                .unwrap(),
        )
        .unwrap();
        match result {
            RpcMessage::Response(response) => assert_eq!(
                response.result_as::<CommandResult>().unwrap(),
                CommandResult::Accepted
            ),
            _ => panic!("expected command response"),
        }
        let drained: RpcMessage = serde_json::from_str(
            &tokio::time::timeout(Duration::from_secs(2), peer_reader.read_line())
                .await
                .unwrap()
                .unwrap(),
        )
        .unwrap();
        match drained {
            RpcMessage::Notification(notification) => {
                assert_eq!(notification.method, method::HOST_DRAINED);
                assert_eq!(
                    notification.params::<HostDrained>().unwrap().drain_id,
                    drain_id
                );
            }
            _ => panic!("expected host.drained notification"),
        }
        session.abort();
    }

    #[tokio::test]
    async fn session_times_budget_from_receipt_and_refuses_seventeenth_inflight_command() {
        let temp = TempDir::new().unwrap();
        let journal = journal(&temp).await;
        let commands = Arc::new(HostCommandProcessor::default());
        let retained_guard = commands.retained.lock().await;
        let state = Arc::new(EmptyState {
            host_id: Uuid::new_v4(),
            reconciliations: Mutex::new(Vec::new()),
        });
        let (host_io, api_io) = tokio::io::duplex(256 * 1024);
        let host_mux = Mux::new(host_io, false);
        let mut api_mux = Mux::new(api_io, true);
        let control = host_mux.open(Channel::Ctl).await.unwrap();
        let session = tokio::spawn(serve_control_stream(
            control,
            state,
            journal,
            commands.clone(),
            Arc::new(LeaseRenewalBroker::default()),
        ));
        let peer = api_mux.accept().await.unwrap();
        let (peer_writer, mut peer_reader) = peer.into_split();
        let hello: RpcMessage =
            serde_json::from_str(&peer_reader.read_line().await.unwrap()).unwrap();
        let hello_id = match hello {
            RpcMessage::Request(request) => request.id,
            _ => panic!("expected hello request"),
        };
        send_message(
            &peer_writer,
            &RpcMessage::Response(
                RpcResponse::result(
                    hello_id,
                    &HelloDisposition::Accept(HostAccept {
                        accepted: true,
                        protocol_version: PROTOCOL_VERSION,
                        incarnation_generation: 1,
                        current_incarnations: Vec::new(),
                        config: HostCtlConfig {
                            heartbeat_secs: 60,
                            result_retention_secs: 600,
                            source_preparation_enabled: false,
                            module_event_hints_enabled: false,
                        },
                    }),
                )
                .unwrap(),
            ),
        )
        .await
        .unwrap();

        for index in 0..=MAX_INFLIGHT_COMMANDS {
            let request = RpcRequest::new(
                format!("command-{index}"),
                method::HOST_DRAIN,
                &CommandParams {
                    idempotency_key: format!("key-{index}"),
                    budget_ms: 1_000,
                    command: HostDrain {
                        drain_id: Uuid::new_v4(),
                    },
                },
            )
            .unwrap();
            send_request(&peer_writer, &request).await.unwrap();
        }

        let busy = tokio::time::timeout(Duration::from_secs(2), peer_reader.read_line())
            .await
            .unwrap()
            .unwrap();
        let busy: RpcMessage = serde_json::from_str(&busy).unwrap();
        match busy {
            RpcMessage::Response(response) => {
                assert_eq!(response.id, format!("command-{MAX_INFLIGHT_COMMANDS}"));
                assert_eq!(
                    response.result_as::<CommandResult>().unwrap(),
                    CommandResult::Busy
                );
            }
            _ => panic!("expected busy response"),
        }

        tokio::time::sleep(Duration::from_millis(1_005)).await;
        drop(retained_guard);
        let expired = tokio::time::timeout(Duration::from_secs(2), peer_reader.read_line())
            .await
            .unwrap()
            .unwrap();
        let expired: RpcMessage = serde_json::from_str(&expired).unwrap();
        match expired {
            RpcMessage::Response(response) => assert_eq!(
                response.result_as::<CommandResult>().unwrap(),
                CommandResult::Expired
            ),
            _ => panic!("expected expired response"),
        }
        session.abort();
    }

    #[test]
    fn retained_rpc_error_shape_is_json_rpc_compliant() {
        let response = RpcResponse {
            jsonrpc: Default::default(),
            id: "x".to_string(),
            result: None,
            error: Some(RpcError {
                code: -32601,
                message: "method not found".to_string(),
                data: None,
            }),
        };
        assert_eq!(
            serde_json::to_value(response).unwrap()["error"]["code"],
            -32601
        );
    }

    #[test]
    fn host_vm_state_keeps_wire_names_stable() {
        assert_eq!(
            serde_json::to_value(HostVmState::Booting).unwrap(),
            "booting"
        );
    }

    /// `g5.runtime.module-event-hints`: module-event frames on a dispatcher
    /// control connection become one content-free hint for the incarnation
    /// the connection is bound to, with execution ids de-duplicated, and
    /// neither the frame's identity claims nor its payload reach the bridge.
    #[tokio::test]
    async fn module_event_frames_become_one_coalesced_hint_for_the_bound_incarnation() {
        let temp = TempDir::new().unwrap();
        let journal = journal(&temp).await;
        let broker = LeaseRenewalBroker::default();
        let workspace_id = Uuid::new_v4();
        let vm_id = Uuid::new_v4();
        let (local_io, peer_io) = tokio::io::duplex(4096);
        let local = Mux::new(local_io, true);
        let mut peer = Mux::new(peer_io, false);
        let stream = local.open(Channel::Rpc).await.unwrap();
        let _accepted = peer.accept().await.unwrap();
        let writer = Mutex::new(stream.into_split().0);
        let exec_a = format!("task.{}.1.{vm_id}", Uuid::new_v4());
        let exec_b = format!("task.{}.1.{vm_id}", Uuid::new_v4());
        for (exec_id, seq) in [(&exec_a, 1), (&exec_a, 2), (&exec_b, 1)] {
            assert!(
                handle_dispatcher_control_message(
                    &serde_json::json!({
                        "jsonrpc": "2.0",
                        "method": "module.event",
                        "params": {
                            "exec_id": exec_id,
                            "seq": seq,
                            "kind": "module.message.delta",
                            "payload": {"text_delta": "secret-looking text"},
                            "terminal": false,
                            // A frame cannot choose another incarnation.
                            "workspace_id": Uuid::new_v4(),
                            "vm_id": Uuid::new_v4(),
                        },
                    })
                    .to_string(),
                    &writer,
                    &broker,
                    &journal,
                    workspace_id,
                    vm_id,
                    3,
                )
                .await,
                "module events are consumed, not forwarded"
            );
        }
        assert!(
            handle_dispatcher_control_message(
                &serde_json::json!({
                    "jsonrpc": "2.0",
                    "method": "module.events.lagged",
                    "params": {"skipped": 4, "vm_id": vm_id},
                })
                .to_string(),
                &writer,
                &broker,
                &journal,
                workspace_id,
                vm_id,
                3,
            )
            .await
        );
        assert!(journal.pending().await.is_empty());
        let hint = tokio::time::timeout(Duration::from_secs(2), broker.next_module_hint())
            .await
            .unwrap();
        assert_eq!(
            hint,
            VmModuleEvents {
                workspace_id,
                vm_id,
                epoch: 3,
                exec_ids: vec![exec_a.clone(), exec_b.clone()],
            }
        );
        assert!(hint.validate().is_ok());
        assert!(
            tokio::time::timeout(Duration::from_millis(200), broker.next_module_hint())
                .await
                .is_err(),
            "the drained set is not repeated"
        );
    }

    async fn accepted_session_with_hints(
        enabled: bool,
    ) -> (
        Arc<LeaseRenewalBroker>,
        MuxWriter,
        MuxReader,
        tokio::task::JoinHandle<Result<SessionOutcome>>,
        (Mux, Mux),
    ) {
        let temp = TempDir::new().unwrap();
        let commands = Arc::new(HostCommandProcessor::default());
        let state = Arc::new(EmptyState {
            host_id: Uuid::new_v4(),
            reconciliations: Mutex::new(Vec::new()),
        });
        let broker = Arc::new(LeaseRenewalBroker::default());
        let (host_io, api_io) = tokio::io::duplex(64 * 1024);
        let host_mux = Mux::new(host_io, false);
        let mut api_mux = Mux::new(api_io, true);
        let control = host_mux.open(Channel::Ctl).await.unwrap();
        let journal = journal(&temp).await;
        std::mem::forget(temp);
        let session = tokio::spawn(serve_control_stream(
            control,
            state,
            journal,
            commands,
            broker.clone(),
        ));
        let mut peer = api_mux.accept().await.unwrap();
        let hello: RpcMessage = serde_json::from_str(&peer.read_line().await.unwrap()).unwrap();
        let hello_id = match hello {
            RpcMessage::Request(request) => {
                let hello = request.params::<HostHello>().unwrap();
                assert!(
                    hello
                        .capabilities
                        .iter()
                        .any(|c| c == "isolation:cloud-hypervisor"),
                    "the test state's hello is the test's own"
                );
                request.id
            }
            _ => panic!("expected hello request"),
        };
        let (peer_writer, peer_reader) = peer.into_split();
        send_message(
            &peer_writer,
            &RpcMessage::Response(
                RpcResponse::result(
                    hello_id,
                    &HelloDisposition::Accept(HostAccept {
                        accepted: true,
                        protocol_version: PROTOCOL_VERSION,
                        incarnation_generation: 1,
                        current_incarnations: Vec::new(),
                        config: HostCtlConfig {
                            heartbeat_secs: 60,
                            result_retention_secs: 600,
                            source_preparation_enabled: false,
                            module_event_hints_enabled: enabled,
                        },
                    }),
                )
                .unwrap(),
            ),
        )
        .await
        .unwrap();
        // Both multiplexers must outlive the test: dropping either closes
        // the control stream.
        (
            broker,
            peer_writer,
            peer_reader,
            session,
            (host_mux, api_mux),
        )
    }

    /// `g5.runtime.module-event-hints`: a control plane that enabled hints
    /// receives `vm.module_events`; one that did not receives nothing, so a
    /// control plane from before the hint never sees an unknown notification.
    #[tokio::test]
    async fn accepted_session_forwards_module_event_hints_only_when_enabled() {
        let (broker, _writer, mut reader, session, _mux) = accepted_session_with_hints(true).await;
        let workspace_id = Uuid::new_v4();
        let vm_id = Uuid::new_v4();
        broker
            .publish_module_hint(workspace_id, vm_id, 2, "task.a.1.vm")
            .await;
        broker
            .publish_module_hint(workspace_id, vm_id, 2, "task.b.1.vm")
            .await;
        let line = tokio::time::timeout(Duration::from_secs(5), reader.read_line())
            .await
            .expect("a hint is sent promptly")
            .unwrap();
        let message: RpcMessage = serde_json::from_str(&line).unwrap();
        match message {
            RpcMessage::Notification(notification) => {
                assert_eq!(notification.method, method::VM_MODULE_EVENTS);
                assert_eq!(
                    notification.params::<VmModuleEvents>().unwrap(),
                    VmModuleEvents {
                        workspace_id,
                        vm_id,
                        epoch: 2,
                        exec_ids: vec!["task.a.1.vm".into(), "task.b.1.vm".into()],
                    }
                );
            }
            other => panic!("expected a module-event hint, got {other:?}"),
        }
        session.abort();

        let (broker, _writer, mut reader, session, _mux) = accepted_session_with_hints(false).await;
        broker
            .publish_module_hint(workspace_id, vm_id, 2, "task.a.1.vm")
            .await;
        assert!(
            tokio::time::timeout(Duration::from_millis(500), reader.read_line())
                .await
                .is_err(),
            "a control plane that did not enable hints receives none"
        );
        session.abort();
    }
}
