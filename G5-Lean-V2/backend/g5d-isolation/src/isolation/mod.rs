//! Pluggable isolation backend system for workspace VMs.
//!
//! This module implements the isolation backend selection, MicroVM and process
//! isolation backends, and runtime detection of available backends.
//!
//! # Architecture
//!
//! - [`IsolationManager`] — Backend selection and lifecycle management
//! - [`MicrovmBackend`] — Trait for all isolation backends
//! - [`CloudHypervisorBackend`] — Linux/KVM with private block disks and passt
//! - [`ProcessBackend`] — Explicit host-process backend
//! - [`detect`] — Runtime detection of available backends

pub mod detect;
pub mod guest_egress;
pub mod handle;
pub mod microvm;
pub mod passt;
pub mod port_map;
pub mod process;
#[cfg(feature = "cloud-hypervisor")]
pub mod vm_exec_args;
pub mod vmm_loopback;
#[cfg(any(feature = "cloud-hypervisor", test))]
pub(crate) mod workload_root;
#[cfg(feature = "cloud-hypervisor")]
pub(crate) mod workload_seed;
#[cfg(any(feature = "cloud-hypervisor", test))]
pub(crate) mod workload_template;

#[cfg(feature = "cloud-hypervisor")]
pub mod cloud_hypervisor;
#[cfg(feature = "cloud-hypervisor")]
pub mod workload_artifact;

pub use detect::{
    check_kvm, detect_backends, find_binary, AvailableBackend, DetectionResult, Platform,
};
pub use handle::TaskIsolationHandle;
pub use microvm::CloudHypervisorBackend;
pub use process::{teardown_incarnation, ProcessBackend, ProcessTag};
#[cfg(feature = "cloud-hypervisor")]
pub use vm_exec_args::VmExecArgs;
#[cfg(feature = "cloud-hypervisor")]
pub(crate) use vmm_loopback::host_vmm_port_map_addr_for_pid;
pub use vmm_loopback::{vmm_loopback_addr_for_pid, VmmLoopbackAddrError};

use std::path::PathBuf;
use std::str::FromStr;
use std::time::Duration;

use clap::ValueEnum;
use serde::{Deserialize, Serialize};
use tokio::sync::watch;

use g5_contracts::error::G5Error;
pub use g5d_foundation::IsolationLevel;

#[cfg(any(target_os = "linux", feature = "cloud-hypervisor", test))]
/// Deprivileged platform children need no supervisor or workspace environment.
/// In particular, their envp must not expose host JWTs or bootstrap credentials.
pub(super) fn platform_command(binary: impl AsRef<std::ffi::OsStr>) -> std::process::Command {
    let mut command = std::process::Command::new(binary);
    command.env_clear();
    command
}

pub(crate) const GRACEFUL_CHILD_REAP_TIMEOUT: Duration = Duration::from_secs(5);
pub(crate) const FORCED_CHILD_REAP_TIMEOUT: Duration = Duration::from_secs(2);

// ---------------------------------------------------------------------------
// MicrovmRunSpec — specification for running a command in isolation
// ---------------------------------------------------------------------------

/// Access mode for a workload input or independent runtime artifact.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MountAccess {
    /// Mount read-write. This is the default for historical mounts.
    ReadWrite,
    /// Attach the independent runtime as a read-only block device.
    ReadOnly,
}

/// Additional host directory mounted into the microVM.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MicrovmMount {
    pub host_path: PathBuf,
    pub guest_path: String,
    pub access: MountAccess,
}

impl MicrovmMount {
    pub fn read_write(host_path: impl Into<PathBuf>, guest_path: impl Into<String>) -> Self {
        Self {
            host_path: host_path.into(),
            guest_path: guest_path.into(),
            access: MountAccess::ReadWrite,
        }
    }

    pub fn read_only(host_path: impl Into<PathBuf>, guest_path: impl Into<String>) -> Self {
        Self {
            host_path: host_path.into(),
            guest_path: guest_path.into(),
            access: MountAccess::ReadOnly,
        }
    }

    pub fn vm_exec_volume_arg(&self) -> String {
        match self.access {
            MountAccess::ReadWrite => {
                format!("{}:{}", self.host_path.display(), self.guest_path)
            }
            MountAccess::ReadOnly => {
                format!("{}:{}:ro", self.host_path.display(), self.guest_path)
            }
        }
    }
}

/// Specification for running a command in an isolated environment.
#[derive(Debug, Clone)]
pub struct MicrovmRunSpec {
    /// OCI image reference (e.g., "ghcr.io/acme/worker:latest").
    pub image: String,
    /// Entrypoint command + arguments.
    pub cmd: Vec<String>,
    /// Environment variables to pass to the isolated environment.
    pub env: Vec<(String, String)>,
    /// Workspace directory on the host to mount into the environment.
    pub mount_host_dir: PathBuf,
    /// Mount point inside the VM/container (e.g., "/work").
    pub mount_guest_dir: String,
    /// Additional volume mounts.
    /// Used for the cache directory, build tool caches, etc.
    pub extra_mounts: Vec<MicrovmMount>,
    /// CPU cap (number of vCPUs).
    pub cpus: u8,
    /// RAM cap in MiB.
    pub mem_mib: u32,
    /// Passt ingress mappings: `(host_port, guest_port)` pairs already allocated by
    /// the caller (typically via [`crate::isolation::port_map::PortAllocator`]).
    /// Empty for tasks that do not request port forwarding. When non-empty,
    /// [`CloudHypervisorBackend`] emits one `-p host_port:guest_port` flag to
    /// `g5d vm-exec` per entry; passt routes each guest port to its
    /// matching host port for the lifetime of the VM. See ontology
    /// D32 (CLI contract) and D33 (allocation policy).
    ///
    /// **Lease-lifetime caller invariant.** This field carries raw `u16` pairs,
    /// not [`crate::isolation::port_map::PortLease`] guards — so the **caller**
    /// is responsible for holding the corresponding `PortLease` (one per
    /// host port listed here) for the entire VM lifetime. Releasing a lease
    /// while its host port is still mapped in a live VM context would allow a
    /// second `PortAllocator::allocate()` to hand the same host port to a
    /// different VM, breaking per-VM port isolation. The workspace runtime
    /// keeps the lease and [`RunHandle`] in the same incarnation scope.
    /// A future refactor MAY tighten this to `Vec<(PortLease, u16)>` so the
    /// invariant is structurally enforced; the current shape is the v1
    /// boundary (broadening it requires churn through every backend impl
    /// including `MockBackend`). See ontology D32 "Lease-lifetime caller
    /// invariant".
    pub port_maps: Vec<(u16, u16)>,
    /// Private complete-workload virtio-blk capacity. Guest flushes are honored.
    pub scratch_disk: Option<ScratchDiskSpec>,
    /// Place the complete workload in a fresh guest-owned ext4 filesystem.
    /// Linux and an owned OCI clone are required; scratch size is total capacity.
    pub block_workload: bool,
    /// Expose hardware virtualization to the guest (`--cpus nested=on`): a host
    /// property (`g5.architecture.microvm_backend.nested`), never a per-VM request.
    pub nested_virtualization: bool,
    /// Host-validated membership; never inferred from guest environment or files.
    pub guest_restore: Option<Box<g5_internal_api::host_ctl::VmStart>>,
}

/// Private complete-workload disk capacity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScratchDiskSpec {
    /// Sparse apparent size in MiB — the per-VM quota; the host filesystem
    /// allocates only written blocks.
    pub size_mib: u32,
}

// ---------------------------------------------------------------------------
// RunResult — result of running a command in isolation
// ---------------------------------------------------------------------------

/// Result of running a command in an isolated environment.
#[derive(Debug, Clone)]
pub struct RunResult {
    /// Process exit code.
    pub exit_code: i32,
    /// Captured stdout.
    pub stdout: String,
    /// Captured stderr.
    pub stderr: String,
    /// Wall-clock duration of execution.
    pub duration: Duration,
}

// ---------------------------------------------------------------------------
// GuestConsoleTail — bounded, redacted console tail for start()-path VMs
// ---------------------------------------------------------------------------

/// Which guest console stream a retained line arrived on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GuestConsoleStream {
    Stdout,
    Stderr,
}

impl GuestConsoleStream {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Stdout => "stdout",
            Self::Stderr => "stderr",
        }
    }
}

/// One retained guest console line: the stream it arrived on plus the
/// already-redacted, lossy-UTF-8-decoded text (`microvm_runtime` D58).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GuestConsoleLine {
    pub stream: GuestConsoleStream,
    pub line: String,
}

