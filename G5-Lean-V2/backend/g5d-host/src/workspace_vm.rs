//! Long-lived workspace VM custody for `g5d host`.
//!
//! This module is the single host-side owner of workspace incarnation
//! admission, VM handles, dispatcher relay endpoints, and workspace
//! roots, which live only as long as their VM (`g5.runtime.purge`).
//! The ctl command processor and heartbeat/inventory projection share this
//! object; neither keeps a second VM-state map.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::net::SocketAddrV4;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use g5_contracts::types::{RepoInfo, RepoRole};
use g5_internal_api::host_ctl::{
    BootSummary, CommandResult, CurrentIncarnation, HostRuntimeIdentity, HostVmState,
    SlotUnavailableReason, SpecRejectReason, StopMode, VmEventKind, VmExit, VmInventory,
    VmReferenceRepoSpec, VmRepoSpec, VmSpec, VmStart, VmStatus, VmStatusReport, VmStop,
    VM_SPEC_VERSION,
};
use g5_internal_api::stream_mux::{Channel, Mux, MuxHandle, MuxStream};
use g5d_foundation::runtime_environment::GUEST_RUNTIME_LIBRARY_PATH;
use g5d_foundation::workspace::WorkspaceCheckout;
use g5d_isolation::port_map::{PortAllocator, PortLease};
use g5d_isolation::{
    CancelledChildReapUnconfirmed, MicrovmMount, MicrovmRunSpec, RunHandle, ScratchDiskSpec,
};
use getrandom::getrandom;
use serde_json::{json, Value};
use tokio::io::AsyncWriteExt;
use tokio::net::TcpStream;
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::ctl::{handle_dispatcher_control_message, EventJournal, LeaseRenewalBroker};
use crate::runtime::VmLauncher;

const DISPATCH_GUEST_PORT: u16 = 7100;
const MAX_CONCURRENT_BOOTS: usize = 2;
const MAX_DISPATCH_PORT_PROBES: usize = 256;
const MAX_DISPATCH_BOOT_ATTEMPTS: usize = 2;
/// Upper bound on the optional base-branch fetch during restore; the shared
/// branch fetch stays mandatory and unbounded by this.
const UPSTREAM_BRANCH_FETCH_TIMEOUT: Duration = Duration::from_secs(60);
const MAX_STOP_GRACE_SECS: u64 = 300;
const MAX_ENV_VALUE_BYTES: usize = 16 * 1024;

const FAKE_PROVIDER_ENV: &[&str] = &[
    "G5_CLAUDE_CODE_FAKE",
    "G5_CODEX_FAKE",
    "G5_FAKE_SCENARIO",
    "G5_FAKE_CLAUDE_SCENARIO",
    "G5_FAKE_CODEX_SCENARIO",
    "G5_FAKE_STRICT_RESUME",
    "G5_FAKE_BUDGET_RESUME_E2E",
];

const ALLOWED_ENV_KEYS: &[&str] = &[
    "G5D_WORKSPACE_ID",
    "G5D_VM_ID",
    "G5D_EPOCH",
    "G5D_WORKSPACE_HOSTNAME",
    "G5D_API_URL",
    "G5D_DISPATCH_PORT",
    "G5D_OWNER_GIT_NAME",
    "G5D_OWNER_GIT_EMAIL",
    "G5D_MAX_PTY_STREAMS",
    "G5D_MODULE_UID",
    "G5D_MODULE_GID",
    "HOME",
    "PATH",
    "CLAUDE_CONFIG_DIR",
    "CODEX_HOME",
];

#[derive(Debug)]
struct DispatcherReadyTimeout;

impl fmt::Display for DispatcherReadyTimeout {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("dispatcher_ready_timeout")
    }
}

impl std::error::Error for DispatcherReadyTimeout {}

#[derive(Debug)]
struct DispatcherExitedBeforeReady {
    exit_code: Option<i32>,
    log_tail: Option<String>,
}

impl fmt::Display for DispatcherExitedBeforeReady {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "dispatcher_exited_before_ready (exit_code={:?})",
            self.exit_code
        )
    }
}

impl std::error::Error for DispatcherExitedBeforeReady {}

impl DispatcherExitedBeforeReady {
    async fn capture(handle: &RunHandle, exit_code: Option<i32>) -> Self {
        handle
            .wait_console_drained(Duration::from_millis(250))
            .await;
        // The existing relay redacts secrets and caps retained bytes/lines.
        let log_tail = handle
            .console_tail()
            .filter(|tail| !tail.is_empty())
            .map(|tail| {
                tail.lines()
                    .into_iter()
                    .map(|line| format!("[{}] {}", line.stream.as_str(), line.line))
                    .collect::<Vec<_>>()
                    .join("\n")
            });
        Self {
            exit_code,
            log_tail,
        }
    }
}

#[async_trait]
pub trait OtherSlotUsage: Send + Sync {
    async fn slots_in_use(&self) -> usize;
}

#[derive(Debug, Clone)]
pub struct WorkspaceVmRuntimeConfig {
    pub org_id: Uuid,
    pub slots_total: u32,
    pub cpus_per_vm: u8,
    pub mem_mib_per_vm: u32,
    pub scratch_disk_mib: u32,
    /// Linux microVM opt-in: use the published scratch capacity for one
    /// disposable complete workload filesystem, including /work and Docker.
    pub block_workload: bool,
    /// Boot every microVM with hardware virtualization exposed
    /// (`g5.architecture.microvm_backend.nested`); resolved once from the host
    /// environment, published on hello and recorded in each boot summary.
    pub nested_virtualization: bool,
    pub api_url: String,
    pub runtime_root: PathBuf,
    pub workspaces_root: PathBuf,
    pub dispatcher_ready_timeout: Duration,
    /// Local process mode (`g5.runtime.boot.backend.process-local`): the host
    /// executable that runs `g5d dispatch` for every incarnation instead of
    /// the guest bundle's. `None` keeps the microVM boot spec.
    pub process_host_executable: Option<PathBuf>,
}

struct ActiveVm {
    spec: VmSpec,
    state: HostVmState,
    boot_at: DateTime<Utc>,
    ready_at: Option<DateTime<Utc>>,
    handle: Option<Arc<RunHandle>>,
    dispatcher: Option<MuxHandle>,
    dispatch_port: PortLease,
    dispatch_token: [u8; 32],
    streams_open: Arc<AtomicU32>,
    cancel: CancellationToken,
}

/// A `vm.start` the host has received but not yet reserved: from the moment
/// the control session dequeues the request until its execution ends. While
/// pending, the incarnation is booting inventory, `vm.status` answers for it
/// and `vm.stop` cancels it, so the control plane never reads host absence
/// for a Start the host could still admit (`g5.runtime.start.settlement`).
struct PendingStart {
    /// Identifies this pending entry even after reservation removes it and a
    /// later request registers the same VM id again.
    registration_id: Uuid,
    registrations: u32,
    workspace_id: Uuid,
    epoch: u64,
    size: g5_internal_api::host_ctl::VmSizeSpec,
    received_at: DateTime<Utc>,
    /// Set by a Stop that found the Start pending: reservation refuses it,
    /// so from then on the incarnation is absent and can never be admitted.
    cancelled: bool,
}

/// Holds a pending Start's registration for the lifetime of its execution.
/// `settle` releases this execution's registration; dropping the guard does
/// the same on the runtime. The last execution removes the pending entry.
pub struct PendingStartGuard {
    state: Arc<Mutex<RuntimeState>>,
    vm_id: Uuid,
    registration_id: Option<Uuid>,
}

impl PendingStartGuard {
    /// This execution ended; other executions may still need the pending
    /// entry, including its Stop cancellation mark.
    pub async fn settle(mut self) {
        let mut state = self.state.lock().await;
        // Keep the registration until the lock is held, so cancellation while
        // waiting still leaves Drop responsible for releasing it.
        if let Some(registration_id) = self.registration_id.take() {
            state.release_start(self.vm_id, registration_id);
        }
    }
}

impl Drop for PendingStartGuard {
    fn drop(&mut self) {
        if let Some(registration_id) = self.registration_id.take() {
            let state = self.state.clone();
            let vm_id = self.vm_id;
            if let Ok(handle) = tokio::runtime::Handle::try_current() {
                handle.spawn(async move {
                    state.lock().await.release_start(vm_id, registration_id);
                });
            }
        }
    }
}

struct RuntimeState {
    active: BTreeMap<Uuid, ActiveVm>,
    /// Starts received but not yet reserved, by VM id; see [`PendingStart`].
    pending_starts: BTreeMap<Uuid, PendingStart>,
    /// VMs that left `active` without proof that their VMM was reaped (a kill
    /// that failed or is still running), with their workspace. Such a VMM may
    /// still be using `/work`, so its workspace's root stays held until its
    /// own kill proves reap, a later Start of the workspace replaces the tree,
    /// or the host restarts (`g5.runtime.custody`).
    unreaped: BTreeMap<Uuid, Uuid>,
    boots_in_progress: usize,
}

impl RuntimeState {
    fn release_start(&mut self, vm_id: Uuid, registration_id: Uuid) {
        if let Some(pending) = self.pending_starts.get_mut(&vm_id) {
            // Reservation may already have removed this entry. An old guard
            // must not release a later registration for the same VM id.
            if pending.registration_id != registration_id {
                return;
            }
            pending.registrations -= 1;
            if pending.registrations == 0 {
                self.pending_starts.remove(&vm_id);
            }
        }
    }
}

/// Prefix of a released workspace root waiting for deletion
/// (`.trash-<uuid>`).
const CLAIMED_ROOT_PREFIX: &str = ".trash-";
/// Retention bookkeeping written by hosts from before roots were deleted at
/// release.
const LEGACY_RETAINED_CONTROL_DIR: &str = ".g5-retained";

/// Deletes the workspace roots no VM holds (`g5.runtime.purge`). A root is
/// claimed by renaming it to `.trash-<uuid>` under the runtime state lock, the
/// same lock that admits a Start, so a later Start of that workspace always
/// builds a fresh tree. Claimed roots are deleted in the background because a
/// root can hold tens of gigabytes of build output.
#[derive(Clone)]
struct RootReaper {
    workspaces_root: PathBuf,
    deleting: Arc<Mutex<()>>,
}

/// The owner of a `workspaces_root` entry the host itself creates.
enum RootEntry {
    /// The root (`<id>`) or a staging or replaced copy left by its
    /// materialization (`.<id>.staging-<uuid>`, `.<id>.previous-<uuid>`).
    Workspace(Uuid),
    LegacyRetention,
}

impl RootEntry {
    fn classify(name: &str) -> Option<Self> {
        if name == LEGACY_RETAINED_CONTROL_DIR {
            return Some(Self::LegacyRetention);
        }
        let id = match name.strip_prefix('.') {
            None => name,
            Some(rest) => {
                let (id, suffix) = rest.split_once('.')?;
                let copy = suffix
                    .strip_prefix("staging-")
                    .or_else(|| suffix.strip_prefix("previous-"))?;
                canonical_uuid(copy)?;
                id
            }
        };
        canonical_uuid(id).map(Self::Workspace)
    }
}

/// `text` as a UUID, only in the canonical spelling the host writes, so an
/// entry someone else put in `workspaces_root` is never taken for ours.
fn canonical_uuid(text: &str) -> Option<Uuid> {
    let id = Uuid::parse_str(text).ok()?;
    (id.hyphenated().to_string() == text).then_some(id)
}

impl RootReaper {
    /// Claim every root no VM holds and start deleting the claimed roots.
    async fn release_unheld(&self, state: &Mutex<RuntimeState>) {
        self.claim_unheld(state).await;
        let reaper = self.clone();
        tokio::spawn(async move { reaper.delete_claimed().await });
    }

    async fn claim_unheld(&self, state: &Mutex<RuntimeState>) {
        let state = state.lock().await;
        let held = state
            .active
            .values()
            .map(|vm| vm.spec.incarnation.workspace_id)
            .chain(state.unreaped.values().copied())
            .collect::<BTreeSet<_>>();
        let mut entries = match tokio::fs::read_dir(&self.workspaces_root).await {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return,
            Err(error) => {
                tracing::warn!(%error, workspaces_root = %self.workspaces_root.display(), "could not list workspace roots to release");
                return;
            }
        };
        loop {
            let entry = match entries.next_entry().await {
                Ok(Some(entry)) => entry,
                Ok(None) => break,
                Err(error) => {
                    tracing::warn!(%error, workspaces_root = %self.workspaces_root.display(), "could not list workspace roots to release");
                    break;
                }
            };
            let claim = match entry.file_name().to_str().and_then(RootEntry::classify) {
                Some(RootEntry::Workspace(workspace_id)) => !held.contains(&workspace_id),
                Some(RootEntry::LegacyRetention) => true,
                None => false,
            };
            if !claim {
                continue;
            }
            let claimed = self
                .workspaces_root
                .join(format!("{CLAIMED_ROOT_PREFIX}{}", Uuid::new_v4()));
            match tokio::fs::rename(entry.path(), &claimed).await {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => tracing::warn!(
                    %error,
                    path = %entry.path().display(),
                    "could not claim a released workspace root; the next release or host start retries"
                ),
            }
        }
    }

    /// Delete every claimed root. One pass runs at a time; a root that fails
    /// to delete stays claimed for the next pass. Deletion never follows a
    /// symlink the guest left in its tree.
    async fn delete_claimed(&self) {
        let _pass = self.deleting.lock().await;
        let mut entries = match tokio::fs::read_dir(&self.workspaces_root).await {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return,
            Err(error) => {
                tracing::warn!(%error, workspaces_root = %self.workspaces_root.display(), "could not list released workspace roots");
                return;
            }
        };
        loop {
            let entry = match entries.next_entry().await {
                Ok(Some(entry)) => entry,
                Ok(None) => break,
                Err(error) => {
                    tracing::warn!(%error, workspaces_root = %self.workspaces_root.display(), "could not list released workspace roots");
                    break;
                }
            };
            if entry
                .file_name()
                .to_str()
                .and_then(|name| name.strip_prefix(CLAIMED_ROOT_PREFIX))
                .and_then(canonical_uuid)
                .is_none()
            {
                continue;
            }
            let path = entry.path();
            let deleted = match tokio::fs::symlink_metadata(&path).await {
                Ok(metadata) if metadata.is_dir() => tokio::fs::remove_dir_all(&path).await,
                Ok(_) => tokio::fs::remove_file(&path).await,
                Err(error) => Err(error),
            };
            match deleted {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => tracing::warn!(
                    %error,
                    path = %path.display(),
                    "could not delete a released workspace root; the next release or host start retries"
                ),
            }
        }
    }
}

/// Stream opener for one already-authenticated workspace dispatcher mux.
#[derive(Clone)]
pub struct DispatcherEndpoint {
    mux: MuxHandle,
    streams_open: Arc<AtomicU32>,
    // Captured with the mux under the runtime lock, so relayed control
    // frames always use the identity belonging to this connection.
    pub(crate) workspace_id: Uuid,
    pub(crate) epoch: u64,
}

impl DispatcherEndpoint {
    pub async fn open(&self, channel: Channel) -> Result<(MuxStream, DispatcherStreamGuard)> {
        let stream = self
            .mux
            .open(channel)
            .await
            .map_err(|error| anyhow!("open workspace dispatcher stream: {error}"))?;
        self.streams_open.fetch_add(1, Ordering::AcqRel);
        Ok((stream, DispatcherStreamGuard(self.streams_open.clone())))
    }
}

pub struct DispatcherStreamGuard(Arc<AtomicU32>);

impl Drop for DispatcherStreamGuard {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::AcqRel);
    }
}

pub struct WorkspaceVmRuntime {
    config: WorkspaceVmRuntimeConfig,
    launcher: Arc<dyn VmLauncher>,
    checkout: Arc<WorkspaceCheckout>,
    journal: Arc<EventJournal>,
    lease_renewals: Arc<LeaseRenewalBroker>,
    other_slots: Arc<dyn OtherSlotUsage>,
    ports: PortAllocator,
    state: Arc<Mutex<RuntimeState>>,
    roots: RootReaper,
    source_preparation: crate::source_preparation::SourcePreparation,
}