/// Total retained bytes across both streams. Matches the supervisor's
/// `GUEST_LOG_TAIL_BYTES` posture: keep the TAIL — the end is where a failing
/// guest prints its error (issue G5Labs/G5#2400; `microvm_runtime` D58).
pub const GUEST_CONSOLE_TAIL_BYTES: usize = 8192;
/// Line-count cap so a guest emitting many tiny lines cannot bloat the
/// per-line bookkeeping.
pub const GUEST_CONSOLE_TAIL_MAX_LINES: usize = 256;

#[derive(Debug, Default)]
struct GuestConsoleTailState {
    lines: std::collections::VecDeque<GuestConsoleLine>,
    bytes: usize,
}

/// Bounded, oldest-evicting tail of a `start()`-path guest's console output
/// (`microvm_runtime` D58). Lines are appended by the console relay AFTER
/// lossy UTF-8 decode and secret redaction, in arrival order across both
/// streams (best-effort interleaving — the only ordering two pipe readers
/// can observe). Reading is non-destructive and remains valid after guest
/// EOF / VM exit / kill.
#[derive(Debug, Default)]
pub struct GuestConsoleTail {
    state: std::sync::Mutex<GuestConsoleTailState>,
}

impl GuestConsoleTail {
    /// Append one redacted line, evicting oldest lines to hold the byte and
    /// line caps. A single line longer than the byte cap is head-truncated to
    /// the cap (the tail of the line is the newest content) on a UTF-8
    /// character boundary.
    pub fn push(&self, stream: GuestConsoleStream, line: &str) {
        let mut kept = line;
        if kept.len() > GUEST_CONSOLE_TAIL_BYTES {
            let mut start = kept.len() - GUEST_CONSOLE_TAIL_BYTES;
            while start < kept.len() && !kept.is_char_boundary(start) {
                start += 1;
            }
            kept = &kept[start..];
        }
        let mut state = self.state.lock().expect("guest console tail poisoned");
        state.bytes += kept.len();
        state.lines.push_back(GuestConsoleLine {
            stream,
            line: kept.to_string(),
        });
        while state.lines.len() > GUEST_CONSOLE_TAIL_MAX_LINES
            || state.bytes > GUEST_CONSOLE_TAIL_BYTES
        {
            let Some(evicted) = state.lines.pop_front() else {
                break;
            };
            state.bytes = state.bytes.saturating_sub(evicted.line.len());
        }
    }

    /// Snapshot of the retained lines in arrival order (non-destructive).
    #[must_use]
    pub fn lines(&self) -> Vec<GuestConsoleLine> {
        self.state
            .lock()
            .expect("guest console tail poisoned")
            .lines
            .iter()
            .cloned()
            .collect()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.state
            .lock()
            .expect("guest console tail poisoned")
            .lines
            .is_empty()
    }
}

// ---------------------------------------------------------------------------
// RunHandle — handle to a running isolated environment
// ---------------------------------------------------------------------------

/// Positive evidence that the backend's sole child waiter observed and reaped
/// the isolated child. Signal delivery alone never creates this receipt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChildReapReceipt {
    exit_code: Option<i32>,
}

impl ChildReapReceipt {
    fn from_exit_status(status: std::process::ExitStatus) -> Self {
        Self {
            exit_code: status.code(),
        }
    }

    /// Process exit code when the platform reported one. Signal exits have no
    /// portable numeric code and therefore return `None`.
    #[must_use]
    pub fn exit_code(&self) -> Option<i32> {
        self.exit_code
    }

    #[cfg(any(test, feature = "test-helpers"))]
    #[must_use]
    pub fn confirmed_for_test(exit_code: Option<i32>) -> Self {
        Self { exit_code }
    }
}

#[derive(Debug, Clone)]
enum ChildReapState {
    Pending,
    Reaped(ChildReapReceipt),
    WaitFailed(String),
}

#[derive(Debug, Clone)]
pub(crate) struct ChildReapPublisher {
    sender: watch::Sender<ChildReapState>,
}

impl ChildReapPublisher {
    pub(crate) fn publish_reaped(&self, status: std::process::ExitStatus) -> ChildReapReceipt {
        let receipt = ChildReapReceipt::from_exit_status(status);
        self.sender
            .send_replace(ChildReapState::Reaped(receipt.clone()));
        receipt
    }

    pub(crate) fn publish_wait_failed(&self, error: impl Into<String>) {
        self.sender
            .send_replace(ChildReapState::WaitFailed(error.into()));
    }
}

#[derive(Debug, Clone)]
pub(crate) struct ChildReapMonitor {
    receiver: watch::Receiver<ChildReapState>,
}

impl ChildReapMonitor {
    async fn wait_for_receipt(&self, bound: Duration) -> anyhow::Result<ChildReapReceipt> {
        let mut receiver = self.receiver.clone();
        let wait = async move {
            loop {
                match receiver.borrow().clone() {
                    ChildReapState::Pending => {}
                    ChildReapState::Reaped(receipt) => return Ok(receipt),
                    ChildReapState::WaitFailed(error) => {
                        anyhow::bail!("child waiter failed before confirming reap: {error}")
                    }
                }
                receiver.changed().await.map_err(|_| {
                    anyhow::anyhow!("child waiter ended without publishing a reap receipt")
                })?;
            }
        };
        tokio::time::timeout(bound, wait).await.map_err(|_| {
            anyhow::anyhow!("timed out waiting for child reap receipt after {bound:?}")
        })?
    }
}

pub(crate) fn child_reap_channel() -> (ChildReapPublisher, ChildReapMonitor) {
    let (sender, receiver) = watch::channel(ChildReapState::Pending);
    (ChildReapPublisher { sender }, ChildReapMonitor { receiver })
}

#[cfg(any(test, feature = "test-helpers"))]
fn pending_child_reap_monitor() -> ChildReapMonitor {
    let (_publisher, monitor) = child_reap_channel();
    monitor
}

/// Test seam paired with [`RunHandle::test_microvm_with_reap_publisher`]: lets
/// a long-lived VM owner observe a natural child exit without a real VMM.
#[cfg(any(test, feature = "test-helpers"))]
#[derive(Debug, Clone)]
pub struct TestChildReapPublisher(ChildReapPublisher);

#[cfg(any(test, feature = "test-helpers"))]
impl TestChildReapPublisher {
    /// Publish a confirmed reap with the given exit code, exactly as the
    /// backend waiter does after `wait()` returns.
    pub fn publish_exit(&self, exit_code: Option<i32>) {
        self.0
            .sender
            .send_replace(ChildReapState::Reaped(ChildReapReceipt { exit_code }));
    }
}

/// Handle to a running isolated environment, used for kill operations.
#[derive(Debug)]
pub struct RunHandle {
    /// Unique identifier for this run (VM name or process ID).
    pub id: String,
    /// Backend type that created this handle.
    pub backend_type: BackendType,
    /// PID-derived host loopback address owning this VM's passt ingress. A
    /// process backend publishes the host loopback, where its child's
    /// listeners live. `None` only for test handles that name no address.
    pub vmm_loopback_addr: Option<std::net::Ipv4Addr>,
    /// Internal state for kill operations (process ID, VM name, etc.).
    pub(crate) inner: RunHandleInner,
    /// Private cloneable observation channel published only by the waiter
    /// that owns and reaps the child.
    reap_monitor: ChildReapMonitor,
    /// Bounded, redacted guest console tail retained by the `start()`-path
    /// console relay (`microvm_runtime` D58). `None` for backends/paths that
    /// do not retain one (`run()`, process backend).
    pub(crate) console_tail: Option<std::sync::Arc<GuestConsoleTail>>,
    /// Drained receipts for the console relay tasks: each `JoinHandle`
    /// completes only after its stream was consumed to EOF, so a post-exit
    /// tail reader can await them (bounded) and never race the final buffered
    /// lines (`microvm_runtime` D58; review round 2 P1). Empty for
    /// backends/paths with no relay.
    pub(crate) console_relay_handles: std::sync::Mutex<Vec<tokio::task::JoinHandle<()>>>,
}

impl RunHandle {
    /// The bounded, redacted guest console tail for this VM, when the
    /// backend retained one (`microvm_runtime` D58). Non-destructive; stays
    /// readable after guest EOF / VM exit / `kill()`.
    #[must_use]
    pub fn console_tail(&self) -> Option<std::sync::Arc<GuestConsoleTail>> {
        self.console_tail.clone()
    }

    /// Await the console relays' drained receipts (stream EOF consumed) for
    /// at most `bound` overall, then return. Best-effort by design: at VM
    /// exit EOF arrives promptly, so the bound is a race guard against
    /// reading the tail before the final buffered lines land — never a
    /// stall. Idempotent: handles are taken on first call; later calls (and
    /// calls on relay-less handles) return immediately.
    pub async fn wait_console_drained(&self, bound: Duration) {
        let handles: Vec<tokio::task::JoinHandle<()>> = {
            let mut guard = self
                .console_relay_handles
                .lock()
                .expect("console relay handles poisoned");
            guard.drain(..).collect()
        };
        if handles.is_empty() {
            return;
        }
        let deadline = tokio::time::Instant::now() + bound;
        for handle in handles {
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            if tokio::time::timeout(remaining, handle).await.is_err() {
                tracing::warn!(
                    vm = %self.id,
                    "console relay not drained within the bound; proceeding with the retained tail"
                );
            }
        }
    }

    /// Test seam: attach a relay `JoinHandle` so drain-receipt consumers can
    /// be exercised without a real VM.
    #[cfg(any(test, feature = "test-helpers"))]
    pub fn test_attach_console_relay_handle(&self, handle: tokio::task::JoinHandle<()>) {
        self.console_relay_handles
            .lock()
            .expect("console relay handles poisoned")
            .push(handle);
    }

    /// PID of the running backend child, when one is available.
    pub fn child_pid(&self) -> Option<u32> {
        match &self.inner {
            RunHandleInner::Microvm { child_pid, .. } => *child_pid,
            RunHandleInner::Process { child_pid, .. } => Some(*child_pid),
        }
    }

    pub(crate) async fn wait_for_reap(&self, bound: Duration) -> anyhow::Result<ChildReapReceipt> {
        self.reap_monitor.wait_for_receipt(bound).await
    }

    /// Return positive child-reap evidence when the backend waiter has already
    /// observed exit. Long-lived workspace VM owners use this to converge a
    /// naturally exited VMM without sending an unnecessary second signal.
    pub fn current_reap_receipt(&self) -> anyhow::Result<Option<ChildReapReceipt>> {
        match self.reap_monitor.receiver.borrow().clone() {
            ChildReapState::Pending => Ok(None),
            ChildReapState::Reaped(receipt) => Ok(Some(receipt)),
            ChildReapState::WaitFailed(error) => {
                anyhow::bail!("child waiter failed before confirming reap: {error}")
            }
        }
    }

    #[cfg(any(test, feature = "test-helpers"))]
    pub fn test_microvm(
        id: impl Into<String>,
        vmm_loopback_addr: Option<std::net::Ipv4Addr>,
        child_pid: Option<u32>,
    ) -> Self {
        let id = id.into();
        Self {
            backend_type: BackendType::CloudHypervisor,
            vmm_loopback_addr,
            inner: RunHandleInner::Microvm {
                vm_name: id.clone(),
                child_pid,
            },
            reap_monitor: pending_child_reap_monitor(),
            console_tail: None,
            console_relay_handles: std::sync::Mutex::new(Vec::new()),

            id,
        }
    }

    /// Like [`Self::test_microvm`] but returning the reap publisher, so a test
    /// can make the handle report a natural exit later.
    #[cfg(any(test, feature = "test-helpers"))]
    pub fn test_microvm_with_reap_publisher(
        id: impl Into<String>,
        vmm_loopback_addr: Option<std::net::Ipv4Addr>,
        child_pid: Option<u32>,
    ) -> (Self, TestChildReapPublisher) {
        let (publisher, monitor) = child_reap_channel();
        let mut handle = Self::test_microvm(id, vmm_loopback_addr, child_pid);
        handle.reap_monitor = monitor;
        (handle, TestChildReapPublisher(publisher))
    }

    /// Like [`Self::test_microvm`] but carrying a pre-populated console tail,
    /// for supervisor-side tests of the D58 tail projection.
    #[cfg(any(test, feature = "test-helpers"))]
    pub fn test_microvm_with_console_tail(
        id: impl Into<String>,
        vmm_loopback_addr: Option<std::net::Ipv4Addr>,
        child_pid: Option<u32>,
        console_tail: std::sync::Arc<GuestConsoleTail>,
    ) -> Self {
        let mut handle = Self::test_microvm(id, vmm_loopback_addr, child_pid);
        handle.console_tail = Some(console_tail);
        handle
    }

    #[cfg(any(test, feature = "test-helpers"))]
    pub fn test_process(
        id: impl Into<String>,
        vmm_loopback_addr: Option<std::net::Ipv4Addr>,
        child_pid: u32,
    ) -> Self {
        Self {
            id: id.into(),
            backend_type: BackendType::Process,
            vmm_loopback_addr,
            inner: RunHandleInner::Process {
                child_pid,
                pgid: None,
                session_leader: false,
                tag: None,
                started: None,
            },
            reap_monitor: pending_child_reap_monitor(),
            console_tail: None,
            console_relay_handles: std::sync::Mutex::new(Vec::new()),
        }
    }
}

#[cfg(unix)]
struct RunHandleReapWaiter<'a> {
    handle: &'a RunHandle,
    graceful_wait: bool,
}

#[cfg(unix)]
#[async_trait::async_trait]
impl g5d_foundation::process_termination::ReapWaiter for RunHandleReapWaiter<'_> {
    type Receipt = ChildReapReceipt;

    async fn wait_for_reap(&mut self, bound: Duration) -> std::io::Result<Option<Self::Receipt>> {
        let graceful = std::mem::take(&mut self.graceful_wait);
        match self.handle.wait_for_reap(bound).await {
            Ok(receipt) => Ok(Some(receipt)),
            Err(error) if graceful => {
                tracing::warn!(%error, handle_id = %self.handle.id, child_pid = ?self.handle.child_pid(), "Child did not confirm reap after graceful signal; escalating to SIGKILL");
                Ok(None)
            }
            Err(error) => Err(std::io::Error::other(error)),
        }
    }
}

#[cfg(unix)]
pub(crate) async fn terminate_handle_group(
    handle: &RunHandle,
    process_group_id: u32,
    grace: Duration,
    kill_deadline: Duration,
) -> anyhow::Result<ChildReapReceipt> {
    terminate_handle_target(
        handle,
        g5d_foundation::process_termination::TerminationTarget::ProcessGroup(process_group_id),
        grace,
        kill_deadline,
    )
    .await
}

#[cfg(unix)]
pub(crate) async fn terminate_handle_target(
    handle: &RunHandle,
    target: g5d_foundation::process_termination::TerminationTarget,
    grace: Duration,
    kill_deadline: Duration,
) -> anyhow::Result<ChildReapReceipt> {
    use g5d_foundation::process_termination::{terminate, TerminationPolicy};
    let receipt = terminate(
        Some(target),
        TerminationPolicy {
            first_signal: libc::SIGTERM,
            grace,
            kill_deadline,
            always_kill_after_grace: false,
        },
        &mut RunHandleReapWaiter {
            handle,
            graceful_wait: !grace.is_zero(),
        },
    )
    .await
    .map_err(|error| {
        use g5d_foundation::process_termination::TerminationTarget;
        let target = match target {
            TerminationTarget::ProcessGroup(id) => format!("process_group_id={id}"),
            TerminationTarget::Session(id) => format!("session_id={id}"),
        };
        anyhow::anyhow!(
            "terminate handle {} (child_pid={:?}, {target}): {error}",
            handle.id,
            handle.child_pid(),
        )
    })?;
    Ok(receipt)
}

/// Internal state for kill operations.
#[derive(Debug)]
#[cfg_attr(not(feature = "cloud-hypervisor"), allow(dead_code))]
pub(crate) enum RunHandleInner {
    /// Cloud Hypervisor microVM — stores the VM name and child process ID.
    Microvm {
        vm_name: String,
        child_pid: Option<u32>,
    },
    /// Process — stores the child process ID and process group ID. A session
    /// leader (`setsid`) is torn down with every group of its session, because
    /// its children may put their own work in further groups.
    Process {
        child_pid: u32,
        #[allow(dead_code)]
        pgid: Option<u32>,
        session_leader: bool,
        /// `key=value` every process of the incarnation carries in its
        /// environment, so teardown also reaches processes that left the
        /// session (`process::ProcessTag`).
        tag: Option<(String, String)>,
        /// When the leader was started: a process started earlier cannot
        /// belong to the incarnation, which bounds what an unreadable
        /// environment can make uncertain.
        started: Option<std::time::SystemTime>,
    },
}

// ---------------------------------------------------------------------------
// MicrovmBackend trait
// ---------------------------------------------------------------------------