impl WorkspaceVmRuntime {
    /// The hello capability naming this host's isolation level.
    #[must_use]
    pub fn isolation_capability(&self) -> &'static str {
        if self.config.process_host_executable.is_some() {
            crate::runtime::CAPABILITY_ISOLATION_PROCESS
        } else {
            crate::runtime::CAPABILITY_ISOLATION_MICROVM
        }
    }

    /// Whether this host boots workspace VMs with hardware virtualization
    /// exposed (`g5.architecture.microvm_backend.nested`). Process mode never
    /// does: there is no VMM to pass the option to.
    #[must_use]
    pub fn nested_virtualization(&self) -> bool {
        self.config.nested_virtualization && self.config.process_host_executable.is_none()
    }

    pub fn slot_size(&self) -> g5_internal_api::host_ctl::VmSizeSpec {
        g5_internal_api::host_ctl::VmSizeSpec {
            cpus: self.config.cpus_per_vm,
            mem_mib: self.config.mem_mib_per_vm,
            scratch_disk_mib: self.config.scratch_disk_mib,
        }
    }

    /// This host's runtime identity, read from its runtime root for every
    /// hello and every start so the published identity and the spec check
    /// can never disagree (`g5.runtime.placement.runtime-identity`). `None`
    /// when the catalog is unreadable: such a host rejects every spec.
    #[must_use]
    pub fn runtime_identity(&self) -> Option<HostRuntimeIdentity> {
        match HostRuntimeIdentity::read(&self.config.runtime_root) {
            Ok(identity) => Some(identity),
            Err(error) => {
                tracing::warn!(
                    %error,
                    runtime_root = %self.config.runtime_root.display(),
                    "runtime module catalog unreadable; publishing no runtime identity"
                );
                None
            }
        }
    }

    #[must_use]
    pub fn new(
        config: WorkspaceVmRuntimeConfig,
        launcher: Arc<dyn VmLauncher>,
        checkout: Arc<WorkspaceCheckout>,
        journal: Arc<EventJournal>,
        lease_renewals: Arc<LeaseRenewalBroker>,
        other_slots: Arc<dyn OtherSlotUsage>,
    ) -> Self {
        let roots = RootReaper {
            workspaces_root: config.workspaces_root.clone(),
            deleting: Arc::new(Mutex::new(())),
        };
        Self {
            config,
            launcher,
            checkout,
            journal,
            lease_renewals,
            other_slots,
            ports: PortAllocator::new(),
            state: Arc::new(Mutex::new(RuntimeState {
                active: BTreeMap::new(),
                pending_starts: BTreeMap::new(),
                unreaped: BTreeMap::new(),
                boots_in_progress: 0,
            })),
            roots,
            source_preparation: Default::default(),
        }
    }

    /// Seed only the already-running boundary for stop/control tests. These
    /// tests do not require the reflink materializer or a real guest boot.
    #[cfg(test)]
    pub(crate) async fn seed_running_vm(
        &self,
        spec: VmSpec,
    ) -> g5d_isolation::TestChildReapPublisher {
        let vm_id = spec.incarnation.vm_id;
        let root = self
            .config
            .workspaces_root
            .join(spec.incarnation.workspace_id.to_string());
        tokio::fs::create_dir_all(root).await.unwrap();
        let (handle, publisher) = RunHandle::test_microvm_with_reap_publisher(
            format!("test-{vm_id}"),
            Some(std::net::Ipv4Addr::LOCALHOST),
            Some(4242),
        );
        let record = ActiveVm {
            spec,
            state: HostVmState::Running,
            boot_at: Utc::now(),
            ready_at: Some(Utc::now()),
            handle: Some(Arc::new(handle)),
            dispatcher: None,
            dispatch_port: self
                .ports
                .lease_bindable(MAX_DISPATCH_PORT_PROBES)
                .await
                .unwrap(),
            dispatch_token: [0; 32],
            streams_open: Arc::new(AtomicU32::new(0)),
            cancel: CancellationToken::new(),
        };
        assert!(self
            .state
            .lock()
            .await
            .active
            .insert(vm_id, record)
            .is_none());
        publisher
    }

    pub async fn cancel_source_preparation(&self) {
        self.source_preparation.stop().await;
    }

    pub fn can_prepare_sources(&self) -> bool {
        self.config.block_workload && self.config.process_host_executable.is_none()
    }

    pub async fn prepare_sources(
        &self,
        assignment: g5_internal_api::host_ctl::HostSourcePreparation,
    ) {
        if !self.can_prepare_sources() || assignment.org_id != self.config.org_id {
            return;
        }
        self.source_preparation
            .schedule(
                assignment,
                self.checkout.clone(),
                self.config.scratch_disk_mib,
                self.config.slots_total,
            )
            .await;
    }

    /// Register a received Start before anything else about its incarnation is
    /// evaluated (`g5.runtime.start.settlement`). The control session calls
    /// this when it dequeues the request, ahead of any later command on the
    /// connection; the returned guard lives until the Start's execution ends.
    /// Every pending execution registers, including a replay. An active
    /// incarnation already has a reservation and needs no pending entry.
    pub async fn register_start(&self, command: &VmStart) -> PendingStartGuard {
        let mut state = self.state.lock().await;
        let registration_id = if state.active.contains_key(&command.vm_id) {
            None
        } else {
            let pending = state
                .pending_starts
                .entry(command.vm_id)
                .or_insert_with(|| PendingStart {
                    registration_id: Uuid::new_v4(),
                    registrations: 0,
                    workspace_id: command.workspace_id,
                    epoch: command.spec.incarnation.epoch,
                    size: command.spec.size.clone(),
                    received_at: Utc::now(),
                    cancelled: false,
                });
            pending.registrations += 1;
            Some(pending.registration_id)
        };
        PendingStartGuard {
            state: self.state.clone(),
            vm_id: command.vm_id,
            registration_id,
        }
    }

    pub async fn start(&self, command: VmStart) -> CommandResult {
        if let Err(reason) = self.validate_start(&command) {
            tracing::warn!(
                workspace_id = %command.workspace_id,
                vm_id = %command.vm_id,
                ?reason,
                "workspace boot specification rejected"
            );
            return CommandResult::SpecRejected { reason };
        }

        let _foreground = self.source_preparation.foreground().await;
        let other_slots = self.other_slots.slots_in_use().await;
        {
            let mut state = self.state.lock().await;
            // A Stop that found this Start pending cancelled it and answered
            // absent; admitting it now would revive a released incarnation
            // (`g5.runtime.start.settlement`).
            if state
                .pending_starts
                .get(&command.vm_id)
                .is_some_and(|pending| pending.cancelled)
            {
                return CommandResult::BootFailed {
                    reason: "boot_cancelled".to_string(),
                    log_tail: None,
                };
            }
            if let Some(existing) = state.active.get(&command.vm_id) {
                return if existing.spec.incarnation.workspace_id == command.workspace_id {
                    CommandResult::AlreadyRunning
                } else {
                    CommandResult::SpecRejected {
                        reason: SpecRejectReason::IdentityMismatch,
                    }
                };
            }
            if state.active.values().any(|vm| {
                vm.spec.incarnation.workspace_id == command.workspace_id
                    && vm.spec.incarnation.vm_id != command.vm_id
            }) {
                return CommandResult::WorkspaceBusy;
            }
            if state.boots_in_progress >= MAX_CONCURRENT_BOOTS {
                tracing::warn!(
                    workspace_id = %command.workspace_id,
                    boots_in_progress = state.boots_in_progress,
                    "refusing workspace VM start: too many boots in progress"
                );
                return CommandResult::SlotUnavailable {
                    reason: SlotUnavailableReason::BootCapacity,
                };
            }
            if state.active.len().saturating_add(other_slots)
                >= usize::try_from(self.config.slots_total).unwrap_or(usize::MAX)
            {
                tracing::warn!(
                    workspace_id = %command.workspace_id,
                    active = state.active.len(),
                    other_slots,
                    slots_total = self.config.slots_total,
                    "refusing workspace VM start: every slot is in use"
                );
                return CommandResult::SlotUnavailable {
                    reason: SlotUnavailableReason::Slots,
                };
            }
            // Admission-time check only: the VM's disk stays sparse and grows
            // with its writes; nothing reserves V for later.
            let free_bytes =
                g5d_foundation::disk::free_bytes(&self.config.workspaces_root).unwrap_or(0);
            tracing::info!(workspace_id = %command.workspace_id, vm_id = %command.vm_id,
                available_bytes = free_bytes,
                required_bytes = u64::from(command.spec.size.scratch_disk_mib) << 20,
                image_ref = %command.spec.image.image_ref,
                phase = "disk_admission", "checking recorded VM disk capacity");
            if let Err(pressure) =
                g5d_foundation::disk::admit_private(free_bytes, command.spec.size.scratch_disk_mib)
            {
                tracing::warn!(workspace_id = %command.workspace_id, %pressure,
                    "refusing workspace VM start: not enough free disk space on the host");
                // Released roots may still be waiting for deletion, or an
                // earlier pass may have failed; retry so a queued Start is
                // not refused by space nothing holds.
                let (roots, state) = (self.roots.clone(), self.state.clone());
                tokio::spawn(async move { roots.release_unheld(&state).await });
                return CommandResult::SlotUnavailable {
                    reason: SlotUnavailableReason::Disk,
                };
            }
            let dispatch_port = match self.ports.lease_bindable(MAX_DISPATCH_PORT_PROBES).await {
                Ok(port) => port,
                Err(error) => {
                    tracing::warn!(%error, "no bindable host port for workspace dispatcher");
                    return CommandResult::SlotUnavailable {
                        reason: SlotUnavailableReason::BootCapacity,
                    };
                }
            };
            let mut dispatch_token = [0_u8; 32];
            if getrandom(&mut dispatch_token).is_err() {
                return CommandResult::BootFailed {
                    reason: "dispatcher_token_unavailable".to_string(),
                    log_tail: None,
                };
            }
            state.boots_in_progress += 1;
            state.active.insert(
                command.vm_id,
                ActiveVm {
                    spec: command.spec.clone(),
                    state: HostVmState::Booting,
                    boot_at: Utc::now(),
                    ready_at: None,
                    handle: None,
                    dispatcher: None,
                    dispatch_port,
                    dispatch_token,
                    streams_open: Arc::new(AtomicU32::new(0)),
                    cancel: CancellationToken::new(),
                },
            );
            // Reserved: the active record now answers for the incarnation.
            state.pending_starts.remove(&command.vm_id);
        }

        let result = self.boot_reserved(&command).await;
        self.complete_boot(&command, result).await
    }

    /// Settle a boot once `boot_reserved` returned: ready, or cancelled, lost
    /// or failed. A boot cancelled while it completed, or whose reservation
    /// vanished, keeps a reservation with its handle, port, slot, workspace
    /// exclusion and inventory while its exit is proven, and is released only
    /// afterwards (`g5.runtime.custody`, review R17).
    async fn complete_boot(
        &self,
        command: &VmStart,
        result: Result<(Arc<RunHandle>, Option<MuxHandle>, RestoreStats)>,
    ) -> CommandResult {
        let mut state = self.state.lock().await;
        state.boots_in_progress = state.boots_in_progress.saturating_sub(1);
        match result {
            Ok((handle, dispatcher, restore)) => {
                // A boot cancelled while it completed, or whose reservation
                // vanished (nothing removes a booting record today, so this is
                // defensive): the incarnation keeps a reservation while its exit
                // is proven, so its slot, port, workspace exclusion and
                // inventory hold until the receipt (review R17).
                let ended = match state.active.get(&command.vm_id) {
                    None => Some("reservation_lost"),
                    Some(record)
                        if record.state == HostVmState::Stopping
                            || record.cancel.is_cancelled() =>
                    {
                        Some("boot_cancelled")
                    }
                    Some(_) => None,
                };
                if let Some(reason) = ended {
                    if let std::collections::btree_map::Entry::Vacant(slot) =
                        state.active.entry(command.vm_id)
                    {
                        match self.ports.lease_bindable(MAX_DISPATCH_PORT_PROBES).await {
                            Ok(stand_in_port) => {
                                let cancel = CancellationToken::new();
                                cancel.cancel();
                                slot.insert(ActiveVm {
                                    spec: command.spec.clone(),
                                    state: HostVmState::Stopping,
                                    boot_at: Utc::now(),
                                    ready_at: None,
                                    handle: Some(handle.clone()),
                                    dispatcher: None,
                                    dispatch_port: stand_in_port,
                                    dispatch_token: [0; 32],
                                    streams_open: Arc::new(AtomicU32::new(0)),
                                    cancel,
                                });
                            }
                            Err(error) => tracing::warn!(
                                %error,
                                vm_id = %command.vm_id,
                                "lost reservation cannot be re-held; only the root hold remains until proof"
                            ),
                        }
                    }
                    if let Some(record) = state.active.get_mut(&command.vm_id) {
                        record.handle = Some(handle.clone());
                        record.state = HostVmState::Stopping;
                    }
                    // The root stays held through the kill either way.
                    state.unreaped.insert(command.vm_id, command.workspace_id);
                    drop(state);
                    let receipt = self
                        .kill_with_proof(&handle, &format!("workspace VM boot ended by {reason}"))
                        .await
                        .ok();
                    if let Some(removed) = self.state.lock().await.active.remove(&command.vm_id) {
                        removed.cancel.cancel();
                    }
                    self.release_after_kill(command.workspace_id, command.vm_id, receipt.is_some())
                        .await;
                    // The stop that cancelled this boot completes through
                    // the proven exit, like every other stop
                    // (`g5.lifecycle.stop`).
                    if let Some(receipt) = receipt {
                        let _ = self
                            .journal
                            .record(
                                command.workspace_id,
                                command.vm_id,
                                VmEventKind::Exited {
                                    code: receipt.exit_code(),
                                    signal: None,
                                },
                                None,
                            )
                            .await;
                    }
                    return CommandResult::BootFailed {
                        reason: reason.to_string(),
                        log_tail: None,
                    };
                }
                let record = state
                    .active
                    .get_mut(&command.vm_id)
                    .expect("reservation present: checked above");
                record.handle = Some(handle.clone());
                record.dispatcher = dispatcher;
                record.state = HostVmState::Ready;
                let ready_at = Utc::now();
                record.ready_at = Some(ready_at);
                let summary = BootSummary {
                    boot_to_ready_ms: u64::try_from(
                        ready_at
                            .signed_duration_since(record.boot_at)
                            .num_milliseconds(),
                    )
                    .unwrap_or_default(),
                    restore_fetch_kib: restore.fetched_kib,
                    nested_virtualization: self.config.nested_virtualization,
                };
                drop(state);
                tracing::info!(
                    workspace_id = %command.workspace_id,
                    vm_id = %command.vm_id,
                    epoch = command.spec.incarnation.epoch,
                    boot_to_ready_ms = summary.boot_to_ready_ms,
                    restore_fetch_kib = summary.restore_fetch_kib,
                    nested_virtualization = summary.nested_virtualization,
                    "workspace VM ready"
                );
                if let Err(error) = self
                    .journal
                    .record(
                        command.workspace_id,
                        command.vm_id,
                        VmEventKind::Ready {
                            epoch: command.spec.incarnation.epoch,
                            module_catalog_version: command.spec.module_catalog_version.clone(),
                            dispatcher_contract_version: command.spec.dispatcher_contract_version,
                        },
                        Some(summary.to_string()),
                    )
                    .await
                {
                    tracing::error!(%error, vm_id = %command.vm_id, "failed to journal workspace VM ready event");
                    // A definite failure is release evidence for the control
                    // plane, so it may be reported only once exit is proven.
                    // `Busy` means a concurrent stop already owns the kill:
                    // join it instead of translating its progress into a
                    // failure (`g5.runtime.start.settlement`, G5-Lean#626 S11).
                    if self
                        .stop(VmStop {
                            workspace_id: command.workspace_id,
                            vm_id: command.vm_id,
                            mode: StopMode::Kill,
                            grace_secs: 0,
                        })
                        .await
                        == CommandResult::Busy
                    {
                        self.await_reservation_release(command.vm_id).await;
                    }
                    return CommandResult::BootFailed {
                        reason: "event_journal_unavailable".to_string(),
                        log_tail: None,
                    };
                }
                self.spawn_exit_monitor(command.workspace_id, command.vm_id, handle);
                CommandResult::Started
            }
            Err(error) => {
                let stopped_during_boot = state
                    .active
                    .remove(&command.vm_id)
                    .is_some_and(|record| record.state == HostVmState::Stopping);
                let child_reaped = error
                    .downcast_ref::<CancelledChildReapUnconfirmed>()
                    .is_none();
                if !child_reaped {
                    tracing::warn!(
                        workspace_id = %command.workspace_id,
                        vm_id = %command.vm_id,
                        "workspace VM was not proven reaped; its root stays held until a later Start replaces the tree or the host restarts"
                    );
                    state.unreaped.insert(command.vm_id, command.workspace_id);
                }
                drop(state);
                self.roots.release_unheld(&self.state).await;
                let pressure =
                    g5d_foundation::disk::classify_io(error.as_ref(), "image_preparation")
                        .or_else(|| {
                            error
                                .downcast_ref::<CancelledChildReapUnconfirmed>()
                                .and_then(|failure| {
                                    g5_contracts::error::HostDiskPressure::parse(&failure.detail)
                                })
                        })
                        .or_else(|| {
                            error
                                .downcast_ref::<DispatcherExitedBeforeReady>()
                                .and_then(|failure| failure.log_tail.as_deref())
                                .and_then(g5_contracts::error::HostDiskPressure::from_console_tail)
                        });
                let reason = pressure
                    .as_ref()
                    .map(|p| p.reason())
                    .unwrap_or_else(|| error.to_string());
                let log_tail = error
                    .downcast_ref::<DispatcherExitedBeforeReady>()
                    .and_then(|failure| failure.log_tail.clone());
                let mut facts = Vec::with_capacity(2);
                if !stopped_during_boot || !child_reaped {
                    facts.push((
                        VmEventKind::BootFailed {
                            reason: reason.clone(),
                        },
                        log_tail.clone(),
                    ));
                }
                if child_reaped {
                    facts.push((
                        VmEventKind::Exited {
                            code: None,
                            signal: None,
                        },
                        if stopped_during_boot {
                            log_tail.clone()
                        } else {
                            pressure.as_ref().map(|p| p.reason())
                        },
                    ));
                }
                if let Err(error) = self
                    .journal
                    .record_batch(command.workspace_id, command.vm_id, facts)
                    .await
                {
                    tracing::error!(%error, vm_id = %command.vm_id, "failed to journal workspace VM boot failure");
                }
                if pressure.is_some() {
                    if child_reaped {
                        CommandResult::SlotUnavailable {
                            reason: SlotUnavailableReason::Disk,
                        }
                    } else {
                        // Capacity pressure never authorizes releasing an
                        // unproven child or permanently failing Start. The
                        // journal/inventory reconciler retains this custody.
                        CommandResult::Accepted
                    }
                } else {
                    CommandResult::BootFailed { reason, log_tail }
                }
            }
        }
    }

    /// Kill a VM its boot abandons, retaining custody until exit is proven.
    async fn kill_abandoned(&self, handle: &RunHandle, what: &str) -> Result<()> {
        self.kill_with_proof(handle, what).await.map(|_| ())
    }

    /// Wait until no active record remains for `vm_id`: the stop that owns its
    /// cleanup removes the record only after its kill proved exit.
    async fn await_reservation_release(&self, vm_id: Uuid) {
        loop {
            let cancel = {
                let state = self.state.lock().await;
                match state.active.get(&vm_id) {
                    None => return,
                    Some(record) => record.cancel.clone(),
                }
            };
            tokio::select! {
                // Boot cancellation precedes exit proof. Once cancelled,
                // only the polling timer can wake this waiter until cleanup
                // releases the record; a ready token would otherwise spin.
                () = cancel.cancelled(), if !cancel.is_cancelled() => {}
                () = tokio::time::sleep(Duration::from_secs(1)) => {}
            }
        }
    }

    /// Retain the live reservation, handle, port and workspace exclusion until
    /// kill proves exit for every backend. Individual reap deadlines do not
    /// authorize release. Host shutdown keeps its existing bounded lifecycle;
    /// the next host settles any remaining process scope before admission.
    async fn kill_with_proof(
        &self,
        handle: &RunHandle,
        what: &str,
    ) -> Result<g5d_isolation::ChildReapReceipt> {
        let mut attempt = 0_u32;
        loop {
            match self.launcher.kill(handle).await {
                Ok(receipt) => return Ok(receipt),
                Err(error) => {
                    attempt += 1;
                    tracing::warn!(
                        handle = %handle.id,
                        attempt,
                        %error,
                        "{what}: exit not yet proven; custody is kept and proof is retried"
                    );
                    tokio::time::sleep(Duration::from_secs(2)).await;
                }
            }
        }
    }

    /// Finish a boot cleanup that held the root across its kill: drop this
    /// VM's hold once its kill proved reap; otherwise keep it.
    async fn release_after_kill(&self, workspace_id: Uuid, vm_id: Uuid, reaped: bool) {
        if reaped {
            self.state.lock().await.unreaped.remove(&vm_id);
        } else {
            tracing::warn!(
                %workspace_id,
                "workspace VM was not proven reaped; its root stays held until a later Start replaces the tree or the host restarts"
            );
        }
        self.roots.release_unheld(&self.state).await;
    }

    async fn boot_reserved(
        &self,
        command: &VmStart,
    ) -> Result<(Arc<RunHandle>, Option<MuxHandle>, RestoreStats)> {
        let (root, restore) = self
            .prepare_workspace_root(&command.spec, &command.workspace_token)
            .await?;
        let (handle, dispatcher) = self.boot_prepared(command, root).await?;
        Ok((handle, dispatcher, restore))
    }

    async fn boot_prepared(
        &self,
        command: &VmStart,
        mut root: PathBuf,
    ) -> Result<(Arc<RunHandle>, Option<MuxHandle>)> {
        let cancel = {
            let state = self.state.lock().await;
            let record = state
                .active
                .get(&command.vm_id)
                .ok_or_else(|| anyhow!("workspace VM reservation disappeared"))?;
            record.cancel.clone()
        };
        let scratch_disk = match &self.config.process_host_executable {
            None => (command.spec.size.scratch_disk_mib > 0).then_some(ScratchDiskSpec {
                size_mib: command.spec.size.scratch_disk_mib,
            }),
            Some(_) => {
                // The contract still carries the slot's scratch size, but a
                // host process uses the host's own disk and Docker.
                tracing::info!(
                    vm_id = %command.vm_id,
                    scratch_disk_mib = command.spec.size.scratch_disk_mib,
                    "process mode: scratch disk skipped (not applicable)"
                );
                None
            }
        };
        for attempt in 1..=MAX_DISPATCH_BOOT_ATTEMPTS {
            if cancel.is_cancelled() {
                return Err(anyhow!("boot_cancelled"));
            }
            let (dispatch_port, dispatch_token) = {
                let state = self.state.lock().await;
                let record = state
                    .active
                    .get(&command.vm_id)
                    .ok_or_else(|| anyhow!("workspace VM reservation disappeared"))?;
                (record.dispatch_port.port(), record.dispatch_token)
            };
            let run_spec = match &self.config.process_host_executable {
                None => MicrovmRunSpec {
                    image: command.spec.image.image_ref.clone(),
                    cmd: command.spec.entrypoint.clone(),
                    env: dispatch_guest_env(command, &self.config.api_url, dispatch_token),
                    mount_host_dir: root.clone(),
                    mount_guest_dir: "/work".to_string(),
                    extra_mounts: vec![MicrovmMount::read_only(
                        self.config.runtime_root.clone(),
                        "/opt/g5",
                    )],
                    cpus: command.spec.size.cpus,
                    mem_mib: command.spec.size.mem_mib,
                    port_maps: vec![(dispatch_port, DISPATCH_GUEST_PORT)],
                    scratch_disk: scratch_disk.clone(),
                    block_workload: self.config.block_workload,
                    nested_virtualization: self.config.nested_virtualization,
                    guest_restore: Some(Box::new(command.clone())),
                },
                // A host process dials the control plane and listens on the
                // host loopback itself: no mounts, port maps or forwards. The
                // API's entrypoint names the guest bundle; the host's own
                // executable stands in for it.
                Some(host_g5d) => MicrovmRunSpec {
                    image: String::new(),
                    cmd: vec![host_g5d.to_string_lossy().into_owned(), "dispatch".into()],
                    env: dispatch_process_env(
                        command,
                        &self.config.api_url,
                        dispatch_token,
                        dispatch_port,
                        &root,
                        &self.config.runtime_root,
                    ),
                    mount_host_dir: root.clone(),
                    mount_guest_dir: root.to_string_lossy().into_owned(),
                    extra_mounts: Vec::new(),
                    cpus: command.spec.size.cpus,
                    mem_mib: command.spec.size.mem_mib,
                    port_maps: Vec::new(),

                    scratch_disk: None,
                    block_workload: false,
                    nested_virtualization: false,
                    guest_restore: None,
                },
            };
            let handle = Arc::new(self.launcher.start(run_spec).await?);
            if cancel.is_cancelled() {
                self.kill_abandoned(&handle, "cancelled workspace VM boot")
                    .await?;
                return Err(anyhow!("boot_cancelled"));
            }
            if let Err(error) = self
                .journal
                .record(
                    command.workspace_id,
                    command.vm_id,
                    VmEventKind::Booted,
                    None,
                )
                .await
            {
                self.kill_abandoned(
                    &handle,
                    &format!("workspace VM after boot journal error: {error}"),
                )
                .await?;
                return Err(error);
            }
            if self.config.dispatcher_ready_timeout.is_zero() {
                return Ok((handle, None));
            }
            match self
                .wait_for_dispatcher_ready(&handle, dispatch_port, dispatch_token, command, &cancel)
                .await
            {
                Ok(dispatcher) => return Ok((handle, Some(dispatcher))),
                Err(error) => {
                    let retryable = error.is::<DispatcherReadyTimeout>()
                        && attempt < MAX_DISPATCH_BOOT_ATTEMPTS
                        && !cancel.is_cancelled();
                    // A new attempt may only start after positive child-reap evidence.
                    // Otherwise the old VMM could still own the rootfs or port.
                    self.kill_abandoned(&handle, &format!("unready workspace VM after {error}"))
                        .await?;
                    if !retryable {
                        return Err(error);
                    }
                    // The unready guest may already have written to /work.
                    // Restore the exact checkout before a new guest sees it.
                    // The retry's restore accounting is not part of the boot
                    // summary; the first preparation's numbers are reported.
                    root = self
                        .prepare_workspace_root(&command.spec, &command.workspace_token)
                        .await?
                        .0;
                    let next_port = self
                        .ports
                        .lease_bindable(MAX_DISPATCH_PORT_PROBES)
                        .await
                        .map_err(|reason| anyhow!("dispatcher retry port unavailable: {reason}"))?;
                    let mut next_token = [0_u8; 32];
                    getrandom(&mut next_token)
                        .map_err(|_| anyhow!("dispatcher_token_unavailable"))?;
                    let mut state = self.state.lock().await;
                    let record = state
                        .active
                        .get_mut(&command.vm_id)
                        .ok_or_else(|| anyhow!("workspace VM reservation disappeared"))?;
                    if record.cancel.is_cancelled() {
                        return Err(anyhow!("boot_cancelled"));
                    }
                    tracing::warn!(
                        vm_id = %command.vm_id,
                        attempt,
                        old_port = dispatch_port,
                        new_port = next_port.port(),
                        "dispatcher did not become ready; reaped the VM and retrying boot"
                    );
                    record.dispatch_port = next_port;
                    record.dispatch_token = next_token;
                }
            }
        }
        unreachable!("bounded boot loop always returns")
    }

    async fn wait_for_dispatcher_ready(
        &self,
        handle: &RunHandle,
        dispatch_port: u16,
        dispatch_token: [u8; 32],
        command: &VmStart,
        cancel: &CancellationToken,
    ) -> Result<MuxHandle> {
        let address = SocketAddrV4::new(
            handle
                .vmm_loopback_addr
                .ok_or_else(|| anyhow!("the launcher did not publish a dispatcher address"))?,
            dispatch_port,
        );
        let deadline = tokio::time::Instant::now() + self.config.dispatcher_ready_timeout;
        loop {
            if cancel.is_cancelled() {
                return Err(anyhow!("boot_cancelled"));
            }
            if let Some(receipt) = handle.current_reap_receipt()? {
                return Err(
                    DispatcherExitedBeforeReady::capture(handle, receipt.exit_code())
                        .await
                        .into(),
                );
            }
            if tokio::time::Instant::now() >= deadline {
                return Err(DispatcherReadyTimeout.into());
            }
            if let Ok(Ok(mut socket)) =
                tokio::time::timeout(Duration::from_secs(1), TcpStream::connect(address)).await
            {
                socket.write_all(&dispatch_token).await?;
                let mux = Mux::new(socket, true);
                let stream = mux.open(Channel::Rpc).await?;
                let (writer, mut reader) = stream.into_split();
                if let Ok(Some(line)) =
                    tokio::time::timeout(Duration::from_secs(2), reader.read_line()).await
                {
                    let ready: Value = serde_json::from_str(&line)?;
                    if dispatcher_ready_matches(
                        &ready,
                        command.workspace_id,
                        command.vm_id,
                        command.spec.incarnation.epoch,
                        &command.spec.module_catalog_version,
                        command.spec.dispatcher_contract_version,
                    ) {
                        let opener = mux.handle();
                        let renewals = self.lease_renewals.clone();
                        let journal = self.journal.clone();
                        let state = self.state.clone();
                        let workspace_id = command.workspace_id;
                        let vm_id = command.vm_id;
                        let epoch = command.spec.incarnation.epoch;
                        let module_catalog_version = command.spec.module_catalog_version.clone();
                        let dispatcher_contract_version = command.spec.dispatcher_contract_version;
                        let cancel = cancel.clone();
                        tokio::spawn(async move {
                            let mut mux = mux;
                            let mut writer = Mutex::new(writer);
                            loop {
                                while let Some(line) =
                                    reader.read_line_with_limit(4 * 1024 * 1024).await
                                {
                                    if !handle_dispatcher_control_message(
                                        &line,
                                        &writer,
                                        &renewals,
                                        &journal,
                                        workspace_id,
                                        vm_id,
                                        epoch,
                                    )
                                    .await
                                    {
                                        tracing::debug!("ignored unsolicited dispatcher frame on host control connection");
                                    }
                                }

                                drop(reader);
                                drop(writer);
                                drop(mux);
                                let should_reconnect = loop {
                                    let mut locked = state.lock().await;
                                    let Some(record) = locked.active.get_mut(&vm_id) else {
                                        break false;
                                    };
                                    if record.cancel.is_cancelled()
                                        || record.spec.incarnation.workspace_id != workspace_id
                                        || record.spec.incarnation.epoch != epoch
                                        || record.dispatch_token != dispatch_token
                                    {
                                        break false;
                                    }
                                    if record.state == HostVmState::Booting {
                                        drop(locked);
                                        tokio::time::sleep(Duration::from_millis(10)).await;
                                        continue;
                                    }
                                    record.dispatcher = None;
                                    break matches!(
                                        record.state,
                                        HostVmState::Ready | HostVmState::Running
                                    );
                                };
                                if !should_reconnect || cancel.is_cancelled() {
                                    return;
                                }

                                tracing::warn!(%workspace_id, %vm_id, %epoch, "workspace dispatcher mux disconnected; reconnecting");
                                loop {
                                    if cancel.is_cancelled() {
                                        return;
                                    }
                                    if let Ok(Ok(mut socket)) = tokio::time::timeout(
                                        Duration::from_secs(1),
                                        TcpStream::connect(address),
                                    )
                                    .await
                                    {
                                        if socket.write_all(&dispatch_token).await.is_ok() {
                                            let next_mux = Mux::new(socket, true);
                                            if let Ok(stream) = next_mux.open(Channel::Rpc).await {
                                                let (next_writer, mut next_reader) =
                                                    stream.into_split();
                                                if let Ok(Some(line)) = tokio::time::timeout(
                                                    Duration::from_secs(2),
                                                    next_reader
                                                        .read_line_with_limit(4 * 1024 * 1024),
                                                )
                                                .await
                                                {
                                                    let ready =
                                                        serde_json::from_str::<Value>(&line);
                                                    if ready.as_ref().is_ok_and(|ready| {
                                                        dispatcher_ready_matches(
                                                            ready,
                                                            workspace_id,
                                                            vm_id,
                                                            epoch,
                                                            &module_catalog_version,
                                                            dispatcher_contract_version,
                                                        )
                                                    }) {
                                                        let next_opener = next_mux.handle();
                                                        let accepted = {
                                                            let mut locked = state.lock().await;
                                                            locked.active.get_mut(&vm_id).is_some_and(
                                                                |record| {
                                                                    if !record.cancel.is_cancelled()
                                                                        && record.spec.incarnation.workspace_id
                                                                            == workspace_id
                                                                        && record.spec.incarnation.epoch == epoch
                                                                        && record.dispatch_token == dispatch_token
                                                                        && matches!(
                                                                            record.state,
                                                                            HostVmState::Ready
                                                                                | HostVmState::Running
                                                                        )
                                                                    {
                                                                        record.dispatcher =
                                                                            Some(next_opener);
                                                                        true
                                                                    } else {
                                                                        false
                                                                    }
                                                                },
                                                            )
                                                        };
                                                        if !accepted {
                                                            return;
                                                        }
                                                        mux = next_mux;
                                                        writer = Mutex::new(next_writer);
                                                        reader = next_reader;
                                                        tracing::info!(%workspace_id, %vm_id, %epoch, "workspace dispatcher mux reconnected");
                                                        break;
                                                    }
                                                }
                                            }
                                        }
                                    }
                                    tokio::time::sleep(Duration::from_millis(100)).await;
                                }
                            }
                        });
                        return Ok(opener);
                    }
                    return Err(anyhow!("dispatcher_runtime_mismatch"));
                }
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }

    async fn prepare_repository_seed(
        &self,
        spec: &VmSpec,
        workspace_token: &str,
        root: &Path,
    ) -> Result<bool> {
        let scope = g5d_isolation::isolation::WorkloadRepoScope {
            org_id: spec.incarnation.org_id,
            repos: spec
                .repos
                .iter()
                .map(|repo| g5d_isolation::isolation::WorkloadRepoMember {
                    repo_id: repo.repo_id,
                    root_path: repo.root_path.clone(),
                    git_url: repo.git_url.clone(),
                })
                .collect(),
        };
        let template = match g5d_isolation::microvm::eligible_repository_template(
            &spec.image.image_ref,
            spec.size.scratch_disk_mib,
            &scope,
        )
        .await
        {
            Ok(template) => template,
            Err(error) => {
                if g5d_foundation::disk::classify_io(error.as_ref(), "repository_seed_selection")
                    .is_some()
                {
                    return Err(error);
                }
                tracing::warn!(%error, "repository seed unavailable; using ordinary guest restore");
                None
            }
        };
        let Some(template) = template else {
            return Ok(false);
        };
        let repos = spec.repos.iter().map(repo_info).collect::<Vec<_>>();
        let stores = self
            .checkout
            .authorized_repository_stores(&repos, workspace_token)
            .await?;
        self.checkout
            .prepare_metadata_only(spec.incarnation.workspace_id)
            .await?;
        match g5d_isolation::microvm::pin_repository_template(
            template,
            root.to_owned(),
            spec.size.scratch_disk_mib,
            scope,
            stores,
        )
        .await
        {
            Ok(pinned) => Ok(pinned),
            Err(error) => {
                if g5d_foundation::disk::classify_io(error.as_ref(), "repository_seed_pin")
                    .is_some()
                {
                    return Err(error);
                }
                tracing::warn!(%error, "repository seed pin unavailable; using ordinary guest restore");
                Ok(false)
            }
        }
    }

    async fn prepare_workspace_root(
        &self,
        spec: &VmSpec,
        workspace_token: &str,
    ) -> Result<(PathBuf, RestoreStats)> {
        let root = self
            .config
            .workspaces_root
            .join(spec.incarnation.workspace_id.to_string());
        // A seed hit creates only host metadata and a private immutable seed pin.
        // Each member is reauthorized by refreshing the independent bare cache.
        // A seed holds members only; the guest fetches reference checkouts,
        // the owner's personal mount included, after restoring them
        // (`restore_references_with_git`).
        if self.config.block_workload && self.config.process_host_executable.is_none() {
            match self
                .prepare_repository_seed(spec, workspace_token, &root)
                .await
            {
                Ok(true) => {
                    tracing::info!(workspace_id = %spec.incarnation.workspace_id, "repository seed pinned; exact restore will run in guest");
                    self.state
                        .lock()
                        .await
                        .unreaped
                        .retain(|_, held| *held != spec.incarnation.workspace_id);
                    return Ok((root, RestoreStats::default()));
                }
                Ok(false) => (),
                Err(error) => {
                    let pressure = g5d_foundation::disk::classify_io(
                        error.as_ref(),
                        "repository_seed_preparation",
                    );
                    // Git can remove its partial pack before returning, making
                    // a subsequent local capacity probe succeed. An opaque
                    // cache-refresh exit is only an unavailable accelerator;
                    // it is NOT capacity evidence. The ordinary trusted guest
                    // path still proves authorization, source and exact receipt.
                    let opaque_refresh = error.downcast_ref::<g5d_foundation::workspace::WorkspaceError>()
                        .is_some_and(|error| matches!(error,
                            g5d_foundation::workspace::WorkspaceError::GitExecution { source }
                                if matches!(source.kind, g5_scm::native::FailureKind::Exit | g5_scm::native::FailureKind::Io)));
                    if pressure.is_none() && !opaque_refresh {
                        return Err(error);
                    }
                    // Every awaited Git/cache owner has completed its cleanup.
                    // Use the normal trusted restore into the private disk.
                    tracing::warn!(%error, ?pressure, "optional repository seed refresh unavailable; using ordinary guest restore");
                }
            }
            // Generic images restore directly into the private V-sized disk using
            // the same trusted SCM bootstrap as seed hits. No duplicate checkout.
            self.checkout
                .prepare_metadata_only(spec.incarnation.workspace_id)
                .await?;
            return Ok((root, RestoreStats::default()));
        }
        // A tree left at this path was writable by an earlier guest, including
        // each repository's .git/config, hooks and attributes. Never execute
        // host Git in it: materialize a fresh tree from the host-owned bare
        // cache, then restore the durable receipt into it
        // (`g5.runtime.materialization`).
        let repos = spec
            .repos
            .iter()
            .map(repo_info)
            .chain(spec.reference_repos.iter().map(reference_repo_info))
            .collect::<Vec<_>>();
        self.checkout
            .materialize_with_git_token(spec.incarnation.workspace_id, &repos, workspace_token)
            .await
            .context("materialize workspace repository vector")?;
        // The materializer swapped a new tree into the workspace path; the old
        // tree is gone from it (deleted, or left as a `.previous-*` copy the
        // reaper claims once no VM holds this workspace), so an unreaped VMM's
        // hold on the path protects nothing any more.
        let workspace_id = spec.incarnation.workspace_id;
        self.state
            .lock()
            .await
            .unreaped
            .retain(|_, held| *held != workspace_id);
        let g5_dir = root.join(".g5");
        tokio::fs::create_dir_all(&g5_dir).await?;
        atomic_write(
            &g5_dir.join("layout.json"),
            &serde_json::to_vec(&if spec.reference_repos.is_empty() {
                json!({
                    "repos": spec.repos,
                    "restore": spec.restore,
                })
            } else {
                json!({
                    "repos": spec.repos,
                    "restore": spec.restore,
                    "references": spec.reference_repos,
                })
            })?,
        )
        .await?;
        reject_symlink_roots(&root, &spec.repos).await?;
        reject_symlink_reference_roots(&root, &spec.reference_repos).await?;
        let restore = restore_workspace(&root, spec, workspace_token).await?;
        restore_references(&root, spec, workspace_token).await?;
        if self.config.process_host_executable.is_some() {
            // Host processes run under the developer's identity; there is no
            // module identity to hand the tree to, even on a root host.
            tracing::debug!(
                workspace_id = %spec.incarnation.workspace_id,
                "process mode: workspace root ownership handoff skipped (not applicable)"
            );
        } else {
            own_workspace_root(&root, spec).await?;
        }
        Ok((root, restore))
    }

    fn validate_start(&self, command: &VmStart) -> std::result::Result<(), SpecRejectReason> {
        let spec = &command.spec;
        if spec.spec_version != VM_SPEC_VERSION {
            return Err(SpecRejectReason::UnsupportedSpecVersion);
        }
        if spec.incarnation.org_id != self.config.org_id {
            return Err(SpecRejectReason::OrgMismatch);
        }
        if self.config.block_workload {
            if !cfg!(target_os = "linux") || self.config.process_host_executable.is_some() {
                return Err(SpecRejectReason::RuntimeMismatch);
            }
            if self.config.scratch_disk_mib == 0 {
                return Err(SpecRejectReason::SizeMismatch);
            }
        }
        if spec.dispatcher_contract_version
            != g5_internal_api::host_ctl::DISPATCHER_CONTRACT_VERSION
        {
            tracing::warn!(
                received = spec.dispatcher_contract_version,
                expected = g5_internal_api::host_ctl::DISPATCHER_CONTRACT_VERSION,
                "dispatcher contract mismatch"
            );
            return Err(SpecRejectReason::RuntimeMismatch);
        }
        if spec.incarnation.workspace_id != command.workspace_id
            || spec.incarnation.vm_id != command.vm_id
            || spec.incarnation.epoch == 0
            || spec.incarnation.lease_ttl_secs == 0
            || command.workspace_token.is_empty()
        {
            return Err(SpecRejectReason::IdentityMismatch);
        }
        if spec.size.cpus != self.config.cpus_per_vm
            || spec.size.mem_mib != self.config.mem_mib_per_vm
            || spec.size.scratch_disk_mib != self.config.scratch_disk_mib
        {
            return Err(SpecRejectReason::SizeMismatch);
        }
        if spec.repos.is_empty()
            || !valid_repo_vector(&spec.repos)
            || !valid_restore_vector(spec)
            || !valid_reference_vector(spec)
        {
            return Err(SpecRejectReason::InvalidPath);
        }
        if spec.entrypoint != ["/opt/g5/bin/g5d", "dispatch"] {
            return Err(SpecRejectReason::RuntimeMismatch);
        }
        if spec.env.iter().any(|(key, value)| {
            !ALLOWED_ENV_KEYS.contains(&key.as_str())
                || value.len() > MAX_ENV_VALUE_BYTES
                || value.contains('\0')
        }) {
            return Err(SpecRejectReason::CredentialInEnv);
        }
        if spec.image.digest.len() != 71
            || !spec.image.digest.starts_with("sha256:")
            || !spec.image.digest[7..]
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit())
            || !spec
                .image
                .image_ref
                .ends_with(&format!("@{}", spec.image.digest))
        {
            return Err(SpecRejectReason::ImageDigestMismatch);
        }
        match self.runtime_identity() {
            Some(identity)
                if identity.module_catalog_version == spec.module_catalog_version
                    && identity
                        .module_catalog_hash
                        .eq_ignore_ascii_case(&spec.module_catalog_hash) => {}
            other => {
                tracing::warn!(
                    expected_version = %spec.module_catalog_version,
                    expected_hash = %spec.module_catalog_hash,
                    local_catalog = ?other,
                    "workspace runtime catalog mismatch"
                );
                return Err(SpecRejectReason::RuntimeMismatch);
            }
        }
        Ok(())
    }

    pub async fn stop(&self, command: VmStop) -> CommandResult {
        let cleanup = {
            let mut state = self.state.lock().await;
            self.admit_stop(&mut state, command, false)
        };
        match cleanup {
            Ok(cleanup) => cleanup.await.expect("VM stop owner panicked"),
            Err(result) => result,
        }
    }

    /// Install the cleanup owner before releasing the reservation lock. Dropping
    /// its join handle detaches it: cancellation of a caller cannot strand a
    /// Stopping reservation. Boot cancellation already has its own cleanup owner.
    fn admit_stop(
        &self,
        state: &mut RuntimeState,
        command: VmStop,
        stale: bool,
    ) -> std::result::Result<tokio::task::JoinHandle<CommandResult>, CommandResult> {
        if command.grace_secs > MAX_STOP_GRACE_SECS {
            return Err(CommandResult::SpecRejected {
                reason: SpecRejectReason::IdentityMismatch,
            });
        }
        let Some(record) = state.active.get_mut(&command.vm_id) else {
            if let Some(pending) = state.pending_starts.get_mut(&command.vm_id) {
                if pending.workspace_id == command.workspace_id {
                    pending.cancelled = true;
                }
            }
            return Err(CommandResult::NotFound);
        };
        if record.spec.incarnation.workspace_id != command.workspace_id {
            return Err(CommandResult::NotFound);
        }
        if record.state == HostVmState::Stopping {
            return Err(CommandResult::Busy);
        }
        record.state = HostVmState::Stopping;
        let Some(handle) = record.handle.clone() else {
            record.cancel.cancel();
            return Err(CommandResult::Accepted);
        };
        let launcher = self.launcher.clone();
        let state = self.state.clone();
        let roots = self.roots.clone();
        let journal = self.journal.clone();
        Ok(tokio::spawn(async move {
            // Every backend retains its slot, port and exclusion until exit is
            // proven. An individual reap deadline never authorizes release.
            let receipt = loop {
                match launcher
                    .stop(
                        &handle,
                        command.mode.clone(),
                        Duration::from_secs(command.grace_secs),
                    )
                    .await
                {
                    Err(error) => {
                        tracing::warn!(
                            workspace_id = %command.workspace_id,
                            vm_id = %command.vm_id,
                            %error,
                            "workspace stop: exit not yet proven; custody is kept and proof is retried"
                        );
                        tokio::time::sleep(Duration::from_secs(2)).await;
                    }
                    Ok(receipt) => break receipt,
                }
            };
            let exit = VmExit {
                code: receipt.exit_code(),
                signal: None,
            };
            let (removed, replaced) = {
                let mut state = state.lock().await;
                let current = state
                    .active
                    .get(&command.vm_id)
                    .and_then(|record| record.handle.as_ref())
                    .is_some_and(|current| Arc::ptr_eq(current, &handle));
                let replaced = !current && state.active.contains_key(&command.vm_id);
                (
                    current
                        .then(|| state.active.remove(&command.vm_id))
                        .flatten(),
                    replaced,
                )
            };
            let mut facts = Vec::new();
            if let Some(removed) = removed {
                removed.cancel.cancel();
                roots.release_unheld(&state).await;
                facts.push((
                    VmEventKind::Exited {
                        code: exit.code,
                        signal: exit.signal,
                    },
                    None,
                ));
            }
            // Status or the natural-exit monitor may already have journaled
            // Exited. Retain the stale-cleanup fact, without touching a newer
            // reservation or publishing a second exit for the original one.
            if stale && !replaced {
                facts.push((VmEventKind::StaleKilled, None));
            }
            if !facts.is_empty() {
                let _ = journal
                    .record_batch(command.workspace_id, command.vm_id, facts)
                    .await;
            }
            CommandResult::Stopped { exit }
        }))
    }

    pub async fn status(&self, command: VmStatus) -> CommandResult {
        let reaped = {
            let state = self.state.lock().await;
            let Some(record) = state.active.get(&command.vm_id) else {
                // A pending Start is reported as booting until it settles; a
                // cancelled one is absent for good.
                if let Some(report) = pending_status_report(&state, command.vm_id)
                    .filter(|report| report.workspace_id == command.workspace_id)
                {
                    return CommandResult::Status { vm: report };
                }
                return CommandResult::NotFound;
            };
            if record.spec.incarnation.workspace_id != command.workspace_id {
                return CommandResult::NotFound;
            }
            record
                .handle
                .as_ref()
                .and_then(|handle| handle.current_reap_receipt().ok().flatten())
        };
        if let Some(receipt) = reaped {
            return self
                .settle_natural_exit(command.workspace_id, command.vm_id, receipt.exit_code())
                .await;
        }
        let state = self.state.lock().await;
        let Some(record) = state.active.get(&command.vm_id) else {
            return CommandResult::NotFound;
        };
        CommandResult::Status {
            vm: status_report(record),
        }
    }

    async fn settle_natural_exit(
        &self,
        workspace_id: Uuid,
        vm_id: Uuid,
        code: Option<i32>,
    ) -> CommandResult {
        let removed = self.state.lock().await.active.remove(&vm_id);
        let Some(removed) = removed else {
            return CommandResult::NotFound;
        };
        removed.cancel.cancel();
        self.roots.release_unheld(&self.state).await;
        let exit = VmExit { code, signal: None };
        let _ = self
            .journal
            .record(
                workspace_id,
                vm_id,
                VmEventKind::Exited { code, signal: None },
                None,
            )
            .await;
        CommandResult::Stopped { exit }
    }

    fn spawn_exit_monitor(&self, workspace_id: Uuid, vm_id: Uuid, handle: Arc<RunHandle>) {
        let state = self.state.clone();
        let journal = self.journal.clone();
        let roots = self.roots.clone();
        tokio::spawn(async move {
            loop {
                match handle.current_reap_receipt() {
                    Ok(Some(receipt)) => {
                        let removed = {
                            let mut state = state.lock().await;
                            let current = state
                                .active
                                .get(&vm_id)
                                .and_then(|record| record.handle.as_ref())
                                .is_some_and(|current| Arc::ptr_eq(current, &handle));
                            let removed = current.then(|| state.active.remove(&vm_id)).flatten();
                            if let Some(removed) = &removed {
                                removed.cancel.cancel();
                            }
                            removed.is_some()
                        };
                        if removed {
                            roots.release_unheld(&state).await;
                            let _ = journal
                                .record(
                                    workspace_id,
                                    vm_id,
                                    VmEventKind::Exited {
                                        code: receipt.exit_code(),
                                        signal: None,
                                    },
                                    None,
                                )
                                .await;
                        }
                        break;
                    }
                    Ok(None) => tokio::time::sleep(Duration::from_secs(1)).await,
                    Err(error) => {
                        tracing::error!(%workspace_id, %vm_id, %error, "workspace VM exit monitor failed");
                        break;
                    }
                }
            }
        });
    }

    pub async fn endpoint(&self, vm_id: Uuid) -> Option<DispatcherEndpoint> {
        let state = self.state.lock().await;
        let record = state.active.get(&vm_id)?;
        if !matches!(record.state, HostVmState::Ready | HostVmState::Running) {
            return None;
        }
        let mux = record.dispatcher.clone()?;
        Some(DispatcherEndpoint {
            mux,
            streams_open: record.streams_open.clone(),
            workspace_id: record.spec.incarnation.workspace_id,
            epoch: record.spec.incarnation.epoch,
        })
    }

    /// Every incarnation the host holds: active records and Starts received
    /// but not yet reserved, which the control plane must not read as absent
    /// (`g5.runtime.start.settlement`).
    pub async fn inventory(&self) -> Vec<VmInventory> {
        let state = self.state.lock().await;
        state
            .active
            .values()
            .map(|record| VmInventory {
                workspace_id: record.spec.incarnation.workspace_id,
                vm_id: record.spec.incarnation.vm_id,
                state: record.state,
                boot_at: record.boot_at,
            })
            .chain(
                pending_inventory(&state).map(|(vm_id, pending)| VmInventory {
                    workspace_id: pending.workspace_id,
                    vm_id,
                    state: HostVmState::Booting,
                    boot_at: pending.received_at,
                }),
            )
            .collect()
    }

    /// Release every root no VM holds: at host start that is every root,
    /// because no VM survives the host process (`g5.runtime.purge`).
    pub async fn release_unheld_roots(&self) {
        self.roots.release_unheld(&self.state).await;
    }

    pub async fn active_count(&self) -> usize {
        self.state.lock().await.active.len()
    }

    pub async fn boots_in_progress(&self) -> u32 {
        u32::try_from(self.state.lock().await.boots_in_progress).unwrap_or(u32::MAX)
    }

    /// A partial heartbeat lists only active records, never pending Starts.
    /// The control plane merges a heartbeat's live ids into `reported_live_vm_ids`
    /// and clears an id only on explicit exit evidence (`g5.runtime.host.inventory`);
    /// a pending Start that is then refused or cancelled never emits an exit, so
    /// advertising it here would leave phantom occupancy until the next complete
    /// hello. Pending-Start custody is carried by the complete [`inventory`] hello
    /// and by `vm.status`, not the heartbeat (G5-Lean#626, Codex P2).
    pub async fn heartbeat_vms(&self) -> Vec<g5_internal_api::host_ctl::VmHeartbeat> {
        self.state
            .lock()
            .await
            .active
            .values()
            .map(|record| g5_internal_api::host_ctl::VmHeartbeat {
                vm_id: record.spec.incarnation.vm_id,
                state: record.state,
                cpu_pct: 0.0,
                mem_mib: u64::from(record.spec.size.mem_mib),
            })
            .collect()
    }

    /// Admit cleanup for the complete roster without waiting for exit proof.
    /// All admissions occur under one lock, so cancellation cannot apply only a
    /// prefix after the caller has recorded this generation as reconciled.
    pub async fn reconcile(&self, current: &[CurrentIncarnation]) {
        let allowed = current
            .iter()
            .map(|incarnation| (incarnation.vm_id, incarnation.epoch))
            .collect::<BTreeSet<_>>();
        let mut state = self.state.lock().await;
        let stale = state
            .active
            .values()
            .filter(|record| {
                !allowed.contains(&(record.spec.incarnation.vm_id, record.spec.incarnation.epoch))
            })
            .map(|record| {
                (
                    record.spec.incarnation.workspace_id,
                    record.spec.incarnation.vm_id,
                )
            })
            .collect::<Vec<_>>();
        for (workspace_id, vm_id) in stale {
            // Each owner survives this method and its caller. A stop already
            // admitted by a command or an older roster keeps its owner.
            let _ = self.admit_stop(
                &mut state,
                VmStop {
                    workspace_id,
                    vm_id,
                    mode: StopMode::Kill,
                    grace_secs: 0,
                },
                true,
            );
        }
    }

    pub async fn shutdown(&self) {
        self.source_preparation.stop().await;
        let active = self
            .state
            .lock()
            .await
            .active
            .values()
            .map(|record| {
                (
                    record.spec.incarnation.workspace_id,
                    record.spec.incarnation.vm_id,
                )
            })
            .collect::<Vec<_>>();
        // All at once and bounded: a process-mode stop retries until exit is
        // proven, and one unproven incarnation must neither hold the others
        // back nor keep the host from ending. What is still unproven when
        // the deadline passes stays recorded, with no exit fact published,
        // for the next host start to settle.
        let stops = active.iter().map(|&(workspace_id, vm_id)| async move {
            let _ = self
                .stop(VmStop {
                    workspace_id,
                    vm_id,
                    mode: StopMode::Kill,
                    grace_secs: 0,
                })
                .await;
            // A command or reconciliation may already own this stop, or its
            // boot may still be cancelling. Join that custody within the same
            // shutdown deadline instead of treating Busy/Accepted as exit.
            self.await_reservation_release(vm_id).await;
        });
        if tokio::time::timeout(SHUTDOWN_DEADLINE, futures_util::future::join_all(stops))
            .await
            .is_err()
        {
            tracing::warn!(
                incarnations = ?active,
                deadline_secs = SHUTDOWN_DEADLINE.as_secs(),
                "host shutdown: exit not proven for every incarnation before the deadline; the next host settles what is left"
            );
        }
    }
}