/// Typed marker for a run whose isolated child could NOT be confirmed reaped:
/// a cancelled run whose kill+reap did not complete within the bounded
/// termination deadline (supervisor_lifecycle D25), or a plain `wait()` that
/// failed (microvm_runtime D40). The child may still be alive and holding its
/// mounts, so callers must not treat the run as terminal: workspace cleanup
/// and same-task successor admission stay fenced until an authoritative
/// teardown lane terminalizes the record.
#[derive(Debug)]
pub struct CancelledChildReapUnconfirmed {
    /// Human-readable kill/reap failure detail from the termination attempt.
    pub detail: String,
}

impl std::fmt::Display for CancelledChildReapUnconfirmed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "isolated child could not be confirmed reaped (it may still be live): {}",
            self.detail
        )
    }
}

impl std::error::Error for CancelledChildReapUnconfirmed {}

/// The trait that all isolation backends must implement.
///
/// Provides methods for running commands in isolated environments,
/// starting long-running processes, and killing running environments.
#[async_trait::async_trait]
pub trait MicrovmBackend: Send + Sync {
    /// Boot the OCI image in an isolated environment and run the command.
    /// Returns the run result when the command completes.
    async fn run(&self, spec: &MicrovmRunSpec) -> anyhow::Result<RunResult>;

    /// Like [`run`](MicrovmBackend::run), additionally honoring a
    /// cancellation token (issue #1070, supervisor_lifecycle D25): on
    /// cancellation the backend must kill AND reap the isolated child before
    /// any per-run cleanup. Confirmed termination returns the observed forced
    /// exit as a normal `RunResult`; a bounded termination attempt that cannot
    /// confirm reap returns [`CancelledChildReapUnconfirmed`]. The default
    /// ignores the token and delegates to `run` for backends without a kill
    /// lane (the force-evict grace then fails loudly at the supervisor instead).
    async fn run_cancellable(
        &self,
        spec: &MicrovmRunSpec,
        cancel: &tokio_util::sync::CancellationToken,
    ) -> anyhow::Result<RunResult> {
        let _ = cancel;
        self.run(spec).await
    }

    /// Start the environment and return a handle for streaming/kill.
    /// Use this for long-running processes where you need to stream output.
    async fn start(&self, spec: &MicrovmRunSpec) -> anyhow::Result<RunHandle>;

    /// Kill the running environment and return only after its sole waiter has
    /// positively observed and reaped the child.
    async fn kill(&self, handle: &RunHandle) -> anyhow::Result<ChildReapReceipt>;

    /// Check if this backend is available on the current platform.
    fn probe(&self) -> bool;

    /// Human-readable name for logging.
    fn name(&self) -> &str;

    /// Isolation level provided by this backend.
    fn isolation_level(&self) -> IsolationLevel;
}

// ---------------------------------------------------------------------------
// BackendType enum
// ---------------------------------------------------------------------------

/// Type of isolation backend.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BackendType {
    /// Cloud Hypervisor microVM.
    CloudHypervisor,
    /// WSL2 (parked placeholder).
    Wsl2,
    /// Process + git worktree.
    Process,
}

impl std::fmt::Display for BackendType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BackendType::CloudHypervisor => write!(f, "cloud-hypervisor"),
            BackendType::Wsl2 => write!(f, "wsl2"),
            BackendType::Process => write!(f, "process"),
        }
    }
}

// ---------------------------------------------------------------------------
// IsolationMode enum — canonical type re-exported from config + worker modules
// ---------------------------------------------------------------------------

/// Isolation mode from configuration.
///
/// This is the single canonical isolation-mode enum across `g5d`. The
/// `crate::config` and `crate::worker` modules re-export it so callers can
/// continue to import it from their usual paths, but there is only one type.
///
/// `Default::default()` is `MicroVm` — the production hosted-worker mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ValueEnum, Default)]
pub enum IsolationMode {
    /// MicroVM isolation via Cloud Hypervisor (hardware-virtualized, hosted default).
    #[serde(rename = "microvm")]
    #[clap(name = "microvm")]
    #[default]
    MicroVm,
    /// Process + git worktree isolation (local-dev only; no security boundary).
    #[serde(rename = "process")]
    #[clap(name = "process")]
    Process,
}

impl std::fmt::Display for IsolationMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            IsolationMode::MicroVm => write!(f, "microvm"),
            IsolationMode::Process => write!(f, "process"),
        }
    }
}

impl FromStr for IsolationMode {
    type Err = G5Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "microvm" => Ok(IsolationMode::MicroVm),
            "process" => Ok(IsolationMode::Process),
            other => Err(G5Error::Config(format!(
                "unrecognized isolation mode: '{}'. Expected 'microvm' or 'process'",
                other
            ))),
        }
    }
}

/// Pure target selection for [`raise_nofile_soft_limit`]: the soft limit
/// should rise to the hard limit, clamped to `platform_cap` where the OS
/// enforces one (macOS `kern.maxfilesperproc`). Returns `None` when no raise
/// is possible or needed, so a second computation after a successful raise is
/// always `None` (fixpoint). Factored out so the decision is unit-tested
/// without mutating process limits.
fn nofile_raise_target(soft: u64, hard: u64, platform_cap: Option<u64>) -> Option<u64> {
    let target = platform_cap.map_or(hard, |cap| hard.min(cap));
    (target > soft).then_some(target)
}

/// Ceiling macOS enforces on a process's soft `RLIMIT_NOFILE` regardless of
/// an `unlimited` hard limit; `setrlimit` above it fails with EINVAL.
#[cfg(target_os = "macos")]
fn macos_maxfilesperproc() -> Option<u64> {
    let name = std::ffi::CString::new("kern.maxfilesperproc").ok()?;
    let mut val: libc::c_int = 0;
    let mut len = std::mem::size_of::<libc::c_int>();
    // SAFETY: `name` is a valid NUL-terminated string and `val`/`len` describe
    // a properly sized output buffer for this integer sysctl.
    let rc = unsafe {
        libc::sysctlbyname(
            name.as_ptr(),
            &mut val as *mut libc::c_int as *mut libc::c_void,
            &mut len,
            std::ptr::null_mut(),
            0,
        )
    };
    (rc == 0 && val > 0).then_some(val as u64)
}

/// Raise this process's soft `RLIMIT_NOFILE` to its hard limit (macOS:
/// clamped to `kern.maxfilesperproc`), so the VMM's file-descriptor budget
/// does not depend on the launch context (ontology `microvm_runtime` [D45],
/// issue #1178). Passt and the dispatcher may hold many concurrent sockets;
/// an interactive-shell soft limit of 1024 must not become their capacity. Best-effort and non-fatal: on failure the VM
/// still boots with the inherited limit, and the failure is logged loudly.
pub fn raise_nofile_soft_limit() {
    let mut lim = libc::rlimit {
        rlim_cur: 0,
        rlim_max: 0,
    };
    // SAFETY: valid pointer to a properly initialized rlimit out-parameter.
    if unsafe { libc::getrlimit(libc::RLIMIT_NOFILE, &mut lim) } != 0 {
        tracing::warn!(
            error = %std::io::Error::last_os_error(),
            "vm-exec: cannot read RLIMIT_NOFILE; keeping the inherited soft limit (ontology D45)"
        );
        return;
    }
    #[cfg(target_os = "macos")]
    let platform_cap = macos_maxfilesperproc();
    #[cfg(not(target_os = "macos"))]
    let platform_cap = None;
    let Some(target) = nofile_raise_target(lim.rlim_cur, lim.rlim_max, platform_cap) else {
        return;
    };
    let requested = libc::rlimit {
        rlim_cur: target,
        rlim_max: lim.rlim_max,
    };
    // SAFETY: valid pointer to a fully initialized rlimit value.
    if unsafe { libc::setrlimit(libc::RLIMIT_NOFILE, &requested) } != 0 {
        tracing::warn!(
            soft = lim.rlim_cur,
            hard = lim.rlim_max,
            target,
            error = %std::io::Error::last_os_error(),
            "vm-exec: cannot raise soft RLIMIT_NOFILE — guest-loopback connection churn \
             may hit EMFILE (issue #1178, ontology D45)"
        );
    } else {
        tracing::debug!(
            from = lim.rlim_cur,
            to = target,
            "vm-exec: raised soft RLIMIT_NOFILE to the launch context's ceiling"
        );
    }
}

// ---------------------------------------------------------------------------
// IsolationConfig
// ---------------------------------------------------------------------------

/// Isolation configuration from workspace.yaml.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IsolationConfig {
    /// Isolation mode: "microvm" (default) or "process".
    #[serde(default)]
    pub mode: IsolationMode,
    /// Deprecated compatibility field. Fallback selection is no longer
    /// supported; the configured mode must be available or initialization
    /// fails closed.
    pub fallback: Option<IsolationMode>,
    /// Base OCI image for worker environments.
    #[serde(default = "default_base_image")]
    pub base_image: String,
    /// Resource limits for isolated environments.
    #[serde(default)]
    pub resources: ResourceConfig,
}

fn default_base_image() -> String {
    "g5/worker:base".to_string()
}

impl Default for IsolationConfig {
    fn default() -> Self {
        Self {
            mode: IsolationMode::default(),
            fallback: None,
            base_image: default_base_image(),
            resources: ResourceConfig::default(),
        }
    }
}

impl IsolationConfig {
    /// Create an IsolationConfig with a specific isolation mode and otherwise
    /// default values.
    #[must_use]
    pub fn with_mode(mode: IsolationMode) -> Self {
        IsolationConfig {
            mode,
            ..Default::default()
        }
    }
}

// ---------------------------------------------------------------------------
// ResourceConfig
// ---------------------------------------------------------------------------

/// Resource configuration for isolated environments.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceConfig {
    /// Number of CPUs (default: 4).
    #[serde(default = "default_cpus")]
    pub cpus: u8,
    /// Memory in GB (default: 16).
    #[serde(default = "default_memory_gb")]
    pub memory_gb: u32,
}

fn default_cpus() -> u8 {
    4
}
fn default_memory_gb() -> u32 {
    16
}

impl Default for ResourceConfig {
    fn default() -> Self {
        Self {
            cpus: default_cpus(),
            memory_gb: default_memory_gb(),
        }
    }
}

// ---------------------------------------------------------------------------
// IsolationManager
// ---------------------------------------------------------------------------

/// Manages isolation backend selection and lifecycle.
///
/// Selects the best available backend at initialization based on platform
/// detection and configuration. Provides convenience methods for running
/// commands in isolated environments.
pub struct IsolationManager {
    /// Active backend (selected at initialization).
    active_backend: Box<dyn MicrovmBackend>,
    /// Detection results (cached).
    detection: DetectionResult,
    /// Configuration.
    config: IsolationConfig,
}

impl std::fmt::Debug for IsolationManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("IsolationManager")
            .field("active_backend", &self.active_backend.name())
            .field("detection", &self.detection)
            .field("config", &self.config)
            .finish()
    }
}

impl IsolationManager {
    /// Create a new IsolationManager by detecting available backends
    /// and selecting the best one based on configuration.
    ///
    /// # Backend Selection Algorithm
    ///
    /// 1. Read `isolation.mode` from configuration:
    ///    - `"microvm"` (default): Probe for VM-capable backends in priority order
    ///    - `"process"`: Skip VM probing entirely, use `ProcessBackend`
    ///
    /// 2. If mode is `"microvm"`:
    ///    a. Probe `CloudHypervisorBackend` — verify KVM and platform artifacts are available
    ///    c. If neither is available, return an error. Silent downgrade is not allowed.
    ///
    /// 3. Log the selected backend at INFO level
    ///
    /// 4. Validate the selected backend can handle the configured resources
    ///
    /// # Errors
    ///
    /// Returns `G5Error` if:
    /// - mode is "microvm" and no VM backend is available
    /// - a deprecated fallback config/env setting is provided
    /// - git is not available (required for process backend)
    pub async fn new(config: IsolationConfig) -> Result<Self, G5Error> {
        Self::new_with_host_api_url(config, None).await
    }

    /// Supply the effective host CLI/config API URL for passt's explicit door.
    pub async fn new_with_host_api_url(
        config: IsolationConfig,
        host_api_url: Option<String>,
    ) -> Result<Self, G5Error> {
        // Apply environment variable overrides
        let mut config = config;
        if let Ok(mode_str) = std::env::var("G5D_ISOLATION_MODE") {
            match mode_str.as_str() {
                "microvm" => config.mode = IsolationMode::MicroVm,
                "process" => config.mode = IsolationMode::Process,
                _ => {
                    tracing::warn!(
                        mode = %mode_str,
                        "Unknown G5D_ISOLATION_MODE value, ignoring"
                    );
                }
            }
        }
        if let Ok(fallback_str) = std::env::var("G5D_ISOLATION_FALLBACK") {
            return Err(G5Error::Internal {
                message: format!(
                    "G5D_ISOLATION_FALLBACK is no longer supported (received {:?}). Configure G5D_ISOLATION_MODE explicitly instead.",
                    fallback_str
                ),
            });
        }
        if let Ok(image) = std::env::var("G5D_ISOLATION_BASE_IMAGE") {
            config.base_image = image;
        }
        if let Some(fallback) = config.fallback {
            return Err(G5Error::Internal {
                message: format!(
                    "Isolation fallback is no longer supported (requested fallback: {}). Configure the desired isolation mode explicitly.",
                    fallback
                ),
            });
        }

        // Detect available backends
        let detection = detect_backends().await;

        // Select backend based on configuration
        let active_backend: Box<dyn MicrovmBackend> = match config.mode {
            IsolationMode::Process => {
                // Skip VM probing entirely, use ProcessBackend
                let backend = ProcessBackend::new();
                if !backend.probe() {
                    return Err(G5Error::Internal {
                        message: "Process isolation unavailable: git not found in PATH".to_string(),
                    });
                }
                tracing::info!(
                    backend = "process",
                    isolation_level = %IsolationLevel::Process,
                    "Isolation backend selected: process (isolation_level: process)"
                );
                Box::new(backend)
            }
            IsolationMode::MicroVm => {
                // Probe for VM-capable backends in priority order
                let backend = CloudHypervisorBackend::new().with_host_api_url(host_api_url);
                if backend.probe() {
                    tracing::info!(
                        backend = "cloud-hypervisor",
                        isolation_level = %IsolationLevel::Vm,
                        "Isolation backend selected: cloud-hypervisor (isolation_level: vm)"
                    );

                    // Validate resources
                    tracing::debug!(
                        cpus = config.resources.cpus,
                        memory_gb = config.resources.memory_gb,
                        "MicroVM resource limits will be enforced by hypervisor"
                    );

                    Box::new(backend)
                } else {
                    return Err(G5Error::Internal {
                        message: "No isolation backend available: microvm requested but not supported on this platform. Install the Cloud Hypervisor runtime (Linux/KVM) or select process mode explicitly.".to_string(),
                    });
                }
            }
        };

        Ok(IsolationManager {
            active_backend,
            detection,
            config,
        })
    }

    /// Get the active backend (for running commands).
    #[must_use]
    pub fn backend(&self) -> &dyn MicrovmBackend {
        self.active_backend.as_ref()
    }

    /// Get the detection results.
    #[must_use]
    pub fn detection(&self) -> &DetectionResult {
        &self.detection
    }

    /// Get the active backend type.
    #[must_use]
    pub fn active_backend_type(&self) -> BackendType {
        // Determine from the backend's name
        match self.active_backend.name() {
            "cloud-hypervisor" => BackendType::CloudHypervisor,
            "wsl2" => BackendType::Wsl2,
            "process" => BackendType::Process,
            _ => BackendType::Process,
        }
    }

    /// Get the isolation level of the active backend.
    #[must_use]
    pub fn isolation_level(&self) -> IsolationLevel {
        self.active_backend.isolation_level()
    }

    /// Get the isolation configuration.
    #[must_use]
    pub fn config(&self) -> &IsolationConfig {
        &self.config
    }

    /// Convenience accessor for the configured base OCI image.
    #[must_use]
    pub fn base_image(&self) -> &str {
        &self.config.base_image
    }

    /// Run a command in the isolated environment.
    /// Convenience method that delegates to the active backend.
    pub async fn run(&self, spec: &MicrovmRunSpec) -> anyhow::Result<RunResult> {
        self.active_backend.run(spec).await
    }

    /// Run with a cancellation token that force-kills the isolated child
    /// (issue #1070, supervisor_lifecycle D25). Delegates to the active
    /// backend's `run_cancellable`.
    pub async fn run_cancellable(
        &self,
        spec: &MicrovmRunSpec,
        cancel: &tokio_util::sync::CancellationToken,
    ) -> anyhow::Result<RunResult> {
        self.active_backend.run_cancellable(spec, cancel).await
    }

    /// Start a long-running command and return a handle.
    pub async fn start(&self, spec: &MicrovmRunSpec) -> anyhow::Result<RunHandle> {
        self.active_backend.start(spec).await
    }