/// How long a host shutdown waits for its incarnations' exit proof.
const SHUTDOWN_DEADLINE: Duration = Duration::from_secs(30);

/// Pending Starts that still count as held incarnations: not cancelled and
/// not yet reserved (a reserved one answers through its active record).
fn pending_inventory(state: &RuntimeState) -> impl Iterator<Item = (Uuid, &PendingStart)> {
    state
        .pending_starts
        .iter()
        .filter(|(vm_id, pending)| !pending.cancelled && !state.active.contains_key(*vm_id))
        .map(|(vm_id, pending)| (*vm_id, pending))
}

/// The booting report for a pending, uncancelled Start.
fn pending_status_report(state: &RuntimeState, vm_id: Uuid) -> Option<VmStatusReport> {
    let pending = state.pending_starts.get(&vm_id)?;
    if pending.cancelled {
        return None;
    }
    Some(VmStatusReport {
        workspace_id: pending.workspace_id,
        vm_id,
        epoch: pending.epoch,
        state: HostVmState::Booting,
        boot_at: pending.received_at,
        ready_at: None,
        resources: pending.size.clone(),
        streams_open: 0,
    })
}

fn status_report(record: &ActiveVm) -> VmStatusReport {
    VmStatusReport {
        workspace_id: record.spec.incarnation.workspace_id,
        vm_id: record.spec.incarnation.vm_id,
        epoch: record.spec.incarnation.epoch,
        state: record.state,
        boot_at: record.boot_at,
        ready_at: record.ready_at,
        resources: record.spec.size.clone(),
        streams_open: record.streams_open.load(Ordering::Acquire),
    }
}

/// The guest dispatcher's boot environment: the incarnation identity on the
/// fixed guest port plus the mounted bundle's loader path.
fn dispatch_guest_env(
    command: &VmStart,
    api_url: &str,
    dispatch_token: [u8; 32],
) -> Vec<(String, String)> {
    let guest_url = g5d_isolation::isolation::passt::rewrite_loopback_url_for_door(api_url)
        .unwrap_or_else(|| api_url.to_owned());
    let mut env = dispatch_identity_env(command, &guest_url, dispatch_token, DISPATCH_GUEST_PORT);
    // The guest g5d's Linux shared libraries live beside it in the mounted
    // runtime bundle. Set this as host-owned boot configuration, after image
    // and control-plane values, so an OCI image cannot redirect the loader.
    set_env(
        &mut env,
        "LD_LIBRARY_PATH",
        GUEST_RUNTIME_LIBRARY_PATH.to_string(),
    );
    env
}

/// The process-mode dispatcher's boot environment
/// (`g5.runtime.boot.backend.process-local`): the same identity on the leased
/// host port, told it runs in process mode with the checkout and the
/// host-native runtime directory as its roots. Guest-only entries are
/// removed explicitly: the guest hostname (a host process never renames its
/// host), the module identity (no uid handoff on a host process), the bundle
/// loader path, and image-declared home and command paths that name guest
/// locations. The Git door takes an ephemeral port so
/// a second incarnation on the same host cannot collide with it. The host's
/// own command path is what the dispatcher appends its runtime bin to.
fn dispatch_process_env(
    command: &VmStart,
    api_url: &str,
    dispatch_token: [u8; 32],
    dispatch_port: u16,
    workspace_root: &Path,
    runtime_root: &Path,
) -> Vec<(String, String)> {
    let mut env = dispatch_identity_env(command, api_url, dispatch_token, dispatch_port);
    env.retain(|(key, _)| {
        !matches!(
            key.as_str(),
            "G5D_WORKSPACE_HOSTNAME"
                | "G5D_MODULE_UID"
                | "G5D_MODULE_GID"
                | "LD_LIBRARY_PATH"
                | "HOME"
                | "PATH"
                | "CLAUDE_CONFIG_DIR"
                | "CODEX_HOME"
        )
    });
    set_env(&mut env, "G5D_ISOLATION_MODE", "process".to_string());
    set_env(
        &mut env,
        "G5D_PROCESS_WORKSPACE_ROOT",
        workspace_root.to_string_lossy().into_owned(),
    );
    set_env(
        &mut env,
        "G5D_PROCESS_RUNTIME_ROOT",
        runtime_root.to_string_lossy().into_owned(),
    );
    set_env(&mut env, "G5D_GIT_DOOR_PORT", "0".to_string());
    set_env(
        &mut env,
        "PATH",
        std::env::var("PATH").unwrap_or_else(|_| "/usr/local/bin:/usr/bin:/bin".to_string()),
    );
    for key in ["LANG", "LC_ALL", "LC_CTYPE", "TERM"] {
        if let Ok(value) = std::env::var(key) {
            set_env(&mut env, key, value);
        }
    }
    host_docker_selection(&mut env, |key| std::env::var_os(key));
    env
}

/// Carry the developer's Docker connection selection into the incarnation
/// (`g5.runtime.boot.backend.process-local`): the explicit `DOCKER_*`
/// variables, and, when no config directory is named, the host home's
/// `.docker` so a selected context and its credentials still apply under the
/// incarnation's own home. Nothing else of the host environment crosses.
fn host_docker_selection(
    env: &mut Vec<(String, String)>,
    lookup: impl Fn(&str) -> Option<std::ffi::OsString>,
) {
    for key in g5d_foundation::runtime_environment::HOST_DOCKER_ENV {
        if let Some(value) = lookup(key).filter(|value| !value.is_empty()) {
            set_env(env, key, value.to_string_lossy().into_owned());
        }
    }
    if !env.iter().any(|(key, _)| key == "DOCKER_CONFIG") {
        if let Some(home) = lookup("HOME").filter(|value| !value.is_empty()) {
            let config = Path::new(&home).join(".docker");
            if config.is_dir() {
                set_env(env, "DOCKER_CONFIG", config.to_string_lossy().into_owned());
            }
        }
    }
}

/// The incarnation identity every dispatcher boot receives, on the given
/// dispatcher port.
fn dispatch_identity_env(
    command: &VmStart,
    api_url: &str,
    dispatch_token: [u8; 32],
    dispatch_port: u16,
) -> Vec<(String, String)> {
    let mut env = command
        .spec
        .env
        .iter()
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect::<Vec<_>>();
    set_env(
        &mut env,
        "G5D_WORKSPACE_ID",
        command.workspace_id.to_string(),
    );
    set_env(&mut env, "G5D_VM_ID", command.vm_id.to_string());
    set_env(
        &mut env,
        "G5D_OWNER_USER_ID",
        command.spec.incarnation.owner_user_id.to_string(),
    );
    set_env(
        &mut env,
        "G5D_EPOCH",
        command.spec.incarnation.epoch.to_string(),
    );
    set_env(&mut env, "G5D_API_URL", api_url.to_string());
    set_env(&mut env, "G5D_DISPATCH_PORT", dispatch_port.to_string());
    set_env(&mut env, "G5D_DISPATCH_TOKEN", hex::encode(dispatch_token));
    set_env(
        &mut env,
        "G5D_WORKSPACE_TOKEN",
        command.workspace_token.clone(),
    );
    set_env(
        &mut env,
        "G5D_LEASE_TTL_SECS",
        command.spec.incarnation.lease_ttl_secs.to_string(),
    );
    set_env(
        &mut env,
        "G5D_MODULE_CATALOG_HASH",
        command.spec.module_catalog_hash.clone(),
    );
    // Deterministic provider mode is a host deployment choice. Carry only
    // its explicit, non-secret test controls into the guest; arbitrary host
    // environment still never crosses the microVM boundary.
    for key in FAKE_PROVIDER_ENV {
        if let Ok(value) = std::env::var(key) {
            set_env(&mut env, key, value);
        }
    }
    env
}

fn set_env(env: &mut Vec<(String, String)>, key: &str, value: String) {
    if let Some((_, existing)) = env.iter_mut().find(|(existing, _)| existing == key) {
        *existing = value;
    } else {
        env.push((key.to_string(), value));
    }
}

fn dispatcher_ready_matches(
    ready: &Value,
    workspace_id: Uuid,
    vm_id: Uuid,
    epoch: u64,
    module_catalog_version: &str,
    dispatcher_contract_version: u32,
) -> bool {
    ready.get("method").and_then(Value::as_str) == Some("runtime.ready")
        && ready["params"]["workspace_id"].as_str() == Some(workspace_id.to_string().as_str())
        && ready["params"]["vm_id"].as_str() == Some(vm_id.to_string().as_str())
        && ready["params"]["epoch"].as_u64() == Some(epoch)
        && ready["params"]["module_catalog_version"].as_str() == Some(module_catalog_version)
        && ready["params"]["dispatcher_contract_version"].as_u64()
            == Some(u64::from(dispatcher_contract_version))
}

fn repo_info(repo: &VmRepoSpec) -> RepoInfo {
    RepoInfo {
        repo_id: repo.repo_id.to_string(),
        name: repo.repo_id.to_string(),
        root_path: Some(repo.root_path.clone()),
        root_paths: Vec::new(),
        // The spec's membership role is authoritative: the docs repository
        // gets the workspace's `project_docs` alias at materialization.
        repo_role: match repo.role.as_deref() {
            Some("docs") => RepoRole::Docs,
            Some("code") => RepoRole::Code,
            _ => RepoRole::Unknown,
        },
        git_url: Some(repo.git_url.clone()),
        branch_name: Some(repo.branch.clone()),
        base_commit: Some(repo.branch_oid.clone()),
    }
}

/// A reference checkout as the materializer sees it: the pinned Base commit
/// on Base's branch name. Its role is withheld so the docs reference gets no
/// `project_docs` alias; a personal workspace has no Project Docs folder.
fn reference_repo_info(repo: &VmReferenceRepoSpec) -> RepoInfo {
    RepoInfo {
        repo_id: repo.repo_id.to_string(),
        name: repo.repo_id.to_string(),
        root_path: Some(repo.root_path.clone()),
        root_paths: Vec::new(),
        repo_role: RepoRole::Unknown,
        git_url: Some(repo.git_url.clone()),
        branch_name: Some(repo.branch.clone()),
        base_commit: Some(repo.commit.clone()),
    }
}

/// Reference checkouts are distinct from members and from each other, with
/// normalized, nonoverlapping roots, valid branches and exact commits.
fn valid_reference_vector(spec: &VmSpec) -> bool {
    let mut ids = spec
        .repos
        .iter()
        .map(|repo| repo.repo_id)
        .collect::<BTreeSet<_>>();
    let mut roots = spec
        .repos
        .iter()
        .map(|repo| Path::new(repo.root_path.as_str()))
        .collect::<Vec<_>>();
    spec.reference_repos.iter().all(|repo| {
        let root = Path::new(repo.root_path.as_str());
        let fresh = ids.insert(repo.repo_id)
            && !roots
                .iter()
                .any(|other| root.starts_with(other) || other.starts_with(root));
        roots.push(root);
        fresh
            && valid_relative_root(&repo.root_path)
            && valid_branch(&repo.branch)
            && valid_oid(&repo.commit)
            && !repo.git_url.trim().is_empty()
    })
}

fn valid_repo_vector(repos: &[VmRepoSpec]) -> bool {
    let mut ids = BTreeSet::new();
    let mut roots = BTreeSet::new();
    let shared_branch = repos.first().map(|repo| repo.branch.as_str());
    !repos.is_empty()
        && repos.iter().all(|repo| {
            ids.insert(repo.repo_id)
                && !roots.iter().any(|root: &&Path| {
                    Path::new(&repo.root_path).starts_with(root)
                        || root.starts_with(&repo.root_path)
                })
                && roots.insert(Path::new(&repo.root_path))
                && shared_branch == Some(repo.branch.as_str())
                && valid_relative_root(&repo.root_path)
                && valid_branch(&repo.branch)
                && repo.upstream_branch.as_deref().is_none_or(valid_branch)
                && valid_oid(&repo.branch_oid)
                && !repo.git_url.trim().is_empty()
        })
}

/// Validate the entire admitted member/receipt vector before modifying any member.
/// Guest bootstrap calls this before initializing additional repositories too.
pub fn validate_restore_spec(spec: &VmSpec) -> Result<()> {
    anyhow::ensure!(
        valid_repo_vector(&spec.repos) && valid_restore_vector(spec),
        "invalid restore vector"
    );
    Ok(())
}

fn valid_restore_vector(spec: &VmSpec) -> bool {
    let Some(restore) = &spec.restore else {
        return true;
    };
    let expected: BTreeSet<_> = spec.repos.iter().map(|repo| repo.repo_id).collect();
    let actual: BTreeSet<_> = restore.repos.iter().map(|repo| repo.repo_id).collect();
    expected == actual
        && actual.len() == restore.repos.len()
        && restore.repos.iter().all(|repo| {
            [
                &repo.branch_oid,
                &repo.local_branch_oid,
                &repo.local_head_oid,
                &repo.snapshot_oid,
            ]
            .into_iter()
            .all(|oid| valid_oid(oid))
                && repo.dirty == (repo.snapshot_oid != repo.local_head_oid)
                && repo.head_ref.as_deref().is_none_or(valid_head_ref)
        })
}

fn valid_head_ref(value: &str) -> bool {
    value.starts_with("refs/heads/")
        && !value.contains("..")
        && !value
            .bytes()
            .any(|byte| byte == 0 || byte < 0x20 || byte == b' ')
}

async fn restore_workspace(root: &Path, spec: &VmSpec, token: &str) -> Result<RestoreStats> {
    let environment = g5d_foundation::workspace::repo_cache::host_git_environment();
    let git = g5_scm::native::NativeGit::host(environment, Duration::from_secs(1800))?;
    restore_workspace_with_git(root, spec, token, &git).await
}

/// Exact receipt semantics shared by host materialization and trusted guest bootstrap.
/// Callers admit the complete spec and select an appropriate native execution context.
pub async fn restore_workspace_with_git(
    root: &Path,
    spec: &VmSpec,
    token: &str,
    git: &g5_scm::native::NativeGit,
) -> Result<RestoreStats> {
    validate_restore_spec(spec)?;
    reject_symlink_roots(root, &spec.repos).await?;
    let mut interrupted = Vec::new();
    let mut stats = RestoreStats::default();
    for repo in &spec.repos {
        let repo_root = root.join(&repo.root_path);
        let restore = spec.restore.as_ref().and_then(|restore| {
            restore
                .repos
                .iter()
                .find(|item| item.repo_id == repo.repo_id)
        });
        let auth = if !token.is_empty()
            && (repo.git_url.starts_with("http://") || repo.git_url.starts_with("https://"))
        {
            g5_scm::GitAuth::Token(token.to_owned())
        } else {
            g5_scm::GitAuth::None
        };
        let request = g5_scm::native::RestoreCheckout {
            repository: repo_root,
            url: repo.git_url.clone(),
            workspace_id: spec.incarnation.workspace_id,
            repo_id: repo.repo_id,
            branch: repo.branch.clone(),
            branch_oid: repo.branch_oid.clone(),
            upstream_branch: repo.upstream_branch.clone(),
            snapshot: restore.map(|receipt| g5_scm::native::RestoreSnapshot {
                flush_id: spec.restore.as_ref().expect("receipt owner").flush_id,
                local_branch_oid: receipt.local_branch_oid.clone(),
                local_head_oid: receipt.local_head_oid.clone(),
                snapshot_oid: receipt.snapshot_oid.clone(),
                head_ref: if matches!(
                    receipt.sequencer,
                    g5_internal_api::host_ctl::VmSequencer::None
                ) {
                    receipt.head_ref.clone()
                } else {
                    None
                },
            }),
        };
        let outcome = git
            .restore_checkout(&request, &auth, UPSTREAM_BRANCH_FETCH_TIMEOUT)
            .await?;
        stats.fetched_kib = stats.fetched_kib.saturating_add(outcome.fetched_kib);
        if let Some(error) = outcome.upstream_failure {
            tracing::warn!(repo_id = %repo.repo_id, upstream_branch = ?repo.upstream_branch, error = %error,
                "base branch not fetched into the workspace root; ahead/behind against base unavailable");
        }
        if let Some(receipt) = restore {
            if !matches!(
                receipt.sequencer,
                g5_internal_api::host_ctl::VmSequencer::None
            ) {
                interrupted.push(
                    json!({ "repo_id": repo.repo_id, "sequencer": receipt.sequencer,
                    "head_ref": receipt.head_ref, "local_head_oid": receipt.local_head_oid }),
                );
            }
        }
    }
    let marker = root.join(".g5/restore-interrupted.json");
    if interrupted.is_empty() {
        let _ = tokio::fs::remove_file(marker).await;
    } else {
        atomic_write(
            &marker,
            &serde_json::to_vec(&json!({"schema":1,"repos":interrupted}))?,
        )
        .await?;
    }
    Ok(stats)
}

/// Give the reference checkouts the host just materialized from its cache an
/// origin that tracks their branch, as guest bootstrap does after a seed hit:
/// a copy of the bare cache has an origin URL but no fetch refspec, so `git
/// pull` would never update `origin/<branch>`, which does not exist yet.
async fn restore_references(root: &Path, spec: &VmSpec, token: &str) -> Result<()> {
    if spec.reference_repos.is_empty() {
        return Ok(());
    }
    let environment = g5d_foundation::workspace::repo_cache::host_git_environment();
    let git = g5_scm::native::NativeGit::host(environment, Duration::from_secs(1800))?;
    restore_reference_checkouts(root, spec, spec, token, &git, false).await
}

/// Materialize the spec's reference checkouts inside a guest whose members
/// came from a repository seed, which holds members only. Each one is fetched
/// over the VM's own token and checked out on its branch at its pinned commit,
/// as the host materializes them from its cache (`g5.personal.mount`,
/// `g5.workspace.identity.reference`). `transport_spec` carries the guest's
/// transport URLs; `canonical` the admitted ones its origin remote keeps.
pub async fn restore_references_with_git(
    root: &Path,
    canonical: &VmSpec,
    transport_spec: &VmSpec,
    token: &str,
    git: &g5_scm::native::NativeGit,
) -> Result<()> {
    restore_reference_checkouts(root, canonical, transport_spec, token, git, true).await
}

/// Fetch each reference's branch into `origin/<branch>` and check it out at
/// its pin. `bootstrap` first initializes the checkout with its origin,
/// whose default refspec tracks every branch; otherwise the checkout is the
/// host's cache copy, and its origin gains a refspec for the branch.
async fn restore_reference_checkouts(
    root: &Path,
    canonical: &VmSpec,
    transport_spec: &VmSpec,
    token: &str,
    git: &g5_scm::native::NativeGit,
    bootstrap: bool,
) -> Result<()> {
    anyhow::ensure!(
        valid_reference_vector(canonical)
            && canonical.reference_repos.len() == transport_spec.reference_repos.len(),
        "invalid reference vector"
    );
    for (repo, transport) in canonical
        .reference_repos
        .iter()
        .zip(&transport_spec.reference_repos)
    {
        anyhow::ensure!(
            repo.repo_id == transport.repo_id,
            "invalid reference vector"
        );
        let repository = root.join(&repo.root_path);
        if bootstrap {
            git.prepare_bootstrap_checkout(&repository, &repo.commit, &repo.git_url)
                .await?;
        } else {
            git.track_remote_branch(&repository, "origin", &repo.branch)
                .await?;
        }
        let auth = if !token.is_empty()
            && (transport.git_url.starts_with("http://")
                || transport.git_url.starts_with("https://"))
        {
            g5_scm::GitAuth::Token(token.to_owned())
        } else {
            g5_scm::GitAuth::None
        };
        let request = g5_scm::native::RestoreCheckout {
            repository,
            url: transport.git_url.clone(),
            workspace_id: canonical.incarnation.workspace_id,
            repo_id: repo.repo_id,
            branch: repo.branch.clone(),
            branch_oid: repo.commit.clone(),
            upstream_branch: None,
            snapshot: None,
        };
        git.restore_checkout(&request, &auth, UPSTREAM_BRANCH_FETCH_TIMEOUT)
            .await?;
    }
    reject_symlink_reference_roots(root, &canonical.reference_repos).await
}

/// Restore accounting returned by workspace root preparation.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RestoreStats {
    pub fetched_kib: u64,
}

fn valid_relative_root(value: &str) -> bool {
    let path = Path::new(value);
    !value.is_empty()
        && !path.is_absolute()
        && path.components().all(|component| {
            matches!(component, Component::Normal(_)) && component.as_os_str().to_str().is_some()
        })
        && path
            .components()
            .next()
            .and_then(|part| part.as_os_str().to_str())
            != Some(".g5")
}

fn valid_branch(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 250
        && !value.starts_with('-')
        && !value
            .bytes()
            .any(|byte| byte == 0 || byte < 0x20 || byte == b' ')
}

fn valid_oid(value: &str) -> bool {
    matches!(value.len(), 40 | 64) && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

/// Hand the materialized tree to the module identity the spec provisions
/// (`G5D_MODULE_UID`/`G5D_MODULE_GID`). Imported files retain their ownership,
/// so the unprivileged agent, terminal and Git children need the complete tree
/// handed over before writing under `/work`.
/// Runs last, after the receipt restore, so restored files are covered; a
/// symlink is re-owned as a link and never followed.
async fn own_workspace_root(root: &Path, spec: &VmSpec) -> Result<()> {
    let Some((uid, gid)) = g5d_foundation::ownership::module_identity_from_env(&spec.env) else {
        return Ok(());
    };
    if !g5d_foundation::ownership::can_hand_over() {
        tracing::warn!(
            uid,
            "not root: the workspace root stays under the host's identity"
        );
        return Ok(());
    }
    let root = root.to_path_buf();
    tokio::task::spawn_blocking(move || {
        g5d_foundation::ownership::chown_tree(&root, Some(uid), gid)
    })
    .await
    .context("workspace root ownership task")?
    .context("hand the workspace root to the module identity")
}

async fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| anyhow!("path has no parent: {}", path.display()))?;
    tokio::fs::create_dir_all(parent).await?;
    let temp = path.with_extension(format!("tmp-{}", Uuid::new_v4()));
    tokio::fs::write(&temp, bytes).await?;
    tokio::fs::rename(&temp, path).await?;
    Ok(())
}

async fn reject_symlink_reference_roots(root: &Path, repos: &[VmReferenceRepoSpec]) -> Result<()> {
    for repo in repos {
        let mut current = root.to_path_buf();
        for component in Path::new(&repo.root_path).components() {
            current.push(component.as_os_str());
            let metadata = tokio::fs::symlink_metadata(&current)
                .await
                .with_context(|| format!("inspect reference root {}", current.display()))?;
            if metadata.file_type().is_symlink() {
                return Err(anyhow!("reference root contains a symlink"));
            }
        }
    }
    Ok(())
}