    /// Kill a running environment.
    pub async fn kill(&self, handle: &RunHandle) -> anyhow::Result<ChildReapReceipt> {
        self.active_backend.kill(handle).await
    }

    /// Stop one started environment with the caller's exact graceful window.
    /// A zero grace sends SIGKILL immediately; otherwise SIGTERM is followed
    /// by SIGKILL only after the requested bound expires.
    pub async fn terminate(
        &self,
        handle: &RunHandle,
        grace: Duration,
    ) -> anyhow::Result<ChildReapReceipt> {
        #[cfg(unix)]
        if let Some(process_group_id) = handle.child_pid() {
            return terminate_handle_group(
                handle,
                process_group_id,
                grace,
                Duration::from_secs(10),
            )
            .await;
        }
        self.active_backend.kill(handle).await
    }

    /// Build an `IsolationManager` directly from a backend + config, without
    /// probing the host. For test-only routing assertions that need an
    /// active backend of a specific `IsolationLevel` (notably to verify
    /// `RunInTerminalTool` routes to `exec_command_in_vm` under
    /// `IsolationLevel::Vm`). Available only under `cfg(test)` or with
    /// the `test-helpers` cargo feature so it never compiles into
    /// release builds — the production path must remain
    /// `IsolationManager::new(config)` so the fail-closed startup
    /// contract applies.
    #[cfg(any(test, feature = "test-helpers"))]
    pub fn with_backend_for_test(
        active_backend: Box<dyn MicrovmBackend>,
        config: IsolationConfig,
        detection: DetectionResult,
    ) -> Self {
        Self {
            active_backend,
            detection,
            config,
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;

    #[test]
    fn platform_children_receive_no_parent_environment() {
        // env prints the actual child envp; no global test-environment mutation.
        let output = platform_command("/usr/bin/env").output().unwrap();
        assert!(output.status.success());
        assert!(
            output.stdout.is_empty(),
            "platform child inherited environment"
        );
    }

    // --- GuestConsoleTail (microvm_runtime D58; test_contract/preview_native_serve D5) ---

    #[test]
    fn guest_console_tail_preserves_arrival_order_across_streams() {
        let tail = GuestConsoleTail::default();
        tail.push(GuestConsoleStream::Stdout, "starting app");
        tail.push(GuestConsoleStream::Stderr, "warning: slow disk");
        tail.push(GuestConsoleStream::Stdout, "still starting");
        let lines = tail.lines();
        assert_eq!(
            lines
                .iter()
                .map(|l| (l.stream.as_str(), l.line.as_str()))
                .collect::<Vec<_>>(),
            vec![
                ("stdout", "starting app"),
                ("stderr", "warning: slow disk"),
                ("stdout", "still starting"),
            ]
        );
        // Non-destructive read.
        assert_eq!(tail.lines().len(), 3);
        assert!(!tail.is_empty());
    }

    #[test]
    fn guest_console_tail_evicts_oldest_beyond_byte_cap() {
        let tail = GuestConsoleTail::default();
        // 100 lines x 100 bytes = 10_000 bytes > 8192 cap.
        for i in 0..100 {
            tail.push(GuestConsoleStream::Stderr, &format!("{i:0>100}"));
        }
        let lines = tail.lines();
        let bytes: usize = lines.iter().map(|l| l.line.len()).sum();
        assert!(bytes <= GUEST_CONSOLE_TAIL_BYTES, "bytes={bytes}");
        // The TAIL is kept: the newest line survives, the oldest are gone.
        assert_eq!(lines.last().unwrap().line, format!("{:0>100}", 99));
        assert_ne!(lines.first().unwrap().line, format!("{:0>100}", 0));
    }

    #[test]
    fn guest_console_tail_evicts_beyond_line_cap() {
        let tail = GuestConsoleTail::default();
        for i in 0..(GUEST_CONSOLE_TAIL_MAX_LINES + 10) {
            tail.push(GuestConsoleStream::Stdout, &format!("l{i}"));
        }
        let lines = tail.lines();
        assert_eq!(lines.len(), GUEST_CONSOLE_TAIL_MAX_LINES);
        assert_eq!(
            lines.last().unwrap().line,
            format!("l{}", GUEST_CONSOLE_TAIL_MAX_LINES + 9)
        );
    }

    #[tokio::test]
    async fn console_relay_drain_receipt_captures_final_eof_lines_on_both_streams() {
        use tokio::io::AsyncWriteExt;
        let tail = std::sync::Arc::new(GuestConsoleTail::default());
        let (mut out_writer, out_reader) = tokio::io::duplex(4096);
        let (mut err_writer, err_reader) = tokio::io::duplex(4096);
        let secrets = vec!["s3cret-value-123".to_string()];
        let out_handle = microvm::spawn_guest_console_relay(
            out_reader,
            GuestConsoleStream::Stdout,
            "vm-drain-test".to_string(),
            secrets.clone(),
            Some(std::sync::Arc::clone(&tail)),
        );
        let err_handle = microvm::spawn_guest_console_relay(
            err_reader,
            GuestConsoleStream::Stderr,
            "vm-drain-test".to_string(),
            secrets,
            Some(std::sync::Arc::clone(&tail)),
        );

        out_writer.write_all(b"stdout line\n").await.unwrap();
        err_writer
            .write_all(b"token=s3cret-value-123\nfinal stderr error without newline")
            .await
            .unwrap();
        // EOF both streams — the VM has "exited"; the final lines may still
        // be buffered inside the relay tasks at this instant.
        drop(out_writer);
        drop(err_writer);

        let handle = RunHandle::test_microvm("vm-drain-test", None, None);
        handle.test_attach_console_relay_handle(out_handle);
        handle.test_attach_console_relay_handle(err_handle);
        handle.wait_console_drained(Duration::from_secs(5)).await;

        let lines = tail.lines();
        let texts: Vec<&str> = lines.iter().map(|l| l.line.as_str()).collect();
        assert!(
            texts.contains(&"stdout line"),
            "final stdout line missing after drain: {texts:?}"
        );
        assert!(
            texts.contains(&"final stderr error without newline"),
            "final partial stderr line at EOF missing after drain: {texts:?}"
        );
        assert!(
            texts.contains(&"token=[REDACTED]"),
            "redaction must hold on drained lines: {texts:?}"
        );
        assert!(
            !texts.iter().any(|t| t.contains("s3cret-value-123")),
            "secret leaked into the retained tail: {texts:?}"
        );
        // Idempotent second wait returns immediately.
        handle.wait_console_drained(Duration::from_millis(10)).await;
    }

    #[test]
    fn guest_console_tail_truncates_single_oversized_line_on_char_boundary() {
        let tail = GuestConsoleTail::default();
        // Multi-byte char repeated past the cap: é is 2 bytes.
        let oversized = "é".repeat(GUEST_CONSOLE_TAIL_BYTES); // 2x cap bytes
        tail.push(GuestConsoleStream::Stdout, &oversized);
        let lines = tail.lines();
        assert_eq!(lines.len(), 1);
        assert!(lines[0].line.len() <= GUEST_CONSOLE_TAIL_BYTES);
        // Valid UTF-8 by construction of &str slicing; ensure content is the tail.
        assert!(lines[0].line.chars().all(|c| c == 'é'));
    }

    #[test]
    fn test_nofile_raise_target_vectors() {
        // Plain raise to the hard limit (the Linux/systemd shape).
        assert_eq!(nofile_raise_target(1024, 1_048_576, None), Some(1_048_576));
        // Already at the ceiling — no raise.
        assert_eq!(nofile_raise_target(1_048_576, 1_048_576, None), None);
        // macOS: unlimited hard clamps to kern.maxfilesperproc.
        assert_eq!(
            nofile_raise_target(256, u64::MAX, Some(10_240)),
            Some(10_240)
        );
        // Platform cap above the hard limit never lifts past hard.
        assert_eq!(nofile_raise_target(256, 1024, Some(10_240)), Some(1024));
        // Soft already at the platform cap — no raise.
        assert_eq!(nofile_raise_target(10_240, u64::MAX, Some(10_240)), None);
        // Degenerate shells where soft exceeds the computed target stay put.
        assert_eq!(nofile_raise_target(20_000, u64::MAX, Some(10_240)), None);
    }

    #[test]
    #[serial]
    fn test_raise_nofile_soft_limit_reaches_fixpoint() {
        // Serialized, and the original soft limit is restored on exit, so no
        // other test can observe a raised process-wide limit.
        let mut original = libc::rlimit {
            rlim_cur: 0,
            rlim_max: 0,
        };
        // SAFETY: valid pointer to an initialized rlimit out-parameter.
        let rc = unsafe { libc::getrlimit(libc::RLIMIT_NOFILE, &mut original) };
        assert_eq!(rc, 0, "getrlimit must succeed in the test process");

        // After the raise, the current limits must be a fixpoint: no further
        // raise is computable — which proves the setrlimit call landed on the
        // computed target ([D45]).
        raise_nofile_soft_limit();
        let mut lim = libc::rlimit {
            rlim_cur: 0,
            rlim_max: 0,
        };
        // SAFETY: valid pointer to an initialized rlimit out-parameter.
        let rc = unsafe { libc::getrlimit(libc::RLIMIT_NOFILE, &mut lim) };
        assert_eq!(rc, 0, "getrlimit must succeed in the test process");
        #[cfg(target_os = "macos")]
        let platform_cap = macos_maxfilesperproc();
        #[cfg(not(target_os = "macos"))]
        let platform_cap = None;
        let fixpoint = nofile_raise_target(lim.rlim_cur, lim.rlim_max, platform_cap);

        // Restore before asserting so a failure cannot leak the raised limit.
        // SAFETY: valid pointer to the previously read rlimit value; lowering
        // the soft limit back is always permitted.
        let rc = unsafe { libc::setrlimit(libc::RLIMIT_NOFILE, &original) };
        assert_eq!(rc, 0, "restoring the original RLIMIT_NOFILE must succeed");

        assert_eq!(
            fixpoint, None,
            "post-raise limits must be a fixpoint (soft={} hard={} cap={:?})",
            lim.rlim_cur, lim.rlim_max, platform_cap
        );
    }

    #[test]
    fn test_isolation_level_ordering() {
        assert!(IsolationLevel::Process < IsolationLevel::Container);
        assert!(IsolationLevel::Container < IsolationLevel::Vm);
        assert!(IsolationLevel::Process < IsolationLevel::Vm);
    }

    #[test]
    fn test_isolation_level_display() {
        assert_eq!(IsolationLevel::Process.to_string(), "process");
        assert_eq!(IsolationLevel::Container.to_string(), "container");
        assert_eq!(IsolationLevel::Vm.to_string(), "vm");
    }

    #[test]
    fn test_backend_type_display() {
        assert_eq!(BackendType::CloudHypervisor.to_string(), "cloud-hypervisor");
        assert_eq!(BackendType::Wsl2.to_string(), "wsl2");
        assert_eq!(BackendType::Process.to_string(), "process");
    }

    #[test]
    fn test_isolation_mode_default() {
        let mode = IsolationMode::default();
        assert_eq!(mode, IsolationMode::MicroVm);
    }

    #[test]
    fn test_isolation_mode_display() {
        assert_eq!(IsolationMode::MicroVm.to_string(), "microvm");
        assert_eq!(IsolationMode::Process.to_string(), "process");
    }

    #[test]
    fn test_isolation_mode_from_str_case_insensitive() {
        // Use the std::str::FromStr impl explicitly — IsolationMode also
        // derives clap::ValueEnum, which exposes a separate from_str on
        // the ValueEnum trait, so unqualified IsolationMode::from_str
        // would be ambiguous.
        use std::str::FromStr;
        // Canonical lowercase.
        assert_eq!(
            <IsolationMode as FromStr>::from_str("microvm").unwrap(),
            IsolationMode::MicroVm
        );
        assert_eq!(
            <IsolationMode as FromStr>::from_str("process").unwrap(),
            IsolationMode::Process
        );
        // Case variants the CLI flag + env var must accept (opus46 review
        // NB #6 — the case-insensitive FromStr impl that powers
        // `--isolation-mode` had no coverage before this).
        assert_eq!(
            <IsolationMode as FromStr>::from_str("MicroVm").unwrap(),
            IsolationMode::MicroVm
        );
        assert_eq!(
            <IsolationMode as FromStr>::from_str("MICROVM").unwrap(),
            IsolationMode::MicroVm
        );
        assert_eq!(
            <IsolationMode as FromStr>::from_str("Process").unwrap(),
            IsolationMode::Process
        );
        assert_eq!(
            <IsolationMode as FromStr>::from_str("PROCESS").unwrap(),
            IsolationMode::Process
        );
        // Unrecognized values surface a descriptive Config error so the
        // operator can fix the mistake.
        let err = <IsolationMode as FromStr>::from_str("invalid").unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains("unrecognized isolation mode"),
            "expected descriptive error, got: {msg}"
        );
        assert!(
            msg.contains("microvm") && msg.contains("process"),
            "expected error to enumerate the accepted values, got: {msg}"
        );
        // The empty string is not a valid mode either.
        assert!(<IsolationMode as FromStr>::from_str("").is_err());
    }

    #[test]
    fn test_isolation_mode_serde() {
        let json = serde_json::to_string(&IsolationMode::MicroVm).unwrap();
        assert_eq!(json, "\"microvm\"");

        let json = serde_json::to_string(&IsolationMode::Process).unwrap();
        assert_eq!(json, "\"process\"");

        let mode: IsolationMode = serde_json::from_str("\"microvm\"").unwrap();
        assert_eq!(mode, IsolationMode::MicroVm);

        let mode: IsolationMode = serde_json::from_str("\"process\"").unwrap();
        assert_eq!(mode, IsolationMode::Process);
    }

    #[test]
    fn test_backend_type_serde() {
        let json = serde_json::to_string(&BackendType::CloudHypervisor).unwrap();
        let back: BackendType = serde_json::from_str(&json).unwrap();
        assert_eq!(back, BackendType::CloudHypervisor);

        let json = serde_json::to_string(&BackendType::Process).unwrap();
        let back: BackendType = serde_json::from_str(&json).unwrap();
        assert_eq!(back, BackendType::Process);
    }

    #[test]
    fn test_resource_config_defaults() {
        let config = ResourceConfig::default();
        assert_eq!(config.cpus, 4);
        assert_eq!(config.memory_gb, 16);
    }

    #[test]
    fn test_isolation_config_defaults() {
        let config = IsolationConfig::default();
        assert_eq!(config.mode, IsolationMode::MicroVm);
        assert_eq!(config.fallback, None);
        assert_eq!(config.base_image, "g5/worker:base");
        assert_eq!(config.resources.cpus, 4);
        assert_eq!(config.resources.memory_gb, 16);
    }

    #[test]
    fn test_isolation_config_serde() {
        let json = r#"{
            "mode": "process",
            "fallback": "microvm",
            "base_image": "custom:latest",
            "resources": {
                "cpus": 8,
                "memory_gb": 32
            }
        }"#;
        let config: IsolationConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.mode, IsolationMode::Process);
        // Parsed for backward compatibility, but rejected at runtime.
        assert_eq!(config.fallback, Some(IsolationMode::MicroVm));
        assert_eq!(config.base_image, "custom:latest");
        assert_eq!(config.resources.cpus, 8);
        assert_eq!(config.resources.memory_gb, 32);
    }

    #[test]
    fn test_microvm_run_spec_debug() {
        let spec = MicrovmRunSpec {
            image: "test:latest".to_string(),
            cmd: vec!["echo".to_string(), "hello".to_string()],
            env: vec![("KEY".to_string(), "VALUE".to_string())],
            mount_host_dir: PathBuf::from("/tmp/workspace"),
            mount_guest_dir: "/work".to_string(),
            cpus: 4,
            mem_mib: 4096,
            extra_mounts: vec![],
            port_maps: vec![],

            scratch_disk: None,
            block_workload: false,
            nested_virtualization: false,
            guest_restore: None,
        };
        let debug = format!("{:?}", spec);
        assert!(debug.contains("test:latest"));
    }

    #[test]
    fn test_run_result_debug() {
        let result = RunResult {
            exit_code: 0,
            stdout: "hello".to_string(),
            stderr: String::new(),
            duration: Duration::from_secs(1),
        };
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout, "hello");
    }

    #[tokio::test]
    #[serial]
    async fn test_isolation_manager_process_mode() {
        // Process mode should work if git is available
        if find_binary("git").is_none() {
            // Skip test if git is not available
            return;
        }

        let config = IsolationConfig {
            mode: IsolationMode::Process,
            fallback: None,
            base_image: "test:latest".to_string(),
            resources: ResourceConfig::default(),
        };

        let manager = IsolationManager::new(config).await.unwrap();
        assert_eq!(manager.active_backend_type(), BackendType::Process);
        assert_eq!(manager.isolation_level(), IsolationLevel::Process);
    }

    #[tokio::test]
    #[serial]
    async fn test_isolation_manager_microvm_default_errors_when_unavailable() {
        // Default config has no fallback — microvm request without a VM backend
        // should hard-error, not silently degrade.
        //
        // Skip when the configured Linux/KVM runtime is available.
        if CloudHypervisorBackend::new().probe() {
            return;
        }

        let config = IsolationConfig {
            mode: IsolationMode::MicroVm,
            // No fallback (the default)
            fallback: None,
            base_image: "test:latest".to_string(),
            resources: ResourceConfig::default(),
        };

        let result = IsolationManager::new(config).await;
        assert!(
            result.is_err(),
            "Expected hard error when microVM unavailable"
        );
        let err_msg = format!("{}", result.unwrap_err());
        assert!(
            err_msg.contains("select process mode explicitly"),
            "Error should explain explicit process selection: {}",
            err_msg
        );
    }

    #[tokio::test]
    #[serial]
    async fn test_isolation_manager_rejects_explicit_fallback_config() {
        let config = IsolationConfig {
            mode: IsolationMode::MicroVm,
            fallback: Some(IsolationMode::Process),
            base_image: "test:latest".to_string(),
            resources: ResourceConfig::default(),
        };

        let result = IsolationManager::new(config).await;
        assert!(result.is_err());
        assert!(
            format!("{}", result.unwrap_err()).contains("no longer supported"),
            "fallback config should be rejected explicitly"
        );
    }

    #[tokio::test]
    #[serial]
    async fn test_isolation_manager_run_delegates_to_backend() {
        // IsolationManager.run() should delegate to the active backend
        if find_binary("git").is_none() {
            return;
        }

        let config = IsolationConfig {
            mode: IsolationMode::Process,
            fallback: None,
            base_image: "test:latest".to_string(),
            resources: ResourceConfig::default(),
        };

        let manager = IsolationManager::new(config).await.unwrap();
        let spec = MicrovmRunSpec {
            image: String::new(),
            cmd: vec!["echo".to_string(), "delegated".to_string()],
            env: vec![],
            mount_host_dir: std::path::PathBuf::from("/tmp"),
            mount_guest_dir: "/work".to_string(),
            cpus: 0,
            mem_mib: 0,
            extra_mounts: vec![],
            port_maps: vec![],

            scratch_disk: None,
            block_workload: false,
            nested_virtualization: false,
            guest_restore: None,
        };

        let result = manager.run(&spec).await.unwrap();
        assert_eq!(result.exit_code, 0);
        assert!(result.stdout.contains("delegated"));
    }

    #[tokio::test]
    #[serial]
    async fn test_isolation_manager_start_and_kill() {
        // IsolationManager.start() + kill() flow
        if find_binary("git").is_none() {
            return;
        }

        let config = IsolationConfig {
            mode: IsolationMode::Process,
            fallback: None,
            base_image: "test:latest".to_string(),
            resources: ResourceConfig::default(),
        };

        let manager = IsolationManager::new(config).await.unwrap();
        let spec = MicrovmRunSpec {
            image: String::new(),
            cmd: vec!["sleep".to_string(), "60".to_string()],
            env: vec![],
            mount_host_dir: std::path::PathBuf::from("/tmp"),
            mount_guest_dir: "/work".to_string(),
            cpus: 0,
            mem_mib: 0,
            extra_mounts: vec![],
            port_maps: vec![],

            scratch_disk: None,
            block_workload: false,
            nested_virtualization: false,
            guest_restore: None,
        };

        let handle = manager.start(&spec).await.unwrap();
        assert!(!handle.id.is_empty());
        assert_eq!(handle.backend_type, BackendType::Process);

        // Kill should succeed without error
        let kill_result = manager.kill(&handle).await;
        assert!(kill_result.is_ok());
    }

    #[tokio::test]
    #[serial]
    async fn test_isolation_manager_env_override_mode() {
        // G5D_ISOLATION_MODE env var should override config.mode
        if find_binary("git").is_none() {
            return;
        }

        // Set env var to force process mode even though config says microvm
        std::env::set_var("G5D_ISOLATION_MODE", "process");

        let config = IsolationConfig {
            mode: IsolationMode::MicroVm,
            fallback: None,
            base_image: "test:latest".to_string(),
            resources: ResourceConfig::default(),
        };

        let manager = IsolationManager::new(config).await.unwrap();
        assert_eq!(manager.active_backend_type(), BackendType::Process);

        std::env::remove_var("G5D_ISOLATION_MODE");
    }

    #[tokio::test]
    #[serial]
    async fn test_isolation_manager_env_override_base_image() {
        // G5D_ISOLATION_BASE_IMAGE env var should override config.base_image
        if find_binary("git").is_none() {
            return;
        }

        std::env::set_var("G5D_ISOLATION_BASE_IMAGE", "custom/image:v2");

        let config = IsolationConfig {
            mode: IsolationMode::Process,
            fallback: None,
            base_image: "default:latest".to_string(),
            resources: ResourceConfig::default(),
        };

        let manager = IsolationManager::new(config).await.unwrap();
        assert_eq!(manager.config().base_image, "custom/image:v2");

        std::env::remove_var("G5D_ISOLATION_BASE_IMAGE");
    }

    #[test]
    fn test_isolation_config_partial_serde() {
        // Missing fields should use defaults
        let json = r#"{"mode": "process"}"#;
        let config: IsolationConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.mode, IsolationMode::Process);
        assert_eq!(config.base_image, "g5/worker:base");
        assert_eq!(config.resources.cpus, 4);
        assert_eq!(config.resources.memory_gb, 16);
    }

    #[test]
    fn test_run_handle_inner_variants() {
        // Verify RunHandleInner covers all backend types
        let microvm = RunHandleInner::Microvm {
            vm_name: "test-vm".to_string(),
            child_pid: Some(12345),
        };
        let debug = format!("{:?}", microvm);
        assert!(debug.contains("test-vm"));

        let process = RunHandleInner::Process {
            child_pid: 42,
            pgid: Some(42),
            session_leader: false,
            tag: None,
            started: None,
        };
        let debug = format!("{:?}", process);
        assert!(debug.contains("42"));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn termination_error_identifies_its_handle_and_process_group() {
        let handle = RunHandle::test_process("observed-process", None, 42);
        let error = terminate_handle_group(&handle, 0, Duration::ZERO, Duration::from_millis(1))
            .await
            .unwrap_err()
            .to_string();
        assert!(error.contains("observed-process"));
        assert!(error.contains("child_pid=Some(42)"));
        assert!(error.contains("process_group_id=0"));
    }

    #[test]
    fn microvm_mount_serializes_read_write_by_default_and_read_only_explicitly() {
        let rw = MicrovmMount::read_write("/host/ws", "/work");
        assert_eq!(rw.access, MountAccess::ReadWrite);
        assert_eq!(rw.vm_exec_volume_arg(), "/host/ws:/work");

        let ro = MicrovmMount::read_only("/host/docs", "/work/docs");
        assert_eq!(ro.access, MountAccess::ReadOnly);
        assert_eq!(ro.vm_exec_volume_arg(), "/host/docs:/work/docs:ro");
    }

    #[tokio::test]
    #[serial]
    async fn test_isolation_manager_rejects_fallback_env_override() {
        std::env::set_var("G5D_ISOLATION_FALLBACK", "process");
        let config = IsolationConfig {
            mode: IsolationMode::Process,
            fallback: None,
            base_image: "test:latest".to_string(),
            resources: ResourceConfig::default(),
        };
        let result = IsolationManager::new(config).await;
        std::env::remove_var("G5D_ISOLATION_FALLBACK");

        assert!(result.is_err());
        assert!(
            format!("{}", result.unwrap_err()).contains("G5D_ISOLATION_FALLBACK"),
            "fallback env override should be rejected explicitly"
        );
    }

    #[test]
    fn test_isolation_manager_accessors() {
        // Test that detection() and config() accessors compile and return references
        // (tested implicitly via other tests, but let's verify the types)
        let config = IsolationConfig::default();
        assert_eq!(config.mode, IsolationMode::MicroVm);
        assert!(
            config.fallback.is_none(),
            "Default should have no fallback (hard error)"
        );
    }
}

/// Authority supplied by the host's validated VM specification, not image data.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct WorkloadRepoScope {
    pub org_id: uuid::Uuid,
    pub repos: Vec<WorkloadRepoMember>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct WorkloadRepoMember {
    pub repo_id: uuid::Uuid,
    pub root_path: String,
    pub git_url: String,
}

#[cfg(feature = "cloud-hypervisor")]
mod vm_scope;