async fn reject_symlink_roots(root: &Path, repos: &[VmRepoSpec]) -> Result<()> {
    for repo in repos {
        let mut current = root.to_path_buf();
        for component in Path::new(&repo.root_path).components() {
            current.push(component.as_os_str());
            let metadata = tokio::fs::symlink_metadata(&current)
                .await
                .with_context(|| format!("inspect repository root {}", current.display()))?;
            if metadata.file_type().is_symlink() {
                return Err(anyhow!("repository root contains a symlink"));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::{Digest, Sha256};

    #[tokio::test]
    async fn dropped_pending_start_guards_release_only_their_own_execution() {
        use crate::test_support::{FakeLauncher, HostFixture};

        // Dropping an execution and cancelling settlement while it waits for
        // the state lock must both preserve the other execution's custody.
        for cancel_settle in [false, true] {
            let temp = tempfile::tempdir().unwrap();
            let fixture = HostFixture::new(temp.path());
            let launcher = Arc::new(FakeLauncher::default());
            let runtime = fixture.runtime(
                launcher.clone(),
                fixture.journal().await,
                Arc::new(LeaseRenewalBroker::default()),
                1,
            );
            let workspace_id = Uuid::new_v4();
            let vm_id = Uuid::new_v4();
            let command = fixture.start(workspace_id, vm_id, 1);
            let original = runtime.register_start(&command).await;
            let resend = runtime.register_start(&command).await;
            let locked = runtime.state.lock().await;
            if cancel_settle {
                let mut settling = Box::pin(original.settle());
                assert!(futures_util::poll!(&mut settling).is_pending());
                drop(settling);
            } else {
                drop(original);
            }
            drop(locked);

            // Synchronize with Drop's asynchronous cleanup before observing
            // status: checking earlier could pass even if cleanup erased it.
            loop {
                let state = runtime.state.lock().await;
                let pending = state
                    .pending_starts
                    .get(&vm_id)
                    .expect("dropping one execution must not erase the other's registration");
                if pending.registrations == 1 {
                    break;
                }
                drop(state);
                tokio::task::yield_now().await;
            }
            assert!(matches!(
                runtime.status(VmStatus { workspace_id, vm_id }).await,
                CommandResult::Status { vm } if vm.state == HostVmState::Booting
            ));
            assert_eq!(runtime.inventory().await.len(), 1);

            drop(resend);
            while runtime
                .state
                .lock()
                .await
                .pending_starts
                .contains_key(&vm_id)
            {
                tokio::task::yield_now().await;
            }
            assert_eq!(
                runtime
                    .status(VmStatus {
                        workspace_id,
                        vm_id
                    })
                    .await,
                CommandResult::NotFound
            );
            assert!(runtime.inventory().await.is_empty());
            assert!(launcher.started().await.is_empty());
        }
    }

    #[test]
    fn repo_info_keeps_the_membership_role() {
        let spec = |role: Option<&str>| VmRepoSpec {
            repo_id: Uuid::nil(),
            root_path: "repo".to_string(),
            role: role.map(str::to_string),
            git_url: "https://git.example/repo".to_string(),
            branch: "ws".to_string(),
            branch_oid: "0".repeat(40),
            upstream_branch: None,
            follow_branch: false,
        };
        assert_eq!(repo_info(&spec(Some("docs"))).repo_role, RepoRole::Docs);
        assert_eq!(repo_info(&spec(Some("code"))).repo_role, RepoRole::Code);
        // An older spec without a role names no docs repository.
        assert_eq!(repo_info(&spec(None)).repo_role, RepoRole::Unknown);
    }
    use g5d_isolation::port_map::EPHEMERAL_START;
    use std::net::Ipv4Addr;
    use std::sync::atomic::{AtomicUsize, Ordering as AtomicOrdering};
    use tokio::io::AsyncReadExt;
    use tokio::net::TcpListener;

    static NEXT_BOOT_RETRY_PORT_LANE: AtomicUsize = AtomicUsize::new(0);
    const BOOT_RETRY_PORT_LANE_WIDTH: usize = 512;

    /// Claim a port lane no other test process uses, and return the ports it
    /// leaves for the runtime. nextest runs each test in its own process, so a
    /// process-local counter alone gives every test the same lane, and one
    /// test's dispatcher client can then reach another test's fake
    /// dispatcher. A lane belongs to whoever holds a listener on its first
    /// port, kept for the life of this test process.
    fn claim_boot_retry_port_lane() -> (u16, u16) {
        let lanes = (usize::from(g5d_isolation::port_map::EPHEMERAL_END)
            - usize::from(EPHEMERAL_START)
            + 1)
            / BOOT_RETRY_PORT_LANE_WIDTH;
        let first = NEXT_BOOT_RETRY_PORT_LANE.fetch_add(1, AtomicOrdering::SeqCst)
            + std::process::id() as usize;
        for offset in 0..lanes {
            let lane = (first + offset) % lanes;
            let start =
                u16::try_from(usize::from(EPHEMERAL_START) + lane * BOOT_RETRY_PORT_LANE_WIDTH)
                    .unwrap();
            if let Ok(claim) = std::net::TcpListener::bind((Ipv4Addr::LOCALHOST, start)) {
                std::mem::forget(claim);
                let end = start
                    .checked_add(u16::try_from(BOOT_RETRY_PORT_LANE_WIDTH - 1).unwrap())
                    .unwrap();
                return (start + 1, end);
            }
        }
        panic!("every boot-retry port lane is claimed by another test process");
    }

    struct BootRetryLauncher {
        starts: AtomicUsize,
        kills: AtomicUsize,
        kill_attempted: tokio::sync::Notify,
        ports: std::sync::Mutex<Vec<u16>>,
        ready_on_attempt: Option<usize>,
        fail_kill: std::sync::atomic::AtomicBool,
        exit_before_ready: std::sync::atomic::AtomicBool,
        ready: Value,
    }

    #[async_trait]
    impl VmLauncher for BootRetryLauncher {
        async fn run(&self, _spec: MicrovmRunSpec) -> Result<g5d_isolation::RunResult> {
            unreachable!("workspace boots use start")
        }

        async fn start(&self, spec: MicrovmRunSpec) -> Result<RunHandle> {
            let attempt = self.starts.fetch_add(1, AtomicOrdering::SeqCst) + 1;
            let port = spec.port_maps[0].0;
            self.ports.lock().unwrap().push(port);
            let partial = spec.mount_host_dir.join("code/partial-from-unready-guest");
            if attempt == 1 {
                std::fs::write(&partial, b"partial guest write")?;
            } else {
                assert!(
                    !partial.exists(),
                    "retry must restore the authoritative checkout"
                );
            }
            if self.ready_on_attempt == Some(attempt) {
                let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, port)).await?;
                let expected_token = spec
                    .env
                    .iter()
                    .find(|(key, _)| key == "G5D_DISPATCH_TOKEN")
                    .expect("dispatcher token in guest environment")
                    .1
                    .clone();
                let ready = self.ready.clone();
                tokio::spawn(async move {
                    let (mut socket, _) = listener.accept().await.unwrap();
                    let mut token = [0_u8; 32];
                    socket.read_exact(&mut token).await.unwrap();
                    assert_eq!(hex::encode(token), expected_token);
                    let mut mux = Mux::new(socket, false);
                    let stream = mux.accept().await.unwrap();
                    stream.send(format!("{ready}\n").as_bytes()).await.unwrap();
                    tokio::time::sleep(Duration::from_secs(1)).await;
                });
            }
            if self.exit_before_ready.load(AtomicOrdering::SeqCst) {
                let (handle, publisher) = RunHandle::test_microvm_with_reap_publisher(
                    format!("attempt-{attempt}"),
                    Some(Ipv4Addr::LOCALHOST),
                    None,
                );
                publisher.publish_exit(Some(70));
                return Ok(handle);
            }
            Ok(RunHandle::test_microvm(
                format!("attempt-{attempt}"),
                Some(Ipv4Addr::LOCALHOST),
                None,
            ))
        }

        async fn kill(&self, _handle: &RunHandle) -> Result<g5d_isolation::ChildReapReceipt> {
            self.kills.fetch_add(1, AtomicOrdering::SeqCst);
            self.kill_attempted.notify_one();
            if self.fail_kill.load(AtomicOrdering::SeqCst) {
                anyhow::bail!("child reap unconfirmed");
            }
            Ok(g5d_isolation::ChildReapReceipt::confirmed_for_test(None))
        }
    }

    async fn boot_retry_fixture(
        ready_on_attempt: Option<usize>,
        fail_kill: bool,
    ) -> (
        tempfile::TempDir,
        WorkspaceVmRuntime,
        VmStart,
        Arc<BootRetryLauncher>,
    ) {
        // The cache and workspace copy must share a reflink-capable filesystem.
        let reflink_test_root = std::env::var_os("G5_TEST_REFLINK_ROOT")
            .map(PathBuf::from)
            .unwrap_or_else(|| std::env::current_dir().unwrap());
        std::fs::create_dir_all(&reflink_test_root).unwrap();
        let temp = tempfile::Builder::new()
            .prefix("g5-boot-retry-")
            .tempdir_in(reflink_test_root)
            .unwrap();
        let source = temp.path().join("source");
        let bare = temp.path().join("origin.git");
        std::fs::create_dir_all(&source).unwrap();
        git(&source, &["init", "-q", "-b", "shared"]);
        std::fs::write(source.join("base.txt"), b"base\n").unwrap();
        git(&source, &["add", "base.txt"]);
        git(&source, &["commit", "-q", "-m", "base"]);
        let branch_oid = git(&source, &["rev-parse", "HEAD"]);
        std::fs::create_dir_all(&bare).unwrap();
        git(&bare, &["init", "-q", "--bare"]);
        git(
            &source,
            &["remote", "add", "origin", bare.to_str().unwrap()],
        );
        git(&source, &["push", "-q", "origin", "shared"]);
        let workspace_id = Uuid::new_v4();
        let vm_id = Uuid::new_v4();
        let org_id = Uuid::new_v4();
        let module_catalog_version = "test".to_string();
        let spec = VmSpec {
            spec_version: VM_SPEC_VERSION,
            dispatcher_contract_version: g5_internal_api::host_ctl::DISPATCHER_CONTRACT_VERSION,
            incarnation: g5_internal_api::host_ctl::VmIncarnationSpec {
                workspace_id,
                vm_id,
                epoch: 1,
                org_id,
                owner_user_id: Uuid::new_v4(),
                lease_ttl_secs: 300,
                idle_timeout_secs: 900,
            },
            image: g5_internal_api::host_ctl::VmImageSpec {
                image_ref: format!("example.invalid/workspace@sha256:{}", "a".repeat(64)),
                digest: format!("sha256:{}", "a".repeat(64)),
            },
            size: g5_internal_api::host_ctl::VmSizeSpec {
                cpus: 2,
                mem_mib: 4096,
                scratch_disk_mib: 0,
            },
            workspace_root: g5_internal_api::host_ctl::WorkspaceRootSpec {
                retain_hours: 24,
                cache_generation: 1,
            },
            repos: vec![VmRepoSpec {
                repo_id: Uuid::new_v4(),
                root_path: "code".into(),
                role: None,
                git_url: bare.to_str().unwrap().into(),
                branch: "shared".into(),
                branch_oid,
                upstream_branch: Some("main".into()),
                follow_branch: false,
            }],
            reference_repos: Vec::new(),
            restore: None,
            env: BTreeMap::new(),
            network: g5_internal_api::host_ctl::VmNetwork::Passt,
            entrypoint: vec!["/opt/g5/bin/g5d".into(), "dispatch".into()],
            module_catalog_version: module_catalog_version.clone(),
            module_catalog_hash: "b".repeat(64),
        };
        let ready = json!({
            "method": "runtime.ready",
            "params": {
                "workspace_id": workspace_id,
                "vm_id": vm_id,
                "epoch": 1,
                "module_catalog_version": module_catalog_version,
                "dispatcher_contract_version": spec.dispatcher_contract_version,
            }
        });
        let launcher = Arc::new(BootRetryLauncher {
            starts: AtomicUsize::new(0),
            kills: AtomicUsize::new(0),
            kill_attempted: tokio::sync::Notify::new(),
            ports: std::sync::Mutex::new(Vec::new()),
            ready_on_attempt,
            fail_kill: std::sync::atomic::AtomicBool::new(fail_kill),
            exit_before_ready: std::sync::atomic::AtomicBool::new(false),
            ready,
        });
        let workspaces_root = temp.path().join("workspaces");
        let mut runtime = WorkspaceVmRuntime::new(
            WorkspaceVmRuntimeConfig {
                org_id,
                slots_total: 1,
                cpus_per_vm: 2,
                mem_mib_per_vm: 4096,
                scratch_disk_mib: 0,
                block_workload: false,
                nested_virtualization: false,
                api_url: "http://api.invalid".into(),

                runtime_root: temp.path().join("runtime"),
                workspaces_root: workspaces_root.clone(),
                process_host_executable: None,
                dispatcher_ready_timeout: Duration::from_millis(30),
            },
            launcher.clone(),
            Arc::new(WorkspaceCheckout::new(
                g5d_foundation::workspace::RepoCache::new(temp.path().join("cache")),
                workspaces_root,
            )),
            Arc::new(
                EventJournal::open(temp.path().join("journal.json"))
                    .await
                    .unwrap(),
            ),
            Arc::new(LeaseRenewalBroker::default()),
            Arc::new(crate::runtime::NoOtherSlotUsage),
        );
        // Independent runtimes use separate ranges so concurrent tests cannot
        // connect to each other's fake dispatcher while retaining local leases.
        let (start, end) = claim_boot_retry_port_lane();
        runtime.ports = PortAllocator::with_range(start, end).unwrap();
        let dispatch_port = runtime
            .ports
            .lease_bindable(MAX_DISPATCH_PORT_PROBES)
            .await
            .unwrap();
        runtime.state.lock().await.active.insert(
            vm_id,
            ActiveVm {
                spec: spec.clone(),
                state: HostVmState::Booting,
                boot_at: Utc::now(),
                ready_at: None,
                handle: None,
                dispatcher: None,
                dispatch_port,
                dispatch_token: [0x5a; 32],
                streams_open: Arc::new(AtomicU32::new(0)),
                cancel: CancellationToken::new(),
            },
        );
        (
            temp,
            runtime,
            VmStart {
                workspace_id,
                vm_id,
                spec,
                workspace_token: "test-token".into(),
            },
            launcher,
        )
    }

    #[tokio::test]
    async fn parallel_boot_retry_fixtures_lease_distinct_host_ports() {
        let (_first_temp, first, first_command, _) = boot_retry_fixture(None, false).await;
        let (_second_temp, second, second_command, _) = boot_retry_fixture(None, false).await;
        let first_port = first.state.lock().await.active[&first_command.vm_id]
            .dispatch_port
            .port();
        let second_port = second.state.lock().await.active[&second_command.vm_id]
            .dispatch_port
            .port();
        assert_ne!(first_port, second_port);
    }

    #[tokio::test]
    async fn disk_admission_refuses_a_start_whose_v_exceeds_free_space_then_admits_one_that_fits() {
        let (_temp, mut runtime, mut command, launcher) = boot_retry_fixture(Some(1), false).await;
        let modules = runtime.config.runtime_root.join("modules");
        std::fs::create_dir_all(&modules).unwrap();
        let catalog = br#"{"catalog_version":"test","modules":[]}"#;
        std::fs::write(modules.join("catalog.json"), catalog).unwrap();
        command.spec.module_catalog_hash = runtime.runtime_identity().unwrap().module_catalog_hash;
        runtime.state.lock().await.active.remove(&command.vm_id);
        runtime.config.dispatcher_ready_timeout = Duration::from_secs(5);
        // The supervisor creates this root at startup; admission measures its
        // filesystem's available space.
        std::fs::create_dir_all(&runtime.config.workspaces_root).unwrap();
        // An unsatisfiable V exercises the real admission path without filling
        // a developer or shared CI filesystem.
        runtime.config.scratch_disk_mib = u32::MAX;
        command.spec.size.scratch_disk_mib = u32::MAX;
        assert_eq!(
            runtime.start(command.clone()).await,
            CommandResult::SlotUnavailable {
                reason: SlotUnavailableReason::Disk,
            }
        );
        assert_eq!(launcher.starts.load(AtomicOrdering::SeqCst), 0);
        // The same admission/settlement path reaches Ready once V fits.
        runtime.config.scratch_disk_mib = 1;
        command.spec.size.scratch_disk_mib = 1;
        assert_eq!(runtime.start(command.clone()).await, CommandResult::Started);
        assert_eq!(
            runtime.state.lock().await.active[&command.vm_id].state,
            HostVmState::Ready
        );
    }

    #[tokio::test]
    async fn authenticated_control_connection_binds_lease_renewals_to_vm_start() {
        use crate::ctl::{STALE_INCARNATION_CODE, STALE_INCARNATION_MESSAGE};
        use g5_internal_api::host_ctl::{LeaseGrant, LeaseRenew, LeaseRenewResult};

        let (_temp, mut runtime, mut command, _) = boot_retry_fixture(None, false).await;
        runtime.config.dispatcher_ready_timeout = Duration::from_secs(5);
        // A non-default epoch catches accidental constants in either caller.
        command.spec.incarnation.epoch = 7;
        let (port, token) = {
            let mut state = runtime.state.lock().await;
            let record = state.active.get_mut(&command.vm_id).unwrap();
            record.spec = command.spec.clone();
            (record.dispatch_port.port(), record.dispatch_token)
        };
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, port))
            .await
            .unwrap();
        let renewal = LeaseRenew {
            workspace_id: command.workspace_id,
            vm_id: command.vm_id,
            epoch: command.spec.incarnation.epoch,
            seq: 10,
        };
        let guest = tokio::spawn({
            let command = command.clone();
            let renewal = renewal.clone();
            async move {
                let (mut socket, _) = listener.accept().await.unwrap();
                let mut presented_token = [0; 32];
                socket.read_exact(&mut presented_token).await.unwrap();
                assert_eq!(presented_token, token);
                let mut mux = Mux::new(socket, false);
                let (writer, mut reader) = mux.accept().await.unwrap().into_split();
                let ready = json!({
                    "method": "runtime.ready",
                    "params": {
                        "workspace_id": command.workspace_id,
                        "vm_id": command.vm_id,
                        "epoch": command.spec.incarnation.epoch,
                        "module_catalog_version": command.spec.module_catalog_version,
                        "dispatcher_contract_version": command.spec.dispatcher_contract_version,
                    },
                });
                writer.send(format!("{ready}\n").as_bytes()).await.unwrap();

                for forged in [
                    LeaseRenew {
                        workspace_id: Uuid::new_v4(),
                        ..renewal.clone()
                    },
                    LeaseRenew {
                        vm_id: Uuid::new_v4(),
                        ..renewal.clone()
                    },
                    LeaseRenew {
                        epoch: renewal.epoch - 1,
                        ..renewal.clone()
                    },
                    LeaseRenew {
                        workspace_id: Uuid::new_v4(),
                        vm_id: Uuid::new_v4(),
                        seq: i64::MAX as u64 - 1,
                        ..renewal.clone()
                    },
                ] {
                    let frame = json!({"jsonrpc":"2.0", "id":"forged", "method":"lease.renew", "params":forged});
                    writer.send(format!("{frame}\n").as_bytes()).await.unwrap();
                    let reply: Value =
                        serde_json::from_str(&reader.read_line().await.unwrap()).unwrap();
                    assert_eq!(reply["id"], "forged");
                    assert_eq!(reply["error"]["code"], STALE_INCARNATION_CODE);
                    assert_eq!(reply["error"]["message"], STALE_INCARNATION_MESSAGE);
                    assert!(reply.get("result").is_none());
                }

                let frame = json!({"jsonrpc":"2.0", "id":"lease:10", "method":"lease.renew", "params":renewal});
                writer.send(format!("{frame}\n").as_bytes()).await.unwrap();
                let reply: Value =
                    serde_json::from_str(&reader.read_line().await.unwrap()).unwrap();
                assert_eq!(reply["id"], "lease:10");
                assert!(reply.get("error").is_none());
                assert_eq!(reply["result"]["token"], "bound-token");
                assert_eq!(reply["result"]["seq"], renewal.seq);
            }
        });
        let handle = RunHandle::test_microvm("lease-test", Some(Ipv4Addr::LOCALHOST), None);
        let cancel = CancellationToken::new();
        let mux = runtime
            .wait_for_dispatcher_ready(&handle, port, token, &command, &cancel)
            .await
            .unwrap();
        {
            let mut state = runtime.state.lock().await;
            let record = state.active.get_mut(&command.vm_id).unwrap();
            record.dispatcher = Some(mux);
            record.state = HostVmState::Ready;
        }
        let endpoint = runtime.endpoint(command.vm_id).await.unwrap();
        assert_eq!(endpoint.workspace_id, command.workspace_id);
        assert_eq!(endpoint.epoch, command.spec.incarnation.epoch);

        let pending = runtime.lease_renewals.next().await.unwrap();
        assert_eq!(
            pending.renewal, renewal,
            "only the bound renewal reaches the broker"
        );
        pending
            .response
            .send(Ok(LeaseRenewResult::Granted(LeaseGrant {
                stale: false,
                seq: renewal.seq,
                token: "bound-token".into(),
                lease_ttl_secs: 300,
                incarnation_generation: 1,
            })))
            .unwrap();
        guest.await.unwrap();
        cancel.cancel();
    }

    #[tokio::test]
    async fn unready_dispatcher_is_reaped_and_retried_on_a_new_port() {
        let (temp, runtime, command, launcher) = boot_retry_fixture(Some(2), false).await;
        let reflink_test_root = std::env::var_os("G5_TEST_REFLINK_ROOT")
            .map(PathBuf::from)
            .unwrap_or_else(|| std::env::current_dir().unwrap());
        let actual_root = temp.path().canonicalize().unwrap();
        let expected_root = reflink_test_root.canonicalize().unwrap();
        assert!(actual_root.starts_with(expected_root));
        let (root, _) = runtime
            .prepare_workspace_root(&command.spec, &command.workspace_token)
            .await
            .unwrap();
        let (handle, dispatcher) = runtime
            .boot_prepared(&command, root)
            .await
            .expect("second VM should publish dispatcher readiness");
        assert_eq!(handle.id, "attempt-2");
        assert!(dispatcher.is_some());
        assert_eq!(launcher.starts.load(AtomicOrdering::SeqCst), 2);
        assert_eq!(launcher.kills.load(AtomicOrdering::SeqCst), 1);
        let ports = launcher.ports.lock().unwrap().clone();
        assert_ne!(ports[0], ports[1]);
        assert_eq!(runtime.ports.allocated_count(), 1);
    }

    #[tokio::test]
    async fn exited_dispatcher_fails_promptly_without_a_second_launch() {
        let (temp, mut runtime, command, launcher) = boot_retry_fixture(None, false).await;
        runtime.config.dispatcher_ready_timeout = Duration::from_secs(120);
        launcher
            .exit_before_ready
            .store(true, AtomicOrdering::SeqCst);
        let root = temp.path().join("prepared");
        std::fs::create_dir_all(root.join("code")).unwrap();
        // Disk/journal work may be delayed by a loaded runner. Virtual time
        // proves no readiness wait without imposing a wall-clock I/O budget.
        // The readiness deadline and retry timers use Tokio's clock.
        tokio::time::pause();
        let started = tokio::time::Instant::now();
        let error = runtime
            .boot_prepared(&command, root)
            .await
            .err()
            .expect("exited child cannot become ready");
        assert!(
            started.elapsed().is_zero(),
            "proven exit must not wait for readiness"
        );
        assert!(error
            .downcast_ref::<DispatcherExitedBeforeReady>()
            .is_some());
        assert!(error.to_string().contains("Some(70)"));
        assert_eq!(launcher.starts.load(AtomicOrdering::SeqCst), 1);
        assert_eq!(launcher.kills.load(AtomicOrdering::SeqCst), 1);
    }

    #[tokio::test]
    async fn live_unready_dispatcher_retains_the_retryable_timeout() {
        let (_temp, runtime, command, _) = boot_retry_fixture(None, false).await;
        let handle = RunHandle::test_microvm("still-live", Some(Ipv4Addr::LOCALHOST), None);
        let port = runtime.state.lock().await.active[&command.vm_id]
            .dispatch_port
            .port();
        let error = runtime
            .wait_for_dispatcher_ready(
                &handle,
                port,
                [0x5a; 32],
                &command,
                &CancellationToken::new(),
            )
            .await
            .err()
            .expect("unready live child times out");
        assert!(error.is::<DispatcherReadyTimeout>(), "{error}");
    }

    #[tokio::test]
    async fn exited_dispatcher_retains_the_existing_bounded_console_diagnostics() {
        let tail = Arc::new(g5d_isolation::isolation::GuestConsoleTail::default());
        tail.push(
            g5d_isolation::isolation::GuestConsoleStream::Stderr,
            "populate ext4 failed: capacity exhausted",
        );
        let handle = RunHandle::test_microvm_with_console_tail(
            "exited",
            Some(Ipv4Addr::LOCALHOST),
            None,
            tail,
        );
        let failure = DispatcherExitedBeforeReady::capture(&handle, Some(70)).await;
        assert_eq!(
            failure.log_tail.as_deref(),
            Some("[stderr] populate ext4 failed: capacity exhausted")
        );
    }

    #[tokio::test]
    async fn unready_dispatcher_does_not_retry_without_child_reap_evidence() {
        let (_temp, runtime, command, launcher) = boot_retry_fixture(Some(2), true).await;
        let (root, _) = runtime
            .prepare_workspace_root(&command.spec, &command.workspace_token)
            .await
            .unwrap();
        let boot = runtime.boot_prepared(&command, root);
        tokio::pin!(boot);
        // This select alone drives boot, including its inline reap retry loop.
        // After the signal, no further kill can run until we poll boot again.
        tokio::select! {
            result = boot.as_mut() => panic!("unproven exit must keep the boot pending: {:?}", result.map(|_| ())),
            _ = launcher.kill_attempted.notified() => {}
        }
        assert_eq!(launcher.starts.load(AtomicOrdering::SeqCst), 1);
        assert_eq!(launcher.kills.load(AtomicOrdering::SeqCst), 1);
        assert_eq!(runtime.active_count().await, 1);
        assert_eq!(runtime.inventory().await[0].vm_id, command.vm_id);
        launcher.fail_kill.store(false, AtomicOrdering::SeqCst);
        boot.await.expect("proof permits the replacement boot");
        assert_eq!(launcher.starts.load(AtomicOrdering::SeqCst), 2);
    }

    #[tokio::test]
    async fn persistently_unready_dispatcher_stops_after_two_reaped_attempts() {
        let (_temp, runtime, command, launcher) = boot_retry_fixture(None, false).await;
        let (root, _) = runtime
            .prepare_workspace_root(&command.spec, &command.workspace_token)
            .await
            .unwrap();
        let error = runtime
            .boot_prepared(&command, root)
            .await
            .err()
            .expect("two failed launches must not loop forever");
        assert_eq!(error.to_string(), "dispatcher_ready_timeout");
        assert_eq!(launcher.starts.load(AtomicOrdering::SeqCst), 2);
        assert_eq!(launcher.kills.load(AtomicOrdering::SeqCst), 2);
    }

    #[tokio::test]
    async fn workspace_start_rejects_dispatch_port_already_bound_on_host() {
        let occupied = (g5d_isolation::port_map::EPHEMERAL_START
            ..=g5d_isolation::port_map::EPHEMERAL_END)
            .find_map(|port| std::net::TcpListener::bind(("0.0.0.0", port)).ok())
            .expect("find a bindable ephemeral port");
        let port = occupied.local_addr().unwrap().port();
        let temp = tempfile::tempdir().unwrap();
        let runtime_root = temp.path().join("runtime");
        std::fs::create_dir_all(runtime_root.join("modules")).unwrap();
        let catalog = br#"{"catalog_version":"test"}"#;
        std::fs::write(runtime_root.join("modules/catalog.json"), catalog).unwrap();
        let workspace_id = Uuid::new_v4();
        let vm_id = Uuid::new_v4();
        let org_id = Uuid::new_v4();
        let workspaces_root = temp.path().join("workspaces");
        std::fs::create_dir_all(&workspaces_root).unwrap();
        let mut runtime = WorkspaceVmRuntime::new(
            WorkspaceVmRuntimeConfig {
                org_id,
                slots_total: 1,
                cpus_per_vm: 2,
                mem_mib_per_vm: 4096,
                scratch_disk_mib: 0,
                block_workload: false,
                nested_virtualization: false,
                api_url: "http://api.invalid".to_string(),

                runtime_root,
                workspaces_root: workspaces_root.clone(),
                process_host_executable: None,
                dispatcher_ready_timeout: Duration::ZERO,
            },
            Arc::new(NeverLauncher),
            Arc::new(WorkspaceCheckout::new(
                g5d_foundation::workspace::RepoCache::new(temp.path().join("cache")),
                workspaces_root,
            )),
            Arc::new(
                EventJournal::open(temp.path().join("journal.json"))
                    .await
                    .unwrap(),
            ),
            Arc::new(LeaseRenewalBroker::default()),
            Arc::new(crate::runtime::NoOtherSlotUsage),
        );
        runtime.ports = PortAllocator::with_range(port, port).unwrap();
        let result = runtime
            .start(VmStart {
                workspace_id,
                vm_id,
                workspace_token: "test-token".into(),
                spec: VmSpec {
                    spec_version: VM_SPEC_VERSION,
                    dispatcher_contract_version:
                        g5_internal_api::host_ctl::DISPATCHER_CONTRACT_VERSION,
                    incarnation: g5_internal_api::host_ctl::VmIncarnationSpec {
                        workspace_id,
                        vm_id,
                        epoch: 1,
                        org_id,
                        owner_user_id: Uuid::new_v4(),
                        lease_ttl_secs: 300,
                        idle_timeout_secs: 900,
                    },
                    image: g5_internal_api::host_ctl::VmImageSpec {
                        image_ref: format!("example.invalid/workspace@sha256:{}", "a".repeat(64)),
                        digest: format!("sha256:{}", "a".repeat(64)),
                    },
                    size: g5_internal_api::host_ctl::VmSizeSpec {
                        cpus: 2,
                        mem_mib: 4096,
                        scratch_disk_mib: 0,
                    },
                    workspace_root: g5_internal_api::host_ctl::WorkspaceRootSpec {
                        retain_hours: 24,
                        cache_generation: 1,
                    },
                    repos: vec![VmRepoSpec {
                        repo_id: Uuid::new_v4(),
                        root_path: "code".into(),
                        role: None,
                        git_url: "https://example.invalid/code.git".into(),
                        branch: "workspace/test".into(),
                        branch_oid: "a".repeat(40),
                        upstream_branch: Some("main".into()),
                        follow_branch: false,
                    }],
                    reference_repos: Vec::new(),
                    restore: None,
                    env: BTreeMap::new(),
                    network: g5_internal_api::host_ctl::VmNetwork::Passt,
                    entrypoint: vec!["/opt/g5/bin/g5d".into(), "dispatch".into()],
                    module_catalog_version: "test".into(),
                    module_catalog_hash: format!("{:x}", Sha256::digest(catalog)),
                },
            })
            .await;
        assert!(
            matches!(
                result,
                CommandResult::SlotUnavailable {
                    reason: SlotUnavailableReason::BootCapacity
                }
            ),
            "occupied dispatcher port must be rejected before VM boot: {result:?}"
        );
        assert_eq!(runtime.ports.allocated_count(), 0);
    }

    struct NeverLauncher;

    #[async_trait]
    impl VmLauncher for NeverLauncher {
        async fn run(&self, _spec: MicrovmRunSpec) -> Result<g5d_isolation::RunResult> {
            Err(anyhow!("test launcher must not run"))
        }

        async fn start(&self, _spec: MicrovmRunSpec) -> Result<RunHandle> {
            Err(anyhow!("test launcher must not start"))
        }

        async fn kill(&self, _handle: &RunHandle) -> Result<g5d_isolation::ChildReapReceipt> {
            Err(anyhow!("test launcher has no child"))
        }
    }

    #[test]
    fn workspace_dispatch_boot_loads_staged_linux_libraries() {
        let workspace_id = Uuid::new_v4();
        let vm_id = Uuid::new_v4();
        let spec = VmSpec {
            spec_version: VM_SPEC_VERSION,
            dispatcher_contract_version: g5_internal_api::host_ctl::DISPATCHER_CONTRACT_VERSION,
            incarnation: g5_internal_api::host_ctl::VmIncarnationSpec {
                workspace_id,
                vm_id,
                epoch: 1,
                org_id: Uuid::new_v4(),
                owner_user_id: Uuid::new_v4(),
                lease_ttl_secs: 300,
                idle_timeout_secs: 900,
            },
            image: g5_internal_api::host_ctl::VmImageSpec {
                image_ref: format!("example.invalid/workspace@sha256:{}", "a".repeat(64)),
                digest: format!("sha256:{}", "a".repeat(64)),
            },
            size: g5_internal_api::host_ctl::VmSizeSpec {
                cpus: 2,
                mem_mib: 4096,
                scratch_disk_mib: 0,
            },
            workspace_root: g5_internal_api::host_ctl::WorkspaceRootSpec {
                retain_hours: 24,
                cache_generation: 1,
            },
            repos: Vec::new(),
            reference_repos: Vec::new(),
            restore: None,
            env: BTreeMap::from([(
                "LD_LIBRARY_PATH".into(),
                "/tmp/untrusted-image-loader".into(),
            )]),
            network: g5_internal_api::host_ctl::VmNetwork::Passt,
            entrypoint: vec!["/opt/g5/bin/g5d".into(), "dispatch".into()],
            module_catalog_version: "test".into(),
            module_catalog_hash: "b".repeat(64),
        };
        let command = VmStart {
            workspace_id,
            vm_id,
            spec,
            workspace_token: "test-token".into(),
        };
        let env = dispatch_guest_env(&command, "http://api.invalid", [0x5a; 32]);
        let loader_paths: Vec<_> = env
            .iter()
            .filter(|(key, _)| key == "LD_LIBRARY_PATH")
            .map(|(_, value)| value.as_str())
            .collect();
        assert_eq!(loader_paths, [GUEST_RUNTIME_LIBRARY_PATH]);
        for (api, expected) in [
            ("http://localhost:7500", "http://g5-host:7500"),
            ("http://g5-api:7500", "http://g5-api:7500"),
            ("https://qa.g5labs.ai", "https://qa.g5labs.ai"),
        ] {
            let guest = dispatch_guest_env(&command, api, [0; 32]);
            assert!(guest
                .iter()
                .any(|(k, v)| k == "G5D_API_URL" && v == expected));
        }
        assert!(env.iter().any(|(key, value)| {
            key == "G5D_DISPATCH_PORT" && value == &DISPATCH_GUEST_PORT.to_string()
        }));
    }

    /// Review R14 (`g5.runtime.custody`, `g5.runtime.boot.backend.process-local`):
    /// a process-mode boot whose dispatcher cannot be proven gone (a detached
    /// child that kept its marker but hides its environment) stays under
    /// custody: the Start does not settle, the slot and workspace stay
    /// excluded, a stop during boot is accepted, and only once the process is
    /// gone does the boot settle as failed and release the root.
    #[cfg(target_os = "linux")]
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    #[serial_test::serial(process_incarnations)]
    async fn an_unproven_process_boot_keeps_custody_until_exit_is_proven() {
        if std::process::Command::new("python3")
            .args(["-c", "import ctypes"])
            .output()
            .map_or(true, |output| !output.status.success())
        {
            eprintln!("SKIP: python3 is required for the hidden-child dispatcher");
            return;
        }
        if !crate::runtime::environment_can_be_hidden() {
            return;
        }
        let reflink_test_root = std::env::var_os("G5_TEST_REFLINK_ROOT")
            .map(PathBuf::from)
            .unwrap_or_else(|| std::env::current_dir().unwrap());
        std::fs::create_dir_all(&reflink_test_root).unwrap();
        let temp = tempfile::Builder::new()
            .prefix("g5-unproven-boot-")
            .tempdir_in(reflink_test_root)
            .unwrap();
        let source = temp.path().join("source");
        let bare = temp.path().join("origin.git");
        std::fs::create_dir_all(&source).unwrap();
        git(&source, &["init", "-q", "-b", "shared"]);
        std::fs::write(source.join("base.txt"), b"base\n").unwrap();
        git(&source, &["add", "base.txt"]);
        git(&source, &["commit", "-q", "-m", "base"]);
        let branch_oid = git(&source, &["rev-parse", "HEAD"]);
        std::fs::create_dir_all(&bare).unwrap();
        git(&bare, &["init", "-q", "--bare"]);
        git(
            &source,
            &["remote", "add", "origin", bare.to_str().unwrap()],
        );
        git(&source, &["push", "-q", "origin", "shared"]);
        // The fake dispatcher: on its first boot it double-forks a child that
        // keeps the incarnation marker but hides its environment (its parent
        // exits, so ancestry reaches only the host), then never becomes
        // ready; later boots only idle.
        let once = temp.path().join("spawned-once");
        let hidden_pid = temp.path().join("hidden.pid");
        let host_g5d = temp.path().join("host-g5d");
        std::fs::write(
            &host_g5d,
            format!(
                "#!/bin/sh\nif [ ! -e '{once}' ]; then : > '{once}'; ( setsid python3 -c \"import ctypes, os, time; ctypes.CDLL(None).prctl(4, 0, 0, 0, 0); open('{pid}', 'w').write(str(os.getpid())); time.sleep(600)\" & ) ; fi\nexec sleep 600\n",
                once = once.display(),
                pid = hidden_pid.display()
            ),
        )
        .unwrap();
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&host_g5d, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        let workspace_id = Uuid::new_v4();
        let vm_id = Uuid::new_v4();
        let org_id = Uuid::new_v4();
        let runtime_root = temp.path().join("runtime");
        std::fs::create_dir_all(runtime_root.join("modules")).unwrap();
        let catalog = br#"{"catalog_version":"test","modules":[]}"#;
        std::fs::write(runtime_root.join("modules/catalog.json"), catalog).unwrap();
        let catalog_hash = format!("{:x}", Sha256::digest(catalog));
        let spec = VmSpec {
            spec_version: VM_SPEC_VERSION,
            dispatcher_contract_version: g5_internal_api::host_ctl::DISPATCHER_CONTRACT_VERSION,
            incarnation: g5_internal_api::host_ctl::VmIncarnationSpec {
                workspace_id,
                vm_id,
                epoch: 1,
                org_id,
                owner_user_id: Uuid::new_v4(),
                lease_ttl_secs: 300,
                idle_timeout_secs: 900,
            },
            image: g5_internal_api::host_ctl::VmImageSpec {
                image_ref: format!("example.invalid/workspace@sha256:{}", "a".repeat(64)),
                digest: format!("sha256:{}", "a".repeat(64)),
            },
            size: g5_internal_api::host_ctl::VmSizeSpec {
                cpus: 2,
                mem_mib: 4096,
                scratch_disk_mib: 0,
            },
            workspace_root: g5_internal_api::host_ctl::WorkspaceRootSpec {
                retain_hours: 24,
                cache_generation: 1,
            },
            repos: vec![VmRepoSpec {
                repo_id: Uuid::new_v4(),
                root_path: "code".into(),
                role: None,
                git_url: bare.to_str().unwrap().into(),
                branch: "shared".into(),
                branch_oid,
                upstream_branch: None,
                follow_branch: false,
            }],
            reference_repos: Vec::new(),
            restore: None,
            env: BTreeMap::from([
                ("G5D_OWNER_GIT_NAME".to_string(), "Custody Test".to_string()),
                (
                    "G5D_OWNER_GIT_EMAIL".to_string(),
                    "custody@example.test".to_string(),
                ),
            ]),
            network: g5_internal_api::host_ctl::VmNetwork::Passt,
            entrypoint: vec!["/opt/g5/bin/g5d".into(), "dispatch".into()],
            module_catalog_version: "test".into(),
            module_catalog_hash: catalog_hash,
        };
        let workspaces_root = temp.path().join("workspaces");
        let runtime = Arc::new(WorkspaceVmRuntime::new(
            WorkspaceVmRuntimeConfig {
                org_id,
                slots_total: 1,
                cpus_per_vm: 2,
                mem_mib_per_vm: 4096,
                scratch_disk_mib: 0,
                block_workload: false,
                nested_virtualization: false,
                api_url: "http://127.0.0.1:1".into(),

                runtime_root,
                workspaces_root: workspaces_root.clone(),
                dispatcher_ready_timeout: Duration::from_secs(1),
                process_host_executable: Some(host_g5d),
            },
            Arc::new(crate::runtime::ProcessVmLauncher::new(
                temp.path().join("state"),
            )),
            Arc::new(WorkspaceCheckout::new(
                g5d_foundation::workspace::RepoCache::new(temp.path().join("cache")),
                workspaces_root.clone(),
            )),
            Arc::new(
                EventJournal::open(temp.path().join("journal.json"))
                    .await
                    .unwrap(),
            ),
            Arc::new(LeaseRenewalBroker::default()),
            Arc::new(crate::runtime::NoOtherSlotUsage),
        ));
        let start = VmStart {
            workspace_id,
            vm_id,
            spec: spec.clone(),
            workspace_token: "incarnation-token".into(),
        };
        let booting = tokio::spawn({
            let runtime = runtime.clone();
            async move { runtime.start(start).await }
        });
        let hidden: i32 = tokio::time::timeout(Duration::from_secs(20), async {
            loop {
                if let Some(pid) = std::fs::read_to_string(&hidden_pid)
                    .ok()
                    .and_then(|text| text.trim().parse().ok())
                {
                    break pid;
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .expect("the fake dispatcher detached its hidden child");
        // Past the readiness timeout and the first proof attempt, the boot has
        // not settled: the record, slot and exclusion are held.
        tokio::time::sleep(Duration::from_secs(9)).await;
        assert!(!booting.is_finished(), "an unproven boot must not settle");
        assert_eq!(runtime.active_count().await, 1);
        let mut replacement = VmStart {
            workspace_id,
            vm_id: Uuid::new_v4(),
            spec: spec.clone(),
            workspace_token: "incarnation-token-2".into(),
        };
        replacement.spec.incarnation.vm_id = replacement.vm_id;
        assert!(
            matches!(
                runtime.start(replacement).await,
                CommandResult::WorkspaceBusy
            ),
            "the workspace stays excluded while its old incarnation is unproven"
        );
        assert!(
            workspaces_root.join(workspace_id.to_string()).exists(),
            "the root is not reclaimed while exit is unproven"
        );
        // A stop during the unproven boot is accepted, not a release.
        assert!(matches!(
            runtime
                .stop(VmStop {
                    workspace_id,
                    vm_id,
                    mode: StopMode::Kill,
                    grace_secs: 0,
                })
                .await,
            CommandResult::Accepted
        ));
        // Once the hidden process is gone, exit is provable: the boot settles
        // as failed and the root is released.
        // SAFETY: the test owns this fixture process.
        unsafe {
            libc::kill(hidden, libc::SIGKILL);
        }
        let result = tokio::time::timeout(Duration::from_secs(30), booting)
            .await
            .expect("the boot settles once exit is proven")
            .unwrap();
        assert!(
            matches!(result, CommandResult::BootFailed { .. }),
            "{result:?}"
        );
        assert_eq!(runtime.active_count().await, 0);
        tokio::time::timeout(Duration::from_secs(10), async {
            while workspaces_root.join(workspace_id.to_string()).exists() {
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        })
        .await
        .expect("the root is released after proof");
    }

    /// A failed boot publishes legacy facts, keeping diagnostics distinct from
    /// exit proof even when a Stop raced with the boot failure.
    #[tokio::test]
    async fn failed_boot_journals_exit_only_when_cleanup_is_proven() {
        for child_reaped in [false, true] {
            for stopping in [false, true] {
                let temp = tempfile::tempdir().unwrap();
                let fixture = crate::test_support::HostFixture::new(temp.path());
                let journal = fixture.journal().await;
                let runtime = fixture.runtime_from_config(
                    fixture.config(1),
                    Arc::new(crate::test_support::FakeLauncher::default()),
                    journal.clone(),
                    Arc::new(LeaseRenewalBroker::default()),
                );
                let workspace_id = Uuid::new_v4();
                let vm_id = Uuid::new_v4();
                let command = fixture.start(workspace_id, vm_id, 1);
                {
                    let mut state = runtime.state.lock().await;
                    state.boots_in_progress = 1;
                    state.active.insert(
                        vm_id,
                        ActiveVm {
                            spec: command.spec.clone(),
                            state: if stopping {
                                HostVmState::Stopping
                            } else {
                                HostVmState::Booting
                            },
                            boot_at: Utc::now(),
                            ready_at: None,
                            handle: None,
                            dispatcher: None,
                            dispatch_port: runtime
                                .ports
                                .lease_bindable(MAX_DISPATCH_PORT_PROBES)
                                .await
                                .unwrap(),
                            dispatch_token: [0; 32],
                            streams_open: Arc::new(AtomicU32::new(0)),
                            cancel: CancellationToken::new(),
                        },
                    );
                }
                let error = if child_reaped {
                    anyhow::Error::new(DispatcherExitedBeforeReady {
                        exit_code: Some(1),
                        log_tail: Some("boot diagnostics".into()),
                    })
                } else {
                    anyhow::Error::new(CancelledChildReapUnconfirmed {
                        detail: "test reap failure".into(),
                    })
                };
                let reason = error.to_string();
                assert!(matches!(
                    runtime.complete_boot(&command, Err(error)).await,
                    CommandResult::BootFailed { .. }
                ));
                let events = journal.pending().await;
                assert_eq!(
                    events[0].log_tail.as_deref(),
                    child_reaped.then_some("boot diagnostics"),
                    "preserve failure diagnostics, including an exit completing Stop"
                );
                let mut expected = Vec::new();
                if !stopping || !child_reaped {
                    expected.push(serde_json::json!({"kind": "boot_failed", "reason": reason}));
                }
                if child_reaped {
                    expected
                        .push(serde_json::json!({"kind": "exited", "code": null, "signal": null}));
                }
                assert_eq!(
                    events
                        .iter()
                        .map(|event| serde_json::to_value(&event.kind).unwrap())
                        .collect::<Vec<_>>(),
                    expected
                );
                assert_eq!(
                    fixture.journal().await.pending().await,
                    events,
                    "facts survive restart unchanged"
                );
                assert_eq!(
                    runtime.state.lock().await.unreaped.contains_key(&vm_id),
                    !child_reaped
                );
            }
        }
    }

    #[tokio::test]
    async fn guest_output_cannot_turn_an_image_or_source_failure_into_capacity_retry() {
        for prefix in [
            "[stdout] ",
            "[stdout] populate: ",
            "[stdout] [stderr] ",
            "[stderr] invalid image: ",
        ] {
            let temp = tempfile::tempdir().unwrap();
            let fixture = crate::test_support::HostFixture::new(temp.path());
            let journal = fixture.journal().await;
            let runtime = fixture.runtime_from_config(
                fixture.config(1),
                Arc::new(crate::test_support::FakeLauncher::default()),
                journal.clone(),
                Arc::new(LeaseRenewalBroker::default()),
            );
            let command = fixture.start(Uuid::new_v4(), Uuid::new_v4(), 1);
            let diagnostic = g5d_foundation::disk::DiskPressure {
                phase: "cold_image".into(),
                available_bytes: 0,
                required_bytes: 1,
            }
            .reason();
            let error = anyhow::Error::new(DispatcherExitedBeforeReady {
                exit_code: Some(1),
                log_tail: Some(format!("{prefix}{diagnostic}")),
            });
            assert!(matches!(
                runtime.complete_boot(&command, Err(error)).await,
                CommandResult::BootFailed { .. }
            ));
            let events = journal.pending().await;
            assert!(
                matches!(&events[0].kind, VmEventKind::BootFailed {reason} if reason.starts_with("dispatcher_exited_before_ready"))
            );
            assert!(
                events[1].log_tail.is_none(),
                "untrusted text must not become a durable capacity fact"
            );
        }
    }

    #[tokio::test]
    async fn capacity_failure_reply_and_replayed_journal_preserve_the_same_start_outcome() {
        for through_helper in [false, true] {
            for proven in [false, true] {
                let temp = tempfile::tempdir().unwrap();
                let fixture = crate::test_support::HostFixture::new(temp.path());
                let journal = fixture.journal().await;
                let runtime = fixture.runtime_from_config(
                    fixture.config(1),
                    Arc::new(crate::test_support::FakeLauncher::default()),
                    journal.clone(),
                    Arc::new(LeaseRenewalBroker::default()),
                );
                let command = fixture.start(Uuid::new_v4(), Uuid::new_v4(), 1);
                let pressure = g5d_foundation::disk::DiskPressure {
                    phase: "image_preparation".into(),
                    available_bytes: 1234,
                    required_bytes: 85899345920,
                };
                let diagnostic = pressure.reason();
                let error = if !proven {
                    anyhow::Error::new(CancelledChildReapUnconfirmed {
                        detail: diagnostic.clone(),
                    })
                } else if through_helper {
                    anyhow::Error::new(DispatcherExitedBeforeReady {
                        exit_code: Some(1),
                        log_tail: Some(format!("[stderr] {diagnostic}")),
                    })
                } else {
                    anyhow::Error::new(pressure)
                };
                let reply = runtime.complete_boot(&command, Err(error)).await;
                assert_eq!(
                    reply,
                    if proven {
                        CommandResult::SlotUnavailable {
                            reason: SlotUnavailableReason::Disk,
                        }
                    } else {
                        CommandResult::Accepted
                    }
                );
                let events = journal.pending().await;
                assert_eq!(events.len(), if proven { 2 } else { 1 });
                assert_eq!(
                    events[0].kind,
                    VmEventKind::BootFailed {
                        reason: diagnostic.clone()
                    }
                );
                if proven {
                    assert!(matches!(events[1].kind, VmEventKind::Exited { .. }));
                    assert_eq!(events[1].log_tail.as_deref(), Some(diagnostic.as_str()));
                }
                assert_eq!(fixture.journal().await.pending().await, events);
                assert_eq!(
                    runtime
                        .state
                        .lock()
                        .await
                        .unreaped
                        .contains_key(&command.vm_id),
                    !proven
                );
            }
        }
    }

    /// Review R17 (`g5.runtime.custody`): a boot cancelled while it completed
    /// keeps its reservation, inventory, slot and workspace exclusion while
    /// its exit is proven, and is released only once the kill proves exit.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    #[serial_test::serial(process_incarnations)]
    async fn a_cancelled_boot_keeps_its_reservation_until_exit_is_proven() {
        assert_unproven_exit_holds_custody(true, true).await;
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    #[serial_test::serial(process_incarnations)]
    async fn a_cancelled_microvm_keeps_its_reservation_until_exit_is_proven() {
        assert_unproven_exit_holds_custody(false, true).await;
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    #[serial_test::serial(process_incarnations)]
    async fn a_stopping_microvm_keeps_its_reservation_until_exit_is_proven() {
        assert_unproven_exit_holds_custody(false, false).await;
    }

    async fn assert_unproven_exit_holds_custody(process_mode: bool, cancelled_boot: bool) {
        let temp = tempfile::tempdir().unwrap();
        let fixture = crate::test_support::HostFixture::new(temp.path());
        let launcher = Arc::new(crate::test_support::FakeLauncher::default());
        launcher.fail_kills(true);
        launcher.fail_stops(true);
        let mut config = fixture.config(1);
        config.process_host_executable = process_mode.then(|| PathBuf::from("/bin/true"));
        let journal = fixture.journal().await;
        let runtime = fixture.runtime_from_config(
            config,
            launcher.clone(),
            journal.clone(),
            Arc::new(LeaseRenewalBroker::default()),
        );
        let workspace_id = Uuid::new_v4();
        let vm_id = Uuid::new_v4();
        let command = fixture.start(workspace_id, vm_id, 1);
        // The reservation as a Stop left it just before the boot completed.
        let (handle, publisher) = RunHandle::test_microvm_with_reap_publisher(
            "cancelled-boot",
            Some(Ipv4Addr::LOCALHOST),
            Some(4242),
        );
        let handle = Arc::new(handle);
        {
            let mut state = runtime.state.lock().await;
            let cancel = CancellationToken::new();
            if cancelled_boot {
                cancel.cancel();
            }
            state.active.insert(
                vm_id,
                ActiveVm {
                    spec: command.spec.clone(),
                    state: if cancelled_boot {
                        HostVmState::Stopping
                    } else {
                        HostVmState::Running
                    },
                    boot_at: Utc::now(),
                    ready_at: None,
                    handle: (!cancelled_boot).then(|| handle.clone()),
                    dispatcher: None,
                    dispatch_port: runtime
                        .ports
                        .lease_bindable(MAX_DISPATCH_PORT_PROBES)
                        .await
                        .unwrap(),
                    dispatch_token: [0; 32],
                    streams_open: Arc::new(AtomicU32::new(0)),
                    cancel,
                },
            );
            state.boots_in_progress = usize::from(cancelled_boot);
        }
        let completing = tokio::spawn({
            let runtime = runtime.clone();
            let command = command.clone();
            let handle = handle.clone();
            async move {
                if cancelled_boot {
                    runtime
                        .complete_boot(&command, Ok((handle, None, RestoreStats::default())))
                        .await
                } else {
                    runtime
                        .stop(VmStop {
                            workspace_id,
                            vm_id,
                            mode: StopMode::Kill,
                            grace_secs: 0,
                        })
                        .await
                }
            }
        });
        tokio::time::sleep(Duration::from_secs(3)).await;
        assert!(!completing.is_finished(), "unproven exit must not settle");
        assert!(
            !(if cancelled_boot {
                launcher.killed().await.is_empty()
            } else {
                launcher.stopped().await.is_empty()
            }),
            "proof was attempted"
        );
        assert_eq!(runtime.active_count().await, 1);
        let inventory = runtime.inventory().await;
        assert_eq!(inventory.len(), 1);
        assert_eq!(inventory[0].vm_id, vm_id);
        assert_eq!(inventory[0].state, HostVmState::Stopping);
        assert!(matches!(
            runtime
                .start(fixture.start(workspace_id, Uuid::new_v4(), 2))
                .await,
            CommandResult::WorkspaceBusy
        ));
        assert!(matches!(
            runtime
                .start(fixture.start(Uuid::new_v4(), Uuid::new_v4(), 1))
                .await,
            CommandResult::SlotUnavailable {
                reason: SlotUnavailableReason::Slots
            }
        ));
        // Proof arrives: the next kill succeeds and the incarnation is released.
        launcher.fail_kills(false);
        launcher.fail_stops(false);
        let result = tokio::time::timeout(Duration::from_secs(10), completing)
            .await
            .expect("the cancelled boot settles once exit is proven")
            .unwrap();
        if cancelled_boot {
            assert!(
                matches!(result, CommandResult::BootFailed { ref reason, .. } if reason == "boot_cancelled"),
                "{result:?}"
            );
            // The cancelling stop completes through the proven exit: the
            // control plane's stop releases on this event instead of waiting
            // out the lease (`g5.lifecycle.stop`, G5-Lean#626 S11).
            assert!(
                journal
                    .pending()
                    .await
                    .iter()
                    .any(|event| event.vm_id == vm_id
                        && matches!(event.kind, VmEventKind::Exited { .. })),
                "a cancelled boot journals its proven exit"
            );
        } else {
            assert!(
                matches!(result, CommandResult::Stopped { .. }),
                "{result:?}"
            );
        }
        assert_eq!(runtime.active_count().await, 0);
        assert!(runtime.inventory().await.is_empty());
        drop(publisher);
    }

    /// A Ready journal write that fails is release evidence for the control
    /// plane, so the boot reports its definite failure only once the VM's exit
    /// is proven, never while the kill is still in flight
    /// (`g5.runtime.start.settlement`, G5-Lean#626 S11).
    #[tokio::test]
    async fn a_failed_ready_journal_holds_custody_until_exit_is_proven() {
        let temp = tempfile::tempdir().unwrap();
        let fixture = crate::test_support::HostFixture::new(temp.path());
        let launcher = Arc::new(crate::test_support::FakeLauncher::default());
        // The kill cannot prove the VMM gone yet.
        launcher.fail_stops(true);
        let journal = fixture.journal().await;
        let runtime = fixture.runtime_from_config(
            fixture.config(1),
            launcher.clone(),
            journal.clone(),
            Arc::new(LeaseRenewalBroker::default()),
        );
        let workspace_id = Uuid::new_v4();
        let vm_id = Uuid::new_v4();
        let command = fixture.start(workspace_id, vm_id, 1);
        let (handle, publisher) = RunHandle::test_microvm_with_reap_publisher(
            "ready-journal-fail",
            Some(Ipv4Addr::LOCALHOST),
            Some(4242),
        );
        let handle = Arc::new(handle);
        {
            let mut state = runtime.state.lock().await;
            state.boots_in_progress = 1;
            state.active.insert(
                vm_id,
                ActiveVm {
                    spec: command.spec.clone(),
                    state: HostVmState::Running,
                    boot_at: Utc::now(),
                    ready_at: None,
                    handle: Some(handle.clone()),
                    dispatcher: None,
                    dispatch_port: runtime
                        .ports
                        .lease_bindable(MAX_DISPATCH_PORT_PROBES)
                        .await
                        .unwrap(),
                    dispatch_token: [0; 32],
                    streams_open: Arc::new(AtomicU32::new(0)),
                    cancel: CancellationToken::new(),
                },
            );
        }
        journal.fail_writes(true);
        let settling = tokio::spawn({
            let runtime = runtime.clone();
            let command = command.clone();
            let handle = handle.clone();
            async move {
                runtime
                    .complete_boot(&command, Ok((handle, None, RestoreStats::default())))
                    .await
            }
        });
        tokio::time::sleep(Duration::from_secs(3)).await;
        assert!(
            !settling.is_finished(),
            "a failed Ready journal must not report failure before exit is proven"
        );
        assert_eq!(runtime.active_count().await, 1, "custody is held");

        // Exit is proven: the boot now reports its definite failure and
        // releases the incarnation.
        launcher.fail_stops(false);
        let result = tokio::time::timeout(Duration::from_secs(10), settling)
            .await
            .expect("the boot settles once exit is proven")
            .unwrap();
        assert!(
            matches!(result, CommandResult::BootFailed { ref reason, .. } if reason == "event_journal_unavailable"),
            "{result:?}"
        );
        assert_eq!(runtime.active_count().await, 0);
        drop(publisher);
    }

    /// The exact G5-Lean#626 S11 interleaving: a concurrent Stop takes
    /// ownership of the kill after the boot passed its cancellation check and
    /// while its Ready journal write is in flight; the write then fails, the
    /// boot's own stop answers `Busy`, and the boot must join that Stop's
    /// proven exit before reporting a definite failure
    /// (`g5.runtime.start.settlement`).
    #[tokio::test]
    async fn a_failed_ready_journal_joins_a_concurrent_stop_that_owns_the_kill() {
        let temp = tempfile::tempdir().unwrap();
        let fixture = crate::test_support::HostFixture::new(temp.path());
        let launcher = Arc::new(crate::test_support::FakeLauncher::default());
        launcher.fail_stops(true);
        let journal = fixture.journal().await;
        let runtime = fixture.runtime_from_config(
            fixture.config(1),
            launcher.clone(),
            journal.clone(),
            Arc::new(LeaseRenewalBroker::default()),
        );
        let workspace_id = Uuid::new_v4();
        let vm_id = Uuid::new_v4();
        let command = fixture.start(workspace_id, vm_id, 1);
        let (handle, publisher) = RunHandle::test_microvm_with_reap_publisher(
            "ready-journal-busy",
            Some(Ipv4Addr::LOCALHOST),
            Some(4242),
        );
        let handle = Arc::new(handle);
        {
            let mut state = runtime.state.lock().await;
            state.boots_in_progress = 1;
            state.active.insert(
                vm_id,
                ActiveVm {
                    spec: command.spec.clone(),
                    state: HostVmState::Running,
                    boot_at: Utc::now(),
                    ready_at: None,
                    handle: Some(handle.clone()),
                    dispatcher: None,
                    dispatch_port: runtime
                        .ports
                        .lease_bindable(MAX_DISPATCH_PORT_PROBES)
                        .await
                        .unwrap(),
                    dispatch_token: [0; 32],
                    streams_open: Arc::new(AtomicU32::new(0)),
                    cancel: CancellationToken::new(),
                },
            );
        }
        // The boot passes its cancellation check, then parks inside the Ready
        // journal write.
        journal.hold_writes(true);
        journal.fail_writes(true);
        let settling = tokio::spawn({
            let runtime = runtime.clone();
            let command = command.clone();
            let handle = handle.clone();
            async move {
                runtime
                    .complete_boot(&command, Ok((handle, None, RestoreStats::default())))
                    .await
            }
        });
        tokio::time::timeout(Duration::from_secs(5), async {
            while journal.held_writers() == 0 {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("the boot parks in its Ready journal write");

        // A Stop takes ownership of the kill meanwhile; exit cannot be proven
        // yet, so it keeps retrying.
        let stopping = tokio::spawn({
            let runtime = runtime.clone();
            async move {
                runtime
                    .stop(VmStop {
                        workspace_id,
                        vm_id,
                        mode: StopMode::Kill,
                        grace_secs: 0,
                    })
                    .await
            }
        });
        tokio::time::timeout(Duration::from_secs(5), async {
            while launcher.stopped().await.is_empty() {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("the concurrent stop begins proving exit");

        // The Ready write now fails. The boot's own stop finds the record
        // `Stopping` (`Busy`) and must not report failure before the owning
        // stop proves exit.
        journal.hold_writes(false);
        tokio::time::sleep(Duration::from_secs(2)).await;
        assert!(
            !settling.is_finished(),
            "a definite boot failure must wait for the owning stop's exit proof"
        );
        assert!(!stopping.is_finished(), "exit is still unproven");
        assert_eq!(runtime.active_count().await, 1, "custody is held");

        // Exit is proven: the stop completes, then the boot reports failure.
        launcher.fail_stops(false);
        let stop_result = tokio::time::timeout(Duration::from_secs(10), stopping)
            .await
            .expect("the owning stop proves exit")
            .unwrap();
        assert!(
            matches!(stop_result, CommandResult::Stopped { .. }),
            "{stop_result:?}"
        );
        let boot_result = tokio::time::timeout(Duration::from_secs(10), settling)
            .await
            .expect("the boot settles once exit is proven")
            .unwrap();
        assert!(
            matches!(boot_result, CommandResult::BootFailed { ref reason, .. } if reason == "event_journal_unavailable"),
            "{boot_result:?}"
        );
        assert_eq!(runtime.active_count().await, 0);
        drop(publisher);
    }

    fn git(root: &Path, args: &[&str]) -> String {
        let binary = std::env::var_os("G5_TEST_RESTORE_HOST_GIT").unwrap_or_else(|| "git".into());
        let output = std::process::Command::new(binary)
            .arg("-C")
            .arg(root)
            .args(args)
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_AUTHOR_NAME", "Restore Test")
            .env("GIT_AUTHOR_EMAIL", "restore@example.test")
            .env("GIT_COMMITTER_NAME", "Restore Test")
            .env("GIT_COMMITTER_EMAIL", "restore@example.test")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout).unwrap().trim().to_owned()
    }

    /// Review R5 and `g5.runtime.boot.backend.process-local`: the process
    /// boot environment keeps the incarnation identity, drops the guest-only
    /// keys and carries the host's Docker connection selection, from the
    /// explicit variables or the host home's `.docker` directory.
    #[test]
    fn process_boot_env_carries_the_host_docker_selection_and_drops_guest_keys() {
        let temp = tempfile::tempdir().unwrap();
        let fixture = crate::test_support::HostFixture::new(temp.path());
        let workspace = Uuid::new_v4();
        let vm = Uuid::new_v4();
        let mut start = fixture.start(workspace, vm, 3);
        start
            .spec
            .env
            .insert("G5D_WORKSPACE_HOSTNAME".into(), "feature-login".into());
        start
            .spec
            .env
            .insert("G5D_MODULE_UID".into(), "1000".into());
        start
            .spec
            .env
            .insert("G5D_MODULE_GID".into(), "1000".into());
        start.spec.env.insert("HOME".into(), "/home/agent".into());
        start
            .spec
            .env
            .insert("CODEX_HOME".into(), "/home/agent/.codex".into());
        let env = dispatch_process_env(
            &start,
            "http://127.0.0.1:7500",
            [7; 32],
            43210,
            Path::new("/checkout"),
            Path::new("/runtime"),
        );
        let get = |key: &str| env.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str());
        assert_eq!(get("G5D_ISOLATION_MODE"), Some("process"));
        assert_eq!(get("G5D_API_URL"), Some("http://127.0.0.1:7500"));
        assert_eq!(get("G5D_DISPATCH_PORT"), Some("43210"));
        assert_eq!(get("G5D_PROCESS_WORKSPACE_ROOT"), Some("/checkout"));
        assert_eq!(get("G5D_PROCESS_RUNTIME_ROOT"), Some("/runtime"));
        assert_eq!(get("G5D_GIT_DOOR_PORT"), Some("0"));
        assert_eq!(get("G5D_VM_ID"), Some(vm.to_string().as_str()));
        for guest_only in [
            "G5D_WORKSPACE_HOSTNAME",
            "G5D_MODULE_UID",
            "G5D_MODULE_GID",
            "HOME",
            "CODEX_HOME",
            "LD_LIBRARY_PATH",
        ] {
            assert_eq!(get(guest_only), None, "{guest_only} is guest-only");
        }

        // Explicit selection wins and nothing else of the host crosses.
        let home = temp.path().join("home");
        std::fs::create_dir_all(home.join(".docker")).unwrap();
        let host: std::collections::HashMap<&str, String> = [
            (
                "DOCKER_HOST",
                "unix:///tmp/review-host-docker.sock".to_string(),
            ),
            ("DOCKER_CONTEXT", "review-host-context".to_string()),
            ("DOCKER_TLS_VERIFY", String::new()),
            ("HOME", home.to_string_lossy().into_owned()),
            ("AWS_SECRET_ACCESS_KEY", "never".to_string()),
        ]
        .into_iter()
        .collect();
        let mut env = Vec::new();
        host_docker_selection(&mut env, |key| host.get(key).map(std::ffi::OsString::from));
        let get = |key: &str| env.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str());
        assert_eq!(
            get("DOCKER_HOST"),
            Some("unix:///tmp/review-host-docker.sock")
        );
        assert_eq!(get("DOCKER_CONTEXT"), Some("review-host-context"));
        assert_eq!(
            get("DOCKER_TLS_VERIFY"),
            None,
            "an empty value is not a selection"
        );
        assert_eq!(
            get("DOCKER_CONFIG"),
            Some(home.join(".docker").to_string_lossy().as_ref()),
            "the host home's .docker stands in for an unnamed config"
        );
        assert_eq!(get("AWS_SECRET_ACCESS_KEY"), None);
        assert_eq!(get("HOME"), None);

        // A named config directory is kept as named.
        let mut env = Vec::new();
        host_docker_selection(&mut env, |key| match key {
            "DOCKER_CONFIG" => Some("/srv/docker-config".into()),
            "HOME" => Some(home.as_os_str().to_owned()),
            _ => None,
        });
        assert_eq!(
            env,
            vec![(
                "DOCKER_CONFIG".to_string(),
                "/srv/docker-config".to_string()
            )]
        );
    }

    #[test]
    fn repo_vector_requires_one_shared_safe_branch_and_disjoint_roots() {
        let id = Uuid::new_v4();
        let mut repos = vec![VmRepoSpec {
            repo_id: id,
            root_path: "app".to_string(),
            role: None,
            git_url: "https://example.invalid/app.git".to_string(),
            branch: "workspace/shared".to_string(),
            branch_oid: "a".repeat(40),
            upstream_branch: Some("main".to_string()),
            follow_branch: false,
        }];
        assert!(valid_repo_vector(&repos));
        for root in ["app", "app/", "app/nested", "app//nested"] {
            let mut overlapping = repos[0].clone();
            overlapping.repo_id = Uuid::new_v4();
            overlapping.root_path = root.into();
            assert!(!valid_repo_vector(&[repos[0].clone(), overlapping]));
        }
        assert!(!valid_repo_vector(&[]));
        repos.push(VmRepoSpec {
            repo_id: Uuid::new_v4(),
            root_path: "../escape".to_string(),
            role: None,
            git_url: "https://example.invalid/docs.git".to_string(),
            branch: "workspace/other".to_string(),
            branch_oid: "b".repeat(40),
            upstream_branch: Some("main".to_string()),
            follow_branch: false,
        });
        assert!(!valid_repo_vector(&repos));
    }

    #[tokio::test]
    async fn invalid_later_restore_member_leaves_earlier_repository_untouched() {
        let temp = tempfile::tempdir().unwrap();
        let fixture = crate::test_support::HostFixture::new(temp.path());
        let root = temp.path().join("restore");
        std::fs::create_dir_all(root.join("app")).unwrap();
        git(&root.join("app"), &["init", "-q"]);
        let config = root.join("app/.git/config");
        let before = std::fs::read(&config).unwrap();
        let mut spec = fixture.spec(Uuid::new_v4(), Uuid::new_v4(), 1);
        let mut later = spec.repos[0].clone();
        later.repo_id = Uuid::new_v4();
        later.root_path = "docs".into();
        later.branch_oid = "not-a-commit".into();
        spec.repos.push(later);
        let runner = g5_scm::native::NativeGit::host(
            g5d_foundation::workspace::repo_cache::host_git_environment(),
            Duration::from_secs(5),
        )
        .unwrap();
        let error = restore_workspace_with_git(&root, &spec, "", &runner)
            .await
            .unwrap_err();
        assert!(error.to_string().contains("invalid restore vector"));
        assert_eq!(std::fs::read(config).unwrap(), before);
        assert!(!root.join("docs").exists());
        assert!(!root.join("app/.git/FETCH_HEAD").exists());
    }

    /// A runtime over `workspaces_root` that never launches a VM.
    async fn runtime_without_vms(base: &Path, workspaces_root: &Path) -> WorkspaceVmRuntime {
        WorkspaceVmRuntime::new(
            WorkspaceVmRuntimeConfig {
                org_id: Uuid::new_v4(),
                slots_total: 1,
                cpus_per_vm: 2,
                mem_mib_per_vm: 4096,
                scratch_disk_mib: 0,
                block_workload: false,
                nested_virtualization: false,
                api_url: "http://api.invalid".to_string(),

                runtime_root: base.join("runtime"),
                workspaces_root: workspaces_root.to_path_buf(),
                process_host_executable: None,
                dispatcher_ready_timeout: Duration::ZERO,
            },
            Arc::new(NeverLauncher),
            Arc::new(WorkspaceCheckout::new(
                g5d_foundation::workspace::RepoCache::new(base.join("cache")),
                workspaces_root.to_path_buf(),
            )),
            Arc::new(EventJournal::open(base.join("journal.json")).await.unwrap()),
            Arc::new(LeaseRenewalBroker::default()),
            Arc::new(crate::runtime::NoOtherSlotUsage),
        )
    }

    /// Boot A left its record and awaits a slow kill; a later boot B of the
    /// same workspace replaced the tree and then failed without reap proof.
    /// A's kill completing must end only A's hold, never B's.
    #[tokio::test]
    async fn a_stale_kill_completion_keeps_a_newer_vms_hold() {
        let temp = tempfile::tempdir().unwrap();
        let workspaces_root = temp.path().join("workspaces");
        let workspace_id = Uuid::new_v4();
        let root = workspaces_root.join(workspace_id.to_string());
        std::fs::create_dir_all(root.join("app")).unwrap();
        std::fs::write(root.join("app/README.md"), b"B's tree").unwrap();
        let runtime = runtime_without_vms(temp.path(), &workspaces_root).await;
        let (vm_a, vm_b) = (Uuid::new_v4(), Uuid::new_v4());
        runtime
            .state
            .lock()
            .await
            .unreaped
            .insert(vm_b, workspace_id);

        runtime.release_after_kill(workspace_id, vm_a, true).await;
        runtime.roots.delete_claimed().await;
        assert!(
            root.join("app/README.md").is_file(),
            "B's possibly live VMM keeps its root"
        );

        runtime.release_after_kill(workspace_id, vm_b, true).await;
        runtime.roots.delete_claimed().await;
        assert!(!root.exists(), "B's proven reap releases the root");
    }

    #[tokio::test]
    async fn host_start_deletes_every_root_and_leaves_other_entries() {
        let temp = tempfile::tempdir().unwrap();
        let workspaces_root = temp.path().join("workspaces");
        let outside = temp.path().join("outside.txt");
        std::fs::write(&outside, b"host file").unwrap();
        // A released root with build output and a guest-written symlink to a
        // host path: deletion removes the link, never its target.
        let root = workspaces_root.join(Uuid::new_v4().to_string());
        std::fs::create_dir_all(root.join("app/target")).unwrap();
        std::fs::write(root.join("app/target/output.bin"), b"built").unwrap();
        std::os::unix::fs::symlink(&outside, root.join("app/escape")).unwrap();
        // Copies an interrupted materialization left behind.
        let workspace_id = Uuid::new_v4();
        for kind in ["staging", "previous"] {
            std::fs::create_dir_all(
                workspaces_root.join(format!(".{workspace_id}.{kind}-{}", Uuid::new_v4())),
            )
            .unwrap();
        }
        // Retention bookkeeping from hosts that kept released roots.
        std::fs::create_dir_all(workspaces_root.join(".g5-retained")).unwrap();
        std::fs::write(
            workspaces_root.join(format!(".g5-retained/{workspace_id}.receipt.json")),
            b"{}",
        )
        .unwrap();
        // A root an earlier deletion pass failed to delete.
        std::fs::create_dir_all(workspaces_root.join(format!(".trash-{}", Uuid::new_v4())))
            .unwrap();
        // Entries the host did not create stay, including ones that only look
        // like ours.
        std::fs::write(workspaces_root.join("notes.txt"), b"operator").unwrap();
        let foreign = Uuid::new_v4().to_string().to_uppercase();
        std::fs::create_dir_all(workspaces_root.join(&foreign)).unwrap();
        let lookalikes = [
            ".trash-operator-backup".to_string(),
            format!(".{workspace_id}.staging-operator-backup"),
            format!(".{workspace_id}.previous-operator-backup"),
        ];
        for name in &lookalikes {
            std::fs::create_dir_all(workspaces_root.join(name)).unwrap();
        }

        let runtime = runtime_without_vms(temp.path(), &workspaces_root).await;
        runtime.release_unheld_roots().await;
        assert!(
            !root.exists(),
            "the root is claimed before host start serves"
        );
        // Wait for the background pass by running one more behind it.
        runtime.roots.delete_claimed().await;

        let mut remaining = std::fs::read_dir(&workspaces_root)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().into_string().unwrap())
            .collect::<Vec<_>>();
        remaining.sort();
        let mut expected = vec![foreign, "notes.txt".to_string()];
        expected.extend(lookalikes);
        expected.sort();
        assert_eq!(remaining, expected);
        assert_eq!(std::fs::read(&outside).unwrap(), b"host file");
    }

    #[tokio::test]
    async fn preparing_a_retained_root_replaces_guest_controlled_git_configuration() {
        let reflink_test_root = std::env::var_os("G5_TEST_REFLINK_ROOT")
            .map(PathBuf::from)
            .unwrap_or_else(|| std::env::current_dir().unwrap());
        std::fs::create_dir_all(&reflink_test_root).unwrap();
        let temp = tempfile::Builder::new()
            .prefix("g5-retained-root-")
            .tempdir_in(reflink_test_root)
            .unwrap();
        let source = temp.path().join("source");
        let bare = temp.path().join("origin.git");
        std::fs::create_dir_all(&source).unwrap();
        git(&source, &["init", "-q", "-b", "shared"]);
        std::fs::write(source.join("base.txt"), b"base\n").unwrap();
        git(&source, &["add", "base.txt"]);
        git(&source, &["commit", "-q", "-m", "base"]);
        let branch_oid = git(&source, &["rev-parse", "HEAD"]);
        std::fs::create_dir_all(&bare).unwrap();
        git(&bare, &["init", "-q", "--bare"]);
        git(
            &source,
            &["remote", "add", "origin", bare.to_str().unwrap()],
        );
        git(&source, &["push", "-q", "origin", "shared"]);

        let workspace_id = Uuid::new_v4();
        let workspace_root = temp.path().join("workspaces");
        let retained = workspace_root.join(workspace_id.to_string());
        std::fs::create_dir_all(&workspace_root).unwrap();
        git(
            temp.path(),
            &[
                "clone",
                "-q",
                "--branch",
                "shared",
                bare.to_str().unwrap(),
                retained.join("app").to_str().unwrap(),
            ],
        );
        // The production spec carries uid 1000. Root (the CI lanes)
        // performs the real transition; a developer-run native host is not
        // root and must still prepare the root, leaving it under its own
        // identity.
        // SAFETY: geteuid/getegid have no failure modes and touch no memory.
        let (module_uid, module_gid) = if unsafe { libc::geteuid() } == 0 {
            (1000, 1000)
        } else {
            unsafe { (libc::geteuid(), libc::getegid()) }
        };
        let spec = VmSpec {
            spec_version: VM_SPEC_VERSION,
            dispatcher_contract_version: g5_internal_api::host_ctl::DISPATCHER_CONTRACT_VERSION,
            incarnation: g5_internal_api::host_ctl::VmIncarnationSpec {
                workspace_id,
                vm_id: Uuid::new_v4(),
                epoch: 2,
                org_id: Uuid::new_v4(),
                owner_user_id: Uuid::new_v4(),
                lease_ttl_secs: 300,
                idle_timeout_secs: 900,
            },
            image: g5_internal_api::host_ctl::VmImageSpec {
                image_ref: format!("example.invalid/runtime@sha256:{}", "a".repeat(64)),
                digest: format!("sha256:{}", "a".repeat(64)),
            },
            size: g5_internal_api::host_ctl::VmSizeSpec {
                cpus: 2,
                mem_mib: 4096,
                scratch_disk_mib: 0,
            },
            workspace_root: g5_internal_api::host_ctl::WorkspaceRootSpec {
                retain_hours: 24,
                cache_generation: 1,
            },
            repos: vec![VmRepoSpec {
                repo_id: Uuid::new_v4(),
                root_path: "app".to_string(),
                role: None,
                git_url: bare.to_string_lossy().into_owned(),
                branch: "shared".to_string(),
                branch_oid,
                upstream_branch: Some("main".to_string()),
                follow_branch: false,
            }],
            reference_repos: Vec::new(),
            restore: None,
            env: BTreeMap::from([
                ("G5D_MODULE_UID".to_string(), "1000".to_string()),
                ("G5D_MODULE_GID".to_string(), "1000".to_string()),
            ]),
            network: g5_internal_api::host_ctl::VmNetwork::Passt,
            entrypoint: vec!["/opt/g5/bin/g5d".to_string(), "dispatch".to_string()],
            module_catalog_version: "test".to_string(),
            module_catalog_hash: "b".repeat(64),
        };
        std::fs::create_dir_all(retained.join(".g5")).unwrap();
        std::fs::write(retained.join(".g5/guest-state"), b"must disappear").unwrap();
        git(
            &retained.join("app"),
            &["config", "core.fsmonitor", "/guest/controlled/program"],
        );
        let runtime = WorkspaceVmRuntime::new(
            WorkspaceVmRuntimeConfig {
                org_id: spec.incarnation.org_id,
                slots_total: 1,
                cpus_per_vm: 2,
                mem_mib_per_vm: 4096,
                scratch_disk_mib: 0,
                block_workload: false,
                nested_virtualization: false,
                api_url: "http://api.invalid".to_string(),

                runtime_root: temp.path().join("runtime"),
                workspaces_root: workspace_root.clone(),
                process_host_executable: None,
                dispatcher_ready_timeout: Duration::ZERO,
            },
            Arc::new(NeverLauncher),
            Arc::new(WorkspaceCheckout::new(
                g5d_foundation::workspace::RepoCache::new(temp.path().join("cache")),
                workspace_root.clone(),
            )),
            Arc::new(
                EventJournal::open(temp.path().join("journal.json"))
                    .await
                    .unwrap(),
            ),
            Arc::new(LeaseRenewalBroker::default()),
            Arc::new(crate::runtime::NoOtherSlotUsage),
        );

        let prepared = match runtime.prepare_workspace_root(&spec, "test-token").await {
            Ok((prepared, _)) => prepared,
            Err(error)
                if cfg!(target_os = "macos")
                    && format!("{error:#}").contains("reflinks unsupported") =>
            {
                // The production host is Linux and requires reflink support.
                // macOS coreutils cannot exercise that materializer boundary.
                return;
            }
            Err(error) => panic!("prepare retained root: {error:#}"),
        };

        assert!(!prepared.join(".g5/guest-state").exists());
        let config = std::fs::read_to_string(prepared.join("app/.git/config")).unwrap();
        assert!(!config.contains("/guest/controlled/program"));
        let layout = std::fs::read_to_string(prepared.join(".g5/layout.json")).unwrap();
        assert!(
            !layout.contains("test-token"),
            "the incarnation token never reaches the host-written layout"
        );
        // The module identity from the spec owns every entry of the tree the
        // guest will mount, including the host-written layout files.
        use std::os::unix::fs::MetadataExt;
        for path in [
            prepared.clone(),
            prepared.join(".g5"),
            prepared.join(".g5/layout.json"),
            prepared.join("app"),
            prepared.join("app/.git/config"),
        ] {
            let metadata = std::fs::metadata(&path).unwrap();
            assert_eq!(metadata.uid(), module_uid, "{path:?}");
            // Non-root preparation deliberately skips chown. On macOS the
            // new tree inherits its parent group, not necessarily getegid().
            if g5d_foundation::ownership::can_hand_over() {
                assert_eq!(metadata.gid(), module_gid, "{path:?}");
            }
        }
    }

    /// A spec with no members yet, for personal-workspace reference tests.
    fn personal_test_spec() -> VmSpec {
        VmSpec {
            spec_version: VM_SPEC_VERSION,
            dispatcher_contract_version: g5_internal_api::host_ctl::DISPATCHER_CONTRACT_VERSION,
            incarnation: g5_internal_api::host_ctl::VmIncarnationSpec {
                workspace_id: Uuid::new_v4(),
                vm_id: Uuid::new_v4(),
                epoch: 1,
                org_id: Uuid::new_v4(),
                owner_user_id: Uuid::new_v4(),
                lease_ttl_secs: 300,
                idle_timeout_secs: 900,
            },
            image: g5_internal_api::host_ctl::VmImageSpec {
                image_ref: format!("example.invalid/runtime@sha256:{}", "a".repeat(64)),
                digest: format!("sha256:{}", "a".repeat(64)),
            },
            size: g5_internal_api::host_ctl::VmSizeSpec {
                cpus: 2,
                mem_mib: 4096,
                scratch_disk_mib: 0,
            },
            workspace_root: g5_internal_api::host_ctl::WorkspaceRootSpec {
                retain_hours: 24,
                cache_generation: 1,
            },
            repos: Vec::new(),
            reference_repos: Vec::new(),
            restore: None,
            env: BTreeMap::new(),
            network: g5_internal_api::host_ctl::VmNetwork::Passt,
            entrypoint: vec!["/opt/g5/bin/g5d".to_string(), "dispatch".to_string()],
            module_catalog_version: "test".to_string(),
            module_catalog_hash: "b".repeat(64),
        }
    }

    #[test]
    fn reference_checkouts_must_be_distinct_nonoverlapping_and_exact() {
        let member = VmRepoSpec {
            repo_id: Uuid::new_v4(),
            root_path: "personal".to_string(),
            role: Some("code".to_string()),
            git_url: "http://api.invalid/git/member".to_string(),
            branch: "users/alice".to_string(),
            branch_oid: "a".repeat(40),
            upstream_branch: None,
            follow_branch: false,
        };
        let reference = |root: &str, repo_id: Uuid, commit: String| VmReferenceRepoSpec {
            repo_id,
            root_path: root.to_string(),
            role: Some("code".to_string()),
            git_url: "http://api.invalid/git/reference".to_string(),
            branch: "main".to_string(),
            commit,
        };
        let spec = |references: Vec<VmReferenceRepoSpec>| {
            let mut spec = personal_test_spec();
            spec.repos = vec![member.clone()];
            spec.reference_repos = references;
            spec
        };
        let code = Uuid::new_v4();
        assert!(valid_reference_vector(&spec(vec![
            reference("code", code, "b".repeat(40)),
            reference("docs", Uuid::new_v4(), "c".repeat(40)),
        ])));
        assert!(!valid_reference_vector(&spec(vec![reference(
            "personal/inner",
            code,
            "b".repeat(40)
        )])));
        assert!(!valid_reference_vector(&spec(vec![reference(
            "code",
            member.repo_id,
            "b".repeat(40)
        )])));
        assert!(!valid_reference_vector(&spec(vec![
            reference("code", code, "b".repeat(40)),
            reference("code/sub", Uuid::new_v4(), "c".repeat(40)),
        ])));
        assert!(!valid_reference_vector(&spec(vec![reference(
            "code",
            code,
            "main".into()
        )])));
        assert!(!valid_reference_vector(&spec(vec![reference(
            "../x",
            code,
            "b".repeat(40)
        )])));
    }

    #[tokio::test]
    async fn reference_checkouts_materialize_at_their_pin_and_are_rebuilt_each_start() {
        let temp = tempfile::tempdir().unwrap();
        let bare = |name: &str| {
            let path = temp.path().join(format!("{name}.git"));
            std::fs::create_dir_all(&path).unwrap();
            git(&path, &["init", "-q", "--bare"]);
            path
        };
        let seed = |name: &str, branch: &str, origin: &Path, files: &[(&str, &str)]| {
            let source = temp.path().join(format!("{name}-source"));
            std::fs::create_dir_all(&source).unwrap();
            git(&source, &["init", "-q", "-b", branch]);
            git(
                &source,
                &["remote", "add", "origin", origin.to_str().unwrap()],
            );
            let mut commits = Vec::new();
            for (path, content) in files {
                std::fs::write(source.join(path), content).unwrap();
                git(&source, &["add", path]);
                git(&source, &["commit", "-q", "-m", path]);
                commits.push(git(&source, &["rev-parse", "HEAD"]));
            }
            git(&source, &["push", "-q", "origin", branch]);
            commits
        };
        let personal = bare("personal");
        let personal_head = seed(
            "personal",
            "users/alice",
            &personal,
            &[("notes.md", "mine\n")],
        );
        let code = bare("code");
        let code_commits = seed(
            "code",
            "main",
            &code,
            &[("old.txt", "pinned\n"), ("new.txt", "after the pin\n")],
        );

        let workspace_root = temp.path().join("workspaces");
        std::fs::create_dir_all(&workspace_root).unwrap();
        let mut spec = personal_test_spec();
        spec.repos = vec![VmRepoSpec {
            repo_id: Uuid::new_v4(),
            root_path: "personal".to_string(),
            role: Some("code".to_string()),
            git_url: personal.to_string_lossy().into_owned(),
            branch: "users/alice".to_string(),
            branch_oid: personal_head[0].clone(),
            upstream_branch: None,
            follow_branch: false,
        }];
        spec.reference_repos = vec![VmReferenceRepoSpec {
            repo_id: Uuid::new_v4(),
            root_path: "code".to_string(),
            role: Some("docs".to_string()),
            git_url: code.to_string_lossy().into_owned(),
            branch: "main".to_string(),
            commit: code_commits[0].clone(),
        }];
        let runtime = WorkspaceVmRuntime::new(
            WorkspaceVmRuntimeConfig {
                org_id: spec.incarnation.org_id,
                slots_total: 1,
                cpus_per_vm: 2,
                mem_mib_per_vm: 4096,
                scratch_disk_mib: 0,
                block_workload: false,
                nested_virtualization: false,
                api_url: "http://api.invalid".to_string(),
                runtime_root: temp.path().join("runtime"),
                workspaces_root: workspace_root.clone(),
                process_host_executable: None,
                dispatcher_ready_timeout: Duration::ZERO,
            },
            Arc::new(NeverLauncher),
            // Copies stand in for reflinks on a filesystem without them;
            // the materialization logic under test is the same.
            Arc::new(
                WorkspaceCheckout::new(
                    g5d_foundation::workspace::RepoCache::new(temp.path().join("cache")),
                    workspace_root.clone(),
                )
                .with_copy_when_reflink_unsupported(true),
            ),
            Arc::new(
                EventJournal::open(temp.path().join("journal.json"))
                    .await
                    .unwrap(),
            ),
            Arc::new(LeaseRenewalBroker::default()),
            Arc::new(crate::runtime::NoOtherSlotUsage),
        );

        let prepared = match runtime.prepare_workspace_root(&spec, "test-token").await {
            Ok((prepared, _)) => prepared,
            Err(error) if format!("{error:#}").contains("reflinks unsupported") => {
                eprintln!("skipping: the test filesystem has no reflink support");
                return;
            }
            Err(error) => panic!("prepare personal root: {error:#}"),
        };
        let reference = prepared.join("code");
        assert_eq!(git(&reference, &["rev-parse", "HEAD"]), code_commits[0]);
        assert!(reference.join("old.txt").is_file());
        assert!(
            !reference.join("new.txt").exists(),
            "the pin, not the branch tip"
        );
        // Its origin tracks the branch, so an ordinary pull brings in Base's
        // newer commits and keeps origin/main current.
        assert_eq!(
            git(&reference, &["config", "--get-all", "remote.origin.fetch"]),
            "+refs/heads/main:refs/remotes/origin/main"
        );
        assert_eq!(
            git(&reference, &["rev-parse", "refs/remotes/origin/main"]),
            code_commits[1]
        );
        assert!(prepared.join("personal/notes.md").is_file());
        assert!(
            !prepared.join("project_docs").exists(),
            "a reference docs checkout gets no Project Docs alias"
        );
        let layout: serde_json::Value =
            serde_json::from_slice(&std::fs::read(prepared.join(".g5/layout.json")).unwrap())
                .unwrap();
        assert_eq!(layout["repos"].as_array().unwrap().len(), 1);
        assert_eq!(layout["references"][0]["root_path"], json!("code"));

        // An edit to a reference checkout is discarded at the next Start, which
        // checks out the newer pin.
        std::fs::write(reference.join("old.txt"), "edited\n").unwrap();
        spec.reference_repos[0].commit = code_commits[1].clone();
        let prepared = runtime
            .prepare_workspace_root(&spec, "test-token")
            .await
            .expect("prepare again")
            .0;
        let reference = prepared.join("code");
        assert_eq!(git(&reference, &["rev-parse", "HEAD"]), code_commits[1]);
        assert_eq!(
            std::fs::read_to_string(reference.join("old.txt")).unwrap(),
            "pinned\n"
        );
        assert!(reference.join("new.txt").is_file());

        let source = temp.path().join("code-source");
        std::fs::write(source.join("newest.txt"), "after Start\n").unwrap();
        git(&source, &["add", "newest.txt"]);
        git(&source, &["commit", "-q", "-m", "newest"]);
        git(&source, &["push", "-q", "origin", "main"]);
        git(&reference, &["pull", "-q", "--ff-only", "origin", "main"]);
        assert!(reference.join("newest.txt").is_file());
        assert_eq!(
            git(&reference, &["rev-parse", "refs/remotes/origin/main"]),
            git(&reference, &["rev-parse", "HEAD"])
        );
    }

    #[cfg(unix)]
    #[test]
    fn receipt_restore_uses_required_host_configuration() {
        use std::os::unix::fs::PermissionsExt;
        let temp = tempfile::tempdir().unwrap();
        let config = temp.path().join("gitconfig");
        std::fs::write(&config, "[g5]\n    restoreProbe = true\n").unwrap();
        let found = std::process::Command::new("which")
            .arg("git")
            .output()
            .unwrap();
        assert!(found.status.success());
        let real_git = String::from_utf8(found.stdout).unwrap().trim().to_owned();
        let quote = |value: &str| format!("'{}'", value.replace('\'', "'\\''"));
        let wrapper = temp.path().join("git");
        std::fs::write(&wrapper, format!(
            "#!/bin/sh\n[ \"$GIT_CONFIG_GLOBAL\" = {} ] || {{ echo 'missing host Git configuration' >&2; exit 91; }}\nexec {} \"$@\"\n",
            quote(config.to_str().unwrap()), quote(&real_git),
        )).unwrap();
        std::fs::set_permissions(&wrapper, std::fs::Permissions::from_mode(0o755)).unwrap();
        let mut paths = vec![temp.path().to_path_buf()];
        paths.extend(std::env::split_paths(&std::env::var_os("PATH").unwrap()));
        // Fixture preparation invokes the real binary; the actual restore path
        // must preserve the required config through its host-owned runner.
        let result = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "workspace_vm::tests::receipt_restore_reproduces_dirty_tree_and_null_restore_resets_warm_root", "--nocapture"])
            .env("G5_TEST_RESTORE_HOST_GIT", real_git)
            .env("PATH", std::env::join_paths(paths).unwrap())
            .env("GIT_CONFIG_GLOBAL", config)
            .output().unwrap();
        assert!(
            result.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&result.stdout),
            String::from_utf8_lossy(&result.stderr)
        );
    }

    #[tokio::test]
    async fn receipt_restore_reproduces_dirty_tree_and_null_restore_resets_warm_root() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("source");
        let bare = temp.path().join("origin.git");
        let workspace_root = temp.path().join("retained");
        let repo_root = workspace_root.join("app");
        std::fs::create_dir_all(&source).unwrap();
        git(&source, &["init", "-q", "-b", "shared"]);
        std::fs::write(source.join("base.txt"), b"base\n").unwrap();
        std::fs::create_dir_all(source.join(".g5/memory/claude")).unwrap();
        std::fs::write(source.join(".g5/memory/claude/MEMORY.md"), "old lesson").unwrap();
        std::fs::write(source.join(".g5/memory/claude/obsolete.md"), "obsolete").unwrap();
        git(&source, &["add", ".g5/memory"]);
        git(&source, &["add", "base.txt"]);
        git(&source, &["commit", "-q", "-m", "base"]);
        let branch_oid = git(&source, &["rev-parse", "HEAD"]);
        std::fs::create_dir_all(&bare).unwrap();
        git(&bare, &["init", "-q", "--bare"]);
        git(
            &source,
            &["remote", "add", "origin", bare.to_str().unwrap()],
        );
        git(&source, &["push", "-q", "origin", "shared"]);

        std::fs::write(source.join("branch.txt"), b"unpushed branch\n").unwrap();
        git(&source, &["add", "branch.txt"]);
        git(&source, &["commit", "-q", "-m", "local branch"]);
        let local_branch_oid = git(&source, &["rev-parse", "HEAD"]);
        git(&source, &["checkout", "-q", "-b", "foreign", &branch_oid]);
        std::fs::write(source.join("foreign.txt"), b"foreign\n").unwrap();
        git(&source, &["add", "foreign.txt"]);
        git(&source, &["commit", "-q", "-m", "foreign"]);
        let local_head_oid = git(&source, &["rev-parse", "HEAD"]);
        std::fs::write(source.join("dirty.txt"), b"dirty bytes\n").unwrap();
        std::fs::write(
            source.join(".g5/memory/claude/MEMORY.md"),
            "restored lesson",
        )
        .unwrap();
        std::fs::write(source.join(".g5/memory/claude/topic.md"), "restored topic").unwrap();
        std::fs::remove_file(source.join(".g5/memory/claude/obsolete.md")).unwrap();
        std::fs::write(source.join("run.sh"), b"#!/bin/sh\nexit 0\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(
                source.join("run.sh"),
                std::fs::Permissions::from_mode(0o755),
            )
            .unwrap();
            std::os::unix::fs::symlink("dirty.txt", source.join("dirty-link")).unwrap();
        }
        git(&source, &["add", "-A"]);
        git(&source, &["commit", "-q", "-m", "snapshot"]);
        let snapshot_oid = git(&source, &["rev-parse", "HEAD"]);
        let workspace_id = Uuid::new_v4();
        let repo_id = Uuid::new_v4();
        let flush_id = Uuid::new_v4();
        git(
            &source,
            &[
                "push",
                "-q",
                "origin",
                &format!("{snapshot_oid}:refs/g5/flush/{workspace_id}/{flush_id}/snapshot"),
                &format!("{local_branch_oid}:refs/g5/flush/{workspace_id}/{flush_id}/branch"),
            ],
        );

        std::fs::create_dir_all(&workspace_root).unwrap();
        git(
            temp.path(),
            &[
                "clone",
                "-q",
                "--branch",
                "shared",
                bare.to_str().unwrap(),
                repo_root.to_str().unwrap(),
            ],
        );
        std::fs::create_dir_all(workspace_root.join(".g5")).unwrap();
        let repo = VmRepoSpec {
            repo_id,
            root_path: "app".into(),
            role: None,
            git_url: bare.to_string_lossy().into_owned(),
            branch: "shared".into(),
            branch_oid: branch_oid.clone(),
            upstream_branch: Some("main".into()),
            follow_branch: false,
        };
        let mut spec = VmSpec {
            spec_version: VM_SPEC_VERSION,
            dispatcher_contract_version: g5_internal_api::host_ctl::DISPATCHER_CONTRACT_VERSION,
            incarnation: g5_internal_api::host_ctl::VmIncarnationSpec {
                workspace_id,
                vm_id: Uuid::new_v4(),
                epoch: 1,
                org_id: Uuid::new_v4(),
                owner_user_id: Uuid::new_v4(),
                lease_ttl_secs: 300,
                idle_timeout_secs: 900,
            },
            image: g5_internal_api::host_ctl::VmImageSpec {
                image_ref: format!("example.invalid/runtime@sha256:{}", "a".repeat(64)),
                digest: format!("sha256:{}", "a".repeat(64)),
            },
            size: g5_internal_api::host_ctl::VmSizeSpec {
                cpus: 2,
                mem_mib: 4096,
                scratch_disk_mib: 0,
            },
            workspace_root: g5_internal_api::host_ctl::WorkspaceRootSpec {
                retain_hours: 24,
                cache_generation: 1,
            },
            repos: vec![repo],
            reference_repos: Vec::new(),
            restore: Some(g5_internal_api::host_ctl::VmRestoreSpec {
                flush_id,
                repos: vec![g5_internal_api::host_ctl::VmRestoreRepoSpec {
                    repo_id,
                    branch_oid: branch_oid.clone(),
                    local_branch_oid: local_branch_oid.clone(),
                    local_head_oid: local_head_oid.clone(),
                    head_ref: Some("refs/heads/foreign".into()),
                    sequencer: g5_internal_api::host_ctl::VmSequencer::Rebase,
                    snapshot_oid: snapshot_oid.clone(),
                    dirty: true,
                }],
            }),
            env: BTreeMap::new(),
            network: g5_internal_api::host_ctl::VmNetwork::Passt,
            entrypoint: vec!["/opt/g5/bin/g5d".into(), "dispatch".into()],
            module_catalog_version: "v1".into(),
            module_catalog_hash: "b".repeat(64),
        };

        restore_workspace(&workspace_root, &spec, "test-token")
            .await
            .unwrap();
        assert_eq!(git(&repo_root, &["rev-parse", "HEAD"]), local_head_oid);
        assert_eq!(
            git(&repo_root, &["rev-parse", "refs/heads/shared"]),
            local_branch_oid
        );
        assert_eq!(
            std::fs::read(repo_root.join("dirty.txt")).unwrap(),
            b"dirty bytes\n"
        );
        assert!(workspace_root.join(".g5/restore-interrupted.json").exists());
        assert_eq!(
            std::fs::read_to_string(repo_root.join(".g5/memory/claude/MEMORY.md")).unwrap(),
            "restored lesson"
        );
        assert_eq!(
            std::fs::read_to_string(repo_root.join(".g5/memory/claude/topic.md")).unwrap(),
            "restored topic"
        );
        assert!(!repo_root.join(".g5/memory/claude/obsolete.md").exists());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(repo_root.join("run.sh"))
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o111,
                0o111
            );
            assert_eq!(
                std::fs::read_link(repo_root.join("dirty-link")).unwrap(),
                PathBuf::from("dirty.txt")
            );
        }

        std::fs::write(repo_root.join("stale.txt"), b"stale\n").unwrap();
        spec.restore = None;
        restore_workspace(&workspace_root, &spec, "test-token")
            .await
            .unwrap();
        assert_eq!(
            git(&repo_root, &["symbolic-ref", "HEAD"]),
            "refs/heads/shared"
        );
        assert_eq!(git(&repo_root, &["rev-parse", "HEAD"]), branch_oid);
        assert_eq!(
            std::fs::read_to_string(repo_root.join(".g5/memory/claude/MEMORY.md")).unwrap(),
            "old lesson"
        );
        assert!(repo_root.join(".g5/memory/claude/obsolete.md").exists());
        assert!(!repo_root.join(".g5/memory/claude/topic.md").exists());
        assert!(!repo_root.join("stale.txt").exists());
        assert!(!workspace_root.join(".g5/restore-interrupted.json").exists());
    }

    /// The repository checkouts are handed to the guest module identity so a
    /// uid-1000 shell or agent can write files and lock refs; `.g5/` stays
    /// host-owned and symlinks are re-owned, never followed.
    #[tokio::test]
    async fn prepared_repository_roots_are_owned_by_the_module_identity() {
        use std::os::unix::fs::MetadataExt;
        let temp = tempfile::tempdir().unwrap();
        let workspace_root = temp.path().join("ws");
        let repo_root = workspace_root.join("app");
        std::fs::create_dir_all(repo_root.join(".git/refs/heads")).unwrap();
        std::fs::create_dir_all(workspace_root.join(".g5")).unwrap();
        std::fs::write(repo_root.join("file.txt"), b"x\n").unwrap();
        std::fs::write(workspace_root.join(".g5/layout.json"), b"{}").unwrap();
        let outside = temp.path().join("outside.txt");
        std::fs::write(&outside, b"outside\n").unwrap();
        std::os::unix::fs::symlink(&outside, repo_root.join("link")).unwrap();

        // The current identity is the only one a non-root test can chown to;
        // under root (the CI lanes) this still exercises the real handover.
        let uid = unsafe { libc::getuid() };
        let gid = unsafe { libc::getgid() };
        let mut env = BTreeMap::new();
        env.insert("G5D_MODULE_UID".to_string(), uid.to_string());
        env.insert("G5D_MODULE_GID".to_string(), gid.to_string());
        let spec = VmSpec {
            spec_version: VM_SPEC_VERSION,
            dispatcher_contract_version: g5_internal_api::host_ctl::DISPATCHER_CONTRACT_VERSION,
            incarnation: g5_internal_api::host_ctl::VmIncarnationSpec {
                workspace_id: Uuid::new_v4(),
                vm_id: Uuid::new_v4(),
                epoch: 1,
                org_id: Uuid::new_v4(),
                owner_user_id: Uuid::new_v4(),
                lease_ttl_secs: 300,
                idle_timeout_secs: 900,
            },
            image: g5_internal_api::host_ctl::VmImageSpec {
                image_ref: format!("example.invalid/runtime@sha256:{}", "a".repeat(64)),
                digest: format!("sha256:{}", "a".repeat(64)),
            },
            size: g5_internal_api::host_ctl::VmSizeSpec {
                cpus: 2,
                mem_mib: 4096,
                scratch_disk_mib: 0,
            },
            workspace_root: g5_internal_api::host_ctl::WorkspaceRootSpec {
                retain_hours: 24,
                cache_generation: 1,
            },
            repos: vec![VmRepoSpec {
                repo_id: Uuid::new_v4(),
                root_path: "app".into(),
                role: None,
                git_url: "http://api.invalid/git/repo".into(),
                branch: "shared".into(),
                branch_oid: "a".repeat(40),
                upstream_branch: Some("main".into()),
                follow_branch: false,
            }],
            reference_repos: Vec::new(),
            restore: None,
            env,
            network: g5_internal_api::host_ctl::VmNetwork::Passt,
            entrypoint: vec!["/opt/g5/bin/g5d".into(), "dispatch".into()],
            module_catalog_version: "v1".into(),
            module_catalog_hash: "b".repeat(64),
        };
        own_workspace_root(&workspace_root, &spec).await.unwrap();
        for path in [
            repo_root.clone(),
            repo_root.join(".git/refs/heads"),
            repo_root.join("file.txt"),
        ] {
            let meta = std::fs::metadata(&path).unwrap();
            assert_eq!((meta.uid(), meta.gid()), (uid, gid), "{}", path.display());
        }
        let link = std::fs::symlink_metadata(repo_root.join("link")).unwrap();
        assert_eq!((link.uid(), link.gid()), (uid, gid));
        assert_eq!(std::fs::read(&outside).unwrap(), b"outside\n");
        // An env without a module identity leaves the tree alone.
        let mut bare = spec.clone();
        bare.env.clear();
        own_workspace_root(&workspace_root, &bare).await.unwrap();
    }

    /// A root materialized from the host's bare cache has no remote-tracking
    /// refs. The guest's association contract requires the shared branch as
    /// `refs/remotes/origin/<branch>` (it never mints one), so the restore
    /// must record it; the base branch is recorded when the origin has it.
    #[tokio::test]
    async fn restore_records_the_internal_tracking_refs_for_the_guest() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("source");
        let bare = temp.path().join("origin.git");
        let workspace_root = temp.path().join("fresh");
        let repo_root = workspace_root.join("app");
        std::fs::create_dir_all(&source).unwrap();
        git(&source, &["init", "-q", "-b", "main"]);
        std::fs::write(source.join("base.txt"), b"base\n").unwrap();
        git(&source, &["add", "base.txt"]);
        git(&source, &["commit", "-q", "-m", "base"]);
        let base_oid = git(&source, &["rev-parse", "HEAD"]);
        git(&source, &["checkout", "-q", "-b", "shared"]);
        std::fs::write(source.join("shared.txt"), b"shared\n").unwrap();
        git(&source, &["add", "shared.txt"]);
        git(&source, &["commit", "-q", "-m", "shared"]);
        let branch_oid = git(&source, &["rev-parse", "HEAD"]);
        std::fs::create_dir_all(&bare).unwrap();
        git(&bare, &["init", "-q", "--bare"]);
        git(
            &source,
            &["remote", "add", "origin", bare.to_str().unwrap()],
        );
        git(&source, &["push", "-q", "origin", "main", "shared"]);

        // Like the cache-materialized root: objects and the local branch are
        // present, remote-tracking refs are not.
        std::fs::create_dir_all(&workspace_root).unwrap();
        git(
            temp.path(),
            &[
                "clone",
                "-q",
                "--branch",
                "shared",
                bare.to_str().unwrap(),
                repo_root.to_str().unwrap(),
            ],
        );
        git(
            &repo_root,
            &["update-ref", "-d", "refs/remotes/origin/shared"],
        );
        git(
            &repo_root,
            &["update-ref", "-d", "refs/remotes/origin/main"],
        );
        assert!(std::process::Command::new("git")
            .args([
                "-C",
                repo_root.to_str().unwrap(),
                "rev-parse",
                "-q",
                "--verify",
                "refs/remotes/origin/shared",
            ])
            .output()
            .unwrap()
            .stdout
            .is_empty());
        std::fs::create_dir_all(workspace_root.join(".g5")).unwrap();

        let spec = VmSpec {
            spec_version: VM_SPEC_VERSION,
            dispatcher_contract_version: g5_internal_api::host_ctl::DISPATCHER_CONTRACT_VERSION,
            incarnation: g5_internal_api::host_ctl::VmIncarnationSpec {
                workspace_id: Uuid::new_v4(),
                vm_id: Uuid::new_v4(),
                epoch: 1,
                org_id: Uuid::new_v4(),
                owner_user_id: Uuid::new_v4(),
                lease_ttl_secs: 300,
                idle_timeout_secs: 900,
            },
            image: g5_internal_api::host_ctl::VmImageSpec {
                image_ref: format!("example.invalid/runtime@sha256:{}", "a".repeat(64)),
                digest: format!("sha256:{}", "a".repeat(64)),
            },
            size: g5_internal_api::host_ctl::VmSizeSpec {
                cpus: 2,
                mem_mib: 4096,
                scratch_disk_mib: 0,
            },
            workspace_root: g5_internal_api::host_ctl::WorkspaceRootSpec {
                retain_hours: 24,
                cache_generation: 1,
            },
            repos: vec![VmRepoSpec {
                repo_id: Uuid::new_v4(),
                root_path: "app".into(),
                role: None,
                git_url: bare.to_string_lossy().into_owned(),
                branch: "shared".into(),
                branch_oid: branch_oid.clone(),
                upstream_branch: Some("main".into()),
                follow_branch: false,
            }],
            reference_repos: Vec::new(),
            restore: None,
            env: BTreeMap::new(),
            network: g5_internal_api::host_ctl::VmNetwork::Passt,
            entrypoint: vec!["/opt/g5/bin/g5d".into(), "dispatch".into()],
            module_catalog_version: "v1".into(),
            module_catalog_hash: "b".repeat(64),
        };

        restore_workspace(&workspace_root, &spec, "test-token")
            .await
            .unwrap();
        assert_eq!(
            git(&repo_root, &["symbolic-ref", "HEAD"]),
            "refs/heads/shared"
        );
        assert_eq!(
            git(&repo_root, &["rev-parse", "refs/remotes/origin/shared"]),
            branch_oid,
            "the shared branch must be recorded as the guest's internal ref"
        );
        assert_eq!(
            git(&repo_root, &["rev-parse", "refs/remotes/origin/main"]),
            base_oid,
            "the base branch backs ahead/behind against base"
        );
    }

    #[tokio::test]
    async fn cancelled_roster_admission_retries_the_same_generation() {
        use crate::ctl::HostControlState;
        let temp = tempfile::tempdir().unwrap();
        let fixture = crate::test_support::HostFixture::new(temp.path());
        let launcher = Arc::new(crate::test_support::FakeLauncher::default());
        let journal = fixture.journal().await;
        let runtime = fixture.runtime(
            launcher,
            journal.clone(),
            Arc::new(LeaseRenewalBroker::default()),
            1,
        );
        let workspace_id = Uuid::new_v4();
        let vm_id = Uuid::new_v4();
        runtime
            .seed_running_vm(fixture.start(workspace_id, vm_id, 1).spec)
            .await;
        let host = crate::runtime::WorkspaceHostState::new(
            Uuid::new_v4(),
            1,
            fixture.workspaces_root.clone(),
            runtime.clone(),
        );
        let locked = runtime.state.lock().await;
        {
            let mut admission = Box::pin(host.reconcile_current_incarnations(7, &[]));
            // Poll all the way to the locked runtime reservation, then cancel.
            // Recording generation 7 before this wait loses this roster forever.
            assert!(matches!(
                futures_util::poll!(&mut admission),
                std::task::Poll::Pending
            ));
        }
        drop(locked);
        host.reconcile_current_incarnations(7, &[]).await;
        tokio::time::timeout(Duration::from_secs(5), async {
            while !journal
                .pending()
                .await
                .iter()
                .any(|e| matches!(e.kind, VmEventKind::StaleKilled))
            {
                tokio::time::sleep(Duration::from_millis(1)).await;
            }
        })
        .await
        .expect("a cancelled generation was not marked applied");
        assert_eq!(runtime.active_count().await, 0);
        runtime
            .seed_running_vm(fixture.start(workspace_id, Uuid::new_v4(), 2).spec)
            .await;
        host.reconcile_current_incarnations(6, &[]).await;
        assert_eq!(
            runtime.inventory().await[0].state,
            HostVmState::Running,
            "stale roster cannot revoke fresh custody"
        );
        runtime.shutdown().await;
    }

    #[tokio::test]
    async fn late_stop_completion_cannot_remove_a_replacement_reservation() {
        let temp = tempfile::tempdir().unwrap();
        let fixture = crate::test_support::HostFixture::new(temp.path());
        let launcher = Arc::new(crate::test_support::FakeLauncher::default());
        let journal = fixture.journal().await;
        let runtime = fixture.runtime(
            launcher.clone(),
            journal.clone(),
            Arc::new(LeaseRenewalBroker::default()),
            1,
        );
        let workspace_id = Uuid::new_v4();
        let vm_id = Uuid::new_v4();
        let publisher = runtime
            .seed_running_vm(fixture.start(workspace_id, vm_id, 1).spec)
            .await;
        launcher.hold_stops(true);
        let stopping = tokio::spawn({
            let runtime = runtime.clone();
            async move {
                runtime
                    .stop(VmStop {
                        workspace_id,
                        vm_id,
                        mode: StopMode::Kill,
                        grace_secs: 0,
                    })
                    .await
            }
        });
        while launcher.stopped().await.is_empty() {
            tokio::task::yield_now().await;
        }
        // Independent exit evidence wins the race with the slow stop call.
        publisher.publish_exit(Some(0));
        assert!(matches!(
            runtime
                .status(VmStatus {
                    workspace_id,
                    vm_id
                })
                .await,
            CommandResult::Stopped { .. }
        ));
        runtime
            .seed_running_vm(fixture.start(workspace_id, vm_id, 2).spec)
            .await;
        launcher.hold_stops(false);
        assert!(matches!(
            stopping.await.unwrap(),
            CommandResult::Stopped { .. }
        ));
        let status = runtime
            .status(VmStatus {
                workspace_id,
                vm_id,
            })
            .await;
        assert!(
            matches!(status, CommandResult::Status { vm } if vm.epoch == 2 && vm.state == HostVmState::Running)
        );
        assert_eq!(
            journal
                .pending()
                .await
                .iter()
                .filter(|e| matches!(e.kind, VmEventKind::Exited { .. }))
                .count(),
            1
        );
        runtime.shutdown().await;
    }

    #[tokio::test(start_paused = true)]
    async fn awaiting_a_cancelled_boot_does_not_wake_itself_without_exit_proof() {
        use std::future::Future;
        let temp = tempfile::tempdir().unwrap();
        let fixture = crate::test_support::HostFixture::new(temp.path());
        let runtime = fixture.runtime(
            Arc::new(crate::test_support::FakeLauncher::default()),
            fixture.journal().await,
            Arc::new(LeaseRenewalBroker::default()),
            1,
        );
        let workspace_id = Uuid::new_v4();
        let vm_id = Uuid::new_v4();
        runtime
            .seed_running_vm(fixture.start(workspace_id, vm_id, 1).spec)
            .await;
        {
            let mut state = runtime.state.lock().await;
            let record = state.active.get_mut(&vm_id).unwrap();
            record.state = HostVmState::Stopping;
            record.handle = None;
            record.cancel.cancel();
        }
        let polls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let waiting = tokio::spawn({
            let runtime = runtime.clone();
            let polls = polls.clone();
            async move {
                let mut waiter = Box::pin(runtime.await_reservation_release(vm_id));
                futures_util::future::poll_fn(|cx| {
                    polls.fetch_add(1, Ordering::Relaxed);
                    waiter.as_mut().poll(cx)
                })
                .await;
            }
        });
        // Keep this task runnable without advancing the paused clock. With no
        // exit evidence or timer tick, the waiter must sleep instead of using
        // its cooperative budget repeatedly and waking itself to run again.
        for _ in 0..16 {
            tokio::task::yield_now().await;
        }
        let observed = polls.load(Ordering::Relaxed);
        assert!(
            (1..=2).contains(&observed),
            "waiter repeatedly scheduled without exit proof: {observed}"
        );
        assert_eq!(
            runtime.active_count().await,
            1,
            "cancellation is not release"
        );
        runtime.state.lock().await.active.remove(&vm_id);
        tokio::time::advance(Duration::from_secs(1)).await;
        waiting.await.unwrap();
    }
}
